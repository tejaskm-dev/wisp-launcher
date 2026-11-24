//! Multiplayer servers: the list each instance keeps in `servers.dat`, a server's live status
//! from the server list ping (the same one the game's server screen uses), the servers a session
//! joined, and a short list of public servers to discover.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::nbt::{self, Tag};

const FILE: &str = "servers.dat";
pub const DEFAULT_PORT: u16 = 25565;
/// The protocol sent with a status request; servers answer with their own whatever it is.
const PROTOCOL: i32 = 769;
/// A status bigger than this is refused (modded servers list their mods, so allow plenty).
const MAX_STATUS: usize = 2 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
const ANSWER_TIMEOUT: Duration = Duration::from_secs(4);
/// How many servers are pinged at once.
const PARALLEL: usize = 12;

/// A server in an instance's list.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedServer {
    pub name: String,
    pub address: String,
    /// Its icon as a PNG data URL, as the game last saw it.
    pub icon: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Type the server's address, like play.example.net.")]
    NoAddress,
    #[error("\"{0}\" doesn't look like a server address.")]
    BadAddress(String),
    #[error("{0} is already in this instance's server list.")]
    AlreadyThere(String),
    #[error("The server list couldn't be read: {0}")]
    Unreadable(String),
    #[error("Couldn't save the server list: {0}")]
    Io(#[from] std::io::Error),
}

/// A server address split up: `host`, `host:port` or `[IPv6]:port`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAddress {
    pub host: String,
    pub port: Option<u16>,
}

impl ServerAddress {
    pub fn parse(text: &str) -> Result<Self, ServerError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(ServerError::NoAddress);
        }
        let bad = || ServerError::BadAddress(text.to_string());
        let (host, port) = if let Some(rest) = text.strip_prefix('[') {
            let (host, after) = rest.split_once(']').ok_or_else(bad)?;
            let port = match after {
                "" => None,
                _ => Some(after.strip_prefix(':').ok_or_else(bad)?.parse::<u16>().map_err(|_| bad())?),
            };
            (host.to_string(), port)
        } else {
            match text.rsplit_once(':') {
                // One colon: host and port. More than one: an IPv6 address without brackets.
                Some((host, port)) if !host.contains(':') => (host.to_string(), Some(port.parse::<u16>().map_err(|_| bad())?)),
                _ => (text.to_string(), None),
            }
        };
        let fine = !host.is_empty() && host.len() <= 253 && host.chars().all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_' | ':'));
        if !fine || port == Some(0) {
            return Err(bad());
        }
        Ok(Self { host, port })
    }

    /// For comparing addresses: lowercase, with the default port written out.
    pub fn key(&self) -> String {
        format!("{}:{}", self.host.trim_end_matches('.').to_ascii_lowercase(), self.port.unwrap_or(DEFAULT_PORT))
    }
}

/// The same server, however it's written (`Example.net` and `example.net:25565` match).
pub fn address_key(address: &str) -> Option<String> {
    ServerAddress::parse(address).ok().map(|parsed| parsed.key())
}

/// The servers in an instance's list, in the game's order. Hidden ones (the last Direct Connect)
/// are left out.
pub fn list(game_dir: &Path) -> Vec<SavedServer> {
    let Ok(root) = nbt::read_file(&game_dir.join(FILE)) else {
        return Vec::new();
    };
    let Some(Tag::List(entries)) = root.get("servers") else {
        return Vec::new();
    };
    entries
        .iter()
        .filter(|entry| entry.get("hidden").and_then(Tag::as_bool) != Some(true))
        .filter_map(|entry| {
            let address = entry.get("ip").and_then(Tag::as_str)?.trim().to_string();
            if address.is_empty() {
                return None;
            }
            let name = entry.get("name").and_then(Tag::as_str).map(str::trim).filter(|name| !name.is_empty()).unwrap_or(&address).to_string();
            let icon = entry.get("icon").and_then(Tag::as_str).filter(|icon| !icon.is_empty()).map(|icon| format!("data:image/png;base64,{icon}"));
            Some(SavedServer { name, address, icon })
        })
        .collect()
}

