use std::path::Path;

use serde::Serialize;

use crate::accounts::{Account, AccountStore};
use crate::device::DeviceInfo;
use crate::instances::{GAME_DIR, Instance, InstanceStore, Loader};
use crate::journal::{Event, Journal, JournalQuery};
use crate::servers;
use crate::worlds::{self, GameMode, WorldInfo};

/// Everything the home screen and sidebar show about this computer's library.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeSnapshot {
    /// The account Play uses, if there is one.
    pub account: Option<Account>,
    pub device: DeviceInfo,
    /// Most recently used first.
    pub instances: Vec<Instance>,
    /// Every world in every instance, most recently played first.
    pub worlds: Vec<WorldSummary>,
    /// Every server in every instance's server list, most recently played first.
    pub servers: Vec<ServerSummary>,
    pub sync: SyncStatus,
}

/// A server from the instances' server lists. One saved in several instances shows up once.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSummary {
    /// The address as saved.
    pub address: String,
    pub name: String,
    /// Its icon as a PNG data URL, as the game last saw it.
    pub icon: Option<String>,
    /// Instances whose list has it; the first is the one Play uses.
    pub instance_ids: Vec<String>,
    /// When a session last joined it (unix ms), if the journal knows.
    pub last_played: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldSummary {
    /// "<instance id>/<folder>".
    pub id: String,
    pub instance_id: String,
    pub instance_name: String,
    /// The world's folder in the instance's saves/.
    pub folder: String,
    pub name: String,
    /// The world's icon.png, relative to Wisp's data folder.
    pub icon: Option<String>,
    /// The latest screenshot taken in the world, relative to Wisp's data folder.
    pub cover: Option<String>,
    /// e.g. "Fabric 26.3" or "Vanilla 26.3".
    pub loader: String,
    pub mod_count: u32,
    /// Unix time in milliseconds.
    pub last_played: i64,
    /// Name of the computer the world was last played on.
    pub last_device: Option<String>,
    pub last_device_is_this: bool,
    /// The newest note the player left about this world.
    pub note: Option<String>,
    pub note_at: Option<i64>,
    pub sync: WorldSync,
    pub game_mode: GameMode,
    pub hardcore: bool,
    pub cheats: bool,
    pub difficulty: Option<String>,
    /// The Minecraft version that last saved it.
    pub version: Option<String>,
    /// Time spent in the world, in seconds.
    pub play_seconds: Option<u64>,
}

/// Where a world's copy on this computer stands against the cloud.
#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum WorldSync {
    /// Only on this computer: cloud sync isn't set up.
    Local,
    Synced { at: i64 },
    Uploading { progress: f32, total_bytes: u64 },
    CloudNewer,
    Conflict,
}

/// Whether cloud sync is set up on this computer.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum SyncStatus {
    /// No storage provider connected yet.
    Off,
}

/// "Fabric 26.3", "Vanilla 1.20.1".
pub fn instance_label(instance: &Instance) -> String {
    let loader = match instance.loader {
        Loader::Vanilla => "Vanilla",
        loader => loader.display_name(),
    };
    format!("{loader} {}", instance.game_version)
}

/// The mods an instance has switched on (jars in its mods folder).
fn mod_count(game_dir: &Path) -> u32 {
    std::fs::read_dir(game_dir.join("mods"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|extension| extension == "jar"))
        .count() as u32
}

impl HomeSnapshot {
    /// Reads the local library: the active account, this computer, its instances and every
    /// world in them, with the notes and screenshots the journal has for each.
    pub fn load(instances: &InstanceStore, accounts: &AccountStore, journal: &Journal) -> Self {
        let device = DeviceInfo::current();
        let list = instances.list();
        let sessions = journal.list(&JournalQuery { kinds: vec!["session".into()], ..Default::default() });
        let notes = journal.list(&JournalQuery { kinds: vec!["note".into()], ..Default::default() });
        let mut worlds = Vec::new();
        for instance in &list {
            let Some(game_dir) = instances.game_dir(&instance.id) else {
                continue;
            };
            let mods = mod_count(&game_dir);
            let label = instance_label(instance);
            for info in worlds::list_info(&game_dir) {
                worlds.push(summary(instance, &game_dir, info, &label, mods, &device, &sessions, &notes));
            }
        }
        worlds.sort_by_key(|world| std::cmp::Reverse(world.last_played));
        let servers = server_summaries(instances, &list, &sessions);
        Self {
            account: accounts.active(),
            device,
            instances: list,
            worlds,
            servers,
            sync: SyncStatus::Off,
        }
    }
}

