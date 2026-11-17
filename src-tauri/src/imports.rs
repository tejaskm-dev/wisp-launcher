//! Files you bring in yourself, picked or dropped on the window: modpacks become instances of
//! their own (see `install_modpack_file`), and mods, resource packs and shader packs go into an
//! instance you choose. Worlds and data packs go through their own imports.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use wisp_core::imports::{self, Added, Inspected};

use crate::{Network, Paths};

/// Opens a file picker for modpacks, mods and packs. Returns the files picked (none if cancelled).
#[tauri::command]
pub async fn pick_import_files(app: AppHandle) -> Result<Vec<String>, String> {
    let picked = app
        .dialog()
        .file()
        .set_title("Add files")
        .add_filter("Modpacks, mods and packs", &["mrpack", "jar", "zip"])
        .blocking_pick_files()
        .unwrap_or_default();
    Ok(picked
        .into_iter()
        .filter_map(|file| file.into_path().ok())
        .map(|path| path.to_string_lossy().into_owned())
        .collect())
}

/// Says what each file is (a modpack, a mod, a resource pack…) before anything is done with it.
#[tauri::command]
pub async fn inspect_import_files(paths: Vec<String>) -> Result<Vec<Inspected>, String> {
    tauri::async_runtime::spawn_blocking(move || paths.iter().map(|path| imports::inspect(Path::new(path))).collect())
        .await
        .map_err(|error| error.to_string())
}

/// How adding one file went.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddedFile {
    path: String,
    added: Option<Added>,
    error: Option<String>,
}

/// Adds mods, resource packs and shader packs to an instance, each looked up on Modrinth so it
/// shows with its real name and can update.
#[tauri::command]
pub async fn add_content_files(instance_id: String, files: Vec<String>, paths: State<'_, Paths>, network: State<'_, Network>) -> Result<Vec<AddedFile>, String> {
    let instances = paths.instances();
    let instance = instances.get(&instance_id).ok_or("That instance no longer exists.")?;
    let dir = instances.dir_of(&instance_id).ok_or("That instance no longer exists.")?;
    let mut results = Vec::with_capacity(files.len());
    for file in files {
        let outcome = imports::add_to_instance(&network.client, &instance, &dir, &PathBuf::from(&file)).await;
        results.push(match outcome {
            Ok(added) => AddedFile { path: file, added: Some(added), error: None },
            Err(error) => AddedFile { path: file, added: None, error: Some(error.to_string()) },
        });
    }
    Ok(results)
}
