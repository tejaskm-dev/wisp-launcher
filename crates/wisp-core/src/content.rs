//! Mods, resource packs, shaders and data packs installed into an instance from Modrinth or
//! CurseForge, with the dependencies they require, and worlds from CurseForge. A small index
//! (`content.json`, next to `instance.json`) records what came from where, so Wisp can show,
//! update and remove it.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::HttpClient;
use crate::curseforge;
use crate::instances::{GAME_DIR, Instance, Loader};
use crate::launch::{Download, DownloadError, Progress, download_all};
use crate::modrinth::{self, ProjectDetails, ProjectType, ProjectVersion};
use crate::worlds;

const INDEX: &str = "content.json";
/// Dependencies of dependencies… stop somewhere sensible.
const MAX_PROJECTS: usize = 40;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledContent {
    pub project_id: String,
    pub version_id: String,
    pub title: String,
    pub version_number: String,
    pub project_type: ProjectType,
    /// Path inside the game folder, e.g. `mods/sodium-fabric-0.9.3.jar`.
    pub file: String,
    pub icon_url: Option<String>,
    /// False when it came along as another project's dependency.
    #[serde(default = "yes")]
    pub explicit: bool,
    /// For a data pack, the world (folder) it's in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world: Option<String>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    #[error(
        "Vanilla instances can't run mods. Make a Fabric, Quilt, Forge or NeoForge instance for this."
    )]
    NeedsLoader,
    #[error("{title} has no version for {loader} on Minecraft {game}.")]
    NoCompatibleVersion {
        title: String,
        loader: &'static str,
        game: String,
    },
    #[error("{title} has no data pack for Minecraft {game}.")]
    NoDatapackVersion { title: String, game: String },
    #[error("Modpacks install as an instance of their own.")]
    IsModpack,
    #[error("{0} is a data pack: pick a world to add it to.")]
    IsDatapack(String),
    #[error("There's no world called {0} in this instance anymore.")]
    NoSuchWorld(String),
    #[error("Wisp can't install this kind of project yet.")]
    Unsupported,
    #[error("{0} has no file to download.")]
    NoFile(String),
    #[error(
        "{0}'s author only allows downloads from the CurseForge website, so Wisp can't fetch it."
    )]
    NotDistributable(String),
    #[error("CurseForge needs an API key. Add one in Settings → Mod sources.")]
    NoCurseForgeKey,
    #[error("The world couldn't be unpacked: {0}")]
    BadWorld(String),
    #[error("{0}")]
    Network(#[from] crate::Error),
    #[error(transparent)]
    Download(#[from] DownloadError),
    #[error("Couldn't save the files: {0}")]
    Io(#[from] std::io::Error),
}

/// The Modrinth loaders whose builds an instance can use, for a kind of project; `None` when the
/// instance can't use that kind at all.
pub fn compatible_loaders(loader: Loader, kind: ProjectType) -> Option<Vec<&'static str>> {
    match kind {
        ProjectType::Mod => match loader {
            Loader::Vanilla => None,
            Loader::Fabric => Some(vec!["fabric"]),
            // Quilt runs most Fabric mods too.
            Loader::Quilt => Some(vec!["quilt", "fabric"]),
            Loader::Forge => Some(vec!["forge"]),
            Loader::NeoForge => Some(vec!["neoforge"]),
        },
        ProjectType::Resourcepack => Some(vec!["minecraft"]),
        ProjectType::Shader => Some(match loader {
            // Core shaders work in plain Minecraft; the rest need Iris or OptiFine.
            Loader::Vanilla => vec!["vanilla"],
            _ => vec!["iris", "optifine", "vanilla", "canvas"],
        }),
        // Data packs are plain Minecraft: any instance's worlds can use them.
        ProjectType::Datapack => Some(vec!["datapack"]),
        _ => None,
    }
}

/// Which folder of the game a version's file belongs in.
fn folder(kind: ProjectType, version: &ProjectVersion) -> &'static str {
    match kind {
        ProjectType::Mod => "mods",
        // Iris and OptiFine shaders are shader packs; core shaders are resource packs.
        ProjectType::Shader
            if version
                .loaders
                .iter()
                .any(|loader| loader == "iris" || loader == "optifine") =>
        {
            "shaderpacks"
        }
        _ => "resourcepacks",
    }
}

