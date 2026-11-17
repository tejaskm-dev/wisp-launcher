//! Faster game starts with Java's class data sharing. Java can map in classes it has already
//! parsed and checked instead of loading them again. Two caches do that here:
//!
//! - Each Java runtime Wisp installed gets its base archive (Mojang's runtimes ship without one),
//!   made once with `java -Xshare:dump`. Java uses it on its own from then on.
//! - Each instance gets a cache of the game's own classes (Java 13 and later), written when the
//!   game exits and used from the next start on. It's tied to the exact Java and classpath, so
//!   any change (another Java, a new loader version) starts a fresh one.
//!
//! A cache that's out of date is only a warning to Java; a launch never depends on one.

use std::path::{Path, PathBuf};

use sha1::{Digest, Sha1};

use crate::instances::Loader;

/// Where an instance keeps its cache, beside its game folder.
const FOLDER: &str = "class-cache";
/// Left in that folder when Java wouldn't start with the cache: it isn't tried again.
const OFF: &str = "off";

/// Makes the base archive for a Java runtime Wisp manages, if it hasn't one yet (under a second).
/// Runtimes picked in settings aren't touched.
pub(crate) async fn ensure_base_archive(java: &Path, runtimes_dir: &Path) {
    if !java.starts_with(runtimes_dir) || base_archive(java).is_none_or(|archive| archive.is_file()) {
        return;
    }
    let mut command = tokio::process::Command::new(java);
    command.arg("-Xshare:dump").stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let _ = tokio::time::timeout(std::time::Duration::from_secs(60), command.status()).await;
}

/// Where Java keeps its base archive: `<home>/lib/server/classes.jsa` (`bin\server` on Windows
/// before Java 9, which this doesn't use).
fn base_archive(java: &Path) -> Option<PathBuf> {
    let home = java.parent()?.parent()?;
    Some(home.join("lib").join("server").join("classes.jsa"))
}

/// The flag that uses this instance's class cache, or has Java write one as the game exits; none
/// when it can't help (or once it's been switched off for the instance).
pub(crate) fn flag(game_dir: &Path, java: &Path, java_major: Option<u32>, loader: Loader, classpath: &[PathBuf]) -> Option<String> {
    // Forge and NeoForge start through their own module layers, which the cache doesn't reach.
    if !matches!(loader, Loader::Vanilla | Loader::Fabric | Loader::Quilt) || java_major.is_none_or(|major| major < 13) {
        return None;
    }
    // Java refuses to start writing a cache when a classpath entry is a folder.
    if classpath.is_empty() || !classpath.iter().all(|entry| entry.is_file()) || !base_archive(java)?.is_file() {
        return None;
    }
    let dir = game_dir.parent()?.join(FOLDER);
    if dir.join(OFF).exists() {
        return None;
    }
    let archive = dir.join(format!("{}.jsa", fingerprint(java, classpath)?));
    if archive.is_file() {
        return Some(format!("-XX:SharedArchiveFile={}", archive.display()));
    }
    std::fs::create_dir_all(&dir).ok()?;
    // One cache per instance: the ones for an older Java or classpath go.
    for entry in std::fs::read_dir(&dir).ok()?.flatten() {
        if entry.path().extension().is_some_and(|extension| extension == "jsa") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    Some(format!("-XX:ArchiveClassesAtExit={}", archive.display()))
}

/// What the cache depends on: the Java, and each classpath jar with its size and time.
fn fingerprint(java: &Path, classpath: &[PathBuf]) -> Option<String> {
    let mut hasher = Sha1::new();
    for path in std::iter::once(java).chain(classpath.iter().map(PathBuf::as_path)) {
        let meta = std::fs::metadata(path).ok()?;
        let modified = meta.modified().ok().and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |since| since.as_secs());
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update(meta.len().to_le_bytes());
        hasher.update(modified.to_le_bytes());
        hasher.update([0]);
    }
    Some(crate::launch::download::hex(&hasher.finalize())[..16].to_string())
}

/// After a game that wouldn't start: if Java blamed the class cache, it's switched off for this
/// instance (the next start goes without it). Returns whether it was.
pub fn switch_off_if_blamed(game_dir: &Path, output: &[String]) -> bool {
    let blamed = output.iter().any(|line| line.contains("[cds]") && (line.contains("rror") || line.contains("Unable to"))) && output.iter().any(|line| line.contains("Error occurred during initialization of VM"));
    if !blamed {
        return false;
    }
    let Some(dir) = game_dir.parent().map(|parent| parent.join(FOLDER)) else {
        return false;
    };
    std::fs::create_dir_all(&dir).is_ok() && std::fs::write(dir.join(OFF), "Java wouldn't start with the class cache.\n").is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caches_only_jar_classpaths_on_newer_java() {
        let root = std::env::temp_dir().join(format!("wisp-cds-{}", std::process::id()));
        let game = root.join("instance").join("game");
        let home = root.join("java");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::create_dir_all(home.join("bin")).unwrap();
        std::fs::create_dir_all(home.join("lib/server")).unwrap();
        let java = home.join("bin").join("java");
        std::fs::write(&java, "").unwrap();
        let jar = root.join("client.jar");
        std::fs::write(&jar, "jar").unwrap();
        let classpath = vec![jar.clone()];

        // No base archive yet: nothing to build on.
        assert_eq!(flag(&game, &java, Some(21), Loader::Fabric, &classpath), None);
        std::fs::write(home.join("lib/server/classes.jsa"), "base").unwrap();
        let first = flag(&game, &java, Some(21), Loader::Fabric, &classpath).unwrap();
        assert!(first.starts_with("-XX:ArchiveClassesAtExit="));
        // Once Java has written it, it's used.
        let written = PathBuf::from(first.trim_start_matches("-XX:ArchiveClassesAtExit="));
        std::fs::write(&written, "cache").unwrap();
        assert_eq!(flag(&game, &java, Some(21), Loader::Fabric, &classpath), Some(format!("-XX:SharedArchiveFile={}", written.display())));
        // Not for old Java, Forge, or a folder on the classpath.
        assert_eq!(flag(&game, &java, Some(8), Loader::Vanilla, &classpath), None);
        assert_eq!(flag(&game, &java, Some(21), Loader::Forge, &classpath), None);
        assert_eq!(flag(&game, &java, Some(21), Loader::Vanilla, std::slice::from_ref(&root)), None);
        // Java blaming the cache switches it off for the instance.
        let output = vec!["[0.1s][error][cds] Error: something".to_string(), "Error occurred during initialization of VM".to_string()];
        assert!(switch_off_if_blamed(&game, &output));
        assert_eq!(flag(&game, &java, Some(21), Loader::Fabric, &classpath), None);
        let _ = std::fs::remove_dir_all(root);
    }
}
