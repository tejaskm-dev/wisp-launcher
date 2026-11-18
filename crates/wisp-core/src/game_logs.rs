//! An instance's logs and crash reports, for reading in Wisp: the game's latest log, older logs
//! (kept gzipped by the game), and crash reports, newest first.

use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// How much of a log is read: its end, where what went wrong usually is.
const MAX_READ: u64 = 2 * 1024 * 1024;
/// Older logs and crash reports beyond these are left off the list.
const KEEP_OLDER: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LogKind {
    /// The log of the last time the game ran.
    Latest,
    /// A log from an earlier time (gzipped by the game).
    Older,
    Crash,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFile {
    /// Relative to the game folder: "logs/latest.log", "crash-reports/crash-....txt".
    pub name: String,
    pub kind: LogKind,
    pub size: u64,
    /// Unix milliseconds.
    pub modified: i64,
}

fn modified_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_millis() as i64)
}

/// The logs and crash reports in a game folder, newest first (the latest log leading).
pub fn list(game_dir: &Path) -> Vec<LogFile> {
    let mut files = Vec::new();
    for (folder, wanted) in [("logs", &[".log", ".log.gz"][..]), ("crash-reports", &[".txt"][..])] {
        let Ok(entries) = std::fs::read_dir(game_dir.join(folder)) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() || name.starts_with('.') || !wanted.iter().any(|suffix| name.ends_with(suffix)) {
                continue;
            }
            let kind = match (folder, name.as_str()) {
                ("crash-reports", _) => LogKind::Crash,
                (_, "latest.log") => LogKind::Latest,
                _ => LogKind::Older,
            };
            files.push(LogFile { name: format!("{folder}/{name}"), kind, size: meta.len(), modified: modified_ms(&meta) });
        }
    }
    files.sort_by(|a, b| (a.kind != LogKind::Latest).cmp(&(b.kind != LogKind::Latest)).then(b.modified.cmp(&a.modified)));
    let mut older = 0;
    files.retain(|file| {
        if file.kind == LogKind::Latest {
            return true;
        }
        older += 1;
        older <= KEEP_OLDER
    });
    files
}

/// A log's text (its last 2 MB), unzipped when the game gzipped it. Only files `list` would show
/// can be read.
pub fn read(game_dir: &Path, name: &str) -> std::io::Result<String> {
    let parts: Vec<&str> = name.split('/').collect();
    let plain = |part: &str| !part.is_empty() && part != "." && part != ".." && !part.contains('\\') && !part.starts_with('.');
    let allowed = match parts.as_slice() {
        ["logs", file] => plain(file) && (file.ends_with(".log") || file.ends_with(".log.gz")),
        ["crash-reports", file] => plain(file) && file.ends_with(".txt"),
        _ => false,
    };
    if !allowed {
        return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "not a log Wisp shows"));
    }
    let path = game_dir.join(name);
    let mut bytes = Vec::new();
    if name.ends_with(".gz") {
        // Unzipped, it can be far bigger: keep reading, holding on to the end only.
        let mut decoder = flate2::read::GzDecoder::new(std::fs::File::open(&path)?);
        let mut chunk = vec![0; 256 * 1024];
        loop {
            let read = decoder.read(&mut chunk)?;
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read]);
            if bytes.len() as u64 > 2 * MAX_READ {
                bytes.drain(..bytes.len() - MAX_READ as usize);
            }
        }
    } else {
        let mut file = std::fs::File::open(&path)?;
        let size = file.metadata()?.len();
        if size > MAX_READ {
            use std::io::Seek;
            file.seek(std::io::SeekFrom::Start(size - MAX_READ))?;
        }
        file.read_to_end(&mut bytes)?;
    }
    if bytes.len() as u64 > MAX_READ {
        bytes.drain(..bytes.len() - MAX_READ as usize);
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    // Cut at a line start when the beginning was dropped.
    Ok(if (bytes.len() as u64) < MAX_READ { text } else { text.split_once('\n').map_or(text.clone(), |(_, rest)| rest.to_string()) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn lists_and_reads_logs_and_crash_reports() {
        let game = std::env::temp_dir().join(format!("wisp-logs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&game);
        std::fs::create_dir_all(game.join("logs")).unwrap();
        std::fs::create_dir_all(game.join("crash-reports")).unwrap();
        std::fs::write(game.join("logs/latest.log"), "[12:00:00] [main/INFO]: Hello\n").unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(b"an older day\n").unwrap();
        std::fs::write(game.join("logs/2026-11-17-1.log.gz"), gz.finish().unwrap()).unwrap();
        std::fs::write(game.join("crash-reports/crash-2026-11-18_12.00.00-client.txt"), "---- Minecraft Crash Report ----\n").unwrap();
        std::fs::write(game.join("logs/notes.md"), "not a log").unwrap();

        let files = list(&game);
        assert_eq!(files.len(), 3);
        assert_eq!((files[0].name.as_str(), files[0].kind), ("logs/latest.log", LogKind::Latest));
        assert!(files.iter().any(|file| file.kind == LogKind::Crash));
        assert_eq!(read(&game, "logs/latest.log").unwrap(), "[12:00:00] [main/INFO]: Hello\n");
        assert_eq!(read(&game, "logs/2026-11-17-1.log.gz").unwrap(), "an older day\n");
        assert!(read(&game, "crash-reports/crash-2026-11-18_12.00.00-client.txt").unwrap().contains("Crash Report"));
        assert!(read(&game, "../instance.json").is_err());
        assert!(read(&game, "logs/notes.md").is_err());
        assert!(read(&game, "config/x.log").is_err());
        std::fs::remove_dir_all(game).unwrap();
    }
}
