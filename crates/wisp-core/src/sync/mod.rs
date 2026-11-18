//! Cloud sync: worlds, instances, options and server lists follow you from computer to computer
//! through storage you already have: a folder a cloud app keeps in step (iCloud Drive, OneDrive,
//! Google Drive, Dropbox, Syncthing...) or any folder you share. Wisp runs no server and signs in
//! to nothing.
//!
//! - `folders` finds the cloud folders on this computer.
//! - `remote` is the sync folder: objects, commits and one file per computer, never edited.
//! - `plan` decides what a world should do from its history and this computer's copy.
//! - `worlds` and `instances` move one world or instance each way; instances merge, worlds never.
//! - `engine` runs it: before a game starts, after it closes, and every so often in between.

mod engine;
mod folders;
mod instances;
mod merge;
mod plan;
mod remote;
mod state;
mod worlds;

use std::collections::HashMap;
use std::path::Path;

pub use engine::{Activity, Choice, Context, DeviceSeen, Direction, ItemState, ItemStatus, LaunchCheck, Overview, Phase, Resolution, Scope, before_launch, check_in, connect, disconnect, fetch_item, overview, resolve, run, take_out, world_states};
pub use folders::{CloudFolder, FolderKind, detect, kind_of, wisp_folder_in};
pub use remote::{FOLDER_NAME, ItemKind, RemoteSummary};
pub use state::FolderChoice;
pub use worlds::world_key;

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Snapshot(#[from] crate::snapshots::SnapshotError),
    #[error("{0}")]
    Remote(#[from] remote::RemoteError),
    /// A file changed while it was being sent; the next sync sends the new one.
    #[error("a file changed while it was being sent")]
    Changing,
    #[error("{0}")]
    Other(String),
}

/// Why a download didn't happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchError {
    /// The server says the file isn't there any more (or sent something else): it was taken down.
    Gone,
    /// It couldn't be asked: no internet, or the server didn't answer.
    Unreachable,
}

/// What sync needs from the internet: mods are downloaded from Modrinth rather than copied
/// through the sync folder. Implemented by the app (and by tests, without any internet).
pub trait Fetch {
    /// Downloads `url` into `target`, checking the file's SHA-1.
    fn download(&self, url: &str, sha1: &str, target: &Path) -> Result<(), FetchError>;
    /// Which of these files Modrinth has, by SHA-1, with where to download each. None when
    /// Modrinth can't be asked right now.
    fn known(&self, sha1s: &[String]) -> Option<HashMap<String, String>>;
}

/// This computer's sync id (made up the first time it's asked for).
pub fn this_device(data: &Path) -> String {
    state::Store::new(data).load().device_id
}

/// A SHA-1 as lowercase hex, for checking downloads.
pub fn sha1_of(bytes: &[u8]) -> String {
    remote::sha1_hex(bytes)
}

/// No internet: everything goes through the sync folder.
pub struct Offline;

impl Fetch for Offline {
    fn download(&self, _url: &str, _sha1: &str, _target: &Path) -> Result<(), FetchError> {
        Err(FetchError::Unreachable)
    }

    fn known(&self, _sha1s: &[String]) -> Option<HashMap<String, String>> {
        None
    }
}
