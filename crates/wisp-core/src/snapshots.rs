//! Restore points: copies of a world you can go back to, stored the way git stores commits.
//!
//! Every file is kept once, compressed, under the SHA-1 of its contents ("objects"), and a
//! restore point is a manifest listing which object each file in the world had. A file that
//! didn't change between two restore points costs nothing the second time, so the restore
//! point Wisp takes after each play session only stores the region files that session touched.
//! Like git's index, a file whose size and time match the last restore point isn't read again.

use std::collections::{HashMap, HashSet};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::worlds::SESSION_LOCK;

const OBJECTS: &str = "objects";
const SNAPSHOTS: &str = "snapshots";
const INDEX: &str = "index.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnapshotReason {
    /// Taken after a play session.
    Auto,
    /// Made from the journal or the world's page.
    Manual,
    /// The world as it was just before a restore, so the restore can be undone.
    BeforeRestore,
    /// Kept when the world synced: sent from this computer, or brought from another one.
    Sync,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    /// Relative to the world folder, with forward slashes.
    pub path: String,
    pub hash: String,
    pub size: u64,
    /// Unix milliseconds.
    pub modified: i64,
}

/// A restore point without its file list: what lists and the journal show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// The SHA-1 of the restore point, like a commit hash.
    pub id: String,
    pub instance_id: String,
    /// The world's folder.
    pub world: String,
    /// The world's name when it was taken.
    pub name: String,
    pub created_at: i64,
    pub reason: SnapshotReason,
    #[serde(default)]
    pub label: Option<String>,
    pub files: u32,
    pub total_bytes: u64,
    /// What this restore point added to the store (files no earlier one had).
    pub added_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    #[serde(flatten)]
    summary: Summary,
    entries: Vec<FileEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("that restore point is gone")]
    Missing,
    #[error("the restore point is damaged: {0}")]
    Damaged(String),
}

/// What a new restore point is for.
pub struct NewSnapshot<'a> {
    pub instance_id: &'a str,
    /// The world's folder name.
    pub world: &'a str,
    pub name: &'a str,
    pub reason: SnapshotReason,
    pub label: Option<String>,
    pub now: i64,
}

