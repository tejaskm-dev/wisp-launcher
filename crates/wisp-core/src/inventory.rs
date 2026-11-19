//! What's in an instance: its mods, resource packs, shader packs and the data packs in its
//! worlds, whoever put them there (a modpack, Wisp, or the player by hand). Each file is named by
//! what it is: Modrinth's name and icon when Modrinth knows the file (found by its SHA-1), else
//! what the file says about itself. Files are turned off and on the way launchers do it (a
//! `.disabled` suffix the game ignores), removed to a holding folder they can be put back from,
//! and checked for updates with one request per kind of file.
//!
//! What's been read is kept in `inventory.json` beside `instance.json`, keyed by size and time, so
//! after the first look a listing costs a walk of the folders and no network.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest, Sha1};

use crate::HttpClient;
use crate::content::{self, ContentIndex, InstallRequest, InstalledContent};
use crate::doctor;
use crate::instances::{GAME_DIR, Instance};
use crate::modrinth::{self, ProjectType};

const CACHE: &str = "inventory.json";
const CACHE_VERSION: u32 = 2;
/// Pictures taken from the files themselves (a resource pack's pack.png, a mod's logo).
const ICONS: &str = "inventory-icons";
/// Where removed files wait, beside instance.json, in case they're wanted back.
const REMOVED: &str = "removed";
const KEEP_REMOVED: usize = 40;
const DISABLED: &str = ".disabled";
/// The modpack's own files, recorded when it's installed: path in the game folder -> SHA-1.
pub const PACK_FILES: &str = "pack-files.json";
/// Files Modrinth didn't know are asked about again after this long: it may have them by now.
const RECHECK_MS: i64 = 7 * 24 * 60 * 60 * 1000;
const MAX_ICON: u64 = 512 * 1024;
const MAX_METADATA: u64 = 1024 * 1024;
/// Projects asked about per request (they go in the URL).
const PROJECTS_PER_REQUEST: usize = 100;
/// Ids every game answers to without a mod for them.
const BUILTIN: [&str; 11] = ["minecraft", "java", "fabricloader", "fabric-loader", "quilt_loader", "forge", "neoforge", "fml", "javafml", "lowcodefml", "mixinextras"];

/// The kinds of things an instance holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Mod,
    ResourcePack,
    Shader,
    DataPack,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Mod, Kind::ResourcePack, Kind::Shader, Kind::DataPack];

    fn project_type(self) -> ProjectType {
        match self {
            Kind::Mod => ProjectType::Mod,
            Kind::ResourcePack => ProjectType::Resourcepack,
            Kind::Shader => ProjectType::Shader,
            Kind::DataPack => ProjectType::Datapack,
        }
    }

    fn extension(self) -> &'static str {
        if self == Kind::Mod { ".jar" } else { ".zip" }
    }
}

/// Who put a file there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Origin {
    /// It came with the modpack the instance was made from.
    Pack,
    /// Wisp installed it (from Discover, or as something another install needed).
    Wisp,
    /// It was added by hand.
    Yours,
}

/// One mod, resource pack, shader pack or data pack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub kind: Kind,
    /// Where it is in the game folder now: "mods/sodium.jar", with `.disabled` while it's off.
    pub file: String,
    pub enabled: bool,
    /// A folder rather than a zip (packs can be either). Folders can't be turned off by
    /// renaming, so they're only listed.
    pub folder: bool,
    pub size: u64,
    pub name: String,
    pub version: String,
    pub description: String,
    /// Modrinth's picture of the project.
    pub icon_url: Option<String>,
    /// A picture taken from the file itself, as a path in the instance folder.
    pub icon_file: Option<String>,
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub origin: Origin,
    /// The world a data pack is in (its folder in saves/).
    pub world: Option<String>,
    /// Something other mods need rather than something picked for itself.
    pub library: bool,
    /// The mods that can't run without it.
    pub needed_by: Vec<String>,
    /// Empty for folders.
    pub sha1: String,
}

/// A newer build of something in an instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    pub file: String,
    pub project_id: String,
    pub version_id: String,
    pub version_number: String,
}

/// A file Modrinth serves, as a modpack export lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct Downloadable {
    /// In the game folder.
    pub path: String,
    pub url: String,
    pub sha1: String,
    pub sha512: String,
    pub size: u64,
}

/// A file taken out of an instance, waiting in case it's wanted back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Removed {
    pub id: String,
    /// Where it was in the game folder.
    pub file: String,
    pub at: i64,
    /// Wisp's record of it, put back with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content: Option<InstalledContent>,
}

#[derive(Debug, thiserror::Error)]
pub enum InventoryError {
    #[error("That isn't a file Wisp lists for this instance.")]
    NotListed,
    #[error("{0} isn't there any more.")]
    Missing(String),
    #[error("Something called {0} is already there.")]
    Taken(String),
    #[error("A folder can't be turned off. Remove it instead.")]
    Folder,
    #[error("couldn't reach Modrinth: {0}")]
    Modrinth(#[from] crate::Error),
    #[error(transparent)]
    Content(#[from] content::ContentError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

// ── Finding files ──

/// A file or folder in one of the folders Wisp lists.
#[derive(Debug, Clone)]
struct Found {
    kind: Kind,
    /// Relative to the game folder, as it is now.
    file: String,
    enabled: bool,
    folder: bool,
    size: u64,
    modified: i64,
    world: Option<String>,
}

fn modified_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_millis() as i64)
}

/// `file` without the `.disabled` that turns it off.
pub fn without_disabled(file: &str) -> &str {
    file.strip_suffix(DISABLED).unwrap_or(file)
}

fn walk(game_dir: &Path) -> Vec<Found> {
    let mut found = Vec::new();
    for (kind, folder) in [(Kind::Mod, "mods"), (Kind::ResourcePack, "resourcepacks"), (Kind::Shader, "shaderpacks")] {
        collect(game_dir, folder, kind, None, &mut found);
    }
    if let Ok(worlds) = std::fs::read_dir(game_dir.join("saves")) {
        for world in worlds.flatten() {
            let name = world.file_name().to_string_lossy().into_owned();
            if world.path().is_dir() && !name.starts_with('.') {
                collect(game_dir, &format!("saves/{name}/datapacks"), Kind::DataPack, Some(name), &mut found);
            }
        }
    }
    found
}

fn collect(game_dir: &Path, folder: &str, kind: Kind, world: Option<String>, found: &mut Vec<Found>) {
    let Ok(entries) = std::fs::read_dir(game_dir.join(folder)) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let (base, enabled) = match name.strip_suffix(DISABLED) {
            Some(base) => (base, false),
            None => (name.as_str(), true),
        };
        let listed = if meta.is_file() {
            base.to_ascii_lowercase().ends_with(kind.extension())
        } else {
            // Unzipped packs work as folders; mods never do.
            meta.is_dir() && enabled && kind != Kind::Mod
        };
        if !listed {
            continue;
        }
        found.push(Found {
            kind,
            file: format!("{folder}/{name}"),
            enabled,
            folder: meta.is_dir(),
            size: if meta.is_file() { meta.len() } else { 0 },
            modified: modified_ms(&meta),
            world: world.clone(),
        });
    }
}

