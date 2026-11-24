//! Servers hosted on this computer, and joining friends' servers.
//!
//! Hosting: a server is set up (the official server software for its loader, downloaded the
//! first time), started, watched (console, who's on), and stopped carefully, with a restore point
//! of its world each time it stops. While it runs, friends can ask to join through the friends
//! channel: Wisp checks they may, lets them in by name, opens the link between the two computers
//! (see wisp_core::tunnel) and hands them the list of the instance's mods.
//!
//! Joining: the friend's Wisp asks, meets the host's computer over the link, makes or updates an
//! instance that matches the server (downloading what Modrinth serves, taking the rest over the
//! link), and starts the game straight into the server through the link. When the game closes,
//! the link closes too.

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as SyncMutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;
use wisp_core::hosting::install::{self, PrepareOptions};
use wisp_core::hosting::content;
use wisp_core::hosting::mods::{self, SetAside};
use wisp_core::hosting::pack::{self, Pack};
use wisp_core::hosting::players::{self, List};
use wisp_core::hosting::process::{self, ConsoleLine, Exit, RunState, ServerEvent, ServerProcess};
use wisp_core::hosting::properties::Properties;
use wisp_core::hosting::{self as core_hosting, HostedServer, NewServer, ServerStore, world};
use wisp_core::instances::{GAME_DIR, Loader, NewInstance};
use wisp_core::launch::{self, Download, LaunchUpdate, Progress};
use wisp_core::snapshots::{NewSnapshot, SnapshotReason, Snapshots, Summary};
use wisp_core::social::{Message, Player, ServerCard};
use wisp_core::tunnel::door::Door;
use wisp_core::tunnel::{self, Endpoint, Keys, Link, Offer, OpenOptions, Reach, Service, Upload};

use crate::library::blocking;
use crate::{Launches, Network, OFFLINE_ACCOUNTS, Paths, SettingsState, now_ms, social};

/// Sent when a hosted server changes: its state, its console, who's on, setting-up progress.
pub const HOSTING_EVENT: &str = "hosting";
/// Sent while joining a friend's server.
pub const JOIN_EVENT: &str = "join";
const GONE: &str = "That server no longer exists.";
const PORT_TAKEN: &str = "Something else on this computer is using the server's port. Change the port in its settings, or close the other program.";
const STOP_FIRST: &str = "Stop the server first.";
/// How long a friend's Wisp waits for the host to answer.
const ANSWER_WITHIN: Duration = Duration::from_secs(40);

/// A server running, or being set up, on this computer.
struct Live {
    /// Set once Java has started.
    process: Option<Arc<ServerProcess>>,
    /// The latest setting-up step, while there's no process yet.
    preparing: Option<LaunchUpdate>,
    /// The link friends come in through, opened when the first one asks.
    link: Option<Arc<Endpoint>>,
    /// Who the server is on links, and whom it lets in (the link and the door share them).
    keys: Keys,
    /// The server's port, where Wisp takes players and passes them on to the server (see
    /// wisp_core::tunnel::door); friends' Wisps come in there too when the link can't form.
    door: Option<Door>,
    service: Arc<Service>,
    /// Friends let in this run (friend codes).
    friends: Vec<String>,
    /// The router's opening for the server's port, when the server asks for one.
    port_mapping: Option<Arc<tunnel::upnp::Mapping>>,
    tasks: Vec<JoinHandle<()>>,
}

impl Live {
    fn preparing(keys: Keys) -> Self {
        Self { process: None, preparing: Some(LaunchUpdate::Checking), link: None, keys, door: None, service: Arc::new(Service::default()), friends: Vec::new(), port_mapping: None, tasks: Vec::new() }
    }
}

/// A friend's server this computer is playing on: the link, the game's way into it, and its way
/// to the host's skin server.
struct Joined {
    link: Link,
    /// The direct link's socket (none when joined through a server's address).
    endpoint: Option<Arc<Endpoint>>,
    forwarding: tokio::task::JoinHandle<()>,
    skins: Option<tokio::task::JoinHandle<()>>,
}

/// What the host said to an ask to join.
pub struct Answer {
    pub offer: Option<Offer>,
    pub server: Option<ServerCard>,
    pub error: Option<String>,
    /// Still starting: ask again in a moment.
    pub starting: bool,
}

#[derive(Default)]
pub struct Hosting {
    live: SyncMutex<HashMap<String, Live>>,
    last_exit: SyncMutex<HashMap<String, (Exit, Option<String>)>>,
    /// Asks to join waiting for the host's answer, by session.
    asks: SyncMutex<HashMap<String, oneshot::Sender<Answer>>>,
    /// How many times each server's code has been announced: a retry gives way to anything newer.
    code_said: SyncMutex<HashMap<String, u64>>,
    /// Friends' servers being played, by the instance playing them.
    joined: SyncMutex<HashMap<String, Joined>>,
    /// One link set up at a time.
    opening: tokio::sync::Mutex<()>,
    /// Quitting: servers are stopping, then Wisp exits.
    quitting: AtomicBool,
    /// Starts again in a row after Wisp fixed a server that crashed as it loaded, by server.
    fixes: SyncMutex<HashMap<String, u32>>,
    /// What Wisp did, for the console of the server's next start.
    notes: SyncMutex<HashMap<String, Vec<String>>>,
}

