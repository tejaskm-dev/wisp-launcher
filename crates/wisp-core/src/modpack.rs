//! Installing a modpack as a new instance: read its index (Modrinth's `.mrpack` or CurseForge's
//! zip with a `manifest.json`), make an instance with the pack's Minecraft version and loader,
//! download every file it lists (checked against its SHA-1) and unpack its overrides (configs and
//! the like).

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
use crate::instances::{
    Instance, InstanceError, InstanceStore, Loader, ModpackOrigin, NewInstance,
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
    for file in &index.files {
        let client_side = file.env.as_ref().and_then(|env| env.client.as_deref());
        if client_side == Some("unsupported") {
            continue;
        }
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
    unpack_overrides(archive, &game_dir, &["overrides/", "client-overrides/"])
}

/// Copies the files under these folders of the pack into the game folder.
fn unpack_overrides(
    archive: &mut zip::ZipArchive<std::fs::File>,
    game_dir: &Path,
    prefixes: &[&str],
) -> Result<(), ModpackError> {
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
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::io::copy(&mut entry, &mut std::fs::File::create(destination)?)?;
        }
    }
    Ok(())
}

// ─── CurseForge packs ───────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurseManifest {
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
    unpack_overrides(archive, &game_dir, &[prefix.as_str()])?;
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
