//! Files you bring in yourself: modpacks (Modrinth's `.mrpack`, CurseForge's zip), mods (`.jar`),
//! resource packs, shader packs and data packs (`.zip`), and worlds. A file is told apart by
//! what's inside it rather than its name, and looked up on Modrinth by its SHA-1, so one that's
//! published there shows its real name and icon and updates like anything from Discover.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::HttpClient;
use crate::content::{ContentError, ContentIndex, InstalledContent, compatible_loaders};
use crate::instances::{GAME_DIR, Instance, Loader};
use crate::modpack::{self, PackSummary};
use crate::modrinth::{self, ProjectType};

/// What a file turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileKind {
    /// Becomes an instance of its own.
    Modpack,
    Mod,
    ResourcePack,
    ShaderPack,
    /// Goes into a world.
    DataPack,
    /// A world, zipped or as its folder.
    World,
    /// Nothing Wisp knows what to do with.
    Unknown,
}

/// A file looked over before it's brought in.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspected {
    pub path: String,
    pub file_name: String,
    pub kind: FileKind,
    /// From the file's own metadata, when it has some.
    pub name: Option<String>,
    pub version: Option<String>,
    /// For a mod: the loaders it's made for ("fabric", "quilt", "forge", "neoforge").
    pub loaders: Vec<String>,
    /// For a modpack: what it makes.
    pub pack: Option<PackSummary>,
    /// Why a modpack can't be read, when it can't.
    pub problem: Option<String>,
}

/// Looks a file over without changing anything.
pub fn inspect(path: &Path) -> Inspected {
    let file_name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let mut inspected = Inspected {
        path: path.to_string_lossy().into_owned(),
        file_name,
        kind: FileKind::Unknown,
        name: None,
        version: None,
        loaders: Vec::new(),
        pack: None,
        problem: None,
    };
    if path.is_dir() {
        if path.join("level.dat").is_file() {
            inspected.kind = FileKind::World;
        }
        return inspected;
    }
    let Ok(mut archive) = std::fs::File::open(path).map_err(drop).and_then(|file| zip::ZipArchive::new(file).map_err(drop)) else {
        return inspected;
    };
    match modpack::summarize(&mut archive) {
        Some(Ok(summary)) => {
            inspected.kind = FileKind::Modpack;
            inspected.name = summary.name.clone();
            inspected.version = summary.version.clone();
            inspected.pack = Some(summary);
            return inspected;
        }
        Some(Err(error)) => {
            inspected.kind = FileKind::Modpack;
            inspected.problem = Some(error.to_string());
            return inspected;
        }
        None => {}
    }
    let names: Vec<String> = archive.file_names().filter_map(Result::ok).map(|name| name.into_owned()).collect();
    let has = |wanted: &str| names.iter().any(|name| name == wanted);
    // A folder at the top, or one level down (zips often wrap everything in one folder).
    let near_top = |wanted: &str| {
        names.iter().any(|name| {
            let mut parts = name.split('/');
            let first = parts.next();
            first == Some(wanted) || (parts.next() == Some(wanted) && name.matches('/').count() >= 2)
        })
    };

    // Mods, by their loader's metadata.
    if has("fabric.mod.json") {
        inspected.loaders.push("fabric".into());
        if let Some(json) = read_text(&mut archive, "fabric.mod.json").and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok()) {
            inspected.name = text_field(&json, &["name"]).or_else(|| text_field(&json, &["id"]));
            inspected.version = text_field(&json, &["version"]);
        }
    }
    if has("quilt.mod.json") {
        inspected.loaders.push("quilt".into());
        if let Some(json) = read_text(&mut archive, "quilt.mod.json").and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok()) {
            inspected.name = inspected.name.take().or_else(|| text_field(&json, &["quilt_loader", "metadata", "name"]));
            inspected.version = inspected.version.take().or_else(|| text_field(&json, &["quilt_loader", "version"]));
        }
    }
    for (file, loader) in [("META-INF/neoforge.mods.toml", "neoforge"), ("META-INF/mods.toml", "forge")] {
        if has(file) {
            inspected.loaders.push(loader.into());
            if let Some(text) = read_text(&mut archive, file) {
                inspected.name = inspected.name.take().or_else(|| toml_value(&text, "displayName"));
                inspected.version = inspected.version.take().or_else(|| toml_value(&text, "version"));
            }
        }
    }
    if has("mcmod.info") && !inspected.loaders.iter().any(|loader| loader == "forge") {
        inspected.loaders.push("forge".into());
    }
    let jar = path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("jar"));
    inspected.kind = if !inspected.loaders.is_empty() || jar {
        FileKind::Mod
    } else if names.iter().any(|name| name == "level.dat" || (name.ends_with("/level.dat") && name.matches('/').count() == 1)) {
        FileKind::World
    } else if near_top("shaders") {
        FileKind::ShaderPack
    } else if has("pack.mcmeta") {
        // Both kinds of pack have a pack.mcmeta; a data pack keeps everything under data/.
        if near_top("assets") || !near_top("data") { FileKind::ResourcePack } else { FileKind::DataPack }
    } else {
        FileKind::Unknown
    };
    if inspected.version.as_deref().is_some_and(|version| version.contains("${")) {
        inspected.version = None;
    }
    inspected
}

