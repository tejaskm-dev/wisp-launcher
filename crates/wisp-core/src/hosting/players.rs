//! Who may join and who's in charge: the whitelist, operators and bans a server keeps as JSON
//! lists beside server.properties. While the server runs it owns these files (it rewrites them
//! from memory), so changes then go through its console; while it's stopped Wisp edits them,
//! keeping the fields the server wrote.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum List {
    Whitelist,
    Ops,
    Banned,
}

impl List {
    pub fn file(self) -> &'static str {
        match self {
            Self::Whitelist => "whitelist.json",
            Self::Ops => "ops.json",
            Self::Banned => "banned-players.json",
        }
    }

    /// The console command that adds or takes a name off the list while the server runs.
    pub fn command(self, add: bool, name: &str) -> String {
        match (self, add) {
            (Self::Whitelist, true) => format!("whitelist add {name}"),
            (Self::Whitelist, false) => format!("whitelist remove {name}"),
            (Self::Ops, true) => format!("op {name}"),
            (Self::Ops, false) => format!("deop {name}"),
            (Self::Banned, true) => format!("ban {name}"),
            (Self::Banned, false) => format!("pardon {name}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// With dashes, the way the server writes it.
    pub uuid: String,
    pub name: String,
}

/// "9a439d5fcb533739aca3f8e1ccb9148b" → "9a439d5f-cb53-3739-aca3-f8e1ccb9148b". Already-dashed
/// UUIDs come back as they are.
pub fn dashed(uuid: &str) -> String {
    let plain: String = uuid.chars().filter(|c| *c != '-').collect::<String>().to_ascii_lowercase();
    if plain.len() != 32 || !plain.chars().all(|c| c.is_ascii_hexdigit()) {
        return uuid.to_string();
    }
    format!("{}-{}-{}-{}-{}", &plain[..8], &plain[8..12], &plain[12..16], &plain[16..20], &plain[20..])
}

fn read_values(dir: &Path, list: List) -> Vec<Value> {
    std::fs::read(dir.join(list.file()))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Vec<Value>>(&bytes).ok())
        .unwrap_or_default()
}

fn write_values(dir: &Path, list: List, values: &[Value]) -> std::io::Result<()> {
    let temp = dir.join(format!("{}.tmp", list.file()));
    std::fs::write(&temp, serde_json::to_vec_pretty(values).map_err(std::io::Error::other)?)?;
    std::fs::rename(temp, dir.join(list.file()))
}

/// The names on a list.
pub fn read(dir: &Path, list: List) -> Vec<Entry> {
    read_values(dir, list)
        .into_iter()
        .filter_map(|value| {
            Some(Entry { uuid: value["uuid"].as_str().unwrap_or_default().to_string(), name: value["name"].as_str()?.to_string() })
        })
        .collect()
}

/// Puts someone on a list (stopped servers only). `uuid` may be plain or dashed; `now` is Unix
/// milliseconds, for a ban's date.
pub fn add(dir: &Path, list: List, name: &str, uuid: &str, now: i64) -> std::io::Result<()> {
    let mut values = read_values(dir, list);
    values.retain(|value| !value["name"].as_str().is_some_and(|found| found.eq_ignore_ascii_case(name)));
    let uuid = dashed(uuid);
    values.push(match list {
        List::Whitelist => json!({ "uuid": uuid, "name": name }),
        List::Ops => json!({ "uuid": uuid, "name": name, "level": 4, "bypassesPlayerLimit": false }),
        List::Banned => json!({
            "uuid": uuid,
            "name": name,
            "created": java_date(now),
            "source": "Wisp",
            "expires": "forever",
            "reason": "Banned by an operator.",
        }),
    });
    write_values(dir, list, &values)
}

/// Takes someone off a list (stopped servers only). False when they weren't on it.
pub fn remove(dir: &Path, list: List, name: &str) -> std::io::Result<bool> {
    let mut values = read_values(dir, list);
    let before = values.len();
    values.retain(|value| !value["name"].as_str().is_some_and(|found| found.eq_ignore_ascii_case(name)));
    if values.len() == before {
        return Ok(false);
    }
    write_values(dir, list, &values)?;
    Ok(true)
}

/// A player's UUID (plain hex) and name as Mojang has them, looked up by name, for servers that
/// check accounts. None when there's no such player.
pub async fn mojang_profile(client: &crate::HttpClient, name: &str) -> Result<Option<Entry>, String> {
    if !crate::accounts::valid_username(name) {
        return Ok(None);
    }
    let response = client
        .get(format!("https://api.mojang.com/users/profiles/minecraft/{}", crate::url_encode(name)))
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if response.status().as_u16() == 204 || response.status().as_u16() == 404 {
        return Ok(None);
    }
    let profile: serde_json::Value = response.error_for_status().map_err(|error| error.to_string())?.json().await.map_err(|error| error.to_string())?;
    Ok(Some(Entry { uuid: profile["id"].as_str().unwrap_or_default().to_string(), name: profile["name"].as_str().unwrap_or(name).to_string() }))
}

/// A date the way the server writes ban dates: "2026-11-19 14:03:07 +0000".
fn java_date(now_ms: i64) -> String {
    let seconds = now_ms.div_euclid(1000);
    let days = seconds.div_euclid(86_400);
    let time = seconds.rem_euclid(86_400);
    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} +0000", time / 3600, time % 3600 / 60, time % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_keep_what_the_server_wrote() {
        let dir = std::env::temp_dir().join(format!("wisp-players-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ops.json"), br#"[{"uuid":"b50ad385-829d-3141-a216-7e7d7539ba7f","name":"Notch","level":2,"bypassesPlayerLimit":true}]"#).unwrap();
        assert_eq!(read(&dir, List::Ops), vec![Entry { uuid: "b50ad385-829d-3141-a216-7e7d7539ba7f".into(), name: "Notch".into() }]);
        add(&dir, List::Ops, "Alex", "9a439d5fcb533739aca3f8e1ccb9148b", 0).unwrap();
        let values = read_values(&dir, List::Ops);
        assert_eq!(values[0]["level"], 2, "the server's own fields stay");
        assert_eq!(values[1]["uuid"], "9a439d5f-cb53-3739-aca3-f8e1ccb9148b");
        assert_eq!(values[1]["level"], 4);
        // Names aren't case-sensitive, and adding again replaces.
        add(&dir, List::Ops, "alex", "9a439d5fcb533739aca3f8e1ccb9148b", 0).unwrap();
        assert_eq!(read(&dir, List::Ops).len(), 2);
        assert!(remove(&dir, List::Ops, "NOTCH").unwrap());
        assert!(!remove(&dir, List::Ops, "Notch").unwrap());
        add(&dir, List::Banned, "Griefer", "00000000000000000000000000000001", 1_795_000_000_000).unwrap();
        assert_eq!(read_values(&dir, List::Banned)[0]["created"], "2026-11-18 11:06:40 +0000");
        assert!(read(&dir, List::Whitelist).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn commands_and_uuids() {
        assert_eq!(List::Whitelist.command(true, "Alex"), "whitelist add Alex");
        assert_eq!(List::Banned.command(false, "Alex"), "pardon Alex");
        assert_eq!(dashed("B50AD385829D3141A2167E7D7539BA7F"), "b50ad385-829d-3141-a216-7e7d7539ba7f");
        assert_eq!(dashed("not-a-uuid"), "not-a-uuid");
        assert_eq!(java_date(0), "1970-01-01 00:00:00 +0000");
    }
}
