//! Installing a modpack as a new instance: read its index (Modrinth's `.mrpack` or CurseForge's
//! zip with a `manifest.json`), make an instance with the pack's Minecraft version and loader,
//! download every file it lists (checked against its SHA-1) and unpack its overrides (configs and
//! the like). The pack comes from Modrinth or CurseForge, or is a file you already have.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::HttpClient;
use crate::content::safe_join;
use crate::curseforge;
use crate::inventory;
use crate::instances::{
    GAME_DIR, Instance, InstanceError, InstanceStore, Loader, ModpackOrigin, NewInstance,
};
use crate::launch::{Download, DownloadError, Progress, download_all};
use crate::modrinth::{self, ProjectType};

/// Progress of a modpack install, sent to the UI.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "phase",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum InstallUpdate {
    Preparing,
    Downloading {
        done_bytes: u64,
        total_bytes: u64,
        done_files: u64,
        total_files: u64,
    },
    Unpacking,
    Done {
        instance_id: String,
        /// CurseForge files the pack lists that their authors don't let other apps download.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        skipped: Vec<String>,
    },
    Failed {
        message: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ModpackError {
    #[error("That project isn't a modpack.")]
    NotAModpack,
    #[error("This modpack has no version to install.")]
    NoVersion,
    #[error("This modpack's file isn't one Wisp can read: {0}")]
    BadPack(String),
    #[error("This modpack needs a loader Wisp doesn't support yet ({0}).")]
    UnknownLoader(String),
    #[error("This modpack's author only allows downloads from the CurseForge website.")]
    NotDistributable,
    #[error("CurseForge needs an API key. Add one in Settings → Mod sources.")]
    NoCurseForgeKey,
    #[error("{0}")]
    Network(#[from] crate::Error),
    #[error(transparent)]
    Download(#[from] DownloadError),
    #[error(transparent)]
    Instance(#[from] InstanceError),
    #[error("Couldn't save the files: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackIndex {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version_id: Option<String>,
    files: Vec<PackFile>,
    dependencies: HashMap<String, String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackFile {
    path: String,
    #[serde(default)]
    hashes: HashMap<String, String>,
    #[serde(default)]
    env: Option<PackEnv>,
    #[serde(default)]
    downloads: Vec<String>,
    #[serde(default)]
    file_size: Option<u64>,
}

#[derive(Deserialize)]
struct PackEnv {
    #[serde(default)]
    client: Option<String>,
}

/// The pack's Minecraft version and loader, from the index's `dependencies`.
fn target(
    dependencies: &HashMap<String, String>,
) -> Result<(String, Loader, Option<String>), ModpackError> {
    let game = dependencies.get("minecraft").cloned().ok_or_else(|| {
        ModpackError::BadPack("it doesn't say which Minecraft version it's for".into())
    })?;
    let loaders = [
        ("fabric-loader", Loader::Fabric),
        ("quilt-loader", Loader::Quilt),
        ("neoforge", Loader::NeoForge),
        ("forge", Loader::Forge),
    ];
    for (key, loader) in loaders {
        if let Some(version) = dependencies.get(key) {
            return Ok((game, loader, Some(version.clone())));
        }
    }
    if let Some(other) = dependencies.keys().find(|key| key.as_str() != "minecraft") {
        return Err(ModpackError::UnknownLoader(other.clone()));
    }
    Ok((game, Loader::Vanilla, None))
}

/// Which modpack to install, and how.
pub struct Request<'a> {
    /// A Modrinth id or slug, or "curseforge:<id>".
    pub project_id: &'a str,
    /// A specific version; left out, the newest release.
    pub version_id: Option<&'a str>,
    /// Where downloaded `.mrpack` files are kept.
    pub cache_dir: &'a Path,
    pub concurrency: usize,
    pub now_ms: i64,
    /// Needed for CurseForge modpacks.
    pub curseforge_key: Option<&'a str>,
}

/// A finished install.
pub struct Installed {
    pub instance: Instance,
    /// Files that couldn't be downloaded (see `InstallUpdate::Done`).
    pub skipped: Vec<String>,
}

/// Installs a modpack as a new instance. On failure, the half-made instance is removed again.
pub async fn install(
    client: &HttpClient,
    instances: &InstanceStore,
    request: Request<'_>,
    report: &(dyn Fn(InstallUpdate) + Send + Sync),
) -> Result<Installed, ModpackError> {
    if curseforge::is_curseforge(request.project_id) {
        return install_curseforge(client, instances, request, report).await;
    }
    let Request {
        project_id,
        version_id,
        cache_dir,
        concurrency,
        now_ms,
        ..
    } = request;
    report(InstallUpdate::Preparing);
    let project = modrinth::project(client, project_id).await?;
    if project.project_type != ProjectType::Modpack {
        return Err(ModpackError::NotAModpack);
    }
    let version = match version_id {
        Some(id) => modrinth::version(client, id).await?,
        None => {
            let versions = modrinth::project_versions(client, &project.id, &[], &[]).await?;
            let release = versions
                .iter()
                .position(|version| version.version_type == "release");
            match release {
                Some(index) => versions.into_iter().nth(index),
                None => versions.into_iter().next(),
            }
            .ok_or(ModpackError::NoVersion)?
        }
    };
    let file = version.primary_file().ok_or(ModpackError::NoVersion)?;
    let pack_path = cache_dir.join(format!("{}.mrpack", version.id));
    if !pack_path.is_file() {
        let download = Download {
            url: file.url.clone(),
            path: pack_path.clone(),
            sha1: file.sha1.clone(),
            size: Some(file.size),
            executable: false,
        };
        download_with_progress(client, vec![download], concurrency, report).await?;
    }

    let pack = std::fs::File::open(&pack_path)?;
    let mut archive =
        zip::ZipArchive::new(pack).map_err(|error| ModpackError::BadPack(error.to_string()))?;
    let index: PackIndex = {
        let mut entry = archive
            .by_name("modrinth.index.json")
            .map_err(|_| ModpackError::BadPack("no modrinth.index.json".into()))?;
        let mut text = String::new();
        entry.read_to_string(&mut text)?;
        serde_json::from_str(&text).map_err(|error| ModpackError::BadPack(error.to_string()))?
    };
    let (game_version, loader, loader_version) = target(&index.dependencies)?;

    let instance = instances.create_with(
        NewInstance {
            name: project.title.clone(),
            game_version,
            loader,
            loader_version: loader_version.clone(),
            icon: project.icon_url.clone(),
        },
        loader_version,
        Some(ModpackOrigin {
            project_id: project.id.clone(),
            version_id: version.id.clone(),
            title: project.title.clone(),
            version_number: version.version_number.clone(),
        }),
        now_ms,
    )?;

    let result = fill(
        client,
        instances,
        &instance,
        &index,
        &mut archive,
        concurrency,
        report,
    )
    .await;
    if let Err(error) = result {
        let _ = instances.delete(&instance.id);
        return Err(error);
    }
    Ok(Installed {
        instance,
        skipped: Vec::new(),
    })
}

/// What a modpack file makes, read from its index without installing anything.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackSummary {
    pub name: Option<String>,
    pub version: Option<String>,
    pub game_version: String,
    pub loader: Loader,
    pub from: PackFormat,
}

/// Whose format a modpack file is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackFormat {
    Modrinth,
    CurseForge,
}

fn read_json<T: serde::de::DeserializeOwned>(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Result<T, ModpackError> {
    let mut entry = archive
        .by_name(name)
        .map_err(|_| ModpackError::BadPack(format!("no {name}")))?;
    let mut text = String::new();
    entry.read_to_string(&mut text)?;
    serde_json::from_str(&text).map_err(|error| ModpackError::BadPack(error.to_string()))
}

/// Reads a modpack file's index: None when it isn't a modpack at all.
pub fn summarize(archive: &mut zip::ZipArchive<std::fs::File>) -> Option<Result<PackSummary, ModpackError>> {
    if archive.index_for_name("modrinth.index.json").is_some() {
        return Some(read_json::<PackIndex>(archive, "modrinth.index.json").and_then(|index| {
            let (game_version, loader, _) = target(&index.dependencies)?;
            Ok(PackSummary { name: index.name, version: index.version_id, game_version, loader, from: PackFormat::Modrinth })
        }));
    }
    if archive.index_for_name("manifest.json").is_some() {
        // Other things ship a manifest.json too; a CurseForge pack's names a Minecraft version.
        let manifest = read_json::<CurseManifest>(archive, "manifest.json").ok()?;
        return Some(curse_target(&manifest.minecraft).map(|(loader, _)| PackSummary {
            name: manifest.name,
            version: manifest.version,
            game_version: manifest.minecraft.version,
            loader,
            from: PackFormat::CurseForge,
        }));
    }
    None
}

/// A modpack file to install as a new instance: Modrinth's `.mrpack`, or a CurseForge zip.
pub struct FileRequest<'a> {
    pub path: &'a Path,
    pub concurrency: usize,
    pub now_ms: i64,
    /// Needed for a CurseForge pack, whose files come from CurseForge.
    pub curseforge_key: Option<&'a str>,
}

/// Installs a modpack file as a new instance. A pack published on Modrinth (found by the file's
/// SHA-1) is linked to its project, so it shows its icon and can update like one installed from
/// Discover; any other keeps the name in its index. On failure, the half-made instance goes.
pub async fn install_file(
    client: &HttpClient,
    instances: &InstanceStore,
    request: FileRequest<'_>,
    report: &(dyn Fn(InstallUpdate) + Send + Sync),
) -> Result<Installed, ModpackError> {
    report(InstallUpdate::Preparing);
    let mut archive = zip::ZipArchive::new(std::fs::File::open(request.path)?)
        .map_err(|error| ModpackError::BadPack(error.to_string()))?;
    let summary = summarize(&mut archive).ok_or_else(|| {
        ModpackError::BadPack("it has neither Modrinth's modrinth.index.json nor CurseForge's manifest.json".into())
    })??;
    let file_name = request
        .path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Modpack".into());
    let name = summary.name.clone().filter(|name| !name.trim().is_empty()).unwrap_or(file_name);

    if summary.from == PackFormat::CurseForge {
        let key = request.curseforge_key.ok_or(ModpackError::NoCurseForgeKey)?;
        let manifest: CurseManifest = read_json(&mut archive, "manifest.json")?;
        let (loader, loader_version) = curse_target(&manifest.minecraft)?;
        let instance = instances.create_with(
            NewInstance { name, game_version: manifest.minecraft.version.clone(), loader, loader_version: loader_version.clone(), icon: None },
            loader_version,
            None,
            request.now_ms,
        )?;
        return match fill_curseforge(client, key, instances, &instance, &manifest, &mut archive, request.concurrency, report).await {
            Ok(skipped) => Ok(Installed { instance, skipped }),
            Err(error) => {
                let _ = instances.delete(&instance.id);
                Err(error)
            }
        };
    }

    let index: PackIndex = read_json(&mut archive, "modrinth.index.json")?;
    let (game_version, loader, loader_version) = target(&index.dependencies)?;
    let published = match crate::snapshots::hash_file(request.path) {
        Ok(sha1) => match modrinth::version_by_hash(client, &sha1).await {
            Ok(Some(version)) => modrinth::project(client, &version.project_id).await.ok().map(|project| (project, version)),
            _ => None,
        },
        Err(_) => None,
    }
    .filter(|(project, _)| project.project_type == ProjectType::Modpack);
    let (name, icon, origin) = match published {
        Some((project, version)) => (
            project.title.clone(),
            project.icon_url.clone(),
            Some(ModpackOrigin { project_id: project.id, version_id: version.id, title: project.title, version_number: version.version_number }),
        ),
        None => (name, None, None),
    };
    let instance = instances.create_with(
        NewInstance { name, game_version, loader, loader_version: loader_version.clone(), icon },
        loader_version,
        origin,
        request.now_ms,
    )?;
    if let Err(error) = fill(client, instances, &instance, &index, &mut archive, request.concurrency, report).await {
        let _ = instances.delete(&instance.id);
        return Err(error);
    }
    Ok(Installed { instance, skipped: Vec::new() })
}

async fn fill(
    client: &HttpClient,
    instances: &InstanceStore,
    instance: &Instance,
    index: &PackIndex,
    archive: &mut zip::ZipArchive<std::fs::File>,
    concurrency: usize,
    report: &(dyn Fn(InstallUpdate) + Send + Sync),
) -> Result<(), ModpackError> {
    let game_dir = instances
        .game_dir(&instance.id)
        .ok_or(InstanceError::NotFound)?;
    let mut downloads = Vec::new();
    // What the pack brought, so the instance can tell its files from ones added later.
    let mut brought: HashMap<String, String> = HashMap::new();
    for file in &index.files {
        let client_side = file.env.as_ref().and_then(|env| env.client.as_deref());
        if client_side == Some("unsupported") {
            continue;
        }
        brought.insert(file.path.clone(), file.hashes.get("sha1").cloned().unwrap_or_default());
        let path = safe_join(&game_dir, &file.path).ok_or_else(|| {
            ModpackError::BadPack(format!(
                "a file wants to go outside the instance: {}",
                file.path
            ))
        })?;
        // Modrinth's spec only allows downloads from a few hosts; take the first https one.
        let url = file
            .downloads
            .iter()
            .find(|url| url.starts_with("https://"))
            .ok_or_else(|| ModpackError::BadPack(format!("{} has no download", file.path)))?;
        downloads.push(Download {
            url: url.clone(),
            path,
            sha1: file.hashes.get("sha1").cloned(),
            size: file.file_size,
            executable: false,
        });
    }
    download_with_progress(client, downloads, concurrency, report).await?;

    // Overrides: configs, resource packs and such, copied over the downloaded files.
    report(InstallUpdate::Unpacking);
    for path in unpack_overrides(archive, &game_dir, &["overrides/", "client-overrides/"])? {
        brought.entry(path).or_default();
    }
    if let Some(dir) = instances.dir_of(&instance.id) {
        let _ = inventory::record_pack_files(&dir, &brought);
    }
    Ok(())
}

/// Copies the files under these folders of the pack into the game folder, returning where they
/// went (relative to it).
fn unpack_overrides(
    archive: &mut zip::ZipArchive<std::fs::File>,
    game_dir: &Path,
    prefixes: &[&str],
) -> Result<Vec<String>, ModpackError> {
    let mut written = Vec::new();
    for prefix in prefixes {
        for position in 0..archive.len() {
            let mut entry = archive
                .by_index(position)
                .map_err(|error| ModpackError::BadPack(error.to_string()))?;
            let Ok(name) = entry.name().map(|name| name.into_owned()) else {
                continue;
            };
            let Some(relative) = name.strip_prefix(prefix) else {
                continue;
            };
            if relative.is_empty() || entry.is_dir() {
                continue;
            }
            let Some(destination) = safe_join(game_dir, relative) else {
                continue;
            };
            // A pack update keeps the player's own game settings.
            if relative == "options.txt" && destination.exists() {
                continue;
            }
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::io::copy(&mut entry, &mut std::fs::File::create(destination)?)?;
            written.push(relative.to_string());
        }
    }
    Ok(written)
}

// ─── Updating a pack ────────────────────────────────────────────────────────────────────────────

/// A version of the modpack an instance came from.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackRelease {
    pub version_id: String,
    pub version_number: String,
    /// The Minecraft versions it's for.
    pub game_versions: Vec<String>,
    /// RFC 3339.
    pub published: String,
}

/// Newer versions of an instance's modpack: the newest for its Minecraft version, and the newest
/// of all when that one is for another Minecraft version.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackUpdates {
    pub same_game: Option<PackRelease>,
    pub newer_game: Option<PackRelease>,
}