fn lock<T>(mutex: &SyncMutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn store(paths: &Paths) -> ServerStore {
    ServerStore::new(paths.data.join("servers"))
}

/// Restore points of servers' worlds, kept apart from the instances' ones.
fn restore_points(paths: &Paths) -> Snapshots {
    Snapshots::new(paths.data.join("servers").join(".restore-points"))
}

fn snapshot_key(id: &str) -> String {
    format!("server:{id}")
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HostingEvent {
    server_id: String,
    #[serde(flatten)]
    change: Change,
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", rename_all_fields = "camelCase")]
enum Change {
    /// Something about the server changed: read it again.
    Changed,
    Progress { update: LaunchUpdate },
    Lines { lines: Vec<ConsoleLine> },
    Players { players: Vec<String> },
    Failed { message: String },
    /// Wisp fixed something about the server by itself (and says what).
    Fixed { message: String },
}

fn emit(app: &AppHandle, id: &str, change: Change) {
    let _ = app.emit(HOSTING_EVENT, HostingEvent { server_id: id.to_string(), change });
}

// ── What the screens show ──

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostedView {
    #[serde(flatten)]
    server: HostedServer,
    /// "stopped", "preparing", "starting", "running", "stopping" or "crashed".
    state: &'static str,
    progress: Option<LaunchUpdate>,
    players: Vec<String>,
    max_players: u32,
    port: u16,
    /// Where people on the same network join.
    lan_address: Option<String>,
    /// Where anyone joins, when the router opened the port.
    public_address: Option<String>,
    started_at: Option<i64>,
    ready_at: Option<i64>,
    exit: Option<Exit>,
    /// Why it couldn't start, if it couldn't.
    failure: Option<String>,
    /// The memory it gets (its own setting, or what Wisp picked), and whether Wisp picked it.
    effective_memory_mb: u32,
    automatic_memory: bool,
    /// What Wisp would pick (shown beside your own amount).
    picked_memory_mb: u32,
    mod_count: usize,
    motd: String,
    online_mode: bool,
    whitelist: bool,
    world_exists: bool,
    instance_name: Option<String>,
    /// How players sign in: "mojang" (Microsoft accounts), "wisp" (through Wisp's skin server, in
    /// builds with offline accounts) or "offline" (nobody's checked).
    accounts: &'static str,
    /// Friends let in this run.
    friends_in: Vec<String>,
    /// How friends reach it, once a friend has asked.
    reach: Option<Reach>,
    /// Mods Wisp keeps out of it (they only run in the game).
    set_aside: Vec<SetAside>,
    /// Its join code: anyone with Wisp joins with it.
    code: Option<String>,
}

fn state_name(live: Option<&Live>, exit: Option<&Exit>) -> &'static str {
    match live {
        Some(live) => match &live.process {
            None => "preparing",
            Some(process) => match process.state() {
                RunState::Starting => "starting",
                RunState::Running => "running",
                RunState::Stopping => "stopping",
                RunState::Crashed => "crashed",
                RunState::Stopped => "stopped",
            },
        },
        None if exit.is_some_and(|exit| exit.crashed) => "crashed",
        None => "stopped",
    }
}

fn view(app: &AppHandle, server: HostedServer) -> HostedView {
    let paths = app.state::<Paths>();
    let hosting = app.state::<Hosting>();
    let store = store(&paths);
    let dir = store.root().join(&server.id);
    let properties = Properties::read(&dir);
    let port = store.port_of(&server.id).unwrap_or(core_hosting::DEFAULT_PORT);
    let mod_count = core_hosting::mod_count(&dir);
    // Plugin servers count their plugins (their memory still follows their mods: none).
    let shown_count = if server.software.is_some() { content::list(&server, &dir).iter().filter(|item| item.enabled).count() } else { mod_count };
    let picked_memory_mb = core_hosting::automatic_memory_mb(mod_count, wisp_core::device::total_memory_mb());
    let effective_memory_mb = server.memory_mb.unwrap_or(picked_memory_mb);
    let (exit, failure) = lock(&hosting.last_exit).get(&server.id).cloned().map_or((None, None), |(exit, failure)| (Some(exit), failure));
    let live = lock(&hosting.live);
    let current = live.get(&server.id);
    let state = state_name(current, exit.as_ref());
    let process = current.and_then(|live| live.process.clone());
    HostedView {
        state,
        progress: current.and_then(|live| live.preparing.clone()),
        players: process.as_ref().map(|process| process.players()).unwrap_or_default(),
        max_players: properties.get("max-players").and_then(|value| value.trim().parse().ok()).unwrap_or(20),
        port,
        lan_address: tunnel::lan_address().map(|ip| format!("{ip}:{port}")),
        public_address: current.and_then(|live| live.port_mapping.as_ref().map(|mapping| mapping.external().to_string())),
        started_at: process.as_ref().map(|process| process.started_at()),
        ready_at: process.as_ref().and_then(|process| process.ready_at()),
        exit: if current.is_some() { None } else { exit },
        failure: if current.is_some() { None } else { failure },
        effective_memory_mb,
        automatic_memory: server.memory_mb.is_none(),
        picked_memory_mb,
        mod_count: shown_count,
        motd: properties.get("motd").unwrap_or_default().to_string(),
        online_mode: properties.get("online-mode").is_none_or(|value| value.trim() == "true"),
        whitelist: properties.get("white-list").is_some_and(|value| value.trim() == "true"),
        world_exists: dir.join(world::WORLD).join("level.dat").is_file(),
        accounts: match properties.get("online-mode").is_none_or(|value| value.trim() == "true") {
            true if signs_in_through_wisp() => "wisp",
            true => "mojang",
            false => "offline",
        },
        instance_name: server.instance_id.as_deref().and_then(|id| paths.instances().get(id)).map(|instance| instance.name),
        friends_in: current.map(|live| live.friends.clone()).unwrap_or_default(),
        reach: current.and_then(|live| live.link.as_ref().map(|link| link.reach())),
        set_aside: mods::set_aside_list(&dir),
        code: code_of(&dir),
        server,
    }
}

// ── Plugins and mods ──

/// A server's plugins or mods.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerContentView {
    /// "plugins", "mods", or nothing for plain Vanilla.
    kind: Option<&'static str>,
    items: Vec<content::ServerItem>,
    /// The instance its mods come from, when it has one (they're changed there).
    instance_name: Option<String>,
    instance_id: Option<String>,
}

fn content_view(app: &AppHandle, server: &HostedServer) -> ServerContentView {
    let dir = store(&app.state::<Paths>()).root().join(&server.id);
    let instance = server.instance_id.as_deref().and_then(|id| app.state::<Paths>().instances().get(id));
    ServerContentView {
        kind: content::folder_of(server),
        items: content::list(server, &dir),
        instance_name: instance.as_ref().map(|instance| instance.name.clone()),
        instance_id: instance.map(|instance| instance.id),
    }
}

#[tauri::command]
pub async fn get_server_content(id: String, app: AppHandle) -> Result<ServerContentView, String> {
    let server = store(&app.state::<Paths>()).get(&id).ok_or(GONE)?;
    Ok(content_view(&app, &server))
}

/// Plugins (or mods) on Modrinth that run on this server: its software and Minecraft version.
#[tauri::command]
pub async fn search_server_content(
    id: String,
    query: String,
    offset: u32,
    sort: Option<wisp_core::modrinth::SearchSort>,
    categories: Option<Vec<String>>,
    app: AppHandle,
) -> Result<wisp_core::modrinth::SearchResults, String> {
    let server = store(&app.state::<Paths>()).get(&id).ok_or(GONE)?;
    let (project_type, loaders) = content::kind_of(&server).ok_or("This server takes neither plugins nor mods.")?;
    let search = wisp_core::modrinth::SearchQuery {
        source: Default::default(),
        query,
        project_type,
        sort: sort.unwrap_or_default(),
        loaders: loaders.iter().map(|loader| loader.to_string()).collect(),
        categories: categories.unwrap_or_default().into_iter().take(8).collect(),
        game_version: Some(server.game_version.clone()),
        // A modded server only wants mods that do something there.
        environment: (project_type == wisp_core::modrinth::ProjectType::Mod).then(|| "server".to_string()),
        offset,
        limit: 24,
    };
    wisp_core::modrinth::search(&app.state::<Network>().client, &search).await.map_err(|error| error.to_string())
}

/// What in the server has a newer build for it on Modrinth.
#[tauri::command]
pub async fn get_server_content_updates(id: String, app: AppHandle) -> Result<Vec<content::Update>, String> {
    let store = store(&app.state::<Paths>());
    let server = store.get(&id).ok_or(GONE)?;
    let dir = store.dir_of(&id).ok_or(GONE)?;
    content::updates(&app.state::<Network>().client, &server, &dir).await
}

/// Installs newer builds: the ones named, or every one there is.
#[tauri::command]
pub async fn update_server_content(id: String, files: Option<Vec<String>>, app: AppHandle) -> Result<ServerContentView, String> {
    let store = store(&app.state::<Paths>());
    let server = store.get(&id).ok_or(GONE)?;
    let dir = store.dir_of(&id).ok_or(GONE)?;
    let network = app.state::<Network>();
    let updates = content::updates(&network.client, &server, &dir).await?;
    let mut failed = Vec::new();
    for update in updates.iter().filter(|update| files.as_ref().is_none_or(|files| files.contains(&update.file))) {
        if let Err(error) = content::apply_update(&network.downloads, &server, &dir, update).await {
            failed.push(format!("{}: {error}", update.file));
        }
    }
    emit(&app, &id, Change::Changed);
    if !failed.is_empty() {
        return Err(failed.join(" "));
    }
    Ok(content_view(&app, &server))
}

#[tauri::command]
pub async fn install_server_content(id: String, project_id: String, app: AppHandle) -> Result<ServerContentView, String> {
    let store = store(&app.state::<Paths>());
    let server = store.get(&id).ok_or(GONE)?;
    let dir = store.dir_of(&id).ok_or(GONE)?;
    content::install(&app.state::<Network>().downloads, &server, &dir, &project_id).await?;
    emit(&app, &id, Change::Changed);
    Ok(content_view(&app, &server))
}

#[tauri::command]
pub async fn set_server_content_enabled(id: String, file: String, enabled: bool, app: AppHandle) -> Result<ServerContentView, String> {
    let store = store(&app.state::<Paths>());
    let server = store.get(&id).ok_or(GONE)?;
    let dir = store.dir_of(&id).ok_or(GONE)?;
    content::set_enabled(&server, &dir, &file, enabled).map_err(|error| error.to_string())?;
    emit(&app, &id, Change::Changed);
    Ok(content_view(&app, &server))
}

#[tauri::command]
pub async fn remove_server_content(id: String, file: String, app: AppHandle) -> Result<ServerContentView, String> {
    let store = store(&app.state::<Paths>());
    let server = store.get(&id).ok_or(GONE)?;
    let dir = store.dir_of(&id).ok_or(GONE)?;
    let path = content::path_of(&server, &dir, &file).ok_or("That's not there anymore.")?;
    blocking(move || crate::library::move_to_trash(&path)).await?;
    content::forget(&dir, &file);
    emit(&app, &id, Change::Changed);
    Ok(content_view(&app, &server))
}

/// Picks a picture for the server's icon in the game's server list.
#[tauri::command]
pub async fn pick_server_icon(id: String, app: AppHandle) -> Result<bool, String> {
    use tauri_plugin_dialog::DialogExt;
    let dir = store(&app.state::<Paths>()).dir_of(&id).ok_or(GONE)?;
    let picked = app.dialog().file().set_title("Server icon").add_filter("Picture", &["png", "jpg", "jpeg"]).blocking_pick_file();
    let Some(path) = picked.and_then(|file| file.into_path().ok()) else {
        return Ok(false);
    };
    blocking(move || core_hosting::set_icon(&dir, &path)).await?;
    emit(&app, &id, Change::Changed);
    Ok(true)
}

/// Puts a mod Wisp set aside back in the server (it starts with it from then on).
#[tauri::command]
pub async fn put_back_server_mod(id: String, file: String, app: AppHandle) -> Result<HostedView, String> {
    let store = store(&app.state::<Paths>());
    let dir = store.dir_of(&id).ok_or(GONE)?;
    blocking(move || mods::put_back(&dir, &file).map_err(|error| error.to_string())).await?;
    emit(&app, &id, Change::Changed);
    Ok(view(&app, store.get(&id).ok_or(GONE)?))
}

#[tauri::command]
pub async fn get_hosted_servers(app: AppHandle) -> Result<Vec<HostedView>, String> {
    let servers = store(&app.state::<Paths>()).list();
    Ok(servers.into_iter().map(|server| view(&app, server)).collect())
}

#[tauri::command]
pub async fn get_hosted_server(id: String, app: AppHandle) -> Result<HostedView, String> {
    let server = store(&app.state::<Paths>()).get(&id).ok_or(GONE)?;
    Ok(view(&app, server))
}

// ── Making and changing servers ──

/// A world to bring into a new server: a singleplayer world from an instance.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldSource {
    instance_id: String,
    folder: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateServer {
    name: String,
    game_version: String,
    loader: Loader,
    loader_version: Option<String>,
    /// The instance people play it with: its mods and configs go to the server.
    instance_id: Option<String>,
    world: Option<WorldSource>,
    #[serde(default)]
    properties: Vec<(String, String)>,
    /// The player ticked "I agree to the Minecraft EULA".
    eula: bool,
    /// A plugin server (Paper, Purpur, Folia, Spigot) instead of plain Vanilla.
    #[serde(default)]
    software: Option<wisp_core::hosting::software::Software>,
}

/// The Minecraft versions a plugin server has builds for, newest first (empty: any release).
#[tauri::command]
pub async fn get_server_software_versions(software: wisp_core::hosting::software::Software, app: AppHandle) -> Result<Vec<String>, String> {
    software.versions(&app.state::<Network>().client).await.map_err(|error| error.to_string())
}

/// Builds where only offline accounts play (developer and tester builds, until Microsoft sign-in)
/// sign players in through Wisp's skin server instead of Mojang's: the server still checks who's
/// joining, and everyone there sees everyone's skin.
fn signs_in_through_wisp() -> bool {
    OFFLINE_ACCOUNTS
}

#[tauri::command]
pub async fn create_hosted_server(request: CreateServer, app: AppHandle) -> Result<HostedView, String> {
    if !request.eula {
        return Err("Agree to the Minecraft EULA to host a server.".into());
    }
    let paths = app.state::<Paths>();
    let instances = paths.instances();
    let instance = match &request.instance_id {
        Some(id) => Some(instances.get(id).ok_or("That instance no longer exists.")?),
        None => None,
    };
    // A server made from an instance runs what the instance does.
    let (game_version, loader, loader_version) = match &instance {
        Some(instance) => (instance.game_version.clone(), instance.loader, instance.loader_version.clone()),
        None => (request.game_version.clone(), request.loader, request.loader_version.clone()),
    };
    let loader_version = match (loader, loader_version) {
        (Loader::Vanilla, _) => None,
        (_, Some(version)) => Some(version),
        (_, None) => wisp_core::loaders::recommended(&app.state::<Network>().client, loader, &game_version).await.map_err(|error| error.to_string())?,
    };
    if loader != Loader::Vanilla && loader_version.is_none() {
        return Err(format!("{} has no build for Minecraft {game_version}.", loader.display_name()));
    }
    let mut properties = vec![("online-mode".to_string(), "true".to_string())];
    if signs_in_through_wisp() {
        // Signed chat needs keys only Mojang hands out.
        properties.push(("enforce-secure-profile".to_string(), "false".to_string()));
    }
    properties.extend(request.properties.into_iter().filter(|(key, _)| key != "server-port"));
    let world_source = match &request.world {
        Some(source) => {
            if crate::library::busy(&app.state::<Launches>(), &source.instance_id) {
                return Err("Close the game first: the world is open in it.".into());
            }
            let game_dir = instances.game_dir(&source.instance_id).ok_or("That instance no longer exists.")?;
            Some(wisp_core::worlds::world_dir(&game_dir, &source.folder).ok_or("That world no longer exists.")?)
        }
        None => None,
    };
    let store = store(&paths);
    // A plugin server (Paper and the rest) runs plain Minecraft for players, with no instance's mods.
    let software = request.software.filter(|_| instance.is_none() && loader == Loader::Vanilla);
    let new = NewServer {
        name: request.name,
        game_version,
        loader,
        loader_version,
        instance_id: request.instance_id.clone(),
        icon: instance.as_ref().and_then(|instance| instance.icon.clone()).filter(|icon| icon.starts_with("wisp:")),
        properties,
        software,
    };
    let server = store.create(new, now_ms()).map_err(|error| error.to_string())?;
    let server = store.update(&server.id, |server| server.eula = true).map_err(|error| error.to_string())?;
    let dir = store.dir_of(&server.id).ok_or(GONE)?;
    if let Some(instance_dir) = request.instance_id.as_deref().and_then(|id| instances.dir_of(id)) {
        let game_dir = instance_dir.join(GAME_DIR);
        let target = dir.clone();
        blocking(move || pack::copy_configs(&game_dir, &target).map_err(|error| error.to_string())).await?;
    }
    if let Some(source) = world_source {
        let host = paths.accounts().active().map(|account| account.uuid);
        let target = dir.clone();
        blocking(move || world::import(&source, &target, host.as_deref()).map(|_| ()).map_err(|error| format!("Couldn't copy the world: {error}"))).await?;
    }
    emit(&app, &server.id, Change::Changed);
    Ok(view(&app, server))
}

/// A server made from a modpack file you have.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerFromPack {
    /// A `.mrpack`, or a CurseForge zip.
    path: String,
    name: String,
    #[serde(default)]
    properties: Vec<(String, String)>,
    eula: bool,
}

/// Makes a server from a modpack file: its version and loader now, and on its first start the pack
/// becomes an instance (so players get what it has) and the server runs its server-side mods, with
/// the pack's own server files.
#[tauri::command]
pub async fn create_server_from_modpack(request: ServerFromPack, app: AppHandle) -> Result<HostedView, String> {
    if !request.eula {
        return Err("Agree to the Minecraft EULA to host a server.".into());
    }
    let source = PathBuf::from(&request.path);
    let inspected = {
        let source = source.clone();
        blocking(move || Ok(wisp_core::imports::inspect(&source))).await?
    };
    let summary = inspected.pack.ok_or_else(|| inspected.problem.unwrap_or_else(|| format!("{} isn't a modpack Wisp can read (a .mrpack, or a CurseForge zip).", inspected.file_name)))?;
    let mut properties = vec![("online-mode".to_string(), "true".to_string())];
    if signs_in_through_wisp() {
        properties.push(("enforce-secure-profile".to_string(), "false".to_string()));
    }
    properties.extend(request.properties.into_iter().filter(|(key, _)| key != "server-port"));
    let store = store(&app.state::<Paths>());
    let name = if request.name.trim().is_empty() { summary.name.clone().unwrap_or_else(|| inspected.file_name.clone()) } else { request.name };
    let new = NewServer { name, game_version: summary.game_version.clone(), loader: summary.loader, loader_version: None, instance_id: None, icon: None, properties, software: None };
    let server = store.create(new, now_ms()).map_err(|error| error.to_string())?;
    let dir = store.dir_of(&server.id).ok_or(GONE)?;
    let extension = if inspected.file_name.to_ascii_lowercase().ends_with(".mrpack") { "mrpack" } else { "zip" };
    let kept = format!(".wisp-modpack.{extension}");
    {
        let target = dir.join(&kept);
        blocking(move || std::fs::copy(&source, target).map(|_| ()).map_err(|error| format!("Couldn't keep the modpack: {error}"))).await?;
    }
    let server = store
        .update(&server.id, |server| {
            server.eula = true;
            server.pack_file = Some(kept.clone());
        })
        .map_err(|error| error.to_string())?;
    emit(&app, &server.id, Change::Changed);
    Ok(view(&app, server))
}

/// Adds plugin or mod files (and data packs) to a server, picked now or dropped on its page. A
/// server made from an instance takes its mods through the instance.
#[tauri::command]
pub async fn add_server_files(id: String, files: Option<Vec<String>>, app: AppHandle) -> Result<Vec<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let store = store(&app.state::<Paths>());
    let server = store.get(&id).ok_or(GONE)?;
    let dir = store.dir_of(&id).ok_or(GONE)?;
    let files: Vec<PathBuf> = match files {
        Some(files) => files.into_iter().map(PathBuf::from).collect(),
        None => app
            .dialog()
            .file()
            .set_title("Add files")
            .add_filter("Plugins, mods and data packs", &["jar", "zip"])
            .blocking_pick_files()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|file| file.into_path().ok())
            .collect(),
    };
    let paths = app.state::<Paths>();
    let instance = server.instance_id.as_deref().and_then(|instance| Some((paths.instances().get(instance)?, paths.instances().dir_of(instance)?)));
    let network = app.state::<Network>();
    let mut added = Vec::new();
    let mut failed = Vec::new();
    for file in files {
        let name = file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let is_jar = name.to_ascii_lowercase().ends_with(".jar");
        let outcome = match &instance {
            // Mods go into the instance, where players get them too (the server follows it).
            Some((instance, instance_dir)) if is_jar => wisp_core::imports::add_to_instance(&network.client, instance, instance_dir, &file).await.map(|done| vec![done.content.title]).map_err(|error| error.to_string()),
            _ => {
                let (server, dir) = (server.clone(), dir.clone());
                blocking(move || content::add_file(&server, &dir, &file)).await
            }
        };
        match outcome {
            Ok(names) => added.extend(names),
            Err(error) => failed.push(error),
        }
    }
    emit(&app, &id, Change::Changed);
    if added.is_empty() && !failed.is_empty() {
        return Err(failed.join(" "));
    }
    Ok(added)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerChanges {
    name: Option<String>,
    friends_join: Option<bool>,
    open_port: Option<bool>,
    backups: Option<bool>,
    /// Memory in MB; `automatic_memory` set lets Wisp pick.
    memory_mb: Option<u32>,
    #[serde(default)]
    automatic_memory: bool,
    /// The address people join from anywhere (empty takes it away).
    address: Option<String>,
    start_with_wisp: Option<bool>,
    /// 0 turns scheduled restarts off.
    restart_every_hours: Option<u32>,
    aikar_flags: Option<bool>,
    /// Empty takes them away.
    jvm_args: Option<String>,
    /// 0 lets Wisp pick.
    java_major: Option<u32>,
}

