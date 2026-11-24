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

pub const MAIN: &str = "main";
const TRAY: &str = "wisp-away";

/// Where the window was when it closed for a game (nothing while it's open), whether the window
/// has just been closed for one, whether it's only hidden (while a server Wisp hosts runs), and
/// whether the tray icon is up.
#[derive(Default)]
pub struct Away {
    placement: Mutex<Option<Placement>>,
    closing: AtomicBool,
    hidden: AtomicBool,
    tray: AtomicBool,
}

#[derive(Clone, Copy)]
struct Placement {
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    maximized: bool,
}

impl Away {
    /// Whether the window is closed (or hidden) for a game right now.
    pub fn is_away(&self) -> bool {
        self.hidden.load(Ordering::SeqCst) || self.placement.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).is_some()
    }

    /// Whether the app being asked to quit is only the window closing for a game (the request that
    /// follows closing the last window): that one is turned down, once. Quitting on purpose
    /// (Cmd+Q, or Quit in the Dock or tray) still quits.
    pub fn closing_for_game(&self) -> bool {
        self.closing.swap(false, Ordering::SeqCst)
    }

    fn set(&self, placement: Option<Placement>) -> Option<Placement> {
        std::mem::replace(&mut *self.placement.lock().unwrap_or_else(|poisoned| poisoned.into_inner()), placement)
    }
}

/// Closes the window for a game, remembering where it was, and puts up the tray icon.
pub fn close_for_game(app: &AppHandle) {
    close_to_tray(app, "Wisp is waiting for your game");
}

/// Hides the window, rather than closing it, while a server Wisp hosts runs: on some systems the
/// last window closing ends Wisp, and the server would go with it. The tray brings the window
/// back, and quitting from there stops the servers carefully first.
pub fn hide_for_hosting(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN) else {
        return;
    };
    app.state::<Away>().hidden.store(true, Ordering::SeqCst);
    show_tray(app, "Wisp is hosting a server");
    let _ = window.hide();
}

fn close_to_tray(app: &AppHandle, tooltip: &'static str) {
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
    show_tray(app, tooltip);
    away.closing.store(true, Ordering::SeqCst);
    let _ = window.destroy();
}

/// Opens the window again, as it was, if it closed for a game.
pub fn come_back(app: &AppHandle) {
    let away = app.state::<Away>();
    away.closing.store(false, Ordering::SeqCst);
    let hidden = away.hidden.swap(false, Ordering::SeqCst);
    let placement = away.set(None);
    if placement.is_none() && !hidden && !away.tray.load(Ordering::SeqCst) {
        return;
    }
    hide_tray(app);
    if let Some(window) = app.get_webview_window(MAIN) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    // Built on a thread of its own: this can be called on the main thread (the tray, the Dock), and
    // building a window there can hang on Windows.
    let app = app.clone();
    std::thread::spawn(move || reopen(&app, placement));
}

fn reopen(app: &AppHandle, placement: Option<Placement>) {
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
    if let Some(placement) = placement {
        let _ = window.set_position(placement.position);
        let _ = window.set_size(placement.size);
        if placement.maximized {
            let _ = window.maximize();
        }
    } else {
        let _ = window.center();
    }
    let _ = window.show();
    let _ = window.set_focus();
}

/// Puts up the tray icon, or changes what it says. Everything to do with the tray happens on the
/// main thread: macOS ends an app that touches the menu bar from any other (quitting a game used to
/// take Wisp down with it, as the tray came down from the thread that watched the game).
fn show_tray(app: &AppHandle, tooltip: &'static str) {
    app.state::<Away>().tray.store(true, Ordering::SeqCst);
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let app = &handle;
        if let Some(tray) = app.tray_by_id(TRAY) {
            let _ = tray.set_tooltip(Some(tooltip));
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
            .tooltip(tooltip)
            .menu(&menu)
            .on_menu_event(|app, event| match event.id.as_ref() {
                "open" => come_back(app),
                // The game keeps running on its own; servers Wisp hosts stop first (see main).
                "quit" => app.exit(0),
                _ => {}
            });
        if let Some(icon) = app.default_window_icon() {
            tray = tray.icon(icon.clone());
        }
        let _ = tray.build(app);
    });
}

/// Takes the tray icon down, on the main thread (see `show_tray`).
fn hide_tray(app: &AppHandle) {
    if !app.state::<Away>().tray.swap(false, Ordering::SeqCst) {
        return;
    }
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || drop(handle.remove_tray_by_id(TRAY)));
}
