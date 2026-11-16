//! Commands for what you've played: worlds across every instance, screenshots, the journal and
//! restore points. Also what happens when a game closes: the session goes into the journal and
//! the worlds that were played get a restore point.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use wisp_core::home::instance_label;
use wisp_core::instances::Instance;
use wisp_core::journal::{self, Entry, Event, JournalQuery};
use wisp_core::launch::LaunchUpdate;
use wisp_core::settings::JournalPrefs;
use wisp_core::snapshots::{NewSnapshot, SnapshotReason, Summary};
use wisp_core::{device, screenshots, storage, worlds};

use crate::{Launches, Paths, now_ms};

/// Sent (with no payload) whenever the journal changes, so open timelines refresh.
pub const JOURNAL_EVENT: &str = "journal";

pub fn journal_changed(app: &AppHandle) {
    let _ = app.emit(JOURNAL_EVENT, ());
}

/// Writes a journal entry, best effort: history is nice to have, never worth failing an action.
pub fn note_event(app: &AppHandle, instance_id: Option<&str>, world: Option<&str>, event: Event) {
    if let Some(paths) = app.try_state::<Paths>()
        && paths.journal().append(instance_id, world, now_ms(), event).is_ok()
    {
        journal_changed(app);
    }
}

fn busy(launches: &Launches, instance_id: &str) -> bool {
    launches
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains_key(instance_id)
}

const CLOSE_FIRST: &str = "Close Minecraft first: the game is using this world.";

fn game_dir(paths: &Paths, instance_id: &str) -> Result<PathBuf, String> {
    paths.instances().game_dir(instance_id).ok_or_else(|| "That instance no longer exists.".to_string())
}

fn world_dir(paths: &Paths, instance_id: &str, folder: &str) -> Result<PathBuf, String> {
    worlds::world_dir(&game_dir(paths, instance_id)?, folder).ok_or_else(|| "That world no longer exists.".to_string())
}

fn world_name(dir: &Path, folder: &str) -> String {
    worlds::read_info(dir).map_or_else(|| folder.to_string(), |info| info.name)
}

/// Moves a file or folder to the Trash. On macOS this asks the system directly rather than
/// scripting Finder, which would make macOS ask permission to "control Finder" first.
pub fn move_to_trash(path: &Path) -> Result<(), String> {
    let mut context = trash::TrashContext::default();
    #[cfg(target_os = "macos")]
    {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        context.set_delete_method(DeleteMethod::NsFileManager);
    }
    context.delete(path).map_err(|error| format!("Couldn't move it to the Trash: {error}"))
}

/// Runs file work on a thread meant for it.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work).await.map_err(|error| error.to_string())?
}

// ── Worlds ──

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldExtras {
    pub size_bytes: u64,
    pub restore_points: Vec<Summary>,
}

/// What the world page shows beyond the world list: its size and restore points.
#[tauri::command]
pub async fn get_world_extras(instance_id: String, folder: String, paths: State<'_, Paths>) -> Result<WorldExtras, String> {
    let dir = world_dir(&paths, &instance_id, &folder)?;
    let snapshots = paths.snapshots();
    blocking(move || {
        Ok(WorldExtras {
            size_bytes: storage::dir_size(&dir),
            restore_points: snapshots.for_world(&instance_id, &folder),
        })
    })
    .await
}

#[tauri::command]
pub async fn open_world_folder(instance_id: String, folder: String, app: AppHandle, paths: State<'_, Paths>) -> Result<(), String> {
    let dir = world_dir(&paths, &instance_id, &folder)?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|error| error.to_string())
}

/// Brings in worlds from zips or world folders (picked, or dropped on the window). Returns the
/// new worlds' folders.
async fn import_paths(app: &AppHandle, paths: &Paths, instance_id: &str, sources: Vec<PathBuf>) -> Result<Vec<String>, String> {
    let game = game_dir(paths, instance_id)?;
    let journal = paths.journal();
    let instance = instance_id.to_string();
    let imported = blocking(move || {
        let mut imported = Vec::new();
        let mut problems = Vec::new();
        for source in sources {
            let name = source.file_stem().map_or_else(|| "World".to_string(), |stem| stem.to_string_lossy().into_owned());
            let result = if source.is_dir() {
                worlds::import_folder(&source, &game, &name)
            } else {
                worlds::import_zip(&source, &game, &name)
            };
            match result {
                Ok(folder) => {
                    let shown = world_name(&game.join(worlds::SAVES).join(&folder), &folder);
                    let _ = journal.append(Some(&instance), Some(&folder), now_ms(), Event::WorldImported { name: shown });
                    imported.push(folder);
                }
                Err(error) => problems.push(format!("{name}: {error}")),
            }
        }
        if imported.is_empty() && !problems.is_empty() {
            return Err(format!("Couldn't import {}", problems.join("; ")));
        }
        Ok(imported)
    })
    .await?;
    journal_changed(app);
    Ok(imported)
}