#[tauri::command]
pub async fn update_hosted_server(id: String, changes: ServerChanges, app: AppHandle) -> Result<HostedView, String> {
    let address = match changes.address.as_deref().map(str::trim) {
        Some("") => Some(None),
        Some(text) => {
            let parsed = wisp_core::servers::ServerAddress::parse(text).map_err(|_| format!("\"{text}\" isn't an address players can join. It looks like play.example.com or 147.185.221.20:25565."))?;
            Some(Some(match parsed.port {
                Some(port) => format!("{}:{port}", parsed.host),
                None => parsed.host,
            }))
        }
        None => None,
    };
    let server = store(&app.state::<Paths>())
        .update(&id, |server| {
            if let Some(address) = address {
                server.address = address;
            }
            if let Some(start) = changes.start_with_wisp {
                server.start_with_wisp = start;
            }
            if let Some(hours) = changes.restart_every_hours {
                server.restart_every_hours = (hours > 0).then_some(hours.min(168));
            }
            if let Some(aikar) = changes.aikar_flags {
                server.aikar_flags = aikar;
            }
            if let Some(args) = &changes.jvm_args {
                let args = args.trim();
                server.jvm_args = (!args.is_empty()).then(|| args.chars().take(2000).collect());
            }
            if let Some(major) = changes.java_major {
                server.java_major = (major > 0).then_some(major);
            }
            if let Some(name) = changes.name {
                server.name = name;
            }
            if let Some(friends_join) = changes.friends_join {
                server.friends_join = friends_join;
            }
            if let Some(open_port) = changes.open_port {
                server.open_port = open_port;
            }
            if let Some(backups) = changes.backups {
                server.backups = backups;
            }
            if changes.automatic_memory {
                server.memory_mb = None;
            } else if let Some(memory) = changes.memory_mb {
                server.memory_mb = Some(memory.clamp(512, 64 * 1024));
            }
        })
        .map_err(|error| error.to_string())?;
    // Friends see what they can join, and the code says where it is now.
    social::announce_soon(&app);
    if is_live(&app, &id) {
        announce_code(&app, &id, code_says_now(&app, &id));
    }
    emit(&app, &id, Change::Changed);
    Ok(view(&app, server))
}

fn is_live(app: &AppHandle, id: &str) -> bool {
    lock(&app.state::<Hosting>().live).contains_key(id)
}

#[tauri::command]
pub async fn delete_hosted_server(id: String, app: AppHandle) -> Result<(), String> {
    if is_live(&app, &id) {
        return Err(STOP_FIRST.into());
    }
    let dir = store(&app.state::<Paths>()).dir_of(&id).ok_or(GONE)?;
    blocking(move || crate::library::move_to_trash(&dir)).await?;
    lock(&app.state::<Hosting>().last_exit).remove(&id);
    emit(&app, &id, Change::Changed);
    Ok(())
}

#[tauri::command]
pub async fn open_server_folder(id: String, app: AppHandle) -> Result<(), String> {
    let dir = store(&app.state::<Paths>()).dir_of(&id).ok_or(GONE)?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|error| error.to_string())
}

// ── Starting and stopping ──

#[tauri::command]
pub async fn start_hosted_server(id: String, app: AppHandle) -> Result<HostedView, String> {
    lock(&app.state::<Hosting>().fixes).remove(&id);
    begin(&app, &id)?;
    let server = store(&app.state::<Paths>()).get(&id).ok_or(GONE)?;
    Ok(view(&app, server))
}

/// Sets the server up and starts it, in the background.
fn begin(app: &AppHandle, id: &str) -> Result<(), String> {
    let paths = app.state::<Paths>();
    let server = store(&paths).get(id).ok_or(GONE)?;
    if !server.eula {
        return Err("Agree to the Minecraft EULA for this server first.".into());
    }
    let keys = Keys::new().map_err(|error| error.to_string())?;
    {
        let hosting = app.state::<Hosting>();
        let mut live = lock(&hosting.live);
        if live.contains_key(id) {
            return Err(format!("{} is already running.", server.name));
        }
        live.insert(id.to_string(), Live::preparing(keys));
        lock(&hosting.last_exit).remove(id);
    }
    let task = {
        let app = app.clone();
        let id = id.to_string();
        tauri::async_runtime::spawn(async move {
            if let Err(message) = start(&app, &id).await {
                let hosting = app.state::<Hosting>();
                lock(&hosting.live).remove(&id);
                lock(&hosting.last_exit).insert(id.clone(), (Exit { code: None, crashed: true, reason: None }, Some(message.clone())));
                emit(&app, &id, Change::Failed { message });
                emit(&app, &id, Change::Changed);
            }
        })
    };
    if let Some(live) = lock(&app.state::<Hosting>().live).get_mut(id) {
        live.tasks.push(task);
    }
    emit(app, id, Change::Changed);
    Ok(())
}

/// Installs a server's modpack as an instance (players join with it), points the server at it, and
/// lays out the pack's own server files.
async fn install_pack(app: &AppHandle, id: &str, dir: &std::path::Path, pack_file: &str, report: &(dyn Fn(LaunchUpdate) + Send + Sync)) -> Result<HostedServer, String> {
    use wisp_core::modpack::{self, InstallUpdate};
    let paths = app.state::<Paths>();
    let network = app.state::<Network>();
    let settings = crate::current_settings(&app.state::<SettingsState>());
    let pack = dir.join(pack_file);
    let progress = |update: InstallUpdate| match update {
        InstallUpdate::Downloading { done_bytes, total_bytes, done_files, total_files } => report(LaunchUpdate::Downloading { done_bytes, total_bytes, done_files, total_files }),
        InstallUpdate::Unpacking => report(LaunchUpdate::Installing { loader: "the modpack".into() }),
        _ => {}
    };
    report(LaunchUpdate::Installing { loader: "the modpack".into() });
    let key = wisp_core::curseforge::api_key(&settings.curseforge_api_key);
    let request = modpack::FileRequest { path: &pack, concurrency: settings.concurrent_downloads as usize, now_ms: now_ms(), curseforge_key: key.as_deref() };
    let installed = modpack::install_file(&network.downloads, &paths.instances(), request, &progress).await.map_err(|error| format!("Couldn't set up the modpack: {error}"))?;
    let instance = installed.instance;
    let store = store(&paths);
    let server = store
        .update(id, |server| {
            server.instance_id = Some(instance.id.clone());
            server.game_version = instance.game_version.clone();
            server.loader = instance.loader;
            server.loader_version = instance.loader_version.clone();
            server.pack_file = None;
        })
        .map_err(|error| error.to_string())?;
    let (instance_dir, target, pack_path) = (paths.instances().dir_of(&instance.id).ok_or(GONE)?, dir.to_path_buf(), pack.clone());
    blocking(move || {
        pack::copy_configs(&instance_dir.join(GAME_DIR), &target).map_err(|error| error.to_string())?;
        pack::apply_server_overrides(&pack_path, &target).map_err(|error| error.to_string())?;
        let _ = std::fs::remove_file(&pack_path);
        Ok(())
    })
    .await?;
    social::announce_soon(app);
    Ok(server)
}

/// The SHA-1s of files Modrinth says never run on a server.
async fn client_only(network: &Network, shareable: &[wisp_core::inventory::Shareable]) -> HashSet<String> {
    let ids: Vec<String> = shareable.iter().filter_map(|file| file.project_id.clone()).collect::<HashSet<_>>().into_iter().collect();
    let Ok(projects) = wisp_core::modrinth::projects(&network.client, &ids).await else {
        return HashSet::new();
    };
    let unsupported: HashSet<String> = projects.into_iter().filter(|project| project.server_side == "unsupported").map(|project| project.id).collect();
    shareable.iter().filter(|file| file.project_id.as_ref().is_some_and(|id| unsupported.contains(id))).map(|file| file.sha1.clone()).collect()
}

async fn start(app: &AppHandle, id: &str) -> Result<(), String> {
    let paths = app.state::<Paths>();
    let network = app.state::<Network>();
    let store = store(&paths);
    let server = store.get(id).ok_or(GONE)?;
    let dir = store.dir_of(id).ok_or(GONE)?;
    let report = {
        let app = app.clone();
        let id = id.to_string();
        move |update: LaunchUpdate| {
            if let Some(live) = lock(&app.state::<Hosting>().live).get_mut(&id) {
                live.preparing = Some(update.clone());
            }
            emit(&app, &id, Change::Progress { update });
        }
    };
    // Made from a modpack: on its first start, the pack becomes the instance it runs.
    let server = match (&server.pack_file, &server.instance_id) {
        (Some(pack_file), None) => install_pack(app, id, &dir, pack_file, &report).await?,
        _ => server,
    };
    // The server runs what its instance has (minus what only runs in the game).
    if let Some(instance_dir) = server.instance_id.as_deref().and_then(|instance| paths.instances().dir_of(instance)) {
        let scan = crate::instance_page::scanned(&network, &instance_dir).await?;
        let shareable = scan.shareable();
        // Left out: what Modrinth says never runs on a server and what crashed this one, but not
        // what this server turned out to need.
        let mut client_only = client_only(&network, &shareable).await;
        client_only.extend(mods::set_aside_hashes(&dir));
        for needed in mods::needed_hashes(&dir) {
            client_only.remove(&needed);
        }
        let game_dir = instance_dir.join(GAME_DIR);
        let target = dir.clone();
        blocking(move || pack::sync_server_mods(&game_dir, &target, &shareable, &client_only).map(|_| ()).map_err(|error| format!("Couldn't copy the instance's mods: {error}"))).await?;
    }
    let settings = crate::current_settings(&app.state::<SettingsState>());
    let java_paths: HashMap<u32, PathBuf> = settings.java_paths.iter().filter_map(|(major, path)| Some((major.parse().ok()?, PathBuf::from(path)))).collect();
    let memory_mb = server.memory_mb.unwrap_or_else(|| core_hosting::automatic_memory_mb(core_hosting::mod_count(&dir), wisp_core::device::total_memory_mb()));
    let concurrency = settings.concurrent_downloads as usize;
    // Players sign in through the skin server in builds with offline accounts: the server asks it
    // who's joining (through authlib-injector, as the game does), and hands everyone their skins.
    let mut jvm_args = if signs_in_through_wisp() {
        let skins = crate::skin_server(app).await.ok_or("Couldn't start Wisp's skin service, which players on this server sign in through.")?;
        let injector = launch::authlib_injector(&network.downloads, &paths.game_files(), concurrency, &report).await.map_err(|error| error.to_string())?;
        skins.jvm_args(&injector)
    } else {
        Vec::new()
    };
    if server.aikar_flags {
        jvm_args.push(format!("-Xms{memory_mb}M"));
        jvm_args.extend(core_hosting::AIKAR_FLAGS.iter().map(|flag| flag.to_string()));
    }
    // The player's own come last, so theirs win.
    jvm_args.extend(server.jvm_args.as_deref().map(core_hosting::split_args).unwrap_or_default());
    let options = PrepareOptions { memory_mb, java_major: server.java_major, java_paths: &java_paths, jvm_args, concurrency };
    let mut plan = install::prepare(&network.downloads, &paths.game_files(), &server, &dir, &options, &report).await.map_err(|error| error.to_string())?;
    // The player agreed to the EULA in Wisp (start refuses otherwise).
    core_hosting::accept_eula(&dir).map_err(|error| error.to_string())?;

    // The server listens on a port of its own on this computer, and Wisp takes players at the
    // server's port and passes them on: so friends' Wisps come in the same way, mods and skins and
    // all, through whatever reaches the port (a playit tunnel, the router, the same network).
    let (keys, service) = {
        let hosting = app.state::<Hosting>();
        let live = lock(&hosting.live);
        let entry = live.get(id).ok_or("The server stopped.")?;
        (entry.keys.clone(), entry.service.clone())
    };
    let port = store.port_of(id).unwrap_or(core_hosting::DEFAULT_PORT);
    let bind = Properties::read(&dir)
        .get("server-ip")
        .and_then(|ip| ip.trim().parse::<IpAddr>().ok())
        .filter(|ip| !ip.is_unspecified())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let inner_port = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).and_then(|listener| listener.local_addr()).map(|address| address.port()).map_err(|error| format!("Couldn't find a free port for the server: {error}"))?;
    let inner = SocketAddr::new(bind, inner_port);
    *service.target.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(inner);
    let door = Door::open(port, inner, &keys, service).await.map_err(|_| PORT_TAKEN.to_string())?;
    plan.listen_on(inner_port);

    let events = {
        let app = app.clone();
        let id = id.to_string();
        Arc::new(move |event: ServerEvent| match event {
            ServerEvent::Lines { lines } => emit(&app, &id, Change::Lines { lines }),
            ServerEvent::Players { players } => {
                emit(&app, &id, Change::Players { players });
                social::announce_soon(&app);
            }
            ServerEvent::State { .. } => emit(&app, &id, Change::Changed),
        })
    };
    let running = process::start(&plan, now_ms, events).map_err(|error| format!("Couldn't start Java: {error}"))?;
    {
        let hosting = app.state::<Hosting>();
        let mut live = lock(&hosting.live);
        let Some(entry) = live.get_mut(id) else {
            // Stopped while it was being set up.
            running.kill();
            return Ok(());
        };
        entry.process = Some(running.clone());
        entry.preparing = None;
        entry.door = Some(door);
    }
    for note in lock(&app.state::<Hosting>().notes).remove(id).unwrap_or_default() {
        running.note(&note);
    }
    let _ = store.update(id, |server| server.last_started = Some(now_ms()));
    let watcher = {
        let app = app.clone();
        let id = id.to_string();
        tauri::async_runtime::spawn(async move { watch(app, id, running).await })
    };
    if let Some(live) = lock(&app.state::<Hosting>().live).get_mut(id) {
        live.tasks.push(watcher);
    }
    if server.open_port {
        open_port(app, id).await;
    }
    // playit's agent carries players from the server's tunnel (builds that run it, once linked).
    if let Err(message) = crate::playit::server_starting(app).await {
        emit(app, id, Change::Failed { message: format!("The server's playit address isn't working: {message}") });
    }
    emit(app, id, Change::Changed);
    Ok(())
}

