//! Wisp's fixer. It finds what would stop an instance's game: before launch from a quick, cached
//! look at the mods folder, and after a crash from the mod loader's own report. Then it fixes
//! what it safely can: missing dependencies, mods for another loader, duplicate copies, an
//! outdated loader, damaged config files, memory and Java. Every change is recorded so it can be
//! undone. Turning off a mod that could have content in a world is only ever offered, never done
//! on its own: a world loaded without a mod loses that mod's blocks.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read, Seek};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::HttpClient;
use crate::content::{self, ContentIndex, InstallRequest, InstalledContent};
use crate::instances::{GAME_DIR, Instance, InstanceStore, Loader};
use crate::launch::java;
use crate::loaders;
use crate::modrinth::{self, ProjectType, SearchQuery};

/// What's been read from each jar, next to `instance.json`.
const CACHE: &str = "mod-metadata.json";
/// Jars bundled in jars bundled in jars: no deeper than this.
const MAX_NESTING: usize = 2;
const MAX_NESTED_JAR: u64 = 64 * 1024 * 1024;
const MAX_METADATA: u64 = 1024 * 1024;
/// Where replaced jars wait, in case a fix is undone.
const BACKUPS: &str = "fix-backups";

/// A mod loader family, as a jar's metadata shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

impl Platform {
    fn label(self) -> &'static str {
        match self {
            Self::Fabric => "Fabric",
            Self::Quilt => "Quilt",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
        }
    }
}

/// What one jar in mods/ says about itself.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModInfo {
    /// The jar's file name in mods/.
    pub file: String,
    /// Its main mod id; empty when the jar doesn't say.
    pub id: String,
    pub name: String,
    pub version: String,
    /// Every id it answers to: its own, what it says it provides, and the mods bundled inside.
    pub provides: Vec<String>,
    /// The loaders it carries metadata for. Empty for jars that aren't mods Wisp can read.
    pub loaders: Vec<Platform>,
    /// Mods it can't run without, from each loader's metadata in it.
    pub requires: Vec<Requirement>,
    /// It says it only runs in the game, never on a server (Fabric's and Quilt's "environment":
    /// "client", Forge's `clientSideOnly`).
    pub client_only: bool,
    /// It says it only runs on a server (Fabric's "environment": "server", Quilt's
    /// "dedicated_server"), so players don't need it.
    #[serde(default)]
    pub server_only: bool,
}

/// A mod a jar can't run without, as one loader's metadata in it says.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Requirement {
    pub id: String,
    /// The versions it accepts, as the jar writes them.
    pub versions: String,
    /// Whose metadata says so. A jar made for several loaders lists each one's needs apart: its
    /// NeoForge half needing NeoForge says nothing about a Fabric game.
    pub platform: Platform,
}

/// The metadata a loader reads from a jar, in the order it looks: Quilt falls back to Fabric's,
/// and NeoForge (for 1.20.1) to Forge's.
fn reads(loader: Loader) -> &'static [Platform] {
    match loader {
        Loader::Fabric => &[Platform::Fabric],
        Loader::Quilt => &[Platform::Quilt, Platform::Fabric],
        Loader::Forge => &[Platform::Forge],
        Loader::NeoForge => &[Platform::NeoForge, Platform::Forge],
        Loader::Vanilla => &[],
    }
}

impl ModInfo {
    /// What it needs where this loader runs it: only the metadata that loader reads. A jar the
    /// loader runs through a bridge (Connector, Kilt) keeps its own first set.
    fn needs(&self, loader: Loader) -> impl Iterator<Item = &Requirement> {
        let platform = reads(loader).iter().copied().find(|platform| self.loaders.contains(platform)).or_else(|| self.loaders.first().copied());
        self.requires.iter().filter(move |requirement| Some(requirement.platform) == platform)
    }

    fn display(&self) -> String {
        if !self.name.is_empty() {
            self.name.clone()
        } else if !self.id.is_empty() {
            self.id.clone()
        } else {
            self.file.clone()
        }
    }

    pub fn answers_to(&self, id: &str) -> bool {
        self.id == id || self.provides.iter().any(|provided| provided == id)
    }
}

// ── Reading jars ──

/// Reads a mod jar's metadata. A jar that can't be read comes back with only its file name.
pub fn read_jar(path: &Path) -> ModInfo {
    let mut info = ModInfo { file: path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(), ..Default::default() };
    if let Ok(file) = std::fs::File::open(path)
        && let Ok(mut archive) = zip::ZipArchive::new(std::io::BufReader::new(file))
    {
        read_archive(&mut archive, &mut info, 0);
    }
    info.provides.sort();
    info.provides.dedup();
    info
}

fn read_archive<R: Read + Seek>(archive: &mut zip::ZipArchive<R>, info: &mut ModInfo, depth: usize) {
    let mut nested = Vec::new();
    if let Some(text) = entry_text(archive, "fabric.mod.json") {
        read_fabric(&text, info, depth, &mut nested);
    }
    if let Some(text) = entry_text(archive, "quilt.mod.json") {
        read_quilt(&text, info, depth, &mut nested);
    }
    for (name, platform) in [("META-INF/neoforge.mods.toml", Platform::NeoForge), ("META-INF/mods.toml", Platform::Forge)] {
        if let Some(text) = entry_text(archive, name) {
            read_forge(&text, platform, info, depth);
        }
    }
    // Forge and NeoForge list the jars they bundle here.
    if let Some(text) = entry_text(archive, "META-INF/jarjar/metadata.json")
        && let Ok(value) = serde_json::from_str::<Value>(&text)
    {
        for jar in value.get("jars").and_then(Value::as_array).into_iter().flatten() {
            if let Some(path) = jar.get("path").and_then(Value::as_str) {
                nested.push(path.to_string());
            }
        }
    }
    if depth >= MAX_NESTING {
        return;
    }
    for path in nested {
        let Some(bytes) = entry_bytes(archive, &path, MAX_NESTED_JAR) else {
            continue;
        };
        let Ok(mut inner) = zip::ZipArchive::new(Cursor::new(bytes)) else {
            continue;
        };
        let mut bundled = ModInfo::default();
        read_archive(&mut inner, &mut bundled, depth + 1);
        info.provides.extend(bundled.provides);
    }
}

fn entry_bytes<R: Read + Seek>(archive: &mut zip::ZipArchive<R>, name: &str, limit: u64) -> Option<Vec<u8>> {
    let entry = archive.by_name(name).ok()?;
    if entry.size() > limit {
        return None;
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.take(limit).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

fn entry_text<R: Read + Seek>(archive: &mut zip::ZipArchive<R>, name: &str) -> Option<String> {
    let bytes = entry_bytes(archive, name, MAX_METADATA)?;
    Some(String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}').to_string())
}

/// JSON as mod authors write it and Fabric's own reader takes it: sometimes with comments, a
/// trailing comma, or a line break typed straight into a description.
fn lenient_json(text: &str) -> Option<Value> {
    if let Ok(value) = serde_json::from_str(text) {
        return Some(value);
    }
    let mut cleaned = String::with_capacity(text.len());
    let (mut in_string, mut escaped) = (false, false);
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if in_string {
            match character {
                _ if escaped => {
                    escaped = false;
                    cleaned.push(character);
                }
                '\\' => {
                    escaped = true;
                    cleaned.push(character);
                }
                '"' => {
                    in_string = false;
                    cleaned.push(character);
                }
                '\n' => cleaned.push_str("\\n"),
                '\r' => cleaned.push_str("\\r"),
                '\t' => cleaned.push_str("\\t"),
                control if control < ' ' => cleaned.push_str(&format!("\\u{:04x}", u32::from(control))),
                _ => cleaned.push(character),
            }
            continue;
        }
        match character {
            '"' => {
                in_string = true;
                cleaned.push(character);
            }
            '/' if characters.peek() == Some(&'/') => {
                for next in characters.by_ref() {
                    if next == '\n' {
                        cleaned.push('\n');
                        break;
                    }
                }
            }
            '#' => {
                for next in characters.by_ref() {
                    if next == '\n' {
                        cleaned.push('\n');
                        break;
                    }
                }
            }
            '/' if characters.peek() == Some(&'*') => {
                characters.next();
                let mut previous = ' ';
                for next in characters.by_ref() {
                    if previous == '*' && next == '/' {
                        break;
                    }
                    previous = next;
                }
                cleaned.push(' ');
            }
            _ => cleaned.push(character),
        }
    }
    static TRAILING: OnceLock<Regex> = OnceLock::new();
    let trailing = TRAILING.get_or_init(|| Regex::new(r",(\s*[}\]])").expect("valid pattern"));
    serde_json::from_str(&trailing.replace_all(&cleaned, "$1")).ok()
}

fn text_at<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn read_fabric(text: &str, info: &mut ModInfo, depth: usize, nested: &mut Vec<String>) {
    let Some(value) = lenient_json(text) else {
        if depth == 0 && !info.loaders.contains(&Platform::Fabric) {
            info.loaders.push(Platform::Fabric);
        }
        return;
    };
    let id = text_at(&value, "id").to_string();
    if !id.is_empty() {
        info.provides.push(id.clone());
    }
    info.provides.extend(value.get("provides").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string));
    nested.extend(value.get("jars").and_then(Value::as_array).into_iter().flatten().filter_map(|jar| jar.get("file").and_then(Value::as_str)).map(str::to_string));
    if depth > 0 {
        return;
    }
    info.loaders.push(Platform::Fabric);
    match text_at(&value, "environment") {
        "client" => info.client_only = true,
        "server" => info.server_only = true,
        _ => {}
    }
    if info.id.is_empty() {
        info.id = id;
    }
    if info.name.is_empty() {
        info.name = text_at(&value, "name").to_string();
    }
    if info.version.is_empty() {
        info.version = text_at(&value, "version").to_string();
    }
    if let Some(Value::Object(depends)) = value.get("depends") {
        for (dependency, versions) in depends {
            let versions = match versions {
                Value::String(text) => text.clone(),
                Value::Array(items) => items.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" || "),
                _ => "*".to_string(),
            };
            info.requires.push(Requirement { id: dependency.clone(), versions, platform: Platform::Fabric });
        }
    }
}

fn read_quilt(text: &str, info: &mut ModInfo, depth: usize, nested: &mut Vec<String>) {
    let Some(value) = lenient_json(text) else {
        return;
    };
    let Some(loader) = value.get("quilt_loader") else {
        return;
    };
    match value.get("minecraft").map(|minecraft| text_at(minecraft, "environment")) {
        Some("client") if depth == 0 => info.client_only = true,
        Some("dedicated_server") if depth == 0 => info.server_only = true,
        _ => {}
    }
    let id = text_at(loader, "id").to_string();
    if !id.is_empty() {
        info.provides.push(id.clone());
    }
    for provided in loader.get("provides").and_then(Value::as_array).into_iter().flatten() {
        if let Some(provided) = provided.as_str().or_else(|| provided.get("id").and_then(Value::as_str)) {
            info.provides.push(plain_id(provided).to_string());
        }
    }
    nested.extend(loader.get("jars").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string));
    if depth > 0 {
        return;
    }
    info.loaders.push(Platform::Quilt);
    if info.id.is_empty() {
        info.id = id;
    }
    if info.name.is_empty() {
        info.name = loader.get("metadata").map(|metadata| text_at(metadata, "name")).unwrap_or_default().to_string();
    }
    if info.version.is_empty() {
        info.version = text_at(loader, "version").to_string();
    }
    for dependency in loader.get("depends").and_then(Value::as_array).into_iter().flatten() {
        match dependency {
            Value::String(id) => info.requires.push(Requirement { id: plain_id(id).to_string(), versions: "*".to_string(), platform: Platform::Quilt }),
            Value::Object(fields) => {
                let optional = fields.get("optional").and_then(Value::as_bool) == Some(true) || fields.contains_key("unless");
                if let (false, Some(id)) = (optional, fields.get("id").and_then(Value::as_str)) {
                    let versions = fields.get("versions").and_then(Value::as_str).unwrap_or("*");
                    info.requires.push(Requirement { id: plain_id(id).to_string(), versions: versions.to_string(), platform: Platform::Quilt });
                }
            }
            _ => {}
        }
    }
}

/// Quilt allows "group:id"; the id is the part after the colon.
fn plain_id(id: &str) -> &str {
    id.rsplit(':').next().unwrap_or(id)
}