/// Checks that `file` is something Wisp lists (and nothing outside the game folder), returning
/// its full path, kind and world.
fn checked(game_dir: &Path, file: &str) -> Result<(PathBuf, Kind, Option<String>), InventoryError> {
    let parts: Vec<&str> = file.split('/').collect();
    if parts.iter().any(|part| part.is_empty() || *part == "." || *part == ".." || part.contains('\\') || part.contains(':')) {
        return Err(InventoryError::NotListed);
    }
    let (kind, world) = match parts.as_slice() {
        ["mods", _] => (Kind::Mod, None),
        ["resourcepacks", _] => (Kind::ResourcePack, None),
        ["shaderpacks", _] => (Kind::Shader, None),
        ["saves", world, "datapacks", _] => (Kind::DataPack, Some(world.to_string())),
        _ => return Err(InventoryError::NotListed),
    };
    Ok((game_dir.join(file), kind, world))
}

// ── Reading files ──

/// What a file says about itself.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Local {
    name: String,
    version: String,
    description: String,
    /// Mods: the id the loader knows it by, every id it answers to, and the ids it needs.
    id: String,
    provides: Vec<String>,
    requires: Vec<String>,
    /// A picture from the file, as a path in the instance folder.
    icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedFile {
    size: u64,
    modified: i64,
    sha1: String,
    local: Local,
}

/// What Modrinth said about a file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Known {
    /// False when Modrinth was asked and didn't have it.
    found: bool,
    checked_at: i64,
    project_id: String,
    version_id: String,
    title: String,
    version_number: String,
    /// When that version came out (RFC 3339), so an older one is never offered as an update.
    published: String,
    description: String,
    icon_url: Option<String>,
    categories: Vec<String>,
    /// Where Modrinth serves the file and its SHA-512 (a modpack export names both).
    url: Option<String>,
    sha512: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Cache {
    #[serde(default)]
    version: u32,
    /// By path in the game folder.
    #[serde(default)]
    files: HashMap<String, CachedFile>,
    /// By SHA-1.
    #[serde(default)]
    known: HashMap<String, Known>,
}

pub(crate) fn sha1_of(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha1::new();
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Some(crate::launch::download::hex(&hasher.finalize()))
}

/// A file inside a zip or a folder, read whole when it's small enough.
fn inner_bytes(path: &Path, folder: bool, name: &str, limit: u64) -> Option<Vec<u8>> {
    if folder {
        let inner = path.join(name);
        let meta = std::fs::metadata(&inner).ok()?;
        return (meta.is_file() && meta.len() <= limit).then(|| std::fs::read(inner).ok()).flatten();
    }
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file)).ok()?;
    let entry = archive.by_name(name).ok()?;
    if entry.size() > limit {
        return None;
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.take(limit).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

fn inner_text(path: &Path, folder: bool, name: &str) -> Option<String> {
    inner_bytes(path, folder, name, MAX_METADATA).map(|bytes| String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}').to_string())
}

/// Minecraft's text: a plain string, or a component ({"text": ..., "extra": [...]}) or a list of
/// them. Formatting codes are dropped.
fn plain_text(value: &Value) -> String {
    fn walk(value: &Value, out: &mut String) {
        match value {
            Value::String(text) => out.push_str(text),
            Value::Array(items) => items.iter().for_each(|item| walk(item, out)),
            Value::Object(fields) => {
                // A translation key means nothing on its own; its fallback (1.19.4 and later) does.
                if let Some(text) = fields.get("text").or_else(|| fields.get("fallback")) {
                    walk(text, out);
                }
                if let Some(extra) = fields.get("extra") {
                    walk(extra, out);
                }
            }
            _ => {}
        }
    }
    let mut out = String::new();
    walk(value, &mut out);
    tidy(&crate::servers::strip_codes(&out))
}

/// One line, trimmed: mod descriptions are often wrapped by hand.
fn tidy(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x89, b'P', b'N', b'G'])
}

/// Keeps a picture from a file in the instance folder, named by `key`; returns its path there.
fn keep_icon(instance_dir: &Path, key: &str, bytes: &[u8]) -> Option<String> {
    if !is_png(bytes) || key.is_empty() {
        return None;
    }
    let relative = format!("{ICONS}/{key}.png");
    let path = instance_dir.join(&relative);
    if !path.is_file() {
        std::fs::create_dir_all(path.parent()?).ok()?;
        std::fs::write(&path, bytes).ok()?;
    }
    Some(relative)
}