/// What's installed in one instance.
pub struct ContentIndex {
    instance_dir: PathBuf,
}

impl ContentIndex {
    pub fn new(instance_dir: impl Into<PathBuf>) -> Self {
        Self {
            instance_dir: instance_dir.into(),
        }
    }

    pub fn list(&self) -> Vec<InstalledContent> {
        std::fs::read(self.instance_dir.join(INDEX))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub(crate) fn save(&self, items: &[InstalledContent]) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(items).map_err(std::io::Error::other)?;
        let partial = self.instance_dir.join("content.json.part");
        std::fs::write(&partial, json)?;
        std::fs::rename(partial, self.instance_dir.join(INDEX))
    }

    pub(crate) fn game_dir(&self) -> PathBuf {
        self.instance_dir.join(GAME_DIR)
    }

    /// Removes a project's file and forgets it (for a data pack, the copy in that world).
    pub fn remove(&self, project_id: &str, world: Option<&str>) -> Result<(), ContentError> {
        let mut items = self.list();
        let matches = |item: &InstalledContent| {
            item.project_id == project_id && item.world.as_deref() == world
        };
        if let Some(item) = items.iter().find(|item| matches(item))
            && let Some(path) = safe_join(&self.game_dir(), &item.file)
        {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        items.retain(|item| !matches(item));
        self.save(&items)?;
        Ok(())
    }
}

/// What to install, and from where.
pub struct InstallRequest<'a> {
    /// A Modrinth id or slug, or "curseforge:<id>".
    pub project_id: &'a str,
    /// A specific version; left out, the newest that fits.
    pub version_id: Option<&'a str>,
    /// Install as a data pack into this world (its folder name).
    pub world: Option<&'a str>,
    pub concurrency: usize,
    /// Needed for CurseForge projects.
    pub curseforge_key: Option<&'a str>,
}

fn need_key(key: Option<&str>) -> Result<&str, ContentError> {
    key.ok_or(ContentError::NoCurseForgeKey)
}

async fn fetch_project(
    client: &HttpClient,
    key: Option<&str>,
    id: &str,
) -> Result<ProjectDetails, ContentError> {
    if curseforge::is_curseforge(id) {
        let mod_id = curseforge::mod_id(id).ok_or(ContentError::Unsupported)?;
        return Ok(curseforge::project_lite(client, need_key(key)?, mod_id).await?);
    }
    Ok(modrinth::project(client, id).await?)
}

async fn fetch_version(
    client: &HttpClient,
    key: Option<&str>,
    project_id: &str,
    version_id: &str,
) -> Result<ProjectVersion, ContentError> {
    if let Some(mod_id) = curseforge::mod_id(project_id) {
        let file_id = version_id.parse().map_err(|_| ContentError::Unsupported)?;
        return Ok(curseforge::version(client, need_key(key)?, mod_id, file_id).await?);
    }
    Ok(modrinth::version(client, version_id).await?)
}

/// `relative` under `base`, unless it tries to leave it.
pub(crate) fn safe_join(base: &Path, relative: &str) -> Option<PathBuf> {
    let path = Path::new(relative);
    path.components()
        .all(|part| matches!(part, std::path::Component::Normal(_)))
        .then(|| base.join(path))
}

/// The newest version that fits: a release if there is one, else the newest beta or alpha.
async fn best_version(
    client: &HttpClient,
    key: Option<&str>,
    project_id: &str,
    loaders: &[&str],
    game: Option<&str>,
) -> Result<Option<ProjectVersion>, ContentError> {
    let games: Vec<&str> = game.into_iter().collect();
    let versions = match curseforge::mod_id(project_id) {
        Some(mod_id) => {
            curseforge::versions(client, need_key(key)?, mod_id, loaders, &games).await?
        }
        None => modrinth::project_versions(client, project_id, loaders, &games).await?,
    };
    let release = versions
        .iter()
        .position(|version| version.version_type == "release");
    Ok(match release {
        Some(index) => versions.into_iter().nth(index),
        None => versions.into_iter().next(),
    })
}

