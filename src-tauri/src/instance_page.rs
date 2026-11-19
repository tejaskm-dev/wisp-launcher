//! An instance's own page: everything in it (mods, resource packs, shader packs, data packs),
//! how long it's been played, what it does differently on this computer, moving it to another
//! Minecraft version or loader, copying and exporting it, its logs, and updating its modpack.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use wisp_core::content::InstalledContent;
use wisp_core::curseforge;
use wisp_core::game_logs::{self, LogFile};
use wisp_core::home::instance_label;
use wisp_core::instances::{GAME_DIR, Instance, Loader, LocalSettings};
use wisp_core::inventory::{self, Item, MovePlan, Moved, Removed, Update};
use wisp_core::journal::{Event, JournalQuery};
use wisp_core::modpack::{self, ExportOptions, Exported, PackUpdated, PackUpdates};
use wisp_core::snapshots::{NewSnapshot, SnapshotReason, Snapshots};
use wisp_core::{loaders, worlds};

use crate::library::{self, blocking, busy, note_event};
use crate::{Launches, Network, Paths, SettingsState, current_settings, now_ms};

const GONE: &str = "That instance no longer exists.";
const RUNNING: &str = "Close Minecraft first: the game is using this instance.";

fn instance_and_dir(paths: &Paths, id: &str) -> Result<(Instance, PathBuf), String> {
    let instances = paths.instances();
    Ok((instances.get(id).ok_or(GONE)?, instances.dir_of(id).ok_or(GONE)?))
}

fn not_running(launches: &Launches, id: &str) -> Result<(), String> {
    if busy(launches, id) { Err(RUNNING.into()) } else { Ok(()) }
}

fn mrpack_of(paths: &Paths, instance: &Instance) -> Option<PathBuf> {
    instance.modpack.as_ref().map(|origin| paths.data.join("cache").join("modpacks").join(format!("{}.mrpack", origin.version_id)))
}

/// Reads an instance's files (off the main thread) and asks Modrinth about new ones.
async fn scanned(network: &Network, dir: &Path) -> Result<inventory::Scan, String> {
    let scan_dir = dir.to_path_buf();
    let mut scan = blocking(move || Ok(inventory::scan(&scan_dir))).await?;
    scan.identify(&network.client, now_ms()).await;
    scan.save(dir);
    Ok(scan)
}

fn named_items(paths: &Paths, instance: &Instance, dir: &Path, scan: &inventory::Scan) -> Vec<Item> {
    let mrpack = mrpack_of(paths, instance);
    let pack = inventory::pack_files(dir, mrpack.as_deref());
    scan.items(instance, dir, pack.as_ref())
}

/// Everything in an instance, named. Pictures taken from files come as paths in the data folder.
#[tauri::command]
pub async fn get_instance_items(id: String, paths: State<'_, Paths>, network: State<'_, Network>) -> Result<Vec<Item>, String> {
    let (instance, dir) = instance_and_dir(&paths, &id)?;
    let scan = scanned(&network, &dir).await?;
    let mut items = named_items(&paths, &instance, &dir, &scan);
    for item in &mut items {
        if let Some(icon) = &item.icon_file {
            item.icon_file = Some(format!("instances/{id}/{icon}"));
        }
    }
    Ok(items)
}

/// Newer builds of what's in an instance, for its loader and Minecraft version.
#[tauri::command]
pub async fn get_instance_updates(id: String, paths: State<'_, Paths>, network: State<'_, Network>) -> Result<Vec<Update>, String> {
    let (instance, dir) = instance_and_dir(&paths, &id)?;
    let scan = scanned(&network, &dir).await?;
    scan.updates(&network.client, &instance).await.map_err(|error| error.to_string())
}

/// Turns a mod or pack on or off. Returns where its file is now.
#[tauri::command]
pub async fn set_item_enabled(id: String, file: String, enabled: bool, paths: State<'_, Paths>, launches: State<'_, Launches>) -> Result<String, String> {
    not_running(&launches, &id)?;
    let (_, dir) = instance_and_dir(&paths, &id)?;
    blocking(move || inventory::set_enabled(&dir, &file, enabled).map_err(|error| error.to_string())).await
}

