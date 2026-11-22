//! Wisp's launcher logic. It has no UI code, so the Tauri app (and later a CLI) can share it.

pub mod accounts;
pub mod codes;
pub mod content;
pub mod curseforge;
pub mod device;
pub mod doctor;
pub mod feeds;
pub mod game_logs;
pub mod game_options;
pub mod home;
pub mod hosting;
pub mod imports;
pub mod instances;
pub mod inventory;
pub mod journal;
pub mod launch;
pub mod loaders;
pub mod modpack;
pub mod modrinth;
pub mod nbt;
pub mod performance;
#[cfg(feature = "playit")]
pub mod playit;
pub mod screenshots;
pub mod servers;
pub mod settings;
pub mod skin_server;
pub mod skins;
pub mod social;
pub mod snapshots;
pub mod storage;
pub mod sync;
pub mod tunnel;
pub mod versions;
pub mod worlds;

mod http;

pub use http::{Error, HttpClient, download_client, http_client};

/// Percent-encodes a value for a URL query string or path segment (RFC 3986 unreserved set kept).
pub(crate) fn url_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    #[test]
    fn url_encode_keeps_safe_characters_and_escapes_the_rest() {
        assert_eq!(super::url_encode("sodium"), "sodium");
        assert_eq!(super::url_encode("a b&c"), "a%20b%26c");
        assert_eq!(super::url_encode("[[\"x:y\"]]"), "%5B%5B%22x%3Ay%22%5D%5D");
        assert_eq!(
            super::url_encode("1.14 Pre-Release 1"),
            "1.14%20Pre-Release%201"
        );
    }
}
