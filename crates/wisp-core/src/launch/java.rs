//! Java for Minecraft, from Mojang's own runtime builds (the ones the official launcher uses), so
//! players never have to install or pick a Java themselves.

use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::LaunchError;
use super::download::Download;
use crate::HttpClient;

const RUNTIME_INDEX: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";
/// Written once a runtime is fully installed; later launches skip the network entirely.
const INSTALLED_MARKER: &str = ".wisp-installed";

/// Mojang's name for this computer in the runtime index. Versions without Apple Silicon natives
/// need the Intel build on an Apple Silicon Mac (it runs under Rosetta).
pub fn platform(intel_on_apple_silicon: bool) -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") if !intel_on_apple_silicon => "mac-os-arm64",
        ("macos", _) => "mac-os",
        ("windows", "aarch64") => "windows-arm64",
        ("windows", "x86") => "windows-x86",
        ("windows", _) => "windows-x64",
        ("linux", "x86") => "linux-i386",
        ("linux", "x86_64") => "linux",
        _ => return None,
    })
}

/// A runtime to install: where it goes, the files to fetch, and the links to make afterwards.
pub struct RuntimePlan {
    pub home: PathBuf,
    pub java: PathBuf,
    pub version: String,
    pub downloads: Vec<Download>,
    links: Vec<(PathBuf, String)>,
}

#[derive(Deserialize)]
struct RuntimeEntry {
    manifest: ManifestRef,
    version: RuntimeVersion,
}

#[derive(Deserialize)]
struct ManifestRef {
    url: String,
}

#[derive(Deserialize)]
struct RuntimeVersion {
    name: String,
}

#[derive(Deserialize)]
struct RuntimeManifest {
    files: BTreeMap<String, RuntimeFile>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum RuntimeFile {
    File {
        #[serde(default)]
        executable: bool,
        downloads: RuntimeDownloads,
    },
    Directory,
    Link {
        target: String,
    },
}

#[derive(Deserialize)]
struct RuntimeDownloads {
    raw: RawDownload,
}

#[derive(Deserialize)]
struct RawDownload {
    sha1: String,
    size: u64,
    url: String,
}

fn java_binary(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("jre.bundle/Contents/Home/bin/java")
    } else if cfg!(windows) {
        home.join("bin/javaw.exe")
    } else {
        home.join("bin/java")
    }
}

/// The installed `java` for this component, if a previous launch already set it up.
pub fn installed(runtimes_dir: &Path, component: &str, platform: &str) -> Option<PathBuf> {
    let home = runtimes_dir.join(component).join(platform);
    let java = java_binary(&home);
    (home.join(INSTALLED_MARKER).is_file() && java.is_file()).then_some(java)
}

/// Works out what installing the runtime takes. Only the small manifests are fetched here.
pub async fn plan(
    client: &HttpClient,
    runtimes_dir: &Path,
    component: &str,
    platform: &str,
) -> Result<RuntimePlan, LaunchError> {
    let index: HashMap<String, HashMap<String, Vec<RuntimeEntry>>> =
        fetch_json(client, RUNTIME_INDEX).await?;
    let entry = index
        .get(platform)
        .and_then(|components| components.get(component))
        .and_then(|entries| entries.first())
        .ok_or_else(|| {
            LaunchError::NoJava(format!("{component} isn't available for {platform}"))
        })?;
    let manifest: RuntimeManifest = fetch_json(client, &entry.manifest.url).await?;

    let home = runtimes_dir.join(component).join(platform);
    let mut downloads = Vec::new();
    let mut links = Vec::new();
    for (relative, file) in manifest.files {
        // Never let a manifest path climb out of the runtime folder.
        let safe = Path::new(&relative)
            .components()
            .all(|part| matches!(part, Component::Normal(_)));
        if !safe {
            continue;
        }
        let path = home.join(&relative);
        match file {
            RuntimeFile::File {
                executable,
                downloads: files,
            } => downloads.push(Download {
                url: files.raw.url,
                path,
                sha1: Some(files.raw.sha1),
                size: Some(files.raw.size),
                executable,
            }),
            RuntimeFile::Directory => {}
            RuntimeFile::Link { target } => links.push((path, target)),
        }
    }
    Ok(RuntimePlan {
        java: java_binary(&home),
        home,
        version: entry.version.name.clone(),
        downloads,
        links,
    })
}

impl RuntimePlan {
    /// Once its files are downloaded: recreates the runtime's symlinks and marks it installed.
    pub fn finish(&self) -> std::io::Result<()> {
        for (path, target) in &self.links {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let _ = std::fs::remove_file(path);
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, path)?;
            #[cfg(windows)]
            {
                // Windows needs special rights for symlinks; a copy does the same job here.
                let source = path.parent().unwrap_or(Path::new("")).join(target);
                if source.is_file() {
                    std::fs::copy(source, path)?;
                }
            }
        }
        std::fs::write(self.home.join(INSTALLED_MARKER), &self.version)
    }
}

/// The Java major version of one of Mojang's runtime components.
/// Mojang's Java runtime for a major version, for when a mod needs a newer one than Minecraft does.
pub fn component_for(major: u32) -> Option<&'static str> {
    Some(match major {
        8 => "jre-legacy",
        16 => "java-runtime-alpha",
        17 => "java-runtime-gamma",
        21 => "java-runtime-delta",
        25 => "java-runtime-epsilon",
        _ => return None,
    })
}

pub fn major_version(component: &str) -> Option<u32> {
    Some(match component {
        "jre-legacy" => 8,
        "java-runtime-alpha" => 16,
        "java-runtime-beta" | "java-runtime-gamma" | "java-runtime-gamma-snapshot" => 17,
        "java-runtime-delta" => 21,
        "java-runtime-epsilon" => 25,
        _ => return None,
    })
}

