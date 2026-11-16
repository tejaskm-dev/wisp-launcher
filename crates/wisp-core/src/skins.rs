//! The skin library: skins you've imported, made or edited, kept as PNGs in skins/ with an index.
//! Also fetching any player's current skin from Mojang by their username.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::HttpClient;

const INDEX: &str = "skins.json";
/// A skin is 64×64 (or the old 64×32); anything big is not a skin.
const MAX_PNG_BYTES: usize = 512 * 1024;
const NAME_LIMIT: usize = 48;

/// The arm width a skin is drawn for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkinModel {
    /// Steve's 4-pixel arms.
    #[default]
    Classic,
    /// Alex's 3-pixel arms.
    Slim,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skin {
    pub id: String,
    pub name: String,
    pub model: SkinModel,
    /// Unix milliseconds.
    pub added: i64,
    pub updated: i64,
    /// Where it came from: "file", "player:<name>", "editor".
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SkinLibrary {
    pub skins: Vec<Skin>,
    /// The skin you've picked to wear.
    pub active: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SkinError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("that isn't a Minecraft skin: {0}")]
    NotASkin(String),
    #[error("that skin is gone")]
    Missing,
    #[error("there's no Minecraft player called {0}")]
    NoSuchPlayer(String),
    #[error("couldn't reach Mojang: {0}")]
    Network(String),
}

pub struct SkinStore {
    dir: PathBuf,
}

impl SkinStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn library(&self) -> SkinLibrary {
        std::fs::read(self.dir.join(INDEX))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<SkinLibrary>(&bytes).ok())
            .map(|mut library| {
                // Drop entries whose picture went missing.
                library.skins.retain(|skin| self.png_path(&skin.id).is_file());
                if library.active.as_ref().is_some_and(|active| !library.skins.iter().any(|skin| &skin.id == active)) {
                    library.active = None;
                }
                library
            })
            .unwrap_or_default()
    }

    fn save(&self, library: &SkinLibrary) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let temp = self.dir.join(format!("{INDEX}.tmp"));
        std::fs::write(&temp, serde_json::to_vec_pretty(library).map_err(std::io::Error::other)?)?;
        std::fs::rename(temp, self.dir.join(INDEX))
    }

    /// Where a skin's picture is. Ids are generated here, so anything else is refused.
    pub fn png_path(&self, id: &str) -> PathBuf {
        let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
        self.dir.join(format!("{safe}.png"))
    }

    /// Adds a skin. The picture is checked and stored as a 64×64 PNG.
    pub fn add(&self, name: &str, model: SkinModel, png: &[u8], source: Option<String>, now: i64) -> Result<Skin, SkinError> {
        let png = normalize_png(png)?;
        let mut library = self.library();
        let skin = Skin {
            id: crate::journal::new_id(now),
            name: clean_name(name),
            model,
            added: now,
            updated: now,
            source,
        };
        std::fs::create_dir_all(&self.dir)?;
        std::fs::write(self.png_path(&skin.id), png)?;
        library.skins.insert(0, skin.clone());
        // The first skin you add is the one you wear, until you pick another.
        if library.active.is_none() {
            library.active = Some(skin.id.clone());
        }
        self.save(&library)?;
        Ok(skin)
    }

    /// Renames a skin, changes its model, or replaces its picture.
    pub fn update(&self, id: &str, name: Option<&str>, model: Option<SkinModel>, png: Option<&[u8]>, now: i64) -> Result<Skin, SkinError> {
        let mut library = self.library();
        let skin = library.skins.iter_mut().find(|skin| skin.id == id).ok_or(SkinError::Missing)?;
        if let Some(png) = png {
            std::fs::write(self.png_path(id), normalize_png(png)?)?;
        }
        if let Some(name) = name {
            skin.name = clean_name(name);
        }
        if let Some(model) = model {
            skin.model = model;
        }
        skin.updated = now;
        let updated = skin.clone();
        self.save(&library)?;
        Ok(updated)
    }

    pub fn remove(&self, id: &str) -> Result<(), SkinError> {
        let mut library = self.library();
        let before = library.skins.len();
        library.skins.retain(|skin| skin.id != id);
        if library.skins.len() == before {
            return Err(SkinError::Missing);
        }
        if library.active.as_deref() == Some(id) {
            library.active = None;
        }
        self.save(&library)?;
        let _ = std::fs::remove_file(self.png_path(id));
        Ok(())
    }

    pub fn set_active(&self, id: Option<&str>) -> Result<(), SkinError> {
        let mut library = self.library();
        if let Some(id) = id
            && !library.skins.iter().any(|skin| skin.id == id)
        {
            return Err(SkinError::Missing);
        }
        library.active = id.map(str::to_string);
        self.save(&library)?;
        Ok(())
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

fn clean_name(name: &str) -> String {
    let name: String = name.trim().chars().filter(|c| !c.is_control()).take(NAME_LIMIT).collect();
    if name.is_empty() { "Untitled skin".to_string() } else { name }
}

/// Checks that `bytes` is a skin and returns it as a 64×64 PNG. Old 64×32 skins get their left
/// arm and leg mirrored from the right ones, the way Minecraft converts them.
pub fn normalize_png(bytes: &[u8]) -> Result<Vec<u8>, SkinError> {
    if bytes.len() > MAX_PNG_BYTES {
        return Err(SkinError::NotASkin("the file is too big".into()));
    }
    let picture = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .map_err(|_| SkinError::NotASkin("it isn't a PNG picture".into()))?
        .to_rgba8();
    let picture = match picture.dimensions() {
        (64, 64) => picture,
        (64, 32) => legacy_to_modern(&picture),
        (width, height) => {
            return Err(SkinError::NotASkin(format!("it's {width}×{height} pixels; skins are 64×64")));
        }
    };
    let mut out = Vec::new();
    picture
        .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|error| SkinError::NotASkin(error.to_string()))?;
    Ok(out)
}

