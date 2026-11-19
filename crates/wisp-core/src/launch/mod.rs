//! From "Play" to a running game: work out everything a version needs, download what's missing
//! (game, libraries, assets and Java) as one batch with one progress bar, then start Minecraft and
//! watch it until it closes. Game files are shared, so a second instance of a version downloads
//! nothing.

pub mod class_cache;
pub(crate) mod download;
pub(crate) mod installer;
pub(crate) mod java;
pub mod macos;
mod version;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::oneshot;

pub(crate) use download::hex;
pub use download::{Download, DownloadError, Progress, download_all, missing};
pub use java::{
    InstalledRuntime, JavaProbe, installed_runtimes, major_version, probe as probe_java,
    remove_runtime,
};
use version::{AssetIndexRef, ResolvedLibraries, substitute};
pub use version::{Environment, VersionJson};

use crate::HttpClient;
use crate::accounts::{Account, AccountKind};
use crate::instances::{Instance, Loader};
use crate::url_encode;

const VERSION_MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const FABRIC_META: &str = "https://meta.fabricmc.net/v2/versions/loader";
const QUILT_META: &str = "https://meta.quiltmc.org/v3/versions/loader";
const RESOURCES: &str = "https://resources.download.minecraft.net";
/// How many lines of the game's output to keep for a crash report.
const LOG_TAIL: usize = 400;
/// authlib-injector, the Java agent that sends the game's skin questions to Wisp's skin server
/// for offline accounts. Pinned to this build (file, address, SHA-1, size), checked on download;
/// its own site and GitHub release carry the same file.
const AUTHLIB_INJECTOR: (&str, &str, &str, u64) = (
    "authlib-injector-1.2.8.jar",
    "https://authlib-injector.yushi.moe/artifact/56/authlib-injector-1.2.8.jar",
    "0e0e66d8a4f91a26f33b9c09f5cdffce4a11f0b8",
    349_681,
);
/// The garbage collector settings Mojang's own launcher starts the game with.
const GC_FLAGS: [&str; 6] = [
    "-XX:+UnlockExperimentalVMOptions",
    "-XX:+UseG1GC",
    "-XX:G1NewSizePercent=20",
    "-XX:G1ReservePercent=20",
    "-XX:MaxGCPauseMillis=50",
    "-XX:G1HeapRegionSize=32M",
];
/// Lines that mean the game window is up.
const WINDOW_UP_MARKERS: [&str; 3] = [
    "Backend library",
    "LWJGL version",
    "Reloading ResourceManager",
];

/// Where the shared game files live (under Wisp's data folder).
pub struct GameFiles {
    pub(crate) root: PathBuf,
}

impl GameFiles {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    pub(crate) fn versions(&self) -> PathBuf {
        self.root.join("versions")
    }
    fn libraries(&self) -> PathBuf {
        self.root.join("libraries")
    }
    fn assets(&self) -> PathBuf {
        self.root.join("assets")
    }
    fn runtimes(&self) -> PathBuf {
        self.root.join("runtimes")
    }
    fn natives(&self, version_id: &str) -> PathBuf {
        self.root.join("natives").join(version_id)
    }
}

/// What the launcher is doing; the UI shows these as they arrive.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "phase",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum LaunchUpdate {
    /// Reading version files and working out what's missing.
    Checking,
    Downloading {
        done_bytes: u64,
        total_bytes: u64,
        done_files: u64,
        total_files: u64,
    },
    /// Running Forge's or NeoForge's installer (first launch of that loader version only).
    Installing {
        loader: String,
    },
    /// Wisp found something that would stop the game, or just did, and is fixing it.
    Fixing {
        message: String,
    },
    /// Java has started; the game window isn't up yet.
    Starting,
    Running {
        started_at: i64,
    },
    /// The game closed and Wisp is taking a restore point of the worlds that were played.
    Saving {
        /// The world's name.
        world: String,
    },
    /// The game closed. `crashed` is false when the player quit (or pressed Stop).
    Exited {
        code: Option<i32>,
        crashed: bool,
        log_tail: Vec<String>,
        /// What Wisp worked out (and fixed) about a crash.
        #[serde(skip_serializing_if = "Option::is_none")]
        help: Option<crate::doctor::Report>,
    },
    Failed {
        message: String,
        /// Something the launcher can do about it: "install-rosetta".
        #[serde(skip_serializing_if = "Option::is_none")]
        fix: Option<&'static str>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("Couldn't reach the download servers. Check your connection and try again. ({0})")]
    Network(String),
    #[error("Minecraft {0} isn't in Mojang's version list.")]
    UnknownVersion(String),
    #[error("{loader} {version} isn't available for Minecraft {game}.")]
    NoLoader {
        loader: &'static str,
        version: String,
        game: String,
    },
    #[error("The mod loader's installer failed: {0}")]
    Installer(String),
    #[error("No Java for this version: {0}.")]
    NoJava(String),
    #[error("This version's files are incomplete: {0}.")]
    BadVersion(String),
    #[error(transparent)]
    Download(#[from] DownloadError),
    #[error("Couldn't prepare the game files: {0}")]
    Io(#[from] std::io::Error),
    #[error("Couldn't start Java: {0}")]
    Spawn(std::io::Error),
    #[error("Minecraft {game} needs Rosetta, Apple's translator for Intel apps: {reason}")]
    NeedsRosetta { game: String, reason: &'static str },
    #[error("The pre-launch command failed: {0}")]
    Hook(String),
}

impl LaunchError {
    /// What the launcher can offer to do about this error, if anything.
    pub fn fix(&self) -> Option<&'static str> {
        matches!(self, Self::NeedsRosetta { .. }).then_some("install-rosetta")
    }
}

/// The game window's size at launch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WindowSize {
    #[default]
    Default,
    Custom {
        width: u32,
        height: u32,
    },
    Fullscreen,
}

/// Commands to run around the game. The pre-launch and post-exit ones go through the shell.
#[derive(Debug, Clone, Default)]
pub struct Hooks {
    pub pre_launch: Option<String>,
    /// A program (and its arguments) to start Java through.
    pub wrapper: Vec<String>,
    pub post_exit: Option<String>,
}

pub struct LaunchOptions {
    pub instance: Instance,
    /// The instance's own game folder: worlds, options, mods, logs.
    pub game_dir: PathBuf,
    pub account: Account,
    pub memory_mb: u32,
    pub concurrency: usize,
    pub launcher_version: String,
    /// Extra JVM arguments, already split into words.
    pub jvm_args: Vec<String>,
    pub window: WindowSize,
    /// Java executables to use instead of Wisp's own, by Java major version.
    pub java_paths: HashMap<u32, PathBuf>,
    pub hooks: Hooks,
    /// A world or server to go straight into.
    pub quick_play: Option<QuickPlay>,
    /// For an offline account: Wisp's skin server, which the game asks for skins through
    /// authlib-injector (see `skin_server`).
    pub skin_server: Option<crate::skin_server::SkinServer>,
}

/// Where the game goes as soon as it opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickPlay {
    /// A world, by its folder in saves/ (Minecraft 1.20 and later).
    World(String),
    /// A server, by address (1.20 and later by Quick Play; older versions by --server).
    Server(String),
}