/// Adds a server to the end of an instance's list, keeping everything else in the file as it was.
pub fn add(game_dir: &Path, name: &str, address: &str) -> Result<SavedServer, ServerError> {
    let address = address.trim();
    let key = ServerAddress::parse(address)?.key();
    let name = match name.trim() {
        "" => address,
        given => given,
    };
    edit(game_dir, |entries| {
        let taken = entries.iter().any(|entry| {
            entry.get("hidden").and_then(Tag::as_bool) != Some(true) && entry.get("ip").and_then(Tag::as_str).and_then(address_key).as_deref() == Some(&key)
        });
        if taken {
            return Err(ServerError::AlreadyThere(address.to_string()));
        }
        let mut entry = HashMap::new();
        entry.insert("name".to_string(), Tag::String(name.to_string()));
        entry.insert("ip".to_string(), Tag::String(address.to_string()));
        entries.push(Tag::Compound(entry));
        Ok(())
    })?;
    Ok(SavedServer { name: name.to_string(), address: address.to_string(), icon: None })
}

/// Takes a server out of an instance's list. False when it wasn't there.
pub fn remove(game_dir: &Path, address: &str) -> Result<bool, ServerError> {
    let key = ServerAddress::parse(address)?.key();
    let mut removed = false;
    edit(game_dir, |entries| {
        let before = entries.len();
        entries.retain(|entry| entry.get("hidden").and_then(Tag::as_bool) == Some(true) || entry.get("ip").and_then(Tag::as_str).and_then(address_key).as_deref() != Some(&key));
        removed = entries.len() != before;
        Ok(())
    })?;
    Ok(removed)
}

/// A server as cloud sync carries it: what the game keeps about it in servers.dat.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListedServer {
    pub name: String,
    pub ip: String,
    /// The icon as the game stores it (base64 PNG).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// The player's answer to "use this server's resource pack?" (1 yes, 0 no).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accept_textures: Option<i8>,
}

/// An instance's server list for syncing, in the game's order (hidden entries left out).
pub fn listed(game_dir: &Path) -> Vec<ListedServer> {
    let Ok(root) = nbt::read_file(&game_dir.join(FILE)) else {
        return Vec::new();
    };
    let Some(Tag::List(entries)) = root.get("servers") else {
        return Vec::new();
    };
    entries
        .iter()
        .filter(|entry| entry.get("hidden").and_then(Tag::as_bool) != Some(true))
        .filter_map(|entry| {
            let ip = entry.get("ip").and_then(Tag::as_str)?.trim().to_string();
            if ip.is_empty() {
                return None;
            }
            Some(ListedServer {
                name: entry.get("name").and_then(Tag::as_str).unwrap_or_default().to_string(),
                ip,
                icon: entry.get("icon").and_then(Tag::as_str).filter(|icon| !icon.is_empty()).map(String::from),
                accept_textures: match entry.get("acceptTextures") {
                    Some(Tag::Byte(value)) => Some(*value),
                    _ => None,
                },
            })
        })
        .collect()
}

/// Replaces an instance's server list with these (a merged list from sync), keeping hidden
/// entries and anything else the game stored about servers that are still there.
pub fn set_listed(game_dir: &Path, servers: &[ListedServer]) -> Result<(), ServerError> {
    edit(game_dir, |entries| {
        let hidden: Vec<Tag> = entries.iter().filter(|entry| entry.get("hidden").and_then(Tag::as_bool) == Some(true)).cloned().collect();
        let mut kept: HashMap<String, HashMap<String, Tag>> = entries
            .iter()
            .filter_map(|entry| match entry {
                Tag::Compound(children) => {
                    let key = children.get("ip").and_then(Tag::as_str).and_then(address_key)?;
                    Some((key, children.clone()))
                }
                _ => None,
            })
            .collect();
        let mut fresh = Vec::new();
        for server in servers {
            let mut entry = address_key(&server.ip).and_then(|key| kept.remove(&key)).unwrap_or_default();
            entry.insert("name".to_string(), Tag::String(server.name.clone()));
            entry.insert("ip".to_string(), Tag::String(server.ip.clone()));
            match &server.icon {
                Some(icon) => entry.insert("icon".to_string(), Tag::String(icon.clone())),
                None => entry.remove("icon"),
            };
            match server.accept_textures {
                Some(value) => entry.insert("acceptTextures".to_string(), Tag::Byte(value)),
                None => entry.remove("acceptTextures"),
            };
            entry.remove("hidden");
            fresh.push(Tag::Compound(entry));
        }
        fresh.extend(hidden);
        *entries = fresh;
        Ok(())
    })
}