/// A mods.toml dependency the mod can't run without: Forge's `mandatory = true`, NeoForge's
/// `type = "required"`.
fn required(dependency: &toml::Value) -> bool {
    dependency.get("mandatory").and_then(toml::Value::as_bool).unwrap_or(false)
        || dependency.get("type").and_then(toml::Value::as_str).is_some_and(|kind| kind.eq_ignore_ascii_case("required"))
}

fn read_forge(text: &str, platform: Platform, info: &mut ModInfo, depth: usize) {
    let Ok(table) = toml::from_str::<toml::Table>(text) else {
        if depth == 0 && !info.loaders.contains(&platform) {
            info.loaders.push(platform);
        }
        return;
    };
    let mods: Vec<&toml::Value> = table.get("mods").and_then(toml::Value::as_array).into_iter().flatten().collect();
    let ids: Vec<String> = mods.iter().filter_map(|entry| entry.get("modId").and_then(toml::Value::as_str)).map(str::to_string).collect();
    info.provides.extend(ids.iter().cloned());
    if depth > 0 {
        return;
    }
    if table.get("clientSideOnly").and_then(toml::Value::as_bool) == Some(true) {
        info.client_only = true;
    }
    let dependencies = table.get("dependencies").and_then(toml::Value::as_table);
    // NeoForge for 1.20.1 to 1.20.4 used mods.toml too; requiring "neoforge" gives it away. A jar
    // made for both lists Forge and NeoForge as optional, and is a Forge jar as much as any.
    let depends_on_neoforge = dependencies.is_some_and(|all| {
        all.values()
            .flat_map(|list| list.as_array().into_iter().flatten())
            .any(|dependency| dependency.get("modId").and_then(toml::Value::as_str) == Some("neoforge") && required(dependency))
    });
    let platform = if platform == Platform::Forge && depends_on_neoforge { Platform::NeoForge } else { platform };
    if !info.loaders.contains(&platform) {
        info.loaders.push(platform);
    }
    if let Some(first) = mods.first() {
        let field = |key: &str| first.get(key).and_then(toml::Value::as_str).filter(|value| !value.starts_with("${")).unwrap_or_default().to_string();
        if info.id.is_empty() {
            info.id = field("modId");
        }
        if info.name.is_empty() {
            info.name = field("displayName");
        }
        if info.version.is_empty() {
            info.version = field("version");
        }
    }
    let Some(dependencies) = dependencies else {
        return;
    };
    for id in &ids {
        for dependency in dependencies.get(id).and_then(toml::Value::as_array).into_iter().flatten() {
            let Some(dependency_id) = dependency.get("modId").and_then(toml::Value::as_str) else {
                continue;
            };
            let server_only = dependency.get("side").and_then(toml::Value::as_str).is_some_and(|side| side.eq_ignore_ascii_case("server"));
            if required(dependency) && !server_only {
                let range = dependency.get("versionRange").and_then(toml::Value::as_str).unwrap_or("");
                info.requires.push(Requirement { id: dependency_id.to_string(), versions: range.to_string(), platform });
            }
        }
    }
}

// ── Scanning mods/ ──

/// Goes up whenever what's read from a jar changes, so caches from before are read afresh.
const CACHE_VERSION: u32 = 3;

#[derive(Default, Serialize, Deserialize)]
struct Cache {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    jars: HashMap<String, Cached>,
}

#[derive(Serialize, Deserialize)]
struct Cached {
    size: u64,
    modified: i64,
    info: ModInfo,
}

fn modified_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_millis() as i64)
}

/// What every enabled jar in an instance's mods/ says about itself. Jars seen before come from a
/// cache (keyed by size and time), so this costs a directory listing on most launches.
pub fn scan(instance_dir: &Path) -> Vec<ModInfo> {
    let mods = instance_dir.join(GAME_DIR).join("mods");
    let Ok(entries) = std::fs::read_dir(&mods) else {
        return Vec::new();
    };
    let cache_path = instance_dir.join(CACHE);
    let mut cache: Cache = std::fs::read(&cache_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Cache>(&bytes).ok())
        .filter(|cache| cache.version == CACHE_VERSION)
        .unwrap_or(Cache { version: CACHE_VERSION, jars: HashMap::new() });
    let mut found = Vec::new();
    let mut unread: Vec<(String, PathBuf, u64, i64)> = Vec::new();
    let mut present = HashSet::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "jar") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let modified = modified_ms(&meta);
        present.insert(name.clone());
        match cache.jars.get(&name) {
            Some(cached) if cached.size == meta.len() && cached.modified == modified => found.push(cached.info.clone()),
            _ => unread.push((name, path, meta.len(), modified)),
        }
    }
    let stale = cache.jars.len() != cache.jars.keys().filter(|name| present.contains(*name)).count();
    if !unread.is_empty() {
        // A modpack's first look reads hundreds of jars; spread them over the cores.
        let workers = std::thread::available_parallelism().map_or(4, |count| count.get()).clamp(1, 8);
        let chunk = unread.len().div_ceil(workers).max(1);
        let read: Vec<(String, u64, i64, ModInfo)> = std::thread::scope(|scope| {
            let handles: Vec<_> = unread
                .chunks(chunk)
                .map(|part| scope.spawn(move || part.iter().map(|(name, path, size, modified)| (name.clone(), *size, *modified, read_jar(path))).collect::<Vec<_>>()))
                .collect();
            handles.into_iter().flat_map(|handle| handle.join().unwrap_or_default()).collect()
        });
        for (name, size, modified, info) in read {
            found.push(info.clone());
            cache.jars.insert(name, Cached { size, modified, info });
        }
    }
    if stale || !unread.is_empty() {
        cache.jars.retain(|name, _| present.contains(name));
        if let Ok(bytes) = serde_json::to_vec(&cache) {
            let _ = std::fs::write(&cache_path, bytes);
        }
    }
    found.sort_by(|a, b| a.file.cmp(&b.file));
    found
}

// ── Problems ──

/// Something that stops (or stopped) the game.
#[derive(Debug, Clone, PartialEq)]
pub enum Problem {
    /// Mods need another that isn't installed (every mod that needs it, by name).
    Missing { id: String, needed_by: Vec<String> },
    /// A mod needs another at a version that isn't the one installed.
    WrongVersion { id: String, needed_by: String, wanted: String, found: String },
    /// A jar made for another loader (a Fabric mod in a Forge instance, say).
    WrongLoader { file: String, name: String, made_for: Platform },
    /// The same mod installed more than once.
    Duplicate { name: String, keep: String, extra: Vec<String> },
    /// The mod loader is older than a mod needs.
    LoaderTooOld { needed_by: String, wanted: String },
    /// A mod made for another Minecraft version.
    WrongGame { file: String, name: String, wanted: String },
    /// A mod the loader says to remove because it clashes with another.
    Incompatible { file: String, name: String, with: String },
    /// A config file the game couldn't read (relative to the game folder).
    BrokenConfig { path: String },
    OutOfMemory,
    /// More memory than Java could reserve.
    TooMuchMemory,
    /// A mod built for a newer Java.
    NeedsJava { major: u32 },
    /// The crash report points at this mod.
    Culprit { file: String, name: String },
    /// A mod that only runs in the game crashed a server as it loaded.
    ClientOnly { file: String, name: String },
}

impl Problem {
    /// The problem in a sentence, for the player.
    pub fn describe(&self, loader: Loader) -> String {
        match self {
            Self::Missing { id, needed_by } => format!("{} {} {}, which isn't installed.", names(needed_by), need(needed_by), dependency_name(id)),
            Self::WrongVersion { id, needed_by, wanted, found } => format!("{needed_by} needs {wanted} of {}, but {found} is installed.", dependency_name(id)),
            Self::WrongLoader { name, made_for, .. } => format!("{name} is made for {}, not {}.", made_for.label(), loader.display_name()),
            Self::Duplicate { name, .. } => format!("{name} is installed more than once."),
            Self::LoaderTooOld { needed_by, .. } => format!("{needed_by} needs a newer {}.", loader.display_name()),
            Self::WrongGame { name, wanted, .. } => format!("{name} is made for another Minecraft version ({wanted})."),
            Self::Incompatible { name, with, .. } => format!("{name} doesn't work alongside {with}."),
            Self::BrokenConfig { path } => format!("A settings file is damaged ({path})."),
            Self::OutOfMemory => "Minecraft ran out of memory.".into(),
            Self::TooMuchMemory => "Java couldn't get the memory it was given.".into(),
            Self::NeedsJava { major } => format!("A mod needs Java {major}."),
            Self::Culprit { name, .. } => format!("The crash points at {name}."),
            Self::ClientOnly { name, .. } => format!("{name} only runs in the game, not on a server."),
        }
    }
}

/// "Sodium", "Sodium and Iris", "Sodium, Iris and Lithium", "Sodium, Iris and 4 other mods".
fn names(list: &[String]) -> String {
    match list {
        [] => "A mod".into(),
        [one] => one.clone(),
        [first, second] => format!("{first} and {second}"),
        [first, second, third] => format!("{first}, {second} and {third}"),
        [first, second, rest @ ..] => format!("{first}, {second} and {} other mods", rest.len()),
    }
}

/// "needs" after one name, "need" after several.
fn need(list: &[String]) -> &'static str {
    if list.len() > 1 { "need" } else { "needs" }
}

/// One of the forty-odd ids Fabric API's jar answers to (its modules, and its old name).
fn is_fabric_api_part(id: &str) -> bool {
    let module = id.starts_with("fabric-") && id.rsplit('-').next().is_some_and(|tail| tail.len() == 2 && tail.starts_with('v') && tail[1..].chars().all(|c| c.is_ascii_digit()));
    module || matches!(id, "fabric-api" | "fabric" | "fabric-api-base")
}

/// A mod id the way players know it: Fabric API's modules are just Fabric API.
fn dependency_name(id: &str) -> String {
    if is_fabric_api_part(id) { "Fabric API".into() } else { id.to_string() }
}

/// Ids that name a mod loader rather than a mod. Never something to look for in mods/: a
/// universal jar's NeoForge half needs NeoForge, which a Fabric game ignores.
fn names_a_loader(id: &str) -> bool {
    matches!(id, "forge" | "neoforge" | "fml" | "javafml" | "lowcodefml" | "fabricloader" | "fabric-loader" | "quilt_loader")
}

/// Ids every instance of a loader answers to without a mod for them.
fn is_builtin(id: &str, loader: Loader) -> bool {
    match id {
        "minecraft" | "java" => true,
        "fabricloader" | "fabric-loader" | "quilt_loader" => matches!(loader, Loader::Fabric | Loader::Quilt),
        // Fabric Loader 0.15+ and NeoForge bundle MixinExtras.
        "mixinextras" => matches!(loader, Loader::Fabric | Loader::Quilt | Loader::NeoForge),
        "forge" | "fml" | "javafml" | "lowcodefml" => matches!(loader, Loader::Forge | Loader::NeoForge),
        "neoforge" => loader == Loader::NeoForge,
        _ => false,
    }
}

fn is_loader_id(id: &str) -> bool {
    matches!(id, "fabricloader" | "fabric-loader" | "quilt_loader" | "forge" | "neoforge")
}

/// True only when `current` is below every minimum the requirement allows (so a newer loader
/// would satisfy it). Requirements Wisp can't read count as satisfied.
fn below_minimum(requirement: &str, current: &str) -> bool {
    let requirement = requirement.trim();
    if requirement.is_empty() || requirement == "*" {
        return false;
    }
    // Maven ranges (Forge, NeoForge): "[47.2,)", "[1.0,2.0)", "[1.0]".
    if requirement.starts_with('[') || requirement.starts_with('(') {
        static RANGE: OnceLock<Regex> = OnceLock::new();
        let range = RANGE.get_or_init(|| Regex::new(r"([\[(])\s*([^,\[\]()]*)").expect("valid pattern"));
        let minimums: Vec<(bool, &str)> = range.captures_iter(requirement).map(|found| (found.get(1).is_some_and(|open| open.as_str() == "["), found.get(2).map_or("", |min| min.as_str().trim()))).collect();
        if minimums.is_empty() {
            return false;
        }
        return minimums.iter().all(|(inclusive, min)| {
            !min.is_empty() && {
                let order = loaders::compare_versions(current, min);
                order == Ordering::Less || (!inclusive && order == Ordering::Equal)
            }
        });
    }
    // Fabric's predicates: "||" between alternatives, spaces between conditions.
    requirement.split("||").all(|alternative| {
        alternative.split_whitespace().any(|condition| {
            let (operator, version) = match condition.find(|character: char| character.is_ascii_alphanumeric()) {
                Some(start) => condition.split_at(start),
                None => return false,
            };
            if version.contains(['x', 'X', '*']) {
                return false;
            }
            let order = loaders::compare_versions(current, version);
            match operator {
                ">=" | "^" | "~" | "=" | "" => order == Ordering::Less,
                ">" => order != Ordering::Greater,
                _ => false,
            }
        })
    })
}

