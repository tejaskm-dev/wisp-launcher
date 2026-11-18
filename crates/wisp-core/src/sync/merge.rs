//! Three-way merges for the parts of an instance that can be combined (worlds never are): what
//! changed here since the last sync is kept, what changed elsewhere comes in, and where both
//! changed the same thing, one side wins.

use std::collections::BTreeMap;

use crate::servers::{ListedServer, address_key};

/// Merges keyed values. `base` is what both sides had at the last sync. Where both changed the
/// same key differently, `ours` wins if `prefer_ours`, otherwise `theirs`.
pub fn merge_maps<V: Clone + PartialEq>(base: &BTreeMap<String, V>, ours: &BTreeMap<String, V>, theirs: &BTreeMap<String, V>, prefer_ours: bool) -> BTreeMap<String, V> {
    let mut keys: Vec<&String> = base.keys().chain(ours.keys()).chain(theirs.keys()).collect();
    keys.sort();
    keys.dedup();
    let mut merged = BTreeMap::new();
    for key in keys {
        let (b, o, t) = (base.get(key), ours.get(key), theirs.get(key));
        // Unchanged here: theirs. Unchanged there, the same on both, or a clash ours wins: ours.
        let pick = if o == b {
            t
        } else if t == b || o == t || prefer_ours {
            o
        } else {
            t
        };
        if let Some(value) = pick {
            merged.insert(key.clone(), value.clone());
        }
    }
    merged
}

/// Minecraft settings that depend on the computer (its screen, graphics card, speakers and
/// mouse), so each computer keeps its own.
pub const LOCAL_OPTIONS: &[&str] = &[
    "version",
    "fullscreen",
    "fullscreenResolution",
    "overrideWidth",
    "overrideHeight",
    "renderDistance",
    "simulationDistance",
    "graphicsMode",
    "fancyGraphics",
    "guiScale",
    "maxFps",
    "enableVsync",
    "mipmapLevels",
    "biomeBlendRadius",
    "entityDistanceScaling",
    "renderClouds",
    "ao",
    "particles",
    "entityShadows",
    "prioritizeChunkUpdates",
    "gamma",
    "glDebugVerbosity",
    "soundDevice",
    "mouseSensitivity",
    "rawMouseInput",
    "menuBackgroundBlurriness",
    "cloudRange",
    "inactivityFpsLimit",
];

/// Mod settings files that are about this computer's graphics rather than the player.
pub const LOCAL_CONFIGS: &[&str] = &[
    "config/sodium-options.json",
    "config/sodium-extra-options.json",
    "config/iris.properties",
    "config/DistantHorizons.toml",
    "config/embeddium-options.json",
    "config/rubidium-options.json",
    "config/oculus.properties",
];

pub fn is_local_option(key: &str) -> bool {
    LOCAL_OPTIONS.contains(&key)
}

/// options.txt as key/value pairs ("key:value" lines; the value may itself contain colons).
pub fn parse_options(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| line.split_once(':'))
        .filter(|(key, _)| !key.trim().is_empty())
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// The settings that travel between computers.
pub fn portable_options(text: &str) -> BTreeMap<String, String> {
    parse_options(text).into_iter().filter(|(key, _)| !is_local_option(key)).collect()
}