fn newest_release(versions: Vec<modrinth::ProjectVersion>) -> Option<modrinth::ProjectVersion> {
    let release = versions.iter().position(|version| version.version_type == "release");
    match release {
        Some(index) => versions.into_iter().nth(index),
        None => versions.into_iter().next(),
    }
}

fn release_of(version: &modrinth::ProjectVersion) -> PackRelease {
    PackRelease {
        version_id: version.id.clone(),
        version_number: version.version_number.clone(),
        game_versions: version.game_versions.clone(),
        published: version.date_published.clone(),
    }
}

/// Looks for newer versions of the Modrinth modpack an instance came from.
pub async fn pack_updates(client: &HttpClient, instance: &Instance) -> Result<PackUpdates, ModpackError> {
    let Some(origin) = instance.modpack.as_ref().filter(|origin| !curseforge::is_curseforge(&origin.project_id)) else {
        return Ok(PackUpdates::default());
    };
    let versions = modrinth::project_versions(client, &origin.project_id, &[], &[]).await?;
    let current = versions.iter().find(|version| version.id == origin.version_id).map(|version| version.date_published.clone());
    let newer = |version: &modrinth::ProjectVersion| version.id != origin.version_id && current.as_ref().is_none_or(|current| version.date_published > *current);
    let same: Vec<modrinth::ProjectVersion> = versions.iter().filter(|version| version.game_versions.contains(&instance.game_version)).cloned().collect();
    let same_game = newest_release(same).filter(|version| newer(version)).map(|version| release_of(&version));
    let newer_game = newest_release(versions)
        .filter(|version| newer(version) && !version.game_versions.contains(&instance.game_version))
        .map(|version| release_of(&version));
    Ok(PackUpdates { same_game, newer_game })
}

