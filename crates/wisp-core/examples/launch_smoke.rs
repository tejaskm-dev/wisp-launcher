//! Prepares and starts a Minecraft version end to end, then closes it a few seconds after its
//! window is up. It uses Wisp's real data folder, so whatever it downloads, the app reuses.
//!
//!   cargo run -p wisp-core --example launch_smoke -- 26.3 fabric   (or quilt, forge, neoforge)
//!
//! Set WISP_DATA to use another folder. To try the launch settings, set WISP_JVM_ARGS,
//! WISP_WINDOW ("1000x600" or "fullscreen"), WISP_PRE, WISP_WRAPPER or WISP_POST.
//!
//! WISP_SKIN starts the skin server for the offline account, wearing the skin worn in Wisp
//! ("worn") or a PNG file, and prints every question the game asks it. WISP_WORLD opens a world
//! in the game folder straight away, and WISP_KEEP keeps the game open that many seconds.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tokio::sync::oneshot;
use wisp_core::accounts::{Account, AccountKind, offline_uuid};
use wisp_core::instances::{Instance, Loader};
use wisp_core::launch::{self, GameFiles, LaunchOptions, LaunchUpdate};

fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("WISP_DATA") {
        return dir.into();
    }
    let home = PathBuf::from(std::env::var_os("HOME").expect("HOME is set"));
    home.join("Library/Application Support/app.wisp.launcher")
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
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
    let client = wisp_core::download_client().expect("client");
    let loader_version = if loader == Loader::Vanilla {
        None
    } else {
        wisp_core::loaders::recommended(&client, loader, &version)
            .await
            .expect("loader meta")
    };
    println!(
        "version {version}, {} {loader_version:?}, data {}",
        loader.display_name(),
        data_dir().display()
    );
    let options = LaunchOptions {
        instance: Instance {
            id: "smoke-test".into(),
            name: "Smoke test".into(),
            game_version: version,
            loader,
            loader_version,
            created: 0,
            last_played: None,
            icon: None,
            modpack: None,
            memory_mb: None,
            java_major: None,
            sync_id: None,
        },
        game_dir: std::env::temp_dir().join("wisp-smoke-test"),
        account: Account {
            id: "smoke".into(),
            name: "WispTester".into(),
            uuid: offline_uuid("WispTester"),
            kind: AccountKind::Offline,
        },
        memory_mb: 3072,
        concurrency: 16,
        launcher_version: "smoke".into(),
        jvm_args: launch::split_args(&env("WISP_JVM_ARGS").unwrap_or_default()),
        window: match env("WISP_WINDOW").as_deref() {
            Some("fullscreen") => launch::WindowSize::Fullscreen,
            Some(size) => size
                .split_once('x')
                .and_then(|(width, height)| Some((width.parse().ok()?, height.parse().ok()?)))
                .map_or(launch::WindowSize::Default, |(width, height)| {
                    launch::WindowSize::Custom { width, height }
                }),
            None => launch::WindowSize::Default,
        },
        java_paths: Default::default(),
        hooks: launch::Hooks {
            pre_launch: env("WISP_PRE"),
            wrapper: launch::split_args(&env("WISP_WRAPPER").unwrap_or_default()),
            post_exit: env("WISP_POST"),
        },
        quick_play: env("WISP_WORLD").map(wisp_core::launch::QuickPlay::World),
        skin_server: None,
    };
    let started = Instant::now();
    let mut options = options;
    if let Some(skin) = env("WISP_SKIN") {
        options.skin_server = Some(skin_server(&skin, &options.account, started).await);
    }
    let keep = env("WISP_KEEP").and_then(|seconds| seconds.parse().ok()).unwrap_or(6);

    let last_print = Mutex::new(Instant::now() - Duration::from_secs(10));
    let (stop, stop_requested) = oneshot::channel();
    let stop = Mutex::new(Some(stop));
    let report = |update: LaunchUpdate| match &update {
        LaunchUpdate::Downloading {
            done_bytes,
            total_bytes,
            done_files,
            total_files,
        } => {
            let mut last = last_print.lock().unwrap();
            if last.elapsed() >= Duration::from_secs(3) || done_files == total_files {
                *last = Instant::now();
                println!(
                    "[{:>5.1}s] downloading {:.0}/{:.0} MB, {done_files}/{total_files} files",
                    started.elapsed().as_secs_f32(),
                    *done_bytes as f64 / 1e6,
                    *total_bytes as f64 / 1e6
                );
            }
        }
        LaunchUpdate::Running { .. } => {
            println!(
                "[{:>5.1}s] window is up; closing in {keep} s",
                started.elapsed().as_secs_f32()
            );
            if let Some(stop) = stop.lock().unwrap().take() {
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(keep)).await;
                    let _ = stop.send(());
                });
            }
        }
        other => println!("[{:>5.1}s] {other:?}", started.elapsed().as_secs_f32()),
    };

    let prepared =
        match launch::prepare(&client, &GameFiles::new(data_dir()), &options, &report).await {
            Ok(prepared) => prepared,
            Err(error) => {
                eprintln!("prepare failed: {error}");
                std::process::exit(1);
            }
        };
    println!("[{:>5.1}s] prepared", started.elapsed().as_secs_f32());
    match launch::run(prepared, &report, stop_requested, now_ms).await {
        Ok(exit) => {
            println!(
                "[{:>5.1}s] exited: code {:?}, crashed {}",
                started.elapsed().as_secs_f32(),
                exit.code,
                exit.crashed
            );
            for line in exit.log_tail.iter().rev().take(15).rev() {
                println!("  | {line}");
            }
        }
        Err(error) => {
            eprintln!("run failed: {error}");
            std::process::exit(1);
        }
    }
}

/// The skin server, answering for the offline account with the skin worn in Wisp ("worn") or a
/// PNG file, and printing each question the game asks it.
async fn skin_server(skin: &str, account: &Account, started: Instant) -> wisp_core::skin_server::SkinServer {
    use std::sync::Arc;
    use wisp_core::skin_server::{self, Observe, Player, Skin, Who};

    let (png, slim) = if skin == "worn" {
        let store = wisp_core::skins::SkinStore::new(data_dir().join("skins"));
        let library = store.library();
        let worn = library.active.as_ref().and_then(|id| library.skins.iter().find(|item| &item.id == id)).expect("a skin worn in Wisp");
        println!("wearing {} ({:?})", worn.name, worn.model);
        (std::fs::read(store.png_path(&worn.id)).expect("the worn skin's png"), worn.model == wisp_core::skins::SkinModel::Slim)
    } else {
        (std::fs::read(skin).expect("the skin png"), false)
    };
    let (uuid, name) = (account.uuid.replace('-', "").to_lowercase(), account.name.clone());
    let player = Player { uuid: uuid.clone(), name: name.clone(), skin: Some(Skin { png, slim }) };
    let lookup: skin_server::Lookup = Arc::new(move |who| {
        let matches = match who {
            Who::Uuid(asked) => asked.replace('-', "").eq_ignore_ascii_case(&uuid),
            Who::Name(asked) | Who::Joining(asked) => asked.eq_ignore_ascii_case(&name),
        };
        matches.then(|| player.clone())
    });
    let observe: Observe = Arc::new(move |method, path, status| println!("[{:>5.1}s] skin server: {method} {path} -> {status}", started.elapsed().as_secs_f32()));
    let key = skin_server::load_or_make_key(&std::env::temp_dir().join("wisp-skin-smoke-key.der")).expect("key");
    let server = skin_server::start_observed(key, lookup, Some(observe)).await.expect("skin server");
    println!("skin server at {}", server.api_root);
    server
}
