//! Finding the cloud folders already on this computer: the ones iCloud Drive, OneDrive, Google
//! Drive, Dropbox and others keep in step with the cloud. Picking one of them is all it takes to
//! sync; nothing signs in to anything.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::remote::{FOLDER_NAME, Remote, RemoteSummary};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FolderKind {
    Icloud,
    Onedrive,
    GoogleDrive,
    Dropbox,
    Box,
    Nextcloud,
    Syncthing,
    Pcloud,
    /// Any other folder the player picked.
    Folder,
}

impl FolderKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Icloud => "iCloud Drive",
            Self::Onedrive => "OneDrive",
            Self::GoogleDrive => "Google Drive",
            Self::Dropbox => "Dropbox",
            Self::Box => "Box",
            Self::Nextcloud => "Nextcloud",
            Self::Syncthing => "Syncthing",
            Self::Pcloud => "pCloud",
            Self::Folder => "A folder",
        }
    }
}

/// A cloud folder found on this computer.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudFolder {
    pub kind: FolderKind,
    /// "OneDrive", or "OneDrive (Contoso)" for a work account.
    pub label: String,
    /// The cloud folder itself; Wisp's folder goes inside it.
    pub path: PathBuf,
    /// What's already synced there from another computer, if anything.
    pub existing: Option<RemoteSummary>,
}

fn home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).filter(|path| path.is_dir())
}

/// The cloud folders on this computer, the most likely choice first: one that already has your
/// worlds from another computer, then the one this kind of computer usually has.
pub fn detect() -> Vec<CloudFolder> {
    let mut found: Vec<(FolderKind, String, PathBuf)> = Vec::new();
    let mut add = |kind: FolderKind, label: String, path: PathBuf| {
        if path.is_dir() && !found.iter().any(|(_, _, known)| same_place(known, &path)) {
            found.push((kind, label, path));
        }
    };
    if let Some(home) = home() {
        if cfg!(target_os = "macos") {
            add(FolderKind::Icloud, FolderKind::Icloud.label().into(), home.join("Library/Mobile Documents/com~apple~CloudDocs"));
            // Apps that use macOS's File Provider keep their folders in ~/Library/CloudStorage.
            for entry in std::fs::read_dir(home.join("Library/CloudStorage")).into_iter().flatten().flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if let Some((kind, label, path)) = cloud_storage_entry(&name, &entry.path()) {
                    add(kind, label, path);
                }
            }
        }
        if cfg!(windows) {
            for (variable, work) in [("OneDriveConsumer", false), ("OneDriveCommercial", true), ("OneDrive", false)] {
                if let Some(path) = std::env::var_os(variable).map(PathBuf::from) {
                    let label = if work { onedrive_work_label(&path) } else { FolderKind::Onedrive.label().to_string() };
                    add(FolderKind::Onedrive, label, path);
                }
            }
            add(FolderKind::Icloud, FolderKind::Icloud.label().into(), home.join("iCloudDrive"));
            if let Some(path) = dropbox_from_info(&home) {
                add(FolderKind::Dropbox, FolderKind::Dropbox.label().into(), path);
            }
            // Google Drive for desktop shows up as its own drive letter (G: unless that's taken).
            for letter in "GHIJKLMNOPQRSTUVWXYZDEF".chars() {
                let path = PathBuf::from(format!("{letter}:\\My Drive"));
                if path.is_dir() {
                    add(FolderKind::GoogleDrive, FolderKind::GoogleDrive.label().into(), path);
                    break;
                }
            }
            add(FolderKind::Box, FolderKind::Box.label().into(), home.join("Box"));
        }
        for (kind, folder) in [
            (FolderKind::Dropbox, "Dropbox"),
            (FolderKind::Onedrive, "OneDrive"),
            (FolderKind::Nextcloud, "Nextcloud"),
            (FolderKind::Pcloud, "pCloudDrive"),
            (FolderKind::Syncthing, "Sync"),
        ] {
            // ~/Sync is only Syncthing's if Syncthing is about.
            if kind == FolderKind::Syncthing && !home.join("Sync/.stfolder").exists() {
                continue;
            }
            add(kind, kind.label().into(), home.join(folder));
        }
    }
    let mut folders: Vec<CloudFolder> = found
        .into_iter()
        .map(|(kind, label, path)| {
            let remote = Remote::new(path.join(FOLDER_NAME), "");
            let existing = remote.is_set_up().then(|| remote.summary());
            CloudFolder { kind, label, path, existing }
        })
        .collect();
    let usual = |kind: FolderKind| match kind {
        FolderKind::Icloud if cfg!(target_os = "macos") => 0,
        FolderKind::Onedrive if cfg!(windows) => 0,
        FolderKind::Icloud | FolderKind::Onedrive => 1,
        FolderKind::GoogleDrive | FolderKind::Dropbox => 2,
        _ => 3,
    };
    folders.sort_by_key(|folder| (folder.existing.as_ref().map_or(i64::MAX, |summary| -summary.last_seen.unwrap_or(0)), usual(folder.kind)));
    folders
}

