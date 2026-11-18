//! Cloud sync in the app: the sync page's commands, a loop that syncs in the background, and the
//! moments around a game (the newest copy before it starts, what changed right after it closes).
//! The work itself is `wisp_core::sync`; this decides when, and keeps the window told.
//!
//! The loop adapts: every half minute while a cloud app is still bringing files, every couple of
//! minutes while another computer is active, a check-in each minute while a game runs here (the
//! full sync waits for it to close), and every ten minutes when nothing's happening.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex as SyncMutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use wisp_core::device::DeviceInfo;
use wisp_core::sync::{self, Activity, CloudFolder, Context, Fetch, FetchError, LaunchCheck, Overview, Phase, Resolution, Scope};
use wisp_core::{HttpClient, modrinth};

use crate::{Launches, Network, Paths, SettingsState, current_settings, now_ms};

/// The sync page and badges listen for this (with the new overview).
const SYNC_EVENT: &str = "sync";
/// What's moving right now (None when nothing is), at most ten times a second.
const ACTIVITY_EVENT: &str = "sync-activity";

#[derive(Default)]
pub struct SyncHub {
    /// One sync at a time.
    lock: tokio::sync::Mutex<()>,
    /// Wakes the background loop early.
    wake: tokio::sync::Notify,
    started: AtomicBool,
}

/// Mods come from Modrinth's CDN, checked against the SHA-1 the other computer recorded.
struct AppFetch {
    client: HttpClient,
}

impl Fetch for AppFetch {
    fn download(&self, url: &str, sha1: &str, target: &Path) -> Result<(), FetchError> {
        if !url.starts_with("https://cdn.modrinth.com/") {
            return Err(FetchError::Gone);
        }
        let bytes = tauri::async_runtime::block_on(async {
            let response = self.client.get(url).send().await.map_err(|_| FetchError::Unreachable)?;
            // Modrinth answering "not here" means the file was taken down; anything else is a
            // hiccup worth trying again later.
            if response.status().is_client_error() {
                return Err(FetchError::Gone);
            }
            let response = response.error_for_status().map_err(|_| FetchError::Unreachable)?;
            response.bytes().await.map_err(|_| FetchError::Unreachable)
        })?;
        if sync::sha1_of(&bytes) != sha1 {
            return Err(FetchError::Gone);
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|_| FetchError::Unreachable)?;
        }
        let temp = target.with_extension("wisp-sync");
        std::fs::write(&temp, &bytes).map_err(|_| FetchError::Unreachable)?;
        std::fs::rename(&temp, target).map_err(|_| FetchError::Unreachable)
    }

    fn known(&self, sha1s: &[String]) -> Option<HashMap<String, String>> {
        let mut found = HashMap::new();
        // Modrinth takes a few hundred hashes a request comfortably.
        for chunk in sha1s.chunks(400) {
            let versions = tauri::async_runtime::block_on(modrinth::versions_by_hashes(&self.client, chunk)).ok()?;
            for (hash, version) in versions {
                if let Some(file) = version.files.iter().find(|file| file.sha1.as_deref() == Some(hash.as_str())) {
                    found.insert(hash, file.url.clone());
                }
            }
        }
        Some(found)
    }
}

/// Instances being prepared or played here: sync leaves them alone.
fn busy(app: &AppHandle) -> HashSet<String> {
    app.try_state::<Launches>().map(|launches| launches.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).keys().cloned().collect()).unwrap_or_default()
}

/// Runs `act` with everything sync needs. Blocking: call it off the async runtime.
fn with_context<T>(app: &AppHandle, act: impl FnOnce(&Context) -> T) -> T {
    let paths = app.state::<Paths>();
    let (data, instances, snapshots) = (paths.data.clone(), paths.instances(), paths.snapshots());
    let prefs = current_settings(&app.state::<SettingsState>()).sync;
    let device = DeviceInfo::current();
    let fetch = AppFetch { client: app.state::<Network>().downloads.clone() };
    let emitter = app.clone();
    let last = SyncMutex::new(Instant::now() - Duration::from_secs(1));
    let activity = move |activity: Option<Activity>| {
        let mut last = last.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if activity.is_none() || last.elapsed() >= Duration::from_millis(100) {
            *last = Instant::now();
            let _ = emitter.emit(ACTIVITY_EVENT, activity);
        }
    };
    let ctx = Context { data: &data, instances: &instances, snapshots: &snapshots, prefs: &prefs, device: &device, fetch: &fetch, activity: &activity, now: now_ms };
    act(&ctx)
}

