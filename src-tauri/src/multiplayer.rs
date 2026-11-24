//! Multiplayer: the server lists in each instance, live status, and Discover's public servers.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::State;
use wisp_core::servers::{self, PublicServer, SavedServer, ServerStatus};

use crate::{Launches, Paths, library};

/// How long a server's status is reused before it's asked again.
const FRESH: Duration = Duration::from_secs(60);
const CLOSE_GAME: &str = "Close Minecraft first: it rewrites its server list when it quits.";

#[derive(Default)]
pub struct StatusCache(Mutex<HashMap<String, (Instant, ServerStatus)>>);

/// Each server's status, in the order asked. Ones asked for in the last minute come from memory
/// unless `refresh` is set.
#[tauri::command]
pub async fn get_server_statuses(addresses: Vec<String>, refresh: Option<bool>, cache: State<'_, StatusCache>, app: tauri::AppHandle) -> Result<Vec<ServerStatus>, String> {
    let mut known: HashMap<String, ServerStatus> = HashMap::new();
    if !refresh.unwrap_or(false) {
        let cached = cache.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        for address in &addresses {
            if let Some((at, status)) = cached.get(address)
                && at.elapsed() < FRESH
            {
                known.insert(address.clone(), status.clone());
            }
        }
    }
    let mut asking = HashSet::new();
    let missing: Vec<String> = addresses.iter().filter(|address| !known.contains_key(*address) && asking.insert((*address).clone())).cloned().collect();
    if !missing.is_empty() {
        let answers = servers::statuses(missing).await;
        let mut cached = cache.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        for status in answers {
            // A server hosted with Wisp says which key its skins are signed with: trust it, so
            // joining it from the game's own list shows everyone's skins.
            if let Some(key) = &status.wisp_skin_key {
                crate::trust_skin_key(&app, key);
            }
            cached.insert(status.address.clone(), (Instant::now(), status.clone()));
            known.insert(status.address.clone(), status);
        }
    }
    Ok(addresses.iter().filter_map(|address| known.get(address).cloned()).collect())
}

#[tauri::command]
pub fn get_public_servers() -> Vec<PublicServer> {
    servers::public_servers()
}

#[tauri::command]
pub async fn add_server(instance_id: String, name: String, address: String, paths: State<'_, Paths>, launches: State<'_, Launches>) -> Result<SavedServer, String> {
    if library::busy(&launches, &instance_id) {
        return Err(CLOSE_GAME.into());
    }
    let instances = paths.instances();
    instances.get(&instance_id).ok_or("That instance no longer exists.")?;
    let game_dir = instances.game_dir(&instance_id).ok_or("That instance no longer exists.")?;
    servers::add(&game_dir, &name, &address).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn remove_server(instance_id: String, address: String, paths: State<'_, Paths>, launches: State<'_, Launches>) -> Result<(), String> {
    if library::busy(&launches, &instance_id) {
        return Err(CLOSE_GAME.into());
    }
    let game_dir = paths.instances().game_dir(&instance_id).ok_or("That instance no longer exists.")?;
    servers::remove(&game_dir, &address).map(|_| ()).map_err(|error| error.to_string())
}
