//! A server's own plugins (Paper and its kin) or mods (a modded server made from scratch): what's
//! in its folder, and installing from Modrinth along with what each one needs. A server made from
//! an instance takes its mods from the instance instead (see `pack::sync_server_mods`).

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::HostedServer;
use crate::HttpClient;
use crate::instances::Loader;
use crate::launch::{self, Download};
use crate::modrinth::{self, ProjectType, ProjectVersion};

/// Which project each file came from, for what Wisp installed.
const INDEX: &str = ".wisp-content.json";
const DISABLED: &str = ".disabled";
/// Enough for any plugin's or mod's dependencies, and no runaway.
const MAX_PROJECTS: usize = 24;

/// What a server takes: plugins (Paper and its kin), mods (a modded loader), or neither.
pub fn folder_of(server: &HostedServer) -> Option<&'static str> {
    match (server.software, server.loader) {
        (Some(_), _) => Some("plugins"),
        (None, Loader::Vanilla) => None,
        (None, _) => Some("mods"),
    }
}

/// What Modrinth calls them for this server, and the loaders they're made for.
pub fn kind_of(server: &HostedServer) -> Option<(ProjectType, Vec<&'static str>)> {
    match server.software {
        Some(software) => Some((ProjectType::Plugin, software.plugin_loaders().to_vec())),
        None => crate::content::compatible_loaders(server.loader, ProjectType::Mod).map(|loaders| (ProjectType::Mod, loaders)),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerItem {
    /// The jar's file name, without ".disabled".
    pub file: String,
    pub name: String,
    pub version: String,
    pub enabled: bool,
    /// The Modrinth project, when Wisp installed it.
    pub project_id: Option<String>,
}

fn index(dir: &Path) -> HashMap<String, String> {
    std::fs::read(dir.join(INDEX)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

fn write_index(dir: &Path, index: &HashMap<String, String>) -> std::io::Result<()> {
    std::fs::write(dir.join(INDEX), serde_json::to_vec_pretty(index).map_err(std::io::Error::other)?)
}

/// A plain jar file name: nothing that leaves the folder.
fn plain_name(file: &str) -> bool {
    file.ends_with(".jar") && !file.starts_with('.') && file.len() <= 200 && !file.contains(['/', '\\', ':']) && !file.contains("..")
}

/// A plugin's name and version, from its plugin.yml (or paper-plugin.yml).
fn read_plugin(path: &Path) -> Option<(String, String)> {
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file)).ok()?;
    let mut text = String::new();
    for name in ["paper-plugin.yml", "plugin.yml", "bungee.yml", "velocity-plugin.json"] {
        if let Ok(entry) = archive.by_name(name) {
            entry.take(64 * 1024).read_to_string(&mut text).ok()?;
            break;
        }
    }
    let field = |key: &str| {
        text.lines().find_map(|line| {
            let value = line.strip_prefix(key)?.trim_start().strip_prefix(':')?.trim();
            Some(value.trim_matches(|c| c == '"' || c == '\'').to_string())
        })
    };
    Some((field("name")?, field("version").unwrap_or_default()))
}

/// What's in the server's plugins or mods folder, on and off, by name.
pub fn list(server: &HostedServer, dir: &Path) -> Vec<ServerItem> {
    let Some(folder) = folder_of(server) else {
        return Vec::new();
    };
    let index = index(dir);
    let mut items: Vec<ServerItem> = std::fs::read_dir(dir.join(folder))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let (file, enabled) = match name.strip_suffix(DISABLED) {
                Some(file) => (file.to_string(), false),
                None => (name.clone(), true),
            };
            if !plain_name(&file) || !entry.file_type().is_ok_and(|kind| kind.is_file()) {
                return None;
            }
            let path = entry.path();
            let (title, version) = if folder == "plugins" {
                read_plugin(&path).unwrap_or_default()
            } else {
                let info = crate::doctor::read_jar(&path);
                (info.name, info.version)
            };
            let title = if title.is_empty() { file.trim_end_matches(".jar").to_string() } else { title };
            Some(ServerItem { project_id: index.get(&file).cloned(), file, name: title, version, enabled })
        })
        .collect();
    items.sort_by_key(|item| item.name.to_lowercase());
    items
}

