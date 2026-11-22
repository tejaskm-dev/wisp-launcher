//! Plugin servers: Paper, Purpur and Folia (downloaded from their makers, checked, kept with
//! Wisp's game files and updated to the newest build at each start), and Spigot, which can only be
//! built from source with its BuildTools (Git needed outside Windows, a few minutes the first time).
//! They run Bukkit plugins from `plugins/`; players join with the plain game.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::Digest as _;

use crate::HttpClient;
use crate::launch::{self, Download, GameFiles, LaunchError, LaunchUpdate};

const FILL: &str = "https://fill.papermc.io/v3/projects";
const PURPUR: &str = "https://api.purpurmc.org/v2/purpur";
const BUILD_TOOLS: &str = "https://hub.spigotmc.org/jenkins/job/BuildTools/lastSuccessfulBuild/artifact/target/BuildTools.jar";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Software {
    Paper,
    Purpur,
    Folia,
    Spigot,
}

impl Software {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Paper => "Paper",
            Self::Purpur => "Purpur",
            Self::Folia => "Folia",
            Self::Spigot => "Spigot",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Paper => "paper",
            Self::Purpur => "purpur",
            Self::Folia => "folia",
            Self::Spigot => "spigot",
        }
    }

    /// The loaders Modrinth lists plugins under that run on it (Paper runs Spigot's and
    /// Bukkit's; Purpur runs Paper's too; Folia runs only plugins made for it).
    pub fn plugin_loaders(self) -> &'static [&'static str] {
        match self {
            Self::Paper => &["paper", "spigot", "bukkit"],
            Self::Purpur => &["purpur", "paper", "spigot", "bukkit"],
            Self::Folia => &["folia"],
            Self::Spigot => &["spigot", "bukkit"],
        }
    }

    /// The Minecraft versions it has builds for, newest first (releases only).
    pub async fn versions(self, client: &HttpClient) -> Result<Vec<String>, LaunchError> {
        let mut versions: Vec<String> = match self {
            Self::Paper | Self::Folia => {
                #[derive(Deserialize)]
                struct Project {
                    versions: std::collections::BTreeMap<String, Vec<String>>,
                }
                let project: Project = launch::fetch_json(client, &format!("{FILL}/{}", self.as_str())).await?;
                project.versions.into_values().flatten().collect()
            }
            Self::Purpur => {
                #[derive(Deserialize)]
                struct Project {
                    versions: Vec<String>,
                }
                let project: Project = launch::fetch_json(client, PURPUR).await?;
                project.versions
            }
            // BuildTools builds any release from 1.8 on; the vanilla list is the one to offer.
            Self::Spigot => return Ok(Vec::new()),
        };
        versions.retain(|version| is_release(version));
        versions.sort_by(|a, b| compare_versions(b, a));
        versions.dedup();
        Ok(versions)
    }

    /// Its server jar for `game_version`, ready to start: the newest build, downloaded or built
    /// once and kept under Wisp's game files (the last one kept is used when offline).
    pub async fn jar(self, client: &HttpClient, files: &GameFiles, game_version: &str, java: &Path, report: &(dyn Fn(LaunchUpdate) + Send + Sync)) -> Result<PathBuf, LaunchError> {
        let folder = files.root.join("servers").join(self.as_str());
        std::fs::create_dir_all(&folder)?;
        match self {
            Self::Paper | Self::Folia => fill_jar(self, client, &folder, game_version, report).await,
            Self::Purpur => purpur_jar(client, &folder, game_version, report).await,
            Self::Spigot => spigot_jar(client, &folder, game_version, java, report).await,
        }
    }
}

/// "1.21.4", "26.3": no "-rc", "-pre", snapshots or other tags.
fn is_release(version: &str) -> bool {
    !version.is_empty() && version.chars().all(|c| c.is_ascii_digit() || c == '.')
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parts = |text: &str| text.split('.').map(|part| part.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>();
    parts(a).cmp(&parts(b))
}

/// The newest jar already kept for a version (by build number), for when the makers can't be reached.
fn kept(folder: &Path, prefix: &str) -> Option<PathBuf> {
    std::fs::read_dir(folder)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let build = name.strip_prefix(prefix)?.strip_suffix(".jar")?.parse::<u64>().ok()?;
            Some((build, entry.path()))
        })
        .max_by_key(|(build, _)| *build)
        .map(|(_, path)| path)
}

