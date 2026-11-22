//! Getting a server ready to start: Java and Mojang's server jar (kept with the rest of Wisp's game
//! files), then the loader's own server software in the server's folder: Fabric's server launcher,
//! Quilt's installer, or Forge's and NeoForge's installers, run headlessly. Each is set up once
//! per loader build; later starts only check the files are there.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::Deserialize;

use super::HostedServer;
use crate::HttpClient;
use crate::instances::Loader;
use crate::launch::{self, Download, DownloadError, GameFiles, LaunchError, LaunchUpdate, ServerRuntime};
use crate::url_encode;

const FABRIC_INSTALLERS: &str = "https://meta.fabricmc.net/v2/versions/installer";
const FABRIC_LOADER: &str = "https://meta.fabricmc.net/v2/versions/loader";
const QUILT_INSTALLERS: &str = "https://meta.quiltmc.org/v3/versions/installer";
/// Which loader build the folder was set up for, so a change of build sets it up again.
const SET_UP: &str = ".wisp-loader";
/// Garbage collection for a server: Java's G1 with short pauses (Java 8 doesn't pick it alone).
const GC_FLAGS: [&str; 2] = ["-XX:+UseG1GC", "-XX:MaxGCPauseMillis=130"];

/// How to start a server: Java and its arguments, run in the server's folder.
#[derive(Debug, Clone, PartialEq)]
pub struct StartPlan {
    pub java: PathBuf,
    pub args: Vec<String>,
    pub dir: PathBuf,
}

impl StartPlan {
    /// Has the server listen on `port` instead of the one in server.properties, which stays as the
    /// player set it (Wisp takes players there and passes them on). Every loader passes the game's
    /// own `--port` through.
    pub fn listen_on(&mut self, port: u16) {
        let at = self.args.iter().rposition(|arg| arg == "nogui").unwrap_or(self.args.len());
        self.args.splice(at..at, ["--port".to_string(), port.to_string()]);
    }
}

pub struct PrepareOptions<'a> {
    pub memory_mb: u32,
    /// Which Java to use; none picks what the Minecraft version wants.
    pub java_major: Option<u32>,
    /// Java executables picked in Settings, by major version.
    pub java_paths: &'a HashMap<u32, PathBuf>,
    /// Extra JVM arguments, already split into words.
    pub jvm_args: Vec<String>,
    pub concurrency: usize,
}

/// Downloads and sets up whatever the server still needs, and works out how to start it.
pub async fn prepare(
    client: &HttpClient,
    files: &GameFiles,
    server: &HostedServer,
    dir: &Path,
    options: &PrepareOptions<'_>,
    report: &(dyn Fn(LaunchUpdate) + Send + Sync),
) -> Result<StartPlan, LaunchError> {
    let runtime = launch::server_runtime(client, files, &server.game_version, server.loader, options.java_major, options.java_paths, options.concurrency, report).await?;
    std::fs::create_dir_all(dir)?;
    let start = match (server.software, server.loader) {
        (Some(software), _) => {
            let jar = software.jar(client, files, &server.game_version, &runtime.java, report).await?;
            // Paper and its forks patch Mojang's jar, which they'd download again: Wisp's checked
            // copy goes where they look first.
            if software != super::software::Software::Spigot {
                let _ = std::fs::create_dir_all(dir.join("cache"));
                let _ = place_server_jar(&runtime, dir, &format!("cache/mojang_{}.jar", server.game_version));
            }
            vec!["-jar".to_string(), jar.to_string_lossy().into_owned()]
        }
        (None, Loader::Vanilla) => vec!["-jar".to_string(), runtime.server_jar.to_string_lossy().into_owned()],
        (None, Loader::Fabric) => fabric(client, dir, server, &runtime, options.concurrency, report).await?,
        (None, Loader::Quilt) => quilt(client, files, dir, server, &runtime, options.concurrency, report).await?,
        (None, Loader::Forge | Loader::NeoForge) => forge(client, files, dir, server, &runtime, options.concurrency, report).await?,
    };
    let mut args = vec![format!("-Xmx{}M", options.memory_mb), "-Dlog4j2.formatMsgNoLookups=true".to_string()];
    args.extend(GC_FLAGS.iter().map(|flag| flag.to_string()));
    // The player's own come after Wisp's, so theirs win.
    args.extend(options.jvm_args.iter().cloned());
    args.extend(start);
    args.push("nogui".into());
    Ok(StartPlan { java: runtime.java, args, dir: dir.to_path_buf() })
}

fn loader_version(server: &HostedServer) -> Result<&str, LaunchError> {
    server.loader_version.as_deref().ok_or_else(|| LaunchError::BadVersion("the server has no loader version".into()))
}

fn set_up_for(dir: &Path) -> Option<String> {
    std::fs::read_to_string(dir.join(SET_UP)).ok().map(|text| text.trim().to_string())
}

fn record_set_up(dir: &Path, what: &str) -> std::io::Result<()> {
    std::fs::write(dir.join(SET_UP), what)
}

