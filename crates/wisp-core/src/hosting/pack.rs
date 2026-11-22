//! What a friend needs to play on a server: its game version and loader, and the mods of the
//! instance people play it with. The host's Wisp makes the list (`Pack`); the friend's Wisp makes
//! a matching instance from it (`plan`), downloading what Modrinth serves and taking the rest over
//! the link. The server itself gets the same mods minus the ones that only run in the game
//! (`sync_server_mods`).

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use super::HostedServer;
use crate::inventory::Shareable;

/// What the server's folder records about the mods Wisp copied in from the instance.
const COPIED: &str = ".wisp-mods.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pack {
    pub server_id: String,
    pub name: String,
    pub game_version: String,
    /// "vanilla", "fabric", "quilt", "forge" or "neoforge".
    pub loader: String,
    #[serde(default)]
    pub loader_version: Option<String>,
    pub files: Vec<PackFile>,
    /// The port of the host's skin server on its computer, in builds where players sign in through
    /// it: a friend's Wisp reaches it over the link at the same port on theirs, so the skin
    /// addresses the host hands out work there too.
    #[serde(default)]
    pub skin_port: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackFile {
    /// In the game folder: "mods/sodium.jar".
    pub path: String,
    pub sha1: String,
    pub size: u64,
    /// Where it can be downloaded (Modrinth's CDN); without one it comes over the link.
    #[serde(default)]
    pub url: Option<String>,
}

/// Only mods for now: "mods/<name>.jar", and nothing that could climb out of the folder.
pub fn safe_path(path: &str) -> bool {
    let mut parts = path.split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some("mods"), Some(name), None) => {
            name.ends_with(".jar")
                && !name.starts_with('.')
                && name.len() <= 200
                && name.chars().all(|c| !c.is_control() && !matches!(c, '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        }
        _ => false,
    }
}

fn valid_sha1(sha1: &str) -> bool {
    sha1.len() == 40 && sha1.chars().all(|c| c.is_ascii_hexdigit())
}

/// Only Modrinth's CDN, over HTTPS: a host can't point a friend's Wisp anywhere else.
fn trusted_url(url: &str) -> bool {
    url.starts_with("https://cdn.modrinth.com/")
}

impl Pack {
    /// The pack the host shares: the server's version and loader, and the turned-on mods of the
    /// instance people play it with.
    pub fn from_instance(server: &HostedServer, shareable: &[Shareable]) -> Self {
        let files = shareable
            .iter()
            .filter(|file| safe_path(&file.path))
            .map(|file| PackFile { path: file.path.clone(), sha1: file.sha1.to_ascii_lowercase(), size: file.size, url: file.url.clone().filter(|url| trusted_url(url)) })
            .collect();
        Self::with_files(server, files)
    }

    /// The pack for a server with no instance of its own: the mods in its folder that players need
    /// too (not those that say they only run on a server), hashed here; Modrinth's addresses can
    /// be added after.
    pub fn from_server_mods(server: &HostedServer, dir: &Path) -> Self {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(dir.join("mods")).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = format!("mods/{name}");
            if !safe_path(&path) || !entry.file_type().is_ok_and(|kind| kind.is_file()) || crate::doctor::read_jar(&entry.path()).server_only {
                continue;
            }
            if let Some((sha1, size)) = hash_file(&entry.path()) {
                files.push(PackFile { path, sha1, size, url: None });
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Self::with_files(server, files)
    }

    fn with_files(server: &HostedServer, files: Vec<PackFile>) -> Self {
        Self {
            server_id: server.id.clone(),
            name: server.name.clone(),
            game_version: server.game_version.clone(),
            loader: server.loader.as_str().to_string(),
            loader_version: server.loader_version.clone(),
            files,
            skin_port: None,
        }
    }

    /// Drops anything a friend's Wisp shouldn't act on (odd paths, bad hashes, other addresses).
    pub fn cleaned(mut self) -> Self {
        let mut seen = HashSet::new();
        self.files.retain(|file| safe_path(&file.path) && valid_sha1(&file.sha1) && seen.insert(file.path.clone()));
        for file in &mut self.files {
            file.sha1 = file.sha1.to_ascii_lowercase();
            file.url = file.url.take().filter(|url| trusted_url(url));
        }
        self
    }
}

/// SHA-1 (hex) and size of a file.
pub fn hash_file(path: &Path) -> Option<(String, u64)> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha1::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    Some((crate::launch::hex(hasher.finalize().as_slice()), size))
}

/// What a friend's instance needs to match a pack.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// Files to fetch (from their address, or over the link).
    pub get: Vec<PackFile>,
    /// Files an earlier join put there that the pack no longer has.
    pub remove: Vec<String>,
}