/// A version with every file in place, ready to start.
#[derive(Debug)]
pub struct Prepared {
    java: PathBuf,
    jvm_args: Vec<String>,
    main_class: String,
    game_args: Vec<String>,
    game_dir: PathBuf,
    hooks: Hooks,
    /// Variables the hooks get, named as Prism Launcher names them ($INST_NAME, $INST_JAVA, ...).
    hook_env: Vec<(&'static str, String)>,
    /// Where the game goes as soon as it opens, when one was asked for and the version can.
    quick_play: Option<QuickPlay>,
}

impl Prepared {
    /// Where the game will go as soon as it opens, if anywhere.
    pub fn quick_play(&self) -> Option<&QuickPlay> {
        self.quick_play.as_ref()
    }
}

/// How a game session ended.
#[derive(Debug, Clone)]
pub struct Exit {
    pub code: Option<i32>,
    pub crashed: bool,
    /// The last lines, for showing.
    pub log_tail: Vec<String>,
    /// Everything kept of the output (the last few hundred lines), for working out what went wrong.
    pub output: Vec<String>,
    /// Whether the game got as far as its window.
    pub window_seen: bool,
}

/// Downloads whatever the instance still needs and works out the command that starts it.
pub async fn prepare(
    client: &HttpClient,
    files: &GameFiles,
    options: &LaunchOptions,
    report: &(dyn Fn(LaunchUpdate) + Send + Sync),
) -> Result<Prepared, LaunchError> {
    report(LaunchUpdate::Checking);
    let env = Environment::current();
    let instance = &options.instance;
    let mut vanilla = vanilla_version(client, files, &instance.game_version).await?;
    let mut component = vanilla
        .java_version
        .as_ref()
        .map_or("jre-legacy", |java| java.component.as_str())
        .to_string();
    let mut needs_intel = false;
    if env.os == "osx" {
        macos::fix_jna(&mut vanilla.libraries);
        let has_arm64_natives =
            ResolvedLibraries::resolve(&vanilla.libraries, &files.libraries(), &env)
                .has_arm64_natives;
        if env.arch == "arm64" && !has_arm64_natives {
            // Forge before 1.17 needs Java 8, which Mojang only builds for Intel Macs.
            let forge_on_java_8 = instance.loader == Loader::Forge && component == "jre-legacy";
            let swappable = macos::runs_on_swapped_natives(vanilla.release_time.as_deref());
            match macos::apple_silicon_libraries(&vanilla.libraries) {
                Some(libraries) if swappable && !forge_on_java_8 => {
                    vanilla.libraries = libraries;
                    component = macos::apple_silicon_java(&component).to_string();
                }
                _ if !macos::rosetta_installed().await => {
                    return Err(LaunchError::NeedsRosetta {
                        game: instance.game_version.clone(),
                        reason: if forge_on_java_8 {
                            "Forge for this version needs Java 8, which only comes as an Intel app."
                        } else {
                            "versions before 1.15 only run as Intel apps on this Mac."
                        },
                    });
                }
                _ => needs_intel = true,
            }
        }
    }
    // A mod can need a newer Java than Minecraft asks for (Wisp's fixes set this).
    if let Some(wanted) = instance.java_major
        && java::major_version(&component).is_some_and(|current| wanted > current)
        && let Some(newer) = java::component_for(wanted)
    {
        component = newer.to_string();
    }
    let platforms = java_platforms(needs_intel)?;
    let java_major = java::major_version(&component)
        .or_else(|| vanilla.java_version.as_ref().map(|java| java.major_version));
    let custom_java = java_major
        .and_then(|major| custom_java(options, major))
        .transpose()?;

    let mut java_ready = None;
    let version = match instance.loader {
        Loader::Vanilla => vanilla,
        Loader::Fabric | Loader::Quilt => {
            let loader_version = loader_version(instance)?;
            meta_profile(
                client,
                files,
                instance.loader,
                &instance.game_version,
                loader_version,
            )
            .await?
            .inherit(vanilla)
        }
        Loader::Forge | Loader::NeoForge => {
            // Their installer runs on Java, so it has to be in place first.
            let java = match &custom_java {
                Some(java) => java.clone(),
                None => {
                    ensure_java(
                        client,
                        files,
                        &component,
                        &platforms,
                        options.concurrency,
                        report,
                    )
                    .await?
                }
            };
            let profile =
                installed_profile(client, files, instance, &java, options.concurrency, report)
                    .await?;
            java_ready = Some(java);
            profile.inherit(vanilla)
        }
    };
    let libraries = ResolvedLibraries::resolve(&version.libraries, &files.libraries(), &env);

    let mut downloads = Vec::new();
    let client_jar = files
        .versions()
        .join(&instance.game_version)
        .join(format!("{}.jar", instance.game_version));
    let client_artifact = version
        .downloads
        .as_ref()
        .map(|downloads| &downloads.client)
        .ok_or_else(|| LaunchError::BadVersion("no game download listed".into()))?;
    downloads.push(Download {
        url: client_artifact.url.clone(),
        path: client_jar.clone(),
        sha1: client_artifact.sha1.clone(),
        size: client_artifact.size,
        executable: false,
    });
    downloads.extend(libraries.downloads.iter().cloned());

    let asset_ref = version
        .asset_index
        .clone()
        .ok_or_else(|| LaunchError::BadVersion("no asset index listed".into()))?;
    let assets = AssetIndex::load(client, &files.assets(), &asset_ref).await?;
    downloads.extend(assets.downloads(&files.assets()));
    let injector = files.root.join("authlib-injector").join(AUTHLIB_INJECTOR.0);
    if options.skin_server.is_some() {
        downloads.push(Download {
            url: AUTHLIB_INJECTOR.1.to_string(),
            path: injector.clone(),
            sha1: Some(AUTHLIB_INJECTOR.2.to_string()),
            size: Some(AUTHLIB_INJECTOR.3),
            executable: false,
        });
    }

    let (java_binary, runtime_plan) = match java_ready.or(custom_java) {
        Some(java) => (java, None),
        None => plan_java(client, files, &component, &platforms).await?,
    };
    if let Some(plan) = &runtime_plan {
        downloads.extend(plan.downloads.iter().cloned());
    }

    download_with_progress(
        client,
        download::missing(downloads),
        options.concurrency,
        report,
    )
    .await?;
    if let Some(plan) = &runtime_plan {
        plan.finish()?;
    }

    // Loader profiles run from a copy of the game jar named after themselves (Forge's module
    // setup expects `<version id>.jar`); a hard link costs no space.
    let classpath_jar = if version.id == instance.game_version {
        client_jar.clone()
    } else {
        let linked = files
            .versions()
            .join(&version.id)
            .join(format!("{}.jar", version.id));
        if !linked.exists() {
            if let Some(parent) = linked.parent() {
                std::fs::create_dir_all(parent)?;
            }
            if std::fs::hard_link(&client_jar, &linked).is_err() {
                std::fs::copy(&client_jar, &linked)?;
            }
        }
        linked
    };

    // Natives: newer versions unpack their own (into these folders); older ones need it done here.
    let natives_dir = files.natives(&version.id);
    for folder in ["java", "jna", "lwjgl", "netty"] {
        std::fs::create_dir_all(natives_dir.join(folder))?;
    }
    if !libraries.natives.is_empty() {
        let jars = libraries.natives.clone();
        let target = natives_dir.clone();
        tokio::task::spawn_blocking(move || extract_natives(&jars, &target))
            .await
            .map_err(std::io::Error::other)??;
    }
    std::fs::create_dir_all(&options.game_dir)?;
    let legacy_assets = assets.lay_out_legacy(&files.assets(), &asset_ref.id, &options.game_dir)?;

    let mut classpath = libraries.classpath;
    classpath.push(classpath_jar);
    let separator = if cfg!(windows) { ";" } else { ":" };
    let path = |path: &Path| path.to_string_lossy().into_owned();
    let account = &options.account;
    let token = match account.kind {
        AccountKind::Offline => "0".to_string(),
    };
    let values: HashMap<&str, String> = HashMap::from([
        ("auth_player_name", account.name.clone()),
        ("version_name", version.id.clone()),
        ("game_directory", path(&options.game_dir)),
        ("assets_root", path(&files.assets())),
        (
            "game_assets",
            path(legacy_assets.as_deref().unwrap_or(&files.assets())),
        ),
        ("assets_index_name", asset_ref.id.clone()),
        ("auth_uuid", account.uuid.clone()),
        ("auth_access_token", token.clone()),
        ("auth_session", format!("token:{token}:{}", account.uuid)),
        ("clientid", "0".into()),
        ("auth_xuid", "0".into()),
        ("user_type", "legacy".into()),
        ("user_properties", "{}".into()),
        (
            "version_type",
            version.kind.clone().unwrap_or_else(|| "release".into()),
        ),
        ("natives_directory", path(&natives_dir)),
        ("launcher_name", "wisp".into()),
        ("launcher_version", options.launcher_version.clone()),
        (
            "classpath",
            classpath
                .iter()
                .map(|entry| path(entry))
                .collect::<Vec<_>>()
                .join(separator),
        ),
        ("classpath_separator", separator.into()),
        ("library_directory", path(&files.libraries())),
    ]);

    let (game_raw, jvm_raw) = version.raw_arguments(&env);
    let mut jvm_args = vec![format!("-Xmx{}M", options.memory_mb)];
    // The official launcher's garbage collector settings: short, even pauses, so frames don't hitch.
    jvm_args.extend(GC_FLAGS.iter().map(|flag| flag.to_string()));
    // Java's own library, already parsed and checked, mapped straight in (see class_cache).
    class_cache::ensure_base_archive(&java_binary, &files.runtimes()).await;
    class_cache::forget(&options.game_dir);
    // The player's own come after Wisp's defaults, so theirs win (Java takes the last -Xmx).
    // Skins for offline accounts, from Wisp's skin server.
    if let Some(server) = &options.skin_server {
        jvm_args.extend(server.jvm_args(&injector));
    }
    jvm_args.extend(options.jvm_args.iter().cloned());
    jvm_args.extend(jvm_raw.iter().map(|argument| substitute(argument, &values)));
    let mut game_args: Vec<String> = game_raw
        .iter()
        .map(|argument| substitute(argument, &values))
        .collect();
    // Versions list --width/--height under a feature Wisp leaves off, so they're added here.
    match options.window {
        WindowSize::Default => {}
        WindowSize::Custom { width, height } => game_args.extend([
            "--width".into(),
            width.to_string(),
            "--height".into(),
            height.to_string(),
        ]),
        WindowSize::Fullscreen => game_args.push("--fullscreen".into()),
    }
    // Quick Play arguments sit behind a feature Wisp leaves off; they're added here when asked.
    // Before 1.20 there's no Quick Play: servers still work through --server and --port.
    let quick_play = match options.quick_play.clone() {
        Some(QuickPlay::World(world)) if version.supports_quick_play() => {
            game_args.extend(["--quickPlaySingleplayer".into(), world.clone()]);
            Some(QuickPlay::World(world))
        }
        Some(QuickPlay::Server(address)) if version.supports_quick_play() => {
            game_args.extend(["--quickPlayMultiplayer".into(), address.clone()]);
            Some(QuickPlay::Server(address))
        }
        Some(QuickPlay::Server(address)) => match crate::servers::ServerAddress::parse(&address) {
            Ok(parsed) => {
                game_args.extend(["--server".into(), parsed.host.clone()]);
                game_args.extend(["--port".into(), parsed.port.unwrap_or(crate::servers::DEFAULT_PORT).to_string()]);
                Some(QuickPlay::Server(address))
            }
            Err(_) => None,
        },
        _ => None,
    };
    let main_class = version
        .main_class
        .clone()
        .ok_or_else(|| LaunchError::BadVersion("no main class".into()))?;

    let hook_env = vec![
        ("INST_NAME", instance.name.clone()),
        ("INST_ID", instance.id.clone()),
        (
            "INST_DIR",
            path(options.game_dir.parent().unwrap_or(&options.game_dir)),
        ),
        ("INST_MC_DIR", path(&options.game_dir)),
        ("INST_JAVA", path(&java_binary)),
        ("INST_MC_VERSION", instance.game_version.clone()),
        ("INST_LOADER", instance.loader.as_str().to_string()),
    ];
    Ok(Prepared {
        java: java_binary,
        jvm_args,
        main_class,
        game_args,
        game_dir: options.game_dir.clone(),
        hooks: options.hooks.clone(),
        hook_env,
        quick_play,
    })
}

/// authlib-injector (see `AUTHLIB_INJECTOR`), downloaded and checked when missing: for a hosted
/// server whose players sign in through Wisp's skin server (builds with offline accounts).
pub async fn authlib_injector(client: &HttpClient, files: &GameFiles, concurrency: usize, report: &(dyn Fn(LaunchUpdate) + Send + Sync)) -> Result<PathBuf, LaunchError> {
    let path = files.root.join("authlib-injector").join(AUTHLIB_INJECTOR.0);
    let download = Download { url: AUTHLIB_INJECTOR.1.to_string(), path: path.clone(), sha1: Some(AUTHLIB_INJECTOR.2.to_string()), size: Some(AUTHLIB_INJECTOR.3), executable: false };
    download_with_progress(client, download::missing([download]), concurrency, report).await?;
    Ok(path)
}

/// What a hosted server runs on: Java, and Mojang's server jar for its version.
#[derive(Debug, Clone)]
pub struct ServerRuntime {
    pub java: PathBuf,
    pub server_jar: PathBuf,
    /// Java's major version (8, 17, 21, 25), when known.
    pub java_major: Option<u32>,
}

/// Java and Mojang's server jar for a version, downloaded when missing, for hosting a server.
/// A server has no natives, so on Apple Silicon it runs on Apple's own Java, except Forge before
/// 1.17, which needs Java 8 (Intel only, so Rosetta).
#[allow(clippy::too_many_arguments)]
pub async fn server_runtime(
    client: &HttpClient,
    files: &GameFiles,
    game_version: &str,
    loader: Loader,
    java_major: Option<u32>,
    java_paths: &HashMap<u32, PathBuf>,
    concurrency: usize,
    report: &(dyn Fn(LaunchUpdate) + Send + Sync),
) -> Result<ServerRuntime, LaunchError> {
    report(LaunchUpdate::Checking);
    let vanilla = vanilla_version(client, files, game_version).await?;
    let artifact = vanilla
        .downloads
        .as_ref()
        .and_then(|downloads| downloads.server.clone())
        .ok_or_else(|| LaunchError::BadVersion(format!("Mojang has no server for Minecraft {game_version}")))?;
    let mut component = vanilla
        .java_version
        .as_ref()
        .map_or("jre-legacy", |java| java.component.as_str())
        .to_string();
    if let Some(wanted) = java_major
        && java::major_version(&component).is_some_and(|current| wanted > current)
        && let Some(newer) = java::component_for(wanted)
    {
        component = newer.to_string();
    }
    let env = Environment::current();
    let mut needs_intel = false;
    if env.os == "osx" && env.arch == "arm64" && macos::apple_silicon_java(&component) != component {
        if loader == Loader::Forge {
            if !macos::rosetta_installed().await {
                return Err(LaunchError::NeedsRosetta {
                    game: game_version.into(),
                    reason: "Forge for this version needs Java 8, which only comes as an Intel app.",
                });
            }
            needs_intel = true;
        } else {
            component = macos::apple_silicon_java(&component).to_string();
        }
    }
    let platforms = java_platforms(needs_intel)?;
    let major = java::major_version(&component).or_else(|| vanilla.java_version.as_ref().map(|java| java.major_version));
    // The Java picked in Settings for this major version wins, as it does for the game.
    let custom = major
        .and_then(|major| java_paths.get(&major).or_else(|| (major == 16).then(|| java_paths.get(&17)).flatten()))
        .filter(|path| path.is_file())
        .cloned();
    let (java, plan) = match custom {
        Some(java) => (java, None),
        None => plan_java(client, files, &component, &platforms).await?,
    };
    let server_jar = files.versions().join(game_version).join(format!("{game_version}-server.jar"));
    let mut downloads = vec![Download {
        url: artifact.url.clone(),
        path: server_jar.clone(),
        sha1: artifact.sha1.clone(),
        size: artifact.size,
        executable: false,
    }];
    if let Some(plan) = &plan {
        downloads.extend(plan.downloads.iter().cloned());
    }
    download_with_progress(client, download::missing(downloads), concurrency, report).await?;
    if let Some(plan) = &plan {
        plan.finish()?;
    }
    Ok(ServerRuntime { java, server_jar, java_major: major })
}

/// The Java picked in settings for this major version (Java 16 runs on the Java 17 one).
fn custom_java(options: &LaunchOptions, major: u32) -> Option<Result<PathBuf, LaunchError>> {
    let path = options
        .java_paths
        .get(&major)
        .or_else(|| (major == 16).then(|| options.java_paths.get(&17)).flatten())?;
    Some(if path.is_file() {
        Ok(path.clone())
    } else {
        Err(LaunchError::NoJava(format!(
            "the Java {major} chosen in Settings isn't at {} anymore",
            path.display()
        )))
    })
}

/// Splits typed arguments into words the way a shell would: spaces separate, quotes group, and
/// a backslash escapes the next character.
pub fn split_args(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(open), c) if c == open => quote = None,
            (Some('"') | None, '\\') => {
                if let Some(next) = chars.next() {
                    word.push(next);
                }
                in_word = true;
            }
            (Some(_), c) => word.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                in_word = true;
            }
            (None, c) if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            (None, c) => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        words.push(word);
    }
    words
}

