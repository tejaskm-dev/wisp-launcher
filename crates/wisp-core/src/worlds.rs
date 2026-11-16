//! The worlds in an instance's saves folder: what they are (read from level.dat), adding worlds
//! downloaded as zips, and exporting them as zips.

use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::nbt::{self, Tag};

pub const SAVES: &str = "saves";
/// Minecraft holds this open while a world is loaded; it's never worth copying.
pub const SESSION_LOCK: &str = "session.lock";

/// How a world is played.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GameMode {
    Survival,
    Creative,
    Adventure,
    Spectator,
    Unknown,
}

/// What the launcher shows about a world, read from its level.dat and the player's stats.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldInfo {
    /// The world's folder in saves/, which identifies it within its instance.
    pub folder: String,
    /// The name Minecraft shows (level.dat's LevelName), or the folder's.
    pub name: String,
    /// Unix milliseconds: level.dat's LastPlayed, else when level.dat was last written.
    pub last_played: Option<i64>,
    pub game_mode: GameMode,
    pub hardcore: bool,
    /// "Allow cheats" (commands) is on.
    pub cheats: bool,
    /// "peaceful", "easy", "normal" or "hard".
    pub difficulty: Option<String>,
    /// The Minecraft version that last saved it, e.g. "26.3".
    pub version: Option<String>,
    /// The world has an icon.png (Minecraft saves one the first time you quit it).
    pub has_icon: bool,
    /// Time spent in the world in seconds, from the player stats.
    pub play_seconds: Option<u64>,
    /// When the world's folder was made, where the system records it (Unix milliseconds).
    pub created: Option<i64>,
}

fn millis(time: std::io::Result<std::time::SystemTime>) -> Option<i64> {
    time.ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_millis() as i64)
}

/// Reads a world folder. None when it isn't a world (no level.dat). A level.dat that can't be
/// read still lists the world, by its folder name.
pub fn read_info(world_dir: &Path) -> Option<WorldInfo> {
    let level = world_dir.join("level.dat");
    let metadata = std::fs::metadata(&level).ok()?;
    let folder = world_dir.file_name()?.to_string_lossy().into_owned();
    let root = nbt::read_file(&level).ok();
    let data = root.as_ref().and_then(|root| root.get("Data"));
    let field = |path: &[&str]| data.and_then(|data| data.at(path));

    let game_mode = match field(&["GameType"]).and_then(Tag::as_i64) {
        Some(0) => GameMode::Survival,
        Some(1) => GameMode::Creative,
        Some(2) => GameMode::Adventure,
        Some(3) => GameMode::Spectator,
        _ => GameMode::Unknown,
    };
    // 26.x keeps difficulty as a word under difficulty_settings; older versions use numbers.
    let difficulty = field(&["difficulty_settings", "difficulty"])
        .and_then(Tag::as_str)
        .map(str::to_string)
        .or_else(|| {
            field(&["Difficulty"]).and_then(Tag::as_i64).and_then(|level| {
                ["peaceful", "easy", "normal", "hard"]
                    .get(level as usize)
                    .map(|name| name.to_string())
            })
        });
    let hardcore = field(&["difficulty_settings", "hardcore"])
        .or_else(|| field(&["hardcore"]))
        .and_then(Tag::as_bool)
        .unwrap_or(false);
    Some(WorldInfo {
        name: field(&["LevelName"])
            .and_then(Tag::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map_or_else(|| folder.clone(), str::to_string),
        last_played: field(&["LastPlayed"])
            .and_then(Tag::as_i64)
            .filter(|time| *time > 0)
            .or_else(|| millis(metadata.modified())),
        game_mode,
        hardcore,
        cheats: field(&["allowCommands"]).and_then(Tag::as_bool).unwrap_or(false),
        difficulty,
        version: field(&["Version", "Name"]).and_then(Tag::as_str).map(str::to_string),
        has_icon: world_dir.join("icon.png").is_file(),
        play_seconds: play_seconds(world_dir),
        created: millis(std::fs::metadata(world_dir).and_then(|meta| meta.created())),
        folder,
    })
}

/// The longest any player has spent in the world, from the stats files. 26.x keeps them in
/// players/stats; 1.13 to 1.21 in stats; the stat was renamed along the way.
fn play_seconds(world_dir: &Path) -> Option<u64> {
    let mut best: Option<u64> = None;
    for folder in ["players/stats", "stats"] {
        let Ok(entries) = std::fs::read_dir(world_dir.join(folder)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok(stats) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                continue;
            };
            let custom = &stats["stats"]["minecraft:custom"];
            let ticks = custom["minecraft:play_time"]
                .as_u64()
                .or_else(|| custom["minecraft:play_one_minute"].as_u64())
                .or_else(|| stats["stat.playOneMinute"].as_u64());
            if let Some(ticks) = ticks {
                best = Some(best.unwrap_or(0).max(ticks / 20));
            }
        }
    }
    best
}