/// Asks the router to open the server's port, so anyone with the address can join.
async fn open_port(app: &AppHandle, id: &str) {
    let Some(port) = store(&app.state::<Paths>()).port_of(id) else {
        return;
    };
    let Some(lan) = tunnel::lan_address() else {
        return;
    };
    let mapping = tunnel::upnp::map(tunnel::upnp::Protocol::TCP, SocketAddr::new(lan.into(), port), "Wisp server").await;
    let Some(mapping) = mapping.map(Arc::new) else {
        emit(app, id, Change::Failed { message: "Your router didn't open the port (it may not allow UPnP). Friends can still join through Wisp.".into() });
        return;
    };
    let renewing = {
        let mapping = mapping.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(tunnel::upnp::LEASE / 2).await;
                mapping.renew().await;
            }
        })
    };
    let hosting = app.state::<Hosting>();
    let mut live = lock(&hosting.live);
    match live.get_mut(id) {
        Some(entry) => {
            entry.port_mapping = Some(mapping);
            entry.tasks.push(renewing);
        }
        None => {
            renewing.abort();
            let mapping = mapping.clone();
            tauri::async_runtime::spawn(async move { mapping.remove().await });
        }
    }
}

/// Follows a running server until it ends, then cleans up after it: the link, the router's
/// openings, and a restore point of its world.
async fn watch(app: AppHandle, id: String, running: Arc<ServerProcess>) {
    // Whoever has its code, or is a friend, sees it's on its way up (and joining waits for it).
    announce_code(&app, &id, CodeSays::Starting);
    social::announce_soon(&app);
    // Friends see it once it's up. The link and the mod list are made ready then too, so a
    // friend who asks to join is answered at once.
    {
        let mut waited = 0;
        while running.state() == RunState::Starting && waited < 900 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            waited += 1;
        }
        social::announce_soon(&app);
        if running.state() == RunState::Running
            && let Some(server) = store(&app.state::<Paths>()).get(&id)
        {
            if let Some(hours) = server.restart_every_hours.filter(|hours| *hours > 0) {
                let task = tauri::async_runtime::spawn(restart_later(app.clone(), id.clone(), running.clone(), hours));
                if let Some(live) = lock(&app.state::<Hosting>().live).get_mut(&id) {
                    live.tasks.push(task);
                }
            }
            let _ = ready_for_joins(&app, &server).await;
            let announcing = tauri::async_runtime::spawn(keep_announcing(app.clone(), id.clone()));
            if let Some(live) = lock(&app.state::<Hosting>().live).get_mut(&id) {
                live.tasks.push(announcing);
            }
            if server.friends_join || !server.invited.is_empty() {
                let _ = link_for(&app, &server).await;
            }
        }
    }
    let exit = running.wait().await;
    let ended = lock(&app.state::<Hosting>().live).remove(&id);
    if let Some(ended) = ended {
        if let Some(link) = &ended.link {
            link.close().await;
        }
        if let Some(mapping) = &ended.port_mapping {
            mapping.remove().await;
        }
        for task in ended.tasks {
            task.abort();
        }
    }
    let paths = app.state::<Paths>();
    let store = store(&paths);
    if let (Some(server), Some(dir)) = (store.get(&id), store.dir_of(&id))
        && server.backups
        && dir.join(world::WORLD).join("level.dat").is_file()
    {
        let snapshots = restore_points(&paths);
        let name = server.name.clone();
        let key = snapshot_key(&id);
        let world_dir = dir.join(world::WORLD);
        let _ = blocking(move || {
            snapshots
                .create(&world_dir, NewSnapshot { instance_id: &key, world: world::WORLD, name: &name, reason: SnapshotReason::Auto, label: Some("When the server stopped".into()), now: now_ms() })
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
        .await;
    }
    // Crashed as it loaded: Wisp reads why, fixes what it can and starts it again (a few times in a
    // row at most), saying what it did. What it can't fix it explains.
    let mut explained = None;
    let quitting = app.state::<Hosting>().quitting.load(Ordering::SeqCst);
    if exit.crashed && exit.reason.is_none() && running.ready_at().is_none() && !quitting {
        match fix_after_crash(&app, &id, &running).await {
            AfterCrash::Fixed(notes) => {
                let attempts = {
                    let hosting = app.state::<Hosting>();
                    let mut fixes = lock(&hosting.fixes);
                    let count = fixes.entry(id.clone()).or_insert(0);
                    *count += 1;
                    *count
                };
                if attempts <= 3 {
                    emit(&app, &id, Change::Fixed { message: notes.join(" ") });
                    lock(&app.state::<Hosting>().notes).insert(id.clone(), notes);
                    if begin(&app, &id).is_ok() {
                        return;
                    }
                }
            }
            AfterCrash::Explained(text) => explained = Some(text),
            AfterCrash::Unknown => {}
        }
    }
    let failure = match exit.reason {
        Some("eula") => Some("The server stopped at Minecraft's EULA. Agree to it for this server and start it again.".to_string()),
        Some("port") => Some(PORT_TAKEN.to_string()),
        _ if exit.crashed => Some(match explained {
            Some(text) => format!("{text} Its console shows more."),
            None => "The server stopped unexpectedly. Its console shows the last thing it said.".to_string(),
        }),
        _ => None,
    };
    lock(&app.state::<Hosting>().last_exit).insert(id.clone(), (exit, failure));
    announce_code(&app, &id, CodeSays::Stopped);
    if lock(&app.state::<Hosting>().live).is_empty() {
        crate::playit::servers_stopped(&app).await;
    }
    emit(&app, &id, Change::Changed);
    social::announce_soon(&app);
}

/// Gives the server a playit address of its own (builds that run playit's agent): a tunnel to its
/// port, made on the player's linked playit account.
#[tauri::command]
pub async fn set_up_server_address(id: String, app: AppHandle) -> Result<HostedView, String> {
    let store = store(&app.state::<Paths>());
    let server = store.get(&id).ok_or(GONE)?;
    let port = store.port_of(&id).unwrap_or(core_hosting::DEFAULT_PORT);
    // The other servers' ports here: their tunnels are theirs, even when the account is full.
    let keep_ports: Vec<u16> = store.list().iter().filter(|other| other.id != id).filter_map(|other| store.port_of(&other.id)).collect();
    let address = crate::playit::tunnel_for(&app, &server.name, port, &keep_ports).await?;
    let server = store.update(&id, |server| server.address = Some(address.clone())).map_err(|error| error.to_string())?;
    if is_live(&app, &id) {
        crate::playit::server_starting(&app).await?;
        announce_code(&app, &id, code_says_now(&app, &id));
    }
    emit(&app, &id, Change::Changed);
    Ok(view(&app, server))
}

/// Whether the server's address reaches it from the internet, asked the way a game would.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressCheck {
    reachable: bool,
    /// When it isn't, why that may be.
    hint: Option<String>,
}

#[tauri::command]
pub async fn check_server_address(id: String, app: AppHandle) -> Result<AddressCheck, String> {
    let store = store(&app.state::<Paths>());
    let server = store.get(&id).ok_or(GONE)?;
    let address = server.address.clone().ok_or("Give the server an address first.")?;
    if !process_of(&app, &id).is_some_and(|process| process.state() == RunState::Running) {
        return Ok(AddressCheck { reachable: false, hint: Some("Start the server, then check again: the address only answers while it runs.".into()) });
    }
    let status = wisp_core::servers::status(&address).await;
    Ok(if status.online {
        AddressCheck { reachable: true, hint: None }
    } else {
        AddressCheck {
            reachable: false,
            hint: Some(format!(
                "Nothing answered at {address}. If it's a playit address, check playit is running and its tunnel points at 127.0.0.1, port {}.",
                store.port_of(&id).unwrap_or(core_hosting::DEFAULT_PORT)
            )),
        }
    })
}

/// Restarts a server after so many hours, warning the players on it first.
async fn restart_later(app: AppHandle, id: String, running: Arc<ServerProcess>, hours: u32) {
    let warnings = [(Duration::from_secs(5 * 60), "in 5 minutes"), (Duration::from_secs(60), "in a minute"), (Duration::from_secs(10), "in 10 seconds")];
    let mut left = Duration::from_secs(u64::from(hours) * 3600);
    for (before, when) in warnings {
        let wait = left.saturating_sub(before);
        tokio::time::sleep(wait).await;
        left -= wait;
        if !running.players().is_empty() {
            let _ = running.send(&format!("say The server restarts {when} (it does every {hours} h). Back in a moment.")).await;
        }
    }
    tokio::time::sleep(left).await;
    running.note("Restarting, as scheduled.");
    // On a task of its own: stopping ends the tasks that belong to this run, this one included.
    tauri::async_runtime::spawn(async move {
        let _ = restart_hosted_server(id, app).await;
    });
}

/// Starts the servers set to start whenever Wisp opens.
pub fn start_with_wisp(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // After the window is up, so opening Wisp stays quick.
        tokio::time::sleep(Duration::from_secs(3)).await;
        let servers = store(&app.state::<Paths>()).list();
        for server in servers.into_iter().filter(|server| server.start_with_wisp && server.eula) {
            let _ = begin(&app, &server.id);
        }
    });
}

enum AfterCrash {
    /// Fixed: what Wisp did, one sentence each.
    Fixed(Vec<String>),
    /// What's wrong, when Wisp couldn't fix it.
    Explained(String),
    Unknown,
}

/// Reads why a server crashed as it loaded, and fixes what it can: a mod that only runs in the game
/// is set aside; a mod the server needs that its instance has (left out by a wrong label) comes
/// in; too little memory gets more.
async fn fix_after_crash(app: &AppHandle, id: &str, running: &ServerProcess) -> AfterCrash {
    use wisp_core::doctor::{self, Problem};
    let paths = app.state::<Paths>();
    let store = store(&paths);
    let (Some(server), Some(dir)) = (store.get(id), store.dir_of(id)) else {
        return AfterCrash::Unknown;
    };
    let console: Vec<String> = running.console().into_iter().map(|line| line.text).collect();
    let (since, loader, read_dir) = (running.started_at(), server.loader, dir.clone());
    let Ok(problems) = blocking(move || Ok(doctor::diagnose_server(&read_dir, loader, &console, since))).await else {
        return AfterCrash::Unknown;
    };
    if problems.is_empty() {
        return AfterCrash::Unknown;
    }
    let mut notes = Vec::new();
    for problem in &problems {
        match problem {
            Problem::ClientOnly { file, name } => {
                if mods::set_aside(&dir, file, name, now_ms()).is_ok() {
                    notes.push(format!("{name} only runs in the game, so Wisp set it aside on this server (players still get it)."));
                }
            }
            Problem::Missing { id: wanted, .. } => {
                let instance_dir = server.instance_id.as_deref().and_then(|instance| paths.instances().dir_of(instance));
                let Some(instance_dir) = instance_dir else { continue };
                let wanted = wanted.clone();
                let found = blocking(move || {
                    let mods = doctor::scan(&instance_dir);
                    Ok(mods.into_iter().find(|info| info.answers_to(&wanted)).and_then(|info| pack::hash_file(&instance_dir.join(GAME_DIR).join("mods").join(&info.file)).map(|(sha1, _)| (sha1, info))))
                })
                .await
                .ok()
                .flatten();
                if let Some((sha1, info)) = found
                    && mods::mark_needed(&dir, &sha1).is_ok()
                {
                    let name = if info.name.is_empty() { info.file.clone() } else { info.name.clone() };
                    notes.push(format!("The server needs {name}, so Wisp added it from the instance."));
                }
            }
            Problem::OutOfMemory => {
                let current = server.memory_mb.unwrap_or_else(|| core_hosting::automatic_memory_mb(core_hosting::mod_count(&dir), wisp_core::device::total_memory_mb()));
                let ceiling = wisp_core::device::total_memory_mb().map_or(8192, |total| (total * 6 / 10) as u32) / 512 * 512;
                let target = (current + 2048).min(ceiling) / 512 * 512;
                if target > current && store.update(id, |stored| stored.memory_mb = Some(target)).is_ok() {
                    notes.push(format!("The server ran out of memory, so Wisp gave it {:.1} GB.", f64::from(target) / 1024.0));
                }
            }
            _ => {}
        }
    }
    if !notes.is_empty() {
        notes.push("Starting it again.".into());
        return AfterCrash::Fixed(notes);
    }
    AfterCrash::Explained(problems.iter().map(|problem| problem.describe(loader)).collect::<Vec<_>>().join(" "))
}

fn process_of(app: &AppHandle, id: &str) -> Option<Arc<ServerProcess>> {
    lock(&app.state::<Hosting>().live).get(id).and_then(|live| live.process.clone())
}

