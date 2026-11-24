// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod away;
mod fixes;
mod hosting;
mod imports;
mod instance_page;
mod library;
mod multiplayer;
mod playit;
mod protocol;
mod shared_options;
mod skins;
mod social;
mod sync;
mod tester_updates;

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Mutex as SyncMutex;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::async_runtime::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;
use wisp_core::accounts::{Account, AccountStore};
use wisp_core::content::{self, ContentIndex, InstalledContent};
use wisp_core::curseforge;
use wisp_core::feeds::{self, PatchNote};
use wisp_core::home::{HomeSnapshot, instance_label};
use wisp_core::instances::{GAME_DIR, Instance, InstanceError, InstanceStore, Loader, NewInstance};
use wisp_core::journal::{Event as JournalEvent, Journal};
use wisp_core::doctor;
use wisp_core::launch::{
    self, GameFiles, Hooks, InstalledRuntime, JavaProbe, LaunchOptions, LaunchUpdate, QuickPlay,
    WindowSize,
};
use wisp_core::loaders::{self, LoaderVersion};
use wisp_core::modpack::{self, InstallUpdate};
use wisp_core::modrinth::{
    self, Category, LoaderTag, Project, ProjectDetails, ProjectSummary, ProjectType,
    ProjectVersion, SearchQuery, SearchResults, Source,
};
use wisp_core::settings::{Settings, WhilePlaying, WindowMode};
use wisp_core::skins::SkinStore;
use wisp_core::snapshots::Snapshots;
use wisp_core::storage::{self, StorageUsage};
use wisp_core::versions::{self, VersionList};
use wisp_core::worlds::{self, WorldEntry};
use wisp_core::{HttpClient, device, download_client, game_options, http_client};

/// Feeds change rarely; keeping them a while makes revisiting a page instant.
const FEED_TTL: Duration = Duration::from_secs(15 * 60);
const FEATURED_MODPACKS: u32 = 3;

/// The last good result of a feed, refreshed after `FEED_TTL`.
struct Cached<T>(Mutex<Option<(Instant, T)>>);

impl<T: Clone> Cached<T> {
    fn new() -> Self {
        Self(Mutex::new(None))
    }

    /// Returns the cached value while it is fresh. Otherwise fetches again, and if that fails
    /// (for example, offline) falls back to the stale value rather than showing an error.
    async fn get_or_fetch<F>(&self, fetch: F) -> Result<T, String>
    where
        F: Future<Output = Result<T, wisp_core::Error>>,
    {
        let mut slot = self.0.lock().await;
        if let Some((fetched_at, value)) = slot.as_ref()
            && fetched_at.elapsed() < FEED_TTL
        {
            return Ok(value.clone());
        }
        match fetch.await {
            Ok(value) => {
                *slot = Some((Instant::now(), value.clone()));
                Ok(value)
            }
            Err(error) => slot
                .as_ref()
                .map(|(_, value)| value.clone())
                .ok_or_else(|| error.to_string()),
        }
    }
}

/// Like `Cached`, for results that depend on a key (a loader, a Minecraft version).
struct KeyedCache<T>(Mutex<HashMap<String, (Instant, T)>>);

impl<T: Clone> KeyedCache<T> {
    fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }

    async fn get_or_fetch<F>(&self, key: String, fetch: F) -> Result<T, String>
    where
        F: Future<Output = Result<T, wisp_core::Error>>,
    {
        let mut slots = self.0.lock().await;
        if let Some((fetched_at, value)) = slots.get(&key)
            && fetched_at.elapsed() < FEED_TTL
        {
            return Ok(value.clone());
        }
        match fetch.await {
            Ok(value) => {
                slots.insert(key, (Instant::now(), value.clone()));
                Ok(value)
            }
            Err(error) => slots
                .get(&key)
                .map(|(_, value)| value.clone())
                .ok_or_else(|| error.to_string()),
        }
    }
}

struct Network {
    client: HttpClient,
    categories: Cached<Vec<Category>>,
    loader_tags: Cached<Vec<LoaderTag>>,
    curseforge_categories: KeyedCache<Vec<Category>>,
    loader_games: KeyedCache<Option<Vec<String>>>,
    loader_versions: KeyedCache<Vec<LoaderVersion>>,
    /// For game downloads, which may take longer than an API call should.
    downloads: HttpClient,
    modpacks: Cached<Vec<Project>>,
    news: Cached<Vec<PatchNote>>,
    versions: Cached<VersionList>,
}

/// Where Wisp keeps its files.
struct Paths {
    data: PathBuf,
    settings: PathBuf,
}

impl Paths {
    fn instances(&self) -> InstanceStore {
        InstanceStore::new(self.data.join("instances"))
    }

    fn accounts(&self) -> AccountStore {
        AccountStore::new(self.data.join("accounts.json"))
    }

    /// Versions, libraries, assets and Java, shared by every instance.
    fn game_files(&self) -> GameFiles {
        GameFiles::new(&self.data)
    }

    fn journal(&self) -> Journal {
        Journal::new(self.data.join("journal"))
    }

    /// Restore points live beside the journal.
    fn snapshots(&self) -> Snapshots {
        Snapshots::new(self.data.join("journal"))
    }

    fn skins(&self) -> SkinStore {
        SkinStore::new(self.data.join("skins"))
    }

    /// Screenshot thumbnails: a cache, cleared with the rest of it.
    fn thumbs(&self) -> PathBuf {
        self.data.join("cache").join("thumbs")
    }
}

/// Modpack installs running in the background, by job id (the project id).
#[derive(Default)]
struct Installs(SyncMutex<HashMap<String, InstallEvent>>);

/// Sent to the UI on the "install" event, and returned by `get_installs`.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallEvent {
    job_id: String,
    title: String,
    update: InstallUpdate,
}

const INSTALL_EVENT: &str = "install";

/// Instances being prepared or played: the latest update for each, and how to stop it.
#[derive(Default)]
struct Launches(SyncMutex<HashMap<String, LaunchSlot>>);

struct LaunchSlot {
    stop: Option<oneshot::Sender<()>>,
    last: LaunchUpdate,
    /// The world the game is opening straight into, once that's known.
    world: Option<String>,
    /// The server the game is joining straight away, once that's known.
    server: Option<String>,
}

/// Sent to the UI on the "launch" event, and returned by `get_launches`.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchEvent {
    instance_id: String,
    /// The world (its folder) the game opened straight into, if any.
    world: Option<String>,
    /// The server the game joined straight away, if any.
    server: Option<String>,
    update: LaunchUpdate,
}

const LAUNCH_EVENT: &str = "launch";
/// Sent when Wisp fixed (or found) problems with an instance, before starting it or after a crash.
const FIXED_EVENT: &str = "fixed";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FixedEvent {
    instance_id: String,
    report: doctor::Report,
}

struct SettingsState(SyncMutex<Settings>);

/// Offline accounts work in developer builds, and in tester builds (the release workflow's
/// "tester" option, built with WISP_TESTER=1) made for a closed group of people who own
/// Minecraft. Public releases need a Microsoft account that owns the game.
const OFFLINE_ACCOUNTS: bool = cfg!(debug_assertions) || is_one(option_env!("WISP_TESTER"));

