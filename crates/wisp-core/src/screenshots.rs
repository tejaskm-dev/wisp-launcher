//! Screenshots in an instance's screenshots folder, and small thumbnails of them for the UI.

use std::io::{BufWriter, Read};
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use sha1::{Digest, Sha1};

pub const SCREENSHOTS: &str = "screenshots";
const EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Screenshot {
    /// The file's name in the screenshots folder.
    pub file: String,
    /// When it was taken (the file's time), in Unix milliseconds.
    pub taken_at: i64,
    pub size_bytes: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
}

fn modified_ms(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

/// Every screenshot in a game folder, newest first.
pub fn list(game_dir: &Path) -> Vec<Screenshot> {
    let Ok(entries) = std::fs::read_dir(game_dir.join(SCREENSHOTS)) else {
        return Vec::new();
    };
    let mut shots: Vec<Screenshot> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let metadata = entry.metadata().ok().filter(|meta| meta.is_file())?;
            if !is_image(&path) {
                return None;
            }
            let (width, height) = image_size(&path).map_or((None, None), |(w, h)| (Some(w), Some(h)));
            Some(Screenshot {
                file: entry.file_name().to_string_lossy().into_owned(),
                taken_at: modified_ms(&metadata),
                size_bytes: metadata.len(),
                width,
                height,
            })
        })
        .collect();
    shots.sort_by_key(|shot| std::cmp::Reverse(shot.taken_at));
    shots
}

/// The screenshots taken between two moments (Unix milliseconds), oldest first.
pub fn taken_between(game_dir: &Path, from: i64, to: i64) -> Vec<String> {
    let mut shots: Vec<Screenshot> = list(game_dir)
        .into_iter()
        .filter(|shot| (from..=to).contains(&shot.taken_at))
        .collect();
    shots.sort_by_key(|shot| shot.taken_at);
    shots.into_iter().map(|shot| shot.file).collect()
}

/// A screenshot's path, if `file` names one in the folder (and nothing sneakier).
pub fn file_path(game_dir: &Path, file: &str) -> Option<PathBuf> {
    let mut parts = Path::new(file).components();
    let plain = matches!(parts.next(), Some(Component::Normal(_))) && parts.next().is_none();
    let path = game_dir.join(SCREENSHOTS).join(file);
    (plain && is_image(&path) && path.is_file()).then_some(path)
}

/// Width and height from a PNG's or JPEG's header, without decoding the picture.
pub fn image_size(path: &Path) -> Option<(u32, u32)> {
    let mut header = [0u8; 24];
    let mut file = std::fs::File::open(path).ok()?;
    file.read_exact(&mut header).ok()?;
    if header.starts_with(b"\x89PNG\r\n\x1a\n") && &header[12..16] == b"IHDR" {
        let width = u32::from_be_bytes(header[16..20].try_into().ok()?);
        let height = u32::from_be_bytes(header[20..24].try_into().ok()?);
        return Some((width, height));
    }
    image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

/// A JPEG of `source` at most `width` pixels wide, made once and kept in `cache_dir` (named after
/// the source's path, size and time, so an edited file gets a fresh one).
pub fn thumbnail(source: &Path, cache_dir: &Path, width: u32) -> std::io::Result<PathBuf> {
    let metadata = std::fs::metadata(source)?;
    let mut hasher = Sha1::new();
    hasher.update(source.to_string_lossy().as_bytes());
    hasher.update(metadata.len().to_le_bytes());
    hasher.update(modified_ms(&metadata).to_le_bytes());
    hasher.update(width.to_le_bytes());
    let key: String = hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
    let out = cache_dir.join(format!("{key}.jpg"));
    if out.is_file() {
        return Ok(out);
    }
    let picture = image::open(source).map_err(std::io::Error::other)?;
    let picture = if picture.width() > width {
        let height = ((picture.height() as u64 * width as u64) / picture.width().max(1) as u64).max(1) as u32;
        picture.thumbnail_exact(width, height)
    } else {
        picture
    };
    std::fs::create_dir_all(cache_dir)?;
    let temp = out.with_extension("jpg.part");
    {
        let mut writer = BufWriter::new(std::fs::File::create(&temp)?);
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, 84)
            .encode_image(&picture.to_rgb8())
            .map_err(std::io::Error::other)?;
    }
    std::fs::rename(&temp, &out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_sizes_and_thumbnails_screenshots() {
        let root = std::env::temp_dir().join(format!("wisp-shots-{}", std::process::id()));
        let shots = root.join("game").join(SCREENSHOTS);
        std::fs::create_dir_all(&shots).unwrap();
        let picture = image::RgbImage::from_fn(800, 450, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 90]));
        picture.save(shots.join("2026-11-16_12.00.00.png")).unwrap();
        std::fs::write(shots.join("notes.txt"), b"not a picture").unwrap();

        let listed = list(&root.join("game"));
        assert_eq!(listed.len(), 1);
        assert_eq!((listed[0].width, listed[0].height), (Some(800), Some(450)));
        assert!(file_path(&root.join("game"), "2026-11-16_12.00.00.png").is_some());
        assert!(file_path(&root.join("game"), "../game/x.png").is_none());
        let taken = listed[0].taken_at;
        assert_eq!(taken_between(&root.join("game"), taken - 1, taken + 1).len(), 1);
        assert!(taken_between(&root.join("game"), taken + 1, taken + 2).is_empty());

        let cache = root.join("thumbs");
        let thumb = thumbnail(&shots.join("2026-11-16_12.00.00.png"), &cache, 320).unwrap();
        assert_eq!(image::image_dimensions(&thumb).unwrap(), (320, 180));
        // A second call reuses the first.
        assert_eq!(thumbnail(&shots.join("2026-11-16_12.00.00.png"), &cache, 320).unwrap(), thumb);
        std::fs::remove_dir_all(root).unwrap();
    }
}
