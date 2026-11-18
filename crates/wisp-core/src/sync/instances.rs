//! Moving one instance each way: what it is (version, loader, mods and their configs), its packs
//! and screenshots, its options and its server list. Unlike worlds these merge: a mod added on
//! one computer and a server added on another both end up on both.
//!
//! Mods that Modrinth has are recorded by where to download them, not copied into the sync
//! folder; only jars you added yourself (and configs, which are small) take space there.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::merge::{LOCAL_CONFIGS, apply_options, merge_maps, merge_servers, portable_options};
use super::remote::{Commit, ItemKind, Remote, sha1_hex};
use super::state::HashCache;
use super::worlds::Who;
use super::{Fetch, SyncError};
use crate::content::{ContentIndex, InstalledContent, safe_join};
use crate::instances::{GAME_DIR, Instance, InstanceStore, Loader, ModpackOrigin, NewInstance};
use crate::servers::{self, ListedServer};
use crate::settings::SyncPrefs;

/// Bigger than this and a file isn't synced (it's not a config or a mod).
const MAX_FILE: u64 = 256 * 1024 * 1024;

/// One synced file in an instance. Two are the same file when their contents are, whether or not
/// each computer found a download link for it.
#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct SyncedFile {
    pub hash: String,
    pub size: u64,
    /// Where Modrinth serves it, when Modrinth has it (then it isn't copied to the sync folder).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl PartialEq for SyncedFile {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

type Files = BTreeMap<String, SyncedFile>;

/// Everything synced about an instance. A section is left out when the computer that synced
/// didn't sync that kind of thing; it's then carried over as it was.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InstanceManifest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<BTreeMap<String, InstalledContent>>,
    /// Mods and mod configs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Files>,
    /// Resource packs and shader packs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub packs: Option<Files>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshots: Option<Files>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub servers: Option<Vec<ListedServer>>,
}

impl InstanceManifest {
    /// What it holds, as one hash (maps serialize in key order, so this is stable).
    pub fn content_hash(&self) -> String {
        sha1_hex(&serde_json::to_vec(self).unwrap_or_default())
    }

    pub fn name(&self) -> Option<&str> {
        self.definition.as_ref()?.get("name")?.as_str()
    }

    fn file_sections(&self) -> [(&'static str, Option<&Files>); 3] {
        [("files", self.files.as_ref()), ("packs", self.packs.as_ref()), ("screenshots", self.screenshots.as_ref())]
    }

    /// Every file it lists and its size, for working out what a download costs.
    pub fn total_bytes(&self) -> u64 {
        self.file_sections().iter().filter_map(|(_, files)| *files).flat_map(|files| files.values()).map(|file| file.size).sum()
    }
}

/// The instance fields that travel (not its memory, which depends on the computer).
const DEFINITION: &[&str] = &["name", "gameVersion", "loader", "loaderVersion", "icon", "modpack", "javaMajor"];

fn definition_of(instance: &Instance) -> BTreeMap<String, serde_json::Value> {
    let value = serde_json::to_value(instance).unwrap_or_default();
    DEFINITION.iter().map(|key| (key.to_string(), value.get(*key).cloned().unwrap_or(serde_json::Value::Null))).collect()
}

/// Which folders of the game folder each section covers.
fn section_folders(section: &str) -> &'static [&'static str] {
    match section {
        "files" => &["mods", "config"],
        "packs" => &["resourcepacks", "shaderpacks"],
        _ => &["screenshots"],
    }
}

fn skipped(relative: &str, name: &str, size: u64) -> bool {
    name.starts_with('.')
        || name.ends_with(".tmp")
        || name.ends_with(".log")
        || name.ends_with(".lock")
        || name.ends_with(".wisp-sync")
        || size > MAX_FILE
        || LOCAL_CONFIGS.iter().any(|local| local.eq_ignore_ascii_case(relative))
}

