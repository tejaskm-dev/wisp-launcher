//! Settings that follow the player between instances on this computer: giving every instance one
//! instance's settings (Settings > Game), and instances that keep their own (Edit instance). The
//! sharing itself happens around each launch, in `play`.

use tauri::State;
use wisp_core::game_options;

use crate::{Paths, now_ms};

/// The instances that keep their own settings.
#[tauri::command]
pub async fn get_instances_keeping_settings(paths: State<'_, Paths>) -> Result<Vec<String>, String> {
    let data = paths.data.clone();
    tauri::async_runtime::spawn_blocking(move || game_options::keeping_own(&data).into_iter().collect())
        .await
        .map_err(|error| error.to_string())
}

/// Sets whether an instance keeps its own settings instead of sharing them.
#[tauri::command]
pub async fn set_instance_keeps_settings(id: String, keep: bool, paths: State<'_, Paths>) -> Result<(), String> {
    let (data, instances) = (paths.data.clone(), paths.instances());
    tauri::async_runtime::spawn_blocking(move || game_options::set_keeps_own(&data, &instances, &id, keep))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| format!("Couldn't save that: {error}"))
}

/// Gives every other instance all of one instance's settings, from the next time each starts.
#[tauri::command]
pub async fn share_instance_settings(id: String, paths: State<'_, Paths>) -> Result<usize, String> {
    let (data, instances) = (paths.data.clone(), paths.instances());
    tauri::async_runtime::spawn_blocking(move || game_options::share_all(&data, &instances, &id, now_ms()))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}