/// What a folder in ~/Library/CloudStorage belongs to: "OneDrive-Personal", "OneDrive-Contoso",
/// "GoogleDrive-name@example.com" (files under "My Drive"), "Dropbox", "Box-Box"...
fn cloud_storage_entry(name: &str, path: &Path) -> Option<(FolderKind, String, PathBuf)> {
    let (app, account) = name.split_once('-').unwrap_or((name, ""));
    match app {
        "OneDrive" => {
            let label = match account {
                "" | "Personal" => FolderKind::Onedrive.label().to_string(),
                work => format!("OneDrive ({work})"),
            };
            Some((FolderKind::Onedrive, label, path.to_path_buf()))
        }
        "GoogleDrive" => {
            let drive = path.join("My Drive");
            Some((FolderKind::GoogleDrive, FolderKind::GoogleDrive.label().into(), if drive.is_dir() { drive } else { path.to_path_buf() }))
        }
        "Dropbox" => Some((FolderKind::Dropbox, FolderKind::Dropbox.label().into(), path.to_path_buf())),
        "Box" => Some((FolderKind::Box, FolderKind::Box.label().into(), path.to_path_buf())),
        "pCloud" | "pCloudDrive" => Some((FolderKind::Pcloud, FolderKind::Pcloud.label().into(), path.to_path_buf())),
        _ => None,
    }
}

fn onedrive_work_label(path: &Path) -> String {
    // Work folders are named "OneDrive - Contoso".
    match path.file_name().map(|name| name.to_string_lossy().into_owned()).and_then(|name| name.split_once(" - ").map(|(_, org)| org.to_string())) {
        Some(org) => format!("OneDrive ({org})"),
        None => "OneDrive (work)".into(),
    }
}

/// Dropbox writes where it keeps its folder to info.json.
fn dropbox_from_info(home: &Path) -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| home.join("AppData/Local"));
    let info: serde_json::Value = serde_json::from_slice(&std::fs::read(local.join("Dropbox/info.json")).ok()?).ok()?;
    ["personal", "business"].iter().find_map(|account| info.get(account)?.get("path")?.as_str().map(PathBuf::from)).or_else(|| Some(home.join("Dropbox")))
}

fn same_place(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// The Wisp folder to use for a folder the player picked: the folder itself if it already is
/// one (or is called Wisp), otherwise a Wisp folder inside it.
pub fn wisp_folder_in(picked: &Path) -> PathBuf {
    if Remote::new(picked, "").is_set_up() || picked.file_name().is_some_and(|name| name == FOLDER_NAME) {
        picked.to_path_buf()
    } else {
        picked.join(FOLDER_NAME)
    }
}

/// What kind of cloud folder a picked path is in, judging by where it is.
pub fn kind_of(path: &Path) -> (FolderKind, String) {
    let text = path.to_string_lossy();
    for folder in detect() {
        if path.starts_with(&folder.path) {
            return (folder.kind, folder.label);
        }
    }
    if text.contains("com~apple~CloudDocs") || text.contains("iCloudDrive") {
        return (FolderKind::Icloud, FolderKind::Icloud.label().into());
    }
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    (FolderKind::Folder, if name.is_empty() || name == FOLDER_NAME { FolderKind::Folder.label().into() } else { name })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_cloud_storage_names() {
        let base = Path::new("/nowhere");
        assert_eq!(cloud_storage_entry("OneDrive-Personal", base).map(|found| found.1), Some("OneDrive".into()));
        assert_eq!(cloud_storage_entry("OneDrive-Contoso", base).map(|found| found.1), Some("OneDrive (Contoso)".into()));
        assert_eq!(cloud_storage_entry("GoogleDrive-sam@example.com", base).map(|found| found.0), Some(FolderKind::GoogleDrive));
        assert_eq!(cloud_storage_entry("Dropbox", base).map(|found| found.0), Some(FolderKind::Dropbox));
        assert_eq!(cloud_storage_entry("SomethingElse", base), None);
        assert_eq!(onedrive_work_label(Path::new("C:\\Users\\sam\\OneDrive - Contoso")), "OneDrive (Contoso)");
    }

    #[test]
    fn picks_the_wisp_folder_inside_what_was_picked() {
        assert_eq!(wisp_folder_in(Path::new("/tmp/anything")), Path::new("/tmp/anything/Wisp"));
        assert_eq!(wisp_folder_in(Path::new("/tmp/Wisp")), Path::new("/tmp/Wisp"));
    }
}