/// Does some sync work off the async runtime, one at a time, then tells the window.
async fn locked<T: Send + 'static>(app: &AppHandle, work: impl FnOnce(&AppHandle, &Context) -> Result<T, String> + Send + 'static, overview: impl Fn(&T) -> Option<Overview>) -> Result<T, String> {
    let hub = app.state::<SyncHub>();
    let _one_at_a_time = hub.lock.lock().await;
    let handle = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || with_context(&handle, |ctx| work(&handle, ctx))).await.map_err(|error| error.to_string())??;
    if let Some(overview) = overview(&result) {
        let _ = app.emit(SYNC_EVENT, &overview);
    }
    Ok(result)
}

async fn run_now(app: &AppHandle, instance: Option<String>, asked: HashSet<String>) -> Result<Overview, String> {
    locked(
        app,
        move |app, ctx| {
            let busy = busy(app);
            let scope = match &instance {
                Some(id) => Scope::Instance(id),
                None => Scope::Everything,
            };
            sync::run(ctx, scope, &busy, &asked).map_err(|error| error.to_string())
        },
        |overview| Some(overview.clone()),
    )
    .await
}

fn data(app: &AppHandle) -> PathBuf {
    app.state::<Paths>().data.clone()
}

/// Starts the background loop (once). It idles while sync is off.
pub fn start(app: &AppHandle) {
    if app.state::<SyncHub>().started.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut delay = Duration::from_secs(6);
        let mut last_full = Instant::now() - Duration::from_secs(3600);
        loop {
            let woken = {
                let hub = app.state::<SyncHub>();
                tokio::select! {
                    _ = tokio::time::sleep(delay) => false,
                    _ = hub.wake.notified() => true,
                }
            };
            let current = sync::overview(&data(&app));
            if current.folder.is_none() {
                delay = Duration::from_secs(600);
                continue;
            }
            let playing: Vec<String> = busy(&app).into_iter().collect();
            if !playing.is_empty() && !woken && last_full.elapsed() < Duration::from_secs(600) {
                // A game is running here: just say so (each minute), and leave the rest for later.
                let handle = app.clone();
                let _ = tauri::async_runtime::spawn_blocking(move || with_context(&handle, |ctx| sync::check_in(ctx, &playing))).await;
                delay = Duration::from_secs(60);
                continue;
            }
            last_full = Instant::now();
            delay = match run_now(&app, None, HashSet::new()).await {
                Ok(overview) => next_delay(&overview, !busy(&app).is_empty()),
                Err(_) => Duration::from_secs(120),
            };
        }
    });
}

fn next_delay(overview: &Overview, playing_here: bool) -> Duration {
    let now = now_ms();
    let others_active = overview.devices.iter().any(|device| !device.this && now - device.seen < 10 * 60 * 1000);
    match overview.phase {
        Phase::Waiting => Duration::from_secs(30),
        Phase::Unreachable | Phase::Failed => Duration::from_secs(120),
        _ if playing_here => Duration::from_secs(60),
        _ if others_active => Duration::from_secs(120),
        _ => Duration::from_secs(600),
    }
}

/// Asks the loop to sync now (settings changed, the window came back...).
pub fn wake(app: &AppHandle) {
    if let Some(hub) = app.try_state::<SyncHub>() {
        hub.wake.notify_one();
    }
}

/// A game just started here: tell the other computers straight away.
pub fn game_started(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if sync::overview(&data(&app)).folder.is_none() {
            return;
        }
        let playing: Vec<String> = busy(&app).into_iter().collect();
        let handle = app.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || with_context(&handle, |ctx| sync::check_in(ctx, &playing))).await;
    });
}

/// A game just closed here (its restore points are taken): send what it changed.
pub fn game_closed(app: &AppHandle, instance_id: &str) {
    let app = app.clone();
    let id = instance_id.to_string();
    tauri::async_runtime::spawn(async move {
        if sync::overview(&data(&app)).folder.is_none() {
            return;
        }
        let _ = run_now(&app, Some(id), HashSet::new()).await;
    });
}

