//! Commands for the skin library: listing, importing (picked, dropped or by player name),
//! saving edits, picking the one to wear, and exporting.

use std::path::PathBuf;

use base64::Engine;
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use wisp_core::skins::{self, Cosmetic, PlayerSkin, Skin, SkinLayer, SkinLibrary, SkinModel};

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

/// The editor's layers for a skin, if it was saved with them.
#[tauri::command]
pub async fn get_skin_layers(id: String, paths: State<'_, Paths>) -> Result<Option<Vec<SkinLayer>>, String> {
    Ok(paths.skins().layers(&id))
}

/// Keeps the editor's layers beside a skin; an empty list forgets them.
#[tauri::command]
pub async fn set_skin_layers(id: String, layers: Vec<SkinLayer>, paths: State<'_, Paths>) -> Result<(), String> {
    let layers: Vec<SkinLayer> = layers.into_iter().map(|layer| SkinLayer { png: strip_data_url(&layer.png).to_string(), ..layer }).collect();
    paths.skins().set_layers(&id, &layers).map_err(|error| error.to_string())
}

fn strip_data_url(png: &str) -> &str {
    png.split_once("base64,").map_or(png, |(_, data)| data)
}

#[tauri::command]
pub async fn delete_skin(id: String, paths: State<'_, Paths>) -> Result<(), String> {
    paths.skins().remove(&id).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_active_skin(id: Option<String>, paths: State<'_, Paths>) -> Result<(), String> {
    paths.skins().set_active(id.as_deref()).map_err(|error| error.to_string())
}

// ── Cosmetics: outer layers to wear over any skin ──

/// Keeps the outer layer of a skin picture (base64) as a cosmetic.
#[tauri::command]
pub async fn add_cosmetic(name: String, png: String, paths: State<'_, Paths>) -> Result<Cosmetic, String> {
    let bytes = decode(&png)?;
    paths.skins().add_cosmetic(&name, &bytes, now_ms()).map_err(|error| error.to_string())
}

/// A cosmetic's picture (a 64×64 skin with only its outer layer), as base64.
#[tauri::command]
pub async fn get_cosmetic_png(id: String, paths: State<'_, Paths>) -> Result<String, String> {
    let bytes = std::fs::read(paths.skins().cosmetic_path(&id)).map_err(|_| "That cosmetic is gone.".to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

#[tauri::command]
pub async fn rename_cosmetic(id: String, name: String, paths: State<'_, Paths>) -> Result<(), String> {
    paths.skins().rename_cosmetic(&id, &name).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn delete_cosmetic(id: String, paths: State<'_, Paths>) -> Result<(), String> {
    paths.skins().remove_cosmetic(&id).map_err(|error| error.to_string())
}

/// Wears a cosmetic over the worn skin, in Wisp and in game (none takes it off).
#[tauri::command]
pub async fn wear_cosmetic(id: Option<String>, paths: State<'_, Paths>) -> Result<(), String> {
    paths.skins().wear_cosmetic(id.as_deref()).map_err(|error| error.to_string())
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

/// The template an image tool paints (four views of a blank character), as base64 PNG, the
/// prompt that goes with it, and the follow-up prompt for the outer layer.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinTemplate {
    png: String,
    prompt: String,
    outer_prompt: String,
}

fn template_png(model: SkinModel) -> Result<Vec<u8>, String> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    wisp_core::turnaround::template(model).write_to(&mut bytes, image::ImageFormat::Png).map_err(|error| error.to_string())?;
    Ok(bytes.into_inner())
}

#[tauri::command]
pub async fn get_skin_template(model: SkinModel) -> Result<SkinTemplate, String> {
    Ok(SkinTemplate {
        png: base64::engine::general_purpose::STANDARD.encode(template_png(model)?),
        prompt: wisp_core::turnaround::PROMPT.to_string(),
        outer_prompt: wisp_core::turnaround::OUTER_PROMPT.to_string(),
    })
}

/// Saves the template where the player picks, to attach in their image tool.
#[tauri::command]
pub async fn save_skin_template(model: SkinModel, app: AppHandle) -> Result<Option<String>, String> {
    let picked = app.dialog().file().set_title("Save the template").set_file_name("Wisp skin template.png").add_filter("PNG picture", &["png"]).blocking_save_file();
    let Some(target) = picked.and_then(|file| file.into_path().ok()) else {
        return Ok(None);
    };
    std::fs::write(&target, template_png(model)?).map_err(|error| format!("Couldn't save it: {error}"))?;
    Ok(Some(target.to_string_lossy().into_owned()))
}

/// Reads a painted template (what the image tool made) back into a skin, as base64 PNG; with
/// `outer`, the follow-up painting of what sits on top, its outer layer from that.
#[tauri::command]
pub async fn skin_from_painting(picture: String, model: SkinModel, outer: Option<String>) -> Result<String, String> {
    let open = |data: &str| -> Result<image::RgbaImage, String> {
        let bytes = decode(data)?;
        Ok(image::load_from_memory(&bytes).map_err(|_| "Wisp can't read that picture. Try a PNG, JPEG or WebP.".to_string())?.to_rgba8())
    };
    let painting = open(&picture)?;
    let skin = match outer {
        Some(outer) => wisp_core::turnaround::read_layers(&painting, &open(&outer)?, model),
        None => wisp_core::turnaround::read(&painting, model),
    }
    .map_err(|error| error.to_string())?;
    let mut png = std::io::Cursor::new(Vec::new());
    skin.write_to(&mut png, image::ImageFormat::Png).map_err(|error| error.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(png.into_inner()))
}