/// A hook command, run through the shell in the game folder with the $INST_* variables.
fn hook_command(line: &str, prepared: &Prepared) -> tokio::process::Command {
    let mut command = if cfg!(windows) {
        let mut command = tokio::process::Command::new("cmd");
        command.args(["/C", line]);
        command
    } else {
        let mut command = tokio::process::Command::new("/bin/sh");
        command.args(["-c", line]);
        command
    };
    command
        .envs(prepared.hook_env.iter().map(|(key, value)| (*key, value)))
        .current_dir(&prepared.game_dir)
        .stdin(Stdio::null());
    command
}

fn loader_version(instance: &Instance) -> Result<&str, LaunchError> {
    instance
        .loader_version
        .as_deref()
        .ok_or_else(|| LaunchError::BadVersion("the instance has no loader version".into()))
}

/// Mojang's runtime platforms to try for this computer, best first.
fn java_platforms(needs_intel: bool) -> Result<Vec<&'static str>, LaunchError> {
    let first = java::platform(needs_intel)
        .ok_or_else(|| LaunchError::NoJava("this operating system".into()))?;
    Ok(if first == "mac-os-arm64" {
        vec![first, "mac-os"]
    } else {
        vec![first]
    })
}

/// The installed Java for `component`, or a plan to install it.
async fn plan_java(
    client: &HttpClient,
    files: &GameFiles,
    component: &str,
    platforms: &[&'static str],
) -> Result<(PathBuf, Option<java::RuntimePlan>), LaunchError> {
    if let Some(java) = platforms
        .iter()
        .find_map(|platform| java::installed(&files.runtimes(), component, platform))
    {
        return Ok((java, None));
    }
    let mut last_error = None;
    for platform in platforms {
        match java::plan(client, &files.runtimes(), component, platform).await {
            Ok(plan) => return Ok((plan.java.clone(), Some(plan))),
            Err(error @ LaunchError::NoJava(_)) => last_error = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| LaunchError::NoJava(component.into())))
}

/// Installs Java now (rather than with the game files), for loaders whose installer needs it.
async fn ensure_java(
    client: &HttpClient,
    files: &GameFiles,
    component: &str,
    platforms: &[&'static str],
    concurrency: usize,
    report: &(dyn Fn(LaunchUpdate) + Send + Sync),
) -> Result<PathBuf, LaunchError> {
    let (java, plan) = plan_java(client, files, component, platforms).await?;
    if let Some(plan) = plan {
        download_with_progress(
            client,
            download::missing(plan.downloads.iter().cloned()),
            concurrency,
            report,
        )
        .await?;
        plan.finish()?;
    }
    Ok(java)
}

/// Downloads a batch, sending progress to `report` while it runs.
pub(crate) async fn download_with_progress(
    client: &HttpClient,
    items: Vec<Download>,
    concurrency: usize,
    report: &(dyn Fn(LaunchUpdate) + Send + Sync),
) -> Result<(), LaunchError> {
    if items.is_empty() {
        return Ok(());
    }
    let progress = Arc::new(Progress::default());
    let downloading = download::download_all(client, items, concurrency, &progress);
    tokio::pin!(downloading);
    let mut tick = tokio::time::interval(Duration::from_millis(150));
    let result = loop {
        tokio::select! {
            result = &mut downloading => break result,
            _ = tick.tick() => report(snapshot(&progress)),
        }
    };
    report(snapshot(&progress));
    Ok(result?)
}

/// Forge's or NeoForge's profile, running its installer the first time a build is used.
async fn installed_profile(
    client: &HttpClient,
    files: &GameFiles,
    instance: &Instance,
    java: &Path,
    concurrency: usize,
    report: &(dyn Fn(LaunchUpdate) + Send + Sync),
) -> Result<VersionJson, LaunchError> {
    let loader_version = loader_version(instance)?;
    let url = installer::installer_url(instance.loader, &instance.game_version, loader_version)
        .ok_or_else(|| LaunchError::BadVersion("this loader has no installer".into()))?;
    let file_name = url
        .rsplit('/')
        .next()
        .unwrap_or("installer.jar")
        .to_string();
    let installer_path = files.root.join("installers").join(file_name);
    if !installer_path.is_file() {
        let download = Download {
            url,
            path: installer_path.clone(),
            sha1: None,
            size: None,
            executable: false,
        };
        download_with_progress(client, vec![download], concurrency, report)
            .await
            .map_err(|error| match error {
                LaunchError::Download(DownloadError::Status { status: 404, .. }) => {
                    LaunchError::NoLoader {
                        loader: instance.loader.display_name(),
                        version: loader_version.into(),
                        game: instance.game_version.clone(),
                    }
                }
                other => other,
            })?;
    }
    let id = installer::profile_version_id(&installer_path)?;
    let profile_path = files.versions().join(&id).join(format!("{id}.json"));
    if read_cached(&profile_path).is_none() {
        report(LaunchUpdate::Installing {
            loader: instance.loader.display_name().into(),
        });
        installer::run(java, &installer_path, &files.root).await?;
    }
    read_cached(&profile_path).ok_or_else(|| {
        LaunchError::Installer("it finished, but the version it installs is missing".into())
    })
}

fn snapshot(progress: &Progress) -> LaunchUpdate {
    let done_bytes = progress.done_bytes.load(Ordering::Relaxed);
    LaunchUpdate::Downloading {
        done_bytes,
        // A few files (some of Fabric's) don't state their size up front.
        total_bytes: progress.total_bytes.load(Ordering::Relaxed).max(done_bytes),
        done_files: progress.done_files.load(Ordering::Relaxed),
        total_files: progress.total_files.load(Ordering::Relaxed),
    }
}

/// Starts the game and waits for it to close. Sending on `stop` ends it early.
pub async fn run(
    prepared: Prepared,
    report: &(dyn Fn(LaunchUpdate) + Send + Sync),
    stop: oneshot::Receiver<()>,
    now_ms: fn() -> i64,
) -> Result<Exit, LaunchError> {
    let mut stop = stop;
    if let Some(line) = &prepared.hooks.pre_launch {
        let output = tokio::select! {
            output = hook_command(line, &prepared).output() => output,
            _ = &mut stop => return Ok(Exit { code: None, crashed: false, log_tail: Vec::new(), output: Vec::new(), window_seen: false }),
        };
        let output = output.map_err(|error| LaunchError::Hook(error.to_string()))?;
        if !output.status.success() {
            let said = String::from_utf8_lossy(&output.stderr);
            let last = said
                .lines()
                .rev()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("");
            let code = output
                .status
                .code()
                .map_or("it was stopped".to_string(), |code| {
                    format!("it exited with code {code}")
                });
            return Err(LaunchError::Hook(if last.is_empty() {
                code
            } else {
                format!("{code}: {}", last.trim())
            }));
        }
    }
    let mut command = match prepared.hooks.wrapper.split_first() {
        Some((program, arguments)) => {
            let mut command = tokio::process::Command::new(program);
            command.args(arguments).arg(&prepared.java);
            command
        }
        None => tokio::process::Command::new(&prepared.java),
    };
    command
        .args(&prepared.jvm_args)
        .arg(&prepared.main_class)
        .args(&prepared.game_args)
        .current_dir(&prepared.game_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console window behind the game
    let launched_at = std::time::SystemTime::now();
    let mut child = command.spawn().map_err(LaunchError::Spawn)?;
    report(LaunchUpdate::Starting);

    let tail = Arc::new(Mutex::new(VecDeque::with_capacity(LOG_TAIL)));
    let (window_tx, mut window_rx) = oneshot::channel();
    let window_tx = Arc::new(Mutex::new(Some(window_tx)));
    let mut readers = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        readers.push(tokio::spawn(collect_lines(
            stdout,
            tail.clone(),
            window_tx.clone(),
        )));
    }
    if let Some(stderr) = child.stderr.take() {
        readers.push(tokio::spawn(collect_lines(
            stderr,
            tail.clone(),
            window_tx.clone(),
        )));
    }
    drop(window_tx);

    // If the log never mentions the window (a modded game, say), assume it's up after a while.
    let fallback = tokio::time::sleep(Duration::from_secs(40));
    tokio::pin!(fallback);
    let (mut window_known, mut window_up, mut stopping) = (false, false, false);
    let status = loop {
        tokio::select! {
            status = child.wait() => break status,
            seen = &mut window_rx, if !window_known => {
                window_known = true;
                if seen.is_ok() {
                    window_up = true;
                    report(LaunchUpdate::Running { started_at: now_ms() });
                }
            }
            _ = &mut fallback, if !window_known => {
                window_known = true;
                window_up = true;
                report(LaunchUpdate::Running { started_at: now_ms() });
            }
            _ = &mut stop, if !stopping => {
                stopping = true;
                let _ = child.start_kill();
            }
        }
    }
    .map_err(LaunchError::Spawn)?;

    for reader in readers {
        let _ = tokio::time::timeout(Duration::from_secs(2), reader).await;
    }
    let lines: Vec<String> = tail
        .lock()
        .map(|tail| tail.iter().cloned().collect())
        .unwrap_or_default();
    let code = status.code();
    // Quitting (the Quit button, Alt+F4, the window's close button) has Minecraft say "Stopping!"
    // on its way out. Some computers report a failing exit code after that anyway (a graphics
    // driver tripping over the closing window, say), but the game had saved and was leaving: a
    // quit, not a crash. A crash report on the way out still counts.
    let crashed = !stopping && code != Some(0) && !quit_on_purpose(&lines, &prepared.game_dir, launched_at);
    // Keep the end of the output beside Minecraft's own logs, for when something goes wrong.
    let logs = prepared.game_dir.join("logs");
    if std::fs::create_dir_all(&logs).is_ok() {
        let ending = match code {
            Some(code) => format!("[Wisp] Java exited with code {code}"),
            None => "[Wisp] Java was stopped from outside".to_string(),
        };
        let _ = std::fs::write(logs.join("wisp-last-run.log"), format!("{}\n{ending}\n", lines.join("\n")));
    }
    if let Some(line) = &prepared.hooks.post_exit {
        // Left running on its own: a slow post-exit command shouldn't hold up the launcher.
        let _ = hook_command(line, &prepared).spawn();
    }
    Ok(Exit {
        code,
        crashed,
        log_tail: lines.iter().rev().take(40).rev().cloned().collect(),
        output: lines,
        window_seen: window_up,
    })
}

/// What Minecraft prints when it crashes: the report, and where it saved it (or couldn't).
const CRASH_MARKERS: [&str; 3] = ["---- Minecraft Crash Report ----", "#@!@# Game crashed!", "#@?@# Game crashed!"];

/// Whether a game that exited with a failing code had in fact been quit: Minecraft said it was
/// stopping, and no crash report came after. Its own log is the second witness, for when the
/// output came up short.
fn quit_on_purpose(output: &[String], game_dir: &Path, launched_at: std::time::SystemTime) -> bool {
    if let Some(quit) = stopped_cleanly(output.iter().map(String::as_str)) {
        return quit;
    }
    // With a little slack for file systems that keep times to the second or two.
    let written_since = std::fs::metadata(game_dir.join("logs").join("latest.log"))
        .and_then(|meta| meta.modified())
        .is_ok_and(|modified| modified + Duration::from_secs(2) >= launched_at);
    written_since && crate::game_logs::read(game_dir, "logs/latest.log").is_ok_and(|log| stopped_cleanly(log.lines()) == Some(true))
}

/// Reading back from the end: `Some(true)` when the last thing of note is Minecraft saying
/// "Stopping!", `Some(false)` when it's a crash report, `None` when it got to neither. A crash
/// the game came back from (some mods catch them) is followed by a later "Stopping!", so it reads
/// as a quit; Java itself failing after "Stopping!" (in a graphics driver, say) does too.
fn stopped_cleanly<'a>(lines: impl DoubleEndedIterator<Item = &'a str>) -> Option<bool> {
    lines.rev().find_map(|line| {
        let line = line.trim_end();
        if CRASH_MARKERS.iter().any(|marker| line.contains(marker)) {
            Some(false)
        } else if line == "Stopping!" || line.ends_with("]: Stopping!") || line.contains("[CDATA[Stopping!]]") {
            Some(true)
        } else {
            None
        }
    })
}

/// Reads the game's output to the end. It must keep draining even past bytes that aren't UTF-8:
/// if nobody reads the pipe, the game blocks once it's full.
async fn collect_lines(
    stream: impl AsyncRead + Unpin,
    tail: Arc<Mutex<VecDeque<String>>>,
    window: Arc<Mutex<Option<oneshot::Sender<()>>>>,
) {
    let mut reader = BufReader::new(stream);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let line = String::from_utf8_lossy(&buffer).trim_end().to_string();
        if WINDOW_UP_MARKERS.iter().any(|marker| line.contains(marker))
            && let Some(sender) = window.lock().ok().and_then(|mut slot| slot.take())
        {
            let _ = sender.send(());
        }
        if let Ok(mut tail) = tail.lock() {
            if tail.len() == LOG_TAIL {
                tail.pop_front();
            }
            tail.push_back(line);
        }
    }
}