/// Takes a mod or pack out of an instance (it can be put back).
#[tauri::command]
pub async fn remove_item(id: String, file: String, name: String, app: AppHandle, paths: State<'_, Paths>, launches: State<'_, Launches>) -> Result<Removed, String> {
    not_running(&launches, &id)?;
    let (_, dir) = instance_and_dir(&paths, &id)?;
    let kind = file.split('/').next().unwrap_or_default().to_string();
    let world = file.strip_prefix("saves/").and_then(|rest| rest.split('/').next()).map(str::to_string);
    let removed = blocking(move || inventory::remove(&dir, &file, now_ms()).map_err(|error| error.to_string())).await?;
    let project_type = match kind.as_str() {
        "mods" => "mod",
        "resourcepacks" => "resourcepack",
        "shaderpacks" => "shader",
        _ => "datapack",
    };
    note_event(&app, Some(&id), world.as_deref(), Event::ContentRemoved { project_id: String::new(), title: name, project_type: project_type.into() });
    Ok(removed)
}

/// Puts a removed mod or pack back. Returns where its file is.
#[tauri::command]
pub async fn put_back_item(id: String, removed_id: String, paths: State<'_, Paths>, launches: State<'_, Launches>) -> Result<String, String> {
    not_running(&launches, &id)?;
    let (_, dir) = instance_and_dir(&paths, &id)?;
    blocking(move || inventory::put_back(&dir, &removed_id).map_err(|error| error.to_string())).await
}

fn note_added(app: &AppHandle, id: &str, installed: &[InstalledContent], project_id: Option<&str>) {
    let main = installed.iter().find(|item| Some(item.project_id.as_str()) == project_id).or(installed.first());
    if let Some(item) = main {
        note_event(
            app,
            Some(id),
            item.world.as_deref(),
            Event::ContentAdded {
                project_id: item.project_id.clone(),
                title: item.title.clone(),
                project_type: item.project_type.as_str().to_string(),
                version: item.version_number.clone(),
                dependencies: installed.len().saturating_sub(1) as u32,
            },
        );
    }
}

