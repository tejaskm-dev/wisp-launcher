//! Minecraft servers hosted on this computer. Each lives in a folder of its own under
//! `<data>/servers/<id>/`, laid out the way a server expects, with Wisp's record of it in
//! `wisp-server.json` beside the game's files. Here: that record, setting up the official server
//! software for its loader (`install`), its settings (`properties`), who may join (`players`),
//! bringing in a singleplayer world (`world`), and the running server with its console
//! (`process`).
//!
//! Nothing here runs anything anywhere but this computer: friends reach the server on the same
//! network, through a direct link between the two Wisps (see `tunnel`), or at an address the
//! player's router opens.

pub mod install;
pub mod pack;
pub mod players;
pub mod process;
pub mod properties;
pub mod world;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::instances::Loader;

const RECORD: &str = "wisp-server.json";
/// Minecraft's own port; a second server on the computer takes the next free one.
pub const DEFAULT_PORT: u16 = 25565;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostedServer {
    pub id: String,
    pub name: String,
    pub game_version: String,
    pub loader: Loader,
    pub loader_version: Option<String>,
    /// Unix milliseconds.
    pub created: i64,
    #[serde(default)]
    pub last_started: Option<i64>,
    /// Memory in MB; none picks it from how many mods there are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u32>,
    /// The instance people play it with: friends who join get its mods.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    /// The player agreed to Minecraft's EULA for this server (Wisp writes eula.txt only then).
    #[serde(default)]
    pub eula: bool,
    /// Friends can join while it runs without being invited.
    #[serde(default = "yes")]
    pub friends_join: bool,
    /// Friends invited to it (friend codes): they can join even when `friends_join` is off.
    #[serde(default)]
    pub invited: Vec<String>,
    /// Ask the router to open the server's port (UPnP), so anyone with the address can join.
    #[serde(default)]
    pub open_port: bool,
    /// Keep a restore point of the world whenever the server stops.
    #[serde(default = "yes")]
    pub backups: bool,
    /// "wisp:<n>" for one of Wisp's icons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

fn yes() -> bool {
    true
}

/// What the "New server" dialog sends.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewServer {
    pub name: String,
    pub game_version: String,
    pub loader: Loader,
    pub loader_version: Option<String>,
    #[serde(default)]
    pub instance_id: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    /// server.properties settings to start with (difficulty, gamemode, level-seed…).
    #[serde(default)]
    pub properties: Vec<(String, String)>,
}

#[derive(Debug, thiserror::Error)]
pub enum HostingError {
    #[error("Give the server a name.")]
    EmptyName,
    #[error("That server no longer exists.")]
    NotFound,
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Servers live in `<root>/<id>/`.
pub struct ServerStore {
    root: PathBuf,
}

impl ServerStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// A server's folder, which is where it runs.
    pub fn dir_of(&self, id: &str) -> Option<PathBuf> {
        let valid = !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        valid.then(|| self.root.join(id)).filter(|dir| dir.join(RECORD).is_file())
    }

