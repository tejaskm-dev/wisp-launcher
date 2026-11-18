//! The sync folder: what's in it and how it's written.
//!
//! ```text
//! Wisp/
//!   wisp-sync.json              what this folder is (written once)
//!   devices/<id>.json           one file per computer, only ever written by that computer
//!   objects/ab/cdef...          every file once, gzip-compressed, under the SHA-1 of its contents
//!   items/<key>/<commit>.json   each world's and instance's history: one small file per sync
//! ```
//!
//! Nothing in it is ever edited in place. A sync adds the objects it needs first and the commit
//! naming them last. Cloud apps (iCloud Drive, OneDrive, Dropbox, Syncthing...) deliver those
//! files in any order and whenever they like, so a computer that sees a commit before all its
//! objects have arrived waits for them; and two computers syncing the same thing at once leave
//! two commits (a fork Wisp can see), never a half-written file or a "conflicted copy".

use std::collections::{HashMap, HashSet};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

/// The folder Wisp makes inside the storage you pick.
pub const FOLDER_NAME: &str = "Wisp";
const MARKER: &str = "wisp-sync.json";
const FORMAT: u32 = 1;
const OBJECTS: &str = "objects";
const ITEMS: &str = "items";
const DEVICES: &str = "devices";

#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// Not in the folder (yet): a cloud app may still be bringing it.
    #[error("not there yet")]
    Missing,
    #[error("damaged: {0}")]
    Damaged(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Marker {
    format: u32,
    created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemKind {
    World,
    Instance,
}

/// One sync of one world or instance: who made it, when, on top of which earlier ones, and the
/// object listing its files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    /// The SHA-1 of the rest of the commit (it names the file).
    #[serde(default)]
    pub id: String,
    pub item: String,
    pub kind: ItemKind,
    pub parents: Vec<String>,
    pub device: String,
    pub device_name: String,
    pub at: i64,
    /// The instance's sync id (for a world, the instance it belongs to).
    pub instance: String,
    /// A world's folder name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world: Option<String>,
    /// The world's or instance's name at the time.
    pub name: String,
    /// The object holding the full manifest (empty for a removal).
    pub manifest: String,
    /// What the manifest says, ignoring where and when it was made: commits with the same
    /// contents have the same one.
    pub content: String,
    pub files: u32,
    pub bytes: u64,
    /// Taken out of the cloud on purpose. Computers keep their own copies.
    #[serde(default)]
    pub removed: bool,
}

/// A computer that syncs to this folder, as it last described itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRecord {
    pub id: String,
    pub name: String,
    pub os: String,
    /// When it last checked in (Unix ms). While it plays, it checks in every minute.
    pub seen: i64,
    /// Instances it's playing right now.
    #[serde(default)]
    pub playing: Vec<Playing>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playing {
    /// The instance's item key.
    pub item: String,
    pub name: String,
    pub since: i64,
}

/// A quick look at a sync folder, for choosing one.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSummary {
    pub devices: Vec<String>,
    pub instances: u32,
    pub worlds: u32,
    /// The latest a computer checked in.
    pub last_seen: Option<i64>,
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn sha1_hex(bytes: &[u8]) -> String {
    hex(&Sha1::digest(bytes))
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 40 && hash.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

/// Item keys are made by Wisp from hex and dashes; anything else in the folder isn't one.
fn valid_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 64 && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

pub struct Remote {
    root: PathBuf,
    /// This computer's id, which keeps its half-written files apart from everyone else's.
    device: String,
}

impl Remote {
    /// `root` is the Wisp folder itself.
    pub fn new(root: impl Into<PathBuf>, device: &str) -> Self {
        Self { root: root.into(), device: device.to_string() }
    }

    /// Whether the folder is there to use (a drive can be unplugged, a cloud app signed out).
    pub fn reachable(&self) -> bool {
        self.root.is_dir()
    }

    pub fn is_set_up(&self) -> bool {
        self.root.join(MARKER).is_file()
    }

    /// Makes the folder ready, leaving anything already in it alone.
    pub fn set_up(&self, now: i64) -> std::io::Result<()> {
        for dir in [OBJECTS, ITEMS, DEVICES] {
            std::fs::create_dir_all(self.root.join(dir))?;
        }
        if !self.is_set_up() {
            let marker = serde_json::to_vec_pretty(&Marker { format: FORMAT, created_at: now }).map_err(std::io::Error::other)?;
            self.write_new(&self.root.join(MARKER), &marker)?;
        }
        Ok(())
    }

    /// Writes a file that must appear whole or not at all: written beside it under a name only
    /// this computer uses, then moved into place.
    fn write_new(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        let parent = path.parent().expect("files in the sync folder have a folder");
        std::fs::create_dir_all(parent)?;
        let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let temp = parent.join(format!(".{name}.{}.part", self.device));
        std::fs::write(&temp, bytes)?;
        std::fs::rename(&temp, path)
    }

    // ── Objects ──

    pub fn object_path(&self, hash: &str) -> PathBuf {
        self.root.join(OBJECTS).join(&hash[..2]).join(&hash[2..])
    }

    pub fn has(&self, hash: &str) -> bool {
        valid_hash(hash) && self.object_path(hash).is_file()
    }

    /// Copies contents that are already compressed (from the restore point store) in, unless
    /// they're there. Returns the bytes written.
    pub fn put_compressed(&self, hash: &str, compressed: &Path) -> std::io::Result<u64> {
        if self.has(hash) {
            return Ok(0);
        }
        let target = self.object_path(hash);
        std::fs::create_dir_all(target.parent().expect("objects have a folder"))?;
        let temp = target.with_file_name(format!(".{}.{}.part", &hash[2..], self.device));
        let written = std::fs::copy(compressed, &temp)?;
        std::fs::rename(&temp, &target)?;
        Ok(written)
    }

    /// Compresses a file in under its hash, unless it's there. Returns the bytes written.
    pub fn put_file(&self, hash: &str, source: &Path) -> std::io::Result<u64> {
        if self.has(hash) {
            return Ok(0);
        }
        let target = self.object_path(hash);
        std::fs::create_dir_all(target.parent().expect("objects have a folder"))?;
        let temp = target.with_file_name(format!(".{}.{}.part", &hash[2..], self.device));
        {
            let mut encoder = flate2::write::GzEncoder::new(BufWriter::new(std::fs::File::create(&temp)?), flate2::Compression::fast());
            std::io::copy(&mut BufReader::new(std::fs::File::open(source)?), &mut encoder)?;
            encoder.finish()?.flush()?;
        }
        let written = std::fs::metadata(&temp)?.len();
        std::fs::rename(&temp, &target)?;
        Ok(written)
    }

    /// Stores a value as JSON, compressed, under the hash of the JSON. Returns the hash.
    pub fn put_json<T: Serialize>(&self, value: &T) -> std::io::Result<String> {
        let bytes = serde_json::to_vec(value).map_err(std::io::Error::other)?;
        let hash = sha1_hex(&bytes);
        if !self.has(&hash) {
            let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(&bytes)?;
            self.write_new(&self.object_path(&hash), &encoder.finish()?)?;
        }
        Ok(hash)
    }

    /// Reads a JSON object back, checking it's the one its hash names.
    pub fn get_json<T: DeserializeOwned>(&self, hash: &str) -> Result<T, RemoteError> {
        if !valid_hash(hash) {
            return Err(RemoteError::Damaged(format!("{hash} isn't an object name")));
        }
        let file = match std::fs::File::open(self.object_path(hash)) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Err(RemoteError::Missing),
            Err(error) => return Err(error.into()),
        };
        let mut bytes = Vec::new();
        // A copy a cloud app is still writing reads short, or fails to unpack: not there yet.
        if flate2::read::GzDecoder::new(BufReader::new(file)).read_to_end(&mut bytes).is_err() || sha1_hex(&bytes) != hash {
            return Err(RemoteError::Missing);
        }
        serde_json::from_slice(&bytes).map_err(|error| RemoteError::Damaged(error.to_string()))
    }

    /// Unpacks an object into a file, checking its contents, written whole or not at all. False
    /// when the object isn't (all) there yet.
    pub fn extract_to(&self, hash: &str, target: &Path) -> std::io::Result<bool> {
        if !self.has(hash) {
            return Ok(false);
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp = target.with_extension("wisp-sync");
        let intact = {
            let mut decoder = flate2::read::GzDecoder::new(BufReader::new(std::fs::File::open(self.object_path(hash))?));
            let mut writer = BufWriter::new(std::fs::File::create(&temp)?);
            let mut hasher = Sha1::new();
            let mut buffer = vec![0u8; 1 << 16];
            let mut intact = true;
            loop {
                match decoder.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => {
                        hasher.update(&buffer[..read]);
                        writer.write_all(&buffer[..read])?;
                    }
                    Err(_) => {
                        intact = false;
                        break;
                    }
                }
            }
            writer.flush()?;
            intact && hex(&hasher.finalize()) == hash
        };
        if !intact {
            let _ = std::fs::remove_file(&temp);
            return Ok(false);
        }
        std::fs::rename(&temp, target)?;
        Ok(true)
    }

    // ── Items and their commits ──

    fn item_dir(&self, key: &str) -> PathBuf {
        self.root.join(ITEMS).join(key)
    }

    /// Every world and instance anyone has synced here.
    pub fn items(&self) -> Vec<String> {
        std::fs::read_dir(self.root.join(ITEMS))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|key| valid_key(key))
            .collect()
    }

    /// An item's commits, oldest first. Files that aren't whole commits (still arriving, damaged,
    /// or a cloud app's copy of one) are skipped.
    pub fn commits(&self, key: &str) -> Vec<Commit> {
        let mut commits: Vec<Commit> = std::fs::read_dir(self.item_dir(key))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let id = name.strip_suffix(".json")?;
                if !valid_hash(id) {
                    return None;
                }
                let commit: Commit = serde_json::from_slice(&std::fs::read(entry.path()).ok()?).ok()?;
                (commit.id == id && commit_id(&commit) == id && commit.item == key).then_some(commit)
            })
            .collect();
        commits.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.id.cmp(&b.id)));
        commits
    }

    /// Adds a commit (its id is worked out here). Returns it with the id.
    pub fn write_commit(&self, mut commit: Commit) -> std::io::Result<Commit> {
        commit.id = commit_id(&commit);
        let path = self.item_dir(&commit.item).join(format!("{}.json", commit.id));
        if !path.is_file() {
            self.write_new(&path, &serde_json::to_vec_pretty(&commit).map_err(std::io::Error::other)?)?;
        }
        Ok(commit)
    }

    // ── Computers ──

    pub fn devices(&self) -> Vec<DeviceRecord> {
        std::fs::read_dir(self.root.join(DEVICES))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.path().extension().is_some_and(|extension| extension == "json"))
            .filter_map(|entry| serde_json::from_slice::<DeviceRecord>(&std::fs::read(entry.path()).ok()?).ok())
            .filter(|record| valid_key(&record.id))
            .collect()
    }

    pub fn write_device(&self, record: &DeviceRecord) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(record).map_err(std::io::Error::other)?;
        self.write_new(&self.root.join(DEVICES).join(format!("{}.json", record.id)), &bytes)
    }

    // ── Looking after it ──

    pub fn summary(&self) -> RemoteSummary {
        let devices = self.devices();
        let items = self.items();
        RemoteSummary {
            devices: devices.iter().map(|device| device.name.clone()).collect(),
            instances: items.iter().filter(|key| key.starts_with("i-")).count() as u32,
            worlds: items.iter().filter(|key| key.starts_with("w-")).count() as u32,
            last_seen: devices.iter().map(|device| device.seen).max(),
        }
    }

    /// The space the folder takes.
    pub fn usage(&self) -> u64 {
        crate::storage::dir_size(&self.root)
    }

    /// Forgets old history: per item, commits older than `before` beyond the newest `keep`
    /// (never the newest ones on any branch), then objects nothing left refers to. Objects
    /// younger than `before` stay whatever: another computer may be halfway through a sync that
    /// needs them. Returns the bytes freed.
    pub fn tidy(&self, before: i64, keep: usize, manifest_files: impl Fn(&str, ItemKind) -> Option<Vec<String>>) -> std::io::Result<u64> {
        let mut freed = 0;
        let mut referenced: HashSet<String> = HashSet::new();
        for key in self.items() {
            let commits = self.commits(&key);
            let heads: HashSet<&str> = {
                let parents: HashSet<&str> = commits.iter().flat_map(|commit| commit.parents.iter().map(String::as_str)).collect();
                commits.iter().map(|commit| commit.id.as_str()).filter(|id| !parents.contains(id)).collect()
            };
            let mut newest_first: Vec<&Commit> = commits.iter().collect();
            newest_first.reverse();
            for (index, commit) in newest_first.iter().enumerate() {
                let old = index >= keep && commit.at < before && !heads.contains(commit.id.as_str());
                if old {
                    let path = self.item_dir(&key).join(format!("{}.json", commit.id));
                    freed += std::fs::metadata(&path).map_or(0, |meta| meta.len());
                    std::fs::remove_file(path)?;
                    continue;
                }
                if commit.removed {
                    continue;
                }
                referenced.insert(commit.manifest.clone());
                match manifest_files(&commit.manifest, commit.kind) {
                    Some(hashes) => referenced.extend(hashes),
                    // A manifest that can't be read yet (still arriving): keep everything.
                    None => return Ok(freed),
                }
            }
        }
        let cutoff = std::time::UNIX_EPOCH + std::time::Duration::from_millis(before.max(0) as u64);
        for bucket in std::fs::read_dir(self.root.join(OBJECTS)).into_iter().flatten().flatten() {
            let prefix = bucket.file_name().to_string_lossy().into_owned();
            if prefix.len() != 2 {
                continue;
            }
            for object in std::fs::read_dir(bucket.path()).into_iter().flatten().flatten() {
                let name = object.file_name().to_string_lossy().into_owned();
                let hash = format!("{prefix}{name}");
                if !valid_hash(&hash) || referenced.contains(&hash) {
                    continue;
                }
                let Ok(meta) = object.metadata() else { continue };
                if meta.modified().is_ok_and(|modified| modified < cutoff) {
                    freed += meta.len();
                    std::fs::remove_file(object.path())?;
                }
            }
        }
        Ok(freed)
    }
}