/// Puts a specific build of a mod or pack in place of the one there (an update, or another
/// version picked by hand).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn install_item_version(
    id: String,
    file: String,
    version_id: String,
    app: AppHandle,
    paths: State<'_, Paths>,
    settings: State<'_, SettingsState>,
    network: State<'_, Network>,
    launches: State<'_, Launches>,
) -> Result<Vec<InstalledContent>, String> {
    not_running(&launches, &id)?;
    let (instance, dir) = instance_and_dir(&paths, &id)?;
    let settings = current_settings(&settings);
    let key = curseforge::api_key(&settings.curseforge_api_key);
    let installed = inventory::install_version(&network.downloads, &instance, &dir, &file, &version_id, settings.concurrent_downloads as usize, key.as_deref(), now_ms())
        .await
        .map_err(|error| error.to_string())?;
    note_added(&app, &id, &installed, None);
    Ok(installed)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatedAll {
    pub updated: Vec<String>,
    /// Names, each with why.
    pub failed: Vec<(String, String)>,
}

/// Updates everything in an instance that has a newer build.
#[tauri::command]
pub async fn update_all_items(
    id: String,
    app: AppHandle,
    paths: State<'_, Paths>,
    settings: State<'_, SettingsState>,
    network: State<'_, Network>,
    launches: State<'_, Launches>,
) -> Result<UpdatedAll, String> {
    not_running(&launches, &id)?;
    let (instance, dir) = instance_and_dir(&paths, &id)?;
    let settings = current_settings(&settings);
    let key = curseforge::api_key(&settings.curseforge_api_key);
    let scan = scanned(&network, &dir).await?;
    let items = named_items(&paths, &instance, &dir, &scan);
    let updates = scan.updates(&network.client, &instance).await.map_err(|error| error.to_string())?;
    let mut result = UpdatedAll { updated: Vec::new(), failed: Vec::new() };
    for update in updates {
        let name = items.iter().find(|item| item.file == update.file).map_or_else(|| update.file.clone(), |item| item.name.clone());
        match inventory::install_version(&network.downloads, &instance, &dir, &update.file, &update.version_id, settings.concurrent_downloads as usize, key.as_deref(), now_ms()).await {
            Ok(installed) => {
                note_added(&app, &id, &installed, Some(&update.project_id));
                result.updated.push(name);
            }
            Err(error) => result.failed.push((name, error.to_string())),
        }
    }
    Ok(result)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceStats {
    /// Every session's length added up, in milliseconds.
    pub playtime_ms: i64,
    pub sessions: u32,
}

/// How long an instance has been played, from its journal.
#[tauri::command]
pub async fn get_instance_stats(id: String, paths: State<'_, Paths>) -> Result<InstanceStats, String> {
    let journal = paths.journal();
    blocking(move || {
        let entries = journal.list(&JournalQuery { instance_id: Some(id), kinds: vec!["session".into()], ..Default::default() });
        let mut stats = InstanceStats { playtime_ms: 0, sessions: 0 };
        for entry in entries {
            if let Event::Session { started_at, ended_at, .. } = entry.event {
                stats.playtime_ms += (ended_at - started_at).max(0);
                stats.sessions += 1;
            }
        }
        Ok(stats)
    })
    .await
}

#[tauri::command]
pub async fn get_local_settings(id: String, paths: State<'_, Paths>) -> Result<LocalSettings, String> {
    paths.instances().dir_of(&id).ok_or(GONE)?;
    Ok(paths.instances().local_settings(&id))
}

#[tauri::command]
pub async fn set_local_settings(id: String, settings: LocalSettings, paths: State<'_, Paths>) -> Result<(), String> {
    paths.instances().set_local_settings(&id, &settings).map_err(|error| error.to_string())
}

/// What an instance's game gets for memory, and where that comes from.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceMemory {
    /// What the game gets.
    mb: u32,
    /// Set for this instance (by hand, or by a fix after it ran out).
    own: bool,
    /// Wisp's setting, in Settings > Game.
    setting_mb: u32,
    /// What Wisp picks when the instance has none of its own: the setting, or more for a big modpack.
    automatic_mb: u32,
    /// The mods turned on, which the automatic memory goes by.
    mods: usize,
    /// The computer's memory, when it can be read.
    system_mb: Option<u64>,
}

/// An instance's memory as its next start will have it.
#[tauri::command]
pub async fn get_instance_memory(id: String, paths: State<'_, Paths>, settings: State<'_, SettingsState>) -> Result<InstanceMemory, String> {
    let instances = paths.instances();
    let instance = instances.get(&id).ok_or("That instance no longer exists.")?;
    let game_dir = instances.game_dir(&id).ok_or("That instance no longer exists.")?;
    let setting_mb = current_settings(&settings).memory_mb;
    blocking(move || {
        let mods = wisp_core::instances::mod_count(&game_dir);
        let system_mb = wisp_core::device::total_memory_mb();
        let automatic = wisp_core::instances::automatic_memory_mb(setting_mb, mods, system_mb);
        Ok(InstanceMemory { mb: instance.memory_mb.unwrap_or(automatic), own: instance.memory_mb.is_some(), setting_mb, automatic_mb: automatic, mods, system_mb })
    })
    .await
}

/// Memory for an instance (None: the one in Settings).
#[tauri::command]
pub async fn set_instance_memory(id: String, memory_mb: Option<u32>, paths: State<'_, Paths>) -> Result<Instance, String> {
    let memory_mb = memory_mb.map(|mb| mb.clamp(512, 65536));
    paths.instances().update(&id, |instance| instance.memory_mb = memory_mb).map_err(|error| error.to_string())
}

/// The Java an instance runs with, by major version (None: the one Minecraft asks for).
#[tauri::command]
pub async fn set_instance_java(id: String, java_major: Option<u32>, paths: State<'_, Paths>) -> Result<Instance, String> {
    paths.instances().update(&id, |instance| instance.java_major = java_major).map_err(|error| error.to_string())
}

/// What moving an instance to another Minecraft version or loader would do to its mods.
#[tauri::command]
pub async fn plan_instance_move(id: String, game_version: String, loader: Loader, paths: State<'_, Paths>, network: State<'_, Network>) -> Result<MovePlan, String> {
    let (instance, dir) = instance_and_dir(&paths, &id)?;
    let scan = scanned(&network, &dir).await?;
    let items = named_items(&paths, &instance, &dir, &scan);
    scan.plan_move(&network.client, &items, &game_version, loader).await.map_err(|error| error.to_string())
}

/// Keeps a restore point of every world in an instance, before something that might not agree
/// with them (an older Minecraft, another modpack version). Returns how many it kept.
fn keep_worlds(app: &AppHandle, snapshots: &Snapshots, id: &str, game_dir: &Path, label: &str) -> u32 {
    let mut kept = 0;
    for world in worlds::list(game_dir) {
        let Some(dir) = worlds::world_dir(game_dir, &world.folder) else { continue };
        let name = library::world_name(&dir, &world.folder);
        let made = snapshots.create(&dir, NewSnapshot { instance_id: id, world: &world.folder, name: &name, reason: SnapshotReason::Manual, label: Some(label.to_string()), now: now_ms() });
        if let Ok(summary) = made {
            note_event(app, Some(id), Some(&world.folder), library::restore_point_event(&summary));
            kept += 1;
        }
    }
    kept
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MovedInstance {
    pub instance: Instance,
    pub moved: Moved,
    /// Worlds a restore point was kept of first (moving to an older Minecraft).
    pub restore_points: u32,
}

/// Moves an instance to another Minecraft version or loader: its mods are swapped for their
/// builds for it, and the ones without one are turned off. Moving to an older Minecraft keeps a
/// restore point of every world first.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn move_instance(
    id: String,
    game_version: String,
    loader: Loader,
    loader_version: Option<String>,
    app: AppHandle,
    paths: State<'_, Paths>,
    settings: State<'_, SettingsState>,
    network: State<'_, Network>,
    launches: State<'_, Launches>,
) -> Result<MovedInstance, String> {
    not_running(&launches, &id)?;
    let (before, dir) = instance_and_dir(&paths, &id)?;
    let loader_version = match (loader, loader_version) {
        (Loader::Vanilla, _) => None,
        (_, Some(version)) => Some(version),
        (_, None) => Some(
            loaders::recommended(&network.client, loader, &game_version)
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| format!("{} doesn't support Minecraft {game_version} yet.", loader.display_name()))?,
        ),
    };
    let scan = scanned(&network, &dir).await?;
    let items = named_items(&paths, &before, &dir, &scan);
    let plan = scan.plan_move(&network.client, &items, &game_version, loader).await.map_err(|error| error.to_string())?;

    let older = loaders::compare_versions(&game_version, &before.game_version) == Ordering::Less;
    let restore_points = if older {
        let label = format!("Before moving to {game_version}");
        let (app, game_dir, id, snapshots) = (app.clone(), dir.join(GAME_DIR), id.clone(), paths.snapshots());
        blocking(move || Ok(keep_worlds(&app, &snapshots, &id, &game_dir, &label))).await?
    } else {
        0
    };
    let after = paths
        .instances()
        .update(&id, |stored| {
            stored.game_version = game_version.clone();
            stored.loader = loader;
            stored.loader_version = loader_version.clone();
        })
        .map_err(|error| error.to_string())?;
    let settings = current_settings(&settings);
    let key = curseforge::api_key(&settings.curseforge_api_key);
    let moved = inventory::apply_move(&network.downloads, &after, &dir, &plan, settings.concurrent_downloads as usize, key.as_deref(), now_ms()).await;
    note_event(&app, Some(&id), None, Event::VersionChanged { from: instance_label(&before), to: instance_label(&after) });
    Ok(MovedInstance { instance: after, moved, restore_points })
}

