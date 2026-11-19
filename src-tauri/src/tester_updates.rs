//! Tester builds update themselves too, from a channel of their own. A tester build can play
//! without a Microsoft account, so its installers are never published: each tester run locks its
//! update files with a key only tester builds carry and puts them, with a feed of their own
//! (tester.json), beside the public one. Anyone else sees sealed files. A tester build downloads its
//! update, unlocks it, and hands it to the updater, which checks the usual signature before anything
//! is replaced.
//!
//! The key comes from a tester code, like `WISP-7KQ2-…`: the release workflow derives the code from
//! the update signing secret (scripts/tester-updates.mjs) and builds it into tester builds, which
//! show it so their owner can invite someone. Pasted into any other copy of Wisp that updates
//! itself, the code brings the newest tester build down in that copy's place.

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State, Url};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::{Network, UpdatesOn};

/// The tester code built into tester builds (nothing in any other build).
const CODE: Option<&str> = option_env!("WISP_TESTER_CODE");
/// A code is 15 bytes, written as 24 letters and digits.
const CODE_LEN: usize = 15;
/// Crockford's base 32: no I, L, O or U, so a code copied out by hand still works.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// What a locked update starts with. It's also the authenticated data, so a file locked some other
/// way doesn't open.
const MAGIC: &[u8] = b"WTU1";
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
/// How far a download gets between progress reports.
const REPORT_EVERY: u64 = 256 * 1024;

type Code = [u8; CODE_LEN];

/// Whether this build takes its updates from the tester channel.
pub fn on() -> bool {
    code().is_some()
}

fn code() -> Option<Code> {
    parse_code(CODE?)
}

/// A code as people pass it around: any case, with or without the `WISP-` in front, dashes or
/// spaces anywhere, and I, L or O typed for 1 and 0.
fn parse_code(text: &str) -> Option<Code> {
    let mut symbols: Vec<u8> = text.bytes().filter(u8::is_ascii_alphanumeric).map(|symbol| symbol.to_ascii_uppercase()).collect();
    if symbols.len() == 28 && symbols.starts_with(b"WISP") {
        symbols.drain(..4);
    }
    if symbols.len() != 24 {
        return None;
    }
    let mut bits = 0u128;
    for symbol in symbols {
        let value = match symbol {
            b'I' | b'L' => 1,
            b'O' => 0,
            _ => ALPHABET.iter().position(|&letter| letter == symbol)?,
        };
        bits = (bits << 5) | value as u128;
    }
    bits.to_be_bytes()[1..].try_into().ok()
}

/// `WISP-` and six groups of four.
fn format_code(code: &Code) -> String {
    let mut wide = [0; 16];
    wide[1..].copy_from_slice(code);
    let bits = u128::from_be_bytes(wide);
    let symbols: Vec<char> = (0..24).rev().map(|at| ALPHABET[((bits >> (at * 5)) & 31) as usize] as char).collect();
    let groups: Vec<String> = symbols.chunks(4).map(|group| group.iter().collect()).collect();
    format!("WISP-{}", groups.join("-"))
}

/// HKDF-SHA256 of the code, salted "wisp-launcher", as the workflow script does it.
fn derive<const N: usize>(code: &Code, info: &[u8]) -> Option<[u8; N]> {
    use aws_lc_rs::hkdf::{HKDF_SHA256, KeyType, Salt};
    struct Length(usize);
    impl KeyType for Length {
        fn len(&self) -> usize {
            self.0
        }
    }
    let mut out = [0; N];
    let secret = Salt::new(HKDF_SHA256, b"wisp-launcher").extract(code);
    secret.expand(&[info], Length(N)).ok()?.fill(&mut out).ok()?;
    Some(out)
}

/// The key tester updates are locked with.
fn key_for(code: &Code) -> Result<[u8; 32], String> {
    derive(code, b"tester updates v1").ok_or_else(|| "Couldn't work out the tester key.".to_string())
}

