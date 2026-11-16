//! Old Minecraft versions on today's Macs.
//!
//! - **JNA.** Builds of JNA before 5.17 abort the whole game on recent macOS when a library fails
//!   to load (an assertion about a truncated error message), so 1.17 to 1.21 died within a second
//!   of starting. Wisp swaps in JNA 5.17, the one current versions ship.
//! - **Apple Silicon.** Minecraft has shipped Apple Silicon natives only since 1.19. For 1.15 to
//!   1.18 Wisp swaps in LWJGL 3.3.1 (the build 1.19 ships, the first with Apple Silicon natives)
//!   and the Objective-C bridge 1.19 uses, then runs them on an Apple Silicon Java 17. Older
//!   versions run an Intel Java under Rosetta: before 1.13 they use LWJGL 2, which never had an
//!   Apple Silicon build, and 1.13 to 1.14.4 crash on the newer GLFW (see `FIRST_NATIVE_RELEASE`).

use super::version::{Artifact, Library, LibraryDownloads};

const MOJANG_LIBRARIES: &str = "https://libraries.minecraft.net";

/// LWJGL 3.3.1 as Minecraft 1.19 ships it: module, SHA-1 and size of its jar, then of its Apple
/// Silicon natives.
const LWJGL_331: [(&str, &str, u64, &str, u64); 7] = [
    (
        "lwjgl",
        "ae58664f88e18a9bb2c77b063833ca7aaec484cb",
        724243,
        "71d0d5e469c9c95351eb949064497e3391616ac9",
        42693,
    ),
    (
        "lwjgl-jemalloc",
        "a817bcf213db49f710603677457567c37d53e103",
        36601,
        "e577b87d8ad2ade361aaea2fcf226c660b15dee8",
        103475,
    ),
    (
        "lwjgl-openal",
        "2623a6b8ae1dfcd880738656a9f0243d2e6840bd",
        88237,
        "23d55e7490b57495320f6c9e1936d78fd72c4ef8",
        346125,
    ),
    (
        "lwjgl-opengl",
        "831a5533a21a5f4f81bbc51bb13e9899319b5411",
        921563,
        "eafe34b871d966292e8db0f1f3d6b8b110d4e91d",
        41665,
    ),
    (
        "lwjgl-glfw",
        "cbac1b8d30cb4795149c1ef540f912671a8616d0",
        128801,
        "cac0d3f712a3da7641fa174735a5f315de7ffe0a",
        129077,
    ),
    (
        "lwjgl-stb",
        "b119297cf8ed01f247abe8685857f8e7fcf5980f",
        112380,
        "fcf073ed911752abdca5f0b00a53cfdf17ff8e8b",
        178408,
    ),
    (
        "lwjgl-tinyfd",
        "0ff1914111ef2e3e0110ef2dabc8d8cdaad82347",
        6767,
        "972ecc17bad3571e81162153077b4d47b7b9eaa9",
        41380,
    ),
];

/// The Objective-C bridge with Apple Silicon natives (1.19 onwards); 1.13 to 1.18 ship 1.0.0.
const OBJC_BRIDGE: (&str, &str, u64) = ("1.1", "1227f9e0666314f9de41477e3ec277e542ed7f7b", 1330045);

/// JNA as current versions ship it.
const JNA_VERSION: [u32; 3] = [5, 17, 0];
const JNA: [(&str, &str, u64); 2] = [
    ("jna", "33d12735bef894440780fce64f9758d420c7bae2", 2002589),
    (
        "jna-platform",
        "a4934c44d25a9d8c2ddf4203affd20330cb3426f",
        1378617,
    ),
];

/// 1.13 to 1.14.4 set the window icon while still treating every GLFW error as fatal, and the GLFW
/// in LWJGL 3.3 reports "regular windows do not have icons on macOS" as one. 1.15 set it later, so
/// from 1.15's release on the swap works. (Its snapshots aren't worth the risk of a crash.)
const FIRST_NATIVE_RELEASE: &str = "2019-12-09";

/// Whether a version without Apple Silicon natives of its own can run on swapped-in ones (by its
/// `releaseTime`).
pub fn runs_on_swapped_natives(release_time: Option<&str>) -> bool {
    release_time
        .and_then(|time| time.get(..10))
        .is_some_and(|date| date >= FIRST_NATIVE_RELEASE)
}

/// `group:artifact:version`, ignoring any classifier or extension.
fn coordinates(name: &str) -> Option<(&str, &str, &str)> {
    let mut parts = name.split('@').next()?.split(':');
    Some((parts.next()?, parts.next()?, parts.next()?))
}

fn numbers(version: &str) -> Vec<u32> {
    version
        .split(['.', '-'])
        .map_while(|part| part.parse().ok())
        .collect()
}