/// A commit's id: the SHA-1 of everything else in it.
pub fn commit_id(commit: &Commit) -> String {
    let mut bare = commit.clone();
    bare.id = String::new();
    sha1_hex(&serde_json::to_vec(&bare).unwrap_or_default())
}

/// The newest commits on each branch of an item's history: the ones nothing came after.
pub fn heads(commits: &[Commit]) -> Vec<&Commit> {
    let parents: HashSet<&str> = commits.iter().flat_map(|commit| commit.parents.iter().map(String::as_str)).collect();
    let mut heads: Vec<&Commit> = commits.iter().filter(|commit| !parents.contains(commit.id.as_str())).collect();
    heads.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.id.cmp(&b.id)));
    heads
}

/// Whether `ancestor` is `of` itself or one of the commits it came from.
pub fn descends(commits: &[Commit], of: &str, ancestor: &str) -> bool {
    let by_id: HashMap<&str, &Commit> = commits.iter().map(|commit| (commit.id.as_str(), commit)).collect();
    let mut pending = vec![of];
    let mut seen = HashSet::new();
    while let Some(id) = pending.pop() {
        if id == ancestor {
            return true;
        }
        if !seen.insert(id) {
            continue;
        }
        if let Some(commit) = by_id.get(id) {
            pending.extend(commit.parents.iter().map(String::as_str));
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wisp-remote-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn commit(item: &str, parents: &[&str], at: i64, content: &str) -> Commit {
        Commit {
            id: String::new(),
            item: item.into(),
            kind: ItemKind::World,
            parents: parents.iter().map(|parent| parent.to_string()).collect(),
            device: "a1".into(),
            device_name: "Laptop".into(),
            at,
            instance: "abc".into(),
            world: Some("New World".into()),
            name: "New World".into(),
            manifest: String::new(),
            content: content.into(),
            files: 0,
            bytes: 0,
            removed: false,
        }
    }

    #[test]
    fn objects_go_in_once_and_come_out_checked() {
        let root = temp("objects");
        let remote = Remote::new(&root, "a1");
        remote.set_up(1).unwrap();
        assert!(remote.is_set_up());

        let source = root.join("source.txt");
        std::fs::write(&source, b"hello region").unwrap();
        let hash = sha1_hex(b"hello region");
        assert!(remote.put_file(&hash, &source).unwrap() > 0);
        assert_eq!(remote.put_file(&hash, &source).unwrap(), 0);
        let out = root.join("out/region.mca");
        assert!(remote.extract_to(&hash, &out).unwrap());
        assert_eq!(std::fs::read(&out).unwrap(), b"hello region");

        // A copy that's still arriving (cut short) doesn't count as there.
        let object = remote.object_path(&hash);
        let bytes = std::fs::read(&object).unwrap();
        std::fs::write(&object, &bytes[..bytes.len() / 2]).unwrap();
        assert!(!remote.extract_to(&hash, &root.join("out/again.mca")).unwrap());
        assert!(!root.join("out/again.mca").exists());

        let manifest = remote.put_json(&vec!["a", "b"]).unwrap();
        assert_eq!(remote.get_json::<Vec<String>>(&manifest).unwrap(), vec!["a", "b"]);
        assert!(matches!(remote.get_json::<Vec<String>>(&"0".repeat(40)), Err(RemoteError::Missing)));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn commits_are_named_by_their_contents_and_impostors_are_ignored() {
        let root = temp("commits");
        let remote = Remote::new(&root, "a1");
        remote.set_up(1).unwrap();
        let first = remote.write_commit(commit("w-abc-1", &[], 10, "x")).unwrap();
        let second = remote.write_commit(commit("w-abc-1", &[&first.id], 20, "y")).unwrap();
        assert_eq!(remote.commits("w-abc-1").iter().map(|c| c.id.clone()).collect::<Vec<_>>(), vec![first.id.clone(), second.id.clone()]);

        // A cloud app's "conflicted copy", a half-written file and a tampered one are all skipped.
        let dir = root.join("items/w-abc-1");
        std::fs::copy(dir.join(format!("{}.json", first.id)), dir.join(format!("{} (1).json", first.id))).unwrap();
        std::fs::write(dir.join(format!("{}.json", "1".repeat(40))), b"{\"id\":").unwrap();
        let mut forged = second.clone();
        forged.content = "z".into();
        std::fs::write(dir.join(format!("{}.json", "2".repeat(40))), serde_json::to_vec(&Commit { id: "2".repeat(40), ..forged }).unwrap()).unwrap();
        assert_eq!(remote.commits("w-abc-1").len(), 2);
        assert_eq!(remote.items(), vec!["w-abc-1".to_string()]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn heads_and_ancestry() {
        let mut a = commit("k", &[], 1, "a");
        a.id = commit_id(&a);
        let mut b = commit("k", &[&a.id], 2, "b");
        b.id = commit_id(&b);
        let mut c = commit("k", &[&a.id], 3, "c");
        c.id = commit_id(&c);
        let all = vec![a.clone(), b.clone(), c.clone()];
        let found: Vec<&str> = heads(&all).iter().map(|commit| commit.id.as_str()).collect();
        assert_eq!(found, vec![c.id.as_str(), b.id.as_str()]);
        assert!(descends(&all, &b.id, &a.id));
        assert!(!descends(&all, &b.id, &c.id));
        assert!(descends(&all, &c.id, &c.id));
    }
}
