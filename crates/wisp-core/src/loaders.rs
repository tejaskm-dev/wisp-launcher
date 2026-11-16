//! What each mod loader supports: which Minecraft versions, and its own versions for each one.

use std::cmp::Ordering;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::http::{Error, HttpClient};
use crate::instances::Loader;
use crate::url_encode;

const FABRIC_META: &str = "https://meta.fabricmc.net/v2/versions";
const QUILT_META: &str = "https://meta.quiltmc.org/v3/versions";
const FORGE_METADATA: &str =
    "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml";
const FORGE_PROMOTIONS: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
const NEOFORGE_VERSIONS: &str =
    "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderVersion {
    pub version: String,
    /// Not marked beta or pre-release.
    pub stable: bool,
    /// The one to use unless you need another: Forge's "recommended", else the newest stable.
    pub recommended: bool,
}

#[derive(Deserialize)]
struct MetaGame {
    version: String,
}

#[derive(Deserialize)]
struct MetaLoaderEntry {
    loader: MetaLoader,
}

#[derive(Deserialize)]
struct MetaLoader {
    version: String,
    #[serde(default)]
    stable: Option<bool>,
}

#[derive(Deserialize)]
struct NeoForgeVersions {
    versions: Vec<String>,
}

#[derive(Deserialize)]
struct ForgePromotions {
    promos: HashMap<String, String>,
}

/// The Minecraft versions a loader supports; `None` means every version (vanilla).
pub async fn game_versions(
    client: &HttpClient,
    loader: Loader,
) -> Result<Option<Vec<String>>, Error> {
    let versions = match loader {
        Loader::Vanilla => return Ok(None),
        Loader::Fabric | Loader::Quilt => {
            let base = if loader == Loader::Fabric {
                FABRIC_META
            } else {
                QUILT_META
            };
            let games: Vec<MetaGame> = get_json(client, &format!("{base}/game")).await?;
            games.into_iter().map(|game| game.version).collect()
        }
        Loader::Forge => forge_versions(client)
            .await?
            .iter()
            .filter_map(|version| version.split_once('-').map(|(game, _)| game.to_string()))
            .collect(),
        Loader::NeoForge => neoforge_versions(client)
            .await?
            .iter()
            .filter_map(|version| neoforge_game_version(version))
            .collect(),
    };
    Ok(Some(dedupe(versions)))
}

/// The loader's versions for one Minecraft version, newest first.
pub async fn loader_versions(
    client: &HttpClient,
    loader: Loader,
    game: &str,
) -> Result<Vec<LoaderVersion>, Error> {
    let mut versions: Vec<LoaderVersion> = match loader {
        Loader::Vanilla => return Ok(Vec::new()),
        Loader::Fabric | Loader::Quilt => {
            let base = if loader == Loader::Fabric {
                FABRIC_META
            } else {
                QUILT_META
            };
            let response = client
                .get(format!("{base}/loader/{}", url_encode(game)))
                .send()
                .await?;
            if response.status().is_client_error() {
                return Ok(Vec::new());
            }
            let entries: Vec<MetaLoaderEntry> = response.error_for_status()?.json().await?;
            entries
                .into_iter()
                .map(|entry| LoaderVersion {
                    stable: entry
                        .loader
                        .stable
                        .unwrap_or_else(|| !is_prerelease(&entry.loader.version)),
                    version: entry.loader.version,
                    recommended: false,
                })
                .collect()
        }
        Loader::Forge => {
            let prefix = format!("{game}-");
            let mut versions: Vec<LoaderVersion> = forge_versions(client)
                .await?
                .iter()
                .filter_map(|version| version.strip_prefix(&prefix))
                .map(|version| LoaderVersion {
                    version: version.to_string(),
                    stable: true,
                    recommended: false,
                })
                .collect();
            // Forge names a recommended build per Minecraft version (or at least a latest one).
            if let Ok(promotions) = get_json::<ForgePromotions>(client, FORGE_PROMOTIONS).await {
                let pick = promotions
                    .promos
                    .get(&format!("{game}-recommended"))
                    .or_else(|| promotions.promos.get(&format!("{game}-latest")));
                if let Some(pick) = pick {
                    versions
                        .iter_mut()
                        .filter(|version| &version.version == pick)
                        .for_each(|version| version.recommended = true);
                }
            }
            versions
        }
        Loader::NeoForge => neoforge_versions(client)
            .await?
            .into_iter()
            .filter(|version| neoforge_game_version(version).as_deref() == Some(game))
            .map(|version| LoaderVersion {
                stable: !is_prerelease(&version),
                version,
                recommended: false,
            })
            .collect(),
    };

    versions.sort_by(|a, b| compare_versions(&b.version, &a.version));
    if !versions.iter().any(|version| version.recommended) {
        let pick = versions
            .iter()
            .position(|version| version.stable)
            .or((!versions.is_empty()).then_some(0));
        if let Some(index) = pick {
            versions[index].recommended = true;
        }
    }
    Ok(versions)
}

/// The version to use when the player doesn't pick one, or `None` if the loader doesn't support
/// this Minecraft version.
pub async fn recommended(
    client: &HttpClient,
    loader: Loader,
    game: &str,
) -> Result<Option<String>, Error> {
    Ok(loader_versions(client, loader, game)
        .await?
        .into_iter()
        .find(|version| version.recommended)
        .map(|version| version.version))
}

