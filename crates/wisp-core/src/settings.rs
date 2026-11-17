use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// How much of the startup animation plays.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IntroMode {
    #[default]
    Full,
    Quick,
    Off,
}

/// The page Wisp opens on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StartPage {
    #[default]
    Home,
    Instances,
    Discover,
    /// Wherever Wisp was when it last closed.
    Last,
}

/// What Wisp's window does while a game runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WhilePlaying {
    KeepOpen,
    Minimize,
    /// Close the window while the game runs, leaving the game the memory it used (Wisp keeps
    /// running behind it and opens the window again when the game closes). Shown as "Close it".
    #[default]
    Hide,
}

/// How much the interface animates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Motion {
    /// Follow the operating system's "reduce motion" setting.
    #[default]
    System,
    Reduced,
    Full,
}

/// Which parts of the home page show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HomeSections {
    pub worlds: bool,
    pub quick_actions: bool,
    pub modpacks: bool,
    pub news: bool,
}

impl Default for HomeSections {
    fn default() -> Self {
        Self {
            worlds: true,
            quick_actions: true,
            modpacks: true,
            news: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WindowMode {
    /// Minecraft's own size (854 × 480, or what the game remembers).
    #[default]
    Default,
    Custom,
    Fullscreen,
}

/// The size the game window opens at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GameWindow {
    pub mode: WindowMode,
    pub width: u32,
    pub height: u32,
}

impl Default for GameWindow {
    fn default() -> Self {
        Self {
            mode: WindowMode::Default,
            width: 1280,
            height: 720,
        }
    }
}

/// Commands run around a game, like Prism Launcher's and the Modrinth App's.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LaunchHooks {
    /// Runs before the game starts; the launch stops if it fails.
    pub pre_launch: String,
    /// Starts the game through this program, e.g. `gamemoderun` or `prime-run`.
    pub wrapper: String,
    /// Runs after the game closes.
    pub post_exit: String,
}

/// What a new instance starts with in its options.txt.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NewInstanceOptions {
    /// An instance whose settings and keybinds new instances copy.
    pub copy_options_from: Option<String>,
    /// Skip the accessibility screen and online-play warnings on first launch.
    pub skip_welcome: bool,
    pub skip_tutorial: bool,
    pub auto_jump_off: bool,
}

/// What cloud sync carries between computers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncPrefs {
    pub worlds: bool,
    /// Keybinds and the game settings that make sense on every computer.
    pub options: bool,
    pub servers: bool,
    pub resource_packs: bool,
    pub screenshots: bool,
}

impl Default for SyncPrefs {
    fn default() -> Self {
        Self {
            worlds: true,
            options: true,
            servers: true,
            resource_packs: true,
            screenshots: false,
        }
    }
}

/// The journal: play history, notes and restore points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JournalPrefs {
    /// Take a restore point of each world you played when the game closes.
    pub auto_restore_points: bool,
    /// How many automatic restore points to keep per world (manual ones are always kept).
    pub keep_restore_points: u32,
    /// Offer to leave a note about the world when the game closes.
    pub ask_for_note: bool,
}

impl Default for JournalPrefs {
    fn default() -> Self {
        Self {
            auto_restore_points: true,
            keep_restore_points: 10,
            ask_for_note: true,
        }
    }
}

pub const KEEP_RESTORE_POINTS_RANGE: (u32, u32) = (1, 100);

/// Wisp's own preferences, saved as JSON in the app's config folder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub intro: IntroMode,
    pub sidebar_collapsed: bool,
    pub start_page: StartPage,
    pub while_playing: WhilePlaying,
    pub motion: Motion,
    pub home: HomeSections,
    /// Memory given to the game, in megabytes.
    pub memory_mb: u32,
    pub game_window: GameWindow,
    /// Extra arguments for Java, as typed (split like a shell would).
    pub jvm_args: String,
    /// Java executables to use instead of Wisp's own, by major version ("8", "17", "21", "25").
    pub java_paths: BTreeMap<String, String>,
    pub hooks: LaunchHooks,
    pub new_instances: NewInstanceOptions,
    pub concurrent_downloads: u32,
    /// Show images that project descriptions load from other websites (they see your IP address).
    pub external_images: bool,
    /// A CurseForge API key, for copies of Wisp built without one.
    pub curseforge_api_key: String,
    pub sync: SyncPrefs,
    pub journal: JournalPrefs,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            intro: IntroMode::Full,
            sidebar_collapsed: false,
            start_page: StartPage::Home,
            while_playing: WhilePlaying::Hide,
            motion: Motion::System,
            home: HomeSections::default(),
            memory_mb: 4096,
            game_window: GameWindow::default(),
            jvm_args: String::new(),
            java_paths: BTreeMap::new(),
            hooks: LaunchHooks::default(),
            new_instances: NewInstanceOptions::default(),
            concurrent_downloads: 16,
            external_images: true,
            curseforge_api_key: String::new(),
            sync: SyncPrefs::default(),
            journal: JournalPrefs::default(),
        }
    }
}

pub const MEMORY_MB_RANGE: (u32, u32) = (1024, 64 * 1024);
pub const DOWNLOADS_RANGE: (u32, u32) = (1, 16);
pub const WINDOW_WIDTH_RANGE: (u32, u32) = (320, 7680);
pub const WINDOW_HEIGHT_RANGE: (u32, u32) = (240, 4320);
/// The Java versions Minecraft uses, which a custom Java can be set for.
pub const JAVA_MAJORS: [&str; 4] = ["8", "17", "21", "25"];