/// Fetches one file, reporting progress; a 404 means the loader has no such build.
async fn fetch(client: &HttpClient, url: String, path: &Path, server: &HostedServer, concurrency: usize, report: &(dyn Fn(LaunchUpdate) + Send + Sync)) -> Result<(), LaunchError> {
    let download = Download { url, path: path.to_path_buf(), sha1: None, size: None, executable: false };
    launch::download_with_progress(client, vec![download], concurrency, report).await.map_err(|error| match error {
        LaunchError::Download(DownloadError::Status { status: 404, .. }) => LaunchError::NoLoader {
            loader: server.loader.display_name(),
            version: server.loader_version.clone().unwrap_or_default(),
            game: server.game_version.clone(),
        },
        other => other,
    })
}

#[derive(Deserialize)]
struct InstallerVersion {
    version: String,
    #[serde(default)]
    stable: Option<bool>,
    #[serde(default)]
    url: Option<String>,
}

/// Puts Mojang's server jar (Wisp's checked copy) in the folder under `name`, as a hard link
/// where the disk allows (no extra space).
fn place_server_jar(runtime: &ServerRuntime, dir: &Path, name: &str) -> std::io::Result<()> {
    let target = dir.join(name);
    let _ = std::fs::remove_file(&target);
    if std::fs::hard_link(&runtime.server_jar, &target).is_err() {
        std::fs::copy(&runtime.server_jar, &target)?;
    }
    Ok(())
}

/// Fabric's server launcher: it reads `server.jar` beside it and fetches Fabric's libraries itself
/// on first start.
async fn fabric(client: &HttpClient, dir: &Path, server: &HostedServer, runtime: &ServerRuntime, concurrency: usize, report: &(dyn Fn(LaunchUpdate) + Send + Sync)) -> Result<Vec<String>, LaunchError> {
    const JAR: &str = "fabric-server-launch.jar";
    let loader = loader_version(server)?;
    let wanted = format!("fabric {} {loader}", server.game_version);
    if !dir.join(JAR).is_file() || set_up_for(dir).as_deref() != Some(wanted.as_str()) {
        let installers: Vec<InstallerVersion> = launch::fetch_json(client, FABRIC_INSTALLERS).await?;
        let installer = installers
            .iter()
            .find(|installer| installer.stable == Some(true))
            .or_else(|| installers.first())
            .ok_or_else(|| LaunchError::BadVersion("Fabric lists no installer".into()))?;
        let url = format!("{FABRIC_LOADER}/{}/{}/{}/server/jar", url_encode(&server.game_version), url_encode(loader), url_encode(&installer.version));
        let _ = std::fs::remove_file(dir.join(JAR));
        fetch(client, url, &dir.join(JAR), server, concurrency, report).await?;
        record_set_up(dir, &wanted)?;
    }
    place_server_jar(runtime, dir, "server.jar")?;
    Ok(vec!["-jar".into(), JAR.into()])
}

/// Quilt's installer sets up its server launcher and libraries in the folder.
async fn quilt(client: &HttpClient, files: &GameFiles, dir: &Path, server: &HostedServer, runtime: &ServerRuntime, concurrency: usize, report: &(dyn Fn(LaunchUpdate) + Send + Sync)) -> Result<Vec<String>, LaunchError> {
    const JAR: &str = "quilt-server-launch.jar";
    let loader = loader_version(server)?;
    let wanted = format!("quilt {} {loader}", server.game_version);
    if !dir.join(JAR).is_file() || set_up_for(dir).as_deref() != Some(wanted.as_str()) {
        let installers: Vec<InstallerVersion> = launch::fetch_json(client, QUILT_INSTALLERS).await?;
        let installer = installers.first().ok_or_else(|| LaunchError::BadVersion("Quilt lists no installer".into()))?;
        let url = installer.url.clone().unwrap_or_else(|| {
            format!("https://maven.quiltmc.org/repository/release/org/quiltmc/quilt-installer/{0}/quilt-installer-{0}.jar", installer.version)
        });
        let path = files.root.join("installers").join(format!("quilt-installer-{}.jar", installer.version));
        if !path.is_file() {
            fetch(client, url, &path, server, concurrency, report).await?;
        }
        place_server_jar(runtime, dir, "server.jar")?;
        report(LaunchUpdate::Installing { loader: "Quilt".into() });
        let install_dir = format!("--install-dir={}", dir.to_string_lossy());
        run_jar(&runtime.java, &path, &["install", "server", &server.game_version, loader, &install_dir], dir).await?;
        if !dir.join(JAR).is_file() {
            return Err(LaunchError::Installer("it finished, but Quilt's server launcher is missing".into()));
        }
        record_set_up(dir, &wanted)?;
    }
    // The launcher looks for server.jar beside it.
    place_server_jar(runtime, dir, "server.jar")?;
    Ok(vec!["-jar".into(), JAR.into()])
}

