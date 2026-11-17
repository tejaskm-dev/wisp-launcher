//! "Speed up": the well-known performance mods, added to an instance in one go. Sodium redraws
//! the world far more efficiently (often several times the frame rate), Lithium runs the game's
//! own logic faster, FerriteCore cuts memory use, ModernFix shortens loading (most of all with
//! many mods), ImmediatelyFast speeds up menus and text, Entity Culling skips what can't be
//! seen, and Dynamic FPS idles the game while its window is in the background. A plain (vanilla)
//! instance moves to Fabric first, since mods need a loader. Each mod is only added where it has
//! a build for that Minecraft version and loader, and never next to one it clashes with.

use serde::Serialize;

use crate::HttpClient;
use crate::content::{self, ContentError, ContentIndex, InstallRequest, InstalledContent};
use crate::instances::{Instance, InstanceStore, Loader};
use crate::loaders;
use crate::modrinth;

/// Modrinth slugs and names, for Fabric and Quilt (Quilt runs Fabric mods).
const FABRIC: &[(&str, &str)] = &[
    ("sodium", "Sodium"),
    ("lithium", "Lithium"),
    ("ferrite-core", "FerriteCore"),
    ("modernfix", "ModernFix"),
    ("immediatelyfast", "ImmediatelyFast"),
    ("entityculling", "Entity Culling"),
    ("dynamic-fps", "Dynamic FPS"),
];
/// NeoForge: Sodium builds for it from Minecraft 1.21 on; Embeddium covers the versions before.
const NEOFORGE: &[(&str, &str)] = &[
    ("sodium", "Sodium"),
    ("embeddium", "Embeddium"),
    ("ferrite-core", "FerriteCore"),
    ("modernfix", "ModernFix"),
    ("immediatelyfast", "ImmediatelyFast"),
    ("entityculling", "Entity Culling"),
    ("dynamic-fps", "Dynamic FPS"),
];
const FORGE: &[(&str, &str)] = &[
    ("embeddium", "Embeddium"),
    ("ferrite-core", "FerriteCore"),
    ("modernfix", "ModernFix"),
    ("immediatelyfast", "ImmediatelyFast"),
    ("entityculling", "Entity Culling"),
    ("dynamic-fps", "Dynamic FPS"),
];

/// Renderers that replace the game's: only one can be in an instance.
const RENDERERS: &[&str] = &["sodium", "embeddium", "rubidium", "optifine", "optifabric"];

/// What speeding an instance up did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpedUp {
    /// The instance as it is now (its loader may have changed).
    pub instance: Instance,
    /// What was added, with anything those need.
    pub added: Vec<InstalledContent>,
    /// Already in the instance.
    pub had: Vec<String>,
    /// With no build for this Minecraft version and loader, or clashing with something there.
    pub skipped: Vec<String>,
    /// The instance moved from vanilla to Fabric.
    pub moved_to_fabric: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SpeedUpError {
    #[error("Fabric doesn't support Minecraft {0} yet, so mods can't be added to it.")]
    NoFabric(String),
    #[error(transparent)]
    Content(#[from] ContentError),
    #[error(transparent)]
    Instance(#[from] crate::instances::InstanceError),
    #[error("{0}")]
    Network(#[from] crate::Error),
}

/// Adds the performance mods to an instance (moving a vanilla one to Fabric first).
pub async fn speed_up(client: &HttpClient, store: &InstanceStore, instance_id: &str, concurrency: usize) -> Result<SpedUp, SpeedUpError> {
    let mut instance = store.get(instance_id).ok_or(crate::instances::InstanceError::NotFound)?;
    let dir = store.dir_of(instance_id).ok_or(crate::instances::InstanceError::NotFound)?;

    let mut moved_to_fabric = false;
    if instance.loader == Loader::Vanilla {
        let versions = loaders::loader_versions(client, Loader::Fabric, &instance.game_version).await?;
        let loader = versions
            .iter()
            .find(|version| version.stable)
            .or(versions.first())
            .ok_or_else(|| SpeedUpError::NoFabric(instance.game_version.clone()))?
            .version
            .clone();
        instance = store.update(instance_id, |instance| {
            instance.loader = Loader::Fabric;
            instance.loader_version = Some(loader);
        })?;
        moved_to_fabric = true;
    }

    let wanted = match instance.loader {
        Loader::Fabric | Loader::Quilt => FABRIC,
        Loader::NeoForge => NEOFORGE,
        Loader::Forge => FORGE,
        Loader::Vanilla => &[],
    };
    // What's there already, by Modrinth id and by file name (for mods added by hand).
    let installed = ContentIndex::new(&dir).list();
    let slugs: Vec<String> = wanted.iter().map(|(slug, _)| slug.to_string()).collect();
    let projects = modrinth::projects(client, &slugs).await.unwrap_or_default();
    let id_of = |slug: &str| projects.iter().find(|project| project.slug == slug).map(|project| project.id.clone());
    let files: Vec<String> = std::fs::read_dir(dir.join(crate::instances::GAME_DIR).join("mods"))
        .map(|entries| entries.flatten().map(|entry| entry.file_name().to_string_lossy().to_lowercase()).collect())
        .unwrap_or_default();
    let present = |slug: &str| {
        id_of(slug).is_some_and(|id| installed.iter().any(|item| item.project_id == id))
            || files.iter().any(|file| file.starts_with(slug) || file.replace(['-', '_'], "").starts_with(&slug.replace('-', "")))
    };
    let mut renderer = RENDERERS.iter().any(|slug| present(slug));

    let mut sped = SpedUp { instance: instance.clone(), added: Vec::new(), had: Vec::new(), skipped: Vec::new(), moved_to_fabric };
    for (slug, name) in wanted {
        if present(slug) {
            sped.had.push(name.to_string());
            continue;
        }
        let is_renderer = RENDERERS.contains(slug);
        if is_renderer && renderer {
            // Sodium is in, or OptiFine: Embeddium (or a second Sodium) would clash.
            if !(*slug == "embeddium" && instance.loader == Loader::NeoForge) {
                sped.skipped.push(name.to_string());
            }
            continue;
        }
        let request = InstallRequest { project_id: slug, version_id: None, world: None, concurrency, curseforge_key: None };
        match content::install(client, &instance, &dir, request).await {
            Ok(added) => {
                renderer |= is_renderer;
                sped.added.extend(added);
            }
            // No build for this version and loader: Embeddium standing in for Sodium on older
            // NeoForge is the expected case, so only the other one counts as skipped.
            Err(ContentError::NoCompatibleVersion { .. }) => {
                if !(*slug == "sodium" && instance.loader == Loader::NeoForge) {
                    sped.skipped.push(name.to_string());
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(sped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_loader_gets_one_renderer_at_most_in_order() {
        for list in [FABRIC, NEOFORGE, FORGE] {
            assert!(list.iter().any(|(slug, _)| RENDERERS.contains(slug)));
            assert!(list.iter().all(|(slug, name)| !slug.is_empty() && !name.is_empty()));
        }
        // NeoForge tries Sodium before Embeddium, so the newer one wins where both exist.
        let position = |slug: &str| NEOFORGE.iter().position(|(candidate, _)| *candidate == slug).unwrap();
        assert!(position("sodium") < position("embeddium"));
    }
}