#[tauri::command]
pub async fn stop_hosted_server(id: String, app: AppHandle) -> Result<HostedView, String> {
    match process_of(&app, &id) {
        Some(process) => {
            process.stop().await;
        }
        None => {
            // Still being set up: stop that.
            let preparing = lock(&app.state::<Hosting>().live).remove(&id);
            if let Some(preparing) = preparing {
                for task in preparing.tasks {
                    task.abort();
                }
            }
        }
    }
    // The watcher tidies up; give it a moment so the answer shows the server stopped.
    for _ in 0..40 {
        if !is_live(&app, &id) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    emit(&app, &id, Change::Changed);
    let server = store(&app.state::<Paths>()).get(&id).ok_or(GONE)?;
    Ok(view(&app, server))
}

#[tauri::command]
pub async fn restart_hosted_server(id: String, app: AppHandle) -> Result<HostedView, String> {
    stop_hosted_server(id.clone(), app.clone()).await?;
    for _ in 0..100 {
        if !is_live(&app, &id) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    start_hosted_server(id, app).await
}

#[tauri::command]
pub async fn agree_server_eula(id: String, app: AppHandle) -> Result<HostedView, String> {
    let server = store(&app.state::<Paths>()).update(&id, |server| server.eula = true).map_err(|error| error.to_string())?;
    Ok(view(&app, server))
}

#[tauri::command]
pub async fn send_server_command(id: String, command: String, app: AppHandle) -> Result<(), String> {
    let process = process_of(&app, &id).ok_or("The server isn't running.")?;
    process.send(&command).await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_server_console(id: String, app: AppHandle) -> Result<Vec<ConsoleLine>, String> {
    Ok(process_of(&app, &id).map(|process| process.console()).unwrap_or_default())
}

/// Whether any server is running or starting (Wisp keeps going in the tray while one is).
pub fn any_running(app: &AppHandle) -> bool {
    app.try_state::<Hosting>().is_some_and(|hosting| !lock(&hosting.live).is_empty())
}

/// Stops every server carefully (on the way out of Wisp).
pub async fn stop_all(app: &AppHandle) {
    let running: Vec<(String, Option<Arc<ServerProcess>>)> = lock(&app.state::<Hosting>().live).iter().map(|(id, live)| (id.clone(), live.process.clone())).collect();
    let stops = running.into_iter().map(|(id, process)| {
        let app = app.clone();
        async move {
            match process {
                Some(process) => {
                    process.stop().await;
                }
                None => {
                    lock(&app.state::<Hosting>().live).remove(&id);
                }
            }
        }
    });
    futures_join_all(stops).await;
    // Let the watchers take their restore points.
    for _ in 0..100 {
        if !any_running(app) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn futures_join_all<F: std::future::Future<Output = ()> + Send + 'static>(futures: impl Iterator<Item = F>) {
    let handles: Vec<_> = futures.map(tauri::async_runtime::spawn).collect();
    for handle in handles {
        let _ = handle.await;
    }
}

/// Quitting Wisp: stops the servers first (once), then exits. True when the exit should wait.
pub fn hold_exit_for_servers(app: &AppHandle) -> bool {
    let hosting = app.state::<Hosting>();
    if !any_running(app) || hosting.quitting.swap(true, Ordering::SeqCst) {
        return false;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        stop_all(&app).await;
        app.exit(0);
    });
    true
}

// ── Settings, players, world, backups ──

#[tauri::command]
pub async fn get_server_properties(id: String, app: AppHandle) -> Result<Vec<(String, String)>, String> {
    let dir = store(&app.state::<Paths>()).dir_of(&id).ok_or(GONE)?;
    Ok(Properties::read(&dir).pairs())
}

/// Changes server.properties. A running server reads it when it starts, so changes wait for a
/// restart (the screen says so).
#[tauri::command]
pub async fn set_server_properties(id: String, values: Vec<(String, String)>, app: AppHandle) -> Result<Vec<(String, String)>, String> {
    let dir = store(&app.state::<Paths>()).dir_of(&id).ok_or(GONE)?;
    let mut properties = Properties::read(&dir);
    for (key, value) in values {
        if !core_hosting::is_property_key(&key) {
            return Err(format!("\"{key}\" isn't a server setting."));
        }
        if key == "server-port" {
            let port: u16 = value.trim().parse().map_err(|_| "The port is a number from 1024 to 65535.")?;
            if port < 1024 {
                return Err("The port is a number from 1024 to 65535.".into());
            }
        }
        properties.set(&key, value.trim_end_matches(['\r', '\n']));
    }
    properties.write(&dir).map_err(|error| error.to_string())?;
    emit(&app, &id, Change::Changed);
    social::announce_soon(&app);
    Ok(properties.pairs())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayersView {
    whitelist: Vec<players::Entry>,
    ops: Vec<players::Entry>,
    banned: Vec<players::Entry>,
    online: Vec<String>,
}

#[tauri::command]
pub async fn get_server_players(id: String, app: AppHandle) -> Result<PlayersView, String> {
    let dir = store(&app.state::<Paths>()).dir_of(&id).ok_or(GONE)?;
    Ok(PlayersView {
        whitelist: players::read(&dir, List::Whitelist),
        ops: players::read(&dir, List::Ops),
        banned: players::read(&dir, List::Banned),
        online: process_of(&app, &id).map(|process| process.players()).unwrap_or_default(),
    })
}

/// Puts someone on (or takes them off) the whitelist, operators or bans: through the console
/// while the server runs, in its files while it's stopped.
#[tauri::command]
pub async fn set_server_player(id: String, list: List, name: String, add: bool, app: AppHandle) -> Result<PlayersView, String> {
    let name = name.trim().to_string();
    if !wisp_core::accounts::valid_username(&name) {
        return Err("Minecraft names are 3 to 16 letters, numbers or underscores.".into());
    }
    let paths = app.state::<Paths>();
    let dir = store(&paths).dir_of(&id).ok_or(GONE)?;
    match process_of(&app, &id).filter(|process| !process.state().is_over()) {
        Some(process) => {
            process.send(&list.command(add, &name)).await.map_err(|error| error.to_string())?;
            // The server writes its file a moment later.
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
        None if add => {
            let online = Properties::read(&dir).get("online-mode").is_none_or(|value| value.trim() == "true");
            let entry = if online && !signs_in_through_wisp() {
                players::mojang_profile(&app.state::<Network>().client, &name).await?.ok_or(format!("Mojang has no player called {name}."))?
            } else {
                players::Entry { uuid: wisp_core::accounts::offline_uuid(&name), name: name.clone() }
            };
            players::add(&dir, list, &entry.name, &entry.uuid, now_ms()).map_err(|error| error.to_string())?;
        }
        None => {
            players::remove(&dir, list, &name).map_err(|error| error.to_string())?;
        }
    }
    emit(&app, &id, Change::Changed);
    get_server_players(id, app).await
}

/// Replaces the server's world with a copy of a singleplayer world (the old one is kept in the
/// server's folder, and as a restore point).
#[tauri::command]
pub async fn set_server_world(id: String, source: WorldSource, app: AppHandle) -> Result<HostedView, String> {
    if is_live(&app, &id) {
        return Err(STOP_FIRST.into());
    }
    if crate::library::busy(&app.state::<Launches>(), &source.instance_id) {
        return Err("Close the game first: the world is open in it.".into());
    }
    let paths = app.state::<Paths>();
    let store = store(&paths);
    let server = store.get(&id).ok_or(GONE)?;
    let dir = store.dir_of(&id).ok_or(GONE)?;
    let game_dir = paths.instances().game_dir(&source.instance_id).ok_or("That instance no longer exists.")?;
    let from = wisp_core::worlds::world_dir(&game_dir, &source.folder).ok_or("That world no longer exists.")?;
    let host = paths.accounts().active().map(|account| account.uuid);
    let snapshots = restore_points(&paths);
    let key = snapshot_key(&id);
    blocking(move || {
        let current = dir.join(world::WORLD);
        if current.join("level.dat").is_file() {
            snapshots
                .create(&current, NewSnapshot { instance_id: &key, world: world::WORLD, name: &server.name, reason: SnapshotReason::Manual, label: Some("Before the world was replaced".into()), now: now_ms() })
                .map_err(|error| error.to_string())?;
        }
        world::import(&from, &dir, host.as_deref()).map(|_| ()).map_err(|error| format!("Couldn't copy the world: {error}"))
    })
    .await?;
    emit(&app, &id, Change::Changed);
    get_hosted_server(id, app).await
}

#[tauri::command]
pub async fn get_server_backups(id: String, app: AppHandle) -> Result<Vec<Summary>, String> {
    let snapshots = restore_points(&app.state::<Paths>());
    let key = snapshot_key(&id);
    blocking(move || Ok(snapshots.for_world(&key, world::WORLD))).await
}

#[tauri::command]
pub async fn back_up_server(id: String, app: AppHandle) -> Result<Summary, String> {
    let paths = app.state::<Paths>();
    let store = store(&paths);
    let server = store.get(&id).ok_or(GONE)?;
    let world_dir = store.dir_of(&id).ok_or(GONE)?.join(world::WORLD);
    if !world_dir.join("level.dat").is_file() {
        return Err("The server has no world yet: it makes one the first time it starts.".into());
    }
    // A running server is asked to write everything out first.
    if let Some(process) = process_of(&app, &id) {
        let _ = process.send("save-all flush").await;
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    let snapshots = restore_points(&paths);
    let key = snapshot_key(&id);
    blocking(move || {
        snapshots
            .create(&world_dir, NewSnapshot { instance_id: &key, world: world::WORLD, name: &server.name, reason: SnapshotReason::Manual, label: None, now: now_ms() })
            .map_err(|error| error.to_string())
    })
    .await
}

#[tauri::command]
pub async fn restore_server_backup(id: String, backup: String, app: AppHandle) -> Result<(), String> {
    if is_live(&app, &id) {
        return Err(STOP_FIRST.into());
    }
    let paths = app.state::<Paths>();
    let store = store(&paths);
    let name = store.get(&id).ok_or(GONE)?.name;
    let world_dir = store.dir_of(&id).ok_or(GONE)?.join(world::WORLD);
    let snapshots = restore_points(&paths);
    let key = snapshot_key(&id);
    blocking(move || {
        if !snapshots.for_world(&key, world::WORLD).iter().any(|summary| summary.id == backup) {
            return Err("That backup belongs to another server.".to_string());
        }
        // The world as it is goes into the backups first, so going back can be undone.
        if world_dir.join("level.dat").is_file() {
            snapshots
                .create(&world_dir, NewSnapshot { instance_id: &key, world: world::WORLD, name: &name, reason: SnapshotReason::BeforeRestore, label: Some("Before going back to an older backup".into()), now: now_ms() })
                .map_err(|error| error.to_string())?;
        }
        snapshots.restore(&backup, &world_dir).map(|_| ()).map_err(|error| error.to_string())
    })
    .await?;
    emit(&app, &id, Change::Changed);
    Ok(())
}

// ── Playing on your own server ──

/// Starts the game straight into one of your servers, with the instance it's played with (made
/// from the server's mods the first time, when it has none).
#[tauri::command]
pub async fn play_hosted_server(id: String, app: AppHandle) -> Result<String, String> {
    let paths = app.state::<Paths>();
    let store = store(&paths);
    let server = store.get(&id).ok_or(GONE)?;
    let port = store.port_of(&id).unwrap_or(core_hosting::DEFAULT_PORT);
    let instances = paths.instances();
    let instance_id = match server.instance_id.as_deref().filter(|instance| instances.get(instance).is_some()) {
        Some(instance) => instance.to_string(),
        None => {
            let made = instances
                .create(NewInstance { name: server.name.clone(), game_version: server.game_version.clone(), loader: server.loader, loader_version: server.loader_version.clone(), icon: server.icon.clone() }, server.loader_version.clone(), now_ms())
                .map_err(|error| error.to_string())?;
            let dir = store.dir_of(&id).ok_or(GONE)?;
            let game_dir = instances.game_dir(&made.id).ok_or(GONE)?;
            blocking(move || {
                std::fs::create_dir_all(game_dir.join("mods")).map_err(|error| error.to_string())?;
                for entry in std::fs::read_dir(dir.join("mods")).into_iter().flatten().flatten() {
                    if entry.file_name().to_string_lossy().ends_with(".jar") {
                        std::fs::copy(entry.path(), game_dir.join("mods").join(entry.file_name())).map_err(|error| error.to_string())?;
                    }
                }
                Ok(())
            })
            .await?;
            store.update(&id, |server| server.instance_id = Some(made.id.clone())).map_err(|error| error.to_string())?;
            made.id
        }
    };
    launch(&app, &instance_id, format!("127.0.0.1:{port}")).await?;
    Ok(instance_id)
}

async fn launch(app: &AppHandle, instance_id: &str, address: String) -> Result<(), String> {
    crate::launch_instance(
        instance_id.to_string(),
        None,
        Some(address),
        app.clone(),
        app.state::<Paths>(),
        app.state::<SettingsState>(),
        app.state::<Network>(),
        app.state::<Launches>(),
        app.state::<crate::SkinServerState>(),
    )
    .await
}

// ── Friends: invites, presence, letting them in ──

/// What friends see of a server.
fn card(app: &AppHandle, server: &HostedServer) -> ServerCard {
    let paths = app.state::<Paths>();
    let dir = store(&paths).root().join(&server.id);
    let properties = Properties::read(&dir);
    let process = process_of(app, &server.id);
    ServerCard {
        id: server.id.clone(),
        name: server.name.clone(),
        game_version: server.game_version.clone(),
        loader: server.loader.as_str().to_string(),
        loader_version: server.loader_version.clone(),
        online: process.as_ref().is_some_and(|process| process.state() == RunState::Running),
        starting: process.as_ref().is_some_and(|process| process.state() == RunState::Starting),
        players: process.as_ref().map_or(0, |process| process.players().len() as u32),
        max_players: properties.get("max-players").and_then(|value| value.trim().parse().ok()).unwrap_or(20),
        mods: core_hosting::mod_count(&dir) as u32,
        software: server.software.map(|software| software.as_str().to_string()),
    }
}

/// The servers a friend can join, up or on their way up, for their presence.
pub fn cards_for(app: &AppHandle, friend: &str) -> Vec<ServerCard> {
    let Some(paths) = app.try_state::<Paths>() else {
        return Vec::new();
    };
    let running: Vec<String> = lock(&app.state::<Hosting>().live)
        .iter()
        .filter(|(_, live)| live.process.as_ref().is_some_and(|process| matches!(process.state(), RunState::Running | RunState::Starting)))
        .map(|(id, _)| id.clone())
        .collect();
    let store = store(&paths);
    running
        .iter()
        .filter_map(|id| store.get(id))
        .filter(|server| server.friends_join || server.invited.iter().any(|code| code == friend))
        .map(|server| card(app, &server))
        .collect()
}

#[tauri::command]
pub async fn invite_to_server(id: String, code: String, app: AppHandle) -> Result<HostedView, String> {
    let store = store(&app.state::<Paths>());
    let server = store.get(&id).ok_or(GONE)?;
    let card = card(&app, &server);
    social::send(&app, &code, &Message::ServerInvite { server: card }).await?;
    let server = store
        .update(&id, |server| {
            if !server.invited.contains(&code) {
                server.invited.push(code.clone());
            }
        })
        .map_err(|error| error.to_string())?;
    Ok(view(&app, server))
}

/// The host's side of an ask to join: checks the friend may, lets them in, and answers with the
/// way to reach this computer.
pub async fn answer_join(app: &AppHandle, from: String, server_id: String, session: String, offer: Offer, player: Option<Player>) {
    // Still starting: wait for it a little (the asker waits ANSWER_WITHIN), then tell them it's on
    // its way, and their Wisp asks again until it's up.
    if !up_within(app, &server_id, STARTING_WAIT).await {
        let name = store(&app.state::<Paths>()).get(&server_id).map_or_else(|| "The server".to_string(), |server| server.name);
        let answer = Message::JoinAnswer { session, offer: None, server: None, error: Some(format!("{name} is still starting. Try again in a minute.")), starting: true };
        let _ = social::send(app, &from, &answer).await;
        return;
    }
    let answer = match let_in(app, &from, &server_id, &offer, player.as_ref()).await {
        Ok((offer, server)) => Message::JoinAnswer { session, offer: Some(offer), server: Some(server), error: None, starting: false },
        Err(error) => Message::JoinAnswer { session, offer: None, server: None, error: Some(error), starting: false },
    };
    let _ = social::send(app, &from, &answer).await;
}

/// How long the host holds an ask for a server that's starting before saying so.
const STARTING_WAIT: Duration = Duration::from_secs(25);
/// How long a friend's Wisp keeps asking while a server starts.
const STARTING_PATIENCE: Duration = Duration::from_secs(5 * 60);

/// False while the server is still starting after `wait`; true once it's up, or if it isn't
/// starting at all (then joining says why).
async fn up_within(app: &AppHandle, server_id: &str, wait: Duration) -> bool {
    let Some(process) = process_of(app, server_id) else {
        return true;
    };
    let mut state = process.watch_state();
    let up = tokio::time::timeout(wait, state.wait_for(|state| *state != RunState::Starting)).await;
    up.is_ok()
}

async fn let_in(app: &AppHandle, from: &str, server_id: &str, offer: &Offer, player: Option<&Player>) -> Result<(Offer, ServerCard), String> {
    let paths = app.state::<Paths>();
    let store = store(&paths);
    let server = store.get(server_id).ok_or("That server isn't there anymore.")?;
    if !server.friends_join && !server.invited.iter().any(|code| code == from) {
        return Err(format!("{} is invite-only.", server.name));
    }
    let process = process_of(app, server_id).filter(|process| process.state() == RunState::Running).ok_or(format!("{} isn't running right now.", server.name))?;
    let (endpoint, service) = link_for(app, &server).await?;
    endpoint.allow(&offer.fingerprint);
    // The skin server answers for them from now on (their skin comes over the link), and they
    // may hand in only themselves.
    if signs_in_through_wisp() && let Some(player) = player.filter(|player| wisp_core::accounts::valid_username(&player.name)) {
        let uuid = player.uuid.replace('-', "").to_ascii_lowercase();
        let guests = app.state::<crate::SkinServerState>().1.clone();
        guests
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entry(uuid.clone())
            .and_modify(|guest| guest.name = player.name.clone())
            .or_insert_with(|| wisp_core::skin_server::Player { uuid: uuid.clone(), name: player.name.clone(), skin: None });
        service.players.write().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(offer.fingerprint.to_ascii_lowercase(), uuid);
    }
    // Let them in by name, when the server keeps a whitelist.
    let whitelist = Properties::read(&store.root().join(server_id)).get("white-list").is_some_and(|value| value.trim() == "true");
    if whitelist && let Some(player) = player.filter(|player| wisp_core::accounts::valid_username(&player.name)) {
        let _ = process.send(&format!("whitelist add {}", player.name)).await;
    }
    if let Some(live) = lock(&app.state::<Hosting>().live).get_mut(server_id)
        && !live.friends.iter().any(|friend| friend == from)
    {
        live.friends.push(from.to_string());
    }
    // Dial them too: a strict router on their side doesn't stop a connection started from here.
    {
        let endpoint = endpoint.clone();
        let offer = offer.clone();
        tauri::async_runtime::spawn(async move {
            if let Ok(link) = endpoint.connect(&offer).await {
                link.serve(service).await;
            }
        });
    }
    emit(app, server_id, Change::Changed);
    let mut answer = endpoint.offer();
    answer.doors = doors_of(app, &server);
    Ok((answer, card(app, &server)))
}

/// Where friends' Wisps can come in through the server's door, nearest first: the same network,
/// the router's opening, the address people join from anywhere.
fn doors_of(app: &AppHandle, server: &HostedServer) -> Vec<String> {
    let port = store(&app.state::<Paths>()).port_of(&server.id).unwrap_or(core_hosting::DEFAULT_PORT);
    let mut doors = Vec::new();
    if let Some(lan) = tunnel::lan_address() {
        doors.push(format!("{lan}:{port}"));
    }
    if let Some(mapping) = lock(&app.state::<Hosting>().live).get(&server.id).and_then(|live| live.port_mapping.clone()) {
        doors.push(mapping.external().to_string());
    }
    if let Some(address) = server.address.as_deref().map(str::trim).filter(|address| !address.is_empty()) {
        doors.push(address.to_string());
    }
    let mut seen = HashSet::new();
    doors.retain(|door| seen.insert(door.clone()));
    doors
}

/// Gets a running server ready for Wisp joins, through its door and the link alike: the list of
/// its mods and their files, the skin service players sign in through, and who may join by its
/// address (whoever its whitelist lets in). Done once per run.
async fn ready_for_joins(app: &AppHandle, server: &HostedServer) -> Result<(), String> {
    let service = lock(&app.state::<Hosting>().live).get(&server.id).map(|entry| entry.service.clone()).ok_or("The server stopped.")?;
    if !service.manifest.read().unwrap_or_else(|poisoned| poisoned.into_inner()).is_empty() {
        return Ok(());
    }
    // Players' games sign in and fetch skins from this computer's skin server.
    let mut skin_port = None;
    if signs_in_through_wisp()
        && let Some(skins) = crate::skin_server(app).await
    {
        skin_port = Some(skins.port);
        *service.skins.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), skins.port));
        *service.skin_key.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(skins.public_key.clone());
        let guests = app.state::<crate::SkinServerState>().1.clone();
        *service.on_player.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Arc::new(move |upload: Upload| {
            if !wisp_core::accounts::valid_username(&upload.name) {
                return;
            }
            use base64::Engine;
            let skin = upload
                .png
                .as_deref()
                .and_then(|png| base64::engine::general_purpose::STANDARD.decode(png).ok())
                .filter(|png| wisp_core::skin_server::valid_skin(png))
                .map(|png| wisp_core::skin_server::Skin { png, slim: upload.slim });
            let player = wisp_core::skin_server::Player { uuid: upload.uuid.clone(), name: upload.name.clone(), skin };
            guests.write().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(upload.uuid, player);
        }));
    }
    // Joining by address: whoever the server itself would let in.
    {
        let app = app.clone();
        let id = server.id.clone();
        *service.on_ask.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Arc::new(move |ask: &tunnel::Ask| {
            if !wisp_core::accounts::valid_username(&ask.name) || ask.uuid.len() != 32 {
                return false;
            }
            let Some(dir) = store(&app.state::<Paths>()).dir_of(&id) else {
                return false;
            };
            let whitelisted = || [List::Whitelist, List::Ops].into_iter().any(|list| players::read(&dir, list).iter().any(|entry| entry.name.eq_ignore_ascii_case(&ask.name)));
            let whitelist_on = Properties::read(&dir).get("white-list").is_some_and(|value| value.trim() == "true");
            let banned = players::read(&dir, List::Banned).iter().any(|entry| entry.name.eq_ignore_ascii_case(&ask.name));
            let yes = !banned && (!whitelist_on || whitelisted());
            if yes && signs_in_through_wisp() {
                let guests = app.state::<crate::SkinServerState>().1.clone();
                guests
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .entry(ask.uuid.clone())
                    .and_modify(|guest| guest.name = ask.name.clone())
                    .or_insert_with(|| wisp_core::skin_server::Player { uuid: ask.uuid.clone(), name: ask.name.clone(), skin: None });
            }
            yes
        }));
    }
    let (manifest, files) = pack_for(app, server, skin_port).await?;
    *service.files.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = files;
    *service.manifest.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = manifest;
    Ok(())
}

/// The server's link, opened the first time a friend asks: the socket, the routers asked, and
/// the instance's mod list ready to hand out.
async fn link_for(app: &AppHandle, server: &HostedServer) -> Result<(Arc<Endpoint>, Arc<Service>), String> {
    let hosting = app.state::<Hosting>();
    let _opening = hosting.opening.lock().await;
    let (existing, service, keys) = {
        let live = lock(&hosting.live);
        let entry = live.get(&server.id).ok_or("The server stopped.")?;
        (entry.link.clone(), entry.service.clone(), entry.keys.clone())
    };
    if let Some(endpoint) = existing {
        return Ok((endpoint, service));
    }
    ready_for_joins(app, server).await?;
    let endpoint = Arc::new(Endpoint::open_with(OpenOptions::default(), keys).await.map_err(|error| error.to_string())?);
    let accepting = {
        let endpoint = endpoint.clone();
        let service = service.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(link) = endpoint.accept().await {
                let service = service.clone();
                tauri::async_runtime::spawn(async move { link.serve(service).await });
            }
        })
    };
    let renewing = {
        let endpoint = endpoint.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(tunnel::upnp::LEASE / 2).await;
                endpoint.renew().await;
            }
        })
    };
    let mut live = lock(&hosting.live);
    let Some(entry) = live.get_mut(&server.id) else {
        accepting.abort();
        renewing.abort();
        let endpoint = endpoint.clone();
        tauri::async_runtime::spawn(async move { endpoint.close().await });
        return Err("The server stopped.".into());
    };
    entry.link = Some(endpoint.clone());
    entry.tasks.extend([accepting, renewing]);
    Ok((endpoint, service))
}

