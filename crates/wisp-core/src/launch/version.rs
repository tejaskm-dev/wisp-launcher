//! Mojang's version JSON: which files a version needs and how to start it. Mod loaders like Fabric
//! publish a small profile that `inheritsFrom` a vanilla version; `inherit` merges the two.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::download::Download;

pub const LIBRARIES_URL: &str = "https://libraries.minecraft.net/";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherits_from: Option<String>,
    #[serde(default)]
    pub main_class: Option<String>,
    #[serde(default)]
    pub arguments: Option<Arguments>,
    /// Pre-1.13 versions use one space-separated string instead of `arguments`.
    #[serde(default)]
    pub minecraft_arguments: Option<String>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default)]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(default)]
    pub downloads: Option<VersionDownloads>,
    #[serde(default)]
    pub java_version: Option<JavaVersion>,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    /// RFC 3339, like "2019-12-09T13:13:38+00:00".
    #[serde(default)]
    pub release_time: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Conditional { rules: Vec<Rule>, value: OneOrMany },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub action: Action,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    /// A regular expression the OS version must match, like `^10\.5\.\d$`.
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    /// Maven coordinates: `group:artifact:version[:classifier][@extension]`.
    pub name: String,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    /// A Maven repository to fetch from (Fabric's libraries say where they live this way).
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub rules: Option<Vec<Rule>>,
    /// Old-style native libraries: OS name → classifier, unpacked before launch.
    #[serde(default)]
    pub natives: Option<HashMap<String, String>>,
    #[serde(default)]
    pub extract: Option<Extract>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    pub url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDownloads {
    pub client: Artifact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub component: String,
    pub major_version: u32,
}

/// This computer, as Mojang's rules describe one.
#[derive(Debug, Clone)]
pub struct Environment {
    /// "osx", "windows" or "linux".
    pub os: &'static str,
    /// "x86" means 32-bit x86 in Mojang's rules; 64-bit machines are "x86_64" or "arm64".
    pub arch: &'static str,
    /// The OS version as Java reports it ("15.1" on macOS, "10.0" on Windows 10 and 11).
    pub os_version: &'static str,
}

impl Environment {
    pub fn current() -> Self {
        let os = match std::env::consts::OS {
            "macos" => "osx",
            "windows" => "windows",
            _ => "linux",
        };
        let arch = match std::env::consts::ARCH {
            "x86" => "x86",
            "aarch64" => "arm64",
            _ => "x86_64",
        };
        Self {
            os,
            arch,
            os_version: os_version(),
        }
    }

    fn matches(&self, rule: &Rule) -> bool {
        if let Some(os) = &rule.os {
            if let Some(name) = &os.name {
                let name_matches = name == self.os
                    || (name == "osx-arm64" && self.os == "osx" && self.arch == "arm64");
                if !name_matches {
                    return false;
                }
            }
            if os.arch.as_ref().is_some_and(|arch| arch != self.arch) {
                return false;
            }
            // A pattern beyond what `version_matches` reads is ignored, as Mojang's launcher
            // ignores one that doesn't compile.
            if let Some(pattern) = &os.version
                && version_matches(pattern, self.os_version) == Some(false)
            {
                return false;
            }
        }
        // Wisp turns on none of the optional features (demo mode, custom resolution, quick play).
        rule.features
            .as_ref()
            .is_none_or(|features| features.values().all(|wanted| !wanted))
    }

    /// Mojang's rule semantics: with no rules, allowed; otherwise the last matching rule decides.
    pub fn allows(&self, rules: Option<&[Rule]>) -> bool {
        match rules {
            None | Some([]) => true,
            Some(rules) => rules.iter().fold(false, |allowed, rule| {
                if self.matches(rule) {
                    rule.action == Action::Allow
                } else {
                    allowed
                }
            }),
        }
    }
}