/// Reads servers.dat, changes its list, and writes it back the way the game does: the old file
/// kept as `servers.dat_old`, the new one written beside it and moved into place.
fn edit(game_dir: &Path, change: impl FnOnce(&mut Vec<Tag>) -> Result<(), ServerError>) -> Result<(), ServerError> {
    let path = game_dir.join(FILE);
    let mut root = match std::fs::read(&path) {
        Ok(bytes) => nbt::parse(&bytes).map_err(|error| ServerError::Unreadable(error.to_string()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Tag::Compound(HashMap::new()),
        Err(error) => return Err(error.into()),
    };
    let Tag::Compound(children) = &mut root else {
        return Err(ServerError::Unreadable("it isn't a list of servers".into()));
    };
    let list = children.entry("servers".to_string()).or_insert_with(|| Tag::List(Vec::new()));
    if !matches!(list, Tag::List(_)) {
        *list = Tag::List(Vec::new());
    }
    let Tag::List(entries) = list else {
        unreachable!("just made sure it's a list");
    };
    change(entries)?;
    std::fs::create_dir_all(game_dir)?;
    let fresh = game_dir.join("servers.dat.wisp-new");
    std::fs::write(&fresh, nbt::to_bytes(&root))?;
    if path.exists() {
        let _ = std::fs::copy(&path, game_dir.join("servers.dat_old"));
    }
    std::fs::rename(&fresh, &path)?;
    Ok(())
}

// ── Status ──

/// What a server says about itself on the server list.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    /// The address as it was asked for.
    pub address: String,
    pub online: bool,
    /// The message of the day, as runs of styled text (line breaks stay in the text).
    pub motd: Vec<MotdSpan>,
    /// e.g. "Paper 1.21.4" or "Requires MC 1.8 / 1.21".
    pub version: Option<String>,
    pub protocol: Option<i32>,
    pub players: Option<u32>,
    pub max_players: Option<u32>,
    /// Names the server shows when you hover the player count (sometimes a message instead).
    pub sample: Vec<String>,
    /// The server's icon as a PNG data URL.
    pub icon: Option<String>,
    pub ping_ms: Option<u32>,
    /// Why it couldn't be reached, when it couldn't.
    pub error: Option<String>,
    /// Hosted with Wisp: joining from Wisp brings its mods and your skin along.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub wisp: bool,
    /// The key a Wisp host signs skin data with (base64 DER), when its server list answer says.
    #[serde(skip)]
    pub wisp_skin_key: Option<String>,
}

/// A run of the MOTD in one style.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MotdSpan {
    pub text: String,
    /// "#rrggbb".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub underlined: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub strikethrough: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub obfuscated: bool,
}

/// Asks a server for its status. Never fails: an unreachable server comes back offline, with why.
pub async fn status(address: &str) -> ServerStatus {
    let offline = |error: String| ServerStatus { address: address.to_string(), error: Some(error), ..Default::default() };
    let parsed = match ServerAddress::parse(address) {
        Ok(parsed) => parsed,
        Err(error) => return offline(error.to_string()),
    };
    let (host, port) = resolve(&parsed).await;
    let asked = query(&parsed.host, parsed.port.unwrap_or(DEFAULT_PORT), &host, port);
    match tokio::time::timeout(CONNECT_TIMEOUT + ANSWER_TIMEOUT, asked).await {
        Ok(Ok((answer, ping))) => read_status(address, &answer, ping),
        Ok(Err(error)) => offline(error),
        Err(_) => offline("It didn't answer in time.".into()),
    }
}