/// What a pack update did.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackUpdated {
    pub from: String,
    pub to: String,
    /// Files the new version brought or changed.
    pub added: usize,
    /// The old version's files it dropped (they wait in the instance's holding folder).
    pub removed: usize,
    /// When the update moved the instance to another Minecraft version or loader.
    pub moved_to: Option<String>,
}

/// Updates an instance to another version of its Modrinth modpack. The pack's old files that the
/// new version drops go to the holding folder (files the player turned off stay off), its new
/// and changed files are downloaded, and its overrides (configs) are unpacked again, keeping the
/// player's options.txt. Mods and packs the player added are left alone.
#[allow(clippy::too_many_arguments)]
pub async fn update_pack(
    client: &HttpClient,
    instances: &InstanceStore,
    instance_id: &str,
    version_id: &str,
    cache_dir: &Path,
    concurrency: usize,
    now_ms: i64,
    report: &(dyn Fn(InstallUpdate) + Send + Sync),
) -> Result<PackUpdated, ModpackError> {
    let instance = instances.get(instance_id).ok_or(InstanceError::NotFound)?;
    let instance_dir = instances.dir_of(instance_id).ok_or(InstanceError::NotFound)?;
    let game_dir = instance_dir.join(GAME_DIR);
    let origin = instance.modpack.clone().filter(|origin| !curseforge::is_curseforge(&origin.project_id)).ok_or(ModpackError::NotAModpack)?;
    report(InstallUpdate::Preparing);
    let old_pack = cache_dir.join(format!("{}.mrpack", origin.version_id));
    let old_files = inventory::pack_files(&instance_dir, Some(&old_pack));

    let version = modrinth::version(client, version_id).await?;
    if version.project_id != origin.project_id {
        return Err(ModpackError::NotAModpack);
    }
    let file = version.primary_file().ok_or(ModpackError::NoVersion)?;
    let pack_path = cache_dir.join(format!("{}.mrpack", version.id));
    if !pack_path.is_file() {
        let download = Download { url: file.url.clone(), path: pack_path.clone(), sha1: file.sha1.clone(), size: Some(file.size), executable: false };
        download_with_progress(client, vec![download], concurrency, report).await?;
    }
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&pack_path)?).map_err(|error| ModpackError::BadPack(error.to_string()))?;
    let index: PackIndex = read_json(&mut archive, "modrinth.index.json")?;
    let (game_version, loader, loader_version) = target(&index.dependencies)?;
    let new_files = inventory::mrpack_files(&pack_path).ok_or_else(|| ModpackError::BadPack("its file list couldn't be read".into()))?;

    // The old version's files the new one doesn't have.
    let mut removed = 0;
    for path in old_files.iter().flat_map(|files| files.keys()) {
        if new_files.contains_key(path) {
            continue;
        }
        let listed = ["mods/", "resourcepacks/", "shaderpacks/"].iter().any(|folder| path.starts_with(folder));
        for candidate in [path.clone(), format!("{path}.disabled")] {
            if listed && game_dir.join(&candidate).is_file() && inventory::remove(&instance_dir, &candidate, now_ms).is_ok() {
                removed += 1;
            }
        }
    }

    // New and changed files; ones the player turned off stay off.
    let mut downloads = Vec::new();
    for file in &index.files {
        if file.env.as_ref().and_then(|env| env.client.as_deref()) == Some("unsupported") {
            continue;
        }
        let path = safe_join(&game_dir, &file.path).ok_or_else(|| ModpackError::BadPack(format!("a file wants to go outside the instance: {}", file.path)))?;
        if game_dir.join(format!("{}.disabled", file.path)).is_file() {
            continue;
        }
        let expected = file.hashes.get("sha1");
        if path.is_file() {
            if expected.is_some() && inventory::sha1_of(&path).as_ref() == expected {
                continue;
            }
            std::fs::remove_file(&path)?;
        }
        let url = file.downloads.iter().find(|url| url.starts_with("https://")).ok_or_else(|| ModpackError::BadPack(format!("{} has no download", file.path)))?;
        downloads.push(Download { url: url.clone(), path, sha1: expected.cloned(), size: file.file_size, executable: false });
    }
    let added = downloads.len();
    download_with_progress(client, downloads, concurrency, report).await?;
    report(InstallUpdate::Unpacking);
    unpack_overrides(&mut archive, &game_dir, &["overrides/", "client-overrides/"])?;

    let before = format!("{} {}", instance.loader.display_name(), instance.game_version);
    let after = format!("{} {}", loader.display_name(), game_version);
    instances.update(instance_id, |stored| {
        stored.game_version = game_version.clone();
        stored.loader = loader;
        stored.loader_version = loader_version.clone();
        if let Some(stored_origin) = stored.modpack.as_mut() {
            stored_origin.version_id = version.id.clone();
            stored_origin.version_number = version.version_number.clone();
        }
    })?;
    let _ = inventory::record_pack_files(&instance_dir, &new_files);
    Ok(PackUpdated { from: origin.version_number, to: version.version_number, added, removed, moved_to: (before != after).then_some(after) })
}