/// How many problems these are to a player: Fabric API's modules missing together are one.
pub fn count(problems: &[Problem]) -> usize {
    let missing: Vec<Problem> = problems.iter().filter(|problem| matches!(problem, Problem::Missing { .. })).cloned().collect();
    problems.len() - missing.len() + families(missing).len()
}

/// Problems the mods folder shows before the game starts. Cheap enough for every launch.
pub fn check(instance: &Instance, instance_dir: &Path) -> Vec<Problem> {
    if instance.loader == Loader::Vanilla {
        return Vec::new();
    }
    check_mods(instance, &scan(instance_dir))
}

fn fits(platform: Platform, loader: Loader) -> bool {
    match loader {
        Loader::Fabric => platform == Platform::Fabric,
        Loader::Quilt => matches!(platform, Platform::Quilt | Platform::Fabric),
        Loader::Forge => platform == Platform::Forge,
        // NeoForge for 1.20.1 runs Forge mods; for later versions its own error says enough.
        Loader::NeoForge => matches!(platform, Platform::NeoForge | Platform::Forge),
        Loader::Vanilla => false,
    }
}

fn check_mods(instance: &Instance, mods: &[ModInfo]) -> Vec<Problem> {
    let mut problems = Vec::new();
    // Connector runs Fabric mods on NeoForge; Kilt runs Forge mods on Fabric.
    let bridged = mods.iter().any(|info| info.answers_to("connector") || info.answers_to("kilt"));
    let mut usable: Vec<&ModInfo> = Vec::new();
    for info in mods {
        if info.loaders.is_empty() || bridged || info.loaders.iter().any(|platform| fits(*platform, instance.loader)) {
            usable.push(info);
        } else {
            problems.push(Problem::WrongLoader { file: info.file.clone(), name: info.display(), made_for: info.loaders[0] });
        }
    }

    let mut by_id: HashMap<&str, Vec<&ModInfo>> = HashMap::new();
    for info in &usable {
        if !info.id.is_empty() {
            by_id.entry(info.id.as_str()).or_default().push(info);
        }
    }
    let mut extras: HashSet<String> = HashSet::new();
    let mut duplicates: Vec<Problem> = Vec::new();
    for copies in by_id.values().filter(|copies| copies.len() > 1) {
        let Some(keep) = copies.iter().max_by(|a, b| loaders::compare_versions(&a.version, &b.version).then_with(|| a.file.cmp(&b.file))) else {
            continue;
        };
        let extra: Vec<String> = copies.iter().filter(|copy| copy.file != keep.file).map(|copy| copy.file.clone()).collect();
        extras.extend(extra.iter().cloned());
        duplicates.push(Problem::Duplicate { name: keep.display(), keep: keep.file.clone(), extra });
    }
    duplicates.sort_by_key(|problem| format!("{problem:?}"));
    problems.extend(duplicates);

    let present = provided(instance, mods, &extras);
    let mut loader_noted = false;
    let mut missing: Vec<(String, Vec<String>)> = Vec::new();
    for info in usable.iter().filter(|info| !extras.contains(&info.file)) {
        for requirement in info.needs(instance.loader) {
            let dependency = plain_id(&requirement.id);
            if is_builtin(dependency, instance.loader) {
                let outdated = is_loader_id(dependency) && instance.loader_version.as_deref().is_some_and(|current| below_minimum(&requirement.versions, current));
                if outdated && !loader_noted {
                    loader_noted = true;
                    problems.push(Problem::LoaderTooOld { needed_by: info.display(), wanted: requirement.versions.clone() });
                }
                continue;
            }
            if names_a_loader(dependency) || present.contains(dependency) {
                continue;
            }
            let name = info.display();
            match missing.iter_mut().find(|(id, _)| id == dependency) {
                Some((_, needed_by)) if !needed_by.contains(&name) => needed_by.push(name),
                Some(_) => {}
                None => missing.push((dependency.to_string(), vec![name])),
            }
        }
    }
    problems.extend(missing.into_iter().map(|(id, needed_by)| Problem::Missing { id, needed_by }));
    problems
}

/// Every id the jars that run in this instance answer to, leaving out the files in `skip` (the
/// extra copies of a mod that are about to be turned off).
fn provided(instance: &Instance, mods: &[ModInfo], skip: &HashSet<String>) -> HashSet<String> {
    let bridged = mods.iter().any(|info| info.answers_to("connector") || info.answers_to("kilt"));
    mods.iter()
        .filter(|info| !skip.contains(&info.file))
        .filter(|info| info.loaders.is_empty() || bridged || info.loaders.iter().any(|platform| fits(*platform, instance.loader)))
        .flat_map(|info| info.provides.iter().cloned())
        .collect()
}

// ── Reading a crash ──

struct Patterns {
    fabric_missing: Regex,
    fabric_wrong: Regex,
    fabric_remove: Regex,
    forge_dependency: Regex,
    neo_requires: Regex,
    neo_missing: Regex,
    class_version: Regex,
    config: Regex,
    suspected: Regex,
    suspect: Regex,
    forge_failure: Regex,
    mixin_mod: Regex,
}

fn patterns() -> &'static Patterns {
    static PATTERNS: OnceLock<Patterns> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let pattern = |source: &str| Regex::new(source).expect("valid pattern");
        let fabric_head = r"Mod '(?P<name>[^']+)' \((?P<id>[^)]+)\) \S+ requires (?P<what>.+?) of (?:mod '[^']*' \()?(?P<dep>[A-Za-z0-9_.\-]+)\)?";
        Patterns {
            fabric_missing: pattern(&format!("{fabric_head}, which is missing")),
            fabric_wrong: pattern(&format!("{fabric_head}, but only the wrong version is present: (?P<found>[^!]+)!")),
            fabric_remove: pattern(r"Remove mod '(?P<name>[^']+)' \((?P<id>[^)]+)\)"),
            forge_dependency: pattern(r"Mod ID: '(?P<dep>[^']+)', Requested by: '(?P<by>[^']+)', Expected range: '(?P<range>[^']*)', Actual version: '(?P<actual>[^']*)'"),
            neo_requires: pattern(r"Mod (?P<by>.+?) requires (?P<dep>[A-Za-z0-9_.\-]+) (?P<range>.+?)\s*$"),
            neo_missing: pattern(r"Currently, (?P<dep>[A-Za-z0-9_.\-]+) is not installed"),
            class_version: pattern(r"class file version (?P<version>\d+)\.\d+\), this version of the Java Runtime only recognizes class file versions up to"),
            config: pattern(r"Failed loading config file (?P<file>\S+) of type (?P<kind>\w+) for modid (?P<id>\S+)"),
            suspected: pattern(r"Suspected Mods?: (?P<list>.+)$"),
            suspect: pattern(r"(?P<name>[^,(]+?) \((?P<id>[A-Za-z0-9_.\-]+)\)"),
            forge_failure: pattern(r"Failure message: (?P<name>.+?) \((?P<id>[A-Za-z0-9_.\-]+)\) (?:has failed to load correctly|encountered an error)"),
            mixin_mod: pattern(r"[Mm]ixin.*from mod (?P<id>[A-Za-z0-9_.\-]+)"),
        }
    })
}

/// A line without § formatting, backticks or a log prefix's padding.
fn clean(line: &str) -> String {
    crate::servers::strip_codes(line).replace('`', "")
}

/// The newest crash report written since the game started.
fn crash_report(game_dir: &Path, since_ms: i64) -> Option<String> {
    let newest = std::fs::read_dir(game_dir.join("crash-reports"))
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            let at = modified_ms(&meta);
            (meta.is_file() && at >= since_ms).then(|| (at, entry.path()))
        })
        .max_by_key(|(at, _)| *at)?;
    let bytes = std::fs::read(newest.1).ok()?;
    Some(String::from_utf8_lossy(&bytes[..bytes.len().min(2 * 1024 * 1024)]).into_owned())
}

/// What a game's output (and crash report) says went wrong.
pub fn diagnose(instance: &Instance, instance_dir: &Path, output: &[String], since_ms: i64) -> Vec<Problem> {
    let report = crash_report(&instance_dir.join(GAME_DIR), since_ms).unwrap_or_default();
    let lines: Vec<String> = output.iter().map(String::as_str).chain(report.lines()).map(clean).collect();
    let mods = scan(instance_dir);
    diagnose_lines(instance, &lines, &mods)
}

/// What a server's console (and crash report) says went wrong: what a game's would, and first, a
/// mod that only runs in the game, which crashes a server as it loads (the commonest way a modded
/// server fails to start).
pub fn diagnose_server(server_dir: &Path, loader: Loader, console: &[String], since_ms: i64) -> Vec<Problem> {
    let report = crash_report(server_dir, since_ms).unwrap_or_default();
    let lines: Vec<String> = console.iter().map(String::as_str).chain(report.lines()).map(clean).collect();
    let mods_dir = server_dir.join("mods");
    let mods: Vec<ModInfo> = std::fs::read_dir(&mods_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "jar") && path.is_file())
        .map(|path| read_jar(&path))
        .collect();
    let mut problems = Vec::new();
    if let Some(culprit) = client_only_culprit(&lines, &mods, &mods_dir) {
        problems.push(culprit);
    }
    for problem in read_problems(loader, &lines, &mods) {
        let same_mod = matches!((&problem, problems.first()), (Problem::Culprit { file, .. }, Some(Problem::ClientOnly { file: known, .. })) if file == known);
        if !same_mod && !problems.contains(&problem) {
            problems.push(problem);
        }
    }
    problems
}

/// The mod behind a crash from something only the game has (`net/minecraft/client/...`) being
/// loaded on a server: the mod Forge's stack frames name, a mixin's mod, the jar holding the first
/// class in the trace that isn't Minecraft's or a loader's, or the crash report's suspect.
fn client_only_culprit(lines: &[String], mods: &[ModInfo], mods_dir: &Path) -> Option<Problem> {
    static FRAME: OnceLock<Regex> = OnceLock::new();
    static MODULE: OnceLock<Regex> = OnceLock::new();
    let game_only = lines.iter().position(|line| {
        line.contains("invalid dist DEDICATED_SERVER")
            || line.contains("in environment type SERVER")
            || ((line.contains("NoClassDefFoundError") || line.contains("ClassNotFoundException")) && (line.contains("net/minecraft/client") || line.contains("net.minecraft.client")))
    })?;
    let after = &lines[game_only..];
    let culprit = |id: &str| -> Option<Problem> {
        let info = mods.iter().find(|info| info.id == id).or_else(|| mods.iter().find(|info| info.answers_to(id)))?;
        Some(Problem::ClientOnly { file: info.file.clone(), name: info.display() })
    };
    let ignored = ["minecraft", "forge", "neoforge", "fml", "fabricloader", "mixinextras", "java", "quilt_loader"];
    // Forge and NeoForge name the mod in each frame: "at TRANSFORMER/mymod@1.2/com.example...".
    let module = MODULE.get_or_init(|| Regex::new(r"at (?:TRANSFORMER|LAYER SERVICE|MC-BOOTSTRAP|SECURE-BOOTSTRAP)?/?(?P<id>[a-z][a-z0-9_\-]*)@").expect("valid pattern"));
    for line in after.iter().take(80) {
        if let Some(found) = module.captures(line)
            && !ignored.contains(&&found["id"])
            && let Some(problem) = culprit(&found["id"])
        {
            return Some(problem);
        }
    }
    let patterns = patterns();
    for line in after.iter().take(80).chain(lines.iter()) {
        if let Some(found) = patterns.mixin_mod.captures(line)
            && !ignored.contains(&&found["id"])
            && let Some(problem) = culprit(&found["id"])
        {
            return Some(problem);
        }
    }
    let frame = FRAME.get_or_init(|| Regex::new(r"at (?:[^ (]+/)?(?P<class>[A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)+)\.[\w$<>]+\(").expect("valid pattern"));
    let framework = [
        "java.", "javax.", "jdk.", "sun.", "com.sun.", "net.minecraft.", "com.mojang.", "net.minecraftforge.", "net.neoforged.", "cpw.mods.", "net.fabricmc.", "org.quiltmc.", "org.spongepowered.", "com.llamalad7.", "org.objectweb.", "io.netty.",
        "it.unimi.", "com.google.", "org.apache.", "org.slf4j.", "kotlin.", "kotlinx.", "scala.", "org.openjdk.",
    ];
    for line in after.iter().take(80) {
        let Some(found) = frame.captures(line) else { continue };
        let class = &found["class"];
        if framework.iter().any(|prefix| class.starts_with(prefix)) {
            continue;
        }
        let entry = format!("{}.class", class.split('$').next().unwrap_or(class).replace('.', "/"));
        let holder = mods.iter().find(|info| {
            std::fs::File::open(mods_dir.join(&info.file)).ok().and_then(|file| zip::ZipArchive::new(std::io::BufReader::new(file)).ok()).is_some_and(|mut archive| archive.by_name(&entry).is_ok())
        });
        if let Some(info) = holder {
            return Some(Problem::ClientOnly { file: info.file.clone(), name: info.display() });
        }
        break;
    }
    for line in lines {
        if let Some(found) = patterns.suspected.captures(line) {
            for suspect in patterns.suspect.captures_iter(&found["list"]) {
                if !ignored.contains(&&suspect["id"])
                    && let Some(problem) = culprit(&suspect["id"])
                {
                    return Some(problem);
                }
            }
        }
    }
    None
}