/// Compares a pack with a friend's game folder. `installed` is what earlier joins put there
/// (path → SHA-1): those are trusted while their size still matches; anything else at a pack's
/// path is hashed. Files the friend added themselves are left alone.
pub fn plan(pack: &Pack, game_dir: &Path, installed: &HashMap<String, String>) -> Plan {
    let mut plan = Plan::default();
    let wanted: HashSet<&str> = pack.files.iter().map(|file| file.path.as_str()).collect();
    for file in &pack.files {
        let path = game_dir.join(&file.path);
        let size = std::fs::metadata(&path).ok().map(|meta| meta.len());
        let current = match size {
            Some(size) if size == file.size && installed.get(&file.path) == Some(&file.sha1) => true,
            Some(size) if size == file.size => hash_file(&path).is_some_and(|(sha1, _)| sha1 == file.sha1),
            _ => false,
        };
        if !current {
            plan.get.push(file.clone());
        }
    }
    let mut remove: Vec<String> = installed.keys().filter(|path| !wanted.contains(path.as_str()) && safe_path(path)).cloned().collect();
    remove.sort();
    plan.remove = remove;
    plan
}

/// What `sync_server_mods` changed.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Synced {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

/// Brings a server's mods in line with the instance people play it with: copies in the instance's
/// mods except those that only run in the game, and takes out ones the instance dropped (only
/// those Wisp copied in before; a jar put in the server's mods folder by hand stays).
/// `client_only` holds the SHA-1s Modrinth says never run on a server.
pub fn sync_server_mods(instance_game_dir: &Path, server_dir: &Path, shareable: &[Shareable], client_only: &HashSet<String>) -> std::io::Result<Synced> {
    let record = server_dir.join(COPIED);
    let mut copied: HashMap<String, String> = std::fs::read(&record).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
    let mut synced = Synced::default();
    let mut keep = HashSet::new();
    std::fs::create_dir_all(server_dir.join("mods"))?;
    for file in shareable.iter().filter(|file| safe_path(&file.path)) {
        let source = instance_game_dir.join(&file.path);
        if client_only.contains(&file.sha1) || crate::doctor::read_jar(&source).client_only {
            continue;
        }
        keep.insert(file.path.clone());
        let target = server_dir.join(&file.path);
        let same = copied.get(&file.path) == Some(&file.sha1) && std::fs::metadata(&target).is_ok_and(|meta| meta.len() == file.size);
        if same {
            continue;
        }
        std::fs::copy(&source, &target)?;
        copied.insert(file.path.clone(), file.sha1.clone());
        synced.added.push(file.path.clone());
    }
    let gone: Vec<String> = copied.keys().filter(|path| !keep.contains(*path)).cloned().collect();
    for path in gone {
        if safe_path(&path) {
            let _ = std::fs::remove_file(server_dir.join(&path));
        }
        copied.remove(&path);
        synced.removed.push(path);
    }
    std::fs::write(record, serde_json::to_vec(&copied).map_err(std::io::Error::other)?)?;
    synced.added.sort();
    synced.removed.sort();
    Ok(synced)
}