/// Guesses whether a 64×64 skin is drawn for slim arms: slim skins leave the strip beside each
/// arm empty (or fill it with one flat colour), as skinview3d and the official launcher assume.
pub fn guess_model(png: &[u8]) -> SkinModel {
    let Ok(picture) = image::load_from_memory_with_format(png, image::ImageFormat::Png).map(|picture| picture.to_rgba8()) else {
        return SkinModel::Classic;
    };
    if picture.dimensions() != (64, 64) {
        return SkinModel::Classic;
    }
    const STRIPS: [(u32, u32, u32, u32); 4] = [(50, 16, 2, 4), (54, 20, 2, 12), (42, 48, 2, 4), (46, 52, 2, 12)];
    let mut pixels = Vec::new();
    for (x, y, width, height) in STRIPS {
        for py in y..y + height {
            for px in x..x + width {
                pixels.push(*picture.get_pixel(px, py));
            }
        }
    }
    let transparent = pixels.iter().any(|pixel| pixel[3] != 255);
    let flat = |value: u8| pixels.iter().all(|pixel| pixel[0] == value && pixel[1] == value && pixel[2] == value);
    if transparent || flat(0) || flat(255) { SkinModel::Slim } else { SkinModel::Classic }
}

/// Minecraft's conversion of 64×32 skins: (source x, y, width, height, target x, y), mirrored.
const LEGACY_COPIES: [(u32, u32, u32, u32, u32, u32); 12] = [
    (4, 16, 4, 4, 20, 48),
    (8, 16, 4, 4, 24, 48),
    (0, 20, 4, 12, 24, 52),
    (4, 20, 4, 12, 20, 52),
    (8, 20, 4, 12, 16, 52),
    (12, 20, 4, 12, 28, 52),
    (44, 16, 4, 4, 36, 48),
    (48, 16, 4, 4, 40, 48),
    (40, 20, 4, 12, 40, 52),
    (44, 20, 4, 12, 36, 52),
    (48, 20, 4, 12, 32, 52),
    (52, 20, 4, 12, 44, 52),
];

fn legacy_to_modern(old: &image::RgbaImage) -> image::RgbaImage {
    let mut skin = image::RgbaImage::new(64, 64);
    image::imageops::replace(&mut skin, old, 0, 0);
    for (sx, sy, width, height, dx, dy) in LEGACY_COPIES {
        for y in 0..height {
            for x in 0..width {
                let pixel = *old.get_pixel(sx + x, sy + y);
                skin.put_pixel(dx + (width - 1 - x), dy + y, pixel);
            }
        }
    }
    skin
}

/// A player's current skin, as Mojang serves it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSkin {
    /// The player's name with Mojang's capitalisation.
    pub name: String,
    pub model: SkinModel,
    /// The skin as a base64 PNG.
    pub png: String,
}