/// What a mod jar says beyond its ids: its description and the path of its logo.
fn mod_extras(path: &Path) -> (String, Option<String>) {
    if let Some(text) = inner_text(path, false, "fabric.mod.json")
        && let Ok(value) = serde_json::from_str::<Value>(&text)
    {
        let icon = match value.get("icon") {
            Some(Value::String(icon)) => Some(icon.clone()),
            // Several sizes: the biggest.
            Some(Value::Object(sizes)) => sizes.iter().filter_map(|(size, icon)| Some((size.parse::<u32>().ok()?, icon.as_str()?))).max_by_key(|(size, _)| *size).map(|(_, icon)| icon.to_string()),
            _ => None,
        };
        return (tidy(value.get("description").and_then(Value::as_str).unwrap_or_default()), icon);
    }
    if let Some(text) = inner_text(path, false, "quilt.mod.json")
        && let Ok(value) = serde_json::from_str::<Value>(&text)
    {
        let metadata = value.get("quilt_loader").and_then(|loader| loader.get("metadata"));
        let field = |key: &str| metadata.and_then(|metadata| metadata.get(key)).and_then(Value::as_str).map(str::to_string);
        return (tidy(&field("description").unwrap_or_default()), field("icon"));
    }
    for name in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        if let Some(text) = inner_text(path, false, name)
            && let Ok(table) = toml::from_str::<toml::Table>(&text)
        {
            let first = table.get("mods").and_then(toml::Value::as_array).and_then(|mods| mods.first());
            let field = |key: &str| first.and_then(|entry| entry.get(key)).and_then(toml::Value::as_str).filter(|value| !value.starts_with("${")).map(str::to_string);
            let logo = field("logoFile").or_else(|| table.get("logoFile").and_then(toml::Value::as_str).map(str::to_string));
            return (tidy(&field("description").unwrap_or_default()), logo);
        }
    }
    (String::new(), None)
}

/// A file's name without its extension or `.disabled`: what to call it when nothing better is known.
fn plain_name(file: &str) -> String {
    let name = without_disabled(file).rsplit('/').next().unwrap_or(file);
    let stem = name.strip_suffix(".jar").or_else(|| name.strip_suffix(".zip")).unwrap_or(name);
    stem.to_string()
}

fn read_local(instance_dir: &Path, found: &Found, sha1: &str) -> Local {
    let path = instance_dir.join(GAME_DIR).join(&found.file);
    // Folders have no hash; their pictures go by their path.
    let key = if sha1.is_empty() { crate::launch::download::hex(&Sha1::digest(found.file.as_bytes())) } else { sha1.to_string() };
    let mut local = Local { name: plain_name(&found.file), ..Default::default() };
    match found.kind {
        Kind::Mod => {
            let info = doctor::read_jar(&path);
            if !info.name.is_empty() {
                local.name = info.name.clone();
            }
            local.version = info.version.clone();
            local.id = info.id.clone();
            local.provides = info.provides.clone();
            local.requires = info.requires.iter().map(|requirement| requirement.id.clone()).filter(|id| !BUILTIN.contains(&id.as_str())).collect::<BTreeSet<_>>().into_iter().collect();
            let (description, logo) = mod_extras(&path);
            local.description = description;
            local.icon = logo.and_then(|logo| inner_bytes(&path, false, logo.trim_start_matches('/'), MAX_ICON)).and_then(|bytes| keep_icon(instance_dir, &key, &bytes));
        }
        Kind::ResourcePack | Kind::DataPack => {
            if let Some(text) = inner_text(&path, found.folder, "pack.mcmeta")
                && let Ok(value) = serde_json::from_str::<Value>(&text)
                && let Some(description) = value.get("pack").and_then(|pack| pack.get("description"))
            {
                local.description = plain_text(description);
            }
            local.icon = inner_bytes(&path, found.folder, "pack.png", MAX_ICON).and_then(|bytes| keep_icon(instance_dir, &key, &bytes));
        }
        Kind::Shader => {}
    }
    local
}

// ── Listing ──

/// An instance's files, read and hashed, ready to be named.
pub struct Scan {
    found: Vec<(Found, CachedFile)>,
    cache: Cache,
    changed: bool,
}