fn mojang_library(
    group: &str,
    artifact: &str,
    version: &str,
    classifier: Option<&str>,
    sha1: &str,
    size: u64,
) -> Library {
    let file = match classifier {
        Some(classifier) => format!("{artifact}-{version}-{classifier}.jar"),
        None => format!("{artifact}-{version}.jar"),
    };
    let path = format!("{}/{artifact}/{version}/{file}", group.replace('.', "/"));
    Library {
        name: match classifier {
            Some(classifier) => format!("{group}:{artifact}:{version}:{classifier}"),
            None => format!("{group}:{artifact}:{version}"),
        },
        downloads: Some(LibraryDownloads {
            artifact: Some(Artifact {
                url: format!("{MOJANG_LIBRARIES}/{path}"),
                path: Some(path),
                sha1: Some(sha1.to_string()),
                size: Some(size),
            }),
            classifiers: None,
        }),
        url: None,
        sha1: None,
        size: None,
        rules: None,
        natives: None,
        extract: None,
    }
}

/// Swaps JNA 5 builds older than 5.17 for 5.17 (see the module notes). JNA 4, which 1.16 and
/// older ship, is left alone: it never had the faulty check.
pub fn fix_jna(libraries: &mut [Library]) {
    for library in libraries.iter_mut() {
        let Some((group, artifact, version)) = coordinates(&library.name) else {
            continue;
        };
        let old = numbers(version);
        if group != "net.java.dev.jna"
            || old.first() != Some(&5)
            || old.as_slice() >= JNA_VERSION.as_slice()
        {
            continue;
        }
        if let Some(&(_, sha1, size)) = JNA.iter().find(|(name, ..)| *name == artifact) {
            let version = JNA_VERSION.map(|part| part.to_string()).join(".");
            let mut fixed = mojang_library(group, artifact, &version, None, sha1, size);
            fixed.rules = library.rules.take();
            *library = fixed;
        }
    }
}

/// `org.lwjgl:<module>:3.x…`
fn lwjgl3_module(name: &str) -> Option<&str> {
    let (group, module, version) = coordinates(name)?;
    (group == "org.lwjgl" && version.starts_with("3.")).then_some(module)
}

/// The libraries with Apple Silicon builds swapped in, for a version that has none of its own. `None`
/// if it isn't an LWJGL 3 version (it's older than 1.13), or uses a module there's no build for.
pub fn apple_silicon_libraries(libraries: &[Library]) -> Option<Vec<Library>> {
    if !libraries
        .iter()
        .any(|library| lwjgl3_module(&library.name).is_some())
    {
        return None;
    }
    let mut patched = Vec::with_capacity(libraries.len() + LWJGL_331.len());
    let mut swapped: Vec<&str> = Vec::new();
    for library in libraries {
        if let Some(module) = lwjgl3_module(&library.name) {
            // Each module is listed several times (one entry per platform); swap it in once.
            if swapped.contains(&module) {
                continue;
            }
            swapped.push(module);
            let &(_, jar, jar_size, natives, natives_size) =
                LWJGL_331.iter().find(|(name, ..)| *name == module)?;
            patched.push(mojang_library(
                "org.lwjgl",
                module,
                "3.3.1",
                None,
                jar,
                jar_size,
            ));
            patched.push(mojang_library(
                "org.lwjgl",
                module,
                "3.3.1",
                Some("natives-macos-arm64"),
                natives,
                natives_size,
            ));
        } else if coordinates(&library.name).is_some_and(|(group, artifact, _)| {
            group == "ca.weblite" && artifact == "java-objc-bridge"
        }) {
            if !swapped.contains(&"java-objc-bridge") {
                swapped.push("java-objc-bridge");
                let (version, sha1, size) = OBJC_BRIDGE;
                patched.push(mojang_library(
                    "ca.weblite",
                    "java-objc-bridge",
                    version,
                    None,
                    sha1,
                    size,
                ));
            }
        } else {
            patched.push(library.clone());
        }
    }
    Some(patched)
}

/// The Java those versions get on Apple Silicon. Mojang's Java 8, 16 and 17 builds for them are
/// Intel-only, but they all run on Java 17, which has an Apple Silicon build.
pub fn apple_silicon_java(component: &str) -> &str {
    match component {
        "jre-legacy" | "java-runtime-alpha" | "java-runtime-beta" => "java-runtime-gamma",
        other => other,
    }
}

