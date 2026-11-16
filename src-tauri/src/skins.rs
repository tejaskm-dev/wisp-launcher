//! Commands for the skin library: listing, importing (picked, dropped or by player name),
//! saving edits, picking the one to wear, and exporting.

use std::path::PathBuf;

use base64::Engine;
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use wisp_core::skins::{self, PlayerSkin, Skin, SkinLibrary, SkinModel};

use crate::{Network, Paths, now_ms};

fn decode(png: &str) -> Result<Vec<u8>, String> {
    // Accept a data URL as well as bare base64.
    let data = png.split_once("base64,").map_or(png, |(_, data)| data);
    base64::engine::general_purpose::STANDARD
        .decode(data.trim())
        .map_err(|_| "That picture didn't come through. Try again.".to_string())
}

#[tauri::command]
pub async fn get_skins(paths: State<'_, Paths>) -> Result<SkinLibrary, String> {
    Ok(paths.skins().library())
}

/// A skin's PNG as base64. The page uses it as a data URL: the 3D viewer and editor read its
/// pixels, which a picture from another origin (even wisp://) can't allow everywhere.
#[tauri::command]
pub async fn get_skin_png(id: String, paths: State<'_, Paths>) -> Result<String, String> {
    let bytes = std::fs::read(paths.skins().png_path(&id)).map_err(|_| "That skin is gone.".to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// Adds a skin from PNG data (base64). With no model given, it's guessed from the picture.
#[tauri::command]
pub async fn add_skin(
    name: String,
    model: Option<SkinModel>,
    png: String,
    source: Option<String>,
    paths: State<'_, Paths>,
) -> Result<Skin, String> {
    let bytes = decode(&png)?;
    let model = model.unwrap_or_else(|| skins::guess_model(&bytes));
    paths.skins().add(&name, model, &bytes, source, now_ms()).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn update_skin(
    id: String,
    name: Option<String>,
    model: Option<SkinModel>,
    png: Option<String>,
    paths: State<'_, Paths>,
) -> Result<Skin, String> {
    let bytes = png.as_deref().map(decode).transpose()?;
    paths
        .skins()
        .update(&id, name.as_deref(), model, bytes.as_deref(), now_ms())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn delete_skin(id: String, paths: State<'_, Paths>) -> Result<(), String> {
    paths.skins().remove(&id).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_active_skin(id: Option<String>, paths: State<'_, Paths>) -> Result<(), String> {
    paths.skins().set_active(id.as_deref()).map_err(|error| error.to_string())
}

/// Any Minecraft player's current skin, by name (from Mojang's public API).
#[tauri::command]
pub async fn fetch_player_skin(username: String, network: State<'_, Network>) -> Result<PlayerSkin, String> {
    skins::fetch_player_skin(&network.client, &username).await.map_err(|error| error.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinImport {
    pub added: Vec<Skin>,
    /// "file.png: why", for the ones that weren't skins.
    pub failed: Vec<String>,
}

fn import_files(paths: &Paths, files: Vec<PathBuf>) -> SkinImport {
    let store = paths.skins();
    let mut result = SkinImport { added: Vec::new(), failed: Vec::new() };
    for file in files {
        let label = file.file_name().map_or_else(|| "file".to_string(), |name| name.to_string_lossy().into_owned());
        let name = file.file_stem().map_or_else(|| "Skin".to_string(), |stem| stem.to_string_lossy().into_owned());
        let added = std::fs::read(&file)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                let model = skins::guess_model(&bytes);
                store.add(&name, model, &bytes, Some("file".into()), now_ms()).map_err(|error| error.to_string())
            });
        match added {
            Ok(skin) => result.added.push(skin),
            Err(error) => result.failed.push(format!("{label}: {error}")),
        }
    }
    result
}

/// Opens a file picker for skin PNGs.
#[tauri::command]
pub async fn pick_skin_files(app: AppHandle, paths: State<'_, Paths>) -> Result<SkinImport, String> {
    let picked = app
        .dialog()
        .file()
        .set_title("Import skins")
        .add_filter("Minecraft skin (.png)", &["png"])
        .blocking_pick_files()
        .unwrap_or_default();
    Ok(import_files(&paths, picked.into_iter().filter_map(|file| file.into_path().ok()).collect()))
}

/// For skin PNGs dropped on the window.
#[tauri::command]
pub async fn import_skin_paths(files: Vec<String>, paths: State<'_, Paths>) -> Result<SkinImport, String> {
    Ok(import_files(&paths, files.into_iter().map(PathBuf::from).collect()))
}

/// Saves a skin's PNG where the player picks. Returns where, or None if they cancelled.
#[tauri::command]
pub async fn export_skin(id: String, app: AppHandle, paths: State<'_, Paths>) -> Result<Option<String>, String> {
    let store = paths.skins();
    let skin = store.library().skins.into_iter().find(|skin| skin.id == id).ok_or("That skin is gone.")?;
    let file_name: String = skin.name.chars().map(|c| if c.is_alphanumeric() || " -_".contains(c) { c } else { '_' }).collect();
    let picked = app
        .dialog()
        .file()
        .set_title("Export skin")
        .set_file_name(format!("{}.png", file_name.trim()))
        .add_filter("PNG picture", &["png"])
        .blocking_save_file();
    let Some(target) = picked.and_then(|file| file.into_path().ok()) else {
        return Ok(None);
    };
    std::fs::copy(store.png_path(&id), &target).map_err(|error| format!("Couldn't save it: {error}"))?;
    Ok(Some(target.to_string_lossy().into_owned()))
}