/// Statuses for several servers, a few at a time, in the order asked.
pub async fn statuses(addresses: Vec<String>) -> Vec<ServerStatus> {
    let gate = Arc::new(tokio::sync::Semaphore::new(PARALLEL));
    let mut tasks = tokio::task::JoinSet::new();
    for (index, address) in addresses.into_iter().enumerate() {
        let gate = gate.clone();
        tasks.spawn(async move {
            let _turn = gate.acquire().await;
            (index, status(&address).await)
        });
    }
    let mut found = Vec::new();
    while let Some(done) = tasks.join_next().await {
        if let Ok(item) = done {
            found.push(item);
        }
    }
    found.sort_by_key(|(index, _)| *index);
    found.into_iter().map(|(_, status)| status).collect()
}

/// Where to connect: an address without a port may point elsewhere through an SRV record, the
/// way the game looks it up.
pub(crate) async fn resolve(address: &ServerAddress) -> (String, u16) {
    if let Some(port) = address.port {
        return (address.host.clone(), port);
    }
    if address.host.parse::<std::net::IpAddr>().is_ok() || address.host.eq_ignore_ascii_case("localhost") {
        return (address.host.clone(), DEFAULT_PORT);
    }
    srv(&address.host).await.unwrap_or_else(|| (address.host.clone(), DEFAULT_PORT))
}

fn resolver() -> Option<&'static hickory_resolver::TokioResolver> {
    static RESOLVER: OnceLock<Option<hickory_resolver::TokioResolver>> = OnceLock::new();
    // The computer's own DNS settings; without them, no SRV lookups (never someone else's DNS).
    RESOLVER
        .get_or_init(|| match hickory_resolver::TokioResolver::builder_tokio() {
            Ok(builder) => builder.build().ok(),
            Err(_) => system_nameservers(),
        })
        .as_ref()
}

/// macOS lists link-local IPv6 nameservers with a zone (`fe80::1%en0`), which the resolver's own
/// reader refuses outright; this keeps the plain addresses from the same file.
fn system_nameservers() -> Option<hickory_resolver::TokioResolver> {
    use hickory_resolver::config::{NameServerConfig, ResolverConfig};
    let text = std::fs::read_to_string("/etc/resolv.conf").ok()?;
    let servers: Vec<NameServerConfig> = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("nameserver"))
        .filter_map(|address| address.trim().parse::<std::net::IpAddr>().ok())
        .map(NameServerConfig::udp_and_tcp)
        .collect();
    if servers.is_empty() {
        return None;
    }
    let provider = hickory_resolver::net::runtime::TokioRuntimeProvider::default();
    hickory_resolver::TokioResolver::builder_with_config(ResolverConfig::from_name_servers(servers), provider).build().ok()
}

async fn srv(host: &str) -> Option<(String, u16)> {
    use hickory_resolver::proto::rr::RData;
    let name = format!("_minecraft._tcp.{}.", host.trim_end_matches('.'));
    let lookup = tokio::time::timeout(Duration::from_secs(3), resolver()?.srv_lookup(name)).await.ok()?.ok()?;
    lookup
        .answers()
        .iter()
        .filter_map(|record| match &record.data {
            RData::SRV(srv) => Some((srv.priority, srv.target.to_ascii().trim_end_matches('.').to_string(), srv.port)),
            _ => None,
        })
        .filter(|(_, target, _)| !target.is_empty())
        .min_by_key(|(priority, _, _)| *priority)
        .map(|(_, target, port)| (target, port))
}

