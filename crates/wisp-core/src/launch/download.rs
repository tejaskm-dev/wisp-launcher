//! Fetching many files at once: limited concurrency, retries, and a SHA-1 check on everything
//! that has a known hash. Files land under a `.part` name and are renamed once complete, so an
//! interrupted download never leaves a half-written file behind.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::HttpClient;

/// One file to fetch.
#[derive(Debug, Clone)]
pub struct Download {
    pub url: String,
    pub path: PathBuf,
    /// Lowercase hex SHA-1, checked after the download.
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub executable: bool,
}

/// Counters for a batch of downloads, read by the progress ticker while it runs.
#[derive(Debug, Default)]
pub struct Progress {
    pub done_bytes: AtomicU64,
    pub total_bytes: AtomicU64,
    pub done_files: AtomicU64,
    pub total_files: AtomicU64,
}

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("couldn't download {url}: {source}")]
    Http {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("{url} answered with HTTP {status}")]
    Status { url: String, status: u16 },
    #[error("{url} arrived damaged (its checksum didn't match)")]
    Checksum { url: String },
    #[error("couldn't write {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

const ATTEMPTS: u32 = 3;

/// The files that aren't on disk yet, or whose size is wrong. Existing files aren't re-hashed:
/// that would mean reading hundreds of megabytes on every launch.
pub fn missing(items: impl IntoIterator<Item = Download>) -> Vec<Download> {
    items
        .into_iter()
        .filter(|item| match std::fs::metadata(&item.path) {
            Ok(meta) => item.size.is_some_and(|size| size != meta.len()),
            Err(_) => true,
        })
        .collect()
}

/// Downloads every item, at most `concurrency` at a time. Stops at the first file that still
/// fails after retries.
pub async fn download_all(
    client: &HttpClient,
    items: Vec<Download>,
    concurrency: usize,
    progress: &Arc<Progress>,
) -> Result<(), DownloadError> {
    let items = distinct(items);
    progress
        .total_files
        .fetch_add(items.len() as u64, Ordering::Relaxed);
    progress.total_bytes.fetch_add(
        items.iter().filter_map(|item| item.size).sum(),
        Ordering::Relaxed,
    );

    let permits = Arc::new(Semaphore::new(concurrency.max(1)));
    let mut tasks = JoinSet::new();
    for item in items {
        let client = client.clone();
        let permits = permits.clone();
        let progress = progress.clone();
        tasks.spawn(async move {
            let _permit = permits.acquire_owned().await.expect("never closed");
            fetch_with_retries(&client, &item, &progress).await
        });
    }

    while let Some(joined) = tasks.join_next().await {
        let result = joined.unwrap_or_else(|panic| {
            Err(DownloadError::Io {
                path: "download task".into(),
                source: std::io::Error::other(panic.to_string()),
            })
        });
        if let Err(error) = result {
            tasks.abort_all();
            return Err(error);
        }
    }
    Ok(())
}

async fn fetch_with_retries(
    client: &HttpClient,
    item: &Download,
    progress: &Progress,
) -> Result<(), DownloadError> {
    let mut attempt = 1;
    loop {
        let mut counted = 0;
        match fetch_once(client, item, progress, &mut counted).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                // Take this attempt's bytes back out, so the progress bar never runs past 100%.
                progress.done_bytes.fetch_sub(counted, Ordering::Relaxed);
                let retryable = !matches!(error, DownloadError::Io { .. });
                if attempt >= ATTEMPTS || !retryable {
                    return Err(error);
                }
                tokio::time::sleep(Duration::from_millis(400 * u64::from(attempt))).await;
                attempt += 1;
            }
        }
    }
}

