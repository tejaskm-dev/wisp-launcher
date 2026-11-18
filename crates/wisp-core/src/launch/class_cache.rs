//! Faster game starts with Java's class data sharing: Java maps in the classes of its own library
//! that it has already parsed and checked, instead of loading them again on every start. Mojang's
//! runtimes ship without that archive, so each runtime Wisp installed gets one, made once with
//! `java -Xshare:dump`; Java then uses it on its own, and ignores it if it ever doesn't fit.
//!
//! There used to be a second, per-instance cache of the game's own classes, written as the game
//! exited. Java refuses to start at all when writing one is combined with a Java agent (as the
//! skin server's authlib-injector is), so it's gone; `forget` clears the folders it left.

use std::path::{Path, PathBuf};

/// Where an instance's old per-instance cache was kept, beside its game folder.
const OLD_FOLDER: &str = "class-cache";

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

/// Where Java keeps its base archive: `<home>/lib/server/classes.jsa`.
fn base_archive(java: &Path) -> Option<PathBuf> {
    let home = java.parent()?.parent()?;
    Some(home.join("lib").join("server").join("classes.jsa"))
}

/// Removes the folder the old per-instance cache used, if it's still there.
pub(crate) fn forget(game_dir: &Path) {
    if let Some(dir) = game_dir.parent().map(|parent| parent.join(OLD_FOLDER))
        && dir.is_dir()
    {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forgets_the_old_per_instance_cache() {
        let root = std::env::temp_dir().join(format!("wisp-cds-{}", std::process::id()));
        let game = root.join("instance").join("game");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::create_dir_all(root.join("instance").join(OLD_FOLDER)).unwrap();
        std::fs::write(root.join("instance").join(OLD_FOLDER).join("off"), "").unwrap();
        forget(&game);
        assert!(!root.join("instance").join(OLD_FOLDER).exists());
        assert!(game.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn finds_the_base_archive_beside_the_java_binary() {
        let java = Path::new("/runtimes/java-runtime-delta/jre/bin/java");
        assert_eq!(base_archive(java), Some(PathBuf::from("/runtimes/java-runtime-delta/jre/lib/server/classes.jsa")));
    }
}
