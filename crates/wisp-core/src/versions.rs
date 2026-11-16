//! Minecraft versions from Mojang, and Fabric loader versions from Fabric's meta service.

use serde::{Deserialize, Serialize};

use crate::http::{Error, HttpClient};
use crate::url_encode;

const MOJANG_MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const FABRIC_LOADERS: &str = "https://meta.fabricmc.net/v2/versions/loader";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameVersion {
    pub id: String,
    /// "release", "snapshot", "old_beta" or "old_alpha".
    pub kind: String,
    /// RFC 3339 release time.
    pub released: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionList {
    pub latest_release: String,
    pub latest_snapshot: String,
    /// Newest first, as Mojang lists them.
    pub versions: Vec<GameVersion>,
}

#[derive(Deserialize)]
struct Manifest {
    latest: Latest,
    versions: Vec<ManifestVersion>,
}

#[derive(Deserialize)]
struct Latest {
    release: String,
    snapshot: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestVersion {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    release_time: String,
}

pub async fn game_versions(client: &HttpClient) -> Result<VersionList, Error> {
    let manifest: Manifest = client
        .get(MOJANG_MANIFEST)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(VersionList {
        latest_release: manifest.latest.release,
        latest_snapshot: manifest.latest.snapshot,
        versions: manifest
            .versions
            .into_iter()
            .map(|version| GameVersion {
                id: version.id,
                kind: version.kind,
                released: version.release_time,
            })
            .collect(),
    })
}

#[derive(Deserialize)]
struct FabricLoaderEntry {
    loader: FabricLoader,
}

#[derive(Deserialize)]
struct FabricLoader {
    version: String,
    stable: bool,
}

/// The newest stable Fabric loader for a Minecraft version, or `None` if Fabric doesn't support it.
pub async fn latest_fabric_loader(
    client: &HttpClient,
    game_version: &str,
) -> Result<Option<String>, Error> {
    let url = format!("{FABRIC_LOADERS}/{}", url_encode(game_version));
    let response = client.get(url).send().await?;
    if response.status().is_client_error() {
        return Ok(None);
    }
    let entries: Vec<FabricLoaderEntry> = response.error_for_status()?.json().await?;
    Ok(entries
        .iter()
        .find(|entry| entry.loader.stable)
        .or(entries.first())
        .map(|entry| entry.loader.version.clone()))
}
