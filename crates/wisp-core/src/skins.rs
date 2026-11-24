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
/// More layers than anyone paints a 64×64 skin with.
const MAX_LAYERS: usize = 32;

/// One of the editor's layers, kept so a skin can be reopened with them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinLayer {
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    /// A 64×64 PNG, base64.
    pub png: String,
}

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
    /// Outer layers kept to wear over any skin: a suit, a hoodie, armour.
    pub cosmetics: Vec<Cosmetic>,
    /// The cosmetic worn over the skin, if any (its outer layer replaces the skin's own).
    pub worn_cosmetic: Option<String>,
}

/// A skin's outer layer (hat, jacket, sleeves, trousers) kept on its own, to wear over any skin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cosmetic {
    pub id: String,
    pub name: String,
    /// Unix milliseconds.
    pub added: i64,
}

/// The outer layer's parts on a 64×64 skin: hat, jacket, right sleeve, left sleeve, right and
/// left trouser legs.
const OUTER: [(u32, u32, u32, u32); 6] = [(32, 0, 32, 16), (16, 32, 24, 16), (40, 32, 16, 16), (48, 48, 16, 16), (0, 32, 16, 16), (0, 48, 16, 16)];

/// Whether a pixel of a 64×64 skin is on its outer layer.
pub fn is_outer(x: u32, y: u32) -> bool {
    OUTER.iter().any(|&(left, top, width, height)| x >= left && x < left + width && y >= top && y < top + height)
}

/// The outer layer of a skin, alone: everything else clear.
pub fn outer_layer(png: &[u8]) -> Result<Vec<u8>, SkinError> {
    let mut picture = image::load_from_memory_with_format(&normalize_png(png)?, image::ImageFormat::Png).map_err(|_| SkinError::NotPng)?.to_rgba8();
    for (x, y, pixel) in picture.enumerate_pixels_mut() {
        if !is_outer(x, y) {
            *pixel = image::Rgba([0, 0, 0, 0]);
        }
    }
    let mut out = Vec::new();
    picture.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).map_err(|error| SkinError::NotASkin(error.to_string()))?;
    Ok(out)
}

/// A skin wearing a cosmetic: its own outer layer replaced by the cosmetic's.
pub fn wear(skin_png: &[u8], cosmetic_png: &[u8]) -> Result<Vec<u8>, SkinError> {
    let mut skin = image::load_from_memory_with_format(&normalize_png(skin_png)?, image::ImageFormat::Png).map_err(|_| SkinError::NotPng)?.to_rgba8();
    let cosmetic = image::load_from_memory_with_format(cosmetic_png, image::ImageFormat::Png).map_err(|_| SkinError::NotPng)?.to_rgba8();
    if cosmetic.dimensions() != (64, 64) {
        return Err(SkinError::NotASkin("a cosmetic is 64 by 64".into()));
    }
    for (x, y, pixel) in skin.enumerate_pixels_mut() {
        if is_outer(x, y) {
            *pixel = *cosmetic.get_pixel(x, y);
        }
    }
    let mut out = Vec::new();
    skin.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).map_err(|error| SkinError::NotASkin(error.to_string()))?;
    Ok(out)
}