/// The OS version, read once.
fn os_version() -> &'static str {
    static VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    VERSION.get_or_init(|| {
        let version = match std::env::consts::OS {
            "macos" => std::process::Command::new("/usr/bin/sw_vers")
                .arg("-productVersion")
                .output()
                .ok()
                .map(|output| String::from_utf8_lossy(&output.stdout).into_owned()),
            "linux" => std::fs::read_to_string("/proc/sys/kernel/osrelease").ok(),
            // Java reports both Windows 10 and 11 as 10.0.
            "windows" => Some("10.0".into()),
            _ => None,
        };
        version.unwrap_or_default().trim().to_string()
    })
}

/// Mojang's OS-version conditions are small regular expressions (`^10\.5\.\d$`, `^10\.`). This
/// reads the subset they use (anchors, `.`, `\d`, escaped characters and `* + ?`) and returns
/// `None` for anything more.
fn version_matches(pattern: &str, text: &str) -> Option<bool> {
    #[derive(Clone, Copy)]
    enum Atom {
        Any,
        Digit,
        Char(char),
    }
    /// One atom, repeated between `min` and `max` times.
    type Token = (Atom, usize, usize);

    fn match_here(tokens: &[Token], text: &[char], to_end: bool) -> bool {
        let Some((&(atom, min, max), rest)) = tokens.split_first() else {
            return !to_end || text.is_empty();
        };
        let fits = |c: &char| match atom {
            Atom::Any => true,
            Atom::Digit => c.is_ascii_digit(),
            Atom::Char(expected) => *c == expected,
        };
        let available = text.iter().take_while(|c| fits(c)).count().min(max);
        available >= min
            && (min..=available)
                .rev()
                .any(|n| match_here(rest, &text[n..], to_end))
    }

    let mut chars = pattern.chars().peekable();
    let anchored = chars.next_if_eq(&'^').is_some();
    let mut tokens: Vec<Token> = Vec::new();
    let mut to_end = false;
    while let Some(c) = chars.next() {
        let atom = match c {
            '$' if chars.peek().is_none() => {
                to_end = true;
                break;
            }
            '.' => Atom::Any,
            '\\' => match chars.next()? {
                'd' => Atom::Digit,
                class if class.is_ascii_alphanumeric() => return None,
                escaped => Atom::Char(escaped),
            },
            '[' | ']' | '(' | ')' | '{' | '}' | '|' | '*' | '+' | '?' | '^' | '$' => return None,
            literal => Atom::Char(literal),
        };
        let (min, max) = match chars.next_if(|c| matches!(c, '*' | '+' | '?')) {
            Some('*') => (0, usize::MAX),
            Some('+') => (1, usize::MAX),
            Some('?') => (0, 1),
            _ => (1, 1),
        };
        tokens.push((atom, min, max));
    }
    let text: Vec<char> = text.chars().collect();
    Some(if anchored {
        match_here(&tokens, &text, to_end)
    } else {
        (0..=text.len()).any(|start| match_here(&tokens, &text[start..], to_end))
    })
}

impl VersionJson {
    /// Lays a child profile (say Fabric's) over the vanilla version it inherits from.
    pub fn inherit(self, parent: VersionJson) -> VersionJson {
        let arguments = match (parent.arguments, self.arguments) {
            (Some(mut base), Some(extra)) => {
                base.game.extend(extra.game);
                base.jvm.extend(extra.jvm);
                Some(base)
            }
            (base, extra) => base.or(extra),
        };
        let mut libraries = self.libraries;
        libraries.extend(parent.libraries);
        VersionJson {
            id: self.id,
            inherits_from: None,
            main_class: self.main_class.or(parent.main_class),
            arguments,
            minecraft_arguments: self.minecraft_arguments.or(parent.minecraft_arguments),
            libraries,
            asset_index: self.asset_index.or(parent.asset_index),
            assets: self.assets.or(parent.assets),
            downloads: self.downloads.or(parent.downloads),
            java_version: self.java_version.or(parent.java_version),
            kind: self.kind.or(parent.kind),
            release_time: self.release_time.or(parent.release_time),
        }
    }