/// The list friends get (the instance's mods, or the server's own), and where each file is.
async fn pack_for(app: &AppHandle, server: &HostedServer, skin_port: Option<u16>) -> Result<(Vec<u8>, HashMap<String, PathBuf>), String> {
    let paths = app.state::<Paths>();
    let network = app.state::<Network>();
    let (mut pack, base) = match server.instance_id.as_deref().and_then(|id| paths.instances().dir_of(id)) {
        Some(instance_dir) => {
            let scan = crate::instance_page::scanned(&network, &instance_dir).await?;
            (Pack::from_instance(server, &scan.shareable()), instance_dir.join(GAME_DIR))
        }
        None => {
            let dir = store(&paths).dir_of(&server.id).ok_or(GONE)?;
            let read = server.clone();
            let from = dir.clone();
            (blocking(move || Ok(Pack::from_server_mods(&read, &from))).await?, dir)
        }
    };
    // Files Modrinth serves are downloaded from Modrinth, not the host's connection.
    let unknown: Vec<String> = pack.files.iter().filter(|file| file.url.is_none()).map(|file| file.sha1.clone()).collect();
    if let Ok(found) = wisp_core::modrinth::versions_by_hashes(&network.client, &unknown).await {
        for file in pack.files.iter_mut().filter(|file| file.url.is_none()) {
            file.url = found.get(&file.sha1).and_then(|version| version.files.iter().find(|candidate| candidate.sha1.as_deref() == Some(file.sha1.as_str()))).map(|candidate| candidate.url.clone());
        }
        // A server's own mods: players don't get the ones Modrinth says never run in the game.
        if server.instance_id.is_none() {
            let ids: Vec<String> = found.values().map(|version| version.project_id.clone()).collect::<HashSet<_>>().into_iter().collect();
            if let Ok(projects) = wisp_core::modrinth::projects(&network.client, &ids).await {
                let server_only: HashSet<String> = projects.into_iter().filter(|project| project.client_side == "unsupported").map(|project| project.id).collect();
                pack.files.retain(|file| found.get(&file.sha1).is_none_or(|version| !server_only.contains(&version.project_id)));
            }
        }
    }
    let mut pack = pack.cleaned();
    pack.skin_port = skin_port;
    let files = pack.files.iter().map(|file| (file.sha1.clone(), base.join(&file.path))).collect();
    Ok((serde_json::to_vec(&pack).map_err(|error| error.to_string())?, files))
}

// ── Joining a friend's server ──

/// What a join is doing, for the screen.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JoinProgress {
    code: String,
    server_id: String,
    /// "asking", "connecting", "mods", "launching", "done" or "failed".
    step: &'static str,
    done_bytes: u64,
    total_bytes: u64,
    message: Option<String>,
}

/// The answer to an ask to join, from the friends channel.
pub fn answered(app: &AppHandle, session: &str, answer: Answer) {
    if let Some(waiting) = lock(&app.state::<Hosting>().asks).remove(session) {
        let _ = waiting.send(answer);
    }
}

/// Which instance plays each friend's server, and what joining put in it.
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct JoinedRecord {
    instance_id: String,
    /// Files the joins put in its game folder (path → SHA-1).
    files: HashMap<String, String>,
}

fn joined_path(paths: &Paths) -> PathBuf {
    paths.data.join("servers").join("joined.json")
}

fn read_joined(paths: &Paths) -> HashMap<String, JoinedRecord> {
    std::fs::read(joined_path(paths)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

fn write_joined(paths: &Paths, records: &HashMap<String, JoinedRecord>) -> std::io::Result<()> {
    let path = joined_path(paths);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(records).map_err(std::io::Error::other)?)
}

#[tauri::command]
pub async fn join_friend_server(code: String, server_id: String, app: AppHandle) -> Result<String, String> {
    let progress = |step: &'static str, done_bytes: u64, total_bytes: u64, message: Option<String>| {
        let _ = app.emit(JOIN_EVENT, JoinProgress { code: code.clone(), server_id: server_id.clone(), step, done_bytes, total_bytes, message });
    };
    match join(&app, &code, &server_id, &progress).await {
        Ok(instance_id) => {
            progress("done", 0, 0, None);
            Ok(instance_id)
        }
        Err(error) => {
            progress("failed", 0, 0, Some(error.clone()));
            Err(error)
        }
    }
}