/// Every world in a game folder with what level.dat says about it, most recently played first.
pub fn list_info(game_dir: &Path) -> Vec<WorldInfo> {
    let Ok(entries) = std::fs::read_dir(game_dir.join(SAVES)) else {
        return Vec::new();
    };
    let mut worlds: Vec<WorldInfo> = entries
        .flatten()
        .filter_map(|entry| read_info(&entry.path()))
        .collect();
    worlds.sort_by_key(|world| std::cmp::Reverse(world.last_played));
    worlds
}

/// A world's icon.png, if it has one.
pub fn icon_path(game_dir: &Path, folder: &str) -> Option<PathBuf> {
    world_dir(game_dir, folder)
        .map(|dir| dir.join("icon.png"))
        .filter(|icon| icon.is_file())
}

/// Packs a world into a zip with its folder at the top, the way worlds are shared. Returns the
/// zip's size.
pub fn export_zip(world_dir: &Path, zip_path: &Path) -> std::io::Result<u64> {
    let folder = world_dir
        .file_name()
        .ok_or_else(|| std::io::Error::other("not a world folder"))?
        .to_string_lossy()
        .into_owned();
    let temp = zip_path.with_extension("zip.part");
    let mut writer = zip::ZipWriter::new(std::fs::File::create(&temp)?);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(true);
    let mut pending = vec![world_dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                pending.push(path);
                continue;
            }
            if !kind.is_file() || entry.file_name() == SESSION_LOCK {
                continue;
            }
            let relative = path
                .strip_prefix(world_dir)
                .map_err(std::io::Error::other)?
                .to_string_lossy()
                .replace('\\', "/");
            writer
                .start_file(format!("{folder}/{relative}"), options)
                .map_err(std::io::Error::other)?;
            std::io::copy(&mut std::fs::File::open(&path)?, &mut writer)?;
        }
    }
    writer.finish().map_err(std::io::Error::other)?.flush()?;
    std::fs::rename(&temp, zip_path)?;
    Ok(std::fs::metadata(zip_path)?.len())
}

/// One world, for picking where a data pack goes.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldEntry {
    /// The world's folder name (Minecraft shows the world's own name, usually the same).
    pub folder: String,
    /// When it was last saved, in Unix milliseconds.
    pub last_played: Option<i64>,
}

/// The worlds in a game folder, most recently played first.
pub fn list(game_dir: &Path) -> Vec<WorldEntry> {
    let Ok(entries) = std::fs::read_dir(game_dir.join(SAVES)) else {
        return Vec::new();
    };
    let mut worlds: Vec<WorldEntry> = entries
        .flatten()
        .filter_map(|entry| {
            let level = entry.path().join("level.dat");
            let metadata = std::fs::metadata(&level).ok()?;
            let last_played = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|elapsed| elapsed.as_millis() as i64);
            Some(WorldEntry {
                folder: entry.file_name().to_string_lossy().into_owned(),
                last_played,
            })
        })
        .collect();
    worlds.sort_by_key(|world| std::cmp::Reverse(world.last_played));
    worlds
}

/// The folder of a world in the saves folder, if `folder` names one (and nothing sneakier).
pub fn world_dir(game_dir: &Path, folder: &str) -> Option<PathBuf> {
    let mut parts = Path::new(folder).components();
    let plain = matches!(parts.next(), Some(Component::Normal(_))) && parts.next().is_none();
    let dir = game_dir.join(SAVES).join(folder);
    (plain && dir.join("level.dat").is_file()).then_some(dir)
}