    /// Whether the game can open a singleplayer world straight from the command line (Quick
    /// Play, Minecraft 1.20 and later): its arguments mention `${quickPlaySingleplayer}`.
    pub fn supports_quick_play(&self) -> bool {
        let mentions = |value: &str| value.contains("${quickPlaySingleplayer}");
        self.arguments.as_ref().is_some_and(|arguments| {
            arguments.game.iter().any(|argument| match argument {
                Argument::Plain(value) => mentions(value),
                Argument::Conditional { value: OneOrMany::One(value), .. } => mentions(value),
                Argument::Conditional { value: OneOrMany::Many(values), .. } => values.iter().any(|value| mentions(value)),
            })
        })
    }

    /// Game and JVM arguments with `${placeholders}` still in them. Older versions only list game
    /// arguments, so the JVM ones every version needs are supplied here.
    pub fn raw_arguments(&self, env: &Environment) -> (Vec<String>, Vec<String>) {
        let flatten = |arguments: &[Argument]| {
            arguments
                .iter()
                .flat_map(|argument| match argument {
                    Argument::Plain(value) => vec![value.clone()],
                    Argument::Conditional { rules, value } if env.allows(Some(rules)) => {
                        match value {
                            OneOrMany::One(value) => vec![value.clone()],
                            OneOrMany::Many(values) => values.clone(),
                        }
                    }
                    Argument::Conditional { .. } => Vec::new(),
                })
                .collect::<Vec<_>>()
        };
        match (&self.arguments, &self.minecraft_arguments) {
            (Some(arguments), _) => (flatten(&arguments.game), flatten(&arguments.jvm)),
            (None, legacy) => {
                let game = legacy
                    .as_deref()
                    .unwrap_or_default()
                    .split_whitespace()
                    .map(String::from)
                    .collect();
                // No -XstartOnFirstThread here: these versions use LWJGL 2, whose macOS window
                // needs AWT's own thread, and hang with it.
                let jvm = vec![
                    "-Djava.library.path=${natives_directory}".into(),
                    "-cp".into(),
                    "${classpath}".into(),
                ];
                (game, jvm)
            }
        }
    }
}

/// Everything a version needs from the libraries folder.
#[derive(Debug, Default)]
pub struct ResolvedLibraries {
    pub classpath: Vec<PathBuf>,
    pub downloads: Vec<Download>,
    /// Old-style native jars to unpack into the natives folder, with paths to leave out.
    pub natives: Vec<(PathBuf, Vec<String>)>,
    /// Whether any library ships Apple Silicon natives. Versions before 1.19 don't, so on those
    /// an Apple Silicon Mac has to run an Intel Java under Rosetta.
    pub has_arm64_natives: bool,
}