/// The handshake and status request, then a ping for the round trip. The handshake carries the
/// address as typed (proxies route by it); the connection goes wherever SRV pointed.
async fn query(typed_host: &str, typed_port: u16, host: &str, port: u16) -> Result<(Value, u32), String> {
    let connecting = TcpStream::connect((host, port));
    let mut stream = match tokio::time::timeout(CONNECT_TIMEOUT, connecting).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) => return Err(reach_error(&error)),
        Err(_) => return Err("It didn't answer in time.".into()),
    };
    let _ = stream.set_nodelay(true);
    let mut handshake = Vec::new();
    write_varint(&mut handshake, 0x00);
    write_varint(&mut handshake, PROTOCOL);
    write_string(&mut handshake, typed_host);
    handshake.extend(typed_port.to_be_bytes());
    write_varint(&mut handshake, 1);
    let mut out = Vec::new();
    write_packet(&mut out, &handshake);
    write_packet(&mut out, &[0x00]);
    let asked = Instant::now();
    stream.write_all(&out).await.map_err(|error| reach_error(&error))?;

    let length = read_varint(&mut stream).await.map_err(|_| "It closed the connection without an answer.".to_string())? as usize;
    if length == 0 || length > MAX_STATUS {
        return Err("Its answer didn't make sense.".into());
    }
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await.map_err(|_| "Its answer was cut off.".to_string())?;
    let status_trip = asked.elapsed();
    let mut cursor: &[u8] = &body;
    if read_varint_slice(&mut cursor) != Some(0) {
        return Err("Its answer didn't make sense.".into());
    }
    let text_length = read_varint_slice(&mut cursor).ok_or("Its answer didn't make sense.")? as usize;
    let text = cursor.get(..text_length).ok_or("Its answer was cut off.")?;
    let answer: Value = serde_json::from_slice(text).map_err(|_| "Its answer didn't make sense.".to_string())?;

    // Some servers hang up after the status; then that round trip stands in for the ping.
    let ping = async {
        let mut packet = vec![0x01];
        packet.extend(1_234_567_i64.to_be_bytes());
        let mut out = Vec::new();
        write_packet(&mut out, &packet);
        let sent = Instant::now();
        stream.write_all(&out).await.ok()?;
        let length = read_varint(&mut stream).await.ok()?;
        let mut pong = vec![0; (length as usize).min(16)];
        stream.read_exact(&mut pong).await.ok()?;
        Some(sent.elapsed())
    };
    let trip = tokio::time::timeout(Duration::from_millis(1500), ping).await.ok().flatten().unwrap_or(status_trip);
    Ok((answer, trip.as_millis().min(u128::from(u32::MAX)) as u32))
}

fn reach_error(error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::ConnectionRefused => "Nothing is listening there.".into(),
        std::io::ErrorKind::TimedOut => "It didn't answer in time.".into(),
        _ if error.to_string().contains("lookup") || error.to_string().contains("resolve") || error.to_string().contains("nodename") => "That address doesn't exist.".into(),
        _ => "Couldn't reach it.".into(),
    }
}

fn read_status(address: &str, answer: &Value, ping: u32) -> ServerStatus {
    let players = answer.get("players");
    let count = |key: &str| players.and_then(|players| players.get(key)).and_then(Value::as_u64).map(|value| value.min(u64::from(u32::MAX)) as u32);
    let sample = players
        .and_then(|players| players.get("sample"))
        .and_then(Value::as_array)
        .map(|sample| {
            sample
                .iter()
                .filter_map(|entry| entry.get("name").and_then(Value::as_str))
                .map(strip_codes)
                .filter(|name| !name.trim().is_empty())
                .take(12)
                .collect()
        })
        .unwrap_or_default();
    let icon = answer.get("favicon").and_then(Value::as_str).filter(|icon| icon.starts_with("data:image/png;base64,") && icon.len() < 400_000).map(str::to_string);
    let version = answer.at("version", "name").and_then(Value::as_str).map(strip_codes).filter(|name| !name.trim().is_empty());
    ServerStatus {
        address: address.to_string(),
        online: true,
        motd: answer.get("description").map(motd).unwrap_or_default(),
        version,
        protocol: answer.at("version", "protocol").and_then(Value::as_i64).map(|protocol| protocol as i32),
        players: count("online"),
        max_players: count("max"),
        sample,
        icon,
        ping_ms: Some(ping),
        error: None,
        wisp: answer.get("wisp").is_some_and(Value::is_object),
        wisp_skin_key: answer.pointer("/wisp/skinKey").and_then(Value::as_str).filter(|key| key.len() < 4096).map(str::to_string),
    }
}

trait At {
    fn at(&self, outer: &str, inner: &str) -> Option<&Value>;
}

impl At for Value {
    fn at(&self, outer: &str, inner: &str) -> Option<&Value> {
        self.get(outer)?.get(inner)
    }
}

// ── The message of the day ──

const LEGACY_COLORS: [&str; 16] = [
    "#000000", "#0000aa", "#00aa00", "#00aaaa", "#aa0000", "#aa00aa", "#ffaa00", "#aaaaaa", "#555555", "#5555ff", "#55ff55", "#55ffff", "#ff5555", "#ff55ff", "#ffff55", "#ffffff",
];
const COLOR_NAMES: [&str; 16] = [
    "black", "dark_blue", "dark_green", "dark_aqua", "dark_red", "dark_purple", "gold", "gray", "dark_gray", "blue", "green", "aqua", "red", "light_purple", "yellow", "white",
];