/// Unpacks a world from a zip into the saves folder, under a name no other world has. Worlds are
/// zipped with level.dat at the top or inside a folder (or two); that folder's contents are what
/// gets copied. Returns the new world's folder name.
pub fn import_zip(zip_path: &Path, game_dir: &Path, name: &str) -> std::io::Result<String> {
    let bad = |why: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, why.to_string());
    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip_path)?)
        .map_err(|error| bad(&error.to_string()))?;
    // Where level.dat sits decides what the world's root is.
    let mut root: Option<String> = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| bad(&error.to_string()))?;
        let Some(path) = entry.enclosed_name() else {
            continue;
        };
        if path.file_name().is_some_and(|file| file == "level.dat")
            && path.components().count() <= 3
        {
            let parent = path
                .parent()
                .map(|parent| parent.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if root.as_ref().is_none_or(|known| parent.len() < known.len()) {
                root = Some(parent);
            }
        }
    }
    let root = root.ok_or_else(|| bad("there's no world (level.dat) in it"))?;
    let saves = game_dir.join(SAVES);
    std::fs::create_dir_all(&saves)?;
    let folder = unused_name(&saves, name);
    let target = saves.join(&folder);
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| bad(&error.to_string()))?;
        let Some(path) = entry.enclosed_name() else {
            continue;
        };
        let path = path.to_string_lossy().replace('\\', "/");
        let relative = if root.is_empty() {
            path.as_str()
        } else {
            match path.strip_prefix(&format!("{root}/")) {
                Some(relative) => relative,
                None => continue,
            }
        };
        if relative.is_empty() || entry.is_dir() {
            continue;
        }
        let destination = target.join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        std::fs::write(destination, bytes)?;
    }
    Ok(folder)
}

/// Copies a world folder (one with a level.dat) into the saves folder under a name no other
/// world has. Returns the new world's folder name.
pub fn import_folder(source: &Path, game_dir: &Path, name: &str) -> std::io::Result<String> {
    if !source.join("level.dat").is_file() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "there's no world (level.dat) in it"));
    }
    let saves = game_dir.join(SAVES);
    std::fs::create_dir_all(&saves)?;
    let folder = unused_name(&saves, name);
    let target = saves.join(&folder);
    let mut pending = vec![source.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let kind = entry.file_type()?;
            let path = entry.path();
            let relative = path.strip_prefix(source).map_err(std::io::Error::other)?;
            if kind.is_dir() {
                std::fs::create_dir_all(target.join(relative))?;
                pending.push(path);
            } else if kind.is_file() && entry.file_name() != SESSION_LOCK {
                if let Some(parent) = target.join(relative).parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&path, target.join(relative))?;
            }
        }
    }
    Ok(folder)
}