/// Installs a project (a specific version, or the newest compatible one) into an instance,
/// with the dependencies it requires. Returns what was installed.
pub async fn install(
    client: &HttpClient,
    instance: &Instance,
    instance_dir: &Path,
    request: InstallRequest<'_>,
) -> Result<Vec<InstalledContent>, ContentError> {
    if let Some(world) = request.world {
        return install_datapack(client, instance, instance_dir, world, &request).await;
    }
    let InstallRequest {
        project_id,
        version_id,
        concurrency,
        curseforge_key: key,
        ..
    } = request;
    let index = ContentIndex::new(instance_dir);
    let existing = index.list();
    let mut planned: Vec<(InstalledContent, Download)> = Vec::new();
    let mut seen = HashSet::new();
    let mut queue = vec![(project_id.to_string(), version_id.map(String::from), true)];

    while let Some((project, version_id, explicit)) = queue.pop() {
        if !seen.insert(project.clone()) || seen.len() > MAX_PROJECTS {
            continue;
        }
        // A dependency that's already there stays as it is.
        if !explicit && existing.iter().any(|item| item.project_id == project) {
            continue;
        }
        let details = fetch_project(client, key, &project).await?;
        if details.project_type == ProjectType::Modpack {
            if explicit {
                return Err(ContentError::IsModpack);
            }
            continue;
        }
        if details.project_type == ProjectType::World && explicit {
            return install_world(
                client,
                key,
                instance_dir,
                &details,
                version_id.as_deref(),
                concurrency,
            )
            .await;
        }
        // Data packs go into a world; one that's only a data pack (no mod build) can't be a mod.
        let only_datapack = details.project_type == ProjectType::Datapack
            || (!details.loaders.is_empty()
                && details.loaders.iter().all(|loader| loader == "datapack"));
        if only_datapack {
            if explicit {
                return Err(ContentError::IsDatapack(details.title.clone()));
            }
            continue;
        }
        let loaders = compatible_loaders(instance.loader, details.project_type).ok_or(
            if details.project_type == ProjectType::Mod {
                ContentError::NeedsLoader
            } else {
                ContentError::Unsupported
            },
        )?;
        let version = match version_id {
            Some(id) => fetch_version(client, key, &details.id, &id).await?,
            None => best_version(
                client,
                key,
                &details.id,
                &loaders,
                Some(&instance.game_version),
            )
            .await?
            .ok_or_else(|| ContentError::NoCompatibleVersion {
                title: details.title.clone(),
                loader: instance.loader.display_name(),
                game: instance.game_version.clone(),
            })?,
        };
        for dependency in version
            .dependencies
            .iter()
            .filter(|dependency| dependency.dependency_type == "required")
        {
            match (&dependency.project_id, &dependency.version_id) {
                (Some(project), version) => queue.push((project.clone(), version.clone(), false)),
                (None, Some(version)) if !curseforge::is_curseforge(&details.id) => {
                    let found = modrinth::version(client, version).await?;
                    queue.push((found.project_id, Some(found.id), false));
                }
                (None, Some(_)) => {}
                (None, None) => {}
            }
        }

        let file = checked_file(&details, &version)?;
        let relative = format!(
            "{}/{}",
            folder(details.project_type, &version),
            file.filename
        );
        planned.push((
            InstalledContent {
                project_id: details.id.clone(),
                version_id: version.id.clone(),
                title: details.title.clone(),
                version_number: version.version_number.clone(),
                project_type: details.project_type,
                file: relative.clone(),
                icon_url: details.icon_url.clone(),
                explicit,
                world: None,
            },
            Download {
                url: file.url.clone(),
                path: index.game_dir().join(&relative),
                sha1: file.sha1.clone(),
                size: Some(file.size),
                executable: false,
            },
        ));
    }

    let downloads = planned
        .iter()
        .map(|(_, download)| download.clone())
        .collect();
    download_all(
        client,
        downloads,
        concurrency,
        &Arc::new(Progress::default()),
    )
    .await?;

    // Swap out older files of the same projects, then record what's there now.
    let mut items = existing;
    for (item, _) in &planned {
        if let Some(old) = items.iter().find(|old| old.project_id == item.project_id)
            && old.file != item.file
            && let Some(path) = safe_join(&index.game_dir(), &old.file)
        {
            let _ = std::fs::remove_file(path);
        }
        let explicit = item.explicit
            || items.iter().any(|old| {
                old.project_id == item.project_id && old.world.is_none() && old.explicit
            });
        items.retain(|old| !(old.project_id == item.project_id && old.world.is_none()));
        items.push(InstalledContent {
            explicit,
            ..item.clone()
        });
    }
    index.save(&items)?;
    Ok(planned.into_iter().map(|(item, _)| item).collect())
}