/// Reads what's new in an instance's folders since the last look (hashing and reading files, on
/// several threads for a modpack's first look). Blocking: run it off the main thread.
pub fn scan(instance_dir: &Path) -> Scan {
    let game_dir = instance_dir.join(GAME_DIR);
    let mut cache: Cache = std::fs::read(instance_dir.join(CACHE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Cache>(&bytes).ok())
        .filter(|cache| cache.version == CACHE_VERSION)
        .unwrap_or_default();
    cache.version = CACHE_VERSION;
    let mut ready: Vec<(Found, CachedFile)> = Vec::new();
    let mut unread: Vec<Found> = Vec::new();
    for found in walk(&game_dir) {
        match cache.files.get(&found.file) {
            Some(cached) if cached.size == found.size && cached.modified == found.modified => ready.push((found, cached.clone())),
            _ => unread.push(found),
        }
    }
    let mut changed = !unread.is_empty() || ready.len() != cache.files.len();
    let game_dir: &Path = &game_dir;
    if !unread.is_empty() {
        let workers = std::thread::available_parallelism().map_or(4, |count| count.get()).clamp(1, 8);
        let chunk = unread.len().div_ceil(workers).max(1);
        let read: Vec<(Found, CachedFile)> = std::thread::scope(|scope| {
            let handles: Vec<_> = unread
                .chunks(chunk)
                .map(|part| {
                    scope.spawn(move || {
                        part.iter()
                            .map(|found| {
                                let sha1 = if found.folder { String::new() } else { sha1_of(&game_dir.join(&found.file)).unwrap_or_default() };
                                let local = read_local(instance_dir, found, &sha1);
                                (found.clone(), CachedFile { size: found.size, modified: found.modified, sha1, local })
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles.into_iter().flat_map(|handle| handle.join().unwrap_or_default()).collect()
        });
        ready.extend(read);
    }
    cache.files = ready.iter().map(|(found, cached)| (found.file.clone(), cached.clone())).collect();
    // Forget what Modrinth said about files that are gone.
    let hashes: HashSet<&str> = ready.iter().map(|(_, cached)| cached.sha1.as_str()).collect();
    let before = cache.known.len();
    cache.known.retain(|hash, _| hashes.contains(hash.as_str()));
    changed |= cache.known.len() != before;
    ready.sort_by(|(a, _), (b, _)| (a.kind, &a.file).cmp(&(b.kind, &b.file)));
    Scan { found: ready, cache, changed }
}

impl Scan {
    /// Asks Modrinth about files it hasn't been asked about (two requests, however many files).
    /// Without a connection, files keep the names they give themselves.
    pub async fn identify(&mut self, client: &HttpClient, now: i64) {
        let unknown: Vec<String> = self
            .found
            .iter()
            .map(|(_, cached)| cached.sha1.clone())
            .filter(|hash| !hash.is_empty())
            .filter(|hash| match self.cache.known.get(hash) {
                Some(known) => !known.found && now - known.checked_at > RECHECK_MS,
                None => true,
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if unknown.is_empty() {
            return;
        }
        let Ok(versions) = modrinth::versions_by_hashes(client, &unknown).await else {
            return;
        };
        let ids: Vec<String> = versions.values().map(|version| version.project_id.clone()).collect::<BTreeSet<_>>().into_iter().collect();
        let mut projects: HashMap<String, modrinth::ProjectSummary> = HashMap::new();
        for chunk in ids.chunks(PROJECTS_PER_REQUEST) {
            match modrinth::projects(client, chunk).await {
                Ok(found) => projects.extend(found.into_iter().map(|project| (project.id.clone(), project))),
                // Ask again next time rather than keep half an answer.
                Err(_) => return,
            }
        }
        for hash in unknown {
            let known = match versions.get(&hash) {
                Some(version) => {
                    let project = projects.get(&version.project_id);
                    let file = version.files.iter().find(|file| file.sha1.as_deref() == Some(hash.as_str()));
                    Known {
                        found: true,
                        checked_at: now,
                        project_id: version.project_id.clone(),
                        version_id: version.id.clone(),
                        title: project.map_or_else(|| version.name.clone(), |project| project.title.clone()),
                        version_number: version.version_number.clone(),
                        published: version.date_published.clone(),
                        description: project.map(|project| tidy(&project.description)).unwrap_or_default(),
                        icon_url: project.and_then(|project| project.icon_url.clone()),
                        categories: project.map(|project| project.categories.clone()).unwrap_or_default(),
                        url: file.map(|file| file.url.clone()),
                        sha512: file.and_then(|file| file.sha512.clone()),
                    }
                }
                None => Known { found: false, checked_at: now, ..Default::default() },
            };
            self.cache.known.insert(hash, known);
        }
        self.changed = true;
    }

    /// Keeps what was read for next time.
    pub fn save(&self, instance_dir: &Path) {
        if self.changed
            && let Ok(bytes) = serde_json::to_vec(&self.cache)
        {
            let partial = instance_dir.join(format!("{CACHE}.part"));
            if std::fs::write(&partial, bytes).is_ok() {
                let _ = std::fs::rename(partial, instance_dir.join(CACHE));
            }
        }
    }

    fn known(&self, sha1: &str) -> Option<&Known> {
        self.cache.known.get(sha1).filter(|known| known.found)
    }

    /// Everything in the instance, named. `pack_files` are the modpack's own files (see
    /// `pack_files`); None when they aren't known, and then whatever Wisp didn't install in a
    /// modpack instance counts as the pack's.
    pub fn items(&self, instance: &Instance, instance_dir: &Path, pack_files: Option<&HashMap<String, String>>) -> Vec<Item> {
        let index: HashMap<String, InstalledContent> = ContentIndex::new(instance_dir).list().into_iter().map(|item| (without_disabled(&item.file).to_string(), item)).collect();
        let mut items: Vec<Item> = self
            .found
            .iter()
            .map(|(found, cached)| {
                let base = without_disabled(&found.file);
                let known = self.known(&cached.sha1);
                let installed = index.get(base);
                let origin = if installed.is_some() {
                    Origin::Wisp
                } else if pack_files.map_or(instance.modpack.is_some(), |files| files.contains_key(base)) {
                    Origin::Pack
                } else {
                    Origin::Yours
                };
                let library = found.kind == Kind::Mod
                    && (installed.is_some_and(|installed| !installed.explicit) || known.is_some_and(|known| known.categories.iter().any(|category| category == "library")));
                Item {
                    kind: found.kind,
                    file: found.file.clone(),
                    enabled: found.enabled,
                    folder: found.folder,
                    size: found.size,
                    name: known.map_or_else(|| cached.local.name.clone(), |known| known.title.clone()),
                    version: known.map_or_else(|| cached.local.version.clone(), |known| known.version_number.clone()),
                    description: known.map(|known| known.description.clone()).filter(|text| !text.is_empty()).unwrap_or_else(|| cached.local.description.clone()),
                    icon_url: known.and_then(|known| known.icon_url.clone()).or_else(|| installed.and_then(|installed| installed.icon_url.clone())),
                    icon_file: cached.local.icon.clone(),
                    project_id: known.map(|known| known.project_id.clone()).or_else(|| installed.map(|installed| installed.project_id.clone())),
                    version_id: known.map(|known| known.version_id.clone()).or_else(|| installed.map(|installed| installed.version_id.clone())),
                    origin,
                    world: found.world.clone(),
                    library,
                    needed_by: Vec::new(),
                    sha1: cached.sha1.clone(),
                }
            })
            .collect();

        // Which mods need which: the ids each turned-on mod needs, matched to whatever answers to them.
        let mods: Vec<(usize, &CachedFile)> = self.found.iter().enumerate().filter(|(_, (found, _))| found.kind == Kind::Mod && found.enabled).map(|(index, (_, cached))| (index, cached)).collect();
        let mut providers: HashMap<&str, usize> = HashMap::new();
        for (index, cached) in &mods {
            for id in cached.local.provides.iter().chain(std::iter::once(&cached.local.id)).filter(|id| !id.is_empty()) {
                providers.entry(id.as_str()).or_insert(*index);
            }
        }
        let mut needed: HashMap<usize, BTreeSet<String>> = HashMap::new();
        for (index, cached) in &mods {
            for id in &cached.local.requires {
                if let Some(&provider) = providers.get(id.as_str())
                    && provider != *index
                {
                    needed.entry(provider).or_default().insert(items[*index].name.clone());
                }
            }
        }
        for (index, names) in needed {
            items[index].needed_by = names.into_iter().collect();
        }
        items
    }

    /// Newer builds of what's in the instance, for its loader and Minecraft version: one request
    /// per kind of file. Only builds newer than the one there are offered.
    pub async fn updates(&self, client: &HttpClient, instance: &Instance) -> Result<Vec<Update>, InventoryError> {
        let mut updates = Vec::new();
        for kind in Kind::ALL {
            let Some(loaders) = content::compatible_loaders(instance.loader, kind.project_type()) else {
                continue;
            };
            let files: Vec<(&Found, &Known, &str)> = self
                .found
                .iter()
                .filter(|(found, _)| found.kind == kind && !found.folder)
                .filter_map(|(found, cached)| Some((found, self.known(&cached.sha1)?, cached.sha1.as_str())))
                .collect();
            let hashes: Vec<String> = files.iter().map(|(_, _, hash)| hash.to_string()).collect::<BTreeSet<_>>().into_iter().collect();
            if hashes.is_empty() {
                continue;
            }
            let latest = modrinth::latest_by_hashes(client, &hashes, &loaders, &[instance.game_version.as_str()]).await?;
            for (found, known, hash) in files {
                if let Some(version) = latest.get(hash)
                    && version.id != known.version_id
                    && version.project_id == known.project_id
                    && version.date_published.as_str() > known.published.as_str()
                {
                    updates.push(Update { file: found.file.clone(), project_id: version.project_id.clone(), version_id: version.id.clone(), version_number: version.version_number.clone() });
                }
            }
        }
        Ok(updates)
    }

    /// Files Modrinth has, with where it serves them and their hashes, and the paths of files it
    /// doesn't: what a modpack export needs. Files that are turned off are left out.
    pub fn for_export(&self) -> (Vec<Downloadable>, Vec<String>) {
        let mut downloadable = Vec::new();
        let mut own = Vec::new();
        for (found, cached) in &self.found {
            if !found.enabled || found.folder {
                if found.enabled {
                    own.push(found.file.clone());
                }
                continue;
            }
            match self.known(&cached.sha1).and_then(|known| Some((known.url.clone()?, known.sha512.clone()?))) {
                Some((url, sha512)) => downloadable.push(Downloadable { path: found.file.clone(), url, sha1: cached.sha1.clone(), sha512, size: found.size }),
                None => own.push(found.file.clone()),
            }
        }
        (downloadable, own)
    }
}

/// A turned-on file that can be handed to someone else: where it goes in the game folder, its
/// SHA-1 and size, and where Modrinth serves it when Modrinth has it.
#[derive(Debug, Clone, PartialEq)]
pub struct Shareable {
    pub path: String,
    pub sha1: String,
    pub size: u64,
    pub url: Option<String>,
    pub project_id: Option<String>,
}

impl Scan {
    /// Every turned-on file outside the worlds (mods, resource packs, shader packs), for sharing an
    /// instance with someone.
    pub fn shareable(&self) -> Vec<Shareable> {
        self.found
            .iter()
            .filter(|(found, cached)| found.enabled && !found.folder && found.world.is_none() && !cached.sha1.is_empty())
            .map(|(found, cached)| {
                let known = self.known(&cached.sha1);
                Shareable {
                    path: found.file.clone(),
                    sha1: cached.sha1.clone(),
                    size: found.size,
                    url: known.and_then(|known| known.url.clone()),
                    project_id: known.map(|known| known.project_id.clone()).filter(|id| !id.is_empty()),
                }
            })
            .collect()
    }
}

// ── Moving to another version ──

/// A mod in a version move.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveItem {
    pub file: String,
    pub name: String,
    /// The build it moves to, for mods that get one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_number: Option<String>,
}

/// What moving an instance to another Minecraft version or loader would do to its mods (the
/// ones turned on; resource and shader packs carry over as they are).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MovePlan {
    /// Swapped for their build for the new version.
    pub updates: Vec<MoveItem>,
    /// Already the right build.
    pub unchanged: Vec<MoveItem>,
    /// No build for the new version: turned off (and back on by moving back).
    pub without_build: Vec<MoveItem>,
    /// Not on Modrinth, so Wisp can't tell: left as they are.
    pub unknown: Vec<MoveItem>,
}

/// What a move did.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Moved {
    pub updated: Vec<String>,
    pub turned_off: Vec<String>,
    /// Mods that couldn't be swapped, with why.
    pub failed: Vec<(String, String)>,
}

impl Scan {
    /// Works out what moving to `game_version` with `loader` does to the mods in `items` (this
    /// scan's items): one request to Modrinth.
    pub async fn plan_move(&self, client: &HttpClient, items: &[Item], game_version: &str, loader: crate::instances::Loader) -> Result<MovePlan, InventoryError> {
        let mods: Vec<&Item> = items.iter().filter(|item| item.kind == Kind::Mod && item.enabled).collect();
        let entry = |item: &Item, version: Option<&modrinth::ProjectVersion>| MoveItem {
            file: item.file.clone(),
            name: item.name.clone(),
            version_id: version.map(|version| version.id.clone()),
            version_number: version.map(|version| version.version_number.clone()),
        };
        let mut plan = MovePlan::default();
        let Some(loaders) = content::compatible_loaders(loader, ProjectType::Mod) else {
            // Plain Minecraft runs no mods.
            plan.without_build = mods.iter().map(|item| entry(item, None)).collect();
            return Ok(plan);
        };
        let hashes: Vec<String> = mods.iter().filter(|item| self.known(&item.sha1).is_some()).map(|item| item.sha1.clone()).collect::<BTreeSet<_>>().into_iter().collect();
        let latest = modrinth::latest_by_hashes(client, &hashes, &loaders, &[game_version]).await?;
        for item in mods {
            let Some(known) = self.known(&item.sha1) else {
                plan.unknown.push(entry(item, None));
                continue;
            };
            match latest.get(&item.sha1) {
                Some(version) if version.id == known.version_id => plan.unchanged.push(entry(item, Some(version))),
                Some(version) => plan.updates.push(entry(item, Some(version))),
                None => plan.without_build.push(entry(item, None)),
            }
        }
        Ok(plan)
    }
}

/// Carries out a move plan on an instance already set to its new version: swaps in the new
/// builds (old files wait in the holding folder) and turns off what has none.
#[allow(clippy::too_many_arguments)]
pub async fn apply_move(client: &HttpClient, instance: &Instance, instance_dir: &Path, plan: &MovePlan, concurrency: usize, curseforge_key: Option<&str>, now: i64) -> Moved {
    let mut moved = Moved::default();
    for item in &plan.updates {
        let Some(version_id) = &item.version_id else { continue };
        match install_version(client, instance, instance_dir, &item.file, version_id, concurrency, curseforge_key, now).await {
            Ok(_) => moved.updated.push(item.name.clone()),
            Err(error) => moved.failed.push((item.name.clone(), error.to_string())),
        }
    }
    for item in &plan.without_build {
        match set_enabled(instance_dir, &item.file, false) {
            Ok(_) => moved.turned_off.push(item.name.clone()),
            Err(error) => moved.failed.push((item.name.clone(), error.to_string())),
        }
    }
    moved
}

// ── The modpack's own files ──

/// Records the files a modpack brought (path in the game folder -> SHA-1, empty when unknown).
pub fn record_pack_files(instance_dir: &Path, files: &HashMap<String, String>) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(files).map_err(std::io::Error::other)?;
    std::fs::write(instance_dir.join(PACK_FILES), bytes)
}

/// The modpack's own files: recorded when it was installed, or read from its `.mrpack` (and then
/// recorded). None when neither is at hand.
pub fn pack_files(instance_dir: &Path, mrpack: Option<&Path>) -> Option<HashMap<String, String>> {
    if let Ok(bytes) = std::fs::read(instance_dir.join(PACK_FILES))
        && let Ok(files) = serde_json::from_slice(&bytes)
    {
        return Some(files);
    }
    let files = mrpack_files(mrpack?)?;
    let _ = record_pack_files(instance_dir, &files);
    Some(files)
}

/// The files a `.mrpack` puts in the game folder: its downloads and its overrides.
pub fn mrpack_files(mrpack: &Path) -> Option<HashMap<String, String>> {
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(std::fs::File::open(mrpack).ok()?)).ok()?;
    let mut files = HashMap::new();
    let mut text = String::new();
    archive.by_name("modrinth.index.json").ok()?.read_to_string(&mut text).ok()?;
    let index: Value = serde_json::from_str(&text).ok()?;
    for file in index.get("files").and_then(Value::as_array).into_iter().flatten() {
        if let Some(path) = file.get("path").and_then(Value::as_str) {
            let sha1 = file.get("hashes").and_then(|hashes| hashes.get("sha1")).and_then(Value::as_str).unwrap_or_default();
            files.insert(path.to_string(), sha1.to_string());
        }
    }
    for name in archive.file_names().filter_map(Result::ok) {
        for prefix in ["overrides/", "client-overrides/"] {
            if let Some(path) = name.strip_prefix(prefix)
                && !path.is_empty()
                && !path.ends_with('/')
            {
                files.entry(path.to_string()).or_default();
            }
        }
    }
    Some(files)
}

// ── Changing things ──

/// Turns a file on or off by renaming it (`.disabled`), keeping Wisp's record pointing at it.
/// Returns where it is now.
pub fn set_enabled(instance_dir: &Path, file: &str, enabled: bool) -> Result<String, InventoryError> {
    let game_dir = instance_dir.join(GAME_DIR);
    let (path, _, _) = checked(&game_dir, file)?;
    if !path.exists() {
        return Err(InventoryError::Missing(plain_name(file)));
    }
    if path.is_dir() {
        return Err(InventoryError::Folder);
    }
    let base = without_disabled(file);
    let target = if enabled { base.to_string() } else { format!("{base}{DISABLED}") };
    if target == file {
        return Ok(target);
    }
    let target_path = game_dir.join(&target);
    if target_path.exists() {
        return Err(InventoryError::Taken(plain_name(&target)));
    }
    std::fs::rename(&path, &target_path)?;
    let index = ContentIndex::new(instance_dir);
    let mut items = index.list();
    if let Some(item) = items.iter_mut().find(|item| without_disabled(&item.file) == base) {
        item.file = target.clone();
        index.save(&items)?;
    }
    Ok(target)
}

fn removed_dir(instance_dir: &Path, id: &str) -> Option<PathBuf> {
    (!id.is_empty() && id.chars().all(|character| character.is_ascii_alphanumeric())).then(|| instance_dir.join(REMOVED).join(id))
}

/// Takes a file out of the instance into a holding folder it can be put back from (see
/// `put_back`), and out of Wisp's record of what's installed.
pub fn remove(instance_dir: &Path, file: &str, now: i64) -> Result<Removed, InventoryError> {
    let game_dir = instance_dir.join(GAME_DIR);
    let (path, _, _) = checked(&game_dir, file)?;
    if !path.exists() {
        return Err(InventoryError::Missing(plain_name(file)));
    }
    let id = crate::journal::new_id(now);
    let holding = removed_dir(instance_dir, &id).ok_or(InventoryError::NotListed)?;
    std::fs::create_dir_all(&holding)?;
    let name = path.file_name().ok_or(InventoryError::NotListed)?.to_owned();
    std::fs::rename(&path, holding.join(&name))?;
    let index = ContentIndex::new(instance_dir);
    let mut items = index.list();
    let base = without_disabled(file);
    let content = items.iter().position(|item| without_disabled(&item.file) == base).map(|position| items.remove(position));
    if content.is_some() {
        index.save(&items)?;
    }
    let record = Removed { id, file: file.to_string(), at: now, content };
    std::fs::write(holding.join("record.json"), serde_json::to_vec(&record).map_err(std::io::Error::other)?)?;
    prune_removed(instance_dir);
    Ok(record)
}

/// Puts a removed file back where it was. Returns its path in the game folder.
pub fn put_back(instance_dir: &Path, id: &str) -> Result<String, InventoryError> {
    let holding = removed_dir(instance_dir, id).ok_or(InventoryError::NotListed)?;
    let record: Removed = std::fs::read(holding.join("record.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or_else(|| InventoryError::Missing("That file".into()))?;
    let game_dir = instance_dir.join(GAME_DIR);
    let (target, _, _) = checked(&game_dir, &record.file)?;
    if target.exists() || game_dir.join(without_disabled(&record.file)).exists() {
        return Err(InventoryError::Taken(plain_name(&record.file)));
    }
    let name = target.file_name().ok_or(InventoryError::NotListed)?.to_owned();
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(holding.join(&name), &target)?;
    if let Some(content) = record.content {
        let index = ContentIndex::new(instance_dir);
        let mut items = index.list();
        if !items.iter().any(|item| item.project_id == content.project_id && item.world == content.world) {
            items.push(content);
            index.save(&items)?;
        }
    }
    let _ = std::fs::remove_dir_all(&holding);
    Ok(record.file)
}

/// Files removed most recently, newest first.
pub fn removed(instance_dir: &Path) -> Vec<Removed> {
    let mut records: Vec<Removed> = std::fs::read_dir(instance_dir.join(REMOVED))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| serde_json::from_slice(&std::fs::read(entry.path().join("record.json")).ok()?).ok())
        .collect();
    records.sort_by_key(|record: &Removed| std::cmp::Reverse(record.at));
    records
}

fn prune_removed(instance_dir: &Path) {
    for old in removed(instance_dir).into_iter().skip(KEEP_REMOVED) {
        if let Some(dir) = removed_dir(instance_dir, &old.id) {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

#[allow(clippy::too_many_arguments)]
/// Puts a specific build of something in place of the file there now (an update, or picking
/// another version). What the new build needs comes with it, and the old file waits in the
/// holding folder unless the install replaced it.
pub async fn install_version(
    client: &HttpClient,
    instance: &Instance,
    instance_dir: &Path,
    file: &str,
    version_id: &str,
    concurrency: usize,
    curseforge_key: Option<&str>,
    now: i64,
) -> Result<Vec<InstalledContent>, InventoryError> {
    let game_dir = instance_dir.join(GAME_DIR);
    let (path, _, world) = checked(&game_dir, file)?;
    let version = modrinth::version(client, version_id).await?;
    let was_off = file.ends_with(DISABLED);
    let request = InstallRequest { project_id: &version.project_id, version_id: Some(version_id), world: world.as_deref(), concurrency, curseforge_key };
    let installed = content::install(client, instance, instance_dir, request).await?;
    let replaced: HashSet<&str> = installed.iter().map(|item| without_disabled(&item.file)).collect();
    if path.exists() && !replaced.contains(without_disabled(file)) {
        remove(instance_dir, file, now)?;
    }
    // Off stays off.
    if was_off && let Some(main) = installed.iter().find(|item| item.project_id == version.project_id) {
        let _ = set_enabled(instance_dir, &main.file, false);
    }
    Ok(installed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            writer.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbaImage::from_pixel(4, 4, image::Rgba([229, 72, 77, 255])).write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        bytes
    }

    fn instance(modpack: bool) -> Instance {
        Instance {
            id: "test".into(),
            name: "Test".into(),
            game_version: "26.3".into(),
            loader: crate::instances::Loader::Fabric,
            loader_version: Some("0.20.1".into()),
            created: 0,
            last_played: None,
            icon: None,
            modpack: modpack.then(|| crate::instances::ModpackOrigin { project_id: "p".into(), version_id: "v".into(), title: "Pack".into(), version_number: "1".into() }),
            memory_mb: None,
            java_major: None,
            sync_id: None,
        }
    }

    fn temp_instance(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wisp-inventory-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for folder in ["mods", "resourcepacks", "shaderpacks", "saves/Home/datapacks"] {
            std::fs::create_dir_all(dir.join(GAME_DIR).join(folder)).unwrap();
        }
        dir
    }

    #[test]
    fn lists_and_names_everything_in_an_instance() {
        let dir = temp_instance("list");
        let game = dir.join(GAME_DIR);
        let api = zip_bytes(&[("fabric.mod.json", br#"{"id": "fabric-api", "name": "Fabric API", "version": "0.161.0", "provides": ["fabric"], "description": "Hooks\n   for mods"}"#)]);
        std::fs::write(game.join("mods/api.jar"), &api).unwrap();
        let icon = png();
        let menu = zip_bytes(&[
            ("fabric.mod.json", br#"{"id": "modmenu", "name": "Mod Menu", "version": "20.0.3", "icon": {"16": "small.png", "128": "assets/modmenu/icon.png"}, "depends": {"fabric": "*", "minecraft": "*"}}"#),
            ("assets/modmenu/icon.png", &icon),
        ]);
        std::fs::write(game.join("mods/menu.jar.disabled"), &menu).unwrap();
        std::fs::write(game.join("mods/notes.txt"), b"not a mod").unwrap();
        std::fs::write(game.join("resourcepacks/faithful.zip"), zip_bytes(&[("pack.mcmeta", br#"{"pack": {"pack_format": 64, "description": [{"text": "\u00a7aFaithful "}, {"text": "32x"}]}}"#), ("pack.png", &icon)])).unwrap();
        std::fs::create_dir_all(game.join("resourcepacks/Loose Pack")).unwrap();
        std::fs::write(game.join("resourcepacks/Loose Pack/pack.mcmeta"), br#"{"pack": {"description": "By hand"}}"#).unwrap();
        std::fs::write(game.join("shaderpacks/Complementary.zip"), zip_bytes(&[("shaders/final.fsh", b"")])).unwrap();
        std::fs::write(game.join("saves/Home/datapacks/trees.zip"), zip_bytes(&[("pack.mcmeta", br#"{"pack": {"description": "More trees"}}"#)])).unwrap();

        let scan = scan(&dir);
        let items = scan.items(&instance(false), &dir, None);
        let by_file = |file: &str| items.iter().find(|item| item.file == file).unwrap_or_else(|| panic!("{file} listed"));
        assert_eq!(items.len(), 6, "{items:#?}");

        let api = by_file("mods/api.jar");
        assert_eq!((api.name.as_str(), api.version.as_str(), api.description.as_str(), api.enabled), ("Fabric API", "0.161.0", "Hooks for mods", true));
        // Mod Menu needs "fabric", which Fabric API answers to; but Mod Menu is off, so nothing needs Fabric API.
        assert!(api.needed_by.is_empty());
        let menu = by_file("mods/menu.jar.disabled");
        assert!(!menu.enabled);
        let menu_icon = menu.icon_file.clone().expect("the biggest logo kept");
        assert_eq!(std::fs::read(dir.join(&menu_icon)).unwrap(), icon);

        let faithful = by_file("resourcepacks/faithful.zip");
        assert_eq!((faithful.kind, faithful.description.as_str(), faithful.origin), (Kind::ResourcePack, "Faithful 32x", Origin::Yours));
        assert!(faithful.icon_file.is_some());
        let loose = by_file("resourcepacks/Loose Pack");
        assert!(loose.folder && loose.sha1.is_empty());
        assert_eq!(loose.description, "By hand");
        assert_eq!(by_file("shaderpacks/Complementary.zip").name, "Complementary");
        let trees = by_file("saves/Home/datapacks/trees.zip");
        assert_eq!((trees.kind, trees.world.as_deref()), (Kind::DataPack, Some("Home")));

        // With Mod Menu turned on, Fabric API is needed by it; the cache keeps it cheap.
        scan.save(&dir);
        assert_eq!(set_enabled(&dir, "mods/menu.jar.disabled", true).unwrap(), "mods/menu.jar");
        let items = super::scan(&dir).items(&instance(false), &dir, None);
        assert_eq!(items.iter().find(|item| item.file == "mods/api.jar").unwrap().needed_by, vec!["Mod Menu".to_string()]);
        assert!(matches!(set_enabled(&dir, "resourcepacks/Loose Pack", false), Err(InventoryError::Folder)));
        assert!(matches!(set_enabled(&dir, "../escape.jar", false), Err(InventoryError::NotListed)));
        assert!(matches!(set_enabled(&dir, "config/x.jar", false), Err(InventoryError::NotListed)));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn tells_pack_files_from_wisps_and_yours() {
        let dir = temp_instance("origin");
        let game = dir.join(GAME_DIR);
        for name in ["pack.jar", "wisp.jar", "mine.jar"] {
            std::fs::write(game.join("mods").join(name), zip_bytes(&[("fabric.mod.json", format!(r#"{{"id": "{name}"}}"#).as_bytes())])).unwrap();
        }
        let installed = InstalledContent {
            project_id: "w".into(),
            version_id: "1".into(),
            title: "Wisp's".into(),
            version_number: "1".into(),
            project_type: ProjectType::Mod,
            file: "mods/wisp.jar".into(),
            icon_url: None,
            explicit: false,
            world: None,
        };
        ContentIndex::new(&dir).save(&[installed]).unwrap();
        let pack: HashMap<String, String> = [("mods/pack.jar".to_string(), String::new())].into();
        record_pack_files(&dir, &pack).unwrap();
        let recorded = pack_files(&dir, None).unwrap();
        let items = scan(&dir).items(&instance(true), &dir, Some(&recorded));
        let origin = |file: &str| items.iter().find(|item| item.file == file).unwrap().origin;
        assert_eq!((origin("mods/pack.jar"), origin("mods/wisp.jar"), origin("mods/mine.jar")), (Origin::Pack, Origin::Wisp, Origin::Yours));
        // Installed as something another mod needed: a library.
        assert!(items.iter().find(|item| item.file == "mods/wisp.jar").unwrap().library);

        // Without the pack's list, what Wisp didn't install in a modpack instance is the pack's.
        let guessed = scan(&dir).items(&instance(true), &dir, None);
        assert_eq!(guessed.iter().find(|item| item.file == "mods/mine.jar").unwrap().origin, Origin::Pack);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn removes_to_a_holding_folder_and_puts_back() {
        let dir = temp_instance("remove");
        let game = dir.join(GAME_DIR);
        std::fs::write(game.join("mods/a.jar"), b"jar").unwrap();
        let installed = InstalledContent {
            project_id: "a".into(),
            version_id: "1".into(),
            title: "A".into(),
            version_number: "1".into(),
            project_type: ProjectType::Mod,
            file: "mods/a.jar".into(),
            icon_url: None,
            explicit: true,
            world: None,
        };
        ContentIndex::new(&dir).save(std::slice::from_ref(&installed)).unwrap();
        let removed = remove(&dir, "mods/a.jar", 1_000).unwrap();
        assert!(!game.join("mods/a.jar").exists());
        assert!(ContentIndex::new(&dir).list().is_empty());
        assert_eq!(super::removed(&dir).len(), 1);
        assert_eq!(put_back(&dir, &removed.id).unwrap(), "mods/a.jar");
        assert_eq!(std::fs::read(game.join("mods/a.jar")).unwrap(), b"jar");
        assert_eq!(ContentIndex::new(&dir).list(), vec![installed]);
        assert!(super::removed(&dir).is_empty());
        assert!(matches!(put_back(&dir, "../../x"), Err(InventoryError::NotListed)));
        // Turning off keeps Wisp's record pointing at the file.
        set_enabled(&dir, "mods/a.jar", false).unwrap();
        assert_eq!(ContentIndex::new(&dir).list()[0].file, "mods/a.jar.disabled");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_a_modpacks_files_from_its_mrpack() {
        let dir = temp_instance("mrpack");
        let mrpack = dir.join("pack.mrpack");
        let index = br#"{"formatVersion": 1, "game": "minecraft", "versionId": "1", "name": "P", "files": [{"path": "mods/sodium.jar", "hashes": {"sha1": "abc", "sha512": "def"}, "downloads": [], "fileSize": 1}], "dependencies": {"minecraft": "26.3"}}"#;
        std::fs::write(&mrpack, zip_bytes(&[("modrinth.index.json", index), ("overrides/config/sodium.json", b"{}"), ("client-overrides/resourcepacks/extra.zip", b"zip")])).unwrap();
        let files = mrpack_files(&mrpack).unwrap();
        assert_eq!(files.get("mods/sodium.jar").map(String::as_str), Some("abc"));
        assert!(files.contains_key("config/sodium.json") && files.contains_key("resourcepacks/extra.zip"));
        // Read once, then kept.
        assert_eq!(pack_files(&dir, Some(&mrpack)), Some(files));
        assert!(dir.join(PACK_FILES).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_minecraft_text() {
        assert_eq!(plain_text(&serde_json::json!("\u{a7}6Gold \u{a7}rpack")), "Gold pack");
        assert_eq!(plain_text(&serde_json::json!({"text": "A", "extra": [{"text": " B"}, " C"]})), "A B C");
        assert_eq!(plain_text(&serde_json::json!({"translate": "fo.resourcePack.modmenuhelper"})), "");
        assert_eq!(plain_text(&serde_json::json!({"translate": "pack.x", "fallback": "Helper"})), "Helper");
        assert_eq!(plain_name("mods/sodium-0.6.jar.disabled"), "sodium-0.6");
    }
}