/// A folder name for a new world: the name made safe, with " (2)" and so on if it's taken.
fn unused_name(saves: &Path, name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric()
                || matches!(c, ' ' | '-' | '_' | '(' | ')' | '\'' | '!' | '&' | ',')
            {
                c
            } else {
                '_'
            }
        })
        .collect();
    let base = cleaned.trim().trim_matches('.').to_string();
    let base = if base.is_empty() {
        "World".to_string()
    } else {
        base
    };
    let mut candidate = base.clone();
    let mut counter = 2;
    while saves.join(&candidate).exists() {
        candidate = format!("{base} ({counter})");
        counter += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_with(path: &Path, files: &[(&str, &[u8])]) {
        let mut writer = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, bytes) in files {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn imports_nested_worlds_under_a_free_name_and_lists_them() {
        let root = std::env::temp_dir().join(format!("wisp-worlds-{}", std::process::id()));
        let game = root.join("game");
        std::fs::create_dir_all(game.join("saves/Skyblock")).unwrap();
        std::fs::write(game.join("saves/Skyblock/level.dat"), b"old").unwrap();
        let zip = root.join("map.zip");
        zip_with(
            &zip,
            &[
                ("Skyblock v2/level.dat", b"new"),
                ("Skyblock v2/region/r.0.0.mca", b"r"),
                ("readme.txt", b"hi"),
            ],
        );

        let folder = import_zip(&zip, &game, "Skyblock").unwrap();
        assert_eq!(folder, "Skyblock (2)");
        assert_eq!(
            std::fs::read(game.join("saves/Skyblock (2)/level.dat")).unwrap(),
            b"new"
        );
        assert!(game.join("saves/Skyblock (2)/region/r.0.0.mca").is_file());
        assert!(!game.join("saves/Skyblock (2)/readme.txt").exists());
        assert_eq!(list(&game).len(), 2);
        assert!(world_dir(&game, "Skyblock").is_some());
        assert!(world_dir(&game, "../game").is_none());

        zip_with(&zip, &[("readme.txt", b"no world here")]);
        assert!(import_zip(&zip, &game, "Nothing").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reads_level_dat_old_and_new_and_exports() {
        use crate::nbt::tests::Builder;
        let root = std::env::temp_dir().join(format!("wisp-world-info-{}", std::process::id()));
        let game = root.join("game");
        // 26.x: difficulty as a word under difficulty_settings, stats under players/.
        let new_world = game.join("saves/New World");
        std::fs::create_dir_all(new_world.join("players/stats")).unwrap();
        let level = Builder::root()
            .open("Data")
            .string("LevelName", "Cherry Hills")
            .long("LastPlayed", 1_794_760_228_899)
            .int("GameType", 0)
            .byte("allowCommands", 1)
            .open("difficulty_settings")
            .string("difficulty", "hard")
            .byte("hardcore", 1)
            .close()
            .open("Version")
            .string("Name", "26.3")
            .close()
            .gzipped();
        std::fs::write(new_world.join("level.dat"), level).unwrap();
        std::fs::write(new_world.join(SESSION_LOCK), b"lock").unwrap();
        std::fs::write(
            new_world.join("players/stats/a.json"),
            br#"{"stats":{"minecraft:custom":{"minecraft:play_time":72000}}}"#,
        )
        .unwrap();
        // 1.12-era: numbers, and the old stat name.
        let old_world = game.join("saves/Old");
        std::fs::create_dir_all(old_world.join("stats")).unwrap();
        let level = Builder::root()
            .open("Data")
            .string("LevelName", "Old Times")
            .int("GameType", 1)
            .byte("Difficulty", 0)
            .gzipped();
        std::fs::write(old_world.join("level.dat"), level).unwrap();
        std::fs::write(old_world.join("stats/b.json"), br#"{"stat.playOneMinute":1200}"#).unwrap();

        let worlds = list_info(&game);
        assert_eq!(worlds.len(), 2);
        let new = worlds.iter().find(|world| world.folder == "New World").unwrap();
        assert_eq!(new.name, "Cherry Hills");
        assert_eq!(new.last_played, Some(1_794_760_228_899));
        assert_eq!(new.game_mode, GameMode::Survival);
        assert!(new.hardcore && new.cheats);
        assert_eq!(new.difficulty.as_deref(), Some("hard"));
        assert_eq!(new.version.as_deref(), Some("26.3"));
        assert_eq!(new.play_seconds, Some(3600));
        let old = worlds.iter().find(|world| world.folder == "Old").unwrap();
        assert_eq!((old.name.as_str(), old.game_mode), ("Old Times", GameMode::Creative));
        assert_eq!(old.difficulty.as_deref(), Some("peaceful"));
        assert_eq!(old.play_seconds, Some(60));
        assert!(old.last_played.is_some(), "falls back to the file's time");

        // Exported with its folder on top and without the lock; it imports back.
        let zip = root.join("export.zip");
        assert!(export_zip(&new_world, &zip).unwrap() > 0);
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&zip).unwrap()).unwrap();
        assert!(archive.by_name("New World/level.dat").is_ok());
        assert!(archive.by_name("New World/session.lock").is_err());
        assert_eq!(import_zip(&zip, &game, "Cherry Hills").unwrap(), "Cherry Hills");
        // A dropped folder copies in, lock and all left behind.
        assert_eq!(import_folder(&new_world, &game, "Cherry Hills").unwrap(), "Cherry Hills (2)");
        assert!(game.join("saves/Cherry Hills (2)/players/stats/a.json").is_file());
        assert!(!game.join("saves/Cherry Hills (2)").join(SESSION_LOCK).exists());
        assert!(import_folder(&root, &game, "Nope").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