/// A version's file, if Wisp may download it and its name is a plain file name.
fn checked_file<'a>(
    details: &ProjectDetails,
    version: &'a ProjectVersion,
) -> Result<&'a modrinth::VersionFile, ContentError> {
    let file = version
        .primary_file()
        .ok_or_else(|| ContentError::NoFile(details.title.clone()))?;
    // CurseForge leaves the address out when the author doesn't allow other apps to download.
    if file.url.is_empty() {
        return Err(ContentError::NotDistributable(details.title.clone()));
    }
    // A file name with a path in it would escape the folder; the sites never send one.
    if file.filename.contains(['/', '\\']) || file.filename.starts_with('.') {
        return Err(ContentError::NoFile(details.title.clone()));
    }
    Ok(file)
}

/// Adds a data pack to one world: `saves/<world>/datapacks/<file>`.
async fn install_datapack(
    client: &HttpClient,
    instance: &Instance,
    instance_dir: &Path,
    world: &str,
    request: &InstallRequest<'_>,
) -> Result<Vec<InstalledContent>, ContentError> {
    let index = ContentIndex::new(instance_dir);
    if worlds::world_dir(&index.game_dir(), world).is_none() {
        return Err(ContentError::NoSuchWorld(world.to_string()));
    }
    let key = request.curseforge_key;
    let details = fetch_project(client, key, request.project_id).await?;
    let version = match request.version_id {
        Some(id) => fetch_version(client, key, &details.id, id).await?,
        None => best_version(
            client,
            key,
            &details.id,
            &["datapack"],
            Some(&instance.game_version),
        )
        .await?
        .ok_or_else(|| ContentError::NoDatapackVersion {
            title: details.title.clone(),
            game: instance.game_version.clone(),
        })?,
    };
    let file = checked_file(&details, &version)?;
    let relative = format!("saves/{world}/datapacks/{}", file.filename);
    download_all(
        client,
        vec![Download {
            url: file.url.clone(),
            path: index.game_dir().join(&relative),
            sha1: file.sha1.clone(),
            size: Some(file.size),
            executable: false,
        }],
        request.concurrency,
        &Arc::new(Progress::default()),
    )
    .await?;

    let item = InstalledContent {
        project_id: details.id.clone(),
        version_id: version.id.clone(),
        title: details.title.clone(),
        version_number: version.version_number.clone(),
        project_type: ProjectType::Datapack,
        file: relative,
        icon_url: details.icon_url.clone(),
        explicit: true,
        world: Some(world.to_string()),
    };
    let mut items = index.list();
    if let Some(old) = items
        .iter()
        .find(|old| old.project_id == item.project_id && old.world == item.world)
        && old.file != item.file
        && let Some(path) = safe_join(&index.game_dir(), &old.file)
    {
        let _ = std::fs::remove_file(path);
    }
    items.retain(|old| !(old.project_id == item.project_id && old.world == item.world));
    items.push(item.clone());
    index.save(&items)?;
    Ok(vec![item])
}