// ─── Exporting a pack ───────────────────────────────────────────────────────────────────────────

/// What goes into an exported modpack besides its mods and packs.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    /// The pack's own version, as its players see it ("1.0").
    pub version: String,
    /// The config folders: config/, defaultconfigs/, kubejs/ and the like.
    pub configs: bool,
    /// The game settings in options.txt.
    pub options: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Exported {
    /// Files players download from Modrinth when they install it.
    pub downloads: usize,
    /// Files carried inside the pack, because Modrinth doesn't have them.
    pub bundled: usize,
    pub bytes: u64,
}

/// Folders of configs and scripts packs commonly carry.
const CONFIG_FOLDERS: [&str; 6] = ["config", "defaultconfigs", "kubejs", "scripts", "global_packs", "resources"];

/// The key a loader goes by in a `.mrpack`'s dependencies.
fn loader_key(loader: Loader) -> Option<&'static str> {
    match loader {
        Loader::Fabric => Some("fabric-loader"),
        Loader::Quilt => Some("quilt-loader"),
        Loader::Forge => Some("forge"),
        Loader::NeoForge => Some("neoforge"),
        Loader::Vanilla => None,
    }
}

/// Writes an instance as a Modrinth modpack (`.mrpack`): what Modrinth has is listed for
/// download, everything else (and, if asked, configs and options.txt) is carried inside. Worlds,
/// screenshots and logs stay out, and so do files that are turned off.
pub fn export_mrpack(instance: &Instance, instance_dir: &Path, scan: &inventory::Scan, out: &Path, options: &ExportOptions) -> Result<Exported, ModpackError> {
    use std::io::Write;
    let game_dir = instance_dir.join(GAME_DIR);
    let (downloadable, own) = scan.for_export();
    let mut dependencies = serde_json::Map::new();
    dependencies.insert("minecraft".into(), instance.game_version.clone().into());
    if let (Some(key), Some(version)) = (loader_key(instance.loader), &instance.loader_version) {
        dependencies.insert(key.into(), version.clone().into());
    }
    let files: Vec<serde_json::Value> = downloadable
        .iter()
        .map(|file| serde_json::json!({ "path": file.path, "hashes": { "sha1": file.sha1, "sha512": file.sha512 }, "downloads": [file.url], "fileSize": file.size }))
        .collect();
    let index = serde_json::json!({
        "formatVersion": 1,
        "game": "minecraft",
        "versionId": options.version,
        "name": instance.name,
        "files": files,
        "dependencies": dependencies,
    });

    let zip_error = |error: zip::result::ZipError| ModpackError::Io(std::io::Error::other(error));
    let partial = out.with_extension("mrpack.part");
    let mut zip = zip::ZipWriter::new(std::io::BufWriter::new(std::fs::File::create(&partial)?));
    let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("modrinth.index.json", deflated).map_err(zip_error)?;
    zip.write_all(&serde_json::to_vec_pretty(&index).map_err(std::io::Error::other)?)?;

    let mut carried: Vec<String> = own;
    if options.configs {
        carried.extend(CONFIG_FOLDERS.iter().filter(|folder| game_dir.join(folder).is_dir()).map(|folder| folder.to_string()));
    }
    if options.options && game_dir.join("options.txt").is_file() {
        carried.push("options.txt".into());
    }
    let mut bundled = 0;
    let mut pending: Vec<String> = carried;
    while let Some(relative) = pending.pop() {
        let path = game_dir.join(&relative);
        if path.is_dir() {
            for entry in std::fs::read_dir(&path)?.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.starts_with('.') {
                    pending.push(format!("{relative}/{name}"));
                }
            }
        } else if path.is_file() {
            zip.start_file(format!("overrides/{relative}"), deflated).map_err(zip_error)?;
            std::io::copy(&mut std::fs::File::open(&path)?, &mut zip)?;
            bundled += 1;
        }
    }
    zip.finish().map_err(zip_error)?;
    std::fs::rename(&partial, out)?;
    let bytes = std::fs::metadata(out).map(|meta| meta.len()).unwrap_or(0);
    Ok(Exported { downloads: downloadable.len(), bundled, bytes })
}