/// The files in some folders of the game folder, by path relative to it.
fn collect(game_dir: &Path, folders: &[&str], cache: &mut HashCache) -> std::io::Result<Files> {
    let mut files = Files::new();
    for folder in folders {
        let mut pending = vec![game_dir.join(folder)];
        while let Some(dir) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(kind) = entry.file_type() else { continue };
                if kind.is_dir() {
                    pending.push(path);
                    continue;
                }
                if !kind.is_file() {
                    continue;
                }
                let Ok(relative) = path.strip_prefix(game_dir) else { continue };
                let relative = relative.to_string_lossy().replace('\\', "/");
                let name = entry.file_name().to_string_lossy().into_owned();
                let size = entry.metadata().map_or(0, |meta| meta.len());
                if skipped(&relative, &name, size) {
                    continue;
                }
                let (hash, size) = cache.hash(&path)?;
                files.insert(relative, SyncedFile { hash, size, url: None });
            }
        }
    }
    Ok(files)
}

/// The instance as it is on this computer, in the sections this computer syncs. Download links
/// are filled in for files Modrinth has (asking it once for any not seen before). Also says
/// whether Modrinth's answer covered every jar and pack (false when it couldn't be asked).
pub fn local_manifest(instance: &Instance, instance_dir: &Path, prefs: &SyncPrefs, cache: &mut HashCache, known: &mut BTreeMap<String, Option<String>>, fetch: &dyn Fetch) -> std::io::Result<(InstanceManifest, bool)> {
    let game_dir = instance_dir.join(GAME_DIR);
    let mut manifest = InstanceManifest::default();
    if prefs.instances {
        manifest.definition = Some(definition_of(instance));
        manifest.content = Some(ContentIndex::new(instance_dir).list().into_iter().map(|item| (item.file.clone(), item)).collect());
        manifest.files = Some(collect(&game_dir, section_folders("files"), cache)?);
    }
    if prefs.resource_packs {
        manifest.packs = Some(collect(&game_dir, section_folders("packs"), cache)?);
    }
    if prefs.screenshots {
        manifest.screenshots = Some(collect(&game_dir, section_folders("screenshots"), cache)?);
    }
    if prefs.options {
        manifest.options = Some(portable_options(&std::fs::read_to_string(game_dir.join("options.txt")).unwrap_or_default()));
    }
    if prefs.servers {
        manifest.servers = Some(servers::listed(&game_dir));
    }
    // Jars and packs might be on Modrinth; configs and screenshots never are.
    let downloadable = |path: &str| path.starts_with("mods/") || path.starts_with("resourcepacks/") || path.starts_with("shaderpacks/");
    let unknown: Vec<String> = [&manifest.files, &manifest.packs]
        .into_iter()
        .flatten()
        .flat_map(|files| files.iter())
        .filter(|(path, file)| downloadable(path) && !known.contains_key(&file.hash))
        .map(|(_, file)| file.hash.clone())
        .collect();
    let mut resolved = true;
    if !unknown.is_empty() {
        match fetch.known(&unknown) {
            Some(found) => {
                for hash in unknown {
                    let url = found.get(&hash).cloned();
                    known.insert(hash, url);
                }
            }
            None => resolved = false,
        }
    }
    for files in [&mut manifest.files, &mut manifest.packs].into_iter().flatten() {
        for (path, file) in files.iter_mut() {
            if downloadable(path) {
                file.url = known.get(&file.hash).cloned().flatten();
            }
        }
    }
    Ok((manifest, resolved))
}

fn section<T: Clone>(base: &Option<T>, ours: &Option<T>, theirs: &Option<T>, empty: T, merge: impl Fn(&T, &T, &T) -> T) -> Option<T> {
    match (ours, theirs) {
        (None, None) => None,
        (None, Some(theirs)) => Some(theirs.clone()),
        (Some(ours), None) => Some(ours.clone()),
        (Some(ours), Some(theirs)) => Some(merge(base.as_ref().unwrap_or(&empty), ours, theirs)),
    }
}