async fn join(app: &AppHandle, code: &str, server_id: &str, progress: &(dyn Fn(&'static str, u64, u64, Option<String>) + Send + Sync)) -> Result<String, String> {
    let paths = app.state::<Paths>();
    progress("asking", 0, 0, None);
    let endpoint = Arc::new(Endpoint::open(OpenOptions::default()).await.map_err(|error| error.to_string())?);
    let session = wisp_core::journal::new_id(now_ms());
    let (answer_tx, answer_rx) = oneshot::channel();
    lock(&app.state::<Hosting>().asks).insert(session.clone(), answer_tx);
    let player = paths.accounts().active().map(|account| Player { name: account.name, uuid: account.uuid });
    // A server that's starting answers "starting": ask again until it's up.
    let patience = tokio::time::Instant::now() + STARTING_PATIENCE;
    let (mut session, mut answer_rx) = (session, answer_rx);
    let answer = loop {
        let asked = social::send(app, code, &Message::JoinRequest { server_id: server_id.to_string(), session: session.clone(), offer: endpoint.offer(), player: player.clone() }).await;
        let answer = match asked {
            Ok(()) => tokio::time::timeout(ANSWER_WITHIN, answer_rx).await,
            Err(error) => {
                lock(&app.state::<Hosting>().asks).remove(&session);
                endpoint.close().await;
                return Err(error);
            }
        };
        lock(&app.state::<Hosting>().asks).remove(&session);
        let answer = match answer {
            Ok(Ok(answer)) => answer,
            _ => {
                endpoint.close().await;
                return Err("They didn't answer. Their Wisp may be closed, or have Friends turned off.".into());
            }
        };
        if !answer.starting || tokio::time::Instant::now() >= patience {
            break answer;
        }
        progress("waiting", 0, 0, None);
        tokio::time::sleep(Duration::from_secs(5)).await;
        session = wisp_core::journal::new_id(now_ms());
        let (next_tx, next_rx) = oneshot::channel();
        lock(&app.state::<Hosting>().asks).insert(session.clone(), next_tx);
        answer_rx = next_rx;
    };
    if let Some(error) = answer.error {
        endpoint.close().await;
        return Err(error);
    }
    let host = answer.offer.ok_or("They answered without a way to reach them.")?;
    progress("connecting", 0, 0, None);
    endpoint.allow(&host.fingerprint);
    let link = match endpoint.link_to(&host).await {
        Ok(link) => link,
        Err(error) => {
            endpoint.close().await;
            return Err(match error {
                tunnel::TunnelError::Unreachable if host.doors.iter().any(|door| !door.starts_with("192.168.") && !door.starts_with("10.")) => {
                    "Couldn't reach their computer, directly or through their server's address. Their tunnel or router may be down: ask them to check the address on their server's page.".to_string()
                }
                tunnel::TunnelError::Unreachable => "Couldn't connect to their computer: both routers are strict. They can give the server an address anyone can reach (a playit tunnel) on its page, or you can play on the same network.".to_string(),
                other => format!("Couldn't connect to their computer: {other}"),
            });
        }
    };
    let host = JoinedHost { key: code.to_string(), name: social_name(app, code) };
    let result = set_up_and_play(app, &host, &answer.server, &link, progress).await;
    match result {
        Ok((instance_id, forwarding, skins)) => {
            remember_join(app, instance_id.clone(), Joined { link, endpoint: Some(endpoint), forwarding, skins }).await;
            watch_joined_game(app.clone(), instance_id.clone());
            Ok(instance_id)
        }
        Err(error) => {
            link.close();
            endpoint.close().await;
            Err(error)
        }
    }
}

/// Whose server is being joined: a friend (by friend code and name) or an address.
struct JoinedHost {
    /// What joins to the same server share (with its id): which instance plays it.
    key: String,
    /// For the instance's name: the friend's name, or the address.
    name: String,
}

/// How joining by address went.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum AddressJoin {
    /// Not a Wisp server (or it didn't answer as one): play it as any server.
    Plain,
    /// On the way: the game starts into it.
    Joined { instance_id: String },
    /// It wants files only it has (not on Modrinth): asked first, since running them means
    /// trusting whoever runs the server.
    NeedsConsent { server: String, files: Vec<String> },
}

/// Joins a server hosted with Wisp by its address (one listed with Wisp's mark): matches its
/// mods and brings this player's skin, as joining a friend's does. Files only the server has wait
/// for `allow_host_files`.
#[tauri::command]
pub async fn join_server_address(address: String, allow_host_files: bool, app: AppHandle) -> Result<AddressJoin, String> {
    let progress = |step: &'static str, done_bytes: u64, total_bytes: u64, message: Option<String>| {
        let _ = app.emit(JOIN_EVENT, JoinProgress { code: address.clone(), server_id: String::new(), step, done_bytes, total_bytes, message });
    };
    let outcome = join_address(&app, &address, allow_host_files, &progress).await;
    match &outcome {
        Ok(AddressJoin::Joined { .. }) => progress("done", 0, 0, None),
        Ok(_) => progress("done", 0, 0, None),
        Err(error) => progress("failed", 0, 0, Some(error.clone())),
    }
    outcome
}