impl ResolvedLibraries {
    pub fn resolve(libraries: &[Library], libraries_dir: &Path, env: &Environment) -> Self {
        let mut resolved = Self::default();
        let mut seen = HashSet::new();
        for library in libraries {
            if !env.allows(library.rules.as_deref()) {
                continue;
            }
            if library.name.contains("arm64") || library.name.contains("aarch_64") {
                resolved.has_arm64_natives = true;
            }
            // A library listed twice (a loader overriding vanilla, say): the first one wins.
            if !seen.insert(library_key(&library.name)) {
                continue;
            }

            if let Some(classifier) = library.native_classifier(env) {
                let artifact = library
                    .downloads
                    .as_ref()
                    .and_then(|downloads| downloads.classifiers.as_ref())
                    .and_then(|classifiers| classifiers.get(&classifier));
                if let Some(artifact) = artifact {
                    let relative = artifact
                        .path
                        .clone()
                        .or_else(|| maven_path(&library.name, Some(&classifier)));
                    if let Some(relative) = relative {
                        let path = libraries_dir.join(relative);
                        resolved.downloads.push(Download {
                            url: artifact.url.clone(),
                            path: path.clone(),
                            sha1: artifact.sha1.clone(),
                            size: artifact.size,
                            executable: false,
                        });
                        let exclude = library
                            .extract
                            .as_ref()
                            .map(|extract| extract.exclude.clone())
                            .unwrap_or_default();
                        resolved.natives.push((path, exclude));
                    }
                }
                // Old native-only entries have no main artifact; nothing goes on the classpath.
                let has_artifact = library
                    .downloads
                    .as_ref()
                    .is_some_and(|downloads| downloads.artifact.is_some());
                if !has_artifact {
                    continue;
                }
            }

            let artifact = library
                .downloads
                .as_ref()
                .and_then(|downloads| downloads.artifact.as_ref());
            let Some(relative) = artifact
                .and_then(|artifact| artifact.path.clone())
                .or_else(|| maven_path(&library.name, None))
            else {
                continue;
            };
            let path = libraries_dir.join(&relative);
            let download = match artifact {
                Some(artifact) if !artifact.url.is_empty() => Some(Download {
                    url: artifact.url.clone(),
                    path: path.clone(),
                    sha1: artifact.sha1.clone(),
                    size: artifact.size,
                    executable: false,
                }),
                Some(_) => None,
                None => {
                    let repository = library.url.as_deref().unwrap_or(LIBRARIES_URL);
                    let separator = if repository.ends_with('/') { "" } else { "/" };
                    Some(Download {
                        url: format!("{repository}{separator}{relative}"),
                        path: path.clone(),
                        sha1: library.sha1.clone(),
                        size: library.size,
                        executable: false,
                    })
                }
            };
            resolved.downloads.extend(download);
            resolved.classpath.push(path);
        }
        resolved
    }
}

impl Library {
    /// For old-style natives: the classifier for this OS, with `${arch}` filled in.
    fn native_classifier(&self, env: &Environment) -> Option<String> {
        let classifier = self.natives.as_ref()?.get(env.os)?;
        let bits = if env.arch == "x86" { "32" } else { "64" };
        Some(classifier.replace("${arch}", bits))
    }
}

/// `group:artifact:version[:classifier][@ext]` → `group/path/artifact/version/artifact-version[-classifier].ext`
pub fn maven_path(name: &str, classifier: Option<&str>) -> Option<String> {
    let (coordinates, extension) = name.split_once('@').unwrap_or((name, "jar"));
    let mut parts = coordinates.split(':');
    let group = parts.next()?;
    let artifact = parts.next()?;
    let version = parts.next()?;
    let classifier = classifier.or(parts.next());
    let file = match classifier {
        Some(classifier) => format!("{artifact}-{version}-{classifier}.{extension}"),
        None => format!("{artifact}-{version}.{extension}"),
    };
    Some(format!(
        "{}/{artifact}/{version}/{file}",
        group.replace('.', "/")
    ))
}

/// Identifies a library regardless of version: `group:artifact[:classifier]`.
fn library_key(name: &str) -> String {
    let coordinates = name.split('@').next().unwrap_or(name);
    let parts: Vec<&str> = coordinates.split(':').collect();
    match parts.as_slice() {
        [group, artifact, _version, classifier, ..] => format!("{group}:{artifact}:{classifier}"),
        [group, artifact, ..] => format!("{group}:{artifact}"),
        _ => name.to_string(),
    }
}