/// Turns a plugin or mod on or off (off is the same file ending in ".disabled").
pub fn set_enabled(server: &HostedServer, dir: &Path, file: &str, enabled: bool) -> std::io::Result<()> {
    let folder = dir.join(folder_of(server).ok_or_else(|| std::io::Error::other("this server takes neither plugins nor mods"))?);
    if !plain_name(file) {
        return Err(std::io::Error::other("not a jar"));
    }
    let (on, off) = (folder.join(file), folder.join(format!("{file}{DISABLED}")));
    match enabled {
        true if off.is_file() => std::fs::rename(off, on),
        false if on.is_file() => std::fs::rename(on, off),
        _ => Ok(()),
    }
}

/// Where a plugin or mod is now (on or off), to take it away.
pub fn path_of(server: &HostedServer, dir: &Path, file: &str) -> Option<PathBuf> {
    let folder = dir.join(folder_of(server)?);
    if !plain_name(file) {
        return None;
    }
    [folder.join(file), folder.join(format!("{file}{DISABLED}"))].into_iter().find(|path| path.is_file())
}

/// Forgets which project a file came from, once it's gone.
pub fn forget(dir: &Path, file: &str) {
    let mut known = index(dir);
    if known.remove(file).is_some() {
        let _ = write_index(dir, &known);
    }
}

/// The newest version made for these loaders and this Minecraft version: a release if there is one.
fn best(versions: Vec<ProjectVersion>) -> Option<ProjectVersion> {
    let newest = |list: &mut Vec<ProjectVersion>| {
        list.sort_by(|a, b| b.date_published.cmp(&a.date_published));
        list.first().cloned()
    };
    let (mut releases, mut others): (Vec<_>, Vec<_>) = versions.into_iter().partition(|version| version.version_type == "release");
    newest(&mut releases).or_else(|| newest(&mut others))
}

/// Installs a project from Modrinth into the server, with the projects it requires. Returns the
/// names of what was added or updated.
pub async fn install(client: &HttpClient, server: &HostedServer, dir: &Path, project_id: &str) -> Result<Vec<String>, String> {
    let folder_name = folder_of(server).ok_or("Vanilla servers take neither plugins nor mods. Make a Paper server for plugins, or a modded one for mods.")?;
    let (_, loaders) = kind_of(server).ok_or("This server can't take mods.")?;
    let folder = dir.join(folder_name);
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let mut known = index(dir);
    let mut installed = Vec::new();
    let mut seen = HashSet::new();
    let mut queue = vec![(project_id.to_string(), true)];
    while let Some((project, explicit)) = queue.pop() {
        if !seen.insert(project.clone()) || seen.len() > MAX_PROJECTS {
            continue;
        }
        // What it needs that's here already stays as it is.
        if !explicit && known.values().any(|known| *known == project) {
            continue;
        }
        let details = modrinth::project(client, &project).await.map_err(|error| error.to_string())?;
        let versions = modrinth::project_versions(client, &details.id, &loaders, &[server.game_version.as_str()]).await.map_err(|error| error.to_string())?;
        let Some(version) = best(versions) else {
            if explicit {
                let made_for = match server.software {
                    Some(software) => software.display_name(),
                    None => server.loader.display_name(),
                };
                return Err(format!("{} has no version for {made_for} {}.", details.title, server.game_version));
            }
            continue;
        };
        let Some(file) = version.files.iter().find(|file| file.primary && file.filename.ends_with(".jar")).or_else(|| version.files.iter().find(|file| file.filename.ends_with(".jar"))) else {
            continue;
        };
        if !plain_name(&file.filename) {
            continue;
        }
        // An update replaces the older file of the same project.
        let older: Vec<String> = known.iter().filter(|(name, id)| **id == details.id && **name != file.filename).map(|(name, _)| name.clone()).collect();
        let target = folder.join(&file.filename);
        if !target.is_file() {
            launch::download_with_progress(client, vec![Download { url: file.url.clone(), path: target.clone(), sha1: file.sha1.clone(), size: Some(file.size), executable: false }], 4, &|_| {})
                .await
                .map_err(|error| format!("Couldn't download {}: {error}", details.title))?;
            installed.push(details.title.clone());
        }
        for name in older {
            let _ = std::fs::remove_file(folder.join(&name));
            let _ = std::fs::remove_file(folder.join(format!("{name}{DISABLED}")));
            known.remove(&name);
        }
        known.insert(file.filename.clone(), details.id.clone());
        for dependency in &version.dependencies {
            if dependency.dependency_type == "required"
                && let Some(id) = &dependency.project_id
            {
                queue.push((id.clone(), false));
            }
        }
    }
    write_index(dir, &known).map_err(|error| error.to_string())?;
    Ok(installed)
}