async fn join_address(app: &AppHandle, address: &str, allow_host_files: bool, progress: &(dyn Fn(&'static str, u64, u64, Option<String>) + Send + Sync)) -> Result<AddressJoin, String> {
    let host = JoinedHost { key: address.to_ascii_lowercase(), name: address.to_string() };
    join_through_doors(app, &host, &[address.to_string()], allow_host_files, progress).await
}

/// Joins a Wisp server through the first of its addresses whose door answers (mods, skin and
/// all). Plain when none does.
async fn join_through_doors(
    app: &AppHandle,
    host: &JoinedHost,
    addresses: &[String],
    allow_host_files: bool,
    progress: &(dyn Fn(&'static str, u64, u64, Option<String>) + Send + Sync),
) -> Result<AddressJoin, String> {
    let paths = app.state::<Paths>();
    let account = paths.accounts().active().ok_or("Add an account to play.")?;
    progress("asking", 0, 0, None);
    let keys = Keys::new().map_err(|error| error.to_string())?;
    let ask = tunnel::Ask { name: account.name.clone(), uuid: account.uuid.replace('-', "").to_ascii_lowercase() };
    let mut reached = None;
    for address in addresses.iter().take(6) {
        match tunnel::ask_door(address, &keys, &ask).await {
            Ok((link, _)) => {
                reached = Some(link);
                break;
            }
            Err(tunnel::TunnelError::Refused) => return Err(format!("{} didn't let you in: it only takes players on its whitelist. Ask whoever runs it to add {}.", host.name, account.name)),
            Err(_) => {}
        }
    }
    let Some(link) = reached else {
        return Ok(AddressJoin::Plain);
    };
    if !allow_host_files {
        let manifest = ready_manifest(&link, progress).await?;
        let pack: Pack = serde_json::from_slice::<Pack>(&manifest).map_err(|_| "The server sent a list this Wisp can't read. Update Wisp.".to_string())?.cleaned();
        let host_only: Vec<String> = pack.files.iter().filter(|file| file.url.is_none()).map(|file| file.path.trim_start_matches("mods/").to_string()).collect();
        if !host_only.is_empty() {
            link.close();
            return Ok(AddressJoin::NeedsConsent { server: pack.name, files: host_only });
        }
    }
    progress("connecting", 0, 0, None);
    match set_up_and_play(app, host, &None, &link, progress).await {
        Ok((instance_id, forwarding, skins)) => {
            remember_join(app, instance_id.clone(), Joined { link, endpoint: None, forwarding, skins }).await;
            watch_joined_game(app.clone(), instance_id.clone());
            Ok(AddressJoin::Joined { instance_id })
        }
        Err(error) => {
            link.close();
            Err(error)
        }
    }
}

// ── Join codes ──

fn relays() -> Vec<String> {
    wisp_core::social::DEFAULT_RELAYS.iter().map(|relay| relay.to_string()).collect()
}

/// Where a server keeps the key its code comes from.
const CODE_FILE: &str = ".wisp-code";

/// A server's join code (its key is made the first time it's asked for).
fn code_of(dir: &std::path::Path) -> Option<String> {
    wisp_core::codes::ServerCode::load_or_make(&dir.join(CODE_FILE)).ok().map(|code| code.code())
}

/// What a server's code says about it right now.
/// What a server's code says about it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CodeSays {
    Starting,
    Up,
    Stopped,
}

/// What the code should say now, from the server's state.
fn code_says_now(app: &AppHandle, id: &str) -> CodeSays {
    match process_of(app, id).map(|process| process.state()) {
        Some(RunState::Running) => CodeSays::Up,
        Some(RunState::Starting) => CodeSays::Starting,
        _ => CodeSays::Stopped,
    }
}

fn code_card(app: &AppHandle, server: &HostedServer, says: CodeSays) -> wisp_core::codes::CodeCard {
    let dir = store(&app.state::<Paths>()).root().join(&server.id);
    let properties = Properties::read(&dir);
    let process = process_of(app, &server.id);
    wisp_core::codes::CodeCard {
        name: server.name.clone(),
        game_version: server.game_version.clone(),
        loader: server.loader.as_str().to_string(),
        software: server.software.map(|software| software.as_str().to_string()),
        addresses: doors_of(app, server),
        online: says == CodeSays::Up,
        starting: says == CodeSays::Starting,
        players: process.as_ref().map_or(0, |process| process.players().len() as u32),
        max_players: properties.get("max-players").and_then(|value| value.trim().parse().ok()).unwrap_or(20),
        mods: core_hosting::mod_count(&dir) as u32,
        at: now_ms(),
    }
}

/// Says where the server is now, for whoever has its code (in the background). If no relay takes
/// it, tries again a few times (otherwise a stale word could stand until the next one, half an
/// hour away), unless something newer has been said by then.
fn announce_code(app: &AppHandle, id: &str, says: CodeSays) {
    let app = app.clone();
    let id = id.to_string();
    let turn = {
        let hosting = app.state::<Hosting>();
        let mut said = lock(&hosting.code_said);
        let turn = said.get(&id).copied().unwrap_or(0) + 1;
        said.insert(id.clone(), turn);
        turn
    };
    tauri::async_runtime::spawn(async move {
        for wait in [0, 5, 20, 60] {
            tokio::time::sleep(Duration::from_secs(wait)).await;
            if lock(&app.state::<Hosting>().code_said).get(&id) != Some(&turn) {
                return;
            }
            let store = store(&app.state::<Paths>());
            let (Some(server), Some(dir)) = (store.get(&id), store.dir_of(&id)) else {
                return;
            };
            let Ok(code) = wisp_core::codes::ServerCode::load_or_make(&dir.join(CODE_FILE)) else {
                return;
            };
            let card = code_card(&app, &server, says);
            if code.publish(&card, &relays()).await.is_ok() {
                return;
            }
        }
    });
}

/// Keeps a running server's word fresh on the relays (its players, and in case one forgot).
async fn keep_announcing(app: AppHandle, id: String) {
    loop {
        announce_code(&app, &id, CodeSays::Up);
        tokio::time::sleep(Duration::from_secs(30 * 60)).await;
    }
}

/// What a code leads to.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeLookup {
    /// The code, tidied up.
    code: String,
    /// The server, as it last said; none when no relay has heard of it.
    server: Option<wisp_core::codes::CodeCard>,
}

/// Finds the server a join code belongs to.
#[tauri::command]
pub async fn look_up_server_code(code: String) -> Result<CodeLookup, String> {
    let code = wisp_core::codes::normalize(&code).ok_or("That isn't a Wisp code. They look like WISP-7K2M-9QXA-4F2P.")?;
    let server = wisp_core::codes::resolve(&code, &relays()).await.map_err(|error| format!("Couldn't look it up: {error}"))?;
    Ok(CodeLookup { code, server })
}

/// Joins a server by its code: wherever it can be reached now, mods and skin and all.
#[tauri::command]
pub async fn join_server_code(code: String, allow_host_files: bool, app: AppHandle) -> Result<AddressJoin, String> {
    let code = wisp_core::codes::normalize(&code).ok_or("That isn't a Wisp code. They look like WISP-7K2M-9QXA-4F2P.")?;
    let progress = |step: &'static str, done_bytes: u64, total_bytes: u64, message: Option<String>| {
        let _ = app.emit(JOIN_EVENT, JoinProgress { code: code.clone(), server_id: String::new(), step, done_bytes, total_bytes, message });
    };
    progress("asking", 0, 0, None);
    let found = wisp_core::codes::resolve(&code, &relays()).await.map_err(|error| format!("Couldn't look the code up: {error}"));
    // On its way up: wait for it to say it's up.
    let found = match found {
        Ok(Some(server)) if server.starting && !server.online => {
            progress("waiting", 0, 0, None);
            Ok(Some(wait_while_starting(&code, server).await))
        }
        other => other,
    };
    let outcome = match found {
        Err(error) => Err(error),
        Ok(None) => Err(format!("No server has {code}. Check it, or ask whoever runs it to start the server.")),
        Ok(Some(server)) if server.starting && !server.online => Err(format!("{} is taking a long time to start. Try again in a minute.", server.name)),
        Ok(Some(server)) if !server.online => Err(format!("{} isn't running right now.", server.name)),
        Ok(Some(server)) => {
            // Joins by code share an instance per code, wherever the server moves.
            let host = JoinedHost { key: code.clone(), name: server.name.clone() };
            match join_through_doors(&app, &host, &server.addresses, allow_host_files, &progress).await {
                Ok(AddressJoin::Plain) => Err(format!("Couldn't reach {}: its address may have changed, or its tunnel is down. Ask whoever runs it to check the server's page.", server.name)),
                other => other,
            }
        }
    };
    match &outcome {
        Ok(_) => progress("done", 0, 0, None),
        Err(error) => progress("failed", 0, 0, Some(error.clone())),
    }
    outcome
}

/// The host's mod list. A host whose server is still starting (or just up and still listing its
/// mods) sends an empty one: wait and ask again, rather than calling it unreadable.
async fn ready_manifest(link: &Link, progress: &(dyn Fn(&'static str, u64, u64, Option<String>) + Send + Sync)) -> Result<Vec<u8>, String> {
    let patience = tokio::time::Instant::now() + STARTING_PATIENCE;
    loop {
        let manifest = link.manifest().await.map_err(|error| error.to_string())?;
        if !manifest.is_empty() {
            return Ok(manifest);
        }
        if tokio::time::Instant::now() >= patience {
            return Err("The server is taking a long time to start. Try again in a minute.".into());
        }
        progress("waiting", 0, 0, None);
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

/// Asks the relays again every few seconds until a starting server says it's up (or gives up).
async fn wait_while_starting(code: &str, mut server: wisp_core::codes::CodeCard) -> wisp_core::codes::CodeCard {
    let patience = tokio::time::Instant::now() + STARTING_PATIENCE;
    while server.starting && !server.online && tokio::time::Instant::now() < patience {
        tokio::time::sleep(Duration::from_secs(5)).await;
        if let Ok(Some(newer)) = wisp_core::codes::resolve(code, &relays()).await
            && newer.at >= server.at
        {
            server = newer;
        }
    }
    server
}

/// Makes or updates the instance that matches the host's, fetches what it's missing, and starts
/// the game straight into the server through the link.
#[allow(clippy::type_complexity)]
async fn set_up_and_play(
    app: &AppHandle,
    host: &JoinedHost,
    card: &Option<ServerCard>,
    link: &Link,
    progress: &(dyn Fn(&'static str, u64, u64, Option<String>) + Send + Sync),
) -> Result<(String, tokio::task::JoinHandle<()>, Option<tokio::task::JoinHandle<()>>), String> {
    let paths = app.state::<Paths>();
    let network = app.state::<Network>();
    progress("mods", 0, 0, None);
    let manifest = ready_manifest(link, progress).await?;
    let pack: Pack = serde_json::from_slice::<Pack>(&manifest).map_err(|_| "Their Wisp sent a list this one can't read. Update both to the newest Wisp.".to_string())?.cleaned();
    let loader: Loader = serde_json::from_value(serde_json::Value::String(pack.loader.clone())).map_err(|_| format!("Their server runs {}, which this Wisp doesn't know.", pack.loader))?;
    let host_name = host.name.clone();
    let key = format!("{}/{}", host.key, pack.server_id);
    let instances = paths.instances();
    let mut records = read_joined(&paths);
    let record = records.remove(&key).unwrap_or_default();
    let instance = match instances.get(&record.instance_id) {
        Some(instance) => {
            if crate::library::busy(&app.state::<Launches>(), &instance.id) {
                return Err(format!("{} is already running.", instance.name));
            }
            if instance.game_version != pack.game_version || instance.loader != loader || instance.loader_version != pack.loader_version {
                instances
                    .update(&instance.id, |instance| {
                        instance.game_version = pack.game_version.clone();
                        instance.loader = loader;
                        instance.loader_version = pack.loader_version.clone();
                    })
                    .map_err(|error| error.to_string())?
            } else {
                instance
            }
        }
        None => instances
            .create(
                NewInstance { name: format!("{} ({host_name})", card.as_ref().map_or(pack.name.as_str(), |card| card.name.as_str())), game_version: pack.game_version.clone(), loader, loader_version: pack.loader_version.clone(), icon: Some("wisp:7".into()) },
                pack.loader_version.clone(),
                now_ms(),
            )
            .map_err(|error| error.to_string())?,
    };
    let instance_dir = instances.dir_of(&instance.id).ok_or("The instance went missing.")?;
    let game_dir = instance_dir.join(GAME_DIR);

    // What's missing comes from Modrinth where it can, else from the host over the link.
    let plan = {
        let (pack, game_dir, installed) = (pack.clone(), game_dir.clone(), record.files.clone());
        blocking(move || Ok(pack::plan(&pack, &game_dir, &installed))).await?
    };
    for path in &plan.remove {
        let _ = std::fs::remove_file(game_dir.join(path));
    }
    let total: u64 = plan.get.iter().map(|file| file.size).sum();
    let downloads: Vec<Download> = plan
        .get
        .iter()
        .filter_map(|file| Some(Download { url: file.url.clone()?, path: game_dir.join(&file.path), sha1: Some(file.sha1.clone()), size: Some(file.size), executable: false }))
        .collect();
    let counter = Arc::new(Progress::default());
    if !downloads.is_empty() {
        let concurrency = crate::current_settings(&app.state::<SettingsState>()).concurrent_downloads as usize;
        let downloading = launch::download_all(&network.downloads, downloads, concurrency, &counter);
        tokio::pin!(downloading);
        let mut tick = tokio::time::interval(Duration::from_millis(200));
        loop {
            tokio::select! {
                _ = &mut downloading => break,
                _ = tick.tick() => progress("mods", counter.done_bytes.load(Ordering::Relaxed), total, None),
            }
        }
    }
    // Over the link: files no site has, and any download that didn't work.
    let mut fetched = counter.done_bytes.load(Ordering::Relaxed);
    for file in &plan.get {
        let target = game_dir.join(&file.path);
        let present = pack::hash_file(&target).is_some_and(|(sha1, _)| sha1 == file.sha1);
        if present {
            continue;
        }
        link.fetch(&file.sha1, &target).await.map_err(|error| format!("Couldn't get {} from them: {error}", file.path.trim_start_matches("mods/")))?;
        fetched += file.size;
        progress("mods", fetched.min(total), total, None);
    }
    let files: HashMap<String, String> = pack.files.iter().map(|file| (file.path.clone(), file.sha1.clone())).collect();
    let _ = wisp_core::inventory::record_pack_files(&instance_dir, &files);
    records.insert(key, JoinedRecord { instance_id: instance.id.clone(), files });
    let _ = write_joined(&paths, &records);

    progress("launching", total, total, None);
    // Where players sign in through the host's skin server: hand it this player and skin, and
    // point the game at it through the link.
    let skins = match pack.skin_port {
        Some(host_port) if signs_in_through_wisp() => Some(host_skins(app, link, host_port, &instance.id).await?),
        _ => None,
    };
    let (port, forwarding) = link.forward().await.map_err(|error| error.to_string())?;
    if let Err(error) = launch(app, &instance.id, format!("127.0.0.1:{port}")).await {
        forwarding.abort();
        if let Some(skins) = skins {
            skins.abort();
        }
        lock(&app.state::<crate::SkinServerState>().2).remove(&instance.id);
        return Err(error);
    }
    Ok((instance.id, forwarding, skins))
}

/// Hands the host who this computer plays as and its skin, opens the way to the host's skin
/// server, and has the next launch of `instance_id` sign in through it.
async fn host_skins(app: &AppHandle, link: &Link, host_port: u16, instance_id: &str) -> Result<tokio::task::JoinHandle<()>, String> {
    use base64::Engine;
    let paths = app.state::<Paths>();
    let account = paths.accounts().active().ok_or("Add an account to play.")?;
    let worn = crate::worn_skin(&paths.data);
    let upload = Upload {
        name: account.name.clone(),
        uuid: account.uuid.replace('-', "").to_ascii_lowercase(),
        slim: worn.as_ref().is_some_and(|skin| skin.slim),
        png: worn.map(|skin| base64::engine::general_purpose::STANDARD.encode(skin.png)),
    };
    link.hand_in(&upload).await.map_err(|error| error.to_string())?;
    let (port, task) = link.forward_skins(host_port).await.map_err(|error| error.to_string())?;
    // The host's skin server describes itself (the key its skin data is signed with) through the link.
    let fetched = async {
        let response = app.state::<Network>().client.get(format!("http://127.0.0.1:{port}/")).send().await.ok()?;
        response.error_for_status().ok()?.text().await.ok()
    };
    let Some(metadata) = tokio::time::timeout(Duration::from_secs(10), fetched).await.ok().flatten() else {
        task.abort();
        return Err("Their Wisp's skin service didn't answer.".into());
    };
    let host = wisp_core::skin_server::SkinServer { api_root: format!("http://127.0.0.1:{port}/"), metadata, port, public_key: String::new() };
    lock(&app.state::<crate::SkinServerState>().2).insert(instance_id.to_string(), host);
    Ok(task)
}

fn social_name(app: &AppHandle, code: &str) -> String {
    social::friend_name(app, code)
}

/// Closes the link to a friend's server once the game that joined it closes.
fn watch_joined_game(app: AppHandle, instance_id: String) {
    tauri::async_runtime::spawn(async move {
        let playing = |app: &AppHandle| crate::library::busy(&app.state::<Launches>(), &instance_id);
        // It takes a moment to show up as running.
        for _ in 0..20 {
            if playing(&app) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        while playing(&app) {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        let ended = lock(&app.state::<Hosting>().joined).remove(&instance_id);
        lock(&app.state::<crate::SkinServerState>().2).remove(&instance_id);
        if let Some(ended) = ended {
            ended.forwarding.abort();
            if let Some(skins) = ended.skins {
                skins.abort();
            }
            ended.link.close();
            if let Some(endpoint) = ended.endpoint {
                endpoint.close().await;
            }
        }
    });
}

// ── Skins when joining from Minecraft's own server list ──

/// The game connected to `address` from inside Minecraft (its server list, or by address). When
/// that's somebody's server hosted with Wisp, this hands its host the player and their skin over
/// Wisp's link, and opens the way to its skin server here. The host waits a few seconds for that
/// before letting the player in, so they arrive with their skin, and (with the skin key testers
/// share) everyone sees everyone's.
pub fn bring_skin(app: &AppHandle, instance_id: &str, address: String) {
    if !signs_in_through_wisp() {
        return;
    }
    let (app, instance_id) = (app.clone(), instance_id.to_string());
    tauri::async_runtime::spawn(async move {
        let _ = bring(&app, &instance_id, &address).await;
    });
}

async fn bring(app: &AppHandle, instance_id: &str, address: &str) -> Result<(), String> {
    // A server hosted here: this computer's own skin server answers for it already.
    if hosted_here(app, address) {
        return Ok(());
    }
    // Only Wisp's doors (their server list answer carries its mark): nobody else's server is
    // knocked on.
    let status = wisp_core::servers::status(address).await;
    if !status.online || !status.wisp {
        return Ok(());
    }
    if let Some(key) = &status.wisp_skin_key {
        crate::trust_skin_key(app, key);
    }
    // Whatever this game had open for the server before (it left, or is joining again) goes
    // first, so its skin server's port here is free again.
    if let Some(previous) = lock(&app.state::<Hosting>().joined).remove(instance_id) {
        previous.forwarding.abort();
        if let Some(skins) = previous.skins {
            skins.abort();
        }
        previous.link.close();
    }
    let paths = app.state::<Paths>();
    let account = paths.accounts().active().ok_or("No account")?;
    let keys = Keys::new().map_err(|error| error.to_string())?;
    let ask = tunnel::Ask { name: account.name.clone(), uuid: account.uuid.replace('-', "").to_ascii_lowercase() };
    let (link, _) = tunnel::ask_door(address, &keys, &ask).await.map_err(|error| error.to_string())?;
    // Its skin server's port is in its list, which is ready by the time anyone can join.
    let manifest = link.manifest().await.map_err(|error| error.to_string())?;
    let Some(host_port) = serde_json::from_slice::<Pack>(&manifest).ok().and_then(|pack| pack.skin_port) else {
        link.close();
        return Ok(());
    };
    use base64::Engine;
    let worn = crate::worn_skin(&paths.data);
    let upload = Upload {
        name: account.name.clone(),
        uuid: account.uuid.replace('-', "").to_ascii_lowercase(),
        slim: worn.as_ref().is_some_and(|skin| skin.slim),
        png: worn.map(|skin| base64::engine::general_purpose::STANDARD.encode(skin.png)),
    };
    if let Err(error) = link.hand_in(&upload).await {
        link.close();
        return Err(error.to_string());
    }
    // Skin addresses in the host's answers name its own port on 127.0.0.1: the same port here
    // leads to it, through the link.
    let forwarding = match link.forward_skins(host_port).await {
        Ok((_, task)) => task,
        Err(error) => {
            link.close();
            return Err(error.to_string());
        }
    };
    remember_join(app, instance_id.to_string(), Joined { link, endpoint: None, forwarding, skins: None }).await;
    watch_joined_game(app.clone(), instance_id.to_string());
    Ok(())
}

/// Whether `address` is one of the servers running here (playing on your own server).
fn hosted_here(app: &AppHandle, address: &str) -> bool {
    let wanted = address.trim().to_ascii_lowercase();
    let ids: Vec<String> = lock(&app.state::<Hosting>().live).keys().cloned().collect();
    let store = store(&app.state::<Paths>());
    ids.iter().any(|id| {
        let Some(server) = store.get(id) else {
            return false;
        };
        let port = store.port_of(id).unwrap_or(core_hosting::DEFAULT_PORT);
        let mut ours = doors_of(app, &server);
        ours.extend([format!("127.0.0.1:{port}"), format!("localhost:{port}")]);
        if port == core_hosting::DEFAULT_PORT {
            ours.extend(["127.0.0.1".to_string(), "localhost".to_string()]);
        }
        ours.iter().any(|door| door.to_ascii_lowercase() == wanted)
    })
}

/// Keeps what playing a joined server needs until its game closes (replacing an earlier join of
/// the same instance).
async fn remember_join(app: &AppHandle, instance_id: String, joined: Joined) {
    let previous = lock(&app.state::<Hosting>().joined).insert(instance_id, joined);
    if let Some(previous) = previous {
        previous.forwarding.abort();
        if let Some(skins) = previous.skins {
            skins.abort();
        }
        previous.link.close();
        if let Some(endpoint) = previous.endpoint {
            endpoint.close().await;
        }
    }
}