/// What the tester feed publishes so a pasted code can be checked before anything downloads.
fn check_for(code: &Code) -> Option<String> {
    let check: [u8; 8] = derive(code, b"tester code check v1")?;
    Some(check.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Whether the build offered is newer than this one: by version, then by tester build number (the
/// 145 in `0.1.8+145`).
pub fn newer(current: (u64, u64, u64, &str), offered: (u64, u64, u64, &str)) -> bool {
    let number = |build: &str| build.parse::<u64>().unwrap_or(0);
    (offered.0, offered.1, offered.2, number(offered.3)) > (current.0, current.1, current.2, number(current.3))
}

/// Unlocks a locked update, in place: `WTU1`, a 12-byte nonce, then AES-256-GCM ciphertext and tag.
fn open(key: &[u8; 32], mut locked: Vec<u8>) -> Result<Vec<u8>, String> {
    use aws_lc_rs::aead::{AES_256_GCM, Aad, LessSafeKey, Nonce, UnboundKey};
    let broken = || "The update didn't arrive whole. Try again in a moment.".to_string();
    let start = MAGIC.len() + NONCE_LEN;
    if locked.len() < start + TAG_LEN || !locked.starts_with(MAGIC) {
        return Err(broken());
    }
    let key = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, key).map_err(|_| broken())?);
    let nonce = Nonce::try_assume_unique_for_key(&locked[MAGIC.len()..start]).map_err(|_| broken())?;
    let length = key
        .open_in_place(nonce, Aad::from(MAGIC), &mut locked[start..])
        .map_err(|_| broken())?
        .len();
    locked.drain(..start);
    locked.truncate(length);
    Ok(locked)
}

/// How far the download has got, for a progress bar.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    done: u64,
    total: Option<u64>,
}

/// Downloads a locked tester build, unlocks it and installs it. On Windows the installer takes over
/// from here and starts Wisp again; elsewhere the caller restarts Wisp.
async fn install(update: Update, key: [u8; 32], network: &Network, on_progress: &Channel<Progress>) -> Result<(), String> {
    let mut response = network
        .downloads
        .get(update.download_url.as_str())
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("Couldn't download the update: {error}"))?;
    let total = response.content_length();
    let mut locked = Vec::with_capacity(usize::try_from(total.unwrap_or(0)).unwrap_or(0));
    let mut reported = 0;
    while let Some(chunk) = response.chunk().await.map_err(|error| format!("The download stopped: {error}"))? {
        locked.extend_from_slice(&chunk);
        let done = locked.len() as u64;
        if done - reported >= REPORT_EVERY || Some(done) == total {
            reported = done;
            let _ = on_progress.send(Progress { done, total });
        }
    }
    tauri::async_runtime::spawn_blocking(move || {
        let bundle = open(&key, locked)?;
        // The updater checks the bundle against the update's signature before installing it.
        update.install(bundle).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// This build's tester code, to invite someone with. None outside tester builds.
#[tauri::command]
pub fn get_tester_code() -> Option<String> {
    code().map(|code| format_code(&code))
}

/// Downloads the tester build the feed offers, unlocks it and installs it.
#[tauri::command]
pub async fn install_tester_update(app: AppHandle, network: State<'_, Network>, on_progress: Channel<Progress>) -> Result<(), String> {
    let code = code().filter(|_| app.state::<UpdatesOn>().0).ok_or("This build doesn't take tester updates.")?;
    let update = app
        .updater()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())?
        .ok_or("There's no newer tester build right now.")?;
    install(update, key_for(&code)?, &network, &on_progress).await
}

/// Switches this copy of Wisp to tester builds: checks the code against the tester feed, then
/// brings down the newest tester build, unlocked with the code, in this one's place.
#[tauri::command]
pub async fn join_tester_builds(app: AppHandle, network: State<'_, Network>, code: String, on_progress: Channel<Progress>) -> Result<(), String> {
    if on() {
        return Err("This is a tester build already.".into());
    }
    let code = parse_code(&code).ok_or("That isn't a tester code. They look like WISP- and six groups of four letters and digits.")?;
    let feed = app
        .state::<UpdatesOn>()
        .0
        .then(|| tester_feed(&app))
        .flatten()
        .ok_or("This copy of Wisp doesn't update itself, so it can't switch to tester builds.")?;
    let update = app
        .updater_builder()
        .endpoints(vec![feed])
        .and_then(|builder| {
            // Whichever version this copy is, the tester build takes its place.
            builder.version_comparator(|_, _| true).build()
        })
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| format!("Couldn't reach the tester builds: {error}"))?
        .ok_or("There's no tester build to switch to right now.")?;
    let published = update.raw_json.get("check").and_then(|check| check.as_str());
    if published.is_none() || published != check_for(&code).as_deref() {
        return Err("That tester code doesn't work any more. Ask for the current one.".into());
    }
    install(update, key_for(&code)?, &network, &on_progress).await
}

