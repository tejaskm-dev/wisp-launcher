//! How much disk Wisp uses, and clearing what it can always download or rebuild again.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// Folders under the data folder that are safe to delete: installers, modpack downloads and
/// unpacked native libraries all come back on the next launch or install.
const CACHE_DIRS: [&str; 3] = ["installers", "cache", "natives"];

/// Bytes on disk per kind of file.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageUsage {
    /// Worlds, mods, screenshots and settings: everything in the instances.
    pub instances: u64,
    /// Game versions and libraries, shared by instances.
    pub game_files: u64,
    /// Sounds, textures and languages, shared by every version.
    pub assets: u64,
    pub java: u64,
    pub cache: u64,
}

/// The size of everything in a folder, without following symlinks.
pub fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    let mut pending: Vec<PathBuf> = vec![path.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            // A DirEntry's metadata describes a symlink itself, not what it points to.
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                total += metadata.len();
            }
        }
    }
    total
}

pub fn usage(data_dir: &Path) -> StorageUsage {
    let size = |name: &str| dir_size(&data_dir.join(name));
    StorageUsage {
        instances: size("instances"),
        game_files: size("versions") + size("libraries"),
        assets: size("assets"),
        java: size("runtimes"),
        cache: CACHE_DIRS.iter().map(|name| size(name)).sum(),
    }
}

/// Deletes the cache folders; returns how many bytes that freed.
pub fn clear_cache(data_dir: &Path) -> std::io::Result<u64> {
    let mut freed = 0;
    for name in CACHE_DIRS {
        let dir = data_dir.join(name);
        let size = dir_size(&dir);
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => freed += size,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(freed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_and_clears() {
        let root = std::env::temp_dir().join(format!("wisp-storage-{}", std::process::id()));
        std::fs::create_dir_all(root.join("installers")).unwrap();
        std::fs::create_dir_all(root.join("instances/a/minecraft")).unwrap();
        std::fs::write(root.join("installers/forge.jar"), vec![0u8; 300]).unwrap();
        std::fs::write(
            root.join("instances/a/minecraft/options.txt"),
            vec![0u8; 50],
        )
        .unwrap();
        let before = usage(&root);
        assert_eq!((before.cache, before.instances), (300, 50));
        assert_eq!(clear_cache(&root).unwrap(), 300);
        assert_eq!(usage(&root).cache, 0);
        assert_eq!(usage(&root).instances, 50);
        std::fs::remove_dir_all(root).unwrap();
    }
}
