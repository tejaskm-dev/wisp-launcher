//! Bringing a singleplayer world into a server, so the world friends join is the one you've been
//! playing. The world is copied (the singleplayer one stays as it was), and the host keeps their
//! inventory and where they stood.

use std::io::Write;
use std::path::Path;

use super::players::dashed;

/// The server's world folder (its `level-name`).
pub const WORLD: &str = "world";

/// Copies `source` (a world folder) in as the server's world. A world already there is kept beside
/// it as `world-old`, `world-old-2`…; its folder name comes back. `host_uuid` is the player who
/// played it in singleplayer, so their things come along.
pub fn import(source: &Path, server_dir: &Path, host_uuid: Option<&str>) -> std::io::Result<Option<String>> {
    if !source.join("level.dat").is_file() {
        return Err(std::io::Error::other("that folder isn't a Minecraft world"));
    }
    let target = server_dir.join(WORLD);
    let kept = if target.exists() {
        let name = (1..)
            .map(|n| if n == 1 { "world-old".to_string() } else { format!("world-old-{n}") })
            .find(|name| !server_dir.join(name).exists())
            .expect("an unused name always exists");
        std::fs::rename(&target, server_dir.join(&name))?;
        Some(name)
    } else {
        None
    };
    copy_dir(source, &target)?;
    if let Some(uuid) = host_uuid {
        // Worth trying; the world works without it.
        let _ = keep_host_player(&target, uuid);
    }
    Ok(kept)
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        // The game's lock belongs to the copy it was taken from.
        if name == "session.lock" {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_dir(&entry.path(), &to.join(&name))?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), to.join(&name))?;
        }
    }
    Ok(())
}

/// Singleplayer worlds from before 26.1 keep the host's data inside level.dat, where a server never
/// looks. It's written out to playerdata/<uuid>.dat, where the server reads that player's data.
/// Newer worlds already keep every player in players/data, the server's own place.
fn keep_host_player(world: &Path, uuid: &str) -> Result<(), Box<dyn std::error::Error>> {
    let level = crate::nbt::read_file(&world.join("level.dat"))?;
    let Some(player) = level.at(&["Data", "Player"]) else {
        return Ok(());
    };
    let dir = world.join("playerdata");
    let file = dir.join(format!("{}.dat", dashed(uuid)));
    if file.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(&dir)?;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&crate::nbt::to_bytes(player))?;
    std::fs::write(file, encoder.finish()?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::nbt::Tag;

    fn compound(entries: Vec<(&str, Tag)>) -> Tag {
        Tag::Compound(entries.into_iter().map(|(key, value)| (key.to_string(), value)).collect::<HashMap<_, _>>())
    }

    #[test]
    fn copies_the_world_and_keeps_the_hosts_things() {
        let root = std::env::temp_dir().join(format!("wisp-host-world-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let source = root.join("saves/Island");
        std::fs::create_dir_all(source.join("region")).unwrap();
        let player = compound(vec![("XpLevel", Tag::Int(30)), ("Pos", Tag::List(vec![Tag::Double(1.0), Tag::Double(64.0), Tag::Double(2.0)]))]);
        let level = compound(vec![("Data", compound(vec![("LevelName", Tag::String("Island".into())), ("Player", player.clone())]))]);
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&crate::nbt::to_bytes(&level)).unwrap();
        std::fs::write(source.join("level.dat"), encoder.finish().unwrap()).unwrap();
        std::fs::write(source.join("region/r.0.0.mca"), b"region").unwrap();
        std::fs::write(source.join("session.lock"), b"lock").unwrap();

        let server = root.join("server");
        std::fs::create_dir_all(server.join(WORLD)).unwrap();
        let kept = import(&source, &server, Some("9a439d5fcb533739aca3f8e1ccb9148b")).unwrap();
        assert_eq!(kept.as_deref(), Some("world-old"));
        assert!(server.join("world-old").is_dir());
        assert_eq!(std::fs::read(server.join("world/region/r.0.0.mca")).unwrap(), b"region");
        assert!(!server.join("world/session.lock").exists());
        let saved = crate::nbt::read_file(&server.join("world/playerdata/9a439d5f-cb53-3739-aca3-f8e1ccb9148b.dat")).unwrap();
        assert_eq!(saved, player);
        // The singleplayer world is untouched.
        assert!(source.join("session.lock").exists());
        assert!(import(&root.join("nothing"), &server, None).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