/// A modpack's files meant for its server (a `.mrpack`'s `server-overrides`), into the server's
/// folder.
pub fn apply_server_overrides(pack: &Path, server_dir: &Path) -> std::io::Result<usize> {
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(std::fs::File::open(pack)?)).map_err(std::io::Error::other)?;
    let mut written = 0;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(std::io::Error::other)?;
        let Ok(full) = entry.name() else { continue };
        let Some(relative) = full.strip_prefix("server-overrides/").map(str::to_string) else { continue };
        // Nothing that climbs out of the folder.
        let safe = !relative.is_empty() && !relative.starts_with('/') && !relative.split(['/', '\\']).any(|part| part == ".." || part.contains(':'));
        if !safe || !entry.is_file() {
            continue;
        }
        let target = server_dir.join(&relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&target)?;
        std::io::copy(&mut entry, &mut out)?;
        written += 1;
    }
    Ok(written)
}

/// Copies the instance's settings folders the server reads too (mods' configs, KubeJS and
/// CraftTweaker scripts), without replacing anything the server already has.
pub fn copy_configs(instance_game_dir: &Path, server_dir: &Path) -> std::io::Result<()> {
    for folder in ["config", "defaultconfigs", "kubejs", "scripts"] {
        let source = instance_game_dir.join(folder);
        if source.is_dir() {
            copy_missing(&source, &server_dir.join(folder))?;
        }
    }
    Ok(())
}