impl Settings {
    /// Reads settings, falling back to the defaults if the file is missing or unreadable.
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .map(migrate)
            .and_then(|value| serde_json::from_value::<Settings>(value).ok())
            .unwrap_or_default()
            .sanitized()
    }

    /// Writes atomically (temp file, then rename) so a crash never leaves half a file behind.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let temp = path.with_extension("json.tmp");
        std::fs::write(
            &temp,
            serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?,
        )?;
        std::fs::rename(temp, path)
    }

    /// Clamps numbers into ranges the launcher can actually use and tidies typed text.
    pub fn sanitized(mut self) -> Self {
        self.memory_mb = self.memory_mb.clamp(MEMORY_MB_RANGE.0, MEMORY_MB_RANGE.1);
        self.concurrent_downloads = self
            .concurrent_downloads
            .clamp(DOWNLOADS_RANGE.0, DOWNLOADS_RANGE.1);
        let window = &mut self.game_window;
        window.width = window
            .width
            .clamp(WINDOW_WIDTH_RANGE.0, WINDOW_WIDTH_RANGE.1);
        window.height = window
            .height
            .clamp(WINDOW_HEIGHT_RANGE.0, WINDOW_HEIGHT_RANGE.1);
        self.journal.keep_restore_points = self
            .journal
            .keep_restore_points
            .clamp(KEEP_RESTORE_POINTS_RANGE.0, KEEP_RESTORE_POINTS_RANGE.1);
        self.jvm_args = self.jvm_args.trim().to_string();
        self.curseforge_api_key = self.curseforge_api_key.trim().to_string();
        for hook in [
            &mut self.hooks.pre_launch,
            &mut self.hooks.wrapper,
            &mut self.hooks.post_exit,
        ] {
            *hook = hook.trim().to_string();
        }
        self.java_paths = std::mem::take(&mut self.java_paths)
            .into_iter()
            .map(|(major, path)| (major, path.trim().to_string()))
            .filter(|(major, path)| JAVA_MAJORS.contains(&major.as_str()) && !path.is_empty())
            .collect();
        self
    }

    /// The custom Java set for a Java major version, if any. Java 16 (Minecraft 1.17) runs on the
    /// Java 17 one.
    pub fn java_path(&self, major: u32) -> Option<&str> {
        let key = if major == 16 { 17 } else { major };
        self.java_paths.get(&key.to_string()).map(String::as_str)
    }
}

/// Carries settings saved by older builds forward.
fn migrate(mut value: serde_json::Value) -> serde_json::Value {
    if let Some(fields) = value.as_object_mut()
        && !fields.contains_key("whilePlaying")
        && let Some(close) = fields
            .remove("closeWhilePlaying")
            .and_then(|close| close.as_bool())
    {
        let mode = if close { "hide" } else { "keep-open" };
        fields.insert("whilePlaying".into(), mode.into());
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("wisp-settings-{name}-{}", std::process::id()));
        (dir.join("settings.json"), dir)
    }

    #[test]
    fn saves_and_loads_settings() {
        let (path, dir) = temp_file("roundtrip");
        let mut settings = Settings {
            intro: IntroMode::Quick,
            memory_mb: 6144,
            while_playing: WhilePlaying::Minimize,
            jvm_args: "-XX:+UseZGC".into(),
            ..Settings::default()
        };
        settings
            .java_paths
            .insert("21".into(), "/opt/java21/bin/java".into());
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn falls_back_to_defaults_and_fills_missing_fields() {
        assert_eq!(
            Settings::load(Path::new("/definitely/not/here.json")),
            Settings::default()
        );
        let partial: Settings = serde_json::from_str(r#"{ "intro": "off" }"#).unwrap();
        assert_eq!(partial.intro, IntroMode::Off);
        assert_eq!(partial.memory_mb, 4096);
        assert!(partial.home.news);
    }

    #[test]
    fn carries_the_old_hide_while_playing_switch_forward() {
        let (path, dir) = temp_file("migrate");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, r#"{ "closeWhilePlaying": false, "memoryMb": 2048 }"#).unwrap();
        let loaded = Settings::load(&path);
        assert_eq!(loaded.while_playing, WhilePlaying::KeepOpen);
        assert_eq!(loaded.memory_mb, 2048);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn clamps_and_tidies() {
        let mut wild = Settings {
            memory_mb: 1,
            concurrent_downloads: 500,
            jvm_args: "  -Xss2M  ".into(),
            ..Settings::default()
        };
        wild.game_window.width = 5;
        wild.java_paths.insert("17".into(), "  ".into());
        wild.java_paths.insert("11".into(), "/java".into());
        let tidy = wild.sanitized();
        assert_eq!(tidy.memory_mb, MEMORY_MB_RANGE.0);
        assert_eq!(tidy.concurrent_downloads, DOWNLOADS_RANGE.1);
        assert_eq!(tidy.game_window.width, WINDOW_WIDTH_RANGE.0);
        assert_eq!(tidy.jvm_args, "-Xss2M");
        assert!(tidy.java_paths.is_empty());
    }

    #[test]
    fn java_16_uses_the_java_17_path() {
        let mut settings = Settings::default();
        settings.java_paths.insert("17".into(), "/j17".into());
        assert_eq!(settings.java_path(16), Some("/j17"));
        assert_eq!(settings.java_path(21), None);
    }
}