fn file_for(mods: &[ModInfo], id: &str) -> Option<String> {
    mods.iter().find(|info| info.id == id).or_else(|| mods.iter().find(|info| info.answers_to(id))).map(|info| info.file.clone())
}

fn diagnose_lines(instance: &Instance, lines: &[String], mods: &[ModInfo]) -> Vec<Problem> {
    read_problems(instance.loader, lines, mods)
}

fn read_problems(loader: Loader, lines: &[String], mods: &[ModInfo]) -> Vec<Problem> {
    let patterns = patterns();
    let mut problems: Vec<Problem> = Vec::new();
    fn push(problems: &mut Vec<Problem>, problem: Problem) {
        if !problems.contains(&problem) {
            problems.push(problem);
        }
    }
    // One problem per missing id, naming every mod that needs it. A loader's own id is never a
    // mod to add.
    let missing = |problems: &mut Vec<Problem>, id: &str, needed_by: String| {
        if is_builtin(id, loader) || names_a_loader(id) {
            return;
        }
        match problems.iter_mut().find_map(|problem| match problem {
            Problem::Missing { id: known, needed_by } if known == id => Some(needed_by),
            _ => None,
        }) {
            Some(names) if !names.contains(&needed_by) => names.push(needed_by),
            Some(_) => {}
            None => problems.push(Problem::Missing { id: id.to_string(), needed_by: vec![needed_by] }),
        }
    };
    let name_of = |id: &str, fallback: &str| mods.iter().find(|info| info.answers_to(id)).map_or(fallback.to_string(), ModInfo::display);
    let joined = lines.join("\n");

    for (index, line) in lines.iter().enumerate() {
        if let Some(found) = patterns.fabric_missing.captures(line) {
            missing(&mut problems, &found["dep"], found["name"].to_string());
        } else if let Some(found) = patterns.fabric_wrong.captures(line) {
            let (name, id, dependency, what) = (&found["name"], &found["id"], &found["dep"], &found["what"]);
            match dependency {
                "minecraft" => {
                    if let Some(file) = file_for(mods, id) {
                        push(&mut problems, Problem::WrongGame { file, name: name.to_string(), wanted: what.trim_start_matches("version ").to_string() });
                    }
                }
                "fabricloader" | "fabric-loader" | "quilt_loader" => push(&mut problems, Problem::LoaderTooOld { needed_by: name.to_string(), wanted: what.to_string() }),
                "java" => {
                    if let Some(major) = what.split(|character: char| !character.is_ascii_digit()).find_map(|part| part.parse::<u32>().ok()) {
                        push(&mut problems, Problem::NeedsJava { major });
                    }
                }
                _ => push(&mut problems, Problem::WrongVersion { id: dependency.to_string(), needed_by: name.to_string(), wanted: what.to_string(), found: found["found"].trim().to_string() }),
            }
        }
        if let Some(found) = patterns.fabric_remove.captures(line)
            && let Some(file) = file_for(mods, &found["id"])
        {
            push(&mut problems, Problem::Incompatible { file, name: found["name"].to_string(), with: "another mod".into() });
        }
        if let Some(found) = patterns.forge_dependency.captures(line) {
            let (dependency, by, range, actual) = (&found["dep"], &found["by"], &found["range"], &found["actual"]);
            let needed_by = name_of(by, by);
            if actual.contains("MISSING") {
                missing(&mut problems, dependency, needed_by);
            } else if dependency == "minecraft" {
                if let Some(file) = file_for(mods, by) {
                    push(&mut problems, Problem::WrongGame { file, name: needed_by, wanted: range.to_string() });
                }
            } else if matches!(dependency, "forge" | "neoforge") {
                push(&mut problems, Problem::LoaderTooOld { needed_by, wanted: range.to_string() });
            } else {
                push(&mut problems, Problem::WrongVersion { id: dependency.to_string(), needed_by, wanted: range.to_string(), found: actual.to_string() });
            }
        }
        if let Some(found) = patterns.neo_missing.captures(line) {
            let dependency = found["dep"].to_string();
            // The line before says who needs it: "Mod jei requires architectury 9.1 or above".
            let needed_by = lines[..index]
                .iter()
                .rev()
                .take(3)
                .find_map(|earlier| patterns.neo_requires.captures(earlier).filter(|requires| requires["dep"] == dependency).map(|requires| name_of(&requires["by"], &requires["by"])))
                .unwrap_or_else(|| "A mod".to_string());
            missing(&mut problems, &dependency, needed_by);
        }
        if let Some(found) = patterns.class_version.captures(line)
            && let Ok(version) = found["version"].parse::<u32>()
        {
            push(&mut problems, Problem::NeedsJava { major: version.saturating_sub(44) });
        }
        if let Some(found) = patterns.config.captures(line)
            && matches!(found["kind"].to_ascii_uppercase().as_str(), "CLIENT" | "COMMON" | "STARTUP")
            && joined.contains("ParsingException")
        {
            push(&mut problems, Problem::BrokenConfig { path: format!("config/{}", &found["file"]) });
        }
        if line.contains("java.lang.OutOfMemoryError: Java heap space") || line.contains("java.lang.OutOfMemoryError: GC overhead limit exceeded") {
            push(&mut problems, Problem::OutOfMemory);
        }
        if line.contains("Could not reserve enough space for object heap") || line.contains("Invalid maximum heap size") || line.contains("insufficient memory for the Java Runtime Environment") {
            push(&mut problems, Problem::TooMuchMemory);
        }
    }

    // Only when nothing clearer turned up: the mod the crash points at.
    if problems.is_empty() {
        let mut suspects: Vec<(String, String)> = Vec::new();
        for line in lines {
            if let Some(found) = patterns.forge_failure.captures(line) {
                suspects.push((found["id"].to_string(), found["name"].trim().to_string()));
            }
            if let Some(found) = patterns.suspected.captures(line) {
                for suspect in patterns.suspect.captures_iter(&found["list"]) {
                    suspects.push((suspect["id"].to_string(), suspect["name"].trim().to_string()));
                }
            }
            if let Some(found) = patterns.mixin_mod.captures(line)
                && (line.contains("fail") || line.contains("Error") || line.contains("rror"))
            {
                suspects.push((found["id"].to_string(), found["id"].to_string()));
            }
        }
        let ignored = ["minecraft", "java", "fabricloader", "fabric-api", "forge", "neoforge", "mixinextras", "quilt_loader"];
        if let Some((id, name)) = suspects.into_iter().find(|(id, _)| !ignored.contains(&id.as_str()) && !id.starts_with("fabric-"))
            && let Some(file) = file_for(mods, &id)
        {
            push(&mut problems, Problem::Culprit { file, name: name_of(&id, &name) });
        }
    }
    problems
}

// ── Fixing ──

/// A change Wisp made to fix something; enough to put it back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum Change {
    /// mods/<file> became mods/<file>.disabled.
    Disabled { file: String, name: String },
    /// Projects installed (files relative to the game folder).
    Added { project_id: String, title: String, version: String, files: Vec<String> },
    /// A project's jar swapped for another version; the old one waits in a backup.
    Replaced { previous: Box<InstalledContent>, backup: String, current: Box<InstalledContent> },
    LoaderChanged { from: Option<String>, to: String },
    /// A damaged config moved aside (paths relative to the game folder).
    ConfigMoved { from: String, to: String },
    MemoryChanged { from: Option<u32>, to: u32 },
    JavaChanged { from: Option<u32>, to: u32 },
}

/// Something Wisp can do with a click, left to the player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    /// For a button: "Turn off Sodium".
    pub label: String,
    pub detail: String,
    pub action: Action,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum Action {
    Disable { file: String, name: String },
}

/// What Wisp found, what it changed, and what's left.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// What went wrong, a sentence each.
    pub found: Vec<String>,
    /// What Wisp did about it, a sentence each.
    pub fixed: Vec<String>,
    pub changes: Vec<Change>,
    pub offers: Vec<Offer>,
    /// What Wisp couldn't fix, with what to try.
    pub unsolved: Vec<String>,
    /// The journal entry recording the changes (for Undo), once recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
}

impl Report {
    pub fn is_empty(&self) -> bool {
        self.found.is_empty() && self.changes.is_empty() && self.offers.is_empty() && self.unsolved.is_empty()
    }
}

/// What fixing needs to know besides the problems.
pub struct Fixer<'a> {
    pub client: &'a HttpClient,
    pub store: &'a InstanceStore,
    pub instance_id: &'a str,
    pub concurrency: usize,
    pub curseforge_key: Option<&'a str>,
    /// The memory this instance runs with now (its own, or the one in Settings).
    pub memory_mb: u32,
    /// This computer's memory, when known.
    pub system_mb: Option<u64>,
}