/// Whether this Mac can run Intel programs (Apple's Rosetta 2 is installed).
pub async fn rosetta_installed() -> bool {
    tokio::process::Command::new("/usr/bin/arch")
        .args(["-x86_64", "/usr/bin/true"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .is_ok_and(|status| status.success())
}

/// Installs Rosetta through Apple's own installer. macOS asks for an administrator's password
/// first. `Ok(false)` means the person cancelled that prompt.
pub async fn install_rosetta() -> Result<bool, String> {
    const SCRIPT: &str = r#"do shell script "/usr/sbin/softwareupdate --install-rosetta --agree-to-license" with administrator privileges"#;
    let output = tokio::process::Command::new("/usr/bin/osascript")
        .args(["-e", SCRIPT])
        .output()
        .await
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        return if rosetta_installed().await {
            Ok(true)
        } else {
            Err("Apple's installer finished, but Rosetta still isn't working. Restart your Mac and try again.".into())
        };
    }
    let message = String::from_utf8_lossy(&output.stderr);
    // AppleScript error -128: the password prompt was cancelled.
    if message.contains("-128") {
        return Ok(false);
    }
    Err(message
        .trim()
        .trim_start_matches("0:")
        .trim()
        .chars()
        .take(300)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library(name: &str) -> Library {
        serde_json::from_value(serde_json::json!({ "name": name })).unwrap()
    }

    fn names(libraries: &[Library]) -> Vec<&str> {
        libraries
            .iter()
            .map(|library| library.name.as_str())
            .collect()
    }

    #[test]
    fn swaps_lwjgl3_and_the_objc_bridge_for_apple_silicon_builds() {
        let libraries = vec![
            library("com.mojang:brigadier:1.0.18"),
            library("org.lwjgl:lwjgl:3.2.1"),
            library("org.lwjgl:lwjgl:3.2.2"),
            library("ca.weblite:java-objc-bridge:1.0.0"),
            library("org.lwjgl:lwjgl-glfw:3.2.1"),
            library("org.lwjgl:lwjgl-glfw:3.2.2:natives-macos"),
        ];
        let patched = apple_silicon_libraries(&libraries).unwrap();
        assert_eq!(
            names(&patched),
            [
                "com.mojang:brigadier:1.0.18",
                "org.lwjgl:lwjgl:3.3.1",
                "org.lwjgl:lwjgl:3.3.1:natives-macos-arm64",
                "ca.weblite:java-objc-bridge:1.1",
                "org.lwjgl:lwjgl-glfw:3.3.1",
                "org.lwjgl:lwjgl-glfw:3.3.1:natives-macos-arm64",
            ]
        );
        let natives = patched[2]
            .downloads
            .as_ref()
            .unwrap()
            .artifact
            .as_ref()
            .unwrap();
        assert_eq!(
            natives.url,
            "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-macos-arm64.jar"
        );
        assert_eq!(
            natives.path.as_deref(),
            Some("org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-macos-arm64.jar")
        );
        assert_eq!(
            natives.sha1.as_deref(),
            Some("71d0d5e469c9c95351eb949064497e3391616ac9")
        );
    }

    #[test]
    fn leaves_lwjgl2_versions_alone() {
        assert!(
            apple_silicon_libraries(&[library("org.lwjgl.lwjgl:lwjgl:2.9.4-nightly-20150209")])
                .is_none()
        );
        assert!(apple_silicon_libraries(&[library("org.lwjgl:lwjgl-vulkan:3.2.1")]).is_none());
    }

    #[test]
    fn upgrades_old_jna_5_only() {
        let mut libraries = vec![
            library("net.java.dev.jna:jna:5.10.0"),
            library("net.java.dev.jna:jna-platform:5.12.1"),
            library("net.java.dev.jna:jna:4.4.0"),
            library("net.java.dev.jna:jna:5.17.0"),
            library("net.java.dev.jna:platform:3.4.0"),
        ];
        fix_jna(&mut libraries);
        assert_eq!(
            names(&libraries),
            [
                "net.java.dev.jna:jna:5.17.0",
                "net.java.dev.jna:jna-platform:5.17.0",
                "net.java.dev.jna:jna:4.4.0",
                "net.java.dev.jna:jna:5.17.0",
                "net.java.dev.jna:platform:3.4.0",
            ]
        );
        let jna = libraries[0]
            .downloads
            .as_ref()
            .unwrap()
            .artifact
            .as_ref()
            .unwrap();
        assert_eq!(
            jna.url,
            "https://libraries.minecraft.net/net/java/dev/jna/jna/5.17.0/jna-5.17.0.jar"
        );
    }

    #[test]
    fn only_1_15_and_newer_get_swapped_natives() {
        assert!(runs_on_swapped_natives(Some("2019-12-09T13:13:38+00:00"))); // 1.15
        assert!(runs_on_swapped_natives(Some("2022-02-28T10:42:45+00:00"))); // 1.18.2
        assert!(!runs_on_swapped_natives(Some("2019-07-19T09:25:47+00:00"))); // 1.14.4
        assert!(!runs_on_swapped_natives(None));
    }

    #[test]
    fn old_java_runs_on_17() {
        assert_eq!(apple_silicon_java("jre-legacy"), "java-runtime-gamma");
        assert_eq!(
            apple_silicon_java("java-runtime-beta"),
            "java-runtime-gamma"
        );
        assert_eq!(
            apple_silicon_java("java-runtime-delta"),
            "java-runtime-delta"
        );
    }
}