// ── Commands ──

#[tauri::command]
pub async fn get_sync(app: AppHandle) -> Result<Overview, String> {
    Ok(sync::overview(&data(&app)))
}

/// The cloud folders on this computer, the likeliest first.
#[tauri::command]
pub async fn get_sync_folders(app: AppHandle) -> Result<Vec<CloudFolder>, String> {
    let data = data(&app);
    tauri::async_runtime::spawn_blocking(move || sync::detect(&sync::this_device(&data))).await.map_err(|error| error.to_string())
}

/// Lets the player pick any folder to sync through.
#[tauri::command]
pub async fn pick_sync_folder(app: AppHandle) -> Result<Option<String>, String> {
    let picked = tauri::async_runtime::spawn_blocking(move || app.dialog().file().set_title("Choose a folder to keep your worlds in").blocking_pick_folder())
        .await
        .map_err(|error| error.to_string())?;
    Ok(picked.and_then(|path| path.into_path().ok()).map(|path| path.to_string_lossy().into_owned()))
}

/// Turns sync on with a cloud folder (Wisp's own folder goes inside it), and syncs.
#[tauri::command]
pub async fn start_sync(path: String, app: AppHandle) -> Result<Overview, String> {
    let folder = sync::wisp_folder_in(Path::new(&path));
    if !folder.parent().is_some_and(Path::is_dir) {
        return Err("That folder isn't there any more.".into());
    }
    let overview = locked(&app, move |_, ctx| sync::connect(ctx, &folder).map_err(|error| format!("Couldn't set up sync there: {error}")), |overview| Some(overview.clone())).await?;
    start(&app);
    Ok(overview)
}

/// Turns sync off here. Nothing is deleted anywhere.
#[tauri::command]
pub async fn stop_sync(app: AppHandle) -> Result<Overview, String> {
    locked(&app, |_, ctx| sync::disconnect(ctx).map_err(|error| error.to_string()), |overview| Some(overview.clone())).await
}

#[tauri::command]
pub async fn sync_now(app: AppHandle) -> Result<Overview, String> {
    run_now(&app, None, HashSet::new()).await
}

/// Settles a world that changed here and on another computer.
#[tauri::command]
pub async fn resolve_sync(key: String, resolution: Resolution, app: AppHandle) -> Result<Overview, String> {
    locked(&app, move |app, ctx| sync::resolve(ctx, &key, resolution, &busy(app)).map_err(|error| error.to_string()), |overview| Some(overview.clone())).await
}

/// Brings a world or instance that's only in the cloud here.
#[tauri::command]
pub async fn bring_sync_item(key: String, app: AppHandle) -> Result<Overview, String> {
    locked(&app, move |app, ctx| sync::fetch_item(ctx, &key, &busy(app)).map_err(|error| error.to_string()), |overview| Some(overview.clone())).await
}

/// Takes a world out of the cloud (every computer keeps its copy).
#[tauri::command]
pub async fn remove_sync_item(key: String, app: AppHandle) -> Result<Overview, String> {
    locked(&app, move |_, ctx| sync::take_out(ctx, &key).map_err(|error| error.to_string()), |overview| Some(overview.clone())).await
}

/// Just before Play: brings the newest copy of the instance and its worlds, and says what (if
/// anything) the player should decide first.
#[tauri::command]
pub async fn check_launch(instance_id: String, app: AppHandle) -> Result<LaunchCheck, String> {
    if sync::overview(&data(&app)).folder.is_none() {
        return Ok(LaunchCheck::default());
    }
    let check = locked(&app, move |app, ctx| sync::before_launch(ctx, &instance_id, &busy(app)).map_err(|error| error.to_string()), |_| None).await?;
    let _ = app.emit(SYNC_EVENT, sync::overview(&data(&app)));
    Ok(check)
}

/// Shows the sync folder in Finder or Explorer.
#[tauri::command]
pub async fn open_sync_folder(app: AppHandle) -> Result<(), String> {
    let folder = sync::overview(&data(&app)).folder.ok_or("Sync is off.")?;
    app.opener().open_path(folder.path.to_string_lossy(), None::<&str>).map_err(|error| error.to_string())
}