impl Fixer<'_> {
    /// Fixes what it safely can and reports the rest.
    pub async fn repair(&self, problems: Vec<Problem>) -> Report {
        let mut report = Report::default();
        let (Some(instance_dir), Some(instance)) = (self.store.dir_of(self.instance_id), self.store.get(self.instance_id)) else {
            return report;
        };
        // What answered before any fix. An id the game missed though a jar here answers to it
        // still gets a look; one a fix below brings along doesn't.
        let before = provided(&instance, &scan(&instance_dir), &HashSet::new());
        let (wanted, others): (Vec<Problem>, Vec<Problem>) = problems.into_iter().partition(|problem| matches!(problem, Problem::Missing { .. }));
        for problem in others {
            let Some(instance) = self.store.get(self.instance_id) else {
                return report;
            };
            report.found.push(problem.describe(instance.loader));
            self.fix(&instance, &instance_dir, problem, &mut report).await;
        }
        for family in families(wanted) {
            let Some(instance) = self.store.get(self.instance_id) else {
                return report;
            };
            let ids = still_missing(&family.ids, &before, &provided(&instance, &scan(&instance_dir), &HashSet::new()));
            if !ids.is_empty() {
                self.fix_missing(&instance, &instance_dir, Family { ids, ..family }, &mut report).await;
            }
        }
        report
    }

    /// Adds what a group of mods is missing, or says what's left to do.
    async fn fix_missing(&self, instance: &Instance, instance_dir: &Path, family: Family, report: &mut Report) {
        let (who, verb, name) = (names(&family.needed_by), need(&family.needed_by), family.name());
        report.found.push(format!("{who} {verb} {name}, which isn't installed."));
        let game = format!("{} {}", instance.loader.display_name(), instance.game_version);
        // A search that came up empty a little while ago would only hold up Play again.
        let key = format!("{game}|{}", family.ids[0]);
        let missed = missed_lately(instance_dir, &key);
        let outcome = if missed { Err(Lookup::NotFound) } else { self.install_providing(instance, instance_dir, &family.ids[0]).await };
        let them = match family.needed_by.as_slice() {
            [one] => one.clone(),
            _ => "them".into(),
        };
        match outcome {
            Ok(change) => {
                remember_miss(instance_dir, &key, false);
                if let Change::Added { title, version, .. } = &change {
                    report.fixed.push(format!("Added {title} {version}, which {who} {verb}."));
                }
                report.changes.push(change);
            }
            Err(Lookup::NotFound) => {
                if !missed {
                    remember_miss(instance_dir, &key, true);
                }
                report.unsolved.push(format!("{who} {verb} {name}, and Wisp couldn't find it for {game}. Add it from Discover, or turn {them} off."));
            }
            Err(Lookup::Unreachable) => report.unsolved.push(format!("{who} {verb} {name}, and Wisp couldn't reach Modrinth to add it. Press Play again once you're online, or turn {them} off.")),
        }
    }

    async fn fix(&self, instance: &Instance, instance_dir: &Path, problem: Problem, report: &mut Report) {
        match problem {
            Problem::WrongLoader { file, name, made_for } => match disable(instance_dir, &file) {
                Ok(()) => {
                    report.fixed.push(format!("Turned off {name}: it's made for {}, so it could never run here.", made_for.label()));
                    report.changes.push(Change::Disabled { file, name });
                }
                Err(error) => report.unsolved.push(format!("Couldn't turn off {name}: {error}")),
            },
            Problem::Duplicate { name, keep, extra } => {
                for file in extra {
                    match disable(instance_dir, &file) {
                        Ok(()) => {
                            report.fixed.push(format!("Turned off an extra copy of {name} ({file}), keeping {keep}."));
                            report.changes.push(Change::Disabled { file, name: name.clone() });
                        }
                        Err(error) => report.unsolved.push(format!("Couldn't turn off the extra {name}: {error}")),
                    }
                }
            }
            // `repair` sends these to `fix_missing` in groups; one on its own goes the same way.
            Problem::Missing { id, needed_by } => self.fix_missing(instance, instance_dir, Family { ids: vec![id], needed_by }, report).await,
            Problem::WrongVersion { id, needed_by, wanted, .. } => {
                let wants_newer = wanted.contains("or later") || wanted.contains(">=") || wanted.ends_with(",)") || wanted.contains("above");
                let changed = if wants_newer { self.update_providing(instance, instance_dir, &id).await } else { None };
                let name = dependency_name(&id);
                match changed {
                    Some(changes) => {
                        report.fixed.push(format!("Updated {name}: {needed_by} needs {wanted}."));
                        report.changes.extend(changes);
                    }
                    None => report.unsolved.push(format!("{needed_by} needs {wanted} of {name}. Install that version, or turn {needed_by} off.")),
                }
            }
            Problem::LoaderTooOld { needed_by, .. } => match self.update_loader(instance).await {
                Some(change) => {
                    if let Change::LoaderChanged { to, .. } = &change {
                        report.fixed.push(format!("Updated {} to {to}: {needed_by} needs a newer one.", instance.loader.display_name()));
                    }
                    report.changes.push(change);
                }
                None => report.unsolved.push(format!("{needed_by} needs a newer {} than Wisp can find for Minecraft {}.", instance.loader.display_name(), instance.game_version)),
            },
            Problem::WrongGame { file, name, wanted } => match self.replace_for_this_game(instance, instance_dir, &file).await {
                Some(change) => {
                    report.fixed.push(format!("Swapped {name} for its build for Minecraft {}.", instance.game_version));
                    report.changes.push(change);
                }
                None => report.offers.push(Offer {
                    label: format!("Turn off {name}"),
                    detail: format!("It's made for Minecraft {wanted} and has no build for {}. Worlds that use its blocks or items lose them while it's off.", instance.game_version),
                    action: Action::Disable { file, name },
                }),
            },
            Problem::Incompatible { file, name, .. } => report.offers.push(Offer {
                label: format!("Turn off {name}"),
                detail: "The mod loader says it can't run alongside another mod here. Worlds that use its blocks or items lose them while it's off.".into(),
                action: Action::Disable { file, name },
            }),
            Problem::ClientOnly { file, name } => report.offers.push(Offer {
                label: format!("Turn off {name}"),
                detail: format!("{name} only runs in the game. Turning it off here changes nothing in play."),
                action: Action::Disable { file, name },
            }),
            Problem::Culprit { file, name } => report.offers.push(Offer {
                label: format!("Turn off {name}"),
                detail: format!("The crash points at {name}. An update may fix it; turning it off gets the game going, but worlds that use its blocks or items lose them while it's off."),
                action: Action::Disable { file, name },
            }),
            Problem::BrokenConfig { path } => {
                let game_dir = instance_dir.join(GAME_DIR);
                let aside = format!("{path}.broken");
                match content::safe_join(&game_dir, &path).zip(content::safe_join(&game_dir, &aside)) {
                    Some((from, to)) if from.is_file() && std::fs::rename(&from, &to).is_ok() => {
                        report.fixed.push(format!("Reset {path}: it was damaged, so its mod starts from default settings. The old file is kept as {aside}."));
                        report.changes.push(Change::ConfigMoved { from: path, to: aside });
                    }
                    _ => report.unsolved.push(format!("{path} is damaged. Deleting it lets its mod make a fresh one.")),
                }
            }
            Problem::OutOfMemory => {
                let current = self.memory_mb;
                let ceiling = self.system_mb.map_or(8192, |total| ((total * 6 / 10) as u32) / 512 * 512);
                let target = ((current + 2048).min(ceiling)) / 512 * 512;
                if target > current && self.store.update(self.instance_id, |stored| stored.memory_mb = Some(target)).is_ok() {
                    report.fixed.push(format!("Gave {} more memory: {} instead of {}.", instance.name, gigabytes(target), gigabytes(current)));
                    report.changes.push(Change::MemoryChanged { from: instance.memory_mb, to: target });
                } else {
                    report.unsolved.push("Minecraft ran out of memory and this computer can't spare more. Fewer mods, a lower render distance or a smaller resource pack will help.".into());
                }
            }
            Problem::TooMuchMemory => {
                let current = self.memory_mb;
                let target = (current / 2).max(1024) / 512 * 512;
                if target < current && self.store.update(self.instance_id, |stored| stored.memory_mb = Some(target)).is_ok() {
                    report.fixed.push(format!("Gave {} less memory ({} instead of {}): Java couldn't get that much.", instance.name, gigabytes(target), gigabytes(current)));
                    report.changes.push(Change::MemoryChanged { from: instance.memory_mb, to: target });
                } else {
                    report.unsolved.push("Java couldn't get the memory it needs. Closing other apps may help.".into());
                }
            }
            Problem::NeedsJava { major } => {
                let newer = instance.java_major.is_none_or(|current| current < major);
                if java::component_for(major).is_some() && newer && self.store.update(self.instance_id, |stored| stored.java_major = Some(major)).is_ok() {
                    report.fixed.push(format!("Switched {} to Java {major}: a mod needs it.", instance.name));
                    report.changes.push(Change::JavaChanged { from: instance.java_major, to: major });
                } else {
                    report.unsolved.push(format!("A mod needs Java {major}. Pick one in Settings, under Java."));
                }
            }
        }
    }

    /// Finds the project whose jar provides a mod id and installs it. Each guess is downloaded
    /// and read before anything goes into the instance, and only a jar that really answers to
    /// the id is installed (that very version), so a wrong guess never touches mods/.
    async fn install_providing(&self, instance: &Instance, instance_dir: &Path, id: &str) -> Result<Change, Lookup> {
        if names_a_loader(id) || is_builtin(id, instance.loader) {
            return Err(Lookup::NotFound);
        }
        let loaders = content::compatible_loaders(instance.loader, ProjectType::Mod).ok_or(Lookup::NotFound)?;
        let mut outcome = Lookup::NotFound;
        for candidate in candidates(self.client, instance, id).await.ok_or(Lookup::Unreachable)? {
            let version = match content::best_version(self.client, None, &candidate, &loaders, Some(&instance.game_version)).await {
                Ok(Some(version)) => version,
                Ok(None) => continue,
                Err(_) => {
                    outcome = Lookup::Unreachable;
                    continue;
                }
            };
            let Some(file) = version.primary_file() else {
                continue;
            };
            match jar_at(self.client, &file.url).await {
                Some(info) if info.answers_to(id) => {}
                Some(_) => continue,
                None => {
                    outcome = Lookup::Unreachable;
                    continue;
                }
            }
            // Listed as installed already doesn't stop it: its jar may be gone or turned off.
            let request = InstallRequest { project_id: &version.project_id, version_id: Some(&version.id), world: None, concurrency: self.concurrency, curseforge_key: self.curseforge_key };
            let Ok(installed) = content::install(self.client, instance, instance_dir, request).await else {
                outcome = Lookup::Unreachable;
                continue;
            };
            mark_dependencies(&ContentIndex::new(instance_dir), &installed);
            let main = installed.iter().find(|item| item.project_id == version.project_id).or(installed.first()).ok_or(Lookup::NotFound)?;
            return Ok(Change::Added {
                project_id: main.project_id.clone(),
                title: main.title.clone(),
                version: main.version_number.clone(),
                files: installed.iter().map(|item| item.file.clone()).collect(),
            });
        }
        Err(outcome)
    }

    /// Updates whatever provides a mod id to its newest build for this instance.
    async fn update_providing(&self, instance: &Instance, instance_dir: &Path, id: &str) -> Option<Vec<Change>> {
        let mods = scan(instance_dir);
        let file = file_for(&mods, id)?;
        if let Some(change) = self.replace_for_this_game(instance, instance_dir, &file).await {
            return Some(vec![change]);
        }
        // Not from Modrinth: add the right one, then turn the old jar off.
        let added = self.install_providing(instance, instance_dir, id).await.ok()?;
        // Unless the newest build is that very file.
        if matches!(&added, Change::Added { files, .. } if files.iter().any(|path| *path == format!("mods/{file}"))) {
            return Some(vec![added]);
        }
        let name = mods.iter().find(|info| info.file == file).map(ModInfo::display).unwrap_or_else(|| file.clone());
        disable(instance_dir, &file).ok()?;
        Some(vec![added, Change::Disabled { file, name }])
    }

    /// Swaps a jar installed from Modrinth or CurseForge for the project's newest build for this
    /// instance, keeping the old jar so the swap can be undone.
    async fn replace_for_this_game(&self, instance: &Instance, instance_dir: &Path, file: &str) -> Option<Change> {
        let index = ContentIndex::new(instance_dir);
        let relative = format!("mods/{file}");
        let previous = index.list().into_iter().find(|item| item.file == relative && item.world.is_none())?;
        let game_dir = instance_dir.join(GAME_DIR);
        let backup = format!("{BACKUPS}/{}-{file}", std::process::id());
        let backup_path = instance_dir.join(&backup);
        std::fs::create_dir_all(backup_path.parent()?).ok()?;
        std::fs::copy(game_dir.join(&relative), &backup_path).ok()?;
        let request = InstallRequest { project_id: &previous.project_id, version_id: None, world: None, concurrency: self.concurrency, curseforge_key: self.curseforge_key };
        let installed = content::install(self.client, instance, instance_dir, request).await.ok();
        let current = installed.and_then(|items| items.into_iter().find(|item| item.project_id == previous.project_id));
        match current {
            Some(current) if current.file != previous.file => Some(Change::Replaced { previous: Box::new(previous), backup, current: Box::new(current) }),
            _ => {
                let _ = std::fs::remove_file(&backup_path);
                None
            }
        }
    }

    async fn update_loader(&self, instance: &Instance) -> Option<Change> {
        let versions = loaders::loader_versions(self.client, instance.loader, &instance.game_version).await.ok()?;
        let target = match &instance.loader_version {
            Some(current) => versions.iter().find(|version| version.recommended).filter(|version| loaders::compare_versions(&version.version, current) == Ordering::Greater)?,
            // Already on the recommended one: only a newer (pre-release) build can help.
            None => versions.first().filter(|newest| !newest.recommended)?,
        };
        self.store.update(self.instance_id, |stored| stored.loader_version = Some(target.version.clone())).ok()?;
        Some(Change::LoaderChanged { from: instance.loader_version.clone(), to: target.version.clone() })
    }
}

/// Missing ids one project brings, with every mod that needs them.
struct Family {
    ids: Vec<String>,
    needed_by: Vec<String>,
}