#[derive(Deserialize)]
struct ProfileLookup {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct SessionProfile {
    properties: Vec<ProfileProperty>,
}

#[derive(Deserialize)]
struct ProfileProperty {
    name: String,
    value: String,
}

/// Looks up a player by name and downloads their skin. Mojang's public profile API needs no
/// sign-in.
pub async fn fetch_player_skin(client: &HttpClient, username: &str) -> Result<PlayerSkin, SkinError> {
    let username = username.trim();
    if !crate::accounts::valid_username(username) {
        return Err(SkinError::NoSuchPlayer(username.to_string()));
    }
    let network = |error: reqwest::Error| SkinError::Network(error.to_string());
    let response = client
        .get(format!("https://api.mojang.com/users/profiles/minecraft/{}", crate::url_encode(username)))
        .send()
        .await
        .map_err(network)?;
    if matches!(response.status().as_u16(), 204 | 404) {
        return Err(SkinError::NoSuchPlayer(username.to_string()));
    }
    let profile: ProfileLookup = response.error_for_status().map_err(network)?.json().await.map_err(network)?;
    let session: SessionProfile = client
        .get(format!("https://sessionserver.mojang.com/session/minecraft/profile/{}", profile.id))
        .send()
        .await
        .map_err(network)?
        .error_for_status()
        .map_err(network)?
        .json()
        .await
        .map_err(network)?;
    let textures = session
        .properties
        .iter()
        .find(|property| property.name == "textures")
        .and_then(|property| base64::engine::general_purpose::STANDARD.decode(&property.value).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .ok_or_else(|| SkinError::Network("Mojang sent a profile without skin details".into()))?;
    let skin = &textures["textures"]["SKIN"];
    let Some(url) = skin["url"].as_str() else {
        // No custom skin: the player wears one of the defaults, which Mojang doesn't serve here.
        return Err(SkinError::NotASkin(format!("{} wears one of Minecraft's default skins", profile.name)));
    };
    let model = if skin["metadata"]["model"].as_str() == Some("slim") { SkinModel::Slim } else { SkinModel::Classic };
    let url = url.replacen("http://", "https://", 1);
    if !url.starts_with("https://textures.minecraft.net/") {
        return Err(SkinError::Network("Mojang pointed somewhere unexpected for the skin".into()));
    }
    let bytes = client
        .get(url)
        .send()
        .await
        .map_err(network)?
        .error_for_status()
        .map_err(network)?
        .bytes()
        .await
        .map_err(network)?;
    let png = normalize_png(&bytes)?;
    Ok(PlayerSkin {
        name: profile.name,
        model,
        png: base64::engine::general_purpose::STANDARD.encode(png),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let picture = image::RgbaImage::from_fn(width, height, |x, y| image::Rgba([(x * 4 % 256) as u8, (y * 4 % 256) as u8, 128, 255]));
        let mut out = Vec::new();
        picture.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    }

    #[test]
    fn keeps_a_library_of_checked_skins() {
        let dir = std::env::temp_dir().join(format!("wisp-skins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = SkinStore::new(&dir);
        let skin = store.add("  Ghost  ", SkinModel::Slim, &png(64, 64), Some("file".into()), 10).unwrap();
        assert_eq!(skin.name, "Ghost");
        assert!(store.png_path(&skin.id).is_file());
        assert_eq!(store.library().active.as_deref(), Some(skin.id.as_str()), "the first skin is worn");
        assert!(matches!(store.add("Big", SkinModel::Classic, &png(128, 128), None, 11), Err(SkinError::NotASkin(_))));
        assert!(matches!(store.add("Text", SkinModel::Classic, b"hello", None, 11), Err(SkinError::NotASkin(_))));

        store.set_active(Some(&skin.id)).unwrap();
        let renamed = store.update(&skin.id, Some("Wisp"), Some(SkinModel::Classic), None, 12).unwrap();
        assert_eq!((renamed.name.as_str(), renamed.model, renamed.updated), ("Wisp", SkinModel::Classic, 12));
        assert_eq!(store.library().active.as_deref(), Some(skin.id.as_str()));
        store.remove(&skin.id).unwrap();
        assert_eq!(store.library(), SkinLibrary::default());
        assert!(store.png_path("../../etc/passwd").starts_with(&dir));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn guesses_slim_skins_from_the_empty_strip() {
        let classic = png(64, 64);
        assert_eq!(guess_model(&classic), SkinModel::Classic);
        let mut slim = image::load_from_memory(&classic).unwrap().to_rgba8();
        for y in 20..32 {
            for x in 54..56 {
                slim.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
            }
        }
        let mut bytes = Vec::new();
        slim.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        assert_eq!(guess_model(&bytes), SkinModel::Slim);
    }

    #[test]
    fn converts_old_skins_like_minecraft() {
        let modern = normalize_png(&png(64, 32)).unwrap();
        let picture = image::load_from_memory(&modern).unwrap().to_rgba8();
        assert_eq!(picture.dimensions(), (64, 64));
        // The right leg's front (4..8, 20..32) mirrored onto the left leg's front (20..24, 52..64).
        assert_eq!(picture.get_pixel(20, 52), picture.get_pixel(7, 20));
        assert_eq!(picture.get_pixel(23, 52), picture.get_pixel(4, 20));
    }
}