pub struct Snapshots {
    root: PathBuf,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn modified_ms(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

/// A file's SHA-1, as lowercase hex.
pub(crate) fn hash_file(path: &Path) -> std::io::Result<String> {
    let mut reader = BufReader::with_capacity(1 << 16, std::fs::File::open(path)?);
    let mut hasher = Sha1::new();
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

/// Every file in a world, as (relative path with forward slashes, full path, metadata).
fn world_files(world_dir: &Path) -> std::io::Result<Vec<(String, PathBuf, std::fs::Metadata)>> {
    let mut files = Vec::new();
    let mut pending = vec![world_dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let kind = entry.file_type()?;
            let path = entry.path();
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() && entry.file_name() != SESSION_LOCK {
                let relative = path
                    .strip_prefix(world_dir)
                    .map_err(std::io::Error::other)?
                    .to_string_lossy()
                    .replace('\\', "/");
                files.push((relative, path, entry.metadata()?));
            }
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

/// A manifest path as a path inside `world_dir`, refusing anything that would step outside it.
fn inside(world_dir: &Path, relative: &str) -> Result<PathBuf, SnapshotError> {
    let path = Path::new(relative);
    if relative.is_empty() || !path.components().all(|part| matches!(part, Component::Normal(_))) {
        return Err(SnapshotError::Damaged(format!("a file has an odd path: {relative}")));
    }
    Ok(world_dir.join(path))
}

impl Snapshots {
    /// `root` is the journal folder; objects and manifests go inside it.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn object_path(&self, hash: &str) -> PathBuf {
        self.root.join(OBJECTS).join(&hash[..2]).join(&hash[2..])
    }

    fn manifest_path(&self, id: &str) -> PathBuf {
        self.root.join(SNAPSHOTS).join(format!("{id}.json"))
    }

    /// Stores a file's contents under its hash, compressed, unless it's already there. Returns
    /// whether it was new.
    fn store_object(&self, hash: &str, source: &Path) -> std::io::Result<bool> {
        let target = self.object_path(hash);
        if target.is_file() {
            return Ok(false);
        }
        std::fs::create_dir_all(target.parent().expect("objects have a folder"))?;
        let temp = target.with_extension("part");
        {
            let mut encoder = flate2::write::GzEncoder::new(BufWriter::new(std::fs::File::create(&temp)?), flate2::Compression::fast());
            std::io::copy(&mut BufReader::new(std::fs::File::open(source)?), &mut encoder)?;
            encoder.finish()?.flush()?;
        }
        std::fs::rename(temp, target)?;
        Ok(true)
    }

    /// Whether a file's contents are in the store.
    pub fn has_object(&self, hash: &str) -> bool {
        self.object_path(hash).is_file()
    }

    /// Where a stored file's contents are (gzip-compressed).
    pub(crate) fn object_file(&self, hash: &str) -> PathBuf {
        self.object_path(hash)
    }

    /// Adds contents that arrive already compressed (from the sync folder), checking they really
    /// are the file `hash` names before keeping them. False when they aren't (a copy still being
    /// written by a cloud app, or a damaged one): nothing is kept then.
    pub(crate) fn import_object(&self, hash: &str, compressed: &Path) -> std::io::Result<bool> {
        let target = self.object_path(hash);
        if target.is_file() {
            return Ok(true);
        }
        std::fs::create_dir_all(target.parent().expect("objects have a folder"))?;
        let temp = target.with_extension("part");
        std::fs::copy(compressed, &temp)?;
        let mut decoder = flate2::read::GzDecoder::new(BufReader::new(std::fs::File::open(&temp)?));
        let mut hasher = Sha1::new();
        let mut buffer = vec![0u8; 1 << 16];
        let intact = loop {
            match decoder.read(&mut buffer) {
                Ok(0) => break true,
                Ok(read) => hasher.update(&buffer[..read]),
                Err(_) => break false,
            }
        };
        if !intact || hex(&hasher.finalize()) != hash {
            let _ = std::fs::remove_file(&temp);
            return Ok(false);
        }
        std::fs::rename(temp, target)?;
        Ok(true)
    }

    /// A restore point's file list.
    pub fn entries(&self, id: &str) -> Result<Vec<FileEntry>, SnapshotError> {
        Ok(self.load(id)?.entries)
    }

    /// The files of the world's newest restore point, by path: what lets an unchanged file skip
    /// being read again (when its size and time still match).
    fn previous(&self, instance_id: &str, world: &str) -> HashMap<String, FileEntry> {
        self.for_world(instance_id, world)
            .first()
            .and_then(|latest| self.load(&latest.id).ok())
            .map(|manifest| manifest.entries.into_iter().map(|entry| (entry.path.clone(), entry)).collect())
            .unwrap_or_default()
    }

    /// The world's files as they are now, without storing anything: hashed only where they
    /// changed since its newest restore point.
    pub fn scan(&self, world_dir: &Path, instance_id: &str, world: &str) -> Result<Vec<FileEntry>, SnapshotError> {
        let previous = self.previous(instance_id, world);
        let mut entries = Vec::new();
        for (relative, path, metadata) in world_files(world_dir)? {
            let size = metadata.len();
            let modified = modified_ms(&metadata);
            let hash = match previous.get(&relative).filter(|entry| entry.size == size && entry.modified == modified) {
                Some(entry) => entry.hash.clone(),
                None => hash_file(&path)?,
            };
            entries.push(FileEntry { path: relative, hash, size, modified });
        }
        Ok(entries)
    }

    /// A restore point of the world as it is now: its newest one when nothing changed since,
    /// otherwise a new one. Returns it with its files.
    pub fn current(&self, world_dir: &Path, new: NewSnapshot) -> Result<(Summary, Vec<FileEntry>), SnapshotError> {
        if let Some(latest) = self.for_world(new.instance_id, new.world).into_iter().next()
            && let Ok(manifest) = self.load(&latest.id)
        {
            let now = self.scan(world_dir, new.instance_id, new.world)?;
            let same = now.len() == manifest.entries.len()
                && now.iter().zip(&manifest.entries).all(|(a, b)| a.path == b.path && a.hash == b.hash)
                && manifest.entries.iter().all(|entry| self.has_object(&entry.hash));
            if same {
                return Ok((manifest.summary, manifest.entries));
            }
        }
        let summary = self.create(world_dir, new)?;
        let entries = self.load(&summary.id)?.entries;
        Ok((summary, entries))
    }

    /// Records a restore point for files whose contents are already in the store (brought from
    /// the sync folder), without reading the world.
    pub fn adopt(&self, new: NewSnapshot, entries: Vec<FileEntry>) -> Result<Summary, SnapshotError> {
        if let Some(missing) = entries.iter().find(|entry| !self.has_object(&entry.hash)) {
            return Err(SnapshotError::Damaged(format!("{} is missing from the store", missing.path)));
        }
        let total_bytes = entries.iter().map(|entry| entry.size).sum();
        self.record(&new, entries, total_bytes, 0)
    }

    /// Writes a restore point's manifest and adds it to the index.
    fn record(&self, new: &NewSnapshot, entries: Vec<FileEntry>, total_bytes: u64, added_bytes: u64) -> Result<Summary, SnapshotError> {
        // The id is the hash of what the restore point says, the way a commit's is.
        let mut hasher = Sha1::new();
        hasher.update(format!("{}\0{}\0{}\0", new.instance_id, new.world, new.now).as_bytes());
        for entry in &entries {
            hasher.update(format!("{}\0{}\n", entry.path, entry.hash).as_bytes());
        }
        let summary = Summary {
            id: hex(&hasher.finalize()),
            instance_id: new.instance_id.to_string(),
            world: new.world.to_string(),
            name: new.name.to_string(),
            created_at: new.now,
            reason: new.reason,
            label: new.label.clone().filter(|label| !label.trim().is_empty()),
            files: entries.len() as u32,
            total_bytes,
            added_bytes,
        };
        let manifest = Manifest { summary: summary.clone(), entries };
        let path = self.manifest_path(&summary.id);
        std::fs::create_dir_all(path.parent().expect("manifests have a folder"))?;
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_vec(&manifest).map_err(std::io::Error::other)?)?;
        std::fs::rename(temp, &path)?;

        let mut summaries = self.list();
        summaries.retain(|known| known.id != summary.id);
        summaries.push(summary.clone());
        self.write_index(&summaries)?;
        Ok(summary)
    }

    /// Every restore point, newest first.
    pub fn list(&self) -> Vec<Summary> {
        let mut summaries = self.read_index().unwrap_or_else(|| self.rebuild_index());
        summaries.sort_by_key(|summary| std::cmp::Reverse(summary.created_at));
        summaries
    }

    /// One world's restore points, newest first.
    pub fn for_world(&self, instance_id: &str, world: &str) -> Vec<Summary> {
        self.list()
            .into_iter()
            .filter(|summary| summary.instance_id == instance_id && summary.world == world)
            .collect()
    }

    fn read_index(&self) -> Option<Vec<Summary>> {
        let bytes = std::fs::read(self.root.join(SNAPSHOTS).join(INDEX)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    fn write_index(&self, summaries: &[Summary]) -> std::io::Result<()> {
        let dir = self.root.join(SNAPSHOTS);
        std::fs::create_dir_all(&dir)?;
        let temp = dir.join(format!("{INDEX}.tmp"));
        std::fs::write(&temp, serde_json::to_vec(summaries).map_err(std::io::Error::other)?)?;
        std::fs::rename(temp, dir.join(INDEX))
    }

    /// Reads every manifest again, for when the index is missing or damaged.
    fn rebuild_index(&self) -> Vec<Summary> {
        let summaries: Vec<Summary> = std::fs::read_dir(self.root.join(SNAPSHOTS))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.file_name() != INDEX && entry.path().extension().is_some_and(|extension| extension == "json"))
            .filter_map(|entry| std::fs::read(entry.path()).ok())
            .filter_map(|bytes| serde_json::from_slice::<Manifest>(&bytes).ok())
            .map(|manifest| manifest.summary)
            .collect();
        let _ = self.write_index(&summaries);
        summaries
    }

    fn load(&self, id: &str) -> Result<Manifest, SnapshotError> {
        if id.len() != 40 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(SnapshotError::Missing);
        }
        let bytes = match std::fs::read(self.manifest_path(id)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Err(SnapshotError::Missing),
            Err(error) => return Err(error.into()),
        };
        serde_json::from_slice(&bytes).map_err(|error| SnapshotError::Damaged(error.to_string()))
    }

    /// Takes a restore point of a world folder.
    pub fn create(&self, world_dir: &Path, new: NewSnapshot) -> Result<Summary, SnapshotError> {
        // Files unchanged since the world's last restore point keep their hash without a read.
        let previous = self.previous(new.instance_id, new.world);
        let mut entries = Vec::new();
        let mut total_bytes = 0;
        let mut added_bytes = 0;
        for (relative, path, metadata) in world_files(world_dir)? {
            let size = metadata.len();
            let modified = modified_ms(&metadata);
            let known = previous
                .get(&relative)
                .filter(|entry| entry.size == size && entry.modified == modified && self.object_path(&entry.hash).is_file());
            let hash = match known {
                Some(entry) => entry.hash.clone(),
                None => {
                    let hash = hash_file(&path)?;
                    if self.store_object(&hash, &path)? {
                        added_bytes += size;
                    }
                    hash
                }
            };
            total_bytes += size;
            entries.push(FileEntry { path: relative, hash, size, modified });
        }
        self.record(&new, entries, total_bytes, added_bytes)
    }

    /// Puts a world folder back the way a restore point has it: files are rewritten where they
    /// differ, and files the restore point doesn't have are removed.
    pub fn restore(&self, id: &str, world_dir: &Path) -> Result<Summary, SnapshotError> {
        let manifest = self.load(id)?;
        // Check everything is there before touching the world.
        for entry in &manifest.entries {
            inside(world_dir, &entry.path)?;
            if !self.object_path(&entry.hash).is_file() {
                return Err(SnapshotError::Damaged(format!("{} is missing from the store", entry.path)));
            }
        }
        std::fs::create_dir_all(world_dir)?;
        // What this computer knew about the world's files at its previous restore point: a file
        // whose size and time still match that, with the hash the restore wants, needn't be read.
        // (Only this computer's own record is trusted that way, never the times in the restore
        // point being put back, which may have come from another computer.)
        let trusted: HashMap<String, FileEntry> = self
            .for_world(&manifest.summary.instance_id, &manifest.summary.world)
            .into_iter()
            .find(|summary| summary.id != id)
            .and_then(|summary| self.load(&summary.id).ok())
            .map(|known| known.entries.into_iter().map(|entry| (entry.path.clone(), entry)).collect())
            .unwrap_or_default();
        let wanted: HashSet<&str> = manifest.entries.iter().map(|entry| entry.path.as_str()).collect();
        for (relative, path, _) in world_files(world_dir)? {
            if !wanted.contains(relative.as_str()) {
                std::fs::remove_file(path)?;
            }
        }
        for entry in &manifest.entries {
            let target = inside(world_dir, &entry.path)?;
            let same = std::fs::metadata(&target).is_ok_and(|meta| {
                meta.len() == entry.size
                    && (trusted.get(&entry.path).is_some_and(|known| known.size == meta.len() && known.modified == modified_ms(&meta) && known.hash == entry.hash)
                        || hash_file(&target).is_ok_and(|hash| hash == entry.hash))
            });
            if same {
                continue;
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let temp = target.with_extension("wisp-restore");
            {
                let mut decoder = flate2::read::GzDecoder::new(BufReader::new(std::fs::File::open(self.object_path(&entry.hash))?));
                let mut writer = BufWriter::new(std::fs::File::create(&temp)?);
                std::io::copy(&mut decoder, &mut writer)?;
                writer.flush()?;
                if entry.modified > 0 {
                    let time = std::time::UNIX_EPOCH + std::time::Duration::from_millis(entry.modified as u64);
                    let _ = writer.get_ref().set_modified(time);
                }
            }
            std::fs::rename(&temp, &target)?;
        }
        remove_empty_dirs(world_dir);
        Ok(manifest.summary)
    }

    /// Deletes a restore point, then whatever only it was keeping. Returns the bytes freed.
    pub fn delete(&self, id: &str) -> Result<u64, SnapshotError> {
        self.load(id)?;
        std::fs::remove_file(self.manifest_path(id))?;
        let mut summaries = self.list();
        summaries.retain(|summary| summary.id != id);
        self.write_index(&summaries)?;
        Ok(self.collect_garbage()?)
    }

    /// Keeps the newest `keep` automatic (and sync) restore points of a world and deletes older ones.
    /// Returns the bytes freed.
    pub fn prune(&self, instance_id: &str, world: &str, keep: usize) -> Result<u64, SnapshotError> {
        let old: Vec<String> = self
            .for_world(instance_id, world)
            .into_iter()
            .filter(|summary| matches!(summary.reason, SnapshotReason::Auto | SnapshotReason::Sync))
            .skip(keep)
            .map(|summary| summary.id)
            .collect();
        if old.is_empty() {
            return Ok(0);
        }
        for id in &old {
            let _ = std::fs::remove_file(self.manifest_path(id));
        }
        let mut summaries = self.list();
        summaries.retain(|summary| !old.contains(&summary.id));
        self.write_index(&summaries)?;
        Ok(self.collect_garbage()?)
    }

    /// Deletes objects no restore point lists. Returns the bytes freed.
    pub fn collect_garbage(&self) -> std::io::Result<u64> {
        let mut referenced = HashSet::new();
        for summary in self.list() {
            match self.load(&summary.id) {
                Ok(manifest) => referenced.extend(manifest.entries.into_iter().map(|entry| entry.hash)),
                // Better to keep everything than to delete what a damaged manifest needed.
                Err(_) => return Ok(0),
            }
        }
        let mut freed = 0;
        for bucket in std::fs::read_dir(self.root.join(OBJECTS)).into_iter().flatten().flatten() {
            let prefix = bucket.file_name().to_string_lossy().into_owned();
            for object in std::fs::read_dir(bucket.path()).into_iter().flatten().flatten() {
                let hash = format!("{prefix}{}", object.file_name().to_string_lossy());
                if !referenced.contains(&hash) {
                    freed += object.metadata().map_or(0, |meta| meta.len());
                    std::fs::remove_file(object.path())?;
                }
            }
            let _ = std::fs::remove_dir(bucket.path());
        }
        Ok(freed)
    }

    /// The disk space restore points use.
    pub fn usage(&self) -> u64 {
        crate::storage::dir_size(&self.root.join(OBJECTS)) + crate::storage::dir_size(&self.root.join(SNAPSHOTS))
    }
}

/// Removes folders left empty (deepest first), keeping `root` itself.
fn remove_empty_dirs(root: &Path) {
    fn walk(dir: &Path, root: &Path) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                walk(&entry.path(), root);
            }
        }
        if dir != root {
            let _ = std::fs::remove_dir(dir);
        }
    }
    walk(root, root);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(store: &Snapshots, world: &Path, reason: SnapshotReason, now: i64) -> Summary {
        store
            .create(
                world,
                NewSnapshot { instance_id: "fabric", world: "New World", name: "New World", reason, label: None, now },
            )
            .unwrap()
    }

    #[test]
    fn stores_only_what_changed_and_restores_exactly() {
        let root = std::env::temp_dir().join(format!("wisp-snapshots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let world = root.join("saves/New World");
        std::fs::create_dir_all(world.join("region")).unwrap();
        std::fs::write(world.join("level.dat"), b"level one").unwrap();
        std::fs::write(world.join("region/r.0.0.mca"), vec![7u8; 50_000]).unwrap();
        std::fs::write(world.join(SESSION_LOCK), b"locked").unwrap();
        let store = Snapshots::new(root.join("journal"));

        let first = snap(&store, &world, SnapshotReason::Auto, 1);
        assert_eq!((first.files, first.total_bytes), (2, 50_009));
        assert_eq!(first.added_bytes, 50_009);
        assert_eq!(first.id.len(), 40);

        // Only the changed file is stored again.
        std::fs::write(world.join("level.dat"), b"level two!").unwrap();
        std::fs::write(world.join("region/r.1.0.mca"), b"new region").unwrap();
        let second = snap(&store, &world, SnapshotReason::Manual, 2);
        assert_eq!(second.added_bytes, 20);
        assert_eq!(store.for_world("fabric", "New World").len(), 2);

        // Back to the first: the new region goes, level.dat comes back, the lock is left alone.
        store.restore(&first.id, &world).unwrap();
        assert_eq!(std::fs::read(world.join("level.dat")).unwrap(), b"level one");
        assert!(!world.join("region/r.1.0.mca").exists());
        assert_eq!(std::fs::read(world.join("region/r.0.0.mca")).unwrap().len(), 50_000);
        assert!(world.join(SESSION_LOCK).exists());

        // Pruning automatic ones keeps manual ones; garbage goes with what referenced it.
        snap(&store, &world, SnapshotReason::Auto, 3);
        store.prune("fabric", "New World", 1).unwrap();
        let left = store.for_world("fabric", "New World");
        assert_eq!(left.iter().map(|summary| summary.created_at).collect::<Vec<_>>(), vec![3, 2]);
        store.delete(&second.id).unwrap();
        assert_eq!(store.list().len(), 1);
        assert!(store.usage() > 0);

        // The index can be rebuilt from the manifests.
        std::fs::remove_file(root.join("journal/snapshots/index.json")).unwrap();
        assert_eq!(store.list().len(), 1);
        assert!(matches!(store.restore(&"0".repeat(40), &world), Err(SnapshotError::Missing)));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_paths_outside_the_world() {
        let world = Path::new("/tmp/w");
        assert!(inside(world, "../escape").is_err());
        assert!(inside(world, "/etc/passwd").is_err());
        assert!(inside(world, "region/r.0.0.mca").is_ok());
    }
}
