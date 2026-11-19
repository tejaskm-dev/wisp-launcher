//! Minecraft's options.txt: what new instances start with, and the settings that follow the
//! player from instance to instance on this computer.
//!
//! Sharing goes by changes, never by guesses. Each instance keeps a note of what its options.txt
//! held when Wisp last brought it up to date, so whatever differs from the note now is something
//! the player changed (or cloud sync brought from another computer). Changes go into one store
//! for the computer, the newest winning, and an instance takes everything in the store each time
//! it starts. So that sharing is on from the start, not only from each setting's next change, the
//! first launch that shares a span of versions starts the store from the instance played last:
//! every instance then plays with those settings, its own kept aside in a file the first time.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::instances::InstanceStore;
use crate::settings::NewInstanceOptions;

const FILE: &str = "options.txt";
/// The shared settings, in Wisp's data folder.
const STORE: &str = "shared-options.json";
/// Beside an instance's instance.json: its settings as of the last time Wisp brought it up to
/// date.
const NOTE: &str = "shared-options-note.json";
/// Beside options.txt: what an instance had before shared settings first changed it.
pub const BEFORE_SHARING: &str = "options.txt.before-sharing";
/// Settings that belong to one instance: the data version Minecraft stamps the file with, its
/// resource packs, and the server address last typed in.
const OWN: &[&str] = &["version", "resourcePacks", "incompatibleResourcePacks", "lastServer"];
/// A file the game saved lists every setting it knows: well over this many.
const SAVED_BY_GAME: usize = 40;

/// The store is read and written by launches that can overlap.
static LOCK: Mutex<()> = Mutex::new(());

/// The options.txt lines Wisp's settings ask new instances to start with.
pub fn chosen_defaults(options: &NewInstanceOptions) -> Vec<(&'static str, &'static str)> {
    let mut set = Vec::new();
    if options.skip_welcome {
        set.extend([
            // The accessibility screen on first launch (1.19.4 and later).
            ("onboardAccessibility", "false"),
            // The warning before playing online, and the social interactions hint after.
            ("skipMultiplayerWarning", "true"),
            ("joinedFirstServer", "true"),
        ]);
    }
    if options.skip_tutorial {
        set.push(("tutorialStep", "none"));
    }
    if options.auto_jump_off {
        set.push(("autoJump", "false"));
    }
    set
}

/// Writes a new instance's options.txt, unless it already has one (a modpack's, say). Returns
/// whether it wrote anything.
pub fn seed(
    game_dir: &Path,
    copy_from: Option<&Path>,
    set: &[(&str, &str)],
) -> std::io::Result<bool> {
    let target = game_dir.join(FILE);
    if target.exists() {
        return Ok(false);
    }
    let mut lines: Vec<String> = copy_from
        .and_then(|dir| std::fs::read_to_string(dir.join(FILE)).ok())
        .map(|text| text.lines().map(String::from).collect())
        .unwrap_or_default();
    for (key, value) in set {
        let line = format!("{key}:{value}");
        match lines
            .iter_mut()
            .find(|existing| existing.split(':').next() == Some(key))
        {
            Some(existing) => *existing = line,
            None => lines.push(line),
        }
    }
    if lines.is_empty() {
        return Ok(false);
    }
    std::fs::create_dir_all(game_dir)?;
    std::fs::write(target, lines.join("\n") + "\n")?;
    Ok(true)
}

// ── Sharing between instances ──

/// Spans of Minecraft versions whose options.txt can be mixed. Keybinds became names in 1.13 and
/// some settings changed type in 1.19 (smooth lighting, for one), so each span shares only
/// within itself, and versions before 1.13 keep their own settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Era {
    /// 1.13 to 1.18.
    Named,
    /// 1.19 and later, the year-numbered versions too.
    Current,
}

impl Era {
    fn key(self) -> &'static str {
        match self {
            Self::Named => "1.13",
            Self::Current => "1.19",
        }
    }
}