fn read_text(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Option<String> {
    let mut entry = archive.by_name(name).ok()?;
    let mut text = String::new();
    entry.by_ref().take(1 << 20).read_to_string(&mut text).ok()?;
    Some(text)
}

fn text_field(json: &serde_json::Value, path: &[&str]) -> Option<String> {
    let mut value = json;
    for key in path {
        value = value.get(key)?;
    }
    value.as_str().map(str::trim).filter(|text| !text.is_empty()).map(str::to_string)
}

/// The first `key = "value"` in a mods.toml (enough for a name and version, without a TOML parser).
fn toml_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (name, value) = line.trim().split_once('=')?;
        (name.trim() == key).then(|| value.trim().trim_matches(['"', '\'']).trim().to_string()).filter(|value| !value.is_empty())
    })
}

/// What adding a file did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Added {
    pub content: InstalledContent,
    /// Worth knowing, like a Fabric mod going into a Forge instance.
    pub note: Option<String>,
}

/// Copies a mod, resource pack or shader pack into an instance and records it with what it came
/// from on Modrinth, if it's there. A newer copy of something already installed replaces it.
pub async fn add_to_instance(client: &HttpClient, instance: &Instance, instance_dir: &Path, path: &Path) -> Result<Added, ContentError> {
    let inspected = inspect(path);
    let (folder, project_type) = match inspected.kind {
        FileKind::Mod => ("mods", ProjectType::Mod),
        FileKind::ResourcePack => ("resourcepacks", ProjectType::Resourcepack),
        FileKind::ShaderPack => ("shaderpacks", ProjectType::Shader),
        FileKind::Modpack => return Err(ContentError::IsModpack),
        FileKind::DataPack => return Err(ContentError::IsDatapack(inspected.file_name)),
        FileKind::World => return Err(ContentError::IsWorld(inspected.file_name)),
        FileKind::Unknown => return Err(ContentError::NotContent(inspected.file_name)),
    };
    if project_type == ProjectType::Mod && instance.loader == Loader::Vanilla {
        return Err(ContentError::NeedsLoader);
    }

    let sha1 = crate::snapshots::hash_file(path)?;
    let destination = place(&instance_dir.join(GAME_DIR).join(folder), &inspected.file_name, &sha1)?;
    if !same_file(path, &destination) {
        std::fs::copy(path, &destination)?;
    }
    let stored = destination.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();

    let published = match modrinth::version_by_hash(client, &sha1).await {
        Ok(Some(version)) => modrinth::project(client, &version.project_id).await.ok().map(|project| (project, version)),
        _ => None,
    };
    let stem = Path::new(&inspected.file_name).file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    let content = match published {
        Some((project, version)) => InstalledContent {
            project_id: project.id,
            version_id: version.id,
            title: project.title,
            version_number: version.version_number,
            project_type,
            file: format!("{folder}/{stored}"),
            icon_url: project.icon_url,
            explicit: true,
            world: None,
        },
        None => InstalledContent {
            project_id: format!("file:{}", &sha1[..12]),
            version_id: sha1.clone(),
            title: inspected.name.clone().unwrap_or(stem),
            version_number: inspected.version.clone().unwrap_or_default(),
            project_type,
            file: format!("{folder}/{stored}"),
            icon_url: None,
            explicit: true,
            world: None,
        },
    };

    let index = ContentIndex::new(instance_dir);
    let mut items = index.list();
    let same = |item: &InstalledContent| item.project_id == content.project_id && item.world.is_none();
    for older in items.iter().filter(|item| same(item) && item.file != content.file) {
        if let Some(old) = crate::content::safe_join(&instance_dir.join(GAME_DIR), &older.file) {
            let _ = std::fs::remove_file(old);
        }
    }
    items.retain(|item| !same(item));
    items.push(content.clone());
    index.save(&items)?;

    let fits = compatible_loaders(instance.loader, ProjectType::Mod).unwrap_or_default();
    let note = (project_type == ProjectType::Mod && !inspected.loaders.is_empty() && !inspected.loaders.iter().any(|loader| fits.contains(&loader.as_str())))
        .then(|| format!("{} is made for {}, and this instance runs {}: it may not load.", content.title, list(&inspected.loaders), instance.loader.display_name()));
    Ok(Added { content, note })
}