impl Family {
    fn name(&self) -> String {
        dependency_name(&self.ids[0])
    }
}

/// Missing mods gathered by what brings them: one line for Fabric API, not one per module.
fn families(problems: Vec<Problem>) -> Vec<Family> {
    let mut families: Vec<Family> = Vec::new();
    for problem in problems {
        let Problem::Missing { id, needed_by } = problem else {
            continue;
        };
        let fabric = is_fabric_api_part(&id);
        match families.iter_mut().find(|family| if fabric { is_fabric_api_part(&family.ids[0]) } else { family.ids[0] == id }) {
            Some(family) => {
                if !family.ids.contains(&id) {
                    family.ids.push(id);
                }
                for name in needed_by {
                    if !family.needed_by.contains(&name) {
                        family.needed_by.push(name);
                    }
                }
            }
            None => families.push(Family { ids: vec![id], needed_by }),
        }
    }
    families
}

/// The ids of a group still to look for: those nothing answers to now, and those something
/// answered to before any fix (the game missed them all the same). One a fix brought is done.
fn still_missing(ids: &[String], before: &HashSet<String>, now: &HashSet<String>) -> Vec<String> {
    ids.iter().filter(|id| !now.contains(*id) || before.contains(*id)).cloned().collect()
}

/// Why a missing mod wasn't added.
enum Lookup {
    /// Modrinth answered, and nothing it has for this instance answers to the id.
    NotFound,
    /// Modrinth couldn't be asked, or a download didn't finish.
    Unreachable,
}

/// Where the fixer notes the mods it looked for and couldn't find, next to `instance.json`.
const MISSES: &str = "fix-misses.json";
/// How long an empty search is trusted: long enough that Play isn't held up by the same search
/// on every launch, short enough to catch a build published since.
const MISS_FOR_MS: i64 = 24 * 60 * 60 * 1000;

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_millis() as i64)
}