// ─── version files ───────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct Manifest {
    versions: Vec<ManifestEntry>,
}

#[derive(Deserialize)]
struct ManifestEntry {
    id: String,
    url: String,
    sha1: String,
}

fn read_cached(path: &Path) -> Option<VersionJson> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && Path::new(id)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

async fn vanilla_version(
    client: &HttpClient,
    files: &GameFiles,
    id: &str,
) -> Result<VersionJson, LaunchError> {
    if !safe_id(id) {
        return Err(LaunchError::UnknownVersion(id.into()));
    }
    let path = files.versions().join(id).join(format!("{id}.json"));
    if let Some(cached) = read_cached(&path) {
        return Ok(cached);
    }
    let manifest: Manifest = fetch_json(client, VERSION_MANIFEST).await?;
    let entry = manifest
        .versions
        .into_iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| LaunchError::UnknownVersion(id.into()))?;
    let bytes = fetch_bytes(client, &entry.url, Some(&entry.sha1)).await?;
    save(&path, &bytes)?;
    serde_json::from_slice(&bytes).map_err(|error| LaunchError::BadVersion(error.to_string()))
}

/// Fabric's or Quilt's launch profile (a small version JSON on top of vanilla), cached so later
/// launches work offline.
async fn meta_profile(
    client: &HttpClient,
    files: &GameFiles,
    loader_kind: Loader,
    game: &str,
    loader: &str,
) -> Result<VersionJson, LaunchError> {
    let (meta, prefix) = match loader_kind {
        Loader::Quilt => (QUILT_META, "quilt-loader"),
        _ => (FABRIC_META, "fabric-loader"),
    };
    let id = format!("{prefix}-{loader}-{game}");
    if !safe_id(&id) {
        return Err(LaunchError::BadVersion("odd loader version".into()));
    }
    let path = files.versions().join(&id).join(format!("{id}.json"));
    if let Some(cached) = read_cached(&path) {
        return Ok(cached);
    }
    let url = format!(
        "{meta}/{}/{}/profile/json",
        url_encode(game),
        url_encode(loader)
    );
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| LaunchError::Network(error.to_string()))?;
    if response.status().is_client_error() {
        return Err(LaunchError::NoLoader {
            loader: loader_kind.display_name(),
            version: loader.into(),
            game: game.into(),
        });
    }
    let bytes = response
        .error_for_status()
        .map_err(|error| LaunchError::Network(error.to_string()))?
        .bytes()
        .await
        .map_err(|error| LaunchError::Network(error.to_string()))?;
    save(&path, &bytes)?;
    serde_json::from_slice(&bytes).map_err(|error| LaunchError::BadVersion(error.to_string()))
}