/// Whether a build-time setting is exactly "1" (an empty one, as the workflow passes when the
/// option's off, isn't).
const fn is_one(value: Option<&str>) -> bool {
    match value {
        Some(text) => matches!(text.as_bytes(), [b'1']),
        None => false,
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

#[tauri::command]
async fn get_home(paths: State<'_, Paths>) -> Result<HomeSnapshot, String> {
    let (instances, accounts, journal, data) = (paths.instances(), paths.accounts(), paths.journal(), paths.data.clone());
    tauri::async_runtime::spawn_blocking(move || HomeSnapshot::load(&instances, &accounts, &journal, &wisp_core::sync::overview(&data)))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn get_settings(settings: State<'_, SettingsState>) -> Result<Settings, String> {
    Ok(settings
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone())
}

#[tauri::command]
async fn set_settings(
    next: Settings,
    app: AppHandle,
    settings: State<'_, SettingsState>,
    paths: State<'_, Paths>,
) -> Result<Settings, String> {
    let next = next.sanitized();
    next.save(&paths.settings)
        .map_err(|error| format!("Couldn't save settings: {error}"))?;
    let previous = std::mem::replace(
        &mut *settings
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
        next.clone(),
    );
    // What syncs changed: sync now, so what was just switched on goes up.
    if previous.sync != next.sync {
        sync::wake(&app);
    }
    Ok(next)
}

#[tauri::command]
async fn get_versions(network: State<'_, Network>) -> Result<VersionList, String> {
    network
        .versions
        .get_or_fetch(versions::game_versions(&network.client))
        .await
}

#[tauri::command]
async fn create_instance(
    request: NewInstance,
    app: AppHandle,
    network: State<'_, Network>,
    paths: State<'_, Paths>,
    settings: State<'_, SettingsState>,
) -> Result<Instance, String> {
    let loader_version = match (request.loader, &request.loader_version) {
        (Loader::Vanilla, _) => None,
        (_, Some(chosen)) => Some(chosen.clone()),
        (loader, None) => {
            let recommended = loaders::recommended(&network.client, loader, &request.game_version)
                .await
                .map_err(|error| error.to_string())?;
            let unsupported = || {
                InstanceError::LoaderUnsupported(
                    loader.display_name(),
                    request.game_version.clone(),
                )
                .to_string()
            };
            Some(recommended.ok_or_else(unsupported)?)
        }
    };
    let instances = paths.instances();
    let instance = instances
        .create(request, loader_version, now_ms())
        .map_err(|error| error.to_string())?;
    seed_options(&instances, &current_settings(&settings), &instance.id);
    library::note_event(
        &app,
        Some(&instance.id),
        None,
        JournalEvent::InstanceCreated {
            name: instance.name.clone(),
            label: instance_label(&instance),
        },
    );
    Ok(instance)
}

fn current_settings(settings: &SettingsState) -> Settings {
    settings
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// Gives a new instance the options.txt that Settings → Game asks for. Best effort: a new
/// instance works fine without it.
fn seed_options(instances: &InstanceStore, settings: &Settings, id: &str) {
    let Some(game_dir) = instances.game_dir(id) else {
        return;
    };
    let source = settings
        .new_instances
        .copy_options_from
        .as_deref()
        .filter(|source| *source != id)
        .and_then(|source| instances.game_dir(source));
    let chosen = game_options::chosen_defaults(&settings.new_instances);
    let _ = game_options::seed(&game_dir, source.as_deref(), &chosen);
}

#[tauri::command]
async fn search_projects(
    query: SearchQuery,
    network: State<'_, Network>,
    settings: State<'_, SettingsState>,
) -> Result<SearchResults, String> {
    let result = match query.source {
        Source::Modrinth => modrinth::search(&network.client, &query).await,
        Source::Curseforge => {
            let key = curseforge_key(&settings)?;
            curseforge::search(&network.client, &key, &query).await
        }
    };
    result.map_err(|error| error.to_string())
}

const NO_CURSEFORGE_KEY: &str = "CurseForge needs an API key. Add one in Settings → Mod sources.";

/// The CurseForge key from Settings, or the one built into this copy of Wisp.
fn curseforge_key(settings: &SettingsState) -> Result<String, String> {
    curseforge::api_key(&current_settings(settings).curseforge_api_key)
        .ok_or_else(|| NO_CURSEFORGE_KEY.to_string())
}

/// Whether CurseForge can be searched, and whether that's thanks to a key built into this copy.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeStatus {
    available: bool,
    built_in: bool,
}

#[tauri::command]
async fn get_curseforge_status(
    settings: State<'_, SettingsState>,
) -> Result<CurseForgeStatus, String> {
    Ok(CurseForgeStatus {
        available: curseforge::api_key(&current_settings(&settings).curseforge_api_key).is_some(),
        built_in: curseforge::api_key("").is_some(),
    })
}

#[tauri::command]
async fn get_curseforge_categories(
    project_type: ProjectType,
    network: State<'_, Network>,
    settings: State<'_, SettingsState>,
) -> Result<Vec<Category>, String> {
    let key = curseforge_key(&settings)?;
    network
        .curseforge_categories
        .get_or_fetch(
            project_type.as_str().to_string(),
            curseforge::categories(&network.client, &key, project_type),
        )
        .await
}

/// The worlds in an instance, for picking where a data pack goes.
#[tauri::command]
async fn get_instance_worlds(
    id: String,
    paths: State<'_, Paths>,
) -> Result<Vec<WorldEntry>, String> {
    let game_dir = paths
        .instances()
        .game_dir(&id)
        .ok_or("That instance no longer exists.")?;
    Ok(worlds::list(&game_dir))
}

#[tauri::command]
async fn open_instance_folder(
    id: String,
    app: AppHandle,
    paths: State<'_, Paths>,
) -> Result<(), String> {
    let dir = paths
        .instances()
        .dir_of(&id)
        .ok_or("That instance no longer exists.")?;
    app.opener()
        .open_path(dir.join(GAME_DIR).to_string_lossy(), None::<&str>)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn get_data_folder(paths: State<'_, Paths>) -> Result<String, String> {
    Ok(paths.data.to_string_lossy().into_owned())
}

#[tauri::command]
async fn open_data_folder(app: AppHandle, paths: State<'_, Paths>) -> Result<(), String> {
    std::fs::create_dir_all(&paths.data).map_err(|error| error.to_string())?;
    app.opener()
        .open_path(paths.data.to_string_lossy(), None::<&str>)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn get_featured_modpacks(network: State<'_, Network>) -> Result<Vec<Project>, String> {
    network
        .modpacks
        .get_or_fetch(feeds::featured_modpacks(&network.client, FEATURED_MODPACKS))
        .await
}

#[tauri::command]
async fn get_java_news(network: State<'_, Network>) -> Result<Vec<PatchNote>, String> {
    network
        .news
        .get_or_fetch(feeds::java_news(&network.client))
        .await
}

/// Starts preparing and playing an instance in the background. Progress arrives as "launch" events.
/// With a world, the game opens straight into it on versions that can (1.20 and later).
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri hands state over as arguments.
async fn launch_instance(
    id: String,
    world: Option<String>,
    server: Option<String>,
    app: AppHandle,
    paths: State<'_, Paths>,
    settings: State<'_, SettingsState>,
    network: State<'_, Network>,
    launches: State<'_, Launches>,
    skins_state: State<'_, SkinServerState>,
) -> Result<(), String> {
    let instances = paths.instances();
    let instance = instances
        .get(&id)
        .ok_or("That instance no longer exists.")?;
    let game_dir = instances
        .game_dir(&id)
        .ok_or("That instance no longer exists.")?;
    let account = paths.accounts().active().ok_or("Add an account to play.")?;
    let settings = current_settings(&settings);
    if let Some(folder) = &world
        && worlds::world_dir(&game_dir, folder).is_none()
    {
        return Err("That world no longer exists.".into());
    }
    if let Some(address) = &server {
        wisp_core::servers::ServerAddress::parse(address).map_err(|error| error.to_string())?;
    }

    let (stop, stop_requested) = oneshot::channel();
    {
        let mut active = launches
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if active.contains_key(&id) {
            return Err(format!("{} is already running.", instance.name));
        }
        active.insert(
            id.clone(),
            LaunchSlot {
                stop: Some(stop),
                last: LaunchUpdate::Checking,
                world: None,
                server: None,
            },
        );
    }
    // Offline accounts wear their Wisp skin in game through the skin server (Microsoft accounts get
    // theirs from Mojang). WISP_SKIN_SERVER=0 leaves it out, to rule it out when a game won't start.
    // A game joining a friend's server signs in through the host's skin server instead (over the
    // link), so everyone there sees everyone's skin.
    // Otherwise, joining a server from Minecraft's own list brings this player's skin to it when
    // it's hosted with Wisp (see hosting::bring_skin).
    let skin_server = match account.kind {
        wisp_core::accounts::AccountKind::Offline if std::env::var_os("WISP_SKIN_SERVER").is_none_or(|value| value != "0") => {
            let joined = skins_state.2.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(&id).cloned();
            match joined {
                Some(host) => Some(host),
                None => skins_state.0.get_or_init(|| start_skin_server(paths.data.clone(), skins_state.1.clone(), skins_state.3.clone())).await.clone(),
            }
        }
        _ => None,
    };
    // What this instance does differently on this computer stands in for Settings.
    let local = instances.local_settings(&id);
    let window = local.game_window.as_ref().unwrap_or(&settings.game_window);
    let hooks = local.hooks.as_ref().unwrap_or(&settings.hooks);
    let memory_mb = match instance.memory_mb {
        Some(own) => own,
        None => automatic_memory(&game_dir, settings.memory_mb).await,
    };
    let options = LaunchOptions {
        instance,
        game_dir,
        account,
        memory_mb,
        concurrency: settings.concurrent_downloads as usize,
        launcher_version: env!("CARGO_PKG_VERSION").to_string(),
        jvm_args: launch::split_args(local.jvm_args.as_deref().unwrap_or(&settings.jvm_args)),
        window: match window.mode {
            WindowMode::Default => WindowSize::Default,
            WindowMode::Custom => WindowSize::Custom {
                width: window.width,
                height: window.height,
            },
            WindowMode::Fullscreen => WindowSize::Fullscreen,
        },
        java_paths: settings
            .java_paths
            .iter()
            .filter_map(|(major, path)| Some((major.parse().ok()?, PathBuf::from(path))))
            .collect(),
        hooks: Hooks {
            pre_launch: Some(hooks.pre_launch.clone()).filter(|line| !line.is_empty()),
            wrapper: launch::split_args(&hooks.wrapper),
            post_exit: Some(hooks.post_exit.clone()).filter(|line| !line.is_empty()),
        },
        quick_play: match (server, world) {
            (Some(address), _) => Some(QuickPlay::Server(address)),
            (None, Some(world)) => Some(QuickPlay::World(world)),
            (None, None) => None,
        },
        skin_server,
    };
    tauri::async_runtime::spawn(play(
        app,
        id,
        network.downloads.clone(),
        paths.game_files(),
        options,
        settings,
        stop_requested,
    ));
    Ok(())
}

/// Records a launch update for an instance and sends it to the window.
fn report_launch(app: &AppHandle, id: &str, update: LaunchUpdate) {
    let (mut world, mut server) = (None, None);
    if let Some(launches) = app.try_state::<Launches>()
        && let Some(slot) = launches
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get_mut(id)
    {
        slot.last = update.clone();
        world = slot.world.clone();
        server = slot.server.clone();
    }
    // Friends see when a game starts and stops.
    if matches!(update, LaunchUpdate::Running { .. } | LaunchUpdate::Exited { .. }) {
        social::announce_soon(app);
    }
    let _ = app.emit(
        LAUNCH_EVENT,
        LaunchEvent {
            instance_id: id.to_string(),
            world,
            server,
            update,
        },
    );
}

/// The game's output after a load that failed yet exited cleanly (Forge's error screen does).
fn failed_to_load(output: &[String]) -> bool {
    const SIGNS: [&str; 4] = [
        "Missing or unsupported mandatory dependencies",
        "Mod resolution encountered an incompatible mod set",
        "ModLoadingException",
        "LoadingFailedException",
    ];
    output.iter().any(|line| SIGNS.iter().any(|sign| line.contains(sign)))
}

fn fixing_message(count: usize) -> String {
    if count == 1 {
        "Fixing a problem with the mods…".into()
    } else {
        format!("Fixing {count} problems with the mods…")
    }
}

/// Records a fix in the journal; its entry id is what Undo uses.
fn record_fix(app: &AppHandle, instance_id: &str, report: &doctor::Report, after_crash: bool) -> Option<String> {
    let paths = app.try_state::<Paths>()?;
    let event = JournalEvent::Fixed {
        found: report.found.clone(),
        fixed: report.fixed.clone(),
        changes: report.changes.clone(),
        after_crash,
    };
    let entry = paths.journal().append(Some(instance_id), None, now_ms(), event).ok()?;
    library::journal_changed(app);
    Some(entry.id)
}

/// The stop button's sender was taken: the player stopped this launch.
fn stop_pressed(app: &AppHandle, id: &str) -> bool {
    app.try_state::<Launches>().is_some_and(|launches| {
        launches
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(id)
            .is_some_and(|slot| slot.stop.is_none())
    })
}

/// Runs a launch from start to finish. Before starting, the mods folder gets a quick look and
/// whatever would stop the game is fixed; after a crash, the loader's report is read and fixed
/// the same way. A crash fixed before the window even opened starts the game again, once.
/// What an instance without memory of its own gets: Wisp's setting, or more for a big modpack.
pub(crate) async fn automatic_memory(game_dir: &Path, setting_mb: u32) -> u32 {
    let game_dir = game_dir.to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        wisp_core::instances::automatic_memory_mb(setting_mb, wisp_core::instances::mod_count(&game_dir), device::total_memory_mb())
    })
    .await
    .unwrap_or(setting_mb)
}

async fn play(
    app: AppHandle,
    id: String,
    client: HttpClient,
    files: GameFiles,
    mut options: LaunchOptions,
    settings: Settings,
    stop_requested: oneshot::Receiver<()>,
) {
    let while_playing = settings.while_playing;
    // Joining a server from the game's own list brings this player's skin (hosting::bring_skin),
    // unless this game signs in through a friend's server already (joined through Wisp).
    let brings_skin = options.skin_server.is_some()
        && !app.state::<SkinServerState>().2.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).contains_key(&id);
    // When the game window came up (0 until it does), for the journal.
    let started_at = AtomicI64::new(0);
    let report = |update: LaunchUpdate| {
        if let LaunchUpdate::Running { started_at: at } = update {
            started_at.store(at, Ordering::Relaxed);
            if let Some(paths) = app.try_state::<Paths>() {
                let _ = paths.instances().mark_played(&id, now_ms());
            }
            sync::game_started(&app);
            match while_playing {
                WhilePlaying::KeepOpen => {}
                WhilePlaying::Minimize => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.minimize();
                    }
                }
                // The window closes outright, so the game gets the memory its web view used; while
                // a server Wisp hosts runs it's only hidden, so nothing can end Wisp (and the
                // server) on the way.
                WhilePlaying::Hide if hosting::any_running(&app) => away::hide_for_hosting(&app),
                WhilePlaying::Hide => away::close_for_game(&app),
            }
        }
        report_launch(&app, &id, update);
    };
    let store = app.try_state::<Paths>().map(|paths| paths.instances());
    let instance_dir = store.as_ref().and_then(|store| store.dir_of(&id));
    // Settings the player changed in their other instances come along (Settings > Game).
    let share_options = settings.share_options;
    if share_options && let Some(paths) = app.try_state::<Paths>() {
        let (data, instances, id) = (paths.data.clone(), paths.instances(), id.clone());
        let _ = tauri::async_runtime::spawn_blocking(move || game_options::before_launch(&data, &instances, &id)).await;
    }
    let system_mb = tauri::async_runtime::spawn_blocking(device::total_memory_mb).await.ok().flatten();
    let curseforge_key = curseforge::api_key(&settings.curseforge_api_key);

    let mut stop_requested = stop_requested;
    let mut retried = false;
    let outcome: Result<Option<(launch::Exit, Option<doctor::Report>)>, launch::LaunchError> = loop {
        let attempt_started = now_ms();
        // Get the game ready and, meanwhile, look over the mods folder.
        let checking = {
            let (instance, dir) = (options.instance.clone(), instance_dir.clone());
            tauri::async_runtime::spawn_blocking(move || dir.map(|dir| doctor::check(&instance, &dir)).unwrap_or_default())
        };
        let ready = tokio::select! {
            ready = async { tokio::join!(launch::prepare(&client, &files, &options, &report), checking) } => Some(ready),
            _ = &mut stop_requested => None,
        };
        let Some((mut prepared, problems)) = ready else {
            break Ok(None);
        };
        let problems = problems.unwrap_or_default();
        if let Some(store) = &store
            && !problems.is_empty()
        {
            report(LaunchUpdate::Fixing { message: fixing_message(doctor::count(&problems)) });
            let fixer = doctor::Fixer {
                client: &client,
                store,
                instance_id: &id,
                concurrency: options.concurrency,
                curseforge_key: curseforge_key.as_deref(),
                memory_mb: options.memory_mb,
                system_mb,
            };
            let mut fixes = tokio::select! {
                fixes = fixer.repair(problems) => fixes,
                _ = &mut stop_requested => break Ok(None),
            };
            if !fixes.changes.is_empty() {
                fixes.entry = record_fix(&app, &id, &fixes, false);
                if let Some(updated) = store.get(&id) {
                    // A newer loader, other memory or another Java changes how the game starts.
                    let restart = fixes.changes.iter().any(|change| {
                        matches!(change, doctor::Change::LoaderChanged { .. } | doctor::Change::JavaChanged { .. } | doctor::Change::MemoryChanged { .. })
                    });
                    options.memory_mb = match updated.memory_mb {
                        Some(own) => own,
                        None => automatic_memory(&options.game_dir, settings.memory_mb).await,
                    };
                    options.instance = updated;
                    if restart {
                        prepared = tokio::select! {
                            again = launch::prepare(&client, &files, &options, &report) => again,
                            _ = &mut stop_requested => break Ok(None),
                        };
                    }
                }
            }
            if !fixes.is_empty() {
                let _ = app.emit(FIXED_EVENT, FixedEvent { instance_id: id.clone(), report: fixes });
            }
        }
        let mut prepared = match prepared {
            Ok(prepared) => prepared,
            Err(error) => break Err(error),
        };
        if brings_skin {
            let (app, id) = (app.clone(), id.clone());
            prepared.on_connect(std::sync::Arc::new(move |address| hosting::bring_skin(&app, &id, address)));
        }
        if let Some(launches) = app.try_state::<Launches>()
            && let Some(slot) = launches
                .0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get_mut(&id)
        {
            (slot.world, slot.server) = match prepared.quick_play() {
                Some(QuickPlay::World(world)) => (Some(world.clone()), None),
                Some(QuickPlay::Server(address)) => (None, Some(address.clone())),
                None => (None, None),
            };
        }
        let exit = match launch::run(prepared, &report, stop_requested, now_ms).await {
            Ok(exit) => exit,
            Err(error) => break Err(error),
        };

        // After a crash, or a load that failed: read what went wrong and fix what can be.
        let failed = !stop_pressed(&app, &id) && (exit.crashed || failed_to_load(&exit.output));
        let (Some(store), Some(dir), true) = (&store, &instance_dir, failed) else {
            break Ok(Some((exit, None)));
        };
        report(LaunchUpdate::Fixing { message: "Working out what went wrong…".into() });
        let problems = {
            let (instance, dir, output) = (options.instance.clone(), dir.clone(), exit.output.clone());
            tauri::async_runtime::spawn_blocking(move || doctor::diagnose(&instance, &dir, &output, attempt_started))
                .await
                .unwrap_or_default()
        };
        if problems.is_empty() {
            break Ok(Some((exit, None)));
        }
        let fixer = doctor::Fixer {
            client: &client,
            store,
            instance_id: &id,
            concurrency: options.concurrency,
            curseforge_key: curseforge_key.as_deref(),
            memory_mb: options.memory_mb,
            system_mb,
        };
        let mut help = fixer.repair(problems).await;
        if !help.changes.is_empty() {
            help.entry = record_fix(&app, &id, &help, true);
        }
        // Fixed before the window even opened, with nothing left over: start again, once.
        let clean = !help.changes.is_empty() && help.offers.is_empty() && help.unsolved.is_empty();
        if clean && !exit.window_seen && !retried {
            retried = true;
            let _ = app.emit(FIXED_EVENT, FixedEvent { instance_id: id.clone(), report: help });
            if let Some(updated) = store.get(&id) {
                options.memory_mb = match updated.memory_mb {
                    Some(own) => own,
                    None => automatic_memory(&options.game_dir, settings.memory_mb).await,
                };
                options.instance = updated;
            }
            let (stop, receiver) = oneshot::channel();
            if let Some(launches) = app.try_state::<Launches>()
                && let Some(slot) = launches
                    .0
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .get_mut(&id)
            {
                slot.stop = Some(stop);
            }
            stop_requested = receiver;
            report(LaunchUpdate::Fixing { message: "Fixed it. Starting again…".into() });
            continue;
        }
        // A load that failed counts as a crash, whatever Java's exit code said.
        break Ok(Some((launch::Exit { crashed: true, ..exit }, Some(help))));
    };
    // Back from wherever it went while the game ran (harmless if the game never started). A window
    // that closed for the game opens again; one opened from the tray meanwhile is just brought up.
    if app.state::<away::Away>().is_away() {
        away::come_back(&app);
    } else if let Some(window) = app.get_webview_window("main")
        && while_playing != WhilePlaying::KeepOpen
    {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    match outcome {
        Ok(result) => {
            let (code, crashed, log_tail, help) = match result {
                Some((exit, help)) => (exit.code, exit.crashed, exit.log_tail, help),
                None => (None, false, Vec::new(), None),
            };
            // The session goes into the journal and played worlds get a restore point, before
            // the launch is over, so the same instance can't start mid-copy.
            let started = started_at.load(Ordering::Relaxed);
            if started > 0 {
                let (app, id) = (app.clone(), id.clone());
                let (instance, game_dir) = (options.instance.clone(), options.game_dir.clone());
                let server = match &options.quick_play {
                    Some(QuickPlay::Server(address)) => Some(address.clone()),
                    _ => None,
                };
                let prefs = settings.journal.clone();
                let ended = now_ms();
                let _ = tauri::async_runtime::spawn_blocking(move || {
                    library::after_session(&app, &instance, &game_dir, started, ended, crashed, server.as_deref(), &prefs, &|update| {
                        report_launch(&app, &id, update)
                    });
                })
                .await;
            }
            // Settings the player changed while it ran go to their other instances.
            if started > 0
                && share_options
                && let Some(paths) = app.try_state::<Paths>()
            {
                let (data, instances, id) = (paths.data.clone(), paths.instances(), id.clone());
                let _ = tauri::async_runtime::spawn_blocking(move || game_options::after_game(&data, &instances, &id)).await;
            }
            report(LaunchUpdate::Exited {
                code,
                crashed,
                log_tail,
                help,
            });
        }
        Err(error) => report(LaunchUpdate::Failed {
            fix: error.fix(),
            message: error.to_string(),
        }),
    }
    if let Some(launches) = app.try_state::<Launches>() {
        launches
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&id);
    }
    // The game ran: what it changed (now in restore points) goes to the other computers.
    if started_at.load(Ordering::Relaxed) > 0 {
        sync::game_closed(&app, &id);
    }
}