/// Fills `${name}` placeholders. Unknown ones are left as they are, as Mojang's launcher does.
pub fn substitute(argument: &str, values: &HashMap<&str, String>) -> String {
    let mut result = String::with_capacity(argument.len());
    let mut rest = argument;
    while let Some(start) = rest.find("${") {
        result.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let key = &after[..end];
                match values.get(key) {
                    Some(value) => result.push_str(value),
                    None => result.push_str(&rest[start..start + 2 + end + 1]),
                }
                rest = &after[end + 1..];
            }
            None => {
                result.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    result.push_str(rest);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mac_arm() -> Environment {
        Environment {
            os: "osx",
            arch: "arm64",
            os_version: "27.0.1",
        }
    }

    fn rule(json: &str) -> Vec<Rule> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn rules_follow_mojangs_last_match_wins_semantics() {
        let env = mac_arm();
        assert!(env.allows(None));
        assert!(env.allows(Some(&rule(r#"[{"action":"allow","os":{"name":"osx"}}]"#))));
        assert!(!env.allows(Some(&rule(
            r#"[{"action":"allow","os":{"name":"windows"}}]"#
        ))));
        assert!(!env.allows(Some(&rule(
            r#"[{"action":"allow"},{"action":"disallow","os":{"name":"osx"}}]"#
        ))));
        assert!(!env.allows(Some(&rule(r#"[{"action":"allow","os":{"arch":"x86"}}]"#))));
        assert!(!env.allows(Some(&rule(
            r#"[{"action":"allow","features":{"is_demo_user":true}}]"#
        ))));
    }

    #[test]
    fn os_version_conditions() {
        assert_eq!(version_matches(r"^10\.5\.\d$", "10.5.8"), Some(true));
        assert_eq!(version_matches(r"^10\.5\.\d$", "10.15.7"), Some(false));
        assert_eq!(version_matches(r"^10\.5\.\d$", "27.0.1"), Some(false));
        assert_eq!(version_matches(r"^10\.", "10.0"), Some(true));
        assert_eq!(version_matches(r"^10\.", "110.0"), Some(false));
        assert_eq!(version_matches(r"10\.", "110.0"), Some(true));
        assert_eq!(version_matches(r"^1\d*\.0$", "10.0"), Some(true));
        assert_eq!(version_matches(r"^(10|11)", "10.0"), None);
        // Minecraft 1.0 picks its LWJGL by macOS version; today's Macs get the regular build.
        let env = mac_arm();
        assert!(env.allows(Some(&rule(
            r#"[{"action":"allow"},{"action":"disallow","os":{"name":"osx","version":"^10\\.5\\.\\d$"}}]"#
        ))));
        assert!(!env.allows(Some(&rule(
            r#"[{"action":"allow","os":{"name":"osx","version":"^10\\.5\\.\\d$"}}]"#
        ))));
    }

    #[test]
    fn legacy_versions_get_no_start_on_first_thread() {
        let legacy = VersionJson {
            id: "1.8.9".into(),
            minecraft_arguments: Some("--username ${auth_player_name}".into()),
            ..Default::default()
        };
        let (game, jvm) = legacy.raw_arguments(&mac_arm());
        assert_eq!(game, ["--username", "${auth_player_name}"]);
        assert!(
            !jvm.iter()
                .any(|argument| argument == "-XstartOnFirstThread")
        );
        assert!(jvm.iter().any(|argument| argument == "${classpath}"));
    }

    #[test]
    fn maven_paths_and_keys() {
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.4.3:natives-macos-arm64", None).unwrap(),
            "org/lwjgl/lwjgl/3.4.3/lwjgl-3.4.3-natives-macos-arm64.jar"
        );
        assert_eq!(
            maven_path("net.fabricmc:intermediary:26.3", None).unwrap(),
            "net/fabricmc/intermediary/26.3/intermediary-26.3.jar"
        );
        assert_eq!(maven_path("a.b:c:1@zip", None).unwrap(), "a/b/c/1/c-1.zip");
        assert_eq!(library_key("org.ow2.asm:asm:9.10.1"), "org.ow2.asm:asm");
        assert_eq!(
            library_key("org.lwjgl:lwjgl:3.4.3:natives-macos"),
            "org.lwjgl:lwjgl:natives-macos"
        );
    }

    #[test]
    fn knows_which_versions_can_open_a_world_directly() {
        let modern: VersionJson = serde_json::from_str(
            r#"{"id":"26.3","arguments":{"game":["--username","${auth_player_name}",
                {"rules":[{"action":"allow","features":{"is_quick_play_singleplayer":true}}],
                 "value":["--quickPlaySingleplayer","${quickPlaySingleplayer}"]}],"jvm":[]}}"#,
        )
        .unwrap();
        assert!(modern.supports_quick_play());
        let older: VersionJson = serde_json::from_str(r#"{"id":"1.12.2","minecraftArguments":"--username ${auth_player_name}"}"#).unwrap();
        assert!(!older.supports_quick_play());
    }

    #[test]
    fn substitutes_known_placeholders_only() {
        let values = HashMap::from([("natives_directory", "/n".to_string())]);
        assert_eq!(
            substitute("-Djava.library.path=${natives_directory}/java", &values),
            "-Djava.library.path=/n/java"
        );
        assert_eq!(substitute("${unknown} x", &values), "${unknown} x");
        assert_eq!(substitute("broken ${", &values), "broken ${");
    }

    #[test]
    fn resolves_libraries_for_this_platform_once_each() {
        let libraries: Vec<Library> = serde_json::from_str(
            r#"[
              {"name":"org.ow2.asm:asm:9.10.1","url":"https://maven.fabricmc.net/","sha1":"aa","size":1},
              {"name":"org.ow2.asm:asm:9.6","downloads":{"artifact":{"path":"org/ow2/asm/asm/9.6/asm-9.6.jar","url":"https://x/asm.jar","sha1":"bb","size":2}}},
              {"name":"org.lwjgl:lwjgl:3.4.3:natives-macos-arm64","downloads":{"artifact":{"path":"org/lwjgl/lwjgl/3.4.3/lwjgl-3.4.3-natives-macos-arm64.jar","url":"https://x/n.jar","sha1":"cc","size":3}},"rules":[{"action":"allow","os":{"name":"osx"}}]},
              {"name":"org.lwjgl:lwjgl:3.4.3:natives-windows","downloads":{"artifact":{"path":"w.jar","url":"https://x/w.jar","sha1":"dd","size":4}},"rules":[{"action":"allow","os":{"name":"windows"}}]}
            ]"#,
        )
        .unwrap();
        let resolved = ResolvedLibraries::resolve(&libraries, Path::new("/libs"), &mac_arm());
        let urls: Vec<_> = resolved.downloads.iter().map(|d| d.url.as_str()).collect();
        assert_eq!(
            urls,
            [
                "https://maven.fabricmc.net/org/ow2/asm/asm/9.10.1/asm-9.10.1.jar",
                "https://x/n.jar"
            ]
        );
        assert_eq!(resolved.classpath.len(), 2);
        assert!(resolved.has_arm64_natives);
    }

    #[test]
    fn fabric_profiles_inherit_from_vanilla() {
        let vanilla = VersionJson {
            id: "26.3".into(),
            main_class: Some("net.minecraft.client.main.Main".into()),
            arguments: Some(Arguments {
                game: vec![Argument::Plain("--username".into())],
                jvm: vec![Argument::Plain("-cp".into())],
            }),
            kind: Some("release".into()),
            ..Default::default()
        };
        let fabric: VersionJson = serde_json::from_str(
            r#"{"id":"fabric-loader-0.20.1-26.3","inheritsFrom":"26.3","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient","arguments":{"game":[],"jvm":["-DFabricMcEmu= net.minecraft.client.main.Main "]},"libraries":[]}"#,
        )
        .unwrap();
        let merged = fabric.inherit(vanilla);
        assert_eq!(merged.id, "fabric-loader-0.20.1-26.3");
        assert_eq!(
            merged.main_class.as_deref(),
            Some("net.fabricmc.loader.impl.launch.knot.KnotClient")
        );
        let (game, jvm) = merged.raw_arguments(&mac_arm());
        assert_eq!(game, ["--username"]);
        assert_eq!(
            jvm,
            ["-cp", "-DFabricMcEmu= net.minecraft.client.main.Main "]
        );
        assert_eq!(merged.kind.as_deref(), Some("release"));
    }
}
