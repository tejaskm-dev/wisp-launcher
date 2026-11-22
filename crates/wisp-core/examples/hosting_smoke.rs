//! Sets up and starts a hosted server end to end in a throwaway folder, using Wisp's real data
//! folder for Java and the game files (so what it downloads, the app reuses).
//!
//!   cargo run -p wisp-core --example hosting_smoke -- 26.3 fabric
//!   (or vanilla, quilt, forge, neoforge; or the plugin servers paper, purpur, folia, spigot)
//!
//! Without WISP_EULA=1 the server stops at Minecraft's EULA check, which proves everything up to
//! the game itself. With it (set it only if you, the person running this, agree to the Minecraft
//! EULA at https://aka.ms/MinecraftEULA) the server runs until "Done", lists its players, and stops.
//! Set WISP_DATA to use another data folder and WISP_KEEP=1 to keep the server folder.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use wisp_core::hosting::install::{self, PrepareOptions};
use wisp_core::hosting::process::{self, RunState, ServerEvent};
use wisp_core::hosting::{NewServer, ServerStore};
use wisp_core::instances::Loader;
use wisp_core::launch::{GameFiles, LaunchUpdate};

fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("WISP_DATA") {
        return dir.into();
    }
    let home = PathBuf::from(std::env::var_os("HOME").expect("HOME is set"));
    home.join("Library/Application Support/app.wisp.launcher")
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|elapsed| elapsed.as_millis() as i64).unwrap_or_default()
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let version = args.first().cloned().unwrap_or_else(|| "26.3".into());
    let loader = match args.get(1).map(String::as_str) {
        Some("fabric") => Loader::Fabric,
        Some("quilt") => Loader::Quilt,
        Some("forge") => Loader::Forge,
        Some("neoforge") => Loader::NeoForge,
        _ => Loader::Vanilla,
    };
    use wisp_core::hosting::software::Software;
    let software = match args.get(1).map(String::as_str) {
        Some("paper") => Some(Software::Paper),
        Some("purpur") => Some(Software::Purpur),
        Some("folia") => Some(Software::Folia),
        Some("spigot") => Some(Software::Spigot),
        _ => None,
    };
    let client = wisp_core::download_client().expect("client");
    let loader_version = if loader == Loader::Vanilla { None } else { wisp_core::loaders::recommended(&client, loader, &version).await.expect("loader meta") };
    println!("Server: Minecraft {version}, {} {}", loader.display_name(), loader_version.as_deref().unwrap_or(""));

    let root = std::env::temp_dir().join(format!("wisp-hosting-smoke-{}", std::process::id()));
    let store = ServerStore::new(&root);
    let server = store
        .create(
            NewServer {
                name: format!("Smoke {version}"),
                game_version: version.clone(),
                loader,
                loader_version,
                instance_id: None,
                icon: None,
                properties: vec![("online-mode".into(), "false".into()), ("level-type".into(), "minecraft:flat".into())],
                software,
            },
            now_ms(),
        )
        .expect("create");
    let dir = store.dir_of(&server.id).expect("dir");
    println!("Folder: {}", dir.display());
    if std::env::var("WISP_EULA").as_deref() == Ok("1") {
        wisp_core::hosting::accept_eula(&dir).expect("eula");
        println!("EULA agreed to by whoever set WISP_EULA=1.");
    }

    let files = GameFiles::new(data_dir());
    let java_paths = Default::default();
    let options = PrepareOptions { memory_mb: 2048, java_major: None, java_paths: &java_paths, jvm_args: Vec::new(), concurrency: 16 };
    let started = std::time::Instant::now();
    let report = |update: LaunchUpdate| match update {
        LaunchUpdate::Downloading { done_bytes, total_bytes, done_files, total_files } if done_files == total_files => {
            println!("  downloaded {done_files} files, {:.1} MB", total_bytes.max(done_bytes) as f64 / 1e6)
        }
        LaunchUpdate::Installing { loader } => println!("  running {loader}'s installer"),
        _ => {}
    };
    let mut plan = install::prepare(&client, &files, &server, &dir, &options, &report).await.expect("prepare");
    // As the app does: the server on a port of its own, behind Wisp's door on the usual one.
    let inner = std::net::TcpListener::bind(("127.0.0.1", 0)).and_then(|listener| listener.local_addr()).expect("a free port").port();
    plan.listen_on(inner);
    println!("Ready to start after {:.1}s: {} {}", started.elapsed().as_secs_f64(), plan.java.display(), plan.args.join(" "));

    let events = Arc::new(|event: ServerEvent| match event {
        ServerEvent::State { state } => println!("  [state] {state:?}"),
        ServerEvent::Players { players } => println!("  [players] {players:?}"),
        ServerEvent::Lines { .. } => {}
    });
    let running = process::start(&plan, now_ms, events).expect("start");
    let mut waited = 0;
    while !running.state().is_over() && running.state() != RunState::Running && waited < 600 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        waited += 1;
    }
    if running.state() == RunState::Running {
        println!("Up after {:.1}s.", (running.ready_at().unwrap_or_default() - running.started_at()) as f64 / 1000.0);
        println!("Listening on its own port {inner}: {}", std::net::TcpStream::connect(("127.0.0.1", inner)).is_ok());
        running.send("list").await.expect("send");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let exit = running.stop().await;
    println!("Exit: {exit:?}");
    for line in running.console().iter().rev().take(12).collect::<Vec<_>>().into_iter().rev() {
        println!("  | {}", line.text);
    }
    if std::env::var("WISP_KEEP").as_deref() != Ok("1") {
        let _ = std::fs::remove_dir_all(&root);
    }
}