fn no_build(software: Software, game_version: &str) -> LaunchError {
    LaunchError::NoServerBuild { software: software.display_name(), game: game_version.to_string() }
}

async fn fill_jar(software: Software, client: &HttpClient, folder: &Path, game_version: &str, report: &(dyn Fn(LaunchUpdate) + Send + Sync)) -> Result<PathBuf, LaunchError> {
    #[derive(Deserialize)]
    struct Build {
        id: u64,
        downloads: std::collections::HashMap<String, FillDownload>,
    }
    #[derive(Deserialize)]
    struct FillDownload {
        checksums: Checksums,
        size: u64,
        url: String,
    }
    #[derive(Deserialize)]
    struct Checksums {
        sha256: String,
    }
    let prefix = format!("{}-{game_version}-", software.as_str());
    let url = format!("{FILL}/{}/versions/{game_version}/builds/latest", software.as_str());
    let latest: Build = match tokio::time::timeout(Duration::from_secs(10), launch::fetch_json(client, &url)).await {
        Ok(Ok(build)) => build,
        Ok(Err(LaunchError::Download(launch::DownloadError::Status { status: 404, .. }))) => return Err(no_build(software, game_version)),
        // Offline: the newest one kept will do.
        _ => return kept(folder, &prefix).ok_or_else(|| LaunchError::BadVersion(format!("couldn't reach PaperMC for {}", software.display_name()))),
    };
    let download = latest.downloads.get("server:default").ok_or_else(|| no_build(software, game_version))?;
    let path = folder.join(format!("{prefix}{}.jar", latest.id));
    if !path.is_file() || !sha256_matches(&path, &download.checksums.sha256) {
        let _ = std::fs::remove_file(&path);
        launch::download_with_progress(client, vec![Download { url: download.url.clone(), path: path.clone(), sha1: None, size: Some(download.size), executable: false }], 1, report).await?;
        if !sha256_matches(&path, &download.checksums.sha256) {
            let _ = std::fs::remove_file(&path);
            return Err(LaunchError::BadVersion(format!("{}'s download arrived damaged", software.display_name())));
        }
    }
    forget_older(folder, &prefix, &path);
    Ok(path)
}

async fn purpur_jar(client: &HttpClient, folder: &Path, game_version: &str, report: &(dyn Fn(LaunchUpdate) + Send + Sync)) -> Result<PathBuf, LaunchError> {
    #[derive(Deserialize)]
    struct Build {
        build: String,
        md5: Option<String>,
    }
    let prefix = format!("purpur-{game_version}-");
    let latest: Build = match tokio::time::timeout(Duration::from_secs(10), launch::fetch_json(client, &format!("{PURPUR}/{game_version}/latest"))).await {
        Ok(Ok(build)) => build,
        Ok(Err(LaunchError::Download(launch::DownloadError::Status { status: 404, .. }))) => return Err(no_build(Software::Purpur, game_version)),
        _ => return kept(folder, &prefix).ok_or_else(|| LaunchError::BadVersion("couldn't reach PurpurMC".into())),
    };
    let path = folder.join(format!("{prefix}{}.jar", latest.build));
    let fine = |path: &Path| latest.md5.as_deref().is_none_or(|md5| md5_matches(path, md5));
    if !path.is_file() || !fine(&path) {
        let _ = std::fs::remove_file(&path);
        let url = format!("{PURPUR}/{game_version}/{}/download", latest.build);
        launch::download_with_progress(client, vec![Download { url, path: path.clone(), sha1: None, size: None, executable: false }], 1, report).await?;
        if !fine(&path) {
            let _ = std::fs::remove_file(&path);
            return Err(LaunchError::BadVersion("Purpur's download arrived damaged".into()));
        }
    }
    forget_older(folder, &prefix, &path);
    Ok(path)
}