/// Servers across instances (most recently used instance first), joined up by address, with when
/// each was last played and in which instance.
fn server_summaries(store: &InstanceStore, instances: &[Instance], sessions: &[crate::journal::Entry]) -> Vec<ServerSummary> {
    // When each server was last joined, and from which instance.
    let mut played: std::collections::HashMap<String, (i64, String)> = std::collections::HashMap::new();
    for entry in sessions {
        let Event::Session { servers: joined, ended_at, .. } = &entry.event else {
            continue;
        };
        for address in joined {
            if let Some(key) = servers::address_key(address) {
                let instance = entry.instance_id.clone().unwrap_or_default();
                let slot = played.entry(key).or_insert((0, String::new()));
                if *ended_at > slot.0 {
                    *slot = (*ended_at, instance);
                }
            }
        }
    }
    let mut found: Vec<(String, ServerSummary)> = Vec::new();
    for instance in instances {
        let Some(game_dir) = store.game_dir(&instance.id) else {
            continue;
        };
        for saved in servers::list(&game_dir) {
            let Some(key) = servers::address_key(&saved.address) else {
                continue;
            };
            match found.iter_mut().find(|(known, _)| *known == key) {
                Some((_, summary)) => {
                    if !summary.instance_ids.contains(&instance.id) {
                        summary.instance_ids.push(instance.id.clone());
                    }
                    if summary.icon.is_none() {
                        summary.icon = saved.icon;
                    }
                }
                None => found.push((
                    key,
                    ServerSummary { address: saved.address, name: saved.name, icon: saved.icon, instance_ids: vec![instance.id.clone()], last_played: None },
                )),
            }
        }
    }
    let mut summaries: Vec<ServerSummary> = found
        .into_iter()
        .map(|(key, mut summary)| {
            if let Some((at, instance)) = played.get(&key) {
                summary.last_played = Some(*at);
                // Play it with the instance it was last played in, when that one still has it.
                if let Some(position) = summary.instance_ids.iter().position(|id| id == instance) {
                    let id = summary.instance_ids.remove(position);
                    summary.instance_ids.insert(0, id);
                }
            }
            summary
        })
        .collect();
    summaries.sort_by_key(|summary| std::cmp::Reverse(summary.last_played.unwrap_or(0)));
    summaries
}