pub(crate) async fn fetch_json<T: serde::de::DeserializeOwned>(
    client: &HttpClient,
    url: &str,
) -> Result<T, LaunchError> {
    let bytes = fetch_bytes(client, url, None).await?;
    serde_json::from_slice(&bytes).map_err(|error| LaunchError::BadVersion(error.to_string()))
}

async fn fetch_bytes(
    client: &HttpClient,
    url: &str,
    sha1: Option<&str>,
) -> Result<Vec<u8>, LaunchError> {
    let network = |error: reqwest::Error| LaunchError::Network(error.to_string());
    let bytes = client
        .get(url)
        .send()
        .await
        .map_err(network)?
        .error_for_status()
        .map_err(network)?
        .bytes()
        .await
        .map_err(network)?;
    if let Some(expected) = sha1
        && !hex(Sha1::digest(&bytes).as_slice()).eq_ignore_ascii_case(expected)
    {
        return Err(LaunchError::Network(format!("{url} arrived damaged")));
    }
    Ok(bytes.to_vec())
}

fn save(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)
}

// ─── assets ──────────────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct AssetIndex {
    objects: HashMap<String, AssetObject>,
    /// Pre-1.7 versions read assets by name from a "virtual" folder...
    #[serde(default, rename = "virtual")]
    is_virtual: bool,
    /// ...or from a `resources` folder inside the game folder.
    #[serde(default)]
    map_to_resources: bool,
}