// ─── CurseForge packs ───────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseManifest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version: Option<String>,
    minecraft: CurseMinecraft,
    #[serde(default)]
    files: Vec<CurseFileRef>,
    #[serde(default = "default_overrides")]
    overrides: String,
}

fn default_overrides() -> String {
    "overrides".into()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseMinecraft {
    version: String,
    #[serde(default)]
    mod_loaders: Vec<CurseLoader>,
}

#[derive(Deserialize)]
struct CurseLoader {
    id: String,
    #[serde(default)]
    primary: bool,
}

#[derive(Deserialize)]
struct CurseFileRef {
    #[serde(rename = "projectID")]
    project_id: u32,
    #[serde(rename = "fileID")]
    file_id: u32,
    #[serde(default = "yes")]
    required: bool,
}

fn yes() -> bool {
    true
}

/// The pack's loader, from ids like "forge-47.2.0" or "fabric-0.16.10".
fn curse_target(minecraft: &CurseMinecraft) -> Result<(Loader, Option<String>), ModpackError> {
    let chosen = minecraft
        .mod_loaders
        .iter()
        .find(|loader| loader.primary)
        .or(minecraft.mod_loaders.first());
    let Some(chosen) = chosen else {
        return Ok((Loader::Vanilla, None));
    };
    let (name, version) = chosen
        .id
        .split_once('-')
        .unwrap_or((chosen.id.as_str(), ""));
    let loader = match name {
        "forge" => Loader::Forge,
        "neoforge" => Loader::NeoForge,
        "fabric" => Loader::Fabric,
        "quilt" => Loader::Quilt,
        other => return Err(ModpackError::UnknownLoader(other.to_string())),
    };
    Ok((
        loader,
        Some(version.to_string()).filter(|version| !version.is_empty()),
    ))
}

/// Where a CurseForge project's files go, by its class.
fn curse_folder(class: Option<u32>) -> &'static str {
    match curseforge::project_type(class) {
        ProjectType::Resourcepack => "resourcepacks",
        ProjectType::Shader => "shaderpacks",
        ProjectType::Datapack => "datapacks",
        _ => "mods",
    }
}