#[tauri::command]
pub async fn import_world(instance_id: String, app: AppHandle, paths: State<'_, Paths>) -> Result<Vec<String>, String> {
    game_dir(&paths, &instance_id)?;
    let picked = app
        .dialog()
        .file()
        .set_title("Import worlds")
        .add_filter("Minecraft world (.zip)", &["zip"])
        .blocking_pick_files();
    let Some(picked) = picked else {
        return Ok(Vec::new());
    };
    let sources = picked.into_iter().filter_map(|file| file.into_path().ok()).collect();
    import_paths(&app, &paths, &instance_id, sources).await
}

/// For worlds dropped on the window: zips and world folders.
#[tauri::command]
pub async fn import_world_paths(instance_id: String, files: Vec<String>, app: AppHandle, paths: State<'_, Paths>) -> Result<Vec<String>, String> {
    import_paths(&app, &paths, &instance_id, files.into_iter().map(PathBuf::from).collect()).await
}

/// Saves a world as a zip where the player picks. Returns where, or None if they cancelled.
#[tauri::command]
pub async fn export_world(instance_id: String, folder: String, app: AppHandle, paths: State<'_, Paths>) -> Result<Option<String>, String> {
    let dir = world_dir(&paths, &instance_id, &folder)?;
    let name = world_name(&dir, &folder);
    let file_name: String = name.chars().map(|c| if c.is_alphanumeric() || " -_()".contains(c) { c } else { '_' }).collect();
    let picked = app
        .dialog()
        .file()
        .set_title("Export world")
        .set_file_name(format!("{}.zip", file_name.trim()))
        .add_filter("Zip archive", &["zip"])
        .blocking_save_file();
    let Some(target) = picked.and_then(|file| file.into_path().ok()) else {
        return Ok(None);
    };
    let zip = target.clone();
    blocking(move || worlds::export_zip(&dir, &zip).map_err(|error| format!("Couldn't export it: {error}"))).await?;
    note_event(&app, Some(&instance_id), Some(&folder), Event::WorldExported { name });
    Ok(Some(target.to_string_lossy().into_owned()))
}

/// Moves a world to the Trash (its restore points stay, so it can come back).
#[tauri::command]
pub async fn delete_world(
    instance_id: String,
    folder: String,
    app: AppHandle,
    paths: State<'_, Paths>,
    launches: State<'_, Launches>,
) -> Result<(), String> {
    if busy(&launches, &instance_id) {
        return Err(CLOSE_FIRST.into());
    }
    let dir = world_dir(&paths, &instance_id, &folder)?;
    let name = world_name(&dir, &folder);
    blocking(move || move_to_trash(&dir)).await?;
    note_event(&app, Some(&instance_id), Some(&folder), Event::WorldDeleted { name });
    Ok(())
}

// ── Screenshots ──

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotView {
    /// "<instance id>/<file>".
    pub id: String,
    pub instance_id: String,
    pub instance_name: String,
    pub file: String,
    /// Relative to Wisp's data folder.
    pub path: String,
    pub taken_at: i64,
    pub size_bytes: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// The world it was taken in, when the journal knows (a session in a single world).
    pub world: Option<String>,
    /// That world's folder.
    pub world_folder: Option<String>,
}

/// Screenshots from every instance, newest first.
#[tauri::command]
pub async fn get_screenshots(paths: State<'_, Paths>) -> Result<Vec<ScreenshotView>, String> {
    let instances = paths.instances();
    let journal = paths.journal();
    blocking(move || {
        let sessions = journal.list(&JournalQuery { kinds: vec!["session".into()], ..Default::default() });
        let mut all = Vec::new();
        for instance in instances.list() {
            let Some(game) = instances.game_dir(&instance.id) else {
                continue;
            };
            let shots = screenshots::list(&game);
            if shots.is_empty() {
                continue;
            }
            let names: HashMap<String, String> = worlds::list_info(&game).into_iter().map(|info| (info.folder, info.name)).collect();
            // Screenshot → world, from sessions that only touched one world.
            let mut taken_in: HashMap<&str, &str> = HashMap::new();
            for entry in sessions.iter().filter(|entry| entry.instance_id.as_deref() == Some(instance.id.as_str())) {
                if let Event::Session { worlds, screenshots, .. } = &entry.event
                    && let [world] = worlds.as_slice()
                {
                    for file in screenshots {
                        taken_in.insert(file, world);
                    }
                }
            }
            for shot in shots {
                let folder = taken_in.get(shot.file.as_str()).map(|folder| folder.to_string());
                all.push(ScreenshotView {
                    id: format!("{}/{}", instance.id, shot.file),
                    instance_id: instance.id.clone(),
                    instance_name: instance.name.clone(),
                    path: format!("instances/{}/{}/{}/{}", instance.id, wisp_core::instances::GAME_DIR, screenshots::SCREENSHOTS, shot.file),
                    world: folder.as_ref().map(|folder| names.get(folder).cloned().unwrap_or_else(|| folder.clone())),
                    world_folder: folder,
                    file: shot.file,
                    taken_at: shot.taken_at,
                    size_bytes: shot.size_bytes,
                    width: shot.width,
                    height: shot.height,
                });
            }
        }
        all.sort_by_key(|shot| std::cmp::Reverse(shot.taken_at));
        Ok(all)
    })
    .await
}