#[derive(Debug, Clone, Default, PartialEq)]
struct Style {
    color: Option<String>,
    bold: bool,
    italic: bool,
    underlined: bool,
    strikethrough: bool,
    obfuscated: bool,
}

impl Style {
    fn with(&self, component: &serde_json::Map<String, Value>) -> Self {
        let mut style = self.clone();
        if let Some(color) = component.get("color").and_then(Value::as_str) {
            style.color = color_value(color);
        }
        let flag = |key: &str, current: bool| component.get(key).and_then(Value::as_bool).unwrap_or(current);
        style.bold = flag("bold", style.bold);
        style.italic = flag("italic", style.italic);
        style.underlined = flag("underlined", style.underlined);
        style.strikethrough = flag("strikethrough", style.strikethrough);
        style.obfuscated = flag("obfuscated", style.obfuscated);
        style
    }
}

fn color_value(color: &str) -> Option<String> {
    if let Some(hex) = color.strip_prefix('#') {
        return (hex.len() == 6 && hex.chars().all(|character| character.is_ascii_hexdigit())).then(|| format!("#{}", hex.to_ascii_lowercase()));
    }
    COLOR_NAMES.iter().position(|name| *name == color).map(|index| LEGACY_COLORS[index].to_string())
}

/// A text component (or plain string with § codes) as styled runs.
pub fn motd(description: &Value) -> Vec<MotdSpan> {
    let mut spans: Vec<MotdSpan> = Vec::new();
    walk(description, &Style::default(), &mut spans, 0);
    // Neighbours in the same style join up; the edges lose stray spaces and line breaks.
    let mut joined: Vec<MotdSpan> = Vec::new();
    for span in spans {
        match joined.last_mut() {
            Some(last) if MotdSpan { text: String::new(), ..last.clone() } == MotdSpan { text: String::new(), ..span.clone() } => last.text.push_str(&span.text),
            _ => joined.push(span),
        }
    }
    if let Some(first) = joined.first_mut() {
        first.text = first.text.trim_start().to_string();
    }
    if let Some(last) = joined.last_mut() {
        last.text = last.text.trim_end().to_string();
    }
    joined.retain(|span| !span.text.is_empty());
    joined
}

fn walk(value: &Value, inherited: &Style, out: &mut Vec<MotdSpan>, depth: usize) {
    if depth > 32 {
        return;
    }
    match value {
        Value::String(text) => push_legacy(text, inherited, out),
        Value::Array(items) => {
            // An array is a component whose first item styles the rest.
            let mut style = inherited.clone();
            for (index, item) in items.iter().enumerate() {
                walk(item, &style, out, depth + 1);
                if index == 0
                    && let Value::Object(first) = item
                {
                    style = style.with(first);
                }
            }
        }
        Value::Object(component) => {
            let style = inherited.with(component);
            if let Some(text) = component.get("text").and_then(Value::as_str) {
                push_legacy(text, &style, out);
            } else if let Some(key) = component.get("translate").and_then(Value::as_str) {
                push_legacy(component.get("fallback").and_then(Value::as_str).unwrap_or(key), &style, out);
            }
            if let Some(Value::Array(extra)) = component.get("extra") {
                for item in extra {
                    walk(item, &style, out, depth + 1);
                }
            }
        }
        Value::Number(number) => push_legacy(&number.to_string(), inherited, out),
        _ => {}
    }
}