/// Copies an instance under a new name, with its worlds or without.
#[tauri::command]
pub async fn duplicate_instance(id: String, name: String, with_worlds: bool, app: AppHandle, paths: State<'_, Paths>, launches: State<'_, Launches>) -> Result<Instance, String> {
    if with_worlds {
        not_running(&launches, &id)?;
    }
    let (original, _) = instance_and_dir(&paths, &id)?;
    let instances = paths.instances();
    let copy = blocking(move || instances.duplicate(&id, &name, with_worlds, now_ms()).map_err(|error| error.to_string())).await?;
    note_event(&app, Some(&copy.id), None, Event::InstanceDuplicated { from: original.name });
    Ok(copy)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedFile {
    pub path: String,
    #[serde(flatten)]
    pub exported: Exported,
}

/// Saves an instance as a Modrinth modpack (.mrpack), asking where.
#[tauri::command]
pub async fn export_instance(id: String, options: ExportOptions, app: AppHandle, paths: State<'_, Paths>, network: State<'_, Network>) -> Result<Option<ExportedFile>, String> {
    let (instance, dir) = instance_and_dir(&paths, &id)?;
    let file_name: String = format!("{} {}", instance.name, options.version).chars().map(|c| if c.is_alphanumeric() || " -_.()".contains(c) { c } else { '_' }).collect();
    let picked = app
        .dialog()
        .file()
        .set_title("Export as a modpack")
        .set_file_name(format!("{}.mrpack", file_name.trim()))
        .add_filter("Modrinth modpack", &["mrpack"])
        .blocking_save_file();
    let Some(target) = picked.and_then(|file| file.into_path().ok()) else {
        return Ok(None);
    };
    let scan = scanned(&network, &dir).await?;
    let out = target.clone();
    let exported = blocking(move || modpack::export_mrpack(&instance, &dir, &scan, &out, &options).map_err(|error| format!("Couldn't export it: {error}"))).await?;
    Ok(Some(ExportedFile { path: target.to_string_lossy().into_owned(), exported }))
}

#[tauri::command]
pub async fn get_instance_logs(id: String, paths: State<'_, Paths>) -> Result<Vec<LogFile>, String> {
    let game_dir = paths.instances().game_dir(&id).ok_or(GONE)?;
    blocking(move || Ok(game_logs::list(&game_dir))).await
}

#[tauri::command]
pub async fn read_instance_log(id: String, name: String, paths: State<'_, Paths>) -> Result<String, String> {
    let game_dir = paths.instances().game_dir(&id).ok_or(GONE)?;
    blocking(move || game_logs::read(&game_dir, &name).map_err(|error| format!("Couldn't read it: {error}"))).await
}

/// Newer versions of the modpack an instance came from.
#[tauri::command]
pub async fn check_pack_updates(id: String, paths: State<'_, Paths>, network: State<'_, Network>) -> Result<PackUpdates, String> {
    let (instance, _) = instance_and_dir(&paths, &id)?;
    modpack::pack_updates(&network.client, &instance).await.map_err(|error| error.to_string())
}

/// Updates an instance to another version of its modpack. One for another Minecraft version
/// keeps a restore point of every world first.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn update_instance_pack(
    id: String,
    version_id: String,
    game_versions: Vec<String>,
    app: AppHandle,
    paths: State<'_, Paths>,
    settings: State<'_, SettingsState>,
    network: State<'_, Network>,
    launches: State<'_, Launches>,
) -> Result<PackUpdated, String> {
    not_running(&launches, &id)?;
    let (instance, dir) = instance_and_dir(&paths, &id)?;
    let title = instance.modpack.as_ref().map_or_else(|| instance.name.clone(), |origin| origin.title.clone());
    if !game_versions.contains(&instance.game_version) {
        let label = format!("Before updating {title}");
        let (app, game_dir, id, snapshots) = (app.clone(), dir.join(GAME_DIR), id.clone(), paths.snapshots());
        blocking(move || Ok(keep_worlds(&app, &snapshots, &id, &game_dir, &label))).await?;
    }
    let settings = current_settings(&settings);
    let cache = paths.data.join("cache").join("modpacks");
    let updated = modpack::update_pack(&network.downloads, &paths.instances(), &id, &version_id, &cache, settings.concurrent_downloads as usize, now_ms(), &|_| {})
        .await
        .map_err(|error| error.to_string())?;
    note_event(&app, Some(&id), None, Event::ModpackUpdated { title, from: updated.from.clone(), to: updated.to.clone() });
    Ok(updated)
}