#[derive(Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

impl AssetIndex {
    async fn load(
        client: &HttpClient,
        assets_dir: &Path,
        reference: &AssetIndexRef,
    ) -> Result<Self, LaunchError> {
        if !safe_id(&reference.id) {
            return Err(LaunchError::BadVersion("odd asset index".into()));
        }
        let path = assets_dir
            .join("indexes")
            .join(format!("{}.json", reference.id));
        let cached = std::fs::read(&path)
            .ok()
            .filter(|bytes| bytes.len() as u64 == reference.size);
        let bytes = match cached {
            Some(bytes) => bytes,
            None => {
                let bytes = fetch_bytes(client, &reference.url, Some(&reference.sha1)).await?;
                save(&path, &bytes)?;
                bytes
            }
        };
        serde_json::from_slice(&bytes).map_err(|error| LaunchError::BadVersion(error.to_string()))
    }

    fn object_path(assets_dir: &Path, hash: &str) -> PathBuf {
        assets_dir.join("objects").join(&hash[..2]).join(hash)
    }

    fn valid_hash(hash: &str) -> bool {
        hash.len() == 40 && hash.chars().all(|c| c.is_ascii_hexdigit())
    }

    fn downloads(&self, assets_dir: &Path) -> Vec<Download> {
        let mut seen = HashSet::new();
        self.objects
            .values()
            .filter(|object| Self::valid_hash(&object.hash) && seen.insert(object.hash.as_str()))
            .map(|object| Download {
                url: format!("{RESOURCES}/{}/{}", &object.hash[..2], object.hash),
                path: Self::object_path(assets_dir, &object.hash),
                sha1: Some(object.hash.clone()),
                size: Some(object.size),
                executable: false,
            })
            .collect()
    }

