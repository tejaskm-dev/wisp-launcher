//! What Wisp learned about a server's mods by running it: mods that only run in the game (they
//! crashed the server as it loaded), kept out of its folder until put back, and mods the server
//! turned out to need although their labels said game only (Modrinth's labels are sometimes
//! wrong), kept in.

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::pack::{hash_file, safe_path};

/// Where set-aside mods go: outside `mods/`, which every loader reads.
pub const SET_ASIDE_DIR: &str = "mods-set-aside";
const RECORD: &str = ".wisp-learned.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Learned {
    set_aside: Vec<SetAside>,
    /// SHA-1s of mods the server needs, whatever their labels say.
    needed: Vec<String>,
}

/// A mod kept out of the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAside {
    /// Its file name (it was `mods/<file>`).
    pub file: String,
    pub name: String,
    pub sha1: String,
    /// Unix milliseconds.
    pub at: i64,
}

fn read(dir: &Path) -> Learned {
    std::fs::read(dir.join(RECORD)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

fn write(dir: &Path, learned: &Learned) -> std::io::Result<()> {
    std::fs::write(dir.join(RECORD), serde_json::to_vec_pretty(learned).map_err(std::io::Error::other)?)
}

/// Mods set aside, most recent first.
pub fn set_aside_list(dir: &Path) -> Vec<SetAside> {
    let mut list = read(dir).set_aside;
    list.sort_by_key(|mod_| std::cmp::Reverse(mod_.at));
    list
}

/// SHA-1s of the mods set aside, which copying the instance's mods in leaves out.
pub fn set_aside_hashes(dir: &Path) -> HashSet<String> {
    read(dir).set_aside.into_iter().map(|mod_| mod_.sha1).collect()
}

/// SHA-1s of the mods the server needs whatever their labels say.
pub fn needed_hashes(dir: &Path) -> HashSet<String> {
    read(dir).needed.into_iter().collect()
}

/// Moves `mods/<file>` out of the server, remembering it so it stays out.
pub fn set_aside(dir: &Path, file: &str, name: &str, now: i64) -> std::io::Result<SetAside> {
    if !safe_path(&format!("mods/{file}")) {
        return Err(std::io::Error::other("not a mod file"));
    }
    let source = dir.join("mods").join(file);
    let (sha1, _) = hash_file(&source).ok_or_else(|| std::io::Error::other("the mod isn't there"))?;
    let target_dir = dir.join(SET_ASIDE_DIR);
    std::fs::create_dir_all(&target_dir)?;
    let target = target_dir.join(file);
    let _ = std::fs::remove_file(&target);
    std::fs::rename(&source, &target)?;
    let mut learned = read(dir);
    learned.set_aside.retain(|known| known.file != file && known.sha1 != sha1);
    learned.needed.retain(|needed| *needed != sha1);
    let entry = SetAside { file: file.to_string(), name: name.to_string(), sha1, at: now };
    learned.set_aside.push(entry.clone());
    write(dir, &learned)?;
    Ok(entry)
}

/// Puts a set-aside mod back in `mods/`, and stops leaving it out.
pub fn put_back(dir: &Path, file: &str) -> std::io::Result<()> {
    if !safe_path(&format!("mods/{file}")) {
        return Err(std::io::Error::other("not a mod file"));
    }
    let source = dir.join(SET_ASIDE_DIR).join(file);
    if source.is_file() {
        std::fs::create_dir_all(dir.join("mods"))?;
        std::fs::rename(&source, dir.join("mods").join(file))?;
    }
    let mut learned = read(dir);
    learned.set_aside.retain(|known| known.file != file);
    write(dir, &learned)
}

/// Remembers that the server needs the mod with this SHA-1 (putting it back if it was set aside).
pub fn mark_needed(dir: &Path, sha1: &str) -> std::io::Result<()> {
    let sha1 = sha1.to_ascii_lowercase();
    if let Some(aside) = read(dir).set_aside.into_iter().find(|known| known.sha1 == sha1) {
        put_back(dir, &aside.file)?;
    }
    let mut learned = read(dir);
    if !learned.needed.contains(&sha1) {
        learned.needed.push(sha1);
    }
    write(dir, &learned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_mods_aside_and_puts_them_back() {
        let dir = std::env::temp_dir().join(format!("wisp-learned-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("mods")).unwrap();
        std::fs::write(dir.join("mods/minimap.jar"), b"a game-only mod").unwrap();
        let entry = set_aside(&dir, "minimap.jar", "Minimap", 5).unwrap();
        assert!(!dir.join("mods/minimap.jar").exists() && dir.join(SET_ASIDE_DIR).join("minimap.jar").is_file());
        assert_eq!(set_aside_list(&dir), vec![entry.clone()]);
        assert!(set_aside_hashes(&dir).contains(&entry.sha1));
        assert!(set_aside(&dir, "../escape.jar", "x", 6).is_err());
        // Needed after all: back in, and no longer left out.
        mark_needed(&dir, &entry.sha1).unwrap();
        assert!(set_aside_list(&dir).is_empty() && needed_hashes(&dir).contains(&entry.sha1));
        assert!(dir.join("mods/minimap.jar").is_file());
        // Set aside by hand later, and put back.
        set_aside(&dir, "minimap.jar", "Minimap", 7).unwrap();
        assert!(!needed_hashes(&dir).contains(&entry.sha1));
        put_back(&dir, "minimap.jar").unwrap();
        assert!(dir.join("mods/minimap.jar").is_file() && set_aside_list(&dir).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