    pub fn get(&self, id: &str) -> Option<HostedServer> {
        let bytes = std::fs::read(self.dir_of(id)?.join(RECORD)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Every readable server, most recently started first.
    pub fn list(&self) -> Vec<HostedServer> {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut servers: Vec<HostedServer> = entries
            .flatten()
            .filter_map(|entry| std::fs::read(entry.path().join(RECORD)).ok())
            .filter_map(|bytes| serde_json::from_slice(&bytes).ok())
            .collect();
        servers.sort_by_key(|server| std::cmp::Reverse(server.last_started.unwrap_or(server.created)));
        servers
    }

    /// Makes the server's folder, its record and its first settings. The server software is set
    /// up the first time it starts.
    pub fn create(&self, request: NewServer, now: i64) -> Result<HostedServer, HostingError> {
        let name: String = request.name.trim().chars().filter(|c| !c.is_control()).take(48).collect();
        if name.is_empty() {
            return Err(HostingError::EmptyName);
        }
        let id = self.unique_id(&name);
        let dir = self.root.join(&id);
        std::fs::create_dir_all(&dir)?;
        let server = HostedServer {
            id,
            name: name.clone(),
            game_version: request.game_version,
            loader: request.loader,
            loader_version: request.loader_version.filter(|_| request.loader != Loader::Vanilla),
            created: now,
            last_started: None,
            memory_mb: None,
            instance_id: request.instance_id,
            eula: false,
            friends_join: true,
            invited: Vec::new(),
            open_port: false,
            backups: true,
            icon: request.icon,
        };
        let mut properties = properties::Properties::read(&dir);
        let taken: Vec<u16> = self.list().iter().filter_map(|other| self.port_of(&other.id)).collect();
        let defaults = [
            ("server-port", free_port(&taken).to_string()),
            ("motd", name),
            ("level-name", world::WORLD.to_string()),
            ("max-players", "10".to_string()),
            // Friends' servers: only the people let in, and building at spawn allowed.
            ("white-list", "true".to_string()),
            ("enforce-whitelist", "true".to_string()),
            ("spawn-protection", "0".to_string()),
        ];
        for (key, value) in defaults {
            properties.set(key, &value);
        }
        for (key, value) in &request.properties {
            if is_property_key(key) {
                properties.set(key, value);
            }
        }
        properties.write(&dir)?;
        self.save(&server)?;
        Ok(server)
    }

    fn save(&self, server: &HostedServer) -> Result<(), HostingError> {
        let dir = self.root.join(&server.id);
        let temp = dir.join(format!("{RECORD}.tmp"));
        std::fs::write(&temp, serde_json::to_vec_pretty(server).map_err(std::io::Error::other)?)?;
        std::fs::rename(temp, dir.join(RECORD))?;
        Ok(())
    }

    /// Changes a server's record and saves it.
    pub fn update(&self, id: &str, change: impl FnOnce(&mut HostedServer)) -> Result<HostedServer, HostingError> {
        let mut server = self.get(id).ok_or(HostingError::NotFound)?;
        change(&mut server);
        server.id = id.to_string();
        server.name = server.name.trim().chars().filter(|c| !c.is_control()).take(48).collect();
        if server.name.is_empty() {
            return Err(HostingError::EmptyName);
        }
        self.save(&server)?;
        Ok(server)
    }

    /// The port the server listens on.
    pub fn port_of(&self, id: &str) -> Option<u16> {
        let dir = self.dir_of(id)?;
        Some(properties::Properties::read(&dir).get("server-port").and_then(|port| port.trim().parse().ok()).unwrap_or(DEFAULT_PORT))
    }

    fn unique_id(&self, name: &str) -> String {
        let slug: String = name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        let base = if slug.is_empty() { "server".to_string() } else { slug };
        std::iter::once(base.clone())
            .chain((2..).map(|n| format!("{base}-{n}")))
            .find(|id| !self.root.join(id).exists())
            .expect("an unused id always exists")
    }
}

/// The first port from 25565 that no other server here uses and nothing else is listening on.
fn free_port(taken: &[u16]) -> u16 {
    (DEFAULT_PORT..DEFAULT_PORT + 100)
        .find(|port| !taken.contains(port) && std::net::TcpListener::bind(("0.0.0.0", *port)).is_ok())
        .unwrap_or(DEFAULT_PORT)
}

/// Keys look like Minecraft's own: lowercase words joined by dashes or dots.
pub fn is_property_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 64 && key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.' || c == '_')
}

/// Memory a server gets when it has none of its own: more for more mods, and never more than a
/// third of the computer's memory, since the host is usually playing on it too.
pub fn automatic_memory_mb(mod_count: usize, system_mb: Option<u64>) -> u32 {
    let wanted: u32 = match mod_count {
        0 => 2048,
        1..60 => 4096,
        60..150 => 6144,
        _ => 8192,
    };
    match system_mb {
        Some(total) => wanted.min(u32::try_from(total / 3).unwrap_or(u32::MAX).max(1024)),
        None => wanted,
    }
}

/// How many mods a server folder holds (turned-on jars in mods/).
pub fn mod_count(dir: &Path) -> usize {
    std::fs::read_dir(dir.join("mods"))
        .map(|entries| entries.flatten().filter(|entry| entry.file_name().to_string_lossy().ends_with(".jar")).count())
        .unwrap_or(0)
}

/// Agrees to Minecraft's EULA in the server's folder. Only call this once the player has.
pub fn accept_eula(dir: &Path) -> std::io::Result<()> {
    std::fs::write(
        dir.join("eula.txt"),
        "#By changing the setting below to TRUE you are indicating your agreement to our EULA (https://aka.ms/MinecraftEULA).\n#Agreed to in Wisp by the person hosting this server.\neula=true\n",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_lists_and_updates_servers() {
        let root = std::env::temp_dir().join(format!("wisp-hosting-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store = ServerStore::new(&root);
        assert!(matches!(
            store.create(NewServer { name: "  ".into(), game_version: "26.3".into(), loader: Loader::Vanilla, loader_version: None, instance_id: None, icon: None, properties: vec![] }, 1),
            Err(HostingError::EmptyName)
        ));
        let request = |name: &str| NewServer {
            name: name.into(),
            game_version: "26.3".into(),
            loader: Loader::Fabric,
            loader_version: Some("0.20.1".into()),
            instance_id: Some("fabric".into()),
            icon: None,
            properties: vec![("difficulty".into(), "hard".into()), ("Bad Key".into(), "x".into())],
        };
        let first = store.create(request("Build Night!"), 1).unwrap();
        assert_eq!(first.id, "build-night");
        assert!(first.friends_join && first.backups && !first.eula);
        let settings = properties::Properties::read(&store.dir_of(&first.id).unwrap());
        assert_eq!(settings.get("difficulty"), Some("hard"));
        assert_eq!(settings.get("white-list"), Some("true"));
        assert_eq!(settings.get("motd"), Some("Build Night!"));
        assert_eq!(settings.get("Bad Key"), None);
        let second = store.create(request("Build Night!"), 2).unwrap();
        assert_eq!(second.id, "build-night-2");
        assert_ne!(store.port_of(&first.id), store.port_of(&second.id), "two servers never share a port");
        store.update(&first.id, |server| server.last_started = Some(9)).unwrap();
        assert_eq!(store.list().iter().map(|server| server.id.as_str()).collect::<Vec<_>>(), ["build-night", "build-night-2"]);
        assert!(store.get("../build-night").is_none());
        // Vanilla servers have no loader version.
        let vanilla = store.create(NewServer { loader: Loader::Vanilla, ..request("Plain") }, 3).unwrap();
        assert_eq!(vanilla.loader_version, None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn memory_grows_with_mods_but_leaves_room() {
        assert_eq!(automatic_memory_mb(0, None), 2048);
        assert_eq!(automatic_memory_mb(80, Some(32 * 1024)), 6144);
        assert_eq!(automatic_memory_mb(300, Some(16 * 1024)), 5461);
        assert_eq!(automatic_memory_mb(300, Some(2 * 1024)), 1024);
    }
}