/// Spigot can't be downloaded: its BuildTools builds it from source (once per version).
async fn spigot_jar(client: &HttpClient, folder: &Path, game_version: &str, java: &Path, report: &(dyn Fn(LaunchUpdate) + Send + Sync)) -> Result<PathBuf, LaunchError> {
    let jar = folder.join(format!("spigot-{game_version}.jar"));
    if jar.is_file() {
        return Ok(jar);
    }
    // BuildTools fetches its own Git on Windows; elsewhere it needs the computer's.
    if !cfg!(windows) && !crate::device::quiet_command("git").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|status| status.success()) {
        return Err(LaunchError::Installer("Spigot is built from source, which needs Git. Install Git (on a Mac, run xcode-select --install in Terminal), or pick Paper: it runs Spigot's plugins too".into()));
    }
    let tools = folder.join("BuildTools.jar");
    if !tools.is_file() {
        launch::download_with_progress(client, vec![Download { url: BUILD_TOOLS.into(), path: tools.clone(), sha1: None, size: None, executable: false }], 1, report).await?;
    }
    report(LaunchUpdate::Installing { loader: "Spigot (building it takes a few minutes the first time)".into() });
    let work = folder.join("build");
    std::fs::create_dir_all(&work)?;
    let mut command = tokio::process::Command::new(java);
    command
        .arg("-jar")
        .arg(&tools)
        .args(["--rev", game_version, "--compile", "spigot", "--nogui", "--final-name"])
        .arg(format!("spigot-{game_version}.jar"))
        .arg("--output-dir")
        .arg(folder)
        .current_dir(&work)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let output = command.output().await.map_err(LaunchError::Spawn)?;
    // Sources and tools take a gigabyte or so; only the jar is kept.
    let _ = std::fs::remove_dir_all(&work);
    if !output.status.success() || !jar.is_file() {
        let text = format!("{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        let tail: Vec<&str> = text.lines().filter(|line| !line.trim().is_empty()).rev().take(4).collect();
        return Err(LaunchError::Installer(format!("BuildTools couldn't build Spigot {game_version}: {}", tail.into_iter().rev().collect::<Vec<_>>().join(" · "))));
    }
    Ok(jar)
}

/// Keeps only the newest build of a version.
fn forget_older(folder: &Path, prefix: &str, keep: &Path) {
    for entry in std::fs::read_dir(folder).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(prefix) && name.ends_with(".jar") && entry.path() != keep {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn sha256_matches(path: &Path, expected: &str) -> bool {
    let Ok(mut file) = std::fs::File::open(path) else { return false };
    let mut hasher = sha2::Sha256::new();
    std::io::copy(&mut file, &mut hasher).is_ok() && crate::launch::hex(hasher.finalize().as_slice()).eq_ignore_ascii_case(expected)
}

fn md5_matches(path: &Path, expected: &str) -> bool {
    use md5::Digest as _;
    use std::io::Read as _;
    let Ok(mut file) = std::fs::File::open(path) else { return false };
    let mut hasher = md5::Md5::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => hasher.update(&buffer[..read]),
            Err(_) => return false,
        }
    }
    crate::launch::hex(hasher.finalize().as_slice()).eq_ignore_ascii_case(expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_releases_newest_first_and_finds_kept_builds() {
        assert!(is_release("1.21.4") && is_release("26.3"));
        assert!(!is_release("1.21.11-rc3") && !is_release("26.3-rc-3") && !is_release("1.21.9-pre4"));
        let mut versions = vec!["1.9".to_string(), "26.3".into(), "1.21.10".into(), "1.21.4".into()];
        versions.sort_by(|a, b| compare_versions(b, a));
        assert_eq!(versions, ["26.3", "1.21.10", "1.21.4", "1.9"]);
        let folder = std::env::temp_dir().join(format!("wisp-software-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        for name in ["paper-1.21.4-230.jar", "paper-1.21.4-232.jar", "paper-1.21.5-10.jar"] {
            std::fs::write(folder.join(name), b"jar").unwrap();
        }
        assert_eq!(kept(&folder, "paper-1.21.4-").unwrap().file_name().unwrap(), "paper-1.21.4-232.jar");
        forget_older(&folder, "paper-1.21.4-", &folder.join("paper-1.21.4-232.jar"));
        assert!(!folder.join("paper-1.21.4-230.jar").exists() && folder.join("paper-1.21.5-10.jar").exists());
        std::fs::remove_dir_all(folder).unwrap();
    }
}
