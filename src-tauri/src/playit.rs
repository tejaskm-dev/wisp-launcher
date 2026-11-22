//! A public address for hosted servers through playit.gg.
//!
//! Tester and developer builds made with the "playit" feature run playit's own agent inside Wisp
//! (wisp_core::playit): the player approves this computer once on playit's page, and from then on
//! each server gets its tunnel and address by itself. Public builds carry none of playit's code
//! (playit's terms ask for their written consent first); there the server page walks the player
//! through playit's own app instead, and Wisp checks the address they paste.

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::Paths;

/// Sent while linking: "linked" or "failed".
#[cfg_attr(not(feature = "playit"), allow(dead_code))]
pub const PLAYIT_EVENT: &str = "playit";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayitView {
    /// This build runs playit's agent itself.
    built_in: bool,
    /// This computer is linked to a playit account.
    linked: bool,
    /// The claim page, while waiting for the player to approve.
    linking: Option<String>,
    /// The agent is carrying players right now.
    running: bool,
    /// playit's licence, which builds carrying its agent show (in About).
    notice: Option<&'static str>,
}

#[cfg(feature = "playit")]
const NOTICE: Option<&str> = Some(concat!("Wisp's tester builds include playit.gg's agent, under this licence:\n\n", include_str!("../licenses/playit-agent.txt")));
#[cfg(not(feature = "playit"))]
const NOTICE: Option<&str> = None;

fn secret_path(paths: &Paths) -> std::path::PathBuf {
    paths.data.join("playit").join("agent-secret")
}

/// This computer's playit secret, once linked.
pub fn secret(app: &AppHandle) -> Option<String> {
    let text = std::fs::read_to_string(secret_path(&app.state::<Paths>())).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(feature = "playit")]
mod built_in {
    use std::sync::Mutex as SyncMutex;

    use tauri::{AppHandle, Emitter, Manager};
    use tauri_plugin_opener::OpenerExt;
    use wisp_core::playit::{self, Agent};

    use super::{PLAYIT_EVENT, secret, secret_path};
    use crate::Paths;

    #[derive(Default)]
    pub struct Playit {
        /// The claim code being approved.
        pub linking: SyncMutex<Option<String>>,
        pub agent: tokio::sync::Mutex<Option<Agent>>,
    }

    #[derive(Clone, serde::Serialize)]
    #[serde(tag = "kind", rename_all = "kebab-case")]
    enum Event {
        Linked,
        Failed { message: String },
    }

    fn lock<T>(mutex: &SyncMutex<T>) -> std::sync::MutexGuard<'_, T> {
        mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn linking_url(app: &AppHandle) -> Option<String> {
        lock(&app.state::<Playit>().linking).as_deref().map(playit::claim_url)
    }

    pub fn running(app: &AppHandle) -> bool {
        app.state::<Playit>().agent.try_lock().ok().is_some_and(|agent| agent.as_ref().is_some_and(Agent::running))
    }