    /// For very old versions, copies assets to where they expect them, by name.
    fn lay_out_legacy(
        &self,
        assets_dir: &Path,
        id: &str,
        game_dir: &Path,
    ) -> std::io::Result<Option<PathBuf>> {
        if !self.is_virtual && !self.map_to_resources {
            return Ok(None);
        }
        let target = if self.map_to_resources {
            game_dir.join("resources")
        } else {
            assets_dir.join("virtual").join(id)
        };
        for (name, object) in &self.objects {
            let safe = Path::new(name)
                .components()
                .all(|part| matches!(part, Component::Normal(_)));
            if !safe || !Self::valid_hash(&object.hash) {
                continue;
            }
            let destination = target.join(name);
            if destination.exists() {
                continue;
            }
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(Self::object_path(assets_dir, &object.hash), destination)?;
        }
        Ok(Some(target))
    }
}

/// Unpacks old-style native jars (pre-1.19) into the natives folder.
fn extract_natives(jars: &[(PathBuf, Vec<String>)], target: &Path) -> std::io::Result<()> {
    for (jar, exclude) in jars {
        let mut archive =
            zip::ZipArchive::new(std::fs::File::open(jar)?).map_err(std::io::Error::other)?;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(std::io::Error::other)?;
            let Ok(name) = entry.name().map(|name| name.into_owned()) else {
                continue;
            };
            if entry.is_dir()
                || exclude
                    .iter()
                    .any(|prefix| name.starts_with(prefix.as_str()))
            {
                continue;
            }
            let Some(relative) = entry.enclosed_name() else {
                continue;
            };
            let destination = target.join(relative);
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::io::copy(&mut entry, &mut std::fs::File::create(destination)?)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_game_that_said_stopping_was_quit() {
        let read = |text: &str| stopped_cleanly(text.lines());
        // Vanilla and Fabric, Forge, and the XML log format.
        assert_eq!(read("[20:01:02] [Render thread/INFO]: Stopping!\n[20:01:02] [Sound engine/INFO]: closed"), Some(true));
        assert_eq!(read("[20:01:02] [Render thread/INFO] [minecraft/Minecraft]: Stopping!"), Some(true));
        assert_eq!(read("<log4j:Message><![CDATA[Stopping!]]></log4j:Message>"), Some(true));
        // Java tripping after the game said goodbye is still a quit.
        assert_eq!(read("[20:01:02] [Render thread/INFO]: Stopping!\n# A fatal error has been detected by the Java Runtime Environment:"), Some(true));
        // A crash on the way out (saving the world, say) is a crash.
        assert_eq!(read("[20:01:02] [Render thread/INFO]: Stopping!\n---- Minecraft Crash Report ----\n// Oops."), Some(false));
        // A crash the game came back from, then a quit.
        assert_eq!(read("#@!@# Game crashed! Crash report saved to: #@!@# x.txt\n[20:09:00] [Render thread/INFO]: Stopping!"), Some(true));
        // Neither: a game that just died, or a mod saying something else.
        assert_eq!(read("[20:01:02] [Render thread/INFO]: Loaded 7 recipes\nSegmentation fault"), None);
        assert_eq!(read("[20:01:02] [Server thread/INFO]: Stopping server"), None);
    }

    /// Runs a stand-in for the game (a shell script) through `run`, in a folder of its own.
    #[cfg(unix)]
    async fn run_stand_in(name: &str, script: &str, before: impl FnOnce(&Path)) -> (Exit, String) {
        let game_dir = std::env::temp_dir().join(format!("wisp-quit-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&game_dir);
        std::fs::create_dir_all(&game_dir).unwrap();
        before(&game_dir);
        let prepared = Prepared {
            java: PathBuf::from("/bin/sh"),
            jvm_args: vec!["-c".into(), script.into()],
            main_class: "minecraft".into(),
            game_args: Vec::new(),
            game_dir: game_dir.clone(),
            hooks: Hooks::default(),
            hook_env: Vec::new(),
            quick_play: None,
        };
        let (_keep_running, stop) = oneshot::channel();
        let exit = run(prepared, &|_| {}, stop, || 0).await.unwrap();
        let last_run = std::fs::read_to_string(game_dir.join("logs").join("wisp-last-run.log")).unwrap();
        let _ = std::fs::remove_dir_all(&game_dir);
        (exit, last_run)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn quitting_is_not_crashing_whatever_the_exit_code() {
        const STOPPING: &str = "echo '[20:01:02] [Render thread/INFO]: Stopping!'";
        // Quit, then Java fails on the way out (the Windows case): a quit.
        let (exit, last_run) = run_stand_in("driver", &format!("{STOPPING}; exit 1"), |_| {}).await;
        assert!(!exit.crashed);
        assert_eq!(exit.code, Some(1));
        assert!(last_run.ends_with("[Wisp] Java exited with code 1\n"), "{last_run}");
        // A plain quit.
        assert!(!run_stand_in("clean", &format!("{STOPPING}; exit 0"), |_| {}).await.0.crashed);
        // A crash, with and without a report.
        assert!(run_stand_in("crash", "echo 'java.lang.IllegalStateException: boom'; exit 255", |_| {}).await.0.crashed);
        let crashed_saving = format!("{STOPPING}; echo '#@!@# Game crashed! Crash report saved to: #@!@# c.txt'; exit 255");
        assert!(run_stand_in("saving", &crashed_saving, |_| {}).await.0.crashed);
        // Nothing on the console, but Minecraft's own log says it was quitting.
        let logged = "mkdir -p logs; echo '[20:01:02] [Render thread/INFO]: Stopping!' > logs/latest.log; exit 1";
        assert!(!run_stand_in("logged", logged, |_| {}).await.0.crashed);
        // A latest.log from an earlier session doesn't count.
        let stale = |dir: &Path| {
            std::fs::create_dir_all(dir.join("logs")).unwrap();
            let log = dir.join("logs").join("latest.log");
            std::fs::write(&log, "[20:01:02] [Render thread/INFO]: Stopping!\n").unwrap();
            let hour_ago = std::time::SystemTime::now() - Duration::from_secs(3600);
            std::fs::File::options().write(true).open(&log).unwrap().set_modified(hour_ago).unwrap();
        };
        assert!(run_stand_in("stale", "exit 1", stale).await.0.crashed);
    }

    #[test]
    fn launch_updates_serialize_the_way_the_ui_reads_them() {
        let update = serde_json::to_value(LaunchUpdate::Downloading {
            done_bytes: 1,
            total_bytes: 2,
            done_files: 3,
            total_files: 4,
        })
        .unwrap();
        assert_eq!(update["phase"], "downloading");
        assert_eq!(update["doneBytes"], 1);
        assert_eq!(update["totalFiles"], 4);
        let exited = serde_json::to_value(LaunchUpdate::Exited {
            code: Some(1),
            crashed: true,
            log_tail: vec!["boom".into()],
            help: None,
        })
        .unwrap();
        assert_eq!(exited["phase"], "exited");
        assert_eq!(exited["logTail"][0], "boom");
    }

    #[test]
    fn asset_downloads_skip_duplicates_and_odd_hashes() {
        let index: AssetIndex = serde_json::from_str(
            r#"{"objects":{
              "a.ogg":{"hash":"0123456789abcdef0123456789abcdef01234567","size":1},
              "b.ogg":{"hash":"0123456789abcdef0123456789abcdef01234567","size":1},
              "c.ogg":{"hash":"../../etc","size":1}
            }}"#,
        )
        .unwrap();
        let downloads = index.downloads(Path::new("/assets"));
        assert_eq!(downloads.len(), 1);
        assert_eq!(
            downloads[0].url,
            "https://resources.download.minecraft.net/01/0123456789abcdef0123456789abcdef01234567"
        );
    }

    #[test]
    fn splits_typed_arguments_like_a_shell() {
        assert_eq!(
            split_args("  -Xss2M   -XX:+UseZGC "),
            ["-Xss2M", "-XX:+UseZGC"]
        );
        assert_eq!(
            split_args(r#"-Dname="Wisp Player" '-Dx=a b' -Dy=c\ d"#),
            ["-Dname=Wisp Player", "-Dx=a b", "-Dy=c d"]
        );
        assert_eq!(split_args(r#"prime-run """#), ["prime-run", ""]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn version_ids_stay_inside_their_folder() {
        assert!(safe_id("26.3"));
        assert!(safe_id("fabric-loader-0.20.1-26.3"));
        assert!(!safe_id("../26.3"));
        assert!(!safe_id(""));
    }
}