async fn install_curseforge(
    client: &HttpClient,
    instances: &InstanceStore,
    request: Request<'_>,
    report: &(dyn Fn(InstallUpdate) + Send + Sync),
) -> Result<Installed, ModpackError> {
    let key = request
        .curseforge_key
        .ok_or(ModpackError::NoCurseForgeKey)?;
    let mod_id = curseforge::mod_id(request.project_id).ok_or(ModpackError::NotAModpack)?;
    report(InstallUpdate::Preparing);
    let project = curseforge::project_lite(client, key, mod_id).await?;
    if project.project_type != ProjectType::Modpack {
        return Err(ModpackError::NotAModpack);
    }
    let version = match request.version_id.and_then(|id| id.parse().ok()) {
        Some(file_id) => curseforge::version(client, key, mod_id, file_id).await?,
        None => {
            let versions = curseforge::versions(client, key, mod_id, &[], &[]).await?;
            let release = versions
                .iter()
                .position(|version| version.version_type == "release");
            match release {
                Some(index) => versions.into_iter().nth(index),
                None => versions.into_iter().next(),
            }
            .ok_or(ModpackError::NoVersion)?
        }
    };
    let file = version.primary_file().ok_or(ModpackError::NoVersion)?;
    if file.url.is_empty() {
        return Err(ModpackError::NotDistributable);
    }
    let pack_path = request
        .cache_dir
        .join(format!("curseforge-{}.zip", version.id));
    if !pack_path.is_file() {
        let download = Download {
            url: file.url.clone(),
            path: pack_path.clone(),
            sha1: file.sha1.clone(),
            size: Some(file.size),
            executable: false,
        };
        download_with_progress(client, vec![download], request.concurrency, report).await?;
    }

    let mut archive = zip::ZipArchive::new(std::fs::File::open(&pack_path)?)
        .map_err(|error| ModpackError::BadPack(error.to_string()))?;
    let manifest: CurseManifest = {
        let mut entry = archive
            .by_name("manifest.json")
            .map_err(|_| ModpackError::BadPack("no manifest.json".into()))?;
        let mut text = String::new();
        entry.read_to_string(&mut text)?;
        serde_json::from_str(&text).map_err(|error| ModpackError::BadPack(error.to_string()))?
    };
    let (loader, loader_version) = curse_target(&manifest.minecraft)?;
    let instance = instances.create_with(
        NewInstance {
            name: project.title.clone(),
            game_version: manifest.minecraft.version.clone(),
            loader,
            loader_version: loader_version.clone(),
            icon: project.icon_url.clone(),
        },
        loader_version,
        Some(ModpackOrigin {
            project_id: project.id.clone(),
            version_id: version.id.clone(),
            title: project.title.clone(),
            version_number: version.name.clone(),
        }),
        request.now_ms,
    )?;

    let result = fill_curseforge(
        client,
        key,
        instances,
        &instance,
        &manifest,
        &mut archive,
        request.concurrency,
        report,
    )
    .await;
    match result {
        Ok(skipped) => Ok(Installed { instance, skipped }),
        Err(error) => {
            let _ = instances.delete(&instance.id);
            Err(error)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn fill_curseforge(
    client: &HttpClient,
    key: &str,
    instances: &InstanceStore,
    instance: &Instance,
    manifest: &CurseManifest,
    archive: &mut zip::ZipArchive<std::fs::File>,
    concurrency: usize,
    report: &(dyn Fn(InstallUpdate) + Send + Sync),
) -> Result<Vec<String>, ModpackError> {
    let game_dir = instances
        .game_dir(&instance.id)
        .ok_or(InstanceError::NotFound)?;
    let wanted: Vec<&CurseFileRef> = manifest.files.iter().filter(|file| file.required).collect();
    let file_ids: Vec<u32> = wanted.iter().map(|file| file.file_id).collect();
    let mod_ids: Vec<u32> = wanted.iter().map(|file| file.project_id).collect();
    let files = curseforge::files(client, key, &file_ids).await?;
    let projects: HashMap<u32, curseforge::RawMod> = curseforge::mods(client, key, &mod_ids)
        .await?
        .into_iter()
        .map(|project| (project.id, project))
        .collect();

    let mut downloads = Vec::new();
    let mut skipped = Vec::new();
    let mut brought: HashMap<String, String> = HashMap::new();
    for file in files {
        let project = projects.get(&file.mod_id);
        let name = project.map_or_else(|| file.file_name.clone(), |project| project.name.clone());
        let Some(url) = file
            .download_url
            .clone()
            .filter(|url| url.starts_with("https://"))
        else {
            skipped.push(name);
            continue;
        };
        if file.file_name.is_empty()
            || file.file_name.contains(['/', '\\'])
            || file.file_name.starts_with('.')
        {
            return Err(ModpackError::BadPack(format!(
                "{name} has an odd file name"
            )));
        }
        let folder = curse_folder(project.and_then(|project| project.class_id));
        brought.insert(format!("{folder}/{}", file.file_name), file.sha1().unwrap_or_default());
        downloads.push(Download {
            url,
            path: game_dir.join(folder).join(&file.file_name),
            sha1: file.sha1(),
            size: Some(file.file_length),
            executable: false,
        });
    }
    download_with_progress(client, downloads, concurrency, report).await?;

    report(InstallUpdate::Unpacking);
    let prefix = format!("{}/", manifest.overrides.trim_end_matches('/'));
    for path in unpack_overrides(archive, &game_dir, &[prefix.as_str()])? {
        brought.entry(path).or_default();
    }
    if let Some(dir) = instances.dir_of(&instance.id) {
        let _ = inventory::record_pack_files(&dir, &brought);
    }
    skipped.sort();
    Ok(skipped)
}

async fn download_with_progress(
    client: &HttpClient,
    items: Vec<Download>,
    concurrency: usize,
    report: &(dyn Fn(InstallUpdate) + Send + Sync),
) -> Result<(), ModpackError> {
    let items = crate::launch::missing(items);
    if items.is_empty() {
        return Ok(());
    }
    let progress = Arc::new(Progress::default());
    let snapshot = |progress: &Progress| {
        let done_bytes = progress.done_bytes.load(Ordering::Relaxed);
        InstallUpdate::Downloading {
            done_bytes,
            total_bytes: progress.total_bytes.load(Ordering::Relaxed).max(done_bytes),
            done_files: progress.done_files.load(Ordering::Relaxed),
            total_files: progress.total_files.load(Ordering::Relaxed),
        }
    };
    let downloading = download_all(client, items, concurrency, &progress);
    tokio::pin!(downloading);
    let mut tick = tokio::time::interval(Duration::from_millis(200));
    let result = loop {
        tokio::select! {
            result = &mut downloading => break result,
            _ = tick.tick() => report(snapshot(&progress)),
        }
    };
    report(snapshot(&progress));
    Ok(result?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_an_instance_as_a_modrinth_pack_wisp_can_read_back() {
        use std::io::Write;
        let root = std::env::temp_dir().join(format!("wisp-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store = InstanceStore::new(root.join("instances"));
        let request = NewInstance { name: "Export".into(), game_version: "26.3".into(), loader: Loader::Fabric, loader_version: None, icon: None };
        let instance = store.create(request, Some("0.20.1".into()), 0).unwrap();
        let dir = store.dir_of(&instance.id).unwrap();
        let game = dir.join(GAME_DIR);
        let mut jar = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        jar.start_file("fabric.mod.json", zip::write::SimpleFileOptions::default()).unwrap();
        jar.write_all(br#"{"id": "mine", "version": "1"}"#).unwrap();
        let jar = jar.finish().unwrap().into_inner();
        for (path, bytes) in [("mods/mine.jar", &jar[..]), ("mods/off.jar.disabled", &jar[..]), ("config/mine.json", b"{}"), ("options.txt", b"fov:0.5"), ("saves/World/level.dat", b"world")] {
            let file = game.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, bytes).unwrap();
        }
        let scan = inventory::scan(&dir);
        let out = root.join("Export 1.0.mrpack");
        let exported = export_mrpack(&instance, &dir, &scan, &out, &ExportOptions { version: "1.0".into(), configs: true, options: false }).unwrap();
        // Nothing here is on Modrinth (no network in tests), so the mod travels inside the pack.
        assert_eq!((exported.downloads, exported.bundled), (0, 2));
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        let summary = summarize(&mut archive).unwrap().unwrap();
        assert_eq!((summary.game_version.as_str(), summary.loader, summary.version.as_deref()), ("26.3", Loader::Fabric, Some("1.0")));
        let names: Vec<String> = archive.file_names().filter_map(Result::ok).map(|name| name.into_owned()).collect();
        assert!(names.contains(&"overrides/mods/mine.jar".to_string()) && names.contains(&"overrides/config/mine.json".to_string()), "{names:?}");
        assert!(!names.iter().any(|name| name.contains("off.jar") || name.contains("options.txt") || name.contains("saves")), "{names:?}");
        let files = inventory::mrpack_files(&out).unwrap();
        assert!(files.contains_key("mods/mine.jar"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reads_curseforge_manifests() {
        let manifest: CurseManifest = serde_json::from_str(
            r#"{"minecraft": {"version": "1.20.1", "modLoaders": [{"id": "forge-47.2.0", "primary": true}]},
                "manifestType": "minecraftModpack", "manifestVersion": 1, "name": "Pack", "version": "1.0",
                "files": [{"projectID": 238222, "fileID": 4712862, "required": true}, {"projectID": 1, "fileID": 2, "required": false}],
                "overrides": "overrides"}"#,
        )
        .unwrap();
        let (loader, version) = curse_target(&manifest.minecraft).unwrap();
        assert_eq!(
            (loader, version.as_deref()),
            (Loader::Forge, Some("47.2.0"))
        );
        assert_eq!(
            manifest.files.iter().filter(|file| file.required).count(),
            1
        );
        let fabric = CurseMinecraft {
            version: "1.21.1".into(),
            mod_loaders: vec![CurseLoader {
                id: "fabric-0.16.10".into(),
                primary: true,
            }],
        };
        assert_eq!(curse_target(&fabric).unwrap().0, Loader::Fabric);
        assert_eq!(curse_folder(Some(12)), "resourcepacks");
        assert_eq!(curse_folder(Some(6)), "mods");
    }

    #[test]
    fn reads_the_pack_target() {
        let deps = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect::<HashMap<_, _>>()
        };
        let (game, loader, version) =
            target(&deps(&[("minecraft", "26.3"), ("fabric-loader", "0.20.1")])).unwrap();
        assert_eq!(
            (game.as_str(), loader, version.as_deref()),
            ("26.3", Loader::Fabric, Some("0.20.1"))
        );
        let (_, loader, _) =
            target(&deps(&[("minecraft", "1.20.1"), ("forge", "47.2.0")])).unwrap();
        assert_eq!(loader, Loader::Forge);
        let (_, loader, _) =
            target(&deps(&[("minecraft", "1.21.1"), ("neoforge", "21.1.77")])).unwrap();
        assert_eq!(loader, Loader::NeoForge);
        assert!(matches!(
            target(&deps(&[("minecraft", "1.7.10"), ("liteloader", "1")])),
            Err(ModpackError::UnknownLoader(_))
        ));
        assert!(target(&deps(&[("fabric-loader", "0.20.1")])).is_err());
    }
}