/// Merges two versions of an instance against the version they both came from. Where both
/// changed the same thing, ours wins if `prefer_ours`.
pub fn merge(base: &InstanceManifest, ours: &InstanceManifest, theirs: &InstanceManifest, prefer_ours: bool) -> InstanceManifest {
    let maps = |b: &BTreeMap<String, _>, o: &BTreeMap<String, _>, t: &BTreeMap<String, _>| merge_maps(b, o, t, prefer_ours);
    let mut merged = InstanceManifest {
        definition: section(&base.definition, &ours.definition, &theirs.definition, BTreeMap::new(), |b, o, t| merge_maps(b, o, t, prefer_ours)),
        content: section(&base.content, &ours.content, &theirs.content, BTreeMap::new(), |b, o, t| merge_maps(b, o, t, prefer_ours)),
        files: section(&base.files, &ours.files, &theirs.files, Files::new(), maps),
        packs: section(&base.packs, &ours.packs, &theirs.packs, Files::new(), maps),
        screenshots: section(&base.screenshots, &ours.screenshots, &theirs.screenshots, Files::new(), maps),
        options: section(&base.options, &ours.options, &theirs.options, BTreeMap::new(), |b, o, t| merge_maps(b, o, t, prefer_ours)),
        servers: section(&base.servers, &ours.servers, &theirs.servers, Vec::new(), |b, o, t| merge_servers(b, o, t, prefer_ours)),
    };
    // A merged-in file keeps whichever side's download link it has.
    for (merged_files, sources) in [
        (&mut merged.files, [&ours.files, &theirs.files]),
        (&mut merged.packs, [&ours.packs, &theirs.packs]),
    ] {
        if let Some(files) = merged_files {
            for (path, file) in files.iter_mut() {
                if file.url.is_none() {
                    file.url = sources.iter().filter_map(|source| source.as_ref()?.get(path)).find(|other| other.hash == file.hash).and_then(|other| other.url.clone());
                }
            }
        }
    }
    // Content records only for files that are still there.
    if let (Some(content), Some(files)) = (&mut merged.content, &merged.files) {
        content.retain(|file, item| item.world.is_some() || files.contains_key(file));
    }
    merged
}

/// What applying a manifest did.
#[derive(Debug, Default)]
pub struct Applied {
    /// Files that couldn't be fetched yet (still arriving in the sync folder, or offline).
    pub waiting: u32,
    pub bytes: u64,
}

/// Makes the instance on this computer match `wanted`, in the sections this computer syncs.
/// `current` is what it has now (from `local_manifest`).
#[allow(clippy::too_many_arguments)]
pub fn apply(
    store: &InstanceStore,
    instance_id: &str,
    current: &InstanceManifest,
    wanted: &InstanceManifest,
    remote: &Remote,
    fetch: &dyn Fetch,
    progress: &dyn Fn(u64, u64),
) -> Result<Applied, SyncError> {
    let dir = store.dir_of(instance_id).ok_or_else(|| SyncError::Other("that instance is gone".into()))?;
    let game_dir = dir.join(GAME_DIR);
    let mut applied = Applied::default();

    if let (Some(now), Some(want)) = (&current.definition, &wanted.definition)
        && now != want
    {
        let want = want.clone();
        store
            .update(instance_id, |instance| {
                let mut value = serde_json::to_value(&*instance).unwrap_or_default();
                if let Some(object) = value.as_object_mut() {
                    for (key, field) in &want {
                        object.insert(key.clone(), field.clone());
                    }
                }
                if let Ok(updated) = serde_json::from_value::<Instance>(value) {
                    *instance = updated;
                }
            })
            .map_err(|error| SyncError::Other(error.to_string()))?;
    }

    // Files: fetch what's different, remove what went away.
    let mut fetches: Vec<(PathBuf, &SyncedFile)> = Vec::new();
    let mut removals: Vec<PathBuf> = Vec::new();
    for ((_, now), (_, want)) in current.file_sections().into_iter().zip(wanted.file_sections()) {
        let (Some(now), Some(want)) = (now, want) else { continue };
        for (path, file) in want {
            if now.get(path).is_some_and(|have| have.hash == file.hash) {
                continue;
            }
            if let Some(target) = safe_join(&game_dir, path) {
                fetches.push((target, file));
            }
        }
        for path in now.keys().filter(|path| !want.contains_key(*path)) {
            if let Some(target) = safe_join(&game_dir, path) {
                removals.push(target);
            }
        }
    }
    let total: u64 = fetches.iter().map(|(_, file)| file.size).sum();
    progress(0, total);
    for (target, file) in fetches {
        let fetched = match &file.url {
            Some(url) => fetch.download(url, &file.hash, &target).is_ok() || remote.extract_to(&file.hash, &target)?,
            None => remote.extract_to(&file.hash, &target)?,
        };
        if fetched {
            applied.bytes += file.size;
            progress(applied.bytes, total);
        } else {
            applied.waiting += 1;
        }
    }
    for target in removals {
        match std::fs::remove_file(&target) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }

    if let (Some(now), Some(want)) = (&current.content, &wanted.content)
        && now != want
    {
        ContentIndex::new(&dir).save(&want.values().cloned().collect::<Vec<_>>())?;
    }
    if let (Some(now), Some(want)) = (&current.options, &wanted.options)
        && now != want
    {
        let path = game_dir.join("options.txt");
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        std::fs::create_dir_all(&game_dir)?;
        std::fs::write(&path, apply_options(&text, want))?;
    }
    if let (Some(now), Some(want)) = (&current.servers, &wanted.servers)
        && now != want
    {
        servers::set_listed(&game_dir, want).map_err(|error| SyncError::Other(error.to_string()))?;
    }
    Ok(applied)
}