/// Adds a file you have to the server, as an instance takes one: a plugin or mod jar into its
/// folder, a data pack into its world, the jars in a zip of them. Returns what was added.
pub fn add_file(server: &HostedServer, dir: &Path, path: &Path) -> Result<Vec<String>, String> {
    use crate::imports::{self, FileKind};
    let inspected = imports::inspect(path);
    let name = inspected.file_name.clone();
    let lower = name.to_ascii_lowercase();
    match inspected.kind {
        FileKind::Modpack => return Err(format!("{name} is a modpack. Make a server from it: Host a server, then From a modpack.")),
        FileKind::ResourcePack => return Err(format!("{name} is a resource pack, which players use. Set it as the server's resource pack in its settings, or add it to your instance.")),
        FileKind::ShaderPack => return Err(format!("{name} is a shader pack, which only runs in the game.")),
        FileKind::World => return Err(format!("{name} is a world. Use it from the server's Settings, under World.")),
        FileKind::DataPack => {
            let packs = dir.join(super::world::WORLD).join("datapacks");
            std::fs::create_dir_all(&packs).map_err(|error| error.to_string())?;
            std::fs::copy(path, packs.join(&name)).map_err(|error| error.to_string())?;
            return Ok(vec![name]);
        }
        _ => {}
    }
    let folder_name = folder_of(server).ok_or("Vanilla servers take neither plugins nor mods. Make a Paper server for plugins, or a modded one for mods.")?;
    let folder = dir.join(folder_name);
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    if lower.ends_with(".jar") {
        if !plain_name(&name) {
            return Err(format!("{name} has an odd file name."));
        }
        let fits = if folder_name == "plugins" { read_plugin(path).is_some() } else { inspected.kind == FileKind::Mod };
        if !fits {
            return Err(if folder_name == "plugins" { format!("{name} isn't a plugin (it has no plugin.yml).") } else { format!("{name} isn't a mod this server's loader reads.") });
        }
        std::fs::copy(path, folder.join(&name)).map_err(|error| error.to_string())?;
        return Ok(vec![name]);
    }
    if lower.ends_with(".zip") {
        // A zip of jars: each one goes in.
        let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
        let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file)).map_err(|_| format!("{name} couldn't be opened."))?;
        let mut added = Vec::new();
        for index in 0..archive.len() {
            let Ok(mut entry) = archive.by_index(index) else { continue };
            let Ok(full) = entry.name() else { continue };
            let inner = full.rsplit('/').next().unwrap_or_default().to_string();
            if !entry.is_file() || !inner.to_ascii_lowercase().ends_with(".jar") || !plain_name(&inner) || entry.size() > 512 * 1024 * 1024 {
                continue;
            }
            let target = folder.join(&inner);
            let mut out = std::fs::File::create(&target).map_err(|error| error.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|error| error.to_string())?;
            added.push(inner);
        }
        if added.is_empty() {
            return Err(format!("{name} holds no plugins or mods."));
        }
        return Ok(added);
    }
    Err(format!("Wisp doesn't know what to do with {name}."))
}

/// A newer build of something in the server, made for its software and version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    /// The file it replaces (without ".disabled").
    pub file: String,
    pub project_id: String,
    pub version_id: String,
    /// "5.5.72".
    pub version: String,
}

