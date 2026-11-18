//! Moving one world each way. Worlds go through the restore point store: sending one sends a
//! restore point of it, and bringing one here makes a restore point first and restores it, so
//! every version that passes through a computer can be gone back to.

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::remote::{Commit, ItemKind, Remote, RemoteError, sha1_hex};
use super::SyncError;
use crate::snapshots::{FileEntry, NewSnapshot, SnapshotReason, Snapshots};

/// The full file list of a synced world, stored as an object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldManifest {
    pub files: Vec<FileEntry>,
}

/// What a world holds, ignoring file times: equal for identical worlds on any computer.
pub fn content_hash(files: &[FileEntry]) -> String {
    let mut text = String::new();
    for file in files {
        text.push_str(&file.path);
        text.push('\0');
        text.push_str(&file.hash);
        text.push('\n');
    }
    sha1_hex(text.as_bytes())
}

/// A world's key in the sync folder: its instance and its folder name (hashed, since folder
/// names can hold characters some cloud services refuse).
pub fn world_key(instance_sync: &str, folder: &str) -> String {
    format!("w-{instance_sync}-{}", &sha1_hex(folder.as_bytes())[..16])
}

/// A world on this computer, and everything needed to name it in the sync folder.
pub struct World<'a> {
    pub key: &'a str,
    /// The instance's id on this computer.
    pub instance_id: &'a str,
    /// The instance's id everywhere.
    pub instance_sync: &'a str,
    pub folder: &'a str,
    pub dir: &'a Path,
    pub name: &'a str,
}

pub struct Who<'a> {
    pub device: &'a str,
    pub device_name: &'a str,
    pub now: i64,
}

/// Sends the world as it is now on top of `parents`. Returns the commit.
pub fn upload(remote: &Remote, snapshots: &Snapshots, world: &World, parents: Vec<String>, who: &Who, progress: &dyn Fn(u64, u64)) -> Result<Commit, SyncError> {
    let (_, files) = snapshots.current(
        world.dir,
        NewSnapshot { instance_id: world.instance_id, world: world.folder, name: world.name, reason: SnapshotReason::Sync, label: None, now: who.now },
    )?;
    let mut seen = HashSet::new();
    let needed: Vec<&FileEntry> = files.iter().filter(|file| seen.insert(file.hash.as_str()) && !remote.has(&file.hash)).collect();
    let total: u64 = needed.iter().map(|file| file.size).sum();
    let mut done = 0;
    progress(0, total);
    for file in needed {
        remote.put_compressed(&file.hash, &snapshots.object_file(&file.hash))?;
        done += file.size;
        progress(done, total);
    }
    let manifest = remote.put_json(&WorldManifest { files: files.clone() })?;
    let commit = remote.write_commit(Commit {
        id: String::new(),
        item: world.key.to_string(),
        kind: ItemKind::World,
        parents,
        device: who.device.to_string(),
        device_name: who.device_name.to_string(),
        at: who.now,
        instance: world.instance_sync.to_string(),
        world: Some(world.folder.to_string()),
        name: world.name.to_string(),
        manifest,
        content: content_hash(&files),
        files: files.len() as u32,
        bytes: files.iter().map(|file| file.size).sum(),
        removed: false,
    })?;
    Ok(commit)
}

/// Brings a commit's version of the world here. With `keep_current`, the world as it is now is
/// kept as a restore point first (when it has changes the cloud doesn't). Returns how many files
/// are still on their way into the sync folder, if it had to stop for them: nothing in the world
/// is touched then.
pub fn download(remote: &Remote, snapshots: &Snapshots, world: &World, commit: &Commit, keep_current: bool, now: i64, progress: &dyn Fn(u64, u64)) -> Result<Option<u32>, SyncError> {
    let manifest: WorldManifest = match remote.get_json(&commit.manifest) {
        Ok(manifest) => manifest,
        Err(RemoteError::Missing) => {
            remote.ask_cloud_for(&commit.manifest);
            return Ok(Some(1));
        }
        Err(error) => return Err(error.into()),
    };
    let mut seen = HashSet::new();
    let needed: Vec<&FileEntry> = manifest.files.iter().filter(|file| seen.insert(file.hash.as_str()) && !snapshots.has_object(&file.hash)).collect();
    let total: u64 = needed.iter().map(|file| file.size).sum();
    let mut done = 0;
    let mut waiting = 0;
    progress(0, total);
    for file in needed {
        let source = remote.object_path(&file.hash);
        if !source.is_file() || !snapshots.import_object(&file.hash, &source)? {
            remote.ask_cloud_for(&file.hash);
            waiting += 1;
            continue;
        }
        done += file.size;
        progress(done, total);
    }
    if waiting > 0 {
        return Ok(Some(waiting));
    }
    if keep_current && world.dir.join("level.dat").is_file() {
        snapshots.create(
            world.dir,
            NewSnapshot { instance_id: world.instance_id, world: world.folder, name: world.name, reason: SnapshotReason::BeforeRestore, label: Some("Before syncing".into()), now },
        )?;
    }
    let summary = snapshots.adopt(
        NewSnapshot { instance_id: world.instance_id, world: world.folder, name: &commit.name, reason: SnapshotReason::Sync, label: Some(format!("From {}", commit.device_name)), now },
        manifest.files,
    )?;
    snapshots.restore(&summary.id, world.dir)?;
    Ok(None)
}

/// Writes a commit that ties several branches back together, with the contents of `like`.
pub fn join(remote: &Remote, heads: &[&Commit], like: &Commit, who: &Who) -> Result<Commit, SyncError> {
    Ok(remote.write_commit(Commit {
        id: String::new(),
        parents: heads.iter().map(|commit| commit.id.clone()).collect(),
        device: who.device.to_string(),
        device_name: who.device_name.to_string(),
        at: who.now,
        ..like.clone()
    })?)
}

/// Marks a world (or instance) as taken out of the cloud.
pub fn remove(remote: &Remote, heads: &[&Commit], like: &Commit, who: &Who) -> Result<Commit, SyncError> {
    Ok(remote.write_commit(Commit {
        id: String::new(),
        parents: heads.iter().map(|commit| commit.id.clone()).collect(),
        device: who.device.to_string(),
        device_name: who.device_name.to_string(),
        at: who.now,
        manifest: String::new(),
        content: String::new(),
        files: 0,
        bytes: 0,
        removed: true,
        ..like.clone()
    })?)
}