async fn get_json<T: serde::de::DeserializeOwned>(
    client: &HttpClient,
    url: &str,
) -> Result<T, Error> {
    Ok(client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

/// Every Forge build, as `<minecraft>-<forge>` (from Forge's Maven metadata).
async fn forge_versions(client: &HttpClient) -> Result<Vec<String>, Error> {
    let xml = client
        .get(FORGE_METADATA)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(xml
        .split("<version>")
        .skip(1)
        .filter_map(|chunk| {
            chunk
                .split_once("</version>")
                .map(|(version, _)| version.trim().to_string())
        })
        .collect())
}

async fn neoforge_versions(client: &HttpClient) -> Result<Vec<String>, Error> {
    Ok(get_json::<NeoForgeVersions>(client, NEOFORGE_VERSIONS)
        .await?
        .versions)
}

/// NeoForge numbers its builds after the Minecraft version: 21.1.77 is for 1.21.1, 20.4.x for
/// 1.20.4, and since Minecraft's 26.x versions, 26.1.2.126 is for 26.1.2 and 26.3.0.103 for 26.3.
pub fn neoforge_game_version(version: &str) -> Option<String> {
    if version.contains('+') || version.contains("alpha") || version.starts_with("0.") {
        return None;
    }
    let core = version.split('-').next()?;
    let parts: Vec<&str> = core.split('.').collect();
    let major: u32 = parts.first()?.parse().ok()?;
    let minor = *parts.get(1)?;
    if major >= 26 {
        let patch = *parts.get(2)?;
        Some(if patch == "0" {
            format!("{major}.{minor}")
        } else {
            format!("{major}.{minor}.{patch}")
        })
    } else {
        Some(if minor == "0" {
            format!("1.{major}")
        } else {
            format!("1.{major}.{minor}")
        })
    }
}

fn is_prerelease(version: &str) -> bool {
    let lower = version.to_ascii_lowercase();
    ["beta", "alpha", "pre", "rc"]
        .iter()
        .any(|tag| lower.contains(tag))
}

fn dedupe(versions: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    versions
        .into_iter()
        .filter(|version| seen.insert(version.clone()))
        .collect()
}

/// Orders version strings the way people read them: number by number ("47.10" after "47.9"),
/// with a pre-release ("1.0-beta") before its release ("1.0").
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    fn tokens(version: &str) -> Vec<Result<u64, String>> {
        let mut out = Vec::new();
        let mut current = String::new();
        let mut digits = false;
        for c in version.chars() {
            if c.is_ascii_digit() != digits && !current.is_empty() {
                out.push(if digits {
                    Ok(current.parse().unwrap_or(u64::MAX))
                } else {
                    Err(current.clone())
                });
                current.clear();
            }
            digits = c.is_ascii_digit();
            current.push(c);
        }
        if !current.is_empty() {
            out.push(if digits {
                Ok(current.parse().unwrap_or(u64::MAX))
            } else {
                Err(current)
            });
        }
        out
    }
    let (left, right) = (tokens(a), tokens(b));
    for (x, y) in left.iter().zip(&right) {
        let order = match (x, y) {
            (Ok(x), Ok(y)) => x.cmp(y),
            (Err(x), Err(y)) => x.cmp(y),
            // A number beats a word at the same spot: "1.0.1" after "1.0-beta".
            (Ok(_), Err(_)) => Ordering::Greater,
            (Err(_), Ok(_)) => Ordering::Less,
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    // "1.0" vs "1.0-beta": the release (shorter, or followed by a separator) is newer.
    match left.len().cmp(&right.len()) {
        Ordering::Equal => Ordering::Equal,
        Ordering::Less => {
            if is_prerelease(b) && !is_prerelease(a) {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        }
        Ordering::Greater => {
            if is_prerelease(a) && !is_prerelease(b) {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neoforge_versions_map_to_minecraft_versions() {
        assert_eq!(neoforge_game_version("21.1.77").as_deref(), Some("1.21.1"));
        assert_eq!(neoforge_game_version("21.0.167").as_deref(), Some("1.21"));
        assert_eq!(neoforge_game_version("20.4.251").as_deref(), Some("1.20.4"));
        assert_eq!(neoforge_game_version("26.3.0.103").as_deref(), Some("26.3"));
        assert_eq!(
            neoforge_game_version("26.1.2.126").as_deref(),
            Some("26.1.2")
        );
        assert_eq!(
            neoforge_game_version("26.2.0.1-beta").as_deref(),
            Some("26.2")
        );
        assert_eq!(neoforge_game_version("26.1.0.0-alpha.1+snapshot-1"), None);
        assert_eq!(neoforge_game_version("0.25w14craftmine.3-beta"), None);
    }

    #[test]
    fn versions_sort_the_way_people_read_them() {
        let mut versions = vec![
            "47.9.0",
            "47.10.0",
            "47.2.0",
            "0.31.0-beta.6",
            "0.31.0",
            "0.31.0-beta.10",
            "0.30.2",
        ];
        versions.sort_by(|a, b| compare_versions(b, a));
        assert_eq!(
            versions,
            [
                "47.10.0",
                "47.9.0",
                "47.2.0",
                "0.31.0",
                "0.31.0-beta.10",
                "0.31.0-beta.6",
                "0.30.2"
            ]
        );
    }

    #[test]
    fn prereleases_are_not_stable() {
        assert!(is_prerelease("0.31.0-beta.6"));
        assert!(is_prerelease("26.2.0.1-beta"));
        assert!(!is_prerelease("21.1.77"));
    }
}