fn copy_missing(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_missing(&entry.path(), &target)?;
        } else if kind.is_file() && !target.exists() {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// A plain server record for tests elsewhere in the crate.
#[cfg(test)]
pub(crate) fn tests_server() -> HostedServer {
    tests::server()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::Loader;

    pub(super) fn server() -> HostedServer {
        HostedServer {
            id: "build".into(),
            name: "Build".into(),
            game_version: "26.3".into(),
            loader: Loader::Fabric,
            loader_version: Some("0.20.1".into()),
            created: 0,
            last_started: None,
            memory_mb: None,
            instance_id: None,
            eula: false,
            friends_join: true,
            invited: vec![],
            open_port: false,
            address: None,
            software: None,
            start_with_wisp: false,
            restart_every_hours: None,
            aikar_flags: false,
            jvm_args: None,
            java_major: None,
            pack_file: None,
            backups: true,
            icon: None,
        }
    }

    fn jar(path: &Path, fabric_json: &str) {
        let mut writer = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        writer.start_file("fabric.mod.json", zip::write::SimpleFileOptions::default()).unwrap();
        std::io::Write::write_all(&mut writer, fabric_json.as_bytes()).unwrap();
        writer.finish().unwrap();
    }

    #[test]
    fn lays_out_a_modpacks_server_files_and_nothing_outside() {
        let root = std::env::temp_dir().join(format!("wisp-server-overrides-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("server")).unwrap();
        let pack = root.join("pack.mrpack");
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&pack).unwrap());
        for (name, text) in [
            ("modrinth.index.json", "{}"),
            ("overrides/config/client.toml", "client"),
            ("server-overrides/config/server.toml", "server"),
            ("server-overrides/../escape.txt", "no"),
            ("server-overrides/server.properties", "motd=Pack"),
        ] {
            writer.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            std::io::Write::write_all(&mut writer, text.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
        assert_eq!(apply_server_overrides(&pack, &root.join("server")).unwrap(), 2);
        assert_eq!(std::fs::read_to_string(root.join("server/config/server.toml")).unwrap(), "server");
        assert!(root.join("server/server.properties").is_file());
        assert!(!root.join("server/config/client.toml").exists() && !root.join("escape.txt").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn paths_and_addresses_stay_safe() {
        assert!(safe_path("mods/sodium-fabric-0.6.jar"));
        for bad in ["mods/../x.jar", "../mods/x.jar", "mods/x.zip", "mods/a/b.jar", "config/x.jar", "mods/.hidden.jar", "mods/c:\\x.jar", "/mods/x.jar"] {
            assert!(!safe_path(bad), "{bad}");
        }
        let pack = Pack {
            server_id: "s".into(),
            name: "S".into(),
            game_version: "26.3".into(),
            loader: "fabric".into(),
            loader_version: None,
            skin_port: None,
            files: vec![
                PackFile { path: "mods/a.jar".into(), sha1: "A".repeat(40), size: 1, url: Some("https://cdn.modrinth.com/data/x/a.jar".into()) },
                PackFile { path: "mods/a.jar".into(), sha1: "b".repeat(40), size: 1, url: None },
                PackFile { path: "mods/b.jar".into(), sha1: "c".repeat(40), size: 1, url: Some("https://evil.example/b.jar".into()) },
                PackFile { path: "mods/c.jar".into(), sha1: "not a hash".into(), size: 1, url: None },
            ],
        }
        .cleaned();
        assert_eq!(pack.files.len(), 2);
        assert_eq!(pack.files[0].sha1, "a".repeat(40));
        assert!(pack.files[0].url.is_some());
        assert_eq!(pack.files[1].url, None, "only Modrinth's CDN is trusted");
    }

    #[test]
    fn plans_a_friends_instance_and_syncs_the_servers_mods() {
        let root = std::env::temp_dir().join(format!("wisp-pack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let instance = root.join("instance");
        std::fs::create_dir_all(instance.join("mods")).unwrap();
        jar(&instance.join("mods/both.jar"), r#"{"id":"both","environment":"*"}"#);
        jar(&instance.join("mods/minimap.jar"), r#"{"id":"minimap","environment":"client"}"#);
        jar(&instance.join("mods/shaders.jar"), r#"{"id":"shaders"}"#);
        let shareable: Vec<Shareable> = ["both", "minimap", "shaders"]
            .iter()
            .map(|name| {
                let path = format!("mods/{name}.jar");
                let (sha1, size) = hash_file(&instance.join(&path)).unwrap();
                Shareable { path, sha1, size, url: None, project_id: None }
            })
            .collect();
        let shaders_sha1 = shareable[2].sha1.clone();

        // The server gets what runs on a server: not the one that says it's client-only, nor the
        // one Modrinth says is.
        let server_dir = root.join("server");
        std::fs::create_dir_all(server_dir.join("mods")).unwrap();
        std::fs::write(server_dir.join("mods/by-hand.jar"), b"the host's own").unwrap();
        let synced = sync_server_mods(&instance, &server_dir, &shareable, &HashSet::from([shaders_sha1])).unwrap();
        assert_eq!(synced.added, ["mods/both.jar"]);
        assert!(server_dir.join("mods/both.jar").is_file() && !server_dir.join("mods/minimap.jar").exists());
        // Again: nothing to do. Then the instance drops it: it goes, the hand-added one stays.
        assert_eq!(sync_server_mods(&instance, &server_dir, &shareable, &HashSet::new()).unwrap().added, ["mods/shaders.jar"]);
        let synced = sync_server_mods(&instance, &server_dir, &shareable[1..], &HashSet::new()).unwrap();
        assert_eq!(synced.removed, ["mods/both.jar"]);
        assert!(server_dir.join("mods/by-hand.jar").is_file());

        // A friend: the pack is every mod in the instance; what they have already is kept.
        let pack = Pack::from_instance(&server(), &shareable);
        assert_eq!(pack.files.len(), 3);
        let friend = root.join("friend");
        std::fs::create_dir_all(friend.join("mods")).unwrap();
        std::fs::copy(instance.join("mods/both.jar"), friend.join("mods/both.jar")).unwrap();
        std::fs::write(friend.join("mods/their-own.jar"), b"mine").unwrap();
        let installed = HashMap::from([("mods/old.jar".to_string(), "d".repeat(40))]);
        let first = plan(&pack, &friend, &installed);
        assert_eq!(first.get.iter().map(|file| file.path.as_str()).collect::<Vec<_>>(), ["mods/minimap.jar", "mods/shaders.jar"]);
        assert_eq!(first.remove, ["mods/old.jar"]);
        // From the server's own folder when there's no instance.
        let own = Pack::from_server_mods(&server(), &server_dir);
        assert!(own.files.iter().any(|file| file.path == "mods/by-hand.jar"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