async fn fetch_once(
    client: &HttpClient,
    item: &Download,
    progress: &Progress,
    counted: &mut u64,
) -> Result<(), DownloadError> {
    let http = |source| DownloadError::Http {
        url: item.url.clone(),
        source,
    };
    let io = |source| DownloadError::Io {
        path: item.path.display().to_string(),
        source,
    };

    let mut response = client.get(&item.url).send().await.map_err(http)?;
    if !response.status().is_success() {
        return Err(DownloadError::Status {
            url: item.url.clone(),
            status: response.status().as_u16(),
        });
    }
    if let Some(parent) = item.path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(io)?;
    }

    let partial = part_path(&item.path);
    // Made before the file, so the file is closed first when a stopped download drops both.
    let leftover = Leftover(Some(partial.clone()));
    let mut file = tokio::fs::File::create(&partial).await.map_err(io)?;
    let mut hasher = Sha1::new();
    while let Some(chunk) = response.chunk().await.map_err(http)? {
        hasher.update(&chunk);
        file.write_all(&chunk).await.map_err(io)?;
        progress
            .done_bytes
            .fetch_add(chunk.len() as u64, Ordering::Relaxed);
        *counted += chunk.len() as u64;
    }
    file.flush().await.map_err(io)?;
    drop(file);

    if let Some(expected) = &item.sha1
        && !hex(hasher.finalize().as_slice()).eq_ignore_ascii_case(expected)
    {
        let _ = tokio::fs::remove_file(&partial).await;
        return Err(DownloadError::Checksum {
            url: item.url.clone(),
        });
    }
    #[cfg(unix)]
    if item.executable {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o755))
            .await
            .map_err(io)?;
    }
    tokio::fs::rename(&partial, &item.path).await.map_err(io)?;
    leftover.finished();
    progress.done_files.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// Each file once: a list that names the same file twice (a modpack's index can) would have two
/// downloads writing one partial file, and the second rename failing.
fn distinct(items: Vec<Download>) -> Vec<Download> {
    let mut seen = std::collections::HashSet::new();
    items.into_iter().filter(|item| seen.insert(item.path.clone())).collect()
}

/// A partial file that goes away unless its download finished: one stopped half way (a launch
/// cancelled, an install abandoned) or failed leaves nothing behind.
struct Leftover(Option<PathBuf>);

impl Leftover {
    /// The download finished and the file has its real name.
    fn finished(mut self) {
        self.0 = None;
    }
}

impl Drop for Leftover {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn part_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    path.with_file_name(name)
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_keeps_absent_and_wrong_sized_files() {
        let dir = std::env::temp_dir().join(format!("wisp-download-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("right"), b"12345").unwrap();
        std::fs::write(dir.join("wrong"), b"123").unwrap();
        let item = |name: &str, size| Download {
            url: format!("https://example.invalid/{name}"),
            path: dir.join(name),
            sha1: None,
            size,
            executable: false,
        };
        let left = missing([
            item("right", Some(5)),
            item("wrong", Some(5)),
            item("absent", Some(5)),
            item("unknown-size", None),
        ]);
        let names: Vec<_> = left
            .iter()
            .map(|item| item.path.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(names, ["wrong", "absent", "unknown-size"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn part_files_sit_beside_the_target() {
        assert_eq!(
            part_path(Path::new("/a/b/client.jar")),
            Path::new("/a/b/client.jar.part")
        );
    }

    #[test]
    fn a_file_listed_twice_is_fetched_once() {
        let item = |name: &str| Download { url: format!("https://example.invalid/{name}"), path: PathBuf::from(format!("/mods/{name}")), sha1: None, size: None, executable: false };
        let names: Vec<PathBuf> = distinct(vec![item("a.jar"), item("b.jar"), item("a.jar")]).into_iter().map(|item| item.path).collect();
        assert_eq!(names, [PathBuf::from("/mods/a.jar"), PathBuf::from("/mods/b.jar")]);
    }

    #[test]
    fn a_stopped_download_takes_its_partial_file_with_it() {
        let dir = std::env::temp_dir().join(format!("wisp-leftover-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let partial = dir.join("a.jar.part");
        std::fs::write(&partial, b"half").unwrap();
        drop(Leftover(Some(partial.clone())));
        assert!(!partial.exists());
        let whole = dir.join("b.jar");
        std::fs::write(&whole, b"whole").unwrap();
        Leftover(Some(whole.clone())).finished();
        assert!(whole.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