/// An installed runtime, for the Java settings.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledRuntime {
    pub component: String,
    pub platform: String,
    pub major: Option<u32>,
    /// The exact version, like "21.0.7".
    pub version: String,
    pub size_bytes: u64,
}

/// Every runtime Wisp has installed.
pub fn installed_runtimes(runtimes_dir: &Path) -> Vec<InstalledRuntime> {
    let mut found = Vec::new();
    let Ok(components) = std::fs::read_dir(runtimes_dir) else {
        return found;
    };
    for component in components.flatten() {
        let Ok(platforms) = std::fs::read_dir(component.path()) else {
            continue;
        };
        for platform in platforms.flatten() {
            let home = platform.path();
            let Ok(version) = std::fs::read_to_string(home.join(INSTALLED_MARKER)) else {
                continue;
            };
            let component = component.file_name().to_string_lossy().into_owned();
            found.push(InstalledRuntime {
                major: major_version(&component),
                component,
                platform: platform.file_name().to_string_lossy().into_owned(),
                version: version.trim().to_string(),
                size_bytes: crate::storage::dir_size(&home),
            });
        }
    }
    found.sort_by(|a, b| b.major.cmp(&a.major).then(a.platform.cmp(&b.platform)));
    found
}

/// Deletes an installed runtime; the next launch that needs it downloads it again.
pub fn remove_runtime(runtimes_dir: &Path, component: &str, platform: &str) -> std::io::Result<()> {
    let plain = |name: &str| {
        !name.is_empty()
            && Path::new(name)
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
    };
    if !plain(component) || !plain(platform) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a runtime",
        ));
    }
    let home = runtimes_dir.join(component).join(platform);
    if !home.join(INSTALLED_MARKER).is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "that Java isn't installed",
        ));
    }
    std::fs::remove_dir_all(&home)?;
    // Drop the component folder too once it's empty.
    let _ = std::fs::remove_dir(runtimes_dir.join(component));
    Ok(())
}

/// What `java -version` says about a Java executable.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaProbe {
    /// Like "21.0.7" (or "1.8.0_402" for Java 8).
    pub version: String,
    pub major: u32,
    /// The first line of its output, which names the vendor build.
    pub description: String,
}

/// Runs `java -version` to check a Java someone picked.
pub async fn probe(java: &Path) -> Result<JavaProbe, String> {
    if !java.is_file() {
        return Err("There's no file at that path.".into());
    }
    let mut command = tokio::process::Command::new(java);
    command.arg("-version").stdin(std::process::Stdio::null());
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        command.output(),
    )
    .await
    .map_err(|_| "It didn't answer within 10 seconds.".to_string())?
    .map_err(|error| format!("It couldn't be run: {error}"))?;
    // `java -version` writes to stderr.
    let text = String::from_utf8_lossy(&output.stderr).into_owned()
        + &String::from_utf8_lossy(&output.stdout);
    parse_version_output(&text).ok_or_else(|| "That doesn't look like Java.".into())
}

fn parse_version_output(text: &str) -> Option<JavaProbe> {
    let first = text.lines().find(|line| line.contains("version \""))?;
    let version = first.split('"').nth(1)?.to_string();
    let mut parts = version
        .split(['.', '_', '-', '+'])
        .map_while(|part| part.parse::<u32>().ok());
    let major = match parts.next()? {
        1 => parts.next()?,
        major => major,
    };
    Some(JavaProbe {
        version,
        major,
        description: first.trim().to_string(),
    })
}

async fn fetch_json<T: serde::de::DeserializeOwned>(
    client: &HttpClient,
    url: &str,
) -> Result<T, LaunchError> {
    let response = client
        .get(url)
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|error| LaunchError::Network(error.to_string()))?;
    response
        .json()
        .await
        .map_err(|error| LaunchError::Network(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_computer_has_a_runtime_platform() {
        assert!(platform(false).is_some());
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            assert_eq!(platform(false), Some("mac-os-arm64"));
            assert_eq!(platform(true), Some("mac-os"));
        }
    }

    #[test]
    fn reads_java_version_output() {
        let modern = parse_version_output(
            "openjdk version \"21.0.7\" 2025-04-15 LTS\nOpenJDK Runtime Environment Microsoft-11371464 (build 21.0.7+6-LTS)",
        )
        .unwrap();
        assert_eq!((modern.version.as_str(), modern.major), ("21.0.7", 21));
        let legacy =
            parse_version_output("java version \"1.8.0_402\"\nJava(TM) SE Runtime Environment")
                .unwrap();
        assert_eq!(legacy.major, 8);
        assert!(parse_version_output("zsh: command not found").is_none());
        assert_eq!(major_version("java-runtime-epsilon"), Some(25));
    }

    #[test]
    fn manifest_entries_parse() {
        let manifest: RuntimeManifest = serde_json::from_str(
            r#"{"files":{
              "jre.bundle/Contents/Home/bin":{"type":"directory"},
              "jre.bundle/Contents/Home/bin/java":{"type":"file","executable":true,"downloads":{"raw":{"sha1":"ab","size":3,"url":"https://x/java"}}},
              "jre.bundle/Contents/Home/legal/x":{"type":"link","target":"../java.base/LICENSE"}
            }}"#,
        )
        .unwrap();
        assert_eq!(manifest.files.len(), 3);
        assert!(matches!(
            manifest.files["jre.bundle/Contents/Home/bin/java"],
            RuntimeFile::File {
                executable: true,
                ..
            }
        ));
    }
}