/// Forge's and NeoForge's official installers, with their server option.
async fn forge(client: &HttpClient, files: &GameFiles, dir: &Path, server: &HostedServer, runtime: &ServerRuntime, concurrency: usize, report: &(dyn Fn(LaunchUpdate) + Send + Sync)) -> Result<Vec<String>, LaunchError> {
    let loader = loader_version(server)?;
    if let Some(start) = forge_start(dir, server.loader, &server.game_version, loader) {
        return Ok(start);
    }
    let url = launch::installer::installer_url(server.loader, &server.game_version, loader).ok_or_else(|| LaunchError::BadVersion("this loader has no installer".into()))?;
    let file_name = url.rsplit('/').next().unwrap_or("installer.jar").to_string();
    let installer = files.root.join("installers").join(file_name);
    if !installer.is_file() {
        fetch(client, url, &installer, server, concurrency, report).await?;
    }
    report(LaunchUpdate::Installing { loader: server.loader.display_name().into() });
    launch::installer::run_server(&runtime.java, &installer, dir).await?;
    record_set_up(dir, &format!("{} {} {loader}", server.loader.as_str(), server.game_version))?;
    forge_start(dir, server.loader, &server.game_version, loader).ok_or_else(|| LaunchError::Installer("it finished, but the files that start the server are missing".into()))
}

/// How an installed Forge or NeoForge server starts: from 1.17 an argument file the installer
/// wrote (with the player's own `user_jvm_args.txt` first), before that a jar named after the build.
fn forge_start(dir: &Path, loader: Loader, game: &str, loader_version: &str) -> Option<Vec<String>> {
    let args_file = if cfg!(windows) { "win_args.txt" } else { "unix_args.txt" };
    let folder = match loader {
        Loader::Forge => format!("libraries/net/minecraftforge/forge/{game}-{loader_version}"),
        Loader::NeoForge => format!("libraries/net/neoforged/neoforge/{loader_version}"),
        _ => return None,
    };
    let relative = format!("{folder}/{args_file}");
    if dir.join(&relative).is_file() {
        let mut args = Vec::new();
        if dir.join("user_jvm_args.txt").is_file() {
            args.push("@user_jvm_args.txt".to_string());
        }
        args.push(format!("@{relative}"));
        return Some(args);
    }
    if loader == Loader::Forge {
        let names = [format!("forge-{game}-{loader_version}.jar"), format!("forge-{game}-{loader_version}-universal.jar"), format!("forge-{game}-{loader_version}-shim.jar")];
        if let Some(name) = names.into_iter().find(|name| dir.join(name).is_file()) {
            return Some(vec!["-jar".into(), name]);
        }
    }
    None
}

/// Runs `java -jar <jar> <args>` to the end, with the last lines of its output if it fails.
async fn run_jar(java: &Path, jar: &Path, args: &[&str], dir: &Path) -> Result<(), LaunchError> {
    let mut command = tokio::process::Command::new(java);
    command.arg("-jar").arg(jar).args(args).current_dir(dir).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let output = command.output().await.map_err(LaunchError::Spawn)?;
    if output.status.success() {
        return Ok(());
    }
    let text = format!("{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let tail: Vec<&str> = text.lines().filter(|line| !line.trim().is_empty()).rev().take(6).collect();
    Err(LaunchError::Installer(tail.into_iter().rev().collect::<Vec<_>>().join(" · ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_how_forge_and_neoforge_servers_start() {
        let dir = std::env::temp_dir().join(format!("wisp-forge-start-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let args = if cfg!(windows) { "win_args.txt" } else { "unix_args.txt" };
        assert_eq!(forge_start(&dir, Loader::Forge, "26.3", "66.1.8"), None);
        let modern = dir.join("libraries/net/minecraftforge/forge/26.3-66.1.8");
        std::fs::create_dir_all(&modern).unwrap();
        std::fs::write(modern.join(args), "-cp x").unwrap();
        assert_eq!(forge_start(&dir, Loader::Forge, "26.3", "66.1.8").unwrap(), vec![format!("@libraries/net/minecraftforge/forge/26.3-66.1.8/{args}")]);
        std::fs::write(dir.join("user_jvm_args.txt"), "# yours").unwrap();
        assert_eq!(forge_start(&dir, Loader::Forge, "26.3", "66.1.8").unwrap()[0], "@user_jvm_args.txt");
        // Another build isn't this one.
        assert_eq!(forge_start(&dir, Loader::Forge, "26.3", "66.1.9"), None);
        let neo = dir.join("libraries/net/neoforged/neoforge/26.3.0.103");
        std::fs::create_dir_all(&neo).unwrap();
        std::fs::write(neo.join(args), "-cp y").unwrap();
        assert!(forge_start(&dir, Loader::NeoForge, "26.3", "26.3.0.103").unwrap().last().unwrap().ends_with(&format!("neoforge/26.3.0.103/{args}")));
        // Old Forge: a jar named after the build.
        std::fs::write(dir.join("forge-1.12.2-14.23.5.2860.jar"), "jar").unwrap();
        assert_eq!(forge_start(&dir, Loader::Forge, "1.12.2", "14.23.5.2860").unwrap(), vec!["-jar".to_string(), "forge-1.12.2-14.23.5.2860.jar".to_string()]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