    /// Opens playit's claim page and waits (in the background) for the player to approve.
    pub fn link(app: &AppHandle) -> Result<String, String> {
        let code = {
            let state = app.state::<Playit>();
            let mut linking = lock(&state.linking);
            linking.get_or_insert_with(playit::claim_code).clone()
        };
        let url = playit::claim_url(&code);
        let _ = app.opener().open_url(&url, None::<&str>);
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let version = format!("Wisp {}", env!("CARGO_PKG_VERSION"));
            let result = playit::claim(&code, &version, &|_| {}).await;
            let still_ours = lock(&app.state::<Playit>().linking).as_deref() == Some(code.as_str());
            if !still_ours {
                return;
            }
            lock(&app.state::<Playit>().linking).take();
            let event = match result {
                Ok(secret) => match save(&app, &secret) {
                    Ok(()) => Event::Linked,
                    Err(error) => Event::Failed { message: format!("Couldn't keep playit's key: {error}") },
                },
                Err(error) => Event::Failed { message: error.to_string() },
            };
            let _ = app.emit(PLAYIT_EVENT, event);
        });
        Ok(url)
    }

    fn save(app: &AppHandle, secret: &str) -> std::io::Result<()> {
        let path = secret_path(&app.state::<Paths>());
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        std::fs::write(&path, secret)?;
        // Only this user reads it.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    pub async fn unlink(app: &AppHandle) {
        lock(&app.state::<Playit>().linking).take();
        if let Some(agent) = app.state::<Playit>().agent.lock().await.take() {
            agent.stop();
        }
        let _ = std::fs::remove_file(secret_path(&app.state::<Paths>()));
    }

    /// The tunnel to a server's port (made if needed), and its address.
    pub async fn tunnel_for(app: &AppHandle, name: &str, port: u16) -> Result<String, String> {
        let secret = secret(app).ok_or("Link Wisp to playit.gg first.")?;
        let tunnel = playit::tunnel_to(&secret, name, port).await.map_err(|error| error.to_string())?;
        Ok(tunnel.address)
    }

    /// Starts the agent, if this computer is linked and it isn't going already.
    pub async fn ensure_running(app: &AppHandle) -> Result<(), String> {
        let Some(secret) = secret(app) else {
            return Ok(());
        };
        let state = app.state::<Playit>();
        let mut agent = state.agent.lock().await;
        if agent.as_ref().is_some_and(Agent::running) {
            return Ok(());
        }
        *agent = Some(Agent::start(&secret).await.map_err(|error| error.to_string())?);
        Ok(())
    }

    pub async fn stop(app: &AppHandle) {
        if let Some(agent) = app.state::<Playit>().agent.lock().await.take() {
            agent.stop();
        }
    }
}

#[cfg(feature = "playit")]
pub use built_in::Playit;

/// Registers what the built-in agent needs (nothing in builds without it).
pub fn manage(app: &tauri::App) {
    #[cfg(feature = "playit")]
    app.manage(Playit::default());
    #[cfg(not(feature = "playit"))]
    let _ = app;
}

#[tauri::command]
pub async fn get_playit(app: AppHandle) -> Result<PlayitView, String> {
    #[cfg(feature = "playit")]
    return Ok(PlayitView { built_in: true, linked: secret(&app).is_some(), linking: built_in::linking_url(&app), running: built_in::running(&app), notice: NOTICE });
    #[cfg(not(feature = "playit"))]
    Ok(PlayitView { built_in: false, linked: secret(&app).is_some(), linking: None, running: false, notice: NOTICE })
}

/// Opens playit's page to link this computer, and returns its address (for showing too).
#[tauri::command]
pub async fn link_playit(app: AppHandle) -> Result<String, String> {
    #[cfg(feature = "playit")]
    return built_in::link(&app);
    #[cfg(not(feature = "playit"))]
    {
        let _ = app;
        Err(NOT_BUILT_IN.into())
    }
}

#[tauri::command]
pub async fn unlink_playit(app: AppHandle) -> Result<(), String> {
    #[cfg(feature = "playit")]
    built_in::unlink(&app).await;
    #[cfg(not(feature = "playit"))]
    let _ = app;
    Ok(())
}

#[cfg(not(feature = "playit"))]
const NOT_BUILT_IN: &str = "This copy of Wisp doesn't run playit itself. Use playit's own app: the server's page shows how.";

/// The tunnel to a server's port, made if needed (built-in agent only).
pub async fn tunnel_for(app: &AppHandle, name: &str, port: u16) -> Result<String, String> {
    #[cfg(feature = "playit")]
    return built_in::tunnel_for(app, name, port).await;
    #[cfg(not(feature = "playit"))]
    {
        let _ = (app, name, port);
        Err(NOT_BUILT_IN.into())
    }
}

/// Starts the agent for a server that's starting (when this computer is linked).
pub async fn server_starting(app: &AppHandle) -> Result<(), String> {
    #[cfg(feature = "playit")]
    return built_in::ensure_running(app).await;
    #[cfg(not(feature = "playit"))]
    {
        let _ = app;
        Ok(())
    }
}

/// Stops the agent once no server here needs it.
pub async fn servers_stopped(app: &AppHandle) {
    #[cfg(feature = "playit")]
    built_in::stop(app).await;
    #[cfg(not(feature = "playit"))]
    let _ = app;
}