/// Text that may still use the old § formatting codes.
fn push_legacy(text: &str, base: &Style, out: &mut Vec<MotdSpan>) {
    let mut style = base.clone();
    let mut run = String::new();
    let mut characters = text.chars();
    let flush = |run: &mut String, style: &Style, out: &mut Vec<MotdSpan>| {
        if !run.is_empty() {
            out.push(MotdSpan {
                text: std::mem::take(run),
                color: style.color.clone(),
                bold: style.bold,
                italic: style.italic,
                underlined: style.underlined,
                strikethrough: style.strikethrough,
                obfuscated: style.obfuscated,
            });
        }
    };
    while let Some(character) = characters.next() {
        if character != '§' {
            run.push(character);
            continue;
        }
        let Some(code) = characters.next() else { break };
        flush(&mut run, &style, out);
        match code.to_ascii_lowercase() {
            digit @ ('0'..='9' | 'a'..='f') => {
                // A colour code also clears bold, italics and the rest, as in the game.
                let index = digit.to_digit(16).unwrap_or(15) as usize;
                style = Style { color: Some(LEGACY_COLORS[index].to_string()), ..Style::default() };
            }
            'k' => style.obfuscated = true,
            'l' => style.bold = true,
            'm' => style.strikethrough = true,
            'n' => style.underlined = true,
            'o' => style.italic = true,
            'r' => style = base.clone(),
            _ => {}
        }
    }
    flush(&mut run, &style, out);
}

/// Text with the § codes taken out.
pub fn strip_codes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character == '§' {
            characters.next();
        } else {
            out.push(character);
        }
    }
    out
}

// ── The wire format ──

fn write_varint(out: &mut Vec<u8>, value: i32) {
    let mut value = value as u32;
    loop {
        if value & !0x7f == 0 {
            out.push(value as u8);
            return;
        }
        out.push((value & 0x7f) as u8 | 0x80);
        value >>= 7;
    }
}

fn write_string(out: &mut Vec<u8>, text: &str) {
    write_varint(out, text.len() as i32);
    out.extend(text.as_bytes());
}

fn write_packet(out: &mut Vec<u8>, body: &[u8]) {
    write_varint(out, body.len() as i32);
    out.extend(body);
}

async fn read_varint(stream: &mut (impl AsyncRead + Unpin)) -> std::io::Result<i32> {
    let mut value = 0u32;
    for shift in (0..35).step_by(7) {
        let byte = stream.read_u8().await?;
        value |= u32::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value as i32);
        }
    }
    Err(std::io::Error::other("varint too long"))
}

fn read_varint_slice(cursor: &mut &[u8]) -> Option<i32> {
    let mut value = 0u32;
    for shift in (0..35).step_by(7) {
        let (&byte, rest) = cursor.split_first()?;
        *cursor = rest;
        value |= u32::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value as i32);
        }
    }
    None
}

// ── Sessions ──

/// The server a log line says the game is connecting to (`Connecting to host, port`).
pub fn connecting_to(line: &str) -> Option<String> {
    let (_, rest) = line.split_once("Connecting to ")?;
    let (host, port) = rest.trim().rsplit_once(", ")?;
    let (host, port) = (host.trim(), port.trim().parse::<u16>().ok()?);
    if host.is_empty() || host.contains(' ') {
        return None;
    }
    Some(if port == DEFAULT_PORT { host.to_string() } else { format!("{host}:{port}") })
}

/// Servers the game connected to, from its log (`Connecting to host, port`), in order.
pub fn joined_in_log(log: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for line in log.lines() {
        let Some(address) = connecting_to(line) else {
            continue;
        };
        if !found.contains(&address) {
            found.push(address);
        }
    }
    found
}

// ── Discover ──

/// A public server Wisp suggests. Status comes live from the server itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicServer {
    pub name: String,
    pub address: String,
    pub description: String,
    /// "Minigames", "Survival", ...
    pub tags: Vec<String>,
    /// Languages it's mainly played in, as language codes.
    #[serde(default)]
    pub languages: Vec<String>,
}