/// Where a file goes in a folder: its own name, unless a different file has it already.
fn place(dir: &Path, file_name: &str, sha1: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let clean: String = file_name.chars().map(|c| if matches!(c, '/' | '\\' | ':') { '_' } else { c }).collect();
    let clean = clean.trim_start_matches('.').to_string();
    let wanted = dir.join(&clean);
    if !wanted.exists() || crate::snapshots::hash_file(&wanted).ok().as_deref() == Some(sha1) {
        return Ok(wanted);
    }
    let path = Path::new(&clean);
    let stem = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    let extension = path.extension().map(|extension| format!(".{}", extension.to_string_lossy())).unwrap_or_default();
    Ok((2..).map(|n| dir.join(format!("{stem} ({n}){extension}"))).find(|candidate| !candidate.exists()).expect("some name is free"))
}

fn same_file(a: &Path, b: &Path) -> bool {
    matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b)
}

fn list(loaders: &[String]) -> String {
    let names: Vec<&str> = loaders
        .iter()
        .map(|loader| match loader.as_str() {
            "fabric" => "Fabric",
            "quilt" => "Quilt",
            "forge" => "Forge",
            "neoforge" => "NeoForge",
            other => other,
        })
        .collect();
    names.join(" or ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_with(dir: &Path, name: &str, files: &[(&str, &str)]) -> PathBuf {
        let path = dir.join(name);
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        for (file, text) in files {
            writer.start_file(*file, zip::write::SimpleFileOptions::default()).unwrap();
            writer.write_all(text.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
        path
    }

    #[test]
    fn tells_files_apart_by_whats_inside() {
        let dir = std::env::temp_dir().join(format!("wisp-imports-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let fabric = inspect(&zip_with(&dir, "sodium.jar", &[("fabric.mod.json", r#"{"id":"sodium","name":"Sodium","version":"0.6.0"}"#)]));
        assert_eq!((fabric.kind, fabric.name.as_deref(), fabric.version.as_deref()), (FileKind::Mod, Some("Sodium"), Some("0.6.0")));
        assert_eq!(fabric.loaders, ["fabric"]);

        let forge = inspect(&zip_with(&dir, "jei.jar", &[("META-INF/mods.toml", "modLoader=\"javafml\"\n[[mods]]\nmodId=\"jei\"\nversion=\"${file.jarVersion}\"\ndisplayName=\"Just Enough Items\"\n")]));
        assert_eq!((forge.kind, forge.name.as_deref(), forge.version), (FileKind::Mod, Some("Just Enough Items"), None));
        assert_eq!(forge.loaders, ["forge"]);

        let pack = inspect(&zip_with(&dir, "faithful.zip", &[("pack.mcmeta", "{}"), ("assets/minecraft/textures/a.png", "")]));
        assert_eq!(pack.kind, FileKind::ResourcePack);
        let data = inspect(&zip_with(&dir, "trees.zip", &[("pack.mcmeta", "{}"), ("data/trees/a.json", "{}")]));
        assert_eq!(data.kind, FileKind::DataPack);
        let shaders = inspect(&zip_with(&dir, "bsl.zip", &[("BSL/shaders/final.fsh", "")]));
        assert_eq!(shaders.kind, FileKind::ShaderPack);
        let world = inspect(&zip_with(&dir, "island.zip", &[("Island/level.dat", "")]));
        assert_eq!(world.kind, FileKind::World);

        let mrpack = inspect(&zip_with(
            &dir,
            "pack.mrpack",
            &[("modrinth.index.json", r#"{"formatVersion":1,"game":"minecraft","name":"My Pack","versionId":"1.2","files":[],"dependencies":{"minecraft":"1.21.1","fabric-loader":"0.16.10"}}"#)],
        ));
        assert_eq!((mrpack.kind, mrpack.name.as_deref()), (FileKind::Modpack, Some("My Pack")));
        assert_eq!(mrpack.pack.map(|pack| (pack.game_version, pack.loader)), Some(("1.21.1".into(), Loader::Fabric)));

        let curse = inspect(&zip_with(
            &dir,
            "curse.zip",
            &[("manifest.json", r#"{"minecraft":{"version":"1.20.1","modLoaders":[{"id":"forge-47.2.0","primary":true}]},"name":"Curse Pack","version":"3","files":[],"overrides":"overrides"}"#)],
        ));
        assert_eq!((curse.kind, curse.name.as_deref()), (FileKind::Modpack, Some("Curse Pack")));

        let junk = inspect(&zip_with(&dir, "notes.zip", &[("readme.txt", "hi")]));
        assert_eq!(junk.kind, FileKind::Unknown);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn keeps_a_different_file_with_the_same_name() {
        let dir = std::env::temp_dir().join(format!("wisp-place-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.jar"), "old").unwrap();
        let sha_old = crate::snapshots::hash_file(&dir.join("a.jar")).unwrap();
        assert_eq!(place(&dir, "a.jar", &sha_old).unwrap(), dir.join("a.jar"));
        assert_eq!(place(&dir, "a.jar", "other").unwrap(), dir.join("a (2).jar"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
