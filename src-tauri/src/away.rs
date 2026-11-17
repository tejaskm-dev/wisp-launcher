//! Wisp's window while a game runs, with "Close it" picked (Settings → General → When a game
//! starts). The window closes once the game is up, which hands the game the memory its web view
//! was using, and opens again where you were when the game quits. Wisp itself keeps running
//! meanwhile (it watches the game, writes the journal and backs up the worlds), with a tray icon
//! to bring the window back sooner, and on macOS the Dock icon does too.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindowBuilder};

const MAIN: &str = "main";
const TRAY: &str = "wisp-away";

/// Where the window was when it closed for a game (nothing while it's open), and whether the
/// window has just been closed for one.
#[derive(Default)]
pub struct Away(Mutex<Option<Placement>>, AtomicBool);

#[derive(Clone, Copy)]
struct Placement {
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    maximized: bool,
}

impl Away {
    /// Whether the window is closed for a game right now.
    pub fn is_away(&self) -> bool {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).is_some()
    }

    /// Whether the app being asked to quit is only the window closing for a game (the request that
    /// follows closing the last window): that one is turned down, once. Quitting on purpose
    /// (Cmd+Q, or Quit in the Dock or tray) still quits.
    pub fn closing_for_game(&self) -> bool {
        self.1.swap(false, Ordering::SeqCst)
    }

    fn set(&self, placement: Option<Placement>) -> Option<Placement> {
        std::mem::replace(&mut *self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()), placement)
    }
}

/// Closes the window for a game, remembering where it was, and puts up the tray icon.
pub fn close_for_game(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN) else {
        return;
    };
    let placement = Placement {
        position: window.outer_position().unwrap_or_default(),
        size: window.inner_size().unwrap_or_default(),
        maximized: window.is_maximized().unwrap_or(false),
    };
    let away = app.state::<Away>();
    away.set(Some(placement));
    show_tray(app);
    away.1.store(true, Ordering::SeqCst);
    let _ = window.destroy();
}

/// Opens the window again, as it was, if it closed for a game.
pub fn come_back(app: &AppHandle) {
    let away = app.state::<Away>();
    away.1.store(false, Ordering::SeqCst);
    let Some(placement) = away.set(None) else {
        return;
    };
    let _ = app.remove_tray_by_id(TRAY);
    if let Some(window) = app.get_webview_window(MAIN) {
        let _ = window.set_focus();
        return;
    }
    let Some(mut config) = app.config().app.windows.iter().find(|window| window.label == MAIN).cloned() else {
        return;
    };
    // Built hidden and put back where it was before it shows, so it doesn't jump.
    config.visible = false;
    config.center = false;
    let built = WebviewWindowBuilder::from_config(app, &config).and_then(|builder| {
        // Read before the page's own scripts run: no intro, and back to the page you were on.
        builder.initialization_script("window.__WISP_FROM_GAME__ = true;").build()
    });
    let Ok(window) = built else {
        return;
    };
    let _ = window.set_position(placement.position);
    let _ = window.set_size(placement.size);
    if placement.maximized {
        let _ = window.maximize();
    }
    let _ = window.show();
    let _ = window.set_focus();
}

fn show_tray(app: &AppHandle) {
    if app.tray_by_id(TRAY).is_some() {
        return;
    }
    let menu = MenuItem::with_id(app, "open", "Open Wisp", true, None::<&str>).and_then(|open| {
        let quit = MenuItem::with_id(app, "quit", "Quit Wisp", true, None::<&str>)?;
        Menu::with_items(app, &[&open, &quit])
    });
    let Ok(menu) = menu else {
        return;
    };
    let mut tray = TrayIconBuilder::with_id(TRAY)
        .tooltip("Wisp is waiting for your game")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => come_back(app),
            // The game keeps running on its own.
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    let _ = tray.build(app);
}