/// tester.json, beside the public feed this build updates from.
fn tester_feed(app: &AppHandle) -> Option<Url> {
    let endpoint = app.config().plugins.0.get("updater")?.get("endpoints")?.get(0)?.as_str()?;
    let mut url = Url::parse(endpoint).ok()?;
    url.path_segments_mut().ok()?.pop().push("tester.json");
    Some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Made by `lock` in scripts/tester-updates.mjs, with the key 32 bytes of 0x42.
    const FROM_THE_SCRIPT: &str = "575455317e8db0698cb65e5aa6e54d68c347d4d5ae99850cb89bd4cf0e35f1b50feec87b4cef696084834d2e7deed28192b4d193bdf0";

    #[test]
    fn opens_what_the_workflow_script_locked() {
        let locked = hex(FROM_THE_SCRIPT);
        assert_eq!(open(&[0x42; 32], locked.clone()).unwrap(), b"a tester build of Wisp");
        // Another key, or a changed byte, doesn't open.
        assert!(open(&[0x43; 32], locked.clone()).is_err());
        let mut changed = locked.clone();
        let last = changed.len() - 1;
        changed[last] ^= 1;
        assert!(open(&[0x42; 32], changed).is_err());
        assert!(open(&[0x42; 32], b"WTU1 too short".to_vec()).is_err());
    }

    #[test]
    fn reads_tester_codes_however_they_are_pasted() {
        let code: Code = core::array::from_fn(|at| (at * 17 + 3) as u8);
        let written = format_code(&code);
        assert_eq!(written.len(), 34);
        assert!(written.starts_with("WISP-"));
        assert_eq!(parse_code(&written), Some(code));
        assert_eq!(parse_code(&written.to_lowercase()), Some(code));
        assert_eq!(parse_code(&written.replace('-', " ")), Some(code));
        assert_eq!(parse_code(&written["WISP-".len()..]), Some(code));
        // I, L and O for 1 and 0, as people type them.
        assert_eq!(parse_code(&written.replace('1', "l").replace('0', "o")), Some(code));
        assert_eq!(parse_code("WISP-0000-0000"), None);
        assert_eq!(parse_code("WISP-UUUU-UUUU-UUUU-UUUU-UUUU-UUUU"), None);
    }

    #[test]
    fn derives_what_the_workflow_script_derives() {
        // From `node scripts/tester-updates.mjs code-test`: the code of the 15 bytes 0 to 14.
        let code: Code = core::array::from_fn(|at| at as u8);
        assert_eq!(format_code(&code), "WISP-000G-40R4-0M30-E209-185G-R38E");
        assert_eq!(hex_of(&key_for(&code).unwrap()), "a8a7d8f20822e4d84a0478a76b18f9d5af73c5f472170b6b88f8374d3d3f7894");
        assert_eq!(check_for(&code).unwrap(), "320fde87d611aebd");
    }

    #[test]
    fn counts_tester_build_numbers() {
        assert!(newer((0, 1, 8, "144"), (0, 1, 8, "145")));
        assert!(!newer((0, 1, 8, "145"), (0, 1, 8, "145")));
        assert!(!newer((0, 1, 8, "145"), (0, 1, 8, "99")));
        // A new version wins whatever the build numbers.
        assert!(newer((0, 1, 8, "145"), (0, 1, 9, "3")));
        assert!(!newer((0, 1, 9, "3"), (0, 1, 8, "145")));
        // A build without a number (one made before tester updates) is older than any.
        assert!(newer((0, 1, 8, ""), (0, 1, 8, "1")));
    }

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len()).step_by(2).map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap()).collect()
    }

    fn hex_of(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