#[allow(clippy::too_many_arguments)]
fn summary(
    instance: &Instance,
    game_dir: &Path,
    info: WorldInfo,
    label: &str,
    mods: u32,
    device: &DeviceInfo,
    sessions: &[crate::journal::Entry],
    notes: &[crate::journal::Entry],
) -> WorldSummary {
    let base = format!("instances/{}/{GAME_DIR}", instance.id);
    // Newest first already: the first session in this world with a screenshot still on disk.
    let cover = sessions
        .iter()
        .filter(|entry| entry.instance_id.as_deref() == Some(instance.id.as_str()))
        .find_map(|entry| match &entry.event {
            Event::Session { worlds, screenshots, .. } if worlds.contains(&info.folder) => screenshots
                .iter()
                .rev()
                .find(|file| crate::screenshots::file_path(game_dir, file).is_some())
                .map(|file| format!("{base}/{}/{file}", crate::screenshots::SCREENSHOTS)),
            _ => None,
        });
    let note = notes.iter().find(|entry| {
        entry.instance_id.as_deref() == Some(instance.id.as_str()) && entry.world.as_deref() == Some(info.folder.as_str())
    });
    WorldSummary {
        id: format!("{}/{}", instance.id, info.folder),
        instance_id: instance.id.clone(),
        instance_name: instance.name.clone(),
        icon: info.has_icon.then(|| format!("{base}/{}/{}/icon.png", worlds::SAVES, info.folder)),
        cover,
        loader: label.to_string(),
        mod_count: mods,
        last_played: info.last_played.unwrap_or(0),
        last_device: Some(device.name.clone()),
        last_device_is_this: true,
        note: note.and_then(|entry| match &entry.event {
            Event::Note { text, .. } => Some(text.clone()),
            _ => None,
        }),
        note_at: note.map(|entry| entry.at),
        sync: WorldSync::Local,
        game_mode: info.game_mode,
        hardcore: info.hardcore,
        cheats: info.cheats,
        difficulty: info.difficulty,
        version: info.version,
        play_seconds: info.play_seconds,
        name: info.name,
        folder: info.folder,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The UI reads these exact names (src/data/types.ts), so a rename here must happen there too.
    #[test]
    fn serializes_with_the_field_names_the_ui_reads() {
        let home = serde_json::to_value(HomeSnapshot::load(
            &InstanceStore::new("/definitely/not/here"),
            &AccountStore::new("/definitely/not/here/accounts.json"),
            &Journal::new("/definitely/not/here/journal"),
        ))
        .unwrap();
        for key in ["account", "device", "instances", "worlds", "sync"] {
            assert!(home.get(key).is_some(), "HomeSnapshot is missing `{key}`");
        }
        assert_eq!(home["sync"]["state"], "off");
        assert!(!home["device"]["name"].as_str().unwrap().is_empty());
        assert_eq!(serde_json::to_value(WorldSync::Local).unwrap()["kind"], "local");
        assert_eq!(
            serde_json::to_value(WorldSync::Uploading { progress: 0.5, total_bytes: 10 }).unwrap()["totalBytes"],
            10
        );
        assert_eq!(serde_json::to_value(WorldSync::CloudNewer).unwrap()["kind"], "cloud-newer");
    }

    #[test]
    fn lists_worlds_with_their_icon_note_and_cover() {
        use crate::nbt::tests::Builder;
        let root = std::env::temp_dir().join(format!("wisp-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let instances = InstanceStore::new(root.join("instances"));
        let instance = instances
            .create(
                crate::instances::NewInstance {
                    name: "Fabric 26.3".into(),
                    game_version: "26.3".into(),
                    loader: Loader::Fabric,
                    loader_version: Some("0.18.0".into()),
                    icon: None,
                },
                Some("0.18.0".into()),
                1,
            )
            .unwrap();
        let game = instances.game_dir(&instance.id).unwrap();
        let world = game.join("saves/Island");
        std::fs::create_dir_all(&world).unwrap();
        std::fs::write(world.join("level.dat"), Builder::root().open("Data").string("LevelName", "Island").long("LastPlayed", 500).gzipped()).unwrap();
        std::fs::write(world.join("icon.png"), b"png").unwrap();
        std::fs::create_dir_all(game.join("screenshots")).unwrap();
        std::fs::write(game.join("screenshots/a.png"), b"png").unwrap();
        std::fs::create_dir_all(game.join("mods")).unwrap();
        std::fs::write(game.join("mods/sodium.jar"), b"jar").unwrap();
        std::fs::write(game.join("mods/old.jar.disabled"), b"jar").unwrap();
        crate::servers::add(&game, "Example", "play.example.net").unwrap();

        let journal = Journal::new(root.join("journal"));
        let session = Event::Session {
            started_at: 1,
            ended_at: 2,
            crashed: false,
            worlds: vec!["Island".into()],
            new_worlds: vec![],
            screenshots: vec!["a.png".into(), "deleted.png".into()],
            servers: vec!["play.example.net".into()],
            device: "Mac".into(),
            label: "Fabric 26.3".into(),
        };
        journal.append(Some(&instance.id), None, 2, session).unwrap();
        journal.append(Some(&instance.id), Some("Island"), 3, Event::Note { text: "Find the village".into(), session: None }).unwrap();

        let home = HomeSnapshot::load(&instances, &AccountStore::new(root.join("accounts.json")), &journal);
        assert_eq!(home.servers.len(), 1);
        assert_eq!((home.servers[0].name.as_str(), home.servers[0].last_played), ("Example", Some(2)));
        assert_eq!(home.servers[0].instance_ids, vec![instance.id.clone()]);
        let world = &home.worlds[0];
        assert_eq!(world.id, format!("{}/Island", instance.id));
        assert_eq!((world.name.as_str(), world.loader.as_str(), world.mod_count), ("Island", "Fabric 26.3", 1));
        assert_eq!(world.icon.as_deref(), Some(format!("instances/{}/minecraft/saves/Island/icon.png", instance.id).as_str()));
        assert_eq!(world.cover.as_deref(), Some(format!("instances/{}/minecraft/screenshots/a.png", instance.id).as_str()));
        assert_eq!((world.note.as_deref(), world.note_at), (Some("Find the village"), Some(3)));
        assert_eq!(world.last_played, 500);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn instances_serialize_with_the_field_names_the_ui_reads() {
        let instance = serde_json::to_value(crate::instances::Instance {
            id: "a".into(),
            name: "A".into(),
            game_version: "26.3".into(),
            loader: crate::instances::Loader::Fabric,
            loader_version: Some("0.20.1".into()),
            created: 1,
            last_played: None,
            icon: None,
            modpack: None,
            memory_mb: None,
            java_major: None,
        })
        .unwrap();
        for key in [
            "gameVersion",
            "loader",
            "loaderVersion",
            "created",
            "lastPlayed",
        ] {
            assert!(instance.get(key).is_some(), "Instance is missing `{key}`");
        }
        assert_eq!(instance["loader"], "fabric");
    }
}