/// The span a Minecraft version's settings belong to; None for versions before 1.13.
pub fn era(game_version: &str) -> Option<Era> {
    let version = game_version.trim();
    if let Some(rest) = version.strip_prefix("1.") {
        return match leading_number(rest)? {
            0..=12 => None,
            13..=18 => Some(Era::Named),
            _ => Some(Era::Current),
        };
    }
    // Snapshots: 17w43a was the first of 1.13, and 22w11a the first of 1.19.
    if let Some((year, rest)) = version.split_once('w')
        && year.len() == 2
        && let (Ok(year), Some(week)) = (year.parse::<u32>(), leading_number(rest))
    {
        return match (year, week) {
            (..=16, _) | (17, ..=42) => None,
            (17, _) | (18..=21, _) | (22, ..=10) => Some(Era::Named),
            _ => Some(Era::Current),
        };
    }
    // Year-numbered versions (26.1 on) come after all of those; alpha, beta and classic before.
    version
        .starts_with(|character: char| character.is_ascii_digit())
        .then_some(Era::Current)
}

fn leading_number(text: &str) -> Option<u32> {
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Setting {
    value: String,
    /// When it was set, in Unix milliseconds.
    at: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Store {
    /// The newest value of each setting the player changed, by span of versions.
    settings: BTreeMap<String, BTreeMap<String, Setting>>,
    /// Instances that keep their own settings.
    keep_own: BTreeSet<String>,
    /// Spans of versions already started from the instance played last.
    started: BTreeSet<String>,
}

#[derive(Debug, Deserialize)]
struct Note {
    era: String,
    settings: BTreeMap<String, String>,
}

fn load(data: &Path) -> Store {
    std::fs::read(data.join(STORE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save(data: &Path, store: &Store) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(store).map_err(std::io::Error::other)?;
    std::fs::create_dir_all(data)?;
    write_atomic(&data.join(STORE), &json)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut partial = path.as_os_str().to_owned();
    partial.push(".part");
    std::fs::write(&partial, bytes)?;
    std::fs::rename(&partial, path)
}

fn read_note(instance_dir: &Path, era: Era) -> Option<BTreeMap<String, String>> {
    let note: Note = serde_json::from_slice(&std::fs::read(instance_dir.join(NOTE)).ok()?).ok()?;
    (note.era == era.key()).then_some(note.settings)
}

fn write_note(instance_dir: &Path, era: Era, settings: &BTreeMap<String, String>) -> std::io::Result<()> {
    let note = serde_json::json!({ "era": era.key(), "settings": settings });
    write_atomic(&instance_dir.join(NOTE), note.to_string().as_bytes())
}

fn forget_note(instance_dir: &Path) {
    let _ = std::fs::remove_file(instance_dir.join(NOTE));
}

/// options.txt's "key:value" lines (a value may itself hold colons).
fn parse(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| line.split_once(':'))
        .filter(|(key, _)| !key.trim().is_empty())
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// An options.txt's settings: None when there's no file, or when it's caught half-written
/// (Minecraft ends every line with a newline, the last one too).
fn read_complete(path: &Path) -> Option<BTreeMap<String, String>> {
    let text = std::fs::read_to_string(path).ok()?;
    text.ends_with('\n').then(|| parse(&text))
}

fn quoted(value: &str) -> bool {
    value.len() >= 2 && value.starts_with('"') && value.ends_with('"')
}

fn unquote(value: &str) -> &str {
    if quoted(value) { &value[1..value.len() - 1] } else { value }
}

/// Some versions quote text values ("fast") and others don't, so a value is the same either way.
fn same(a: &str, b: &str) -> bool {
    unquote(a) == unquote(b)
}

/// A value written the way the line it replaces was: quoted or not.
fn styled(value: &str, like: &str) -> String {
    match (quoted(like), quoted(value)) {
        (true, false) if !value.starts_with('[') => format!("\"{value}\""),
        (false, true) if !like.starts_with('[') => unquote(value).to_string(),
        _ => value.to_string(),
    }
}

/// When a file last changed, in Unix milliseconds (0 when that can't be read).
fn modified_ms(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_millis() as i64)
}

/// Whether the game itself has saved this options.txt (Wisp's start for a new instance lists only
/// a few settings).
fn saved_by_game(settings: &BTreeMap<String, String>) -> bool {
    settings.contains_key("version") && settings.len() >= SAVED_BY_GAME
}

/// Puts what changed in one instance since its note into the store (the newest change to a
/// setting wins) and brings the note up to date. Returns whether the store changed.
fn gather(store: &mut Store, era: Era, instance_dir: &Path, game_dir: &Path) -> bool {
    let path = game_dir.join(FILE);
    let Some(now) = read_complete(&path) else {
        return false;
    };
    let at = modified_ms(&path);
    let before = read_note(instance_dir, era);
    let mut changed = false;
    if let Some(before) = &before {
        let shared = store.settings.entry(era.key().to_string()).or_default();
        for (key, value) in &now {
            // Only settings that were there and now hold something else: one the game adds
            // when it first saves is its default, not the player's choice.
            let Some(old) = before.get(key) else {
                continue;
            };
            if OWN.contains(&key.as_str()) || same(old, value) {
                continue;
            }
            if shared.get(key).is_none_or(|newest| newest.at <= at && !same(&newest.value, value)) {
                shared.insert(key.clone(), Setting { value: value.clone(), at });
                changed = true;
            }
        }
    }
    if before.as_ref() != Some(&now) {
        let _ = write_note(instance_dir, era, &now);
    }
    changed
}

/// Starts a span of versions' shared settings from one instance's: each of its settings goes
/// into the store unless something newer was changed there.
fn start_from(store: &mut Store, era: Era, settings: BTreeMap<String, String>, at: i64) {
    let shared = store.settings.entry(era.key().to_string()).or_default();
    for (key, value) in settings {
        if OWN.contains(&key.as_str()) {
            continue;
        }
        if shared.get(&key).is_none_or(|newest| newest.at <= at && !same(&newest.value, &value)) {
            shared.insert(key, Setting { value, at });
        }
    }
    store.started.insert(era.key().to_string());
}

/// Sets the shared settings in an instance's options.txt, rewriting lines where they are. A file
/// the game has saved lists every setting its version and mods know, so a setting it doesn't
/// list (from a mod this instance hasn't got, say) stays out; a file the game hasn't saved yet
/// (a new instance's) gets them all. Returns the settings the file holds after.
fn apply(game_dir: &Path, shared: &BTreeMap<String, Setting>) -> std::io::Result<BTreeMap<String, String>> {
    let path = game_dir.join(FILE);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let before = parse(&text);
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = Vec::new();
    let mut changed = false;
    for line in text.lines() {
        let replacement = line.split_once(':').and_then(|(key, old)| {
            let setting = shared.get(key).filter(|_| !OWN.contains(&key))?;
            (!same(old, &setting.value)).then(|| format!("{key}:{}", styled(&setting.value, old)))
        });
        changed |= replacement.is_some();
        lines.push(replacement.unwrap_or_else(|| line.to_string()));
    }
    if !saved_by_game(&before) {
        for (key, setting) in shared {
            if !before.contains_key(key) && !OWN.contains(&key.as_str()) {
                lines.push(format!("{key}:{}", setting.value));
                changed = true;
            }
        }
    }
    if !changed {
        return Ok(before);
    }
    // The first time shared settings change an instance's own, those are kept aside.
    let aside = game_dir.join(BEFORE_SHARING);
    if !text.is_empty() && !aside.exists() {
        let _ = write_atomic(&aside, text.as_bytes());
    }
    let mut out = lines.join(newline);
    out.push_str(newline);
    std::fs::create_dir_all(game_dir)?;
    write_atomic(&path, out.as_bytes())?;
    Ok(parse(&out))
}

/// Where a shared instance's settings live: its era and its two folders. None for instances that
/// keep their own settings, or whose version can't share.
fn sharing_dirs(store: &Store, instances: &InstanceStore, id: &str, game_version: &str) -> Option<(Era, std::path::PathBuf, std::path::PathBuf)> {
    if store.keep_own.contains(id) {
        return None;
    }
    Some((era(game_version)?, instances.dir_of(id)?, instances.game_dir(id)?))
}

/// Just before an instance starts: every instance's changes go into the store (cloud sync may
/// have brought some, and a game Wisp didn't see close may have left some), then this instance
/// takes everything in it. While sharing is off in Settings this isn't called, so turning it
/// back on counts what changed meanwhile, newest first, like any other change. A span of versions
/// that hasn't shared before starts from the instance played last (see the top of this file).
pub fn before_launch(data: &Path, instances: &InstanceStore, id: &str) -> std::io::Result<()> {
    let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut store = load(data);
    let mut changed = false;
    let mut starting = None;
    // For each span not started yet: the settings of the instance whose game saved them last.
    let mut latest: BTreeMap<&'static str, (Era, i64, BTreeMap<String, String>)> = BTreeMap::new();
    for instance in instances.list() {
        let Some((era, dir, game)) = sharing_dirs(&store, instances, &instance.id, &instance.game_version) else {
            continue;
        };
        if !store.started.contains(era.key()) {
            let path = game.join(FILE);
            let at = modified_ms(&path);
            if latest.get(era.key()).is_none_or(|(_, newest, _)| at > *newest)
                && let Some(settings) = read_complete(&path).filter(saved_by_game)
            {
                latest.insert(era.key(), (era, at, settings));
            }
        }
        changed |= gather(&mut store, era, &dir, &game);
        if instance.id == id {
            starting = Some((era, dir, game));
        }
    }
    for (era, at, settings) in latest.into_values() {
        start_from(&mut store, era, settings, at);
        changed = true;
    }
    if changed {
        save(data, &store)?;
    }
    let Some((era, dir, game)) = starting else {
        return Ok(());
    };
    let empty = BTreeMap::new();
    let now = apply(&game, store.settings.get(era.key()).unwrap_or(&empty))?;
    if !now.is_empty() {
        write_note(&dir, era, &now)?;
    }
    Ok(())
}

/// After a game closes: what the player changed while it ran goes into the store.
pub fn after_game(data: &Path, instances: &InstanceStore, id: &str) -> std::io::Result<()> {
    let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut store = load(data);
    let Some(instance) = instances.get(id) else {
        return Ok(());
    };
    let Some((era, dir, game)) = sharing_dirs(&store, instances, id, &instance.game_version) else {
        return Ok(());
    };
    if gather(&mut store, era, &dir, &game) {
        save(data, &store)?;
    }
    Ok(())
}

/// Why one instance's settings couldn't be shared with the rest.
#[derive(Debug, thiserror::Error)]
pub enum ShareError {
    #[error("That instance no longer exists.")]
    NotFound,
    #[error("Minecraft {0} keeps its own settings: versions before 1.13 store them in a way newer ones can't read.")]
    TooOld(String),
    #[error("It has no settings yet. Play it once, then try again.")]
    NoSettings,
    #[error("Couldn't save the shared settings: {0}")]
    Io(#[from] std::io::Error),
}

/// Makes all of one instance's settings everyone's: every other instance of the same span of
/// versions takes them the next time it starts. For setting things up once, rather than waiting
/// for each setting to be changed. Returns how many settings that is.
pub fn share_all(data: &Path, instances: &InstanceStore, id: &str, now_ms: i64) -> Result<usize, ShareError> {
    let instance = instances.get(id).ok_or(ShareError::NotFound)?;
    let era = era(&instance.game_version).ok_or_else(|| ShareError::TooOld(instance.game_version.clone()))?;
    let (dir, game) = instances.dir_of(id).zip(instances.game_dir(id)).ok_or(ShareError::NotFound)?;
    let settings = read_complete(&game.join(FILE)).filter(|settings| !settings.is_empty()).ok_or(ShareError::NoSettings)?;
    let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut store = load(data);
    let shared = store.settings.entry(era.key().to_string()).or_default();
    let mut count = 0;
    for (key, value) in &settings {
        if !OWN.contains(&key.as_str()) {
            shared.insert(key.clone(), Setting { value: value.clone(), at: now_ms });
            count += 1;
        }
    }
    save(data, &store)?;
    // These are in the store already; they mustn't come back as its changes.
    if !store.keep_own.contains(id) {
        write_note(&dir, era, &settings)?;
    }
    Ok(count)
}

/// The instances that keep their own settings.
pub fn keeping_own(data: &Path) -> BTreeSet<String> {
    load(data).keep_own
}

/// Sets whether an instance keeps its own settings. Either way it starts afresh: settings it had
/// while apart don't count as changes, and it takes the shared ones the next time it starts.
pub fn set_keeps_own(data: &Path, instances: &InstanceStore, id: &str, keep: bool) -> std::io::Result<()> {
    let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut store = load(data);
    let changed = if keep { store.keep_own.insert(id.to_string()) } else { store.keep_own.remove(id) };
    if changed {
        if let Some(dir) = instances.dir_of(id) {
            forget_note(&dir);
        }
        save(data, &store)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::{Loader, NewInstance};
    use std::time::{Duration, SystemTime};

    #[test]
    fn copies_then_applies_the_chosen_defaults() {
        let root = std::env::temp_dir().join(format!("wisp-options-{}", std::process::id()));
        let source = root.join("old");
        let target = root.join("new");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(
            source.join(FILE),
            "version:3955\nautoJump:true\nkey_key.jump:key.keyboard.space\n",
        )
        .unwrap();
        let set = chosen_defaults(&NewInstanceOptions {
            auto_jump_off: true,
            ..NewInstanceOptions::default()
        });
        assert!(seed(&target, Some(&source), &set).unwrap());
        assert_eq!(
            std::fs::read_to_string(target.join(FILE)).unwrap(),
            "version:3955\nautoJump:false\nkey_key.jump:key.keyboard.space\n"
        );
        // An instance that already has options keeps them.
        assert!(!seed(&target, None, &[("autoJump", "true")]).unwrap());
        // Nothing to copy and nothing chosen: no file at all.
        assert!(!seed(&root.join("empty"), None, &[]).unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn knows_which_versions_can_share() {
        for (version, expected) in [
            ("1.12.2", None),
            ("1.8.9", None),
            ("b1.7.3", None),
            ("rd-132211", None),
            ("17w42a", None),
            ("1.13", Some(Era::Named)),
            ("1.13-pre1", Some(Era::Named)),
            ("17w43a", Some(Era::Named)),
            ("1.16.5", Some(Era::Named)),
            ("1.18.2", Some(Era::Named)),
            ("22w06a", Some(Era::Named)),
            ("22w11a", Some(Era::Current)),
            ("1.19", Some(Era::Current)),
            ("1.20.1", Some(Era::Current)),
            ("1.21.11", Some(Era::Current)),
            ("24w14potato", Some(Era::Current)),
            ("26.1", Some(Era::Current)),
            ("26.3", Some(Era::Current)),
        ] {
            assert_eq!(era(version), expected, "{version}");
        }
    }

    #[test]
    fn writes_values_the_way_the_file_does() {
        assert!(same("\"fast\"", "fast"));
        assert!(!same("true", "false"));
        assert_eq!(styled("fast", "\"true\""), "\"fast\"");
        assert_eq!(styled("\"fast\"", "true"), "fast");
        assert_eq!(styled("0.5", "0.0"), "0.5");
    }

    /// A file the way Minecraft saves one: the data version, then every setting it knows.
    fn saved(extra: &[(&str, &str)]) -> String {
        let mut settings: BTreeMap<String, String> = (0..SAVED_BY_GAME).map(|n| (format!("setting{n:02}"), "0".to_string())).collect();
        settings.insert("fov".into(), "0.0".into());
        settings.insert("key_key.jump".into(), "key.keyboard.space".into());
        settings.insert("renderClouds".into(), "\"true\"".into());
        settings.insert("resourcePacks".into(), "[\"vanilla\"]".into());
        for (key, value) in extra {
            settings.insert(key.to_string(), value.to_string());
        }
        let mut text = String::from("version:4325\n");
        for (key, value) in settings.iter().filter(|(key, _)| key.as_str() != "version") {
            text.push_str(&format!("{key}:{value}\n"));
        }
        text
    }

    /// Rewrites one setting the way the game does when the player changes it, at a given time.
    fn play(game_dir: &Path, key: &str, value: &str, at_secs: u64) {
        let path = game_dir.join(FILE);
        let text = std::fs::read_to_string(&path).unwrap();
        let mut lines: Vec<String> = text.lines().map(String::from).collect();
        match lines.iter_mut().find(|line| line.split(':').next() == Some(key)) {
            Some(line) => *line = format!("{key}:{value}"),
            None => lines.push(format!("{key}:{value}")),
        }
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(at_secs)).unwrap();
    }

    fn get(game_dir: &Path, key: &str) -> Option<String> {
        parse(&std::fs::read_to_string(game_dir.join(FILE)).unwrap_or_default()).get(key).cloned()
    }

    /// Dates an instance's options.txt, as if its game last saved it then.
    fn age(game_dir: &Path, at_secs: u64) {
        let file = std::fs::File::options().write(true).open(game_dir.join(FILE)).unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(at_secs)).unwrap();
    }

    #[test]
    fn sharing_starts_from_the_instance_played_last() {
        let root = std::env::temp_dir().join(format!("wisp-shared-start-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let data = root.join("data");
        let instances = InstanceStore::new(data.join("instances"));
        let make = |name: &str, contents: &str, at_secs: u64| {
            let made = instances
                .create(NewInstance { name: name.into(), game_version: "1.21.1".into(), loader: Loader::Vanilla, loader_version: None, icon: None }, None, 0)
                .unwrap();
            let game = instances.game_dir(&made.id).unwrap();
            std::fs::create_dir_all(&game).unwrap();
            std::fs::write(game.join(FILE), contents).unwrap();
            age(&game, at_secs);
            (made.id, game)
        };
        // Set up before sharing: one played long ago, one played last, one that keeps its own
        // (newest of all, but not a starting point), and a new one whose few lines Wisp wrote
        // just now (a start, not the player's choices).
        let (old, old_game) = make("Old", &saved(&[("fov", "0.2")]), 100);
        let (_last, last_game) = make("Last", &saved(&[("fov", "0.8"), ("key_key.jump", "key.keyboard.j")]), 900);
        let (apart, apart_game) = make("Apart", &saved(&[("fov", "1.0")]), 1_000);
        set_keeps_own(&data, &instances, &apart, true).unwrap();
        let (new, new_game) = make("New", "autoJump:false\n", 2_000);

        // The first start of any of them starts everyone from the one played last.
        before_launch(&data, &instances, &new).unwrap();
        assert_eq!(get(&new_game, "fov").as_deref(), Some("0.8"));
        assert_eq!(get(&new_game, "autoJump").as_deref(), Some("false"));
        before_launch(&data, &instances, &old).unwrap();
        assert_eq!(get(&old_game, "fov").as_deref(), Some("0.8"));
        assert_eq!(get(&old_game, "key_key.jump").as_deref(), Some("key.keyboard.j"));
        assert_eq!(get(&apart_game, "fov").as_deref(), Some("1.0"));
        assert_eq!(get(&last_game, "fov").as_deref(), Some("0.8"));
        // What Old had is kept aside, from the first time only.
        let aside = |game: &Path| parse(&std::fs::read_to_string(game.join(BEFORE_SHARING)).unwrap_or_default()).get("fov").cloned();
        assert_eq!(aside(&old_game).as_deref(), Some("0.2"));
        assert_eq!(aside(&last_game), None);

        // That happens once: from then on, changes are what travel.
        assert!(load(&data).started.contains("1.19"));
        play(&old_game, "fov", "0.4", 3_000);
        after_game(&data, &instances, &old).unwrap();
        before_launch(&data, &instances, &old).unwrap();
        assert_eq!(get(&old_game, "fov").as_deref(), Some("0.4"));
        assert_eq!(aside(&old_game).as_deref(), Some("0.2"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn settings_follow_the_player_between_instances() {
        let root = std::env::temp_dir().join(format!("wisp-shared-options-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let data = root.join("data");
        let instances = InstanceStore::new(data.join("instances"));
        let make = |name: &str, version: &str| {
            let made = instances
                .create(NewInstance { name: name.into(), game_version: version.into(), loader: Loader::Vanilla, loader_version: None, icon: None }, None, 0)
                .unwrap();
            let game = instances.game_dir(&made.id).unwrap();
            std::fs::create_dir_all(&game).unwrap();
            (made.id, game)
        };
        let (a, a_game) = make("Main", "26.3");
        let (b, b_game) = make("Pack", "1.21.1");
        let (old, old_game) = make("Old pack", "1.16.5");
        let (ancient, ancient_game) = make("Ancient", "1.12.2");
        std::fs::write(a_game.join(FILE), saved(&[])).unwrap();
        std::fs::write(b_game.join(FILE), saved(&[("fov", "0.25"), ("key_key.sneak", "key.keyboard.left.shift")])).unwrap();
        std::fs::write(old_game.join(FILE), saved(&[])).unwrap();
        std::fs::write(ancient_game.join(FILE), "version:1343\nkey_key.jump:57\nfov:0.0\n").unwrap();
        // Main was played a while ago, the pack last.
        age(&a_game, 500);
        age(&b_game, 600);
        age(&old_game, 600);

        // Sharing starts from the instance played last, so Main takes the pack's FOV at its
        // first start. A setting Main's game doesn't list (the pack's sneak key) stays out.
        before_launch(&data, &instances, &a).unwrap();
        assert_eq!(get(&a_game, "fov").as_deref(), Some("0.25"));
        assert_eq!(get(&a_game, "key_key.sneak"), None);
        assert_eq!(get(&b_game, "fov").as_deref(), Some("0.25"));

        // Playing Main, the player turns up the FOV and moves jump to J.
        play(&a_game, "fov", "0.5", 1_000);
        play(&a_game, "key_key.jump", "key.keyboard.j", 1_000);
        // A setting the game adds by itself isn't the player's choice, and a resource pack
        // belongs to its instance.
        play(&a_game, "newSetting", "1", 1_000);
        play(&a_game, "resourcePacks", "[\"vanilla\",\"file/Faithful.zip\"]", 1_000);
        after_game(&data, &instances, &a).unwrap();

        // The pack takes both the next time it starts, keeps its other settings, and keeps its
        // own resource packs. The old pack is another span of versions; the ancient one keeps
        // its own.
        before_launch(&data, &instances, &b).unwrap();
        assert_eq!(get(&b_game, "fov").as_deref(), Some("0.5"));
        assert_eq!(get(&b_game, "key_key.jump").as_deref(), Some("key.keyboard.j"));
        assert_eq!(get(&b_game, "key_key.sneak").as_deref(), Some("key.keyboard.left.shift"));
        assert_eq!(get(&b_game, "resourcePacks").as_deref(), Some("[\"vanilla\"]"));
        assert_eq!(get(&b_game, "newSetting"), None);
        before_launch(&data, &instances, &old).unwrap();
        before_launch(&data, &instances, &ancient).unwrap();
        assert_eq!(get(&old_game, "fov").as_deref(), Some("0.0"));
        assert_eq!(get(&ancient_game, "key_key.jump").as_deref(), Some("57"));

        // The newest change wins, whichever instance it was made in, and a value is written
        // the way the file writes its others.
        play(&b_game, "fov", "0.75", 3_000);
        play(&a_game, "fov", "0.6", 2_000);
        play(&a_game, "renderClouds", "fast", 2_000);
        before_launch(&data, &instances, &a).unwrap();
        assert_eq!(get(&a_game, "fov").as_deref(), Some("0.75"));
        before_launch(&data, &instances, &b).unwrap();
        assert_eq!(get(&b_game, "renderClouds").as_deref(), Some("\"fast\""));

        // An instance that keeps its own settings neither sends nor takes any.
        set_keeps_own(&data, &instances, &b, true).unwrap();
        play(&b_game, "fov", "1.0", 4_000);
        after_game(&data, &instances, &b).unwrap();
        play(&a_game, "key_key.jump", "key.keyboard.k", 5_000);
        after_game(&data, &instances, &a).unwrap();
        before_launch(&data, &instances, &b).unwrap();
        assert_eq!(get(&b_game, "fov").as_deref(), Some("1.0"));
        assert_eq!(get(&b_game, "key_key.jump").as_deref(), Some("key.keyboard.j"));
        assert_eq!(keeping_own(&data), BTreeSet::from([b.clone()]));
        // Back to sharing: it takes the shared settings, and what it had meanwhile isn't news.
        set_keeps_own(&data, &instances, &b, false).unwrap();
        before_launch(&data, &instances, &b).unwrap();
        assert_eq!(get(&b_game, "key_key.jump").as_deref(), Some("key.keyboard.k"));
        assert_eq!(get(&b_game, "fov").as_deref(), Some("0.75"));

        // With sharing off in Settings nothing is called, so nothing moves. Turned back on,
        // what changed meanwhile counts like any change: the newest wins.
        play(&a_game, "fov", "0.1", 6_000);
        assert_eq!(get(&b_game, "fov").as_deref(), Some("0.75"));
        before_launch(&data, &instances, &b).unwrap();
        assert_eq!(get(&b_game, "fov").as_deref(), Some("0.1"));

        // A new instance starts with everything shared, ready for its first start.
        let (new, new_game) = make("New", "26.2");
        std::fs::write(new_game.join(FILE), "autoJump:false\n").unwrap();
        before_launch(&data, &instances, &new).unwrap();
        assert_eq!(get(&new_game, "fov").as_deref(), Some("0.1"));
        assert_eq!(get(&new_game, "key_key.jump").as_deref(), Some("key.keyboard.k"));
        assert_eq!(get(&new_game, "autoJump").as_deref(), Some("false"));
        assert_eq!(get(&new_game, "resourcePacks"), None);

        // Sharing everything from one instance: the others take all of its settings, over
        // older changes of their own, and a later change wins over it again.
        play(&b_game, "setting07", "5", 7_000);
        let own = parse(&std::fs::read_to_string(a_game.join(FILE)).unwrap()).keys().filter(|key| !OWN.contains(&key.as_str())).count();
        assert_eq!(share_all(&data, &instances, &a, 8_000_000).unwrap(), own);
        before_launch(&data, &instances, &b).unwrap();
        assert_eq!(get(&b_game, "setting07").as_deref(), Some("0"));
        play(&b_game, "setting07", "6", 9_000);
        before_launch(&data, &instances, &a).unwrap();
        assert_eq!(get(&a_game, "setting07").as_deref(), Some("6"));
        assert!(matches!(share_all(&data, &instances, &ancient, 9_000_000), Err(ShareError::TooOld(_))));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn leaves_half_written_files_and_line_endings_alone() {
        let root = std::env::temp_dir().join(format!("wisp-options-apply-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(FILE), "version:4325\nfov:0.0\r\nkey_key.jump:key.keyb").unwrap();
        assert_eq!(read_complete(&root.join(FILE)), None);
        std::fs::write(root.join(FILE), "version:4325\r\nfov:0.0\r\nlastServer:example.org\r\n").unwrap();
        let shared = BTreeMap::from([
            ("fov".to_string(), Setting { value: "0.5".into(), at: 1 }),
            ("lastServer".to_string(), Setting { value: "elsewhere.net".into(), at: 1 }),
        ]);
        apply(&root, &shared).unwrap();
        assert_eq!(std::fs::read_to_string(root.join(FILE)).unwrap(), "version:4325\r\nfov:0.5\r\nlastServer:example.org\r\n");
        std::fs::remove_dir_all(root).unwrap();
    }
}
