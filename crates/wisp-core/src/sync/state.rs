//! What this computer remembers about syncing, in `<data>/sync/`: its own id, the folder in use,
//! and for every world and instance, the commit it last synced with.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use rand_core::RngCore;
use serde::{Deserialize, Serialize};

use super::folders::FolderKind;

const STATE: &str = "state.json";
const HASHES: &str = "hashes.json";
const BASES: &str = "bases";

/// Where this computer syncs to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderChoice {
    /// The Wisp folder itself.
    pub path: PathBuf,
    pub kind: FolderKind,
    /// "iCloud Drive", "OneDrive"... as the sync page shows it.
    pub label: String,
    /// When sync was turned on here.
    pub since: i64,
}

/// Where an item stood after its last sync on this computer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Synced {
    /// The commit both sides agreed on.
    pub base: String,
    /// Its content hash.
    pub content: String,
    /// When that commit was made, and by which computer.
    pub at: i64,
    pub device: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct State {
    pub device_id: String,
    pub folder: Option<FolderChoice>,
    pub items: BTreeMap<String, Synced>,
    /// Items this computer said no to (taken out of the cloud here, or a big download it was asked
    /// about): not fetched again unless asked.
    pub declined: BTreeSet<String>,
    /// Files Modrinth has, by SHA-1, with where to download them (None: Modrinth doesn't have it).
    pub known_files: BTreeMap<String, Option<String>>,
    /// When the folder's old history was last tidied (by this computer).
    pub last_tidy: i64,
    /// When this computer last finished a sync.
    pub last_synced: Option<i64>,
}

pub struct Store {
    dir: PathBuf,
}

/// A fresh random id for a computer or an instance.
pub fn random_id() -> String {
    let mut bytes = [0u8; 8];
    rand_core::OsRng.fill_bytes(&mut bytes);
    super::remote::hex(&bytes)
}

impl Store {
    /// `data` is Wisp's data folder.
    pub fn new(data: &Path) -> Self {
        Self { dir: data.join("sync") }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The saved state, with this computer's id made up the first time.
    pub fn load(&self) -> State {
        let mut state: State = std::fs::read(self.dir.join(STATE)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
        if state.device_id.is_empty() {
            state.device_id = random_id();
            let _ = self.save(&state);
        }
        state
    }

    pub fn save(&self, state: &State) -> std::io::Result<()> {
        write_json(&self.dir.join(STATE), state)
    }

    /// The manifest an instance had at its last sync: the "before" for three-way merges.
    pub fn base_manifest<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        serde_json::from_slice(&std::fs::read(self.dir.join(BASES).join(format!("{key}.json"))).ok()?).ok()
    }

    pub fn set_base_manifest<T: Serialize>(&self, key: &str, manifest: &T) -> std::io::Result<()> {
        write_json(&self.dir.join(BASES).join(format!("{key}.json")), manifest)
    }

    pub fn forget_base_manifest(&self, key: &str) {
        let _ = std::fs::remove_file(self.dir.join(BASES).join(format!("{key}.json")));
    }

    pub fn hash_cache(&self) -> HashCache {
        HashCache {
            path: self.dir.join(HASHES),
            entries: std::fs::read(self.dir.join(HASHES)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default(),
            dirty: false,
        }
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    std::fs::create_dir_all(path.parent().expect("sync files have a folder"))?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_vec(value).map_err(std::io::Error::other)?)?;
    std::fs::rename(temp, path)
}

/// SHA-1s of instance files (mods, configs, packs), kept with each file's size and time so an
/// unchanged file isn't read again on every sync.
pub struct HashCache {
    path: PathBuf,
    entries: BTreeMap<String, (u64, i64, String)>,
    dirty: bool,
}

impl HashCache {
    pub fn hash(&mut self, file: &Path) -> std::io::Result<(String, u64)> {
        let meta = std::fs::metadata(file)?;
        let modified = meta
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |elapsed| elapsed.as_millis() as i64);
        let key = file.to_string_lossy().into_owned();
        if let Some((size, time, hash)) = self.entries.get(&key)
            && *size == meta.len()
            && *time == modified
        {
            return Ok((hash.clone(), *size));
        }
        let hash = crate::snapshots::hash_file(file)?;
        self.entries.insert(key, (meta.len(), modified, hash.clone()));
        self.dirty = true;
        Ok((hash, meta.len()))
    }

    pub fn save(&mut self) {
        if self.dirty {
            // Forget files that are gone, so the cache doesn't grow forever.
            self.entries.retain(|path, _| Path::new(path).exists());
            let _ = write_json(&self.path, &self.entries);
            self.dirty = false;
        }
    }
}