/// Downloads a world (a CurseForge map) and unpacks it into the instance's saves. Worlds become
/// the player's own, so they aren't tracked in content.json.
async fn install_world(
    client: &HttpClient,
    key: Option<&str>,
    instance_dir: &Path,
    details: &ProjectDetails,
    version_id: Option<&str>,
    concurrency: usize,
) -> Result<Vec<InstalledContent>, ContentError> {
    let version = match version_id {
        Some(id) => fetch_version(client, key, &details.id, id).await?,
        None => best_version(client, key, &details.id, &[], None)
            .await?
            .ok_or_else(|| ContentError::NoFile(details.title.clone()))?,
    };
    let file = checked_file(details, &version)?;
    let game_dir = instance_dir.join(GAME_DIR);
    let zip = instance_dir.join(format!(".download-{}", file.filename));
    download_all(
        client,
        vec![Download {
            url: file.url.clone(),
            path: zip.clone(),
            sha1: file.sha1.clone(),
            size: Some(file.size),
            executable: false,
        }],
        concurrency,
        &Arc::new(Progress::default()),
    )
    .await?;
    let title = details.title.clone();
    let unpacked = tokio::task::spawn_blocking(move || {
        let result = worlds::import_zip(&zip, &game_dir, &title);
        let _ = std::fs::remove_file(&zip);
        result
    })
    .await
    .map_err(|error| ContentError::BadWorld(error.to_string()))?
    .map_err(|error| ContentError::BadWorld(error.to_string()))?;
    Ok(vec![InstalledContent {
        project_id: details.id.clone(),
        version_id: version.id.clone(),
        title: details.title.clone(),
        version_number: version.version_number.clone(),
        project_type: ProjectType::World,
        file: format!("saves/{unpacked}"),
        icon_url: details.icon_url.clone(),
        explicit: true,
        world: Some(unpacked),
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instances_use_the_right_builds() {
        assert_eq!(
            compatible_loaders(Loader::Quilt, ProjectType::Mod).unwrap(),
            ["quilt", "fabric"]
        );
        assert!(compatible_loaders(Loader::Vanilla, ProjectType::Mod).is_none());
        assert_eq!(
            compatible_loaders(Loader::Vanilla, ProjectType::Resourcepack).unwrap(),
            ["minecraft"]
        );
        assert_eq!(
            compatible_loaders(Loader::Vanilla, ProjectType::Shader).unwrap(),
            ["vanilla"]
        );
        assert!(compatible_loaders(Loader::Fabric, ProjectType::Modpack).is_none());
    }

    #[test]
    fn shader_files_go_where_their_loader_reads_them() {
        let version = |loaders: &[&str]| ProjectVersion {
            id: "v".into(),
            project_id: "p".into(),
            name: String::new(),
            version_number: String::new(),
            version_type: "release".into(),
            loaders: loaders.iter().map(|loader| loader.to_string()).collect(),
            game_versions: Vec::new(),
            date_published: String::new(),
            downloads: 0,
            changelog: String::new(),
            files: Vec::new(),
            dependencies: Vec::new(),
        };
        assert_eq!(
            folder(ProjectType::Shader, &version(&["iris"])),
            "shaderpacks"
        );
        assert_eq!(
            folder(ProjectType::Shader, &version(&["vanilla"])),
            "resourcepacks"
        );
        assert_eq!(folder(ProjectType::Mod, &version(&["fabric"])), "mods");
    }

    #[test]
    fn paths_stay_inside_the_game_folder() {
        assert!(safe_join(Path::new("/g"), "mods/a.jar").is_some());
        assert!(safe_join(Path::new("/g"), "../escape.jar").is_none());
        assert!(safe_join(Path::new("/g"), "/etc/passwd").is_none());
    }

    #[test]
    fn removing_forgets_and_deletes() {
        let dir = std::env::temp_dir().join(format!("wisp-content-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(GAME_DIR).join("mods")).unwrap();
        std::fs::write(dir.join(GAME_DIR).join("mods/a.jar"), b"x").unwrap();
        let index = ContentIndex::new(&dir);
        index
            .save(&[InstalledContent {
                project_id: "p".into(),
                version_id: "v".into(),
                title: "A".into(),
                version_number: "1".into(),
                project_type: ProjectType::Mod,
                file: "mods/a.jar".into(),
                icon_url: None,
                explicit: true,
                world: None,
            }])
            .unwrap();
        assert_eq!(index.list().len(), 1);
        index.remove("p", None).unwrap();
        assert!(index.list().is_empty());
        assert!(!dir.join(GAME_DIR).join("mods/a.jar").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