/// What in the server has a newer build on Modrinth for its software and Minecraft version, asked
/// in one request by the files' hashes (so it finds what was added by hand too).
pub async fn updates(client: &HttpClient, server: &HostedServer, dir: &Path) -> Result<Vec<Update>, String> {
    let (Some(folder), Some((_, loaders))) = (folder_of(server), kind_of(server)) else {
        return Ok(Vec::new());
    };
    let mut by_hash: HashMap<String, String> = HashMap::new();
    for item in list(server, dir) {
        let path = dir.join(folder).join(if item.enabled { item.file.clone() } else { format!("{}{DISABLED}", item.file) });
        if let Some((sha1, _)) = super::pack::hash_file(&path) {
            by_hash.insert(sha1, item.file);
        }
    }
    let hashes: Vec<String> = by_hash.keys().cloned().collect();
    let latest = modrinth::latest_by_hashes(client, &hashes, &loaders, &[server.game_version.as_str()]).await.map_err(|error| error.to_string())?;
    let mut found: Vec<Update> = latest
        .into_iter()
        .filter_map(|(hash, version)| {
            let file = by_hash.get(&hash)?.clone();
            // The newest is the one already there.
            let same = version.files.iter().any(|candidate| candidate.sha1.as_deref() == Some(hash.as_str()));
            (!same).then(|| Update { file, project_id: version.project_id.clone(), version_id: version.id.clone(), version: version.version_number.clone() })
        })
        .collect();
    found.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(found)
}

/// Replaces a file with its newer build (kept off if it was off).
pub async fn apply_update(client: &HttpClient, server: &HostedServer, dir: &Path, update: &Update) -> Result<String, String> {
    let folder = dir.join(folder_of(server).ok_or("This server takes neither plugins nor mods.")?);
    let version = modrinth::version(client, &update.version_id).await.map_err(|error| error.to_string())?;
    let file = version.files.iter().find(|file| file.primary && file.filename.ends_with(".jar")).or_else(|| version.files.iter().find(|file| file.filename.ends_with(".jar"))).ok_or("That update has no jar.")?;
    if !plain_name(&file.filename) || !plain_name(&update.file) {
        return Err("That update has an odd file name.".into());
    }
    let was_off = folder.join(format!("{}{DISABLED}", update.file)).is_file();
    let target = folder.join(if was_off { format!("{}{DISABLED}", file.filename) } else { file.filename.clone() });
    launch::download_with_progress(client, vec![Download { url: file.url.clone(), path: target, sha1: file.sha1.clone(), size: Some(file.size), executable: false }], 4, &|_| {})
        .await
        .map_err(|error| format!("Couldn't download the update: {error}"))?;
    if file.filename != update.file {
        let _ = std::fs::remove_file(folder.join(&update.file));
        let _ = std::fs::remove_file(folder.join(format!("{}{DISABLED}", update.file)));
    }
    let mut known = index(dir);
    known.remove(&update.file);
    known.insert(file.filename.clone(), version.project_id.clone());
    write_index(dir, &known).map_err(|error| error.to_string())?;
    Ok(file.filename.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_plugins_and_turns_them_off_and_on() {
        let dir = std::env::temp_dir().join(format!("wisp-server-content-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("plugins")).unwrap();
        let mut writer = zip::ZipWriter::new(std::fs::File::create(dir.join("plugins/EssentialsX-2.21.jar")).unwrap());
        writer.start_file("plugin.yml", zip::write::SimpleFileOptions::default()).unwrap();
        std::io::Write::write_all(&mut writer, b"name: Essentials\nversion: '2.21.0'\nmain: com.earth2me.essentials.Essentials\n").unwrap();
        writer.finish().unwrap();
        std::fs::write(dir.join("plugins/odd.jar"), b"not a zip").unwrap();
        let server = HostedServer { software: Some(super::super::software::Software::Paper), ..crate::hosting::pack::tests_server() };
        let items = list(&server, &dir);
        assert_eq!(items.iter().map(|item| (item.name.as_str(), item.version.as_str(), item.enabled)).collect::<Vec<_>>(), [("Essentials", "2.21.0", true), ("odd", "", true)]);
        set_enabled(&server, &dir, "odd.jar", false).unwrap();
        assert!(dir.join("plugins/odd.jar.disabled").is_file());
        assert!(!list(&server, &dir)[1].enabled);
        assert_eq!(path_of(&server, &dir, "odd.jar").unwrap(), dir.join("plugins/odd.jar.disabled"));
        assert!(set_enabled(&server, &dir, "../escape.jar", true).is_err());
        set_enabled(&server, &dir, "odd.jar", true).unwrap();
        assert!(dir.join("plugins/odd.jar").is_file());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