#[derive(Debug, thiserror::Error)]
pub enum SkinError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("that isn't a Minecraft skin: {0}")]
    NotASkin(String),
    #[error("that skin is gone")]
    Missing,
    #[error("that isn't a PNG")]
    NotPng,
    #[error("that's more layers than a skin can keep ({MAX_LAYERS})")]
    TooManyLayers,
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

    fn layers_path(&self, id: &str) -> PathBuf {
        self.png_path(id).with_extension("layers.json")
    }

    /// The editor's layers for a skin, if it was saved with them.
    pub fn layers(&self, id: &str) -> Option<Vec<SkinLayer>> {
        let bytes = std::fs::read(self.layers_path(id)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Keeps the editor's layers beside a skin (the skin itself is them flattened). An empty list
    /// forgets them.
    pub fn set_layers(&self, id: &str, layers: &[SkinLayer]) -> Result<(), SkinError> {
        if !self.library().skins.iter().any(|skin| skin.id == id) {
            return Err(SkinError::Missing);
        }
        if layers.is_empty() {
            let _ = std::fs::remove_file(self.layers_path(id));
            return Ok(());
        }
        if layers.len() > MAX_LAYERS {
            return Err(SkinError::TooManyLayers);
        }
        let mut checked = Vec::with_capacity(layers.len());
        for layer in layers {
            let png = normalize_png(&base64::engine::general_purpose::STANDARD.decode(&layer.png).map_err(|_| SkinError::NotPng)?)?;
            checked.push(SkinLayer { name: clean_name(&layer.name), visible: layer.visible, opacity: layer.opacity.clamp(0.0, 1.0), png: base64::engine::general_purpose::STANDARD.encode(png) });
        }
        std::fs::write(self.layers_path(id), serde_json::to_vec(&checked).map_err(std::io::Error::other)?)?;
        Ok(())
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
        let _ = std::fs::remove_file(self.layers_path(id));
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

    /// Where a cosmetic's picture is.
    pub fn cosmetic_path(&self, id: &str) -> PathBuf {
        let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
        self.dir.join("cosmetics").join(format!("{safe}.png"))
    }

    /// Keeps a skin's outer layer as a cosmetic (only the outer layer of `png` is kept).
    pub fn add_cosmetic(&self, name: &str, png: &[u8], now: i64) -> Result<Cosmetic, SkinError> {
        let outer = outer_layer(png)?;
        let mut library = self.library();
        let cosmetic = Cosmetic { id: format!("cosmetic-{}", crate::journal::new_id(now)), name: clean_name(name).replace("Untitled skin", "Untitled cosmetic"), added: now };
        let path = self.cosmetic_path(&cosmetic.id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, outer)?;
        library.cosmetics.push(cosmetic.clone());
        self.save(&library)?;
        Ok(cosmetic)
    }

    pub fn rename_cosmetic(&self, id: &str, name: &str) -> Result<(), SkinError> {
        let mut library = self.library();
        let cosmetic = library.cosmetics.iter_mut().find(|cosmetic| cosmetic.id == id).ok_or(SkinError::Missing)?;
        cosmetic.name = clean_name(name);
        self.save(&library)?;
        Ok(())
    }

    pub fn remove_cosmetic(&self, id: &str) -> Result<(), SkinError> {
        let mut library = self.library();
        library.cosmetics.retain(|cosmetic| cosmetic.id != id);
        if library.worn_cosmetic.as_deref() == Some(id) {
            library.worn_cosmetic = None;
        }
        self.save(&library)?;
        let _ = std::fs::remove_file(self.cosmetic_path(id));
        Ok(())
    }

    /// Wears a cosmetic over whatever skin is worn (none takes it off).
    pub fn wear_cosmetic(&self, id: Option<&str>) -> Result<(), SkinError> {
        let mut library = self.library();
        if let Some(id) = id
            && !library.cosmetics.iter().any(|cosmetic| cosmetic.id == id)
        {
            return Err(SkinError::Missing);
        }
        library.worn_cosmetic = id.map(str::to_string);
        self.save(&library)?;
        Ok(())
    }

    /// The worn skin as the game gets it (with the worn cosmetic over it), and whether it has slim arms.
    pub fn worn_png(&self) -> Option<(Vec<u8>, bool)> {
        let library = self.library();
        let skin = library.active.as_ref().and_then(|id| library.skins.iter().find(|skin| &skin.id == id))?;
        let png = std::fs::read(self.png_path(&skin.id)).ok()?;
        let png = match library.worn_cosmetic.as_deref().and_then(|id| std::fs::read(self.cosmetic_path(id)).ok()) {
            Some(cosmetic) => wear(&png, &cosmetic).unwrap_or(png),
            None => png,
        };
        Some((png, skin.model == SkinModel::Slim))
    }
}

/// The face friends see: the head's front with the hat layer drawn over it, as an 8×8 PNG.
pub fn face_png(png: &[u8]) -> Option<Vec<u8>> {
    let skin = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?.to_rgba8();
    if skin.width() < 48 || skin.height() < 16 {
        return None;
    }
    let mut face = image::RgbaImage::new(8, 8);
    for y in 0..8 {
        for x in 0..8 {
            face.put_pixel(x, y, image::Rgba(over(skin.get_pixel(40 + x, 8 + y).0, skin.get_pixel(8 + x, 8 + y).0)));
        }
    }
    let mut out = Vec::new();
    face.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).ok()?;
    Some(out)
}

/// `top` drawn over `bottom`, the way a canvas draws one picture on another.
fn over(top: [u8; 4], bottom: [u8; 4]) -> [u8; 4] {
    let (above, below) = (top[3] as f32 / 255.0, bottom[3] as f32 / 255.0);
    let alpha = above + below * (1.0 - above);
    if alpha <= 0.0 {
        return [0; 4];
    }
    let mix = |t: u8, b: u8| ((t as f32 * above + b as f32 * below * (1.0 - above)) / alpha).round() as u8;
    [mix(top[0], bottom[0]), mix(top[1], bottom[1]), mix(top[2], bottom[2]), (alpha * 255.0).round() as u8]
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

    fn filled(color: [u8; 4]) -> Vec<u8> {
        let picture = image::RgbaImage::from_pixel(64, 64, image::Rgba(color));
        let mut out = Vec::new();
        picture.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    }

    #[test]
    fn the_face_friends_see_has_the_hat_layer_over_it() {
        let mut picture = image::RgbaImage::from_pixel(64, 64, image::Rgba([0, 0, 0, 0]));
        for y in 8..16 {
            for x in 8..16 {
                picture.put_pixel(x, y, image::Rgba([240, 200, 170, 255]));
            }
        }
        // A fringe on the hat layer's top row, and one half-clear pixel.
        for x in 40..48 {
            picture.put_pixel(x, 8, image::Rgba([60, 40, 30, 255]));
        }
        picture.put_pixel(40, 12, image::Rgba([0, 0, 255, 128]));
        let mut png = Vec::new();
        picture.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let face = image::load_from_memory(&face_png(&png).unwrap()).unwrap().to_rgba8();
        assert_eq!(face.dimensions(), (8, 8));
        assert_eq!(face.get_pixel(3, 0).0, [60, 40, 30, 255], "the fringe covers the face");
        assert_eq!(face.get_pixel(3, 5).0, [240, 200, 170, 255], "skin where the hat layer is clear");
        assert_eq!(face.get_pixel(0, 4).0, [120, 100, 213, 255], "half-clear hat pixels blend");
        assert!(face_png(b"not a picture").is_none());
    }

    #[test]
    fn cosmetics_keep_only_the_outer_layer_and_go_over_any_skin() {
        let dir = std::env::temp_dir().join(format!("wisp-cosmetics-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = SkinStore::new(&dir);
        let skin = store.add("Me", SkinModel::Classic, &filled([200, 150, 120, 255]), None, 1).unwrap();
        store.set_active(Some(&skin.id)).unwrap();
        // A "suit": a whole skin in navy, of which only the outer layer is kept.
        let suit = store.add_cosmetic("Suit", &filled([20, 30, 70, 255]), 2).unwrap();
        let kept = image::load_from_memory(&std::fs::read(store.cosmetic_path(&suit.id)).unwrap()).unwrap().to_rgba8();
        assert_eq!(kept.get_pixel(40, 8).0, [20, 30, 70, 255], "hat");
        assert_eq!(kept.get_pixel(8, 8).0[3], 0, "the face isn't part of it");
        store.wear_cosmetic(Some(&suit.id)).unwrap();
        let (worn, slim) = store.worn_png().unwrap();
        let worn = image::load_from_memory(&worn).unwrap().to_rgba8();
        assert!(!slim);
        assert_eq!(worn.get_pixel(8, 8).0, [200, 150, 120, 255], "the skin's own face");
        assert_eq!(worn.get_pixel(20, 36).0, [20, 30, 70, 255], "the suit's jacket");
        assert_eq!(worn.get_pixel(20, 20).0, [200, 150, 120, 255], "the skin's own body underneath");
        store.remove_cosmetic(&suit.id).unwrap();
        assert_eq!(store.library().worn_cosmetic, None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let picture = image::RgbaImage::from_fn(width, height, |x, y| image::Rgba([(x * 4 % 256) as u8, (y * 4 % 256) as u8, 128, 255]));
        let mut out = Vec::new();
        picture.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    }

    #[test]
    fn keeps_the_editors_layers_beside_a_skin() {
        let dir = std::env::temp_dir().join(format!("wisp-skin-layers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = SkinStore::new(&dir);
        let skin = store.add("Layered", SkinModel::Classic, &png(64, 64), None, 10).unwrap();
        assert!(store.layers(&skin.id).is_none());
        let layer = |name: &str| SkinLayer { name: name.into(), visible: true, opacity: 1.5, png: base64::engine::general_purpose::STANDARD.encode(png(64, 64)) };
        store.set_layers(&skin.id, &[layer("Base"), layer("Shading")]).unwrap();
        let kept = store.layers(&skin.id).unwrap();
        assert_eq!(kept.iter().map(|layer| layer.name.as_str()).collect::<Vec<_>>(), ["Base", "Shading"]);
        assert_eq!(kept[0].opacity, 1.0);
        let broken = SkinLayer { png: "not base64!".into(), ..layer("Broken") };
        assert!(store.set_layers(&skin.id, &[broken]).is_err());
        assert!(matches!(store.set_layers("missing", &[layer("X")]), Err(SkinError::Missing)));
        store.remove(&skin.id).unwrap();
        assert!(store.layers(&skin.id).is_none());
        std::fs::remove_dir_all(dir).unwrap();
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
