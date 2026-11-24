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

/// Sent while linking: "step" (where linking is), "linked" or "failed".
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
    /// Something wrong with the link that the player should know (playit.gg forgot this computer).
    problem: Option<String>,
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

/// The link being approved, kept so it isn't lost if Wisp closes meanwhile: playit.gg makes an
/// agent on the account for each one, and an account only has room for so many.
#[cfg_attr(not(feature = "playit"), allow(dead_code))]
fn claim_path(paths: &Paths) -> std::path::PathBuf {
    paths.data.join("playit").join("linking.json")
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
    use std::time::{Duration, Instant};

    use tauri::{AppHandle, Emitter, Manager};
    use tauri_plugin_opener::OpenerExt;
    use wisp_core::playit::{self, Agent, ClaimStep, PlayitError};

    use super::{PLAYIT_EVENT, claim_path, secret, secret_path};
    use crate::{Paths, now_ms};

    /// How long a link waits to be approved (playit's codes last about this long).
    const CLAIM_LASTS_MS: i64 = 15 * 60 * 1000;
    /// How often whether playit.gg still knows this computer is asked again.
    const CHECK_EVERY: Duration = Duration::from_secs(60);

    #[derive(Default)]
    pub struct Playit {
        /// The claim code being approved.
        pub linking: SyncMutex<Option<String>>,
        /// A wait for approval is going (one at a time, however often the page is opened).
        waiting: std::sync::atomic::AtomicBool,
        pub agent: tokio::sync::Mutex<Option<Agent>>,
        /// The last check of the link: when, and what was wrong (None when fine).
        checked: SyncMutex<Option<(Instant, Option<String>)>>,
    }

    #[derive(Clone, serde::Serialize)]
    #[serde(tag = "kind", rename_all = "kebab-case")]
    enum Event {
        /// "visit" (the page isn't open yet), "approve" (open, waiting), "approved" (finishing).
        Step { step: &'static str },
        Linked,
        Failed { message: String },
    }

    #[derive(serde::Serialize, serde::Deserialize)]
    struct Pending {
        code: String,
        at: i64,
    }

    /// The link being approved, from before Wisp last closed, if it's still young enough.
    fn pending(app: &AppHandle) -> Option<String> {
        let text = std::fs::read_to_string(claim_path(&app.state::<Paths>())).ok()?;
        let pending: Pending = serde_json::from_str(&text).ok()?;
        (now_ms() - pending.at < CLAIM_LASTS_MS).then_some(pending.code)
    }

    fn keep_pending(app: &AppHandle, code: &str) {
        let path = claim_path(&app.state::<Paths>());
        if let Some(folder) = path.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        if let Ok(text) = serde_json::to_string(&Pending { code: code.to_string(), at: now_ms() }) {
            let _ = std::fs::write(path, text);
        }
    }

    fn forget_pending(app: &AppHandle) {
        let _ = std::fs::remove_file(claim_path(&app.state::<Paths>()));
    }

    /// Carries on with a link from before Wisp closed, if one was waiting to be approved.
    pub fn resume(app: &AppHandle) {
        if secret(app).is_some() {
            forget_pending(app);
            return;
        }
        if let Some(code) = pending(app) {
            *lock(&app.state::<Playit>().linking) = Some(code.clone());
            wait_for_approval(app, code);
        }
    }

    /// Whether playit.gg still knows this computer (asked at most once a minute). A problem to
    /// show the player, or None when it's fine or can't be asked right now.
    pub async fn problem(app: &AppHandle) -> Option<String> {
        let secret = secret(app)?;
        {
            let state = app.state::<Playit>();
            let checked = lock(&state.checked);
            if let Some((at, problem)) = checked.as_ref()
                && at.elapsed() < CHECK_EVERY
            {
                return problem.clone();
            }
        }
        let problem = match playit::check(&secret).await {
            Err(PlayitError::NotLinked) => Some("playit.gg doesn't know this computer any more: it was removed there, or the guest account it was linked with ended. Link it again (signing in to playit keeps it).".to_string()),
            _ => None,
        };
        *lock(&app.state::<Playit>().checked) = Some((Instant::now(), problem.clone()));
        problem
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

    /// Opens playit's claim page and waits (in the background) for the player to approve. The
    /// same link carries on however often the page is opened again, and after Wisp restarts, so
    /// playit.gg doesn't make a second agent for one computer.
    pub fn link(app: &AppHandle) -> Result<String, String> {
        let code = {
            let state = app.state::<Playit>();
            let mut linking = lock(&state.linking);
            if linking.is_none() {
                *linking = Some(pending(app).unwrap_or_else(playit::claim_code));
            }
            linking.clone().unwrap_or_default()
        };
        keep_pending(app, &code);
        let url = playit::claim_url(&code);
        let _ = app.opener().open_url(&url, None::<&str>);
        wait_for_approval(app, code);
        Ok(url)
    }

    /// Waits for the player to approve `code` on playit's page (unless that's being waited for
    /// already), then keeps the agent's key.
    fn wait_for_approval(app: &AppHandle, code: String) {
        use std::sync::atomic::Ordering;
        if app.state::<Playit>().waiting.swap(true, Ordering::SeqCst) {
            return;
        }
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let version = format!("Wisp {}", env!("CARGO_PKG_VERSION"));
            let step = {
                let app = app.clone();
                move |step: ClaimStep| {
                    let step = match step {
                        ClaimStep::Visit => "visit",
                        ClaimStep::Approve => "approve",
                        ClaimStep::Approved => "approved",
                    };
                    let _ = app.emit(PLAYIT_EVENT, Event::Step { step });
                }
            };
            let result = playit::claim(&code, &version, &step).await;
            app.state::<Playit>().waiting.store(false, Ordering::SeqCst);
            let still_ours = lock(&app.state::<Playit>().linking).as_deref() == Some(code.as_str());
            if !still_ours {
                return;
            }
            lock(&app.state::<Playit>().linking).take();
            forget_pending(&app);
            let event = match result {
                Ok(secret) => match save(&app, &secret) {
                    Ok(()) => {
                        lock(&app.state::<Playit>().checked).take();
                        Event::Linked
                    }
                    Err(error) => Event::Failed { message: format!("Couldn't keep playit's key: {error}") },
                },
                Err(error) => Event::Failed { message: error.to_string() },
            };
            let _ = app.emit(PLAYIT_EVENT, event);
        });
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
        lock(&app.state::<Playit>().checked).take();
        forget_pending(app);
        if let Some(agent) = app.state::<Playit>().agent.lock().await.take() {
            agent.stop();
        }
        let _ = std::fs::remove_file(secret_path(&app.state::<Paths>()));
    }

    /// The tunnel to a server's port (made if needed), and its address. `keep_ports` are the
    /// other servers' here, whose tunnels mustn't be reused.
    pub async fn tunnel_for(app: &AppHandle, name: &str, port: u16, keep_ports: &[u16]) -> Result<String, String> {
        let secret = secret(app).ok_or("Link Wisp to playit.gg first.")?;
        let tunnel = playit::tunnel_to(&secret, name, port, keep_ports).await.map_err(|error| error.to_string())?;
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
    {
        app.manage(Playit::default());
        // A link that was waiting to be approved when Wisp last closed carries on.
        built_in::resume(app.handle());
    }
    #[cfg(not(feature = "playit"))]
    let _ = app;
}

#[tauri::command]
pub async fn get_playit(app: AppHandle) -> Result<PlayitView, String> {
    #[cfg(feature = "playit")]
    return Ok(PlayitView {
        built_in: true,
        linked: secret(&app).is_some(),
        linking: built_in::linking_url(&app),
        running: built_in::running(&app),
        problem: built_in::problem(&app).await,
        notice: NOTICE,
    });
    #[cfg(not(feature = "playit"))]
    Ok(PlayitView { built_in: false, linked: secret(&app).is_some(), linking: None, running: false, problem: None, notice: NOTICE })
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

/// The tunnel to a server's port, made if needed (built-in agent only). `keep_ports` are the
/// other servers' here, whose tunnels mustn't be reused.
pub async fn tunnel_for(app: &AppHandle, name: &str, port: u16, keep_ports: &[u16]) -> Result<String, String> {
    #[cfg(feature = "playit")]
    return built_in::tunnel_for(app, name, port, keep_ports).await;
    #[cfg(not(feature = "playit"))]
    {
        let _ = (app, name, port, keep_ports);
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
