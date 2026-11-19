//! Forge and NeoForge patch the game when they install, using an installer that runs their
//! "processors". Wisp runs that official installer headlessly, pointed at its own data folder:
//! the folder has the same layout as Mojang's launcher folder, so the installed profile and
//! libraries land exactly where launches look for them.

use std::io::Read;
use std::path::Path;
use std::process::Stdio;

use super::LaunchError;
use crate::instances::Loader;

/// Where the official installer for a Forge or NeoForge build lives.
pub fn installer_url(loader: Loader, game: &str, loader_version: &str) -> Option<String> {
    match loader {
        Loader::Forge => {
            let full = format!("{game}-{loader_version}");
            Some(format!(
                "https://maven.minecraftforge.net/net/minecraftforge/forge/{full}/forge-{full}-installer.jar"
            ))
        }
        Loader::NeoForge => Some(format!(
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/{loader_version}/neoforge-{loader_version}-installer.jar"
        )),
        _ => None,
    }
}

/// The version id the installer creates (its folder under `versions/`), read from the
/// `install_profile.json` inside the installer.
pub fn profile_version_id(installer: &Path) -> Result<String, LaunchError> {
    let bad = |why: &str| LaunchError::Installer(format!("the installer looks damaged ({why})"));
    let file = std::fs::File::open(installer)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|_| bad("not a zip"))?;
    let mut entry = archive
        .by_name("install_profile.json")
        .map_err(|_| bad("no install profile"))?;
    let mut text = String::new();
    entry.read_to_string(&mut text)?;
    let profile: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| bad("unreadable install profile"))?;
    // Modern installers say `version`; pre-1.13 ones nest it under `install.target`.
    profile["version"]
        .as_str()
        .or_else(|| profile["install"]["target"].as_str())
        .or_else(|| profile["versionInfo"]["id"].as_str())
        .map(String::from)
        .filter(|id| !id.is_empty() && !id.contains(['/', '\\']) && id != "." && id != "..")
        .ok_or_else(|| bad("no version id"))
}

/// Runs the installer headlessly into `data_dir`.
pub async fn run(java: &Path, installer: &Path, data_dir: &Path) -> Result<(), LaunchError> {
    // The installer insists on finding the official launcher's profile list in the folder.
    let profiles = data_dir.join("launcher_profiles.json");
    if !profiles.exists() {
        std::fs::write(&profiles, br#"{"profiles":{}}"#)?;
    }
    run_with(java, installer, data_dir, &["--installClient", "--install-client"]).await
}

/// Runs the installer headlessly to set up a server in `server_dir` (its libraries, the vanilla
/// server, and the files that start it).
pub async fn run_server(java: &Path, installer: &Path, server_dir: &Path) -> Result<(), LaunchError> {
    run_with(java, installer, server_dir, &["--installServer", "--install-server"]).await
}

async fn run_with(java: &Path, installer: &Path, data_dir: &Path, options: &[&str]) -> Result<(), LaunchError> {
    let work_dir = installer.parent().unwrap_or(data_dir);
    // Forge spells the options --installClient and --installServer; some NeoForge installers use
    // --install-client and --install-server.
    for option in options {
        let mut command = tokio::process::Command::new(java);
        command
            .arg("-jar")
            .arg(installer)
            .arg(option)
            .arg(data_dir)
            .current_dir(work_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let output = command.output().await.map_err(LaunchError::Spawn)?;
        if output.status.success() {
            return Ok(());
        }
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if text.contains("UnrecognizedOption") || text.contains("is not a recognized option") {
            continue;
        }
        let tail: Vec<&str> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .rev()
            .take(6)
            .collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        return Err(LaunchError::Installer(tail.join(" · ")));
    }
    Err(LaunchError::Installer(
        "the installer didn't accept the install option".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installer_urls() {
        assert_eq!(
            installer_url(Loader::Forge, "26.3", "66.1.8").unwrap(),
            "https://maven.minecraftforge.net/net/minecraftforge/forge/26.3-66.1.8/forge-26.3-66.1.8-installer.jar"
        );
        assert_eq!(
            installer_url(Loader::NeoForge, "26.3", "26.3.0.103").unwrap(),
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/26.3.0.103/neoforge-26.3.0.103-installer.jar"
        );
        assert!(installer_url(Loader::Fabric, "26.3", "0.20.1").is_none());
    }

    #[test]
    fn reads_the_version_id_from_an_installer() {
        let path = std::env::temp_dir().join(format!("wisp-installer-{}.jar", std::process::id()));
        {
            let mut writer = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
            writer
                .start_file(
                    "install_profile.json",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            std::io::Write::write_all(&mut writer, br#"{"version":"26.3-forge-66.1.8","spec":1}"#)
                .unwrap();
            writer.finish().unwrap();
        }
        assert_eq!(profile_version_id(&path).unwrap(), "26.3-forge-66.1.8");
        std::fs::remove_file(path).unwrap();
    }
}