/// The servers on Discover's Servers tab.
pub fn public_servers() -> Vec<PublicServer> {
    serde_json::from_str(include_str!("servers.json")).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("wisp-servers-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parses_addresses_the_way_the_game_does() {
        assert_eq!(ServerAddress::parse("play.example.net").unwrap(), ServerAddress { host: "play.example.net".into(), port: None });
        assert_eq!(ServerAddress::parse(" example.net:25570 ").unwrap().port, Some(25570));
        assert_eq!(ServerAddress::parse("[::1]:25566").unwrap(), ServerAddress { host: "::1".into(), port: Some(25566) });
        assert_eq!(ServerAddress::parse("2001:db8::1").unwrap().port, None);
        assert!(ServerAddress::parse("").is_err());
        assert!(ServerAddress::parse("bad host").is_err());
        assert!(ServerAddress::parse("example.net:99999").is_err());
        assert_eq!(address_key("Example.NET"), address_key("example.net:25565"));
        assert_ne!(address_key("example.net"), address_key("example.net:25566"));
    }

    #[test]
    fn adds_lists_and_removes_keeping_the_rest_of_the_file() {
        let dir = temp_dir("list");
        // A list the game wrote, with a field Wisp doesn't know about and a hidden entry.
        let mut first = HashMap::new();
        first.insert("name".to_string(), Tag::String("Home".into()));
        first.insert("ip".to_string(), Tag::String("home.example.net".into()));
        first.insert("acceptTextures".to_string(), Tag::Byte(1));
        let mut hidden = HashMap::new();
        hidden.insert("ip".to_string(), Tag::String("direct.example.net".into()));
        hidden.insert("hidden".to_string(), Tag::Byte(1));
        let mut root = HashMap::new();
        root.insert("servers".to_string(), Tag::List(vec![Tag::Compound(first), Tag::Compound(hidden)]));
        std::fs::write(dir.join(FILE), nbt::to_bytes(&Tag::Compound(root))).unwrap();

        add(&dir, "Friends", "friends.example.net:25570").unwrap();
        assert!(matches!(add(&dir, "Again", "HOME.example.net:25565"), Err(ServerError::AlreadyThere(_))));
        let listed = list(&dir);
        assert_eq!(listed.iter().map(|server| server.name.as_str()).collect::<Vec<_>>(), ["Home", "Friends"]);
        let raw = nbt::read_file(&dir.join(FILE)).unwrap();
        let Some(Tag::List(entries)) = raw.get("servers") else { panic!("no list") };
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].get("acceptTextures"), Some(&Tag::Byte(1)));
        assert!(dir.join("servers.dat_old").exists());

        assert!(remove(&dir, "home.example.net").unwrap());
        assert!(!remove(&dir, "home.example.net").unwrap());
        assert_eq!(list(&dir).len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_styled_and_legacy_motds() {
        let component = serde_json::json!({
            "text": "",
            "extra": [
                { "text": "Hello ", "color": "gold", "bold": true },
                { "text": "World", "color": "#12AB34" },
                "\n§cRed §lbold§r plain"
            ]
        });
        let spans = motd(&component);
        assert_eq!(spans[0], MotdSpan { text: "Hello ".into(), color: Some("#ffaa00".into()), bold: true, ..Default::default() });
        assert_eq!(spans[1].color.as_deref(), Some("#12ab34"));
        assert!(spans.iter().any(|span| span.text == "bold" && span.bold && span.color.as_deref() == Some("#ff5555")));
        assert_eq!(spans.last().unwrap().text, " plain");
        assert_eq!(motd(&Value::String("  §aA server  ".into())), vec![MotdSpan { text: "A server".into(), color: Some("#55ff55".into()), ..Default::default() }]);
        assert_eq!(strip_codes("§6Gold§r!"), "Gold!");
    }

    #[test]
    fn finds_servers_in_a_log() {
        let log = "[12:00:01] [Render thread/INFO]: Connecting to mc.example.net, 25565\n[12:30:00] [Render thread/INFO]: Connecting to 10.0.0.2, 25570\nConnecting to mc.example.net, 25565\n";
        assert_eq!(joined_in_log(log), ["mc.example.net", "10.0.0.2:25570"]);
    }

    #[test]
    fn varints_round_trip() {
        for value in [0, 1, 127, 128, 255, 25565, 2_097_151, -1, i32::MAX] {
            let mut bytes = Vec::new();
            write_varint(&mut bytes, value);
            let mut cursor: &[u8] = &bytes;
            assert_eq!(read_varint_slice(&mut cursor), Some(value));
        }
    }

    #[test]
    fn the_public_list_is_readable() {
        let servers = public_servers();
        assert!(servers.len() >= 10);
        for server in &servers {
            assert!(ServerAddress::parse(&server.address).is_ok(), "{}", server.address);
            assert!(!server.description.is_empty() && !server.tags.is_empty(), "{}", server.name);
        }
    }
}