fn screenshot_path(paths: &Paths, instance_id: &str, file: &str) -> Result<PathBuf, String> {
    screenshots::file_path(&game_dir(paths, instance_id)?, file).ok_or_else(|| "That screenshot is gone.".to_string())
}

/// A screenshot's bytes, for copying it to the clipboard.
#[tauri::command]
pub async fn get_screenshot_bytes(instance_id: String, file: String, paths: State<'_, Paths>) -> Result<tauri::ipc::Response, String> {
    let path = screenshot_path(&paths, &instance_id, &file)?;
    let bytes = blocking(move || std::fs::read(&path).map_err(|error| error.to_string())).await?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Moves a screenshot to the Trash.
#[tauri::command]
pub async fn delete_screenshot(instance_id: String, file: String, paths: State<'_, Paths>) -> Result<(), String> {
    let path = screenshot_path(&paths, &instance_id, &file)?;
    blocking(move || move_to_trash(&path)).await
}

/// Shows a screenshot in Finder or Explorer.
#[tauri::command]
pub async fn reveal_screenshot(instance_id: String, file: String, app: AppHandle, paths: State<'_, Paths>) -> Result<(), String> {
    let path = screenshot_path(&paths, &instance_id, &file)?;
    app.opener().reveal_item_in_dir(path).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn open_screenshots_folder(instance_id: String, app: AppHandle, paths: State<'_, Paths>) -> Result<(), String> {
    let dir = game_dir(&paths, &instance_id)?.join(screenshots::SCREENSHOTS);
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|error| error.to_string())
}

// ── Journal ──

#[tauri::command]
pub async fn get_journal(query: JournalQuery, paths: State<'_, Paths>) -> Result<Vec<Entry>, String> {
    let journal = paths.journal();
    blocking(move || Ok(journal.list(&query))).await
}

#[tauri::command]
pub async fn add_note(
    instance_id: Option<String>,
    world: Option<String>,
    text: String,
    session: Option<String>,
    app: AppHandle,
    paths: State<'_, Paths>,
) -> Result<Entry, String> {
    let text = journal::clean_note(&text).map_err(|error| error.to_string())?;
    let entry = paths
        .journal()
        .append(instance_id.as_deref(), world.as_deref(), now_ms(), Event::Note { text, session })
        .map_err(|error| error.to_string())?;
    journal_changed(&app);
    Ok(entry)
}

#[tauri::command]
pub async fn edit_note(id: String, text: String, app: AppHandle, paths: State<'_, Paths>) -> Result<(), String> {
    paths.journal().edit_note(&id, &text).map_err(|error| error.to_string())?;
    journal_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn delete_note(id: String, app: AppHandle, paths: State<'_, Paths>) -> Result<(), String> {
    paths.journal().delete_note(&id).map_err(|error| error.to_string())?;
    journal_changed(&app);
    Ok(())
}

// ── Restore points ──

#[tauri::command]
pub async fn get_restore_points(paths: State<'_, Paths>) -> Result<Vec<Summary>, String> {
    let snapshots = paths.snapshots();
    blocking(move || Ok(snapshots.list())).await
}

#[tauri::command]
pub async fn get_restore_point_usage(paths: State<'_, Paths>) -> Result<u64, String> {
    let snapshots = paths.snapshots();
    blocking(move || Ok(snapshots.usage())).await
}

fn restore_point_event(summary: &Summary) -> Event {
    Event::RestorePoint {
        snapshot: summary.id.clone(),
        reason: summary.reason,
        files: summary.files,
        total_bytes: summary.total_bytes,
        added_bytes: summary.added_bytes,
        label: summary.label.clone(),
    }
}

#[tauri::command]
pub async fn create_restore_point(
    instance_id: String,
    folder: String,
    label: Option<String>,
    app: AppHandle,
    paths: State<'_, Paths>,
    launches: State<'_, Launches>,
) -> Result<Summary, String> {
    if busy(&launches, &instance_id) {
        return Err(CLOSE_FIRST.into());
    }
    let dir = world_dir(&paths, &instance_id, &folder)?;
    let snapshots = paths.snapshots();
    let (instance, world) = (instance_id.clone(), folder.clone());
    let summary = blocking(move || {
        let name = world_name(&dir, &world);
        snapshots
            .create(&dir, NewSnapshot { instance_id: &instance, world: &world, name: &name, reason: SnapshotReason::Manual, label, now: now_ms() })
            .map_err(|error| format!("Couldn't make the restore point: {error}"))
    })
    .await?;
    note_event(&app, Some(&instance_id), Some(&folder), restore_point_event(&summary));
    Ok(summary)
}

/// Puts a world back the way a restore point has it. The world as it is now is kept as a
/// restore point first, so this can be undone.
#[tauri::command]
pub async fn restore_world(id: String, app: AppHandle, paths: State<'_, Paths>, launches: State<'_, Launches>) -> Result<Summary, String> {
    let snapshots = paths.snapshots();
    let target = snapshots.list().into_iter().find(|summary| summary.id == id).ok_or("That restore point is gone.")?;
    if busy(&launches, &target.instance_id) {
        return Err(CLOSE_FIRST.into());
    }
    let game = game_dir(&paths, &target.instance_id)?;
    // The world may have been deleted since; restoring brings it back under its old folder.
    let plain = !target.world.is_empty() && !target.world.contains(['/', '\\']) && !target.world.starts_with('.');
    if !plain {
        return Err("That restore point names an odd folder.".into());
    }
    let dir = game.join(worlds::SAVES).join(&target.world);
    let restored = target.clone();
    let before = blocking(move || {
        let before = if dir.join("level.dat").is_file() {
            Some(
                snapshots
                    .create(
                        &dir,
                        NewSnapshot {
                            instance_id: &restored.instance_id,
                            world: &restored.world,
                            name: &world_name(&dir, &restored.world),
                            reason: SnapshotReason::BeforeRestore,
                            label: None,
                            now: now_ms(),
                        },
                    )
                    .map_err(|error| format!("Couldn't save the world as it is first: {error}"))?,
            )
        } else {
            None
        };
        snapshots.restore(&restored.id, &dir).map_err(|error| format!("Couldn't restore it: {error}"))?;
        Ok(before)
    })
    .await?;
    if let Some(before) = &before {
        note_event(&app, Some(&target.instance_id), Some(&target.world), restore_point_event(before));
    }
    note_event(&app, Some(&target.instance_id), Some(&target.world), Event::Restored { snapshot: target.id.clone() });
    Ok(target)
}

/// Deletes a restore point; returns the bytes it freed.
#[tauri::command]
pub async fn delete_restore_point(id: String, app: AppHandle, paths: State<'_, Paths>) -> Result<u64, String> {
    let snapshots = paths.snapshots();
    let freed = blocking(move || snapshots.delete(&id).map_err(|error| error.to_string())).await?;
    journal_changed(&app);
    Ok(freed)
}

// ── After a game closes ──

/// Writes the session to the journal, then takes restore points of the worlds that were played
/// (reporting "saving" while it does). Runs on a blocking thread.
#[allow(clippy::too_many_arguments)]
pub fn after_session(
    app: &AppHandle,
    instance: &Instance,
    game_dir: &Path,
    started_at: i64,
    ended_at: i64,
    crashed: bool,
    prefs: &JournalPrefs,
    report: &dyn Fn(LaunchUpdate),
) {
    let Some(paths) = app.try_state::<Paths>() else {
        return;
    };
    let journal = paths.journal();
    let facts = journal::session_facts(game_dir, started_at, ended_at);
    let only_world = match facts.worlds.as_slice() {
        [world] => Some(world.clone()),
        _ => None,
    };
    let session = Event::Session {
        started_at,
        ended_at,
        crashed,
        worlds: facts.worlds.clone(),
        new_worlds: facts.new_worlds,
        screenshots: facts.screenshots,
        device: device::DeviceInfo::current().name,
        label: instance_label(instance),
    };
    let _ = journal.append(Some(&instance.id), only_world.as_deref(), ended_at, session);
    journal_changed(app);

    // A crash can leave a world half-saved; the restore point before it is the one to keep.
    if crashed || !prefs.auto_restore_points {
        return;
    }
    let snapshots = paths.snapshots();
    for folder in &facts.worlds {
        let Some(dir) = worlds::world_dir(game_dir, folder) else {
            continue;
        };
        let name = world_name(&dir, folder);
        report(LaunchUpdate::Saving { world: name.clone() });
        let created = snapshots.create(
            &dir,
            NewSnapshot { instance_id: &instance.id, world: folder, name: &name, reason: SnapshotReason::Auto, label: None, now: now_ms() },
        );
        if let Ok(summary) = created {
            let _ = journal.append(Some(&instance.id), Some(folder), now_ms(), restore_point_event(&summary));
            let _ = snapshots.prune(&instance.id, folder, prefs.keep_restore_points as usize);
        }
    }
    journal_changed(app);
}