/// options.txt with the travelling settings set to `portable`: changed lines rewritten in place,
/// settings that went away removed, new ones added at the end, and this computer's own settings
/// left as they were.
pub fn apply_options(text: &str, portable: &BTreeMap<String, String>) -> String {
    let mut lines = Vec::new();
    let mut written = std::collections::HashSet::new();
    for line in text.lines() {
        match line.split_once(':') {
            Some((key, _)) if !is_local_option(key) && !key.trim().is_empty() => {
                if let Some(value) = portable.get(key) {
                    lines.push(format!("{key}:{value}"));
                    written.insert(key.to_string());
                }
            }
            _ => lines.push(line.to_string()),
        }
    }
    for (key, value) in portable {
        if !written.contains(key) {
            lines.push(format!("{key}:{value}"));
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Merges server lists by address. When only one side changed the list, it's taken whole (with
/// its order); when both did, servers are combined, in our order with theirs added after.
pub fn merge_servers(base: &[ListedServer], ours: &[ListedServer], theirs: &[ListedServer], prefer_ours: bool) -> Vec<ListedServer> {
    if ours == base {
        return theirs.to_vec();
    }
    if theirs == base || ours == theirs {
        return ours.to_vec();
    }
    let keyed = |list: &[ListedServer]| -> BTreeMap<String, ListedServer> { list.iter().filter_map(|server| Some((address_key(&server.ip)?, server.clone()))).collect() };
    let merged = merge_maps(&keyed(base), &keyed(ours), &keyed(theirs), prefer_ours);
    let mut result = Vec::new();
    for server in ours.iter().chain(theirs) {
        if let Some(key) = address_key(&server.ip)
            && let Some(kept) = merged.get(&key)
            && !result.iter().any(|existing: &ListedServer| address_key(&existing.ip).as_deref() == Some(key.as_str()))
        {
            result.push(kept.clone());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(key, value)| (key.to_string(), value.to_string())).collect()
    }

    #[test]
    fn keeps_both_sides_changes() {
        let base = map(&[("a", "1"), ("b", "1"), ("c", "1")]);
        let ours = map(&[("a", "2"), ("b", "1"), ("c", "1"), ("new-here", "x")]);
        let theirs = map(&[("a", "1"), ("b", "3"), ("new-there", "y")]);
        let merged = merge_maps(&base, &ours, &theirs, true);
        // a changed here, b there, c removed there, one added on each side.
        assert_eq!(merged, map(&[("a", "2"), ("b", "3"), ("new-here", "x"), ("new-there", "y")]));
        // Both changed the same key: the preferred side wins.
        let clash = map(&[("a", "9")]);
        assert_eq!(merge_maps(&base, &map(&[("a", "2")]), &clash, true)["a"], "2");
        assert_eq!(merge_maps(&base, &map(&[("a", "2")]), &clash, false)["a"], "9");
    }

    #[test]
    fn options_travel_but_screen_settings_stay() {
        let text = "version:3955\nfov:0.5\nguiScale:2\nkey_key.jump:key.keyboard.space\nlastServer:play.example.net:25565\n";
        let portable = portable_options(text);
        assert!(!portable.contains_key("guiScale") && !portable.contains_key("version"));
        assert_eq!(portable["lastServer"], "play.example.net:25565");

        let mut changed = portable.clone();
        changed.insert("fov".into(), "0.75".into());
        changed.remove("lastServer");
        changed.insert("key_key.sneak".into(), "key.keyboard.left.shift".into());
        let written = apply_options(text, &changed);
        assert_eq!(written, "version:3955\nfov:0.75\nguiScale:2\nkey_key.jump:key.keyboard.space\nkey_key.sneak:key.keyboard.left.shift\n");
    }

    #[test]
    fn servers_merge_by_address() {
        let server = |name: &str, ip: &str| ListedServer { name: name.into(), ip: ip.into(), icon: None, accept_textures: None };
        let base = vec![server("Hub", "hub.example.net")];
        let ours = vec![server("Hub", "hub.example.net"), server("Mine", "mine.example.net")];
        let theirs = vec![server("Hub!", "hub.example.net"), server("Theirs", "theirs.example.net")];
        let merged = merge_servers(&base, &ours, &theirs, true);
        assert_eq!(merged.iter().map(|server| server.name.as_str()).collect::<Vec<_>>(), vec!["Hub!", "Mine", "Theirs"]);
        // Only one side changed: its list (and order) as it is.
        assert_eq!(merge_servers(&base, &base, &theirs, true), theirs);
    }
}
