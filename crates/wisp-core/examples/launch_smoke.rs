//! Prepares and starts a Minecraft version end to end, then closes it a few seconds after its
//! window is up. It uses Wisp's real data folder, so whatever it downloads, the app reuses.
//!
//!   cargo run -p wisp-core --example launch_smoke -- 26.3 fabric   (or quilt, forge, neoforge)
//!
//! Set WISP_DATA to use another folder. To try the launch settings, set WISP_JVM_ARGS,
//! WISP_WINDOW ("1000x600" or "fullscreen"), WISP_PRE, WISP_WRAPPER or WISP_POST.

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
        quick_play: env("WISP_WORLD"),
    };

    let started = Instant::now();
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
                "[{:>5.1}s] window is up; closing in 6 s",
                started.elapsed().as_secs_f32()
            );
            if let Some(stop) = stop.lock().unwrap().take() {
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(6)).await;
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