/// Sends an instance version: the files it needs that the sync folder hasn't got (and that
/// can't be downloaded from Modrinth), then the manifest and the commit.
#[allow(clippy::too_many_arguments)]
pub fn upload(remote: &Remote, game_dir: &Path, key: &str, sync_id: &str, manifest: &InstanceManifest, parents: Vec<String>, who: &Who, progress: &dyn Fn(u64, u64)) -> Result<Commit, SyncError> {
    let mut needed: Vec<(&String, &SyncedFile)> = Vec::new();
    for (_, files) in manifest.file_sections() {
        for (path, file) in files.into_iter().flatten() {
            if file.url.is_none() && !remote.has(&file.hash) && !needed.iter().any(|(_, other)| other.hash == file.hash) {
                needed.push((path, file));
            }
        }
    }
    let total: u64 = needed.iter().map(|(_, file)| file.size).sum();
    let mut done = 0;
    progress(0, total);
    for (path, file) in needed {
        let Some(source) = safe_join(game_dir, path) else { continue };
        // The file may have changed since it was hashed; only send it if it's still the same.
        if crate::snapshots::hash_file(&source).ok().as_deref() != Some(file.hash.as_str()) {
            return Err(SyncError::Changing);
        }
        remote.put_file(&file.hash, &source)?;
        done += file.size;
        progress(done, total);
    }
    let object = remote.put_json(manifest)?;
    let name = manifest.name().unwrap_or("Instance").to_string();
    Ok(remote.write_commit(Commit {
        id: String::new(),
        item: key.to_string(),
        kind: ItemKind::Instance,
        parents,
        device: who.device.to_string(),
        device_name: who.device_name.to_string(),
        at: who.now,
        instance: sync_id.to_string(),
        world: None,
        name,
        manifest: object,
        content: manifest.content_hash(),
        files: manifest.file_sections().iter().filter_map(|(_, files)| *files).map(|files| files.len() as u32).sum(),
        bytes: manifest.total_bytes(),
        removed: false,
    })?)
}

/// Makes a new instance on this computer for one that came from another, from its definition.
pub fn create_from(store: &InstanceStore, sync_id: &str, manifest: &InstanceManifest, now: i64) -> Result<Instance, SyncError> {
    let definition = manifest.definition.as_ref().ok_or_else(|| SyncError::Other("the instance's details didn't sync".into()))?;
    let text = |key: &str| definition.get(key).and_then(|value| value.as_str()).map(String::from);
    let loader: Loader = definition.get("loader").cloned().and_then(|value| serde_json::from_value(value).ok()).unwrap_or(Loader::Vanilla);
    let modpack: Option<ModpackOrigin> = definition.get("modpack").cloned().and_then(|value| serde_json::from_value(value).ok());
    let request = NewInstance {
        name: text("name").unwrap_or_else(|| "Synced instance".into()),
        game_version: text("gameVersion").ok_or_else(|| SyncError::Other("the instance has no game version".into()))?,
        loader,
        loader_version: text("loaderVersion"),
        icon: text("icon"),
    };
    let loader_version = request.loader_version.clone();
    let instance = store.create_with(request, loader_version, modpack, now).map_err(|error| SyncError::Other(error.to_string()))?;
    let java_major = definition.get("javaMajor").and_then(|value| value.as_u64()).map(|major| major as u32);
    store
        .update(&instance.id, |instance| {
            instance.sync_id = Some(sync_id.to_string());
            instance.java_major = java_major;
        })
        .map_err(|error| SyncError::Other(error.to_string()))
}