#[tauri::command]
async fn stop_instance(id: String, launches: State<'_, Launches>) -> Result<(), String> {
    if let Some(slot) = launches
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get_mut(&id)
        && let Some(stop) = slot.stop.take()
    {
        let _ = stop.send(());
    }
    Ok(())
}

/// What's preparing or running right now, so a reloaded window can pick up where it was.
#[tauri::command]
async fn get_launches(launches: State<'_, Launches>) -> Result<Vec<LaunchEvent>, String> {
    Ok(launches
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .map(|(id, slot)| LaunchEvent {
            instance_id: id.clone(),
            world: slot.world.clone(),
            server: slot.server.clone(),
            update: slot.last.clone(),
        })
        .collect())
}

/// Offline accounts exist for testing until Microsoft sign-in lands, so only developer builds
/// can make them. Release builds will allow them once a Microsoft account owning Minecraft has
/// signed in, as other launchers do.
#[tauri::command]
async fn add_offline_account(name: String, paths: State<'_, Paths>) -> Result<Account, String> {
    if !OFFLINE_ACCOUNTS {
        return Err(
            "Offline accounts unlock after you sign in with a Microsoft account that owns Minecraft."
                .into(),
        );
    }
    paths
        .accounts()
        .add_offline(&name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn remove_account(id: String, paths: State<'_, Paths>) -> Result<(), String> {
    paths
        .accounts()
        .remove(&id)
        .map_err(|error| error.to_string())
}

/// Minecraft versions a loader supports (`None`: all of them).
#[tauri::command]
async fn get_loader_game_versions(
    loader: Loader,
    network: State<'_, Network>,
) -> Result<Option<Vec<String>>, String> {
    network
        .loader_games
        .get_or_fetch(
            loader.as_str().into(),
            loaders::game_versions(&network.client, loader),
        )
        .await
}

/// A loader's versions for one Minecraft version, newest first.
#[tauri::command]
async fn get_loader_versions(
    loader: Loader,
    game_version: String,
    network: State<'_, Network>,
) -> Result<Vec<LoaderVersion>, String> {
    let key = format!("{}/{game_version}", loader.as_str());
    network
        .loader_versions
        .get_or_fetch(
            key,
            loaders::loader_versions(&network.client, loader, &game_version),
        )
        .await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstancePatch {
    name: Option<String>,
    /// An empty string clears the icon.
    icon: Option<String>,
}

#[tauri::command]
async fn update_instance(
    id: String,
    patch: InstancePatch,
    paths: State<'_, Paths>,
) -> Result<Instance, String> {
    paths
        .instances()
        .update(&id, |instance| {
            if let Some(name) = patch.name {
                instance.name = name;
            }
            if let Some(icon) = patch.icon {
                instance.icon = (!icon.is_empty()).then_some(icon);
            }
        })
        .map_err(|error| error.to_string())
}

/// Adds the performance mods to an instance, moving a vanilla one to Fabric first.
#[tauri::command]
async fn speed_up_instance(
    instance_id: String,
    paths: State<'_, Paths>,
    settings: State<'_, SettingsState>,
    network: State<'_, Network>,
    launches: State<'_, Launches>,
) -> Result<wisp_core::performance::SpedUp, String> {
    if launches
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains_key(&instance_id)
    {
        return Err("Quit the game first: its mods can't change while it's running.".into());
    }
    let concurrency = current_settings(&settings).concurrent_downloads as usize;
    wisp_core::performance::speed_up(&network.downloads, &paths.instances(), &instance_id, concurrency)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn delete_instance(
    id: String,
    paths: State<'_, Paths>,
    launches: State<'_, Launches>,
) -> Result<(), String> {
    if launches
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains_key(&id)
    {
        return Err("Stop the game before deleting its instance.".into());
    }
    paths
        .instances()
        .delete(&id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn get_project(
    id: String,
    network: State<'_, Network>,
    settings: State<'_, SettingsState>,
) -> Result<ProjectDetails, String> {
    let result = match curseforge::mod_id(&id) {
        Some(mod_id) => {
            curseforge::project(&network.client, &curseforge_key(&settings)?, mod_id).await
        }
        None => modrinth::project(&network.client, &id).await,
    };
    result.map_err(|error| error.to_string())
}

#[tauri::command]
async fn get_project_versions(
    id: String,
    loaders: Vec<String>,
    game_versions: Vec<String>,
    network: State<'_, Network>,
    settings: State<'_, SettingsState>,
) -> Result<Vec<ProjectVersion>, String> {
    let loaders: Vec<&str> = loaders.iter().map(String::as_str).collect();
    let game_versions: Vec<&str> = game_versions.iter().map(String::as_str).collect();
    let result = match curseforge::mod_id(&id) {
        Some(mod_id) => {
            let key = curseforge_key(&settings)?;
            curseforge::versions(&network.client, &key, mod_id, &loaders, &game_versions).await
        }
        None => modrinth::project_versions(&network.client, &id, &loaders, &game_versions).await,
    };
    result.map_err(|error| error.to_string())
}

#[tauri::command]
async fn get_projects(
    ids: Vec<String>,
    network: State<'_, Network>,
    settings: State<'_, SettingsState>,
) -> Result<Vec<ProjectSummary>, String> {
    let (curse, modrinth_ids): (Vec<String>, Vec<String>) = ids
        .into_iter()
        .partition(|id| curseforge::is_curseforge(id));
    let mut found = modrinth::projects(&network.client, &modrinth_ids)
        .await
        .map_err(|error| error.to_string())?;
    let curse: Vec<u32> = curse
        .iter()
        .filter_map(|id| curseforge::mod_id(id))
        .collect();
    if !curse.is_empty() {
        let key = curseforge_key(&settings)?;
        found.extend(
            curseforge::summaries(&network.client, &key, &curse)
                .await
                .map_err(|error| error.to_string())?,
        );
    }
    Ok(found)
}

#[tauri::command]
async fn get_categories(network: State<'_, Network>) -> Result<Vec<Category>, String> {
    network
        .categories
        .get_or_fetch(modrinth::categories(&network.client))
        .await
}

#[tauri::command]
async fn get_loader_tags(network: State<'_, Network>) -> Result<Vec<LoaderTag>, String> {
    network
        .loader_tags
        .get_or_fetch(modrinth::loader_tags(&network.client))
        .await
}

#[tauri::command]
async fn get_instance_content(
    id: String,
    paths: State<'_, Paths>,
) -> Result<Vec<InstalledContent>, String> {
    let dir = paths
        .instances()
        .dir_of(&id)
        .ok_or("That instance no longer exists.")?;
    Ok(ContentIndex::new(dir).list())
}

/// Installs a mod, resource pack or shader (and what it requires) into an instance.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri hands state over as arguments.
async fn install_project(
    instance_id: String,
    project_id: String,
    version_id: Option<String>,
    world: Option<String>,
    app: AppHandle,
    paths: State<'_, Paths>,
    settings: State<'_, SettingsState>,
    network: State<'_, Network>,
) -> Result<Vec<InstalledContent>, String> {
    let instances = paths.instances();
    let instance = instances
        .get(&instance_id)
        .ok_or("That instance no longer exists.")?;
    let dir = instances
        .dir_of(&instance_id)
        .ok_or("That instance no longer exists.")?;
    let settings = current_settings(&settings);
    let key = curseforge::api_key(&settings.curseforge_api_key);
    let installed = content::install(
        &network.downloads,
        &instance,
        &dir,
        content::InstallRequest {
            project_id: &project_id,
            version_id: version_id.as_deref(),
            world: world.as_deref(),
            concurrency: settings.concurrent_downloads as usize,
            curseforge_key: key.as_deref(),
        },
    )
    .await
    .map_err(|error| error.to_string())?;
    if let Some(item) = installed.iter().find(|item| item.project_id == project_id) {
        library::note_event(
            &app,
            Some(&instance_id),
            world.as_deref(),
            JournalEvent::ContentAdded {
                project_id: item.project_id.clone(),
                title: item.title.clone(),
                project_type: item.project_type.as_str().to_string(),
                version: item.version_number.clone(),
                dependencies: installed.len().saturating_sub(1) as u32,
            },
        );
    }
    Ok(installed)
}

#[tauri::command]
async fn remove_content(
    instance_id: String,
    project_id: String,
    world: Option<String>,
    app: AppHandle,
    paths: State<'_, Paths>,
) -> Result<(), String> {
    let dir = paths
        .instances()
        .dir_of(&instance_id)
        .ok_or("That instance no longer exists.")?;
    let index = ContentIndex::new(dir);
    let removed = index
        .list()
        .into_iter()
        .find(|item| item.project_id == project_id && item.world == world);
    index
        .remove(&project_id, world.as_deref())
        .map_err(|error| error.to_string())?;
    if let Some(item) = removed {
        library::note_event(
            &app,
            Some(&instance_id),
            world.as_deref(),
            JournalEvent::ContentRemoved {
                project_id: item.project_id,
                title: item.title,
                project_type: item.project_type.as_str().to_string(),
            },
        );
    }
    Ok(())
}

/// Installs a Modrinth modpack as a new instance, in the background. Progress arrives as
/// "install" events; the job id is the project id.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri hands state over as arguments.
async fn install_modpack(
    project_id: String,
    version_id: Option<String>,
    title: String,
    app: AppHandle,
    paths: State<'_, Paths>,
    settings: State<'_, SettingsState>,
    network: State<'_, Network>,
    installs: State<'_, Installs>,
) -> Result<String, String> {
    let job_id = project_id.clone();
    start_modpack_job(app, &paths, &settings, &network, &installs, job_id, title, PackSource::Project { project_id, version_id })
}

/// Installs a modpack file you have (a `.mrpack`, or a CurseForge zip) as a new instance.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri hands state over as arguments.
async fn install_modpack_file(
    path: String,
    title: String,
    app: AppHandle,
    paths: State<'_, Paths>,
    settings: State<'_, SettingsState>,
    network: State<'_, Network>,
    installs: State<'_, Installs>,
) -> Result<String, String> {
    let job_id = format!("file:{path}");
    start_modpack_job(app, &paths, &settings, &network, &installs, job_id, title, PackSource::File(PathBuf::from(path)))
}

/// Where a modpack comes from.
enum PackSource {
    Project { project_id: String, version_id: Option<String> },
    File(PathBuf),
}

/// Starts installing a modpack as a new instance in the background; progress goes out on the
/// install event, keyed by the job id this returns.
#[allow(clippy::too_many_arguments)]
fn start_modpack_job(
    app: AppHandle,
    paths: &Paths,
    settings: &SettingsState,
    network: &Network,
    installs: &Installs,
    job_id: String,
    title: String,
    source: PackSource,
) -> Result<String, String> {
    {
        let mut active = installs
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if active.contains_key(&job_id) {
            return Err(format!("{title} is already installing."));
        }
        active.insert(
            job_id.clone(),
            InstallEvent {
                job_id: job_id.clone(),
                title: title.clone(),
                update: InstallUpdate::Preparing,
            },
        );
    }
    let settings = current_settings(settings);
    let concurrency = settings.concurrent_downloads as usize;
    let client = network.downloads.clone();
    let instances = paths.instances();
    let cache = paths.data.join("cache").join("modpacks");
    let job = job_id.clone();
    tauri::async_runtime::spawn(async move {
        let report = |update: InstallUpdate| {
            let event = InstallEvent {
                job_id: job.clone(),
                title: title.clone(),
                update,
            };
            if let Some(installs) = app.try_state::<Installs>() {
                let mut active = installs
                    .0
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if matches!(
                    event.update,
                    InstallUpdate::Done { .. } | InstallUpdate::Failed { .. }
                ) {
                    active.remove(&job);
                } else {
                    active.insert(job.clone(), event.clone());
                }
            }
            let _ = app.emit(INSTALL_EVENT, event);
        };
        let key = curseforge::api_key(&settings.curseforge_api_key);
        let result = match &source {
            PackSource::Project { project_id, version_id } => {
                let request = modpack::Request {
                    project_id,
                    version_id: version_id.as_deref(),
                    cache_dir: &cache,
                    concurrency,
                    now_ms: now_ms(),
                    curseforge_key: key.as_deref(),
                };
                modpack::install(&client, &instances, request, &report).await
            }
            PackSource::File(path) => {
                let request = modpack::FileRequest {
                    path,
                    concurrency,
                    now_ms: now_ms(),
                    curseforge_key: key.as_deref(),
                };
                modpack::install_file(&client, &instances, request, &report).await
            }
        };
        match result {
            Ok(installed) => {
                // Only fills in an options.txt if the modpack didn't bring its own.
                seed_options(&instances, &settings, &installed.instance.id);
                let version = installed
                    .instance
                    .modpack
                    .as_ref()
                    .map(|origin| origin.version_number.clone())
                    .unwrap_or_default();
                library::note_event(
                    &app,
                    Some(&installed.instance.id),
                    None,
                    JournalEvent::ModpackInstalled {
                        title: installed.instance.name.clone(),
                        version,
                    },
                );
                report(InstallUpdate::Done {
                    instance_id: installed.instance.id,
                    skipped: installed.skipped,
                })
            }
            Err(error) => report(InstallUpdate::Failed {
                message: error.to_string(),
            }),
        }
    });
    Ok(job_id)
}

#[tauri::command]
async fn get_installs(installs: State<'_, Installs>) -> Result<Vec<InstallEvent>, String> {
    Ok(installs
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .values()
        .cloned()
        .collect())
}

/// Installs Apple's Rosetta, for Minecraft versions that only run on Intel Macs. macOS asks for
/// the person's password first; `false` means they cancelled.
#[tauri::command]
async fn install_rosetta() -> Result<bool, String> {
    if !cfg!(target_os = "macos") {
        return Err("Rosetta is only for Macs.".into());
    }
    launch::macos::install_rosetta().await
}

#[tauri::command]
async fn get_system_memory() -> Result<Option<u64>, String> {
    static MEMORY: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    if let Some(known) = MEMORY.get() {
        return Ok(*known);
    }
    let found = tauri::async_runtime::spawn_blocking(device::total_memory_mb)
        .await
        .map_err(|error| error.to_string())?;
    Ok(*MEMORY.get_or_init(|| found))
}

#[tauri::command]
async fn get_java_runtimes(paths: State<'_, Paths>) -> Result<Vec<InstalledRuntime>, String> {
    let dir = paths.data.join("runtimes");
    tauri::async_runtime::spawn_blocking(move || launch::installed_runtimes(&dir))
        .await
        .map_err(|error| error.to_string())
}

fn games_running(launches: &Launches) -> bool {
    !launches
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .is_empty()
}

#[tauri::command]
async fn delete_java_runtime(
    component: String,
    platform: String,
    paths: State<'_, Paths>,
    launches: State<'_, Launches>,
) -> Result<(), String> {
    if games_running(&launches) {
        return Err("Close Minecraft first: a running game may be using this Java.".into());
    }
    launch::remove_runtime(&paths.data.join("runtimes"), &component, &platform)
        .map_err(|error| format!("Couldn't delete it: {error}"))
}

#[tauri::command]
async fn test_java(path: String) -> Result<JavaProbe, String> {
    launch::probe_java(Path::new(path.trim())).await
}

#[tauri::command]
async fn get_storage_usage(paths: State<'_, Paths>) -> Result<StorageUsage, String> {
    let dir = paths.data.clone();
    tauri::async_runtime::spawn_blocking(move || storage::usage(&dir))
        .await
        .map_err(|error| error.to_string())
}

/// Deletes installers, modpack downloads and unpacked natives; returns the bytes freed.
#[tauri::command]
async fn clear_cache(
    paths: State<'_, Paths>,
    launches: State<'_, Launches>,
    installs: State<'_, Installs>,
) -> Result<u64, String> {
    if games_running(&launches) {
        return Err("Close Minecraft first: a running game uses some of these files.".into());
    }
    let installing = !installs
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .is_empty();
    if installing {
        return Err("Wait for installs to finish first.".into());
    }
    let dir = paths.data.clone();
    tauri::async_runtime::spawn_blocking(move || storage::clear_cache(&dir))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| format!("Couldn't clear everything: {error}"))
}

/// The skin server (see wisp_core::skin_server), started with the first game on an offline account
/// or the first hosted server (None inside if it couldn't start: games then use the default
/// skins); the friends playing on servers hosted here, whom it answers for too; and the host skin
/// servers that games joining friends' servers sign in through instead, by instance.
#[derive(Default)]
struct SkinServerState(
    tokio::sync::OnceCell<Option<wisp_core::skin_server::SkinServer>>,
    wisp_core::skin_server::Guests,
    SyncMutex<HashMap<String, wisp_core::skin_server::SkinServer>>,
    /// Other Wisp hosts' skin keys the game trusts (learned from their servers' list answers).
    wisp_core::skin_server::Trusted,
);

/// The seed tester builds make their shared skin key from (a build secret; see
/// scripts/tester-updates.mjs). WISP_SKIN_SEED in the environment stands in, for trying it out.
const SKIN_SEED: Option<&str> = option_env!("WISP_SKIN_SEED");

fn skin_seed() -> Option<String> {
    std::env::var("WISP_SKIN_SEED").ok().or_else(|| SKIN_SEED.map(str::to_string)).map(|seed| seed.trim().to_string()).filter(|seed| !seed.is_empty())
}

/// Where the skin keys learned from other Wisp hosts are kept.
fn trusted_keys_path(data: &Path) -> PathBuf {
    data.join("skins").join("trusted-keys.json")
}

/// Trusts another Wisp host's skin key from now on (kept for next time; the newest 64).
pub(crate) fn trust_skin_key(app: &AppHandle, key: &str) {
    if key.is_empty() || key.len() > 4096 {
        return;
    }
    let trusted = app.state::<SkinServerState>().3.clone();
    let list = {
        let mut list = trusted.write().unwrap_or_else(|poisoned| poisoned.into_inner());
        if list.iter().any(|known| known == key) {
            return;
        }
        list.push(key.to_string());
        let extra = list.len().saturating_sub(64);
        list.drain(..extra);
        list.clone()
    };
    if let Ok(text) = serde_json::to_string(&list) {
        let _ = std::fs::write(trusted_keys_path(&app.state::<Paths>().data), text);
    }
}

/// The skin worn in Wisp, read fresh.
fn worn_skin(data: &Path) -> Option<wisp_core::skin_server::Skin> {
    // The worn skin with the worn cosmetic over it, as players see it.
    let (png, slim) = SkinStore::new(data.join("skins")).worn_png()?;
    Some(wisp_core::skin_server::Skin { png, slim })
}

/// The skin server, started if it isn't yet.
async fn skin_server(app: &AppHandle) -> Option<wisp_core::skin_server::SkinServer> {
    let state = app.state::<SkinServerState>();
    let data = app.state::<Paths>().data.clone();
    state.0.get_or_init(|| start_skin_server(data, state.1.clone(), state.3.clone())).await.clone()
}

/// Starts the skin server: it answers for the account playing, wearing the skin worn in Wisp, and
/// for friends playing on a server hosted here. Anyone else asked for by name is an offline
/// player (a server looks names up to put them on its whitelist).
async fn start_skin_server(data: PathBuf, guests: wisp_core::skin_server::Guests, trusted: wisp_core::skin_server::Trusted) -> Option<wisp_core::skin_server::SkinServer> {
    use wisp_core::skin_server::{self, Player, Who};
    // Tester builds sign with the key they all share (so every tester's game trusts every
    // tester's server); others with this computer's own.
    let key_path = data.join("skins").join("signing-key.der");
    let key = tauri::async_runtime::spawn_blocking(move || match skin_seed() {
        Some(seed) => skin_server::shared_key(seed.as_bytes()),
        None => skin_server::load_or_make_key(&key_path),
    })
    .await
    .ok()?
    .ok()?;
    // The keys learned from other Wisp hosts, and the shared one (for builds that sign with their
    // own but meet testers' servers).
    if let Ok(text) = std::fs::read_to_string(trusted_keys_path(&data))
        && let Ok(known) = serde_json::from_str::<Vec<String>>(&text)
    {
        trusted.write().unwrap_or_else(|poisoned| poisoned.into_inner()).extend(known);
    }
    let lookup: skin_server::Lookup = std::sync::Arc::new(move |who| {
        if let Some(account) = AccountStore::new(data.join("accounts.json")).active() {
            let uuid = account.uuid.replace('-', "").to_lowercase();
            let matches = match who {
                Who::Uuid(asked) => asked.replace('-', "").eq_ignore_ascii_case(&uuid),
                Who::Name(asked) | Who::Joining(asked) => asked.eq_ignore_ascii_case(&account.name),
            };
            if matches {
                return Some(Player { uuid, name: account.name, skin: worn_skin(&data) });
            }
        }
        let found = {
            let guests = guests.read().unwrap_or_else(|poisoned| poisoned.into_inner());
            match who {
                Who::Uuid(asked) => guests.get(&asked.replace('-', "").to_lowercase()).cloned(),
                Who::Name(asked) | Who::Joining(asked) => guests.values().find(|guest| guest.name.eq_ignore_ascii_case(asked)).cloned(),
            }
        };
        if found.is_some() {
            return found;
        }
        // Anyone else asked for by name is an offline player (a server looks names up for its
        // whitelist). Not someone logging in: the skin server waits a moment for their skin.
        match who {
            Who::Name(name) if wisp_core::accounts::valid_username(name) => Some(Player { uuid: wisp_core::accounts::offline_uuid(name), name: name.to_string(), skin: None }),
            _ => None,
        }
    });
    skin_server::start_with(key, lookup, None, Some(trusted), skin_server::HAND_IN_WAIT).await.ok()
}

/// Whether this build can update itself (see `main`).
struct UpdatesOn(bool);

/// Whether this build can update itself: release builds can, developer builds can't.
#[tauri::command]
fn updates_on(updates: State<'_, UpdatesOn>) -> bool {
    updates.0
}

/// Writes down any mistake Wisp makes (a panic) in `logs/problems.log`, with where it happened, so
/// one can be found and fixed. The job it happened in stops; Wisp keeps going.
fn note_problems(data: &std::path::Path) {
    let log = data.join("logs").join("problems.log");
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let place = info.location().map(|place| format!("{}:{}", place.file(), place.line())).unwrap_or_default();
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|text| text.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_default();
        let line = format!("{} {} [{}] {message} at {place}\n", now_ms(), env!("CARGO_PKG_VERSION"), thread.name().unwrap_or("?"));
        if let Some(folder) = log.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&log) {
            use std::io::Write;
            let _ = file.write_all(line.as_bytes());
        }
        previous(info);
    }));
}

fn main() {
    let network = Network {
        client: http_client().expect("failed to create the HTTP client"),
        downloads: download_client().expect("failed to create the download client"),
        categories: Cached::new(),
        loader_tags: Cached::new(),
        curseforge_categories: KeyedCache::new(),
        loader_games: KeyedCache::new(),
        loader_versions: KeyedCache::new(),
        modpacks: Cached::new(),
        news: Cached::new(),
        versions: Cached::new(),
    };

    let context = tauri::generate_context!();
    // Updates are signed, so they only turn on in builds whose config carries the signing key's
    // public half (release builds made by the release workflow). Developer builds never replace
    // themselves.
    let updates = context
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|updater| updater.get("pubkey"))
        .and_then(|key| key.as_str())
        .is_some_and(|key| !key.trim().is_empty());
    let builder = tauri::Builder::default()
        // First, so a second launch stops before anything starts: two copies would share one data
        // folder, each think only its own servers run, and both answer friends (one saying a
        // server isn't running). Opening Wisp again brings up the one already running instead.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            match app.get_webview_window(away::MAIN) {
                Some(window) => {
                    let _ = window.unminimize();
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                // Closed for a game or a server: open it again, as the tray does.
                None => away::come_back(app),
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .register_asynchronous_uri_scheme_protocol(protocol::SCHEME, protocol::handle)
        .manage(network)
        .manage(Launches::default())
        .manage(away::Away::default())
        .manage(UpdatesOn(updates))
        .manage(SkinServerState::default())
        .manage(Installs::default())
        .manage(multiplayer::StatusCache::default())
        .manage(sync::SyncHub::default())
        .manage(hosting::Hosting::default())
        .setup(|app| {
            let data = app.path().app_data_dir()?;
            note_problems(&data);
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            app.manage(SettingsState(SyncMutex::new(Settings::load(
                &settings_path,
            ))));
            app.manage(social::Social::new(data.join("social")));
            app.manage(Paths {
                data,
                settings: settings_path,
            });
            playit::manage(app);
            hosting::start_with_wisp(app.handle());
            social::start_if_enabled(app.handle());
            sync::start(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_home,
            get_settings,
            set_settings,
            get_versions,
            create_instance,
            search_projects,
            open_instance_folder,
            get_data_folder,
            open_data_folder,
            get_featured_modpacks,
            get_java_news,
            launch_instance,
            stop_instance,
            get_launches,
            updates_on,
            add_offline_account,
            remove_account,
            get_loader_game_versions,
            get_loader_versions,
            update_instance,
            speed_up_instance,
            delete_instance,
            get_project,
            get_project_versions,
            get_projects,
            get_categories,
            get_loader_tags,
            get_curseforge_status,
            get_curseforge_categories,
            get_instance_worlds,
            get_instance_content,
            install_project,
            remove_content,
            instance_page::get_instance_items,
            instance_page::get_instance_updates,
            instance_page::set_item_enabled,
            instance_page::remove_item,
            instance_page::put_back_item,
            instance_page::install_item_version,
            instance_page::update_all_items,
            instance_page::get_instance_stats,
            instance_page::get_local_settings,
            instance_page::set_local_settings,
            instance_page::set_instance_memory,
            instance_page::get_instance_memory,
            instance_page::set_instance_java,
            instance_page::plan_instance_move,
            instance_page::move_instance,
            instance_page::duplicate_instance,
            instance_page::export_instance,
            instance_page::get_instance_logs,
            instance_page::read_instance_log,
            instance_page::check_pack_updates,
            instance_page::update_instance_pack,
            install_modpack,
            install_modpack_file,
            imports::pick_import_files,
            imports::inspect_import_files,
            imports::add_content_files,
            get_installs,
            install_rosetta,
            get_system_memory,
            get_java_runtimes,
            delete_java_runtime,
            test_java,
            get_storage_usage,
            clear_cache,
            library::get_world_extras,
            library::open_world_folder,
            library::import_world,
            library::import_world_paths,
            library::export_world,
            library::delete_world,
            library::get_screenshots,
            library::get_screenshot_bytes,
            library::delete_screenshot,
            library::reveal_screenshot,
            library::open_screenshots_folder,
            library::get_journal,
            library::add_note,
            library::edit_note,
            library::delete_note,
            library::get_restore_points,
            library::get_restore_point_usage,
            library::create_restore_point,
            library::restore_world,
            library::delete_restore_point,
            skins::get_skins,
            skins::get_skin_png,
            skins::get_skin_layers,
            skins::set_skin_layers,
            skins::add_skin,
            skins::update_skin,
            skins::delete_skin,
            skins::set_active_skin,
            skins::add_cosmetic,
            skins::get_cosmetic_png,
            skins::rename_cosmetic,
            skins::delete_cosmetic,
            skins::wear_cosmetic,
            skins::fetch_player_skin,
            skins::pick_skin_files,
            skins::import_skin_paths,
            skins::export_skin,
            skins::get_skin_template,
            skins::save_skin_template,
            skins::skin_from_painting,
            social::get_social,
            social::set_social_enabled,
            social::set_social_profile,
            social::set_share_activity,
            social::add_friend,
            social::accept_friend,
            social::decline_friend,
            social::remove_friend,
            social::create_party,
            social::invite_to_party,
            social::join_party,
            social::decline_party,
            social::leave_party,
            social::decline_server_invite,
            social::get_chat,
            social::send_chat,
            hosting::get_hosted_servers,
            hosting::put_back_server_mod,
            hosting::set_up_server_address,
            hosting::get_server_software_versions,
            hosting::get_server_content,
            hosting::pick_server_icon,
            hosting::join_server_address,
            hosting::look_up_server_code,
            hosting::join_server_code,
            hosting::create_server_from_modpack,
            hosting::add_server_files,
            hosting::search_server_content,
            hosting::install_server_content,
            hosting::get_server_content_updates,
            hosting::update_server_content,
            hosting::set_server_content_enabled,
            hosting::remove_server_content,
            hosting::check_server_address,
            playit::get_playit,
            playit::link_playit,
            playit::unlink_playit,
            hosting::get_hosted_server,
            hosting::create_hosted_server,
            hosting::update_hosted_server,
            hosting::delete_hosted_server,
            hosting::open_server_folder,
            hosting::start_hosted_server,
            hosting::stop_hosted_server,
            hosting::restart_hosted_server,
            hosting::agree_server_eula,
            hosting::send_server_command,
            hosting::get_server_console,
            hosting::get_server_properties,
            hosting::set_server_properties,
            hosting::get_server_players,
            hosting::set_server_player,
            hosting::set_server_world,
            hosting::get_server_backups,
            hosting::back_up_server,
            hosting::restore_server_backup,
            hosting::play_hosted_server,
            hosting::invite_to_server,
            hosting::join_friend_server,
            multiplayer::get_server_statuses,
            multiplayer::get_public_servers,
            multiplayer::add_server,
            multiplayer::remove_server,
            fixes::undo_fix,
            fixes::apply_fix,
            shared_options::get_instances_keeping_settings,
            shared_options::set_instance_keeps_settings,
            shared_options::share_instance_settings,
            sync::get_sync,
            sync::get_sync_folders,
            sync::pick_sync_folder,
            sync::start_sync,
            sync::stop_sync,
            sync::sync_now,
            sync::resolve_sync,
            sync::bring_sync_item,
            sync::remove_sync_item,
            sync::check_launch,
            sync::open_sync_folder,
            tester_updates::install_tester_update,
            tester_updates::get_tester_code,
            tester_updates::join_tester_builds
        ]);
    let builder = if updates {
        let updater = tauri_plugin_updater::Builder::new();
        // Tester builds count their build numbers too: 0.1.8+145 follows 0.1.8+144.
        let updater = if tester_updates::on() {
            updater.default_version_comparator(|current, release| {
                let offered = &release.version;
                tester_updates::newer(
                    (current.major, current.minor, current.patch, current.build.as_str()),
                    (offered.major, offered.minor, offered.patch, offered.build.as_str()),
                )
            })
        } else {
            updater
        };
        builder.plugin(updater.build())
    } else {
        builder
    };
    builder
        .build(context)
        .expect("failed to start Wisp")
        .run(|app, event| match event {
            // The window closing for a game (or a running server) isn't quitting: Wisp waits in the tray.
            tauri::RunEvent::ExitRequested { code: None, api, .. } if app.state::<away::Away>().closing_for_game() => api.prevent_exit(),
            // The window's close button while a server runs: the window hides, the server keeps
            // going, and the tray brings the window back. (Quitting on purpose still quits, below.)
            tauri::RunEvent::WindowEvent { label, event: tauri::WindowEvent::CloseRequested { api, .. }, .. } if label == "main" && hosting::any_running(app) => {
                api.prevent_close();
                away::hide_for_hosting(app);
            }
            // Quitting with servers running: they stop (and save) first.
            tauri::RunEvent::ExitRequested { api, .. } if hosting::hold_exit_for_servers(app) => api.prevent_exit(),
            // Clicking the Dock icon while the window is closed for a game opens it.
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => away::come_back(app),
            _ => {}
        });
}