fn read_misses(instance_dir: &Path) -> HashMap<String, i64> {
    std::fs::read(instance_dir.join(MISSES)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

/// True when a search for this ("Fabric 26.2|frozenlib") came up empty within the last day.
fn missed_lately(instance_dir: &Path, key: &str) -> bool {
    read_misses(instance_dir).get(key).is_some_and(|at| now_ms() - at < MISS_FOR_MS)
}

/// Notes that a search came up empty, or forgets it once the mod is found.
fn remember_miss(instance_dir: &Path, key: &str, missed: bool) {
    let mut misses = read_misses(instance_dir);
    let now = now_ms();
    let before = misses.len();
    misses.retain(|_, at| now - *at < MISS_FOR_MS);
    let changed = if missed {
        misses.insert(key.to_string(), now);
        true
    } else {
        misses.remove(key).is_some() || misses.len() != before
    };
    if !changed {
        return;
    }
    let path = instance_dir.join(MISSES);
    if misses.is_empty() {
        let _ = std::fs::remove_file(path);
    } else if let Ok(bytes) = serde_json::to_vec(&misses) {
        let _ = std::fs::write(path, bytes);
    }
}

/// The biggest jar Wisp downloads only to read what it is.
const MAX_GUESS: u64 = 128 * 1024 * 1024;

/// What a jar on the web says about itself, read in memory: how a guess is checked before it
/// goes anywhere near mods/. None when it couldn't be downloaded.
async fn jar_at(client: &HttpClient, url: &str) -> Option<ModInfo> {
    let response = client.get(url).send().await.ok()?.error_for_status().ok()?;
    let mut info = ModInfo::default();
    if response.content_length().is_some_and(|length| length > MAX_GUESS) {
        return Some(info);
    }
    let bytes = response.bytes().await.ok()?;
    if let Ok(mut archive) = zip::ZipArchive::new(Cursor::new(bytes)) {
        read_archive(&mut archive, &mut info, 0);
    }
    Some(info)
}

fn gigabytes(mb: u32) -> String {
    let gb = f64::from(mb) / 1024.0;
    if gb.fract() == 0.0 { format!("{gb:.0} GB") } else { format!("{gb:.1} GB") }
}

/// Modrinth projects that may be the mod with this id: the id as a slug first (with a few
/// well-known names that differ), then a search. None when Modrinth couldn't be asked at all.
async fn candidates(client: &HttpClient, instance: &Instance, id: &str) -> Option<Vec<String>> {
    let Some(loaders) = content::compatible_loaders(instance.loader, ProjectType::Mod) else {
        return Some(Vec::new());
    };
    let mut slugs: Vec<String> = Vec::new();
    for slug in [alias(id).map(str::to_string), Some(id.to_string()), Some(id.replace('_', "-"))].into_iter().flatten() {
        if !slugs.contains(&slug) {
            slugs.push(slug);
        }
    }
    let mut found: Vec<String> = Vec::new();
    for slug in slugs {
        if let Ok(details) = modrinth::project(client, &slug).await
            && details.project_type == ProjectType::Mod
            && details.loaders.iter().any(|loader| loaders.contains(&loader.as_str()))
            && !found.contains(&details.id)
        {
            found.push(details.id);
        }
    }
    let query: Option<SearchQuery> = serde_json::from_value(serde_json::json!({
        "query": id.replace(['-', '_'], " "),
        "projectType": "mod",
        "loaders": loaders,
        "gameVersion": instance.game_version,
        "limit": 4,
    }))
    .ok();
    if let Some(query) = query {
        match modrinth::search(client, &query).await {
            Ok(results) => {
                for hit in results.hits {
                    if !found.contains(&hit.id) {
                        found.push(hit.id);
                    }
                }
            }
            Err(_) if found.is_empty() => return None,
            Err(_) => {}
        }
    }
    found.truncate(5);
    Some(found)
}

/// Mod ids whose Modrinth project goes by another name.
fn alias(id: &str) -> Option<&'static str> {
    if is_fabric_api_part(id) {
        return Some("fabric-api");
    }
    Some(match id {
        "cloth-config2" | "cloth_config" => "cloth-config",
        "architectury" => "architectury-api",
        "kotlinforforge" => "kotlin-for-forge",
        "geckolib3" => "geckolib",
        "owo" => "owo-lib",
        "yet_another_config_lib_v3" | "yet-another-config-lib" => "yacl",
        "puzzleslib" => "puzzles-lib",
        "forgeconfigapiport" => "forge-config-api-port",
        "resourcefullib" => "resourceful-lib",
        "bookshelf" => "bookshelf-lib",
        "prism" => "prism-lib",
        "cardinal-components" | "cardinal-components-base" => "cardinal-components-api",
        _ => return None,
    })
}

/// Records projects a fix added as dependencies rather than things the player picked.
fn mark_dependencies(index: &ContentIndex, installed: &[InstalledContent]) {
    let mut items = index.list();
    for item in items.iter_mut() {
        if installed.iter().any(|added| added.project_id == item.project_id) {
            item.explicit = false;
        }
    }
    let _ = index.save(&items);
}

/// Turns a jar off the way launchers do: mods/<file> becomes mods/<file>.disabled.
fn disable(instance_dir: &Path, file: &str) -> std::io::Result<()> {
    rename_in_mods(instance_dir, file, &format!("{file}.disabled"))
}

fn rename_in_mods(instance_dir: &Path, from: &str, to: &str) -> std::io::Result<()> {
    let plain = |name: &str| !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != "..";
    if !plain(from) || !plain(to) {
        return Err(std::io::Error::other("not a file in mods/"));
    }
    let mods = instance_dir.join(GAME_DIR).join("mods");
    std::fs::rename(mods.join(from), mods.join(to))?;
    // Keep the content list pointing at the file.
    let index = ContentIndex::new(instance_dir);
    let mut items = index.list();
    let (old, new) = (format!("mods/{from}"), format!("mods/{to}"));
    if let Some(item) = items.iter_mut().find(|item| item.file == old) {
        item.file = new;
        let _ = index.save(&items);
    }
    Ok(())
}

/// Does what an offer says. Returns the change, to record (and undo) like the automatic ones.
pub fn apply(store: &InstanceStore, instance_id: &str, action: &Action) -> Result<Change, String> {
    let instance_dir = store.dir_of(instance_id).ok_or("That instance no longer exists.")?;
    match action {
        Action::Disable { file, name } => {
            disable(&instance_dir, file).map_err(|error| format!("Couldn't turn off {name}: {error}"))?;
            Ok(Change::Disabled { file: file.clone(), name: name.clone() })
        }
    }
}

/// Puts back what a fix changed, newest change first.
pub fn undo(store: &InstanceStore, instance_id: &str, changes: &[Change]) -> Result<(), String> {
    let instance_dir = store.dir_of(instance_id).ok_or("That instance no longer exists.")?;
    let game_dir = instance_dir.join(GAME_DIR);
    let index = ContentIndex::new(&instance_dir);
    let mut problems = Vec::new();
    for change in changes.iter().rev() {
        let result: Result<(), String> = match change {
            Change::Disabled { file, .. } => rename_in_mods(&instance_dir, &format!("{file}.disabled"), file).map_err(|error| error.to_string()),
            Change::Added { project_id, .. } => index.remove(project_id, None).map_err(|error| error.to_string()),
            Change::Replaced { previous, backup, current } => (|| {
                let current_path = content::safe_join(&game_dir, &current.file).ok_or("bad path")?;
                let previous_path = content::safe_join(&game_dir, &previous.file).ok_or("bad path")?;
                std::fs::copy(instance_dir.join(backup), &previous_path).map_err(|error| error.to_string())?;
                if current_path != previous_path {
                    let _ = std::fs::remove_file(&current_path);
                }
                let mut items = index.list();
                items.retain(|item| item.project_id != current.project_id || item.world.is_some());
                items.push((**previous).clone());
                index.save(&items).map_err(|error| error.to_string())?;
                let _ = std::fs::remove_file(instance_dir.join(backup));
                Ok(())
            })(),
            Change::LoaderChanged { from, .. } => store.update(instance_id, |stored| stored.loader_version = from.clone()).map(|_| ()).map_err(|error| error.to_string()),
            Change::ConfigMoved { from, to } => match content::safe_join(&game_dir, to).zip(content::safe_join(&game_dir, from)) {
                Some((aside, original)) => std::fs::rename(aside, original).map_err(|error| error.to_string()),
                None => Err("bad path".into()),
            },
            Change::MemoryChanged { from, .. } => store.update(instance_id, |stored| stored.memory_mb = *from).map(|_| ()).map_err(|error| error.to_string()),
            Change::JavaChanged { from, .. } => store.update(instance_id, |stored| stored.java_major = *from).map(|_| ()).map_err(|error| error.to_string()),
        };
        if let Err(error) = result {
            problems.push(error);
        }
    }
    if problems.is_empty() { Ok(()) } else { Err(format!("Some of it couldn't be put back: {}", problems.join("; "))) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn server_jar(path: &Path, entries: &[(&str, &str)]) {
        let mut writer = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, text) in entries {
            writer.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            writer.write_all(text.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn finds_the_game_only_mod_that_crashed_a_server() {
        let dir = std::env::temp_dir().join(format!("wisp-server-doctor-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("mods")).unwrap();
        server_jar(&dir.join("mods/minimap-1.0.jar"), &[("META-INF/mods.toml", "modLoader=\"javafml\"\n[[mods]]\nmodId=\"minimap\"\ndisplayName=\"Minimap\"\n"), ("com/example/minimap/MinimapClient.class", "")]);
        server_jar(&dir.join("mods/zoom.jar"), &[("fabric.mod.json", r#"{"schemaVersion":1,"id":"zoom","name":"Zoom","version":"2"}"#), ("dev/zoom/ZoomClient.class", "")]);
        server_jar(&dir.join("mods/fine.jar"), &[("fabric.mod.json", r#"{"schemaVersion":1,"id":"fine","version":"1"}"#)]);

        // Forge names the mod in the trace.
        let forge: Vec<String> = [
            "[12:00:01] [main/ERROR] [ne.mi.fm.ja.FMLModContainer/LOADING]: Failed to create mod instance. ModID: minimap",
            "java.lang.RuntimeException: Attempted to load class net/minecraft/client/gui/screens/Screen for invalid dist DEDICATED_SERVER",
            "\tat TRANSFORMER/minecraft@1.21.4/net.minecraft.SomeClass.load(SomeClass.java:3)",
            "\tat TRANSFORMER/minimap@1.0/com.example.minimap.MinimapClient.<init>(MinimapClient.java:12)",
        ]
        .map(str::to_string)
        .to_vec();
        let problems = diagnose_server(&dir, Loader::Forge, &forge, i64::MAX);
        assert_eq!(problems.first(), Some(&Problem::ClientOnly { file: "minimap-1.0.jar".into(), name: "Minimap".into() }));

        // Fabric doesn't: the jar holding the first class that isn't Minecraft's or Fabric's.
        let fabric: Vec<String> = [
            "java.lang.RuntimeException: Cannot load class net.minecraft.class_310 in environment type SERVER",
            "\tat net.fabricmc.loader.impl.launch.knot.KnotClassDelegate.tryLoadClass(KnotClassDelegate.java:323)",
            "\tat dev.zoom.ZoomClient$1.onInitialize(ZoomClient.java:9)",
        ]
        .map(str::to_string)
        .to_vec();
        let problems = diagnose_server(&dir, Loader::Fabric, &fabric, i64::MAX);
        assert_eq!(problems, vec![Problem::ClientOnly { file: "zoom.jar".into(), name: "Zoom".into() }]);

        // A crash that has nothing to do with game-only code isn't blamed on one.
        let other: Vec<String> = ["java.lang.IllegalStateException: something else".to_string()].to_vec();
        assert!(!diagnose_server(&dir, Loader::Fabric, &other, i64::MAX).iter().any(|problem| matches!(problem, Problem::ClientOnly { .. })));
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn jar(path: &Path, entries: &[(&str, Vec<u8>)]) {
        let mut writer = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, bytes) in entries {
            writer.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }

    fn jar_bytes(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            writer.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn instance(loader: Loader, loader_version: Option<&str>) -> Instance {
        Instance {
            id: "test".into(),
            name: "Test".into(),
            game_version: "1.21.1".into(),
            loader,
            loader_version: loader_version.map(str::to_string),
            created: 0,
            last_played: None,
            icon: None,
            modpack: None,
            memory_mb: None,
            java_major: None,
            sync_id: None,
        }
    }

    fn temp_instance(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wisp-doctor-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(GAME_DIR).join("mods")).unwrap();
        dir
    }

    #[test]
    fn reads_fabric_quilt_and_forge_metadata_with_bundled_jars() {
        let dir = temp_instance("read");
        let mods = dir.join(GAME_DIR).join("mods");
        let bundled = jar_bytes(&[("fabric.mod.json", br#"{"id": "fabric-networking-api-v1", "version": "1.0"}"#.to_vec())]);
        jar(
            &mods.join("api.jar"),
            &[
                ("fabric.mod.json", br#"{ "id": "fabric-api", "name": "Fabric API", "version": "0.100.0", "provides": ["fabric"], // a comment
                    "jars": [{"file": "META-INF/jars/net.jar"}], }"#.to_vec()),
                ("META-INF/jars/net.jar", bundled),
            ],
        );
        jar(&mods.join("menu.jar"), &[("fabric.mod.json", br#"{"id": "modmenu", "name": "Mod Menu", "version": "11.0.0", "depends": {"fabricloader": ">=0.15.0", "fabric-networking-api-v1": "*", "minecraft": ["1.21", "1.21.1"]}}"#.to_vec())]);
        jar(&mods.join("quilt.jar"), &[("quilt.mod.json", br#"{"quilt_loader": {"id": "qmod", "version": "1", "provides": [{"id": "other"}], "depends": ["qsl", {"id": "maybe", "optional": true}], "metadata": {"name": "Q"}}}"#.to_vec())]);
        jar(
            &mods.join("rei.jar"),
            &[("META-INF/mods.toml", b"modLoader=\"javafml\"\n[[mods]]\nmodId=\"rei\"\nversion=\"${file.jarVersion}\"\ndisplayName=\"REI\"\n[[dependencies.rei]]\nmodId=\"architectury\"\nmandatory=true\nversionRange=\"[9.1,)\"\nside=\"BOTH\"\n[[dependencies.rei]]\nmodId=\"jei\"\ntype=\"optional\"\n".to_vec())],
        );
        let found = scan(&dir);
        let by_file = |file: &str| found.iter().find(|info| info.file == file).unwrap().clone();
        let api = by_file("api.jar");
        assert!(api.answers_to("fabric") && api.answers_to("fabric-networking-api-v1"));
        assert_eq!(api.loaders, vec![Platform::Fabric]);
        let menu = by_file("menu.jar");
        assert_eq!(menu.requires.iter().find(|requirement| requirement.id == "minecraft").unwrap().versions, "1.21 || 1.21.1");
        let quilt = by_file("quilt.jar");
        assert_eq!((quilt.name.as_str(), quilt.requires.len()), ("Q", 1));
        assert!(quilt.answers_to("other"));
        let rei = by_file("rei.jar");
        assert_eq!((rei.id.as_str(), rei.version.as_str(), rei.loaders.clone()), ("rei", "", vec![Platform::Forge]));
        assert_eq!(rei.requires, vec![Requirement { id: "architectury".into(), versions: "[9.1,)".into(), platform: Platform::Forge }]);
        // The second look comes from the cache, which says what reads it.
        assert!(std::fs::read_to_string(dir.join(CACHE)).unwrap().contains(&format!("\"version\":{CACHE_VERSION}")));
        assert_eq!(scan(&dir), found);
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn info(file: &str, id: &str, version: &str, loaders: &[Platform], requires: &[(&str, &str)]) -> ModInfo {
        ModInfo {
            file: file.into(),
            id: id.into(),
            name: id.into(),
            version: version.into(),
            provides: vec![id.into()],
            loaders: loaders.to_vec(),
            requires: requires.iter().map(|(id, range)| Requirement { id: id.to_string(), versions: range.to_string(), platform: loaders.first().copied().unwrap_or(Platform::Fabric) }).collect(),
            client_only: false,
            server_only: false,
        }
    }

    #[test]
    fn checks_loaders_duplicates_dependencies_and_the_loader_version() {
        let fabric = instance(Loader::Fabric, Some("0.14.21"));
        let mods = vec![
            info("sodium-0.5.jar", "sodium", "0.5.0", &[Platform::Fabric], &[("fabricloader", ">=0.15.0")]),
            info("sodium-0.6.jar", "sodium", "0.6.0", &[Platform::Fabric], &[]),
            info("menu.jar", "modmenu", "1", &[Platform::Fabric], &[("fabric-api", "*"), ("minecraft", "1.21.1"), ("java", ">=21")]),
            info("jei-forge.jar", "jei", "1", &[Platform::Forge], &[]),
            info("library.jar", "", "", &[], &[]),
        ];
        let problems = check_mods(&fabric, &mods);
        assert!(problems.contains(&Problem::WrongLoader { file: "jei-forge.jar".into(), name: "jei".into(), made_for: Platform::Forge }));
        assert!(problems.contains(&Problem::Duplicate { name: "sodium".into(), keep: "sodium-0.6.jar".into(), extra: vec!["sodium-0.5.jar".into()] }));
        assert!(problems.contains(&Problem::Missing { id: "fabric-api".into(), needed_by: vec!["modmenu".into()] }));
        // The old copy wanted a newer loader, but it's the one being turned off.
        assert!(!problems.iter().any(|problem| matches!(problem, Problem::LoaderTooOld { .. })));
        assert_eq!(problems.len(), 3);

        let mods = vec![info("menu.jar", "modmenu", "1", &[Platform::Fabric], &[("fabricloader", ">=0.15.0")])];
        assert_eq!(check_mods(&fabric, &mods), vec![Problem::LoaderTooOld { needed_by: "modmenu".into(), wanted: ">=0.15.0".into() }]);
        assert!(check_mods(&instance(Loader::Fabric, Some("0.16.5")), &mods).is_empty());
        assert!(check_mods(&instance(Loader::Fabric, None), &mods).is_empty());
        // Quilt runs Fabric mods; Connector lets NeoForge run them.
        assert!(check_mods(&instance(Loader::Quilt, None), &[info("a.jar", "a", "1", &[Platform::Fabric], &[])]).is_empty());
        let bridged = [info("connector.jar", "connector", "1", &[Platform::NeoForge], &[]), info("a.jar", "a", "1", &[Platform::Fabric], &[])];
        assert!(check_mods(&instance(Loader::NeoForge, None), &bridged).is_empty());
    }

    #[test]
    fn a_universal_jar_needs_only_what_its_loader_reads() {
        let dir = temp_instance("universal");
        let mods = dir.join(GAME_DIR).join("mods");
        jar(&mods.join("api.jar"), &[("fabric.mod.json", br#"{"id": "fabric-api", "version": "0.100.0"}"#.to_vec())]);
        // One jar for Fabric, Forge and NeoForge, each half needing its own loader.
        jar(
            &mods.join("puzzle.jar"),
            &[
                ("fabric.mod.json", br#"{"id": "puzzle", "version": "2.3.1", "depends": {"fabricloader": ">=0.15", "fabric-api": "*", "minecraft": "26.2"}}"#.to_vec()),
                (
                    "META-INF/neoforge.mods.toml",
                    b"modLoader=\"javafml\"\n[[mods]]\nmodId=\"puzzle\"\n[[dependencies.puzzle]]\nmodId=\"neoforge\"\ntype=\"required\"\nversionRange=\"[21,)\"\n[[dependencies.puzzle]]\nmodId=\"puzzleslib\"\ntype=\"required\"\nversionRange=\"[1,)\"\n".to_vec(),
                ),
                ("META-INF/mods.toml", b"modLoader=\"javafml\"\n[[mods]]\nmodId=\"puzzle\"\n[[dependencies.puzzle]]\nmodId=\"forge\"\nmandatory=true\nversionRange=\"[47,)\"\n".to_vec()),
            ],
        );
        let found = scan(&dir);
        assert_eq!(found.iter().find(|info| info.file == "puzzle.jar").unwrap().loaders, vec![Platform::Fabric, Platform::NeoForge, Platform::Forge]);
        assert_eq!(check_mods(&instance(Loader::Fabric, Some("0.16.0")), &found), vec![]);
        // On NeoForge the same jar needs what its NeoForge half says, and nothing of Fabric's.
        let missing: Vec<Problem> = check_mods(&instance(Loader::NeoForge, Some("21.1.0")), &found).into_iter().filter(|problem| matches!(problem, Problem::Missing { .. })).collect();
        assert_eq!(missing, vec![Problem::Missing { id: "puzzleslib".into(), needed_by: vec!["puzzle".into()] }]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_jars_the_way_their_loaders_do() {
        let dir = temp_instance("lenient");
        let mods = dir.join(GAME_DIR).join("mods");
        // A line break typed into the description, a tab and a block comment: Fabric takes all three.
        jar(
            &mods.join("frozenlib.jar"),
            &[("fabric.mod.json", b"{\n  \"id\": \"frozenlib\", /* a library */\n  \"provides\": [\"frozenblocklib\"],\n  \"description\": \"Tools for modders.\nAnd more.\tTabs too.\",\n  \"version\": \"1.9.3\"\n}".to_vec())],
        );
        // One jar for Fabric, Forge and NeoForge whose Forge half lists NeoForge as optional.
        jar(
            &mods.join("packetfixer.jar"),
            &[
                ("fabric.mod.json", br#"{"id": "packetfixer", "version": "3.1.4", "depends": {"fabricloader": "*"}}"#.to_vec()),
                (
                    "META-INF/mods.toml",
                    b"modLoader=\"javafml\"\n[[mods]]\nmodId=\"packetfixer\"\n[[dependencies.packetfixer]]\nmodId=\"forge\"\nmandatory=false\nversionRange=\"[0,)\"\n[[dependencies.packetfixer]]\nmodId=\"neoforge\"\nmandatory=false\nversionRange=\"[0,)\"\n".to_vec(),
                ),
                ("META-INF/neoforge.mods.toml", b"modLoader=\"javafml\"\n[[mods]]\nmodId=\"packetfixer\"\n".to_vec()),
            ],
        );
        let found = scan(&dir);
        let frozen = found.iter().find(|info| info.file == "frozenlib.jar").unwrap();
        assert_eq!((frozen.id.as_str(), frozen.version.as_str()), ("frozenlib", "1.9.3"));
        assert!(frozen.answers_to("frozenblocklib"));
        let fixer = found.iter().find(|info| info.file == "packetfixer.jar").unwrap();
        assert_eq!(fixer.loaders, vec![Platform::Fabric, Platform::NeoForge, Platform::Forge]);
        assert_eq!(check_mods(&instance(Loader::Forge, Some("43.5.1")), std::slice::from_ref(fixer)), vec![]);
        assert_eq!(check_mods(&instance(Loader::Fabric, Some("0.16.0")), &found), vec![]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn remembers_a_search_that_came_up_empty_for_a_day() {
        let dir = temp_instance("misses");
        let key = "Fabric 1.20.1|frozenlib";
        assert!(!missed_lately(&dir, key));
        remember_miss(&dir, key, true);
        assert!(missed_lately(&dir, key));
        assert!(!missed_lately(&dir, "Fabric 1.21.1|frozenlib"));
        remember_miss(&dir, key, false);
        assert!(!missed_lately(&dir, key));
        assert!(!dir.join(MISSES).exists());
        // One from more than a day ago doesn't count.
        std::fs::write(dir.join(MISSES), format!("{{\"{key}\": {}}}", now_ms() - MISS_FOR_MS - 1)).unwrap();
        assert!(!missed_lately(&dir, key));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_loader_is_never_a_missing_mod() {
        let fabric = instance(Loader::Fabric, None);
        let mods = vec![info("odd.jar", "odd", "1", &[Platform::Fabric], &[("forge", "*"), ("neoforge", "[21,)"), ("javafml", "*")])];
        assert_eq!(check_mods(&fabric, &mods), vec![]);
        let lines: Vec<String> = ["\t - Mod 'Odd' (odd) 1 requires any version of neoforge, which is missing!"].iter().map(|line| clean(line)).collect();
        assert_eq!(diagnose_lines(&fabric, &lines, &mods), vec![]);
    }

    #[test]
    fn gathers_what_is_missing_by_what_brings_it() {
        let fabric = instance(Loader::Fabric, None);
        let mods = vec![
            info("continuity.jar", "continuity", "1", &[Platform::Fabric], &[("fabric-api", "*")]),
            info("optigui.jar", "optigui", "1", &[Platform::Fabric], &[("fabric-networking-api-v1", "*"), ("fabric-api", "*")]),
            info("menu.jar", "modmenu", "1", &[Platform::Fabric], &[("fabric-api", "*"), ("cloth-config", "*")]),
        ];
        let problems = check_mods(&fabric, &mods);
        assert_eq!(
            problems,
            vec![
                Problem::Missing { id: "fabric-api".into(), needed_by: vec!["continuity".into(), "optigui".into(), "modmenu".into()] },
                Problem::Missing { id: "fabric-networking-api-v1".into(), needed_by: vec!["optigui".into()] },
                Problem::Missing { id: "cloth-config".into(), needed_by: vec!["modmenu".into()] },
            ]
        );
        assert_eq!(problems[0].describe(Loader::Fabric), "continuity, optigui and modmenu need Fabric API, which isn't installed.");
        assert_eq!(problems[2].describe(Loader::Fabric), "modmenu needs cloth-config, which isn't installed.");
        // Fabric API and its modules are one thing to fix.
        assert_eq!(count(&problems), 2);
        let grouped = families(problems);
        assert_eq!(grouped.len(), 2);
        assert_eq!((grouped[0].ids.clone(), grouped[0].name()), (vec!["fabric-api".to_string(), "fabric-networking-api-v1".to_string()], "Fabric API".to_string()));
        assert_eq!(grouped[0].needed_by, vec!["continuity", "optigui", "modmenu"]);
        assert_eq!(grouped[1].name(), "cloth-config");
        assert_eq!(names(&["a".into(), "b".into(), "c".into(), "d".into()]), "a, b and 2 other mods");

        // The crash names each module apart; they still come out as one.
        let lines: Vec<String> = [
            "\t - Mod 'Continuity' (continuity) 3.0.1 requires any version of fabric-api, which is missing!",
            "\t - Mod 'OptiGUI' (optigui) 2.3.0 requires any version of fabric-networking-api-v1, which is missing!",
            "\t - Mod 'LambDynamicLights' (lambdynlights) 4.1 requires any version of fabric-api, which is missing!",
        ]
        .iter()
        .map(|line| clean(line))
        .collect();
        let problems = diagnose_lines(&fabric, &lines, &[]);
        assert_eq!(problems[0], Problem::Missing { id: "fabric-api".into(), needed_by: vec!["Continuity".into(), "LambDynamicLights".into()] });
        assert_eq!(count(&problems), 1);

        // Once a fix brings Fabric API, its modules are done; what the game missed though a jar
        // claimed it before any fix still gets a look.
        let set = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<HashSet<String>>();
        let ids = vec!["fabric-networking-api-v1".to_string(), "cloth-config".to_string(), "sodium".to_string()];
        assert_eq!(still_missing(&ids, &set(&["sodium"]), &set(&["fabric-networking-api-v1", "sodium"])), vec!["cloth-config", "sodium"]);
    }

    #[test]
    fn reads_version_requirements() {
        assert!(below_minimum(">=0.15.0", "0.14.21"));
        assert!(!below_minimum(">=0.15.0", "0.15.0"));
        assert!(!below_minimum(">=0.15.0 <0.17", "0.16.1"));
        assert!(below_minimum(">0.15.0", "0.15.0"));
        assert!(!below_minimum("*", "0.1"));
        assert!(!below_minimum("0.15.x", "0.14.0"));
        assert!(!below_minimum(">=0.17 || >=0.14.0", "0.15.0"));
        assert!(below_minimum("[47.2.0,)", "47.1.3"));
        assert!(!below_minimum("[47.2.0,)", "47.2.0"));
        assert!(below_minimum("(47.2.0,)", "47.2.0"));
        assert!(!below_minimum("[47,48)", "47.3.0"));
        assert!(!below_minimum("", "1"));
    }

    #[test]
    fn reads_what_fabric_forge_and_java_say_went_wrong() {
        let mods = vec![info("menu.jar", "modmenu", "11", &[Platform::Fabric], &[]), info("old.jar", "oldmod", "1", &[Platform::Fabric], &[]), info("optifabric.jar", "optifabric", "1", &[Platform::Fabric], &[])];
        let fabric = instance(Loader::Fabric, Some("0.14.21"));
        let lines: Vec<String> = [
            "net.fabricmc.loader.impl.FormattedException: Mod resolution encountered an incompatible mod set!",
            "\t - Mod 'Mod Menu' (modmenu) 11.0.0 requires any version of fabric-api, which is missing!",
            "\t - Mod 'Mod Menu' (modmenu) 11.0.0 requires version 0.16.0 or later of fabricloader, but only the wrong version is present: 0.14.21!",
            "\t - Mod 'Old Mod' (oldmod) 1.0 requires version 1.20.1 of minecraft, but only the wrong version is present: 1.21.1!",
            "\t - Mod 'Mod Menu' (modmenu) 11.0.0 requires version 2.0 or later of mod 'Cloth Config' (cloth-config), but only the wrong version is present: 1.0!",
            "\t - Remove mod 'OptiFabric' (optifabric) 1.13.0.",
        ]
        .iter()
        .map(|line| clean(line))
        .collect();
        let problems = diagnose_lines(&fabric, &lines, &mods);
        assert_eq!(
            problems,
            vec![
                Problem::Missing { id: "fabric-api".into(), needed_by: vec!["Mod Menu".into()] },
                Problem::LoaderTooOld { needed_by: "Mod Menu".into(), wanted: "version 0.16.0 or later".into() },
                Problem::WrongGame { file: "old.jar".into(), name: "Old Mod".into(), wanted: "1.20.1".into() },
                Problem::WrongVersion { id: "cloth-config".into(), needed_by: "Mod Menu".into(), wanted: "version 2.0 or later".into(), found: "1.0".into() },
                Problem::Incompatible { file: "optifabric.jar".into(), name: "OptiFabric".into(), with: "another mod".into() },
            ]
        );

        let forge = instance(Loader::Forge, Some("47.1.0"));
        let lines: Vec<String> = [
            "Missing or unsupported mandatory dependencies:",
            "\tMod ID: 'architectury', Requested by: 'rei', Expected range: '[9.1.12,)', Actual version: '[MISSING]'",
            "\tMod ID: 'forge', Requested by: 'rei', Expected range: '[47.2,)', Actual version: '47.1.0'",
            "java.lang.UnsupportedClassVersionError: x has been compiled by a more recent version of the Java Runtime (class file version 65.0), this version of the Java Runtime only recognizes class file versions up to 61.0",
            "java.lang.OutOfMemoryError: Java heap space",
            "Failed loading config file jei-client.toml of type CLIENT for modid jei",
            "com.electronwill.nightconfig.core.io.ParsingException: Not enough data available",
        ]
        .iter()
        .map(|line| clean(line))
        .collect();
        let problems = diagnose_lines(&forge, &lines, &[]);
        assert!(problems.contains(&Problem::Missing { id: "architectury".into(), needed_by: vec!["rei".into()] }));
        assert!(problems.contains(&Problem::LoaderTooOld { needed_by: "rei".into(), wanted: "[47.2,)".into() }));
        assert!(problems.contains(&Problem::NeedsJava { major: 21 }));
        assert!(problems.contains(&Problem::OutOfMemory));
        assert!(problems.contains(&Problem::BrokenConfig { path: "config/jei-client.toml".into() }));

        let neo = instance(Loader::NeoForge, None);
        let lines: Vec<String> = ["Mod §eJust Enough Items§r requires §6architectury§r 13.0.0 or above", "§rCurrently, §6architectury§r is §cnot installed"].iter().map(|line| clean(line)).collect();
        assert_eq!(diagnose_lines(&neo, &lines, &[]), vec![Problem::Missing { id: "architectury".into(), needed_by: vec!["Just Enough Items".into()] }]);

        // With nothing clearer, the suspect a crash report names.
        let mods = vec![info("sodium.jar", "sodium", "1", &[Platform::Fabric], &[])];
        let lines = vec![clean("Suspected Mods: Fabric API (fabric-api), Sodium (sodium)")];
        assert_eq!(diagnose_lines(&fabric, &lines, &mods), vec![Problem::Culprit { file: "sodium.jar".into(), name: "sodium".into() }]);
    }

    #[test]
    fn turns_off_and_undoes_changes() {
        let root = std::env::temp_dir().join(format!("wisp-doctor-undo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store = InstanceStore::new(&root);
        let made = store
            .create(crate::instances::NewInstance { name: "Undo".into(), game_version: "1.21.1".into(), loader: Loader::Fabric, loader_version: None, icon: None }, Some("0.15.0".into()), 0)
            .unwrap();
        let instance_dir = store.dir_of(&made.id).unwrap();
        let mods = instance_dir.join(GAME_DIR).join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("a.jar"), b"jar").unwrap();
        let config = instance_dir.join(GAME_DIR).join("config");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(config.join("x.toml.broken"), b"").unwrap();

        let disabled = apply(&store, &made.id, &Action::Disable { file: "a.jar".into(), name: "A".into() }).unwrap();
        assert!(mods.join("a.jar.disabled").exists() && !mods.join("a.jar").exists());
        store.update(&made.id, |stored| {
            stored.memory_mb = Some(6144);
            stored.loader_version = Some("0.16.0".into());
        })
        .unwrap();
        let changes = vec![
            disabled,
            Change::MemoryChanged { from: None, to: 6144 },
            Change::LoaderChanged { from: Some("0.15.0".into()), to: "0.16.0".into() },
            Change::ConfigMoved { from: "config/x.toml".into(), to: "config/x.toml.broken".into() },
        ];
        undo(&store, &made.id, &changes).unwrap();
        assert!(mods.join("a.jar").exists());
        assert!(config.join("x.toml").exists());
        let restored = store.get(&made.id).unwrap();
        assert_eq!((restored.memory_mb, restored.loader_version.as_deref()), (None, Some("0.15.0")));
        assert!(apply(&store, &made.id, &Action::Disable { file: "../escape.jar".into(), name: "X".into() }).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn knows_where_mod_ids_live_on_modrinth() {
        assert_eq!(alias("fabric-networking-api-v1"), Some("fabric-api"));
        assert_eq!(alias("fabric"), Some("fabric-api"));
        assert_eq!(alias("fabric-language-kotlin"), None);
        assert_eq!(alias("architectury"), Some("architectury-api"));
        assert_eq!(gigabytes(6144), "6 GB");
        assert_eq!(gigabytes(2560), "2.5 GB");
    }
}