/// Which file hashes a manifest needs from the sync folder (for tidying it).
pub fn stored_hashes(manifest: &InstanceManifest) -> Vec<String> {
    manifest.file_sections().iter().filter_map(|(_, files)| *files).flat_map(|files| files.values()).filter(|file| file.url.is_none()).map(|file| file.hash.clone()).collect()
}

/// A local instance that looks like the same one as a synced one: same name, version and loader.
pub fn looks_like(instance: &Instance, manifest: &InstanceManifest) -> bool {
    let Some(definition) = &manifest.definition else { return false };
    let mine = definition_of(instance);
    ["name", "gameVersion", "loader"].iter().all(|key| mine.get(*key) == definition.get(*key))
}

/// Download links by hash, gathered from manifests (so a link found by one computer is reused).
pub fn links(manifest: &InstanceManifest) -> HashMap<String, String> {
    manifest
        .file_sections()
        .iter()
        .filter_map(|(_, files)| *files)
        .flat_map(|files| files.values())
        .filter_map(|file| Some((file.hash.clone(), file.url.clone()?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(hash: &str, url: Option<&str>) -> SyncedFile {
        SyncedFile { hash: hash.into(), size: 1, url: url.map(String::from) }
    }

    #[test]
    fn sections_one_side_skips_are_carried_over() {
        let base = InstanceManifest { options: Some(BTreeMap::from([("fov".into(), "0.5".into())])), ..Default::default() };
        // This computer doesn't sync options; the cloud's stay as they are.
        let ours = InstanceManifest { files: Some(Files::from([("mods/a.jar".into(), file("aa", None))])), ..Default::default() };
        let theirs = InstanceManifest { options: Some(BTreeMap::from([("fov".into(), "0.8".into())])), files: Some(Files::new()), ..Default::default() };
        let merged = merge(&base, &ours, &theirs, true);
        assert_eq!(merged.options.unwrap()["fov"], "0.8");
        assert!(merged.files.unwrap().contains_key("mods/a.jar"));
    }

    #[test]
    fn files_merge_and_keep_download_links() {
        let base = InstanceManifest { files: Some(Files::from([("mods/old.jar".into(), file("o", None))])), ..Default::default() };
        let ours = InstanceManifest { files: Some(Files::from([("mods/old.jar".into(), file("o", None)), ("mods/mine.jar".into(), file("m", None))])), ..Default::default() };
        let theirs = InstanceManifest { files: Some(Files::from([("mods/theirs.jar".into(), file("t", Some("https://cdn.modrinth.com/t.jar")))])), ..Default::default() };
        let merged = merge(&base, &ours, &theirs, true).files.unwrap();
        // old.jar was removed over there, each side added one.
        assert_eq!(merged.keys().cloned().collect::<Vec<_>>(), vec!["mods/mine.jar".to_string(), "mods/theirs.jar".to_string()]);
        assert_eq!(merged["mods/theirs.jar"].url.as_deref(), Some("https://cdn.modrinth.com/t.jar"));
        // A link found by only one computer doesn't make the files differ.
        assert_eq!(file("x", None), file("x", Some("https://cdn.modrinth.com/x.jar")));
    }

    #[test]
    fn local_graphics_configs_and_junk_are_skipped() {
        assert!(skipped("config/sodium-options.json", "sodium-options.json", 10));
        assert!(skipped("config/.DS_Store", ".DS_Store", 10));
        assert!(skipped("mods/huge.jar", "huge.jar", MAX_FILE + 1));
        assert!(!skipped("config/jei/jei-client.ini", "jei-client.ini", 10));
    }
}
