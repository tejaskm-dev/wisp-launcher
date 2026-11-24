//! Your Wisp skin in game, for offline accounts. Minecraft asks Mojang's servers for a player's
//! skin; an offline account isn't known there, so the game falls back to Steve or Alex. The game
//! is started with authlib-injector (a small Java agent, see `launch`), which sends those
//! questions here instead: a tiny server on this computer only (127.0.0.1), answering with the
//! skin you're wearing in Wisp. Its answers are signed with a key kept in Wisp's data folder,
//! the way the game expects skin data to be.
//!
//! It answers what the game asks of Mojang's session server and services (profiles, skins, the
//! keys skin data is signed with, which features a player may use) and nothing else.
//!
//! On a server hosted here it answers for everyone who joins. A player whose Wisp hands in their
//! skin as their game connects (even when they joined from Minecraft's own server list) is waited
//! for a moment, so they arrive with it. Tester builds all sign with one key made from a build
//! secret, and every game trusts it, so players see each other's skins on any Wisp-hosted server.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey, LineEnding};
use rsa::signature::{SignatureEncoding, Signer};
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

/// A player the server can answer for.
#[derive(Debug, Clone)]
pub struct Player {
    /// Without dashes, as the game sends it.
    pub uuid: String,
    pub name: String,
    pub skin: Option<Skin>,
}

#[derive(Debug, Clone)]
pub struct Skin {
    pub png: Vec<u8>,
    /// Slim arms (Alex's model) rather than classic (Steve's).
    pub slim: bool,
}

/// Who the game is asking about.
#[derive(Debug, Clone, Copy)]
pub enum Who<'a> {
    Uuid(&'a str),
    Name(&'a str),
    /// A player logging in to a server hosted here, by name: only someone this Wisp really knows
    /// (its own account, or a guest who handed in a skin), never a stand-in.
    Joining(&'a str),
}

/// How long a server's login waits for a player's Wisp to hand in their skin (it does as their
/// game connects, a second or two through a tunnel), before letting them in without one.
pub const HAND_IN_WAIT: Duration = Duration::from_secs(6);

/// Keys besides its own whose skin data the game should trust (base64 DER public keys): servers
/// hosted with Wisp that sign with their own.
pub type Trusted = Arc<std::sync::RwLock<Vec<String>>>;

/// Finds a player, read fresh for each question (so a skin changed in Wisp shows next time the
/// game asks).
pub type Lookup = Arc<dyn Fn(Who<'_>) -> Option<Player> + Send + Sync>;

/// Friends playing on a server hosted here, by UUID (plain lowercase hex), with the skins they
/// handed in: the skin server answers for them too.
pub type Guests = Arc<std::sync::RwLock<std::collections::HashMap<String, Player>>>;

/// Told about each question the game asked and the status it got back (method, path, status),
/// for checking what a game really asks.
pub type Observe = Arc<dyn Fn(&str, &str, u16) + Send + Sync>;

/// A running skin server.
#[derive(Debug, Clone)]
pub struct SkinServer {
    /// Where authlib-injector points the game (ends in a slash).
    pub api_root: String,
    /// The server's description, handed to authlib-injector up front so it needn't ask.
    pub metadata: String,
    /// Its port on 127.0.0.1 (skin addresses in its answers name it).
    pub port: u16,
    /// The public key its answers are signed with (base64 DER), for others to trust.
    pub public_key: String,
}

/// Whether bytes are a skin the game can wear: a PNG, 64 by 64 or 64 by 32, and small.
pub fn valid_skin(png: &[u8]) -> bool {
    png.len() <= 64 * 1024
        && image::load_from_memory_with_format(png, image::ImageFormat::Png).is_ok_and(|picture| picture.width() == 64 && (picture.height() == 64 || picture.height() == 32))
}

struct Inner {
    port: u16,
    signer: SigningKey<sha1_10::Sha1>,
    public_der: String,
    metadata: String,
    lookup: Lookup,
    observe: Option<Observe>,
    trusted: Option<Trusted>,
    hand_in_wait: Duration,
}

/// The key every tester build signs skin data with, made from a secret the build carries (so the
/// same everywhere, and not in the source). Made the same way each time: a seeded generator
/// drives the key maker. A change to the `rsa` crate could change the key it makes from the same
/// seed, which `the_shared_key_is_the_same_every_time` catches.
pub fn shared_key(seed: &[u8]) -> std::io::Result<RsaPrivateKey> {
    use rand_chacha::rand_core::SeedableRng;
    let mut rng = rand_chacha::ChaCha20Rng::from_seed(<sha2::Sha256 as sha2::Digest>::digest(seed).into());
    RsaPrivateKey::new(&mut rng, 2048).map_err(std::io::Error::other)
}

/// A key's public half as the game takes it (base64 DER).
pub fn public_der(key: &RsaPrivateKey) -> std::io::Result<String> {
    let der = RsaPublicKey::from(key).to_public_key_der().map_err(std::io::Error::other)?;
    Ok(STANDARD.encode(der.as_bytes()))
}

/// The key skin data is signed with: loaded from `key_path`, or made (and kept there) the first time.
pub fn load_or_make_key(key_path: &Path) -> std::io::Result<RsaPrivateKey> {
    if let Ok(bytes) = std::fs::read(key_path)
        && let Ok(key) = RsaPrivateKey::from_pkcs8_der(&bytes)
    {
        return Ok(key);
    }
    let key = RsaPrivateKey::new(&mut rand_core::OsRng, 2048).map_err(std::io::Error::other)?;
    let der = key.to_pkcs8_der().map_err(std::io::Error::other)?;
    if let Some(parent) = key_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(key_path, der.as_bytes())?;
    Ok(key)
}

/// Starts the server on a free port of 127.0.0.1. It runs until Wisp quits.
pub async fn start(key: RsaPrivateKey, lookup: Lookup) -> std::io::Result<SkinServer> {
    start_observed(key, lookup, None).await
}

/// `start`, telling `observe` about every question the game asks.
pub async fn start_observed(key: RsaPrivateKey, lookup: Lookup, observe: Option<Observe>) -> std::io::Result<SkinServer> {
    start_with(key, lookup, observe, None, HAND_IN_WAIT).await
}

/// `start_observed`, also trusting `trusted` keys (read each time the game asks), and waiting up
/// to `hand_in_wait` for a player logging in whose skin hasn't been handed in yet.
pub async fn start_with(key: RsaPrivateKey, lookup: Lookup, observe: Option<Observe>, trusted: Option<Trusted>, hand_in_wait: Duration) -> std::io::Result<SkinServer> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    let public = RsaPublicKey::from(&key);
    let public_pem = public.to_public_key_pem(LineEnding::LF).map_err(std::io::Error::other)?;
    let public_der = STANDARD.encode(public.to_public_key_der().map_err(std::io::Error::other)?.as_bytes());
    let metadata = json!({
        "meta": {
            "serverName": "Wisp",
            "implementationName": "Wisp",
            "implementationVersion": env!("CARGO_PKG_VERSION"),
        },
        "skinDomains": ["127.0.0.1", "localhost"],
        "signaturePublickey": public_pem,
    })
    .to_string();
    let inner = Arc::new(Inner { port, signer: SigningKey::<sha1_10::Sha1>::new(key), public_der: public_der.clone(), metadata: metadata.clone(), lookup, observe, trusted, hand_in_wait });
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let inner = inner.clone();
            tokio::spawn(async move {
                let _ = serve(stream, &inner).await;
            });
        }
    });
    Ok(SkinServer { api_root: format!("http://127.0.0.1:{port}/"), metadata, port, public_key: public_der })
}

struct Response {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

impl Response {
    fn json(value: Value) -> Self {
        Self { status: 200, content_type: "application/json; charset=utf-8", body: value.to_string().into_bytes() }
    }
    fn empty(status: u16) -> Self {
        Self { status, content_type: "text/plain", body: Vec::new() }
    }
}

async fn serve(stream: TcpStream, inner: &Inner) -> std::io::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut head = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        head.push(line);
        if head.len() > 100 {
            return Ok(());
        }
    }
    let Some(request) = head.first() else {
        return Ok(());
    };
    let mut parts = request.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
    let length = head
        .iter()
        .skip(1)
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim().eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<usize>().ok()).flatten()
        })
        .unwrap_or(0)
        .min(1 << 16);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).await?;

    // Someone logging in whom this Wisp doesn't know yet: their Wisp may be handing in their skin
    // right now (it does as their game connects), so give it a moment.
    if method == "GET"
        && let Some(name) = joining_name(target)
        && wisp_name(&name)
    {
        let deadline = tokio::time::Instant::now() + inner.hand_in_wait;
        while (inner.lookup)(Who::Joining(&name)).is_none() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
    let response = answer(inner, method, target, &body);
    if let Some(observe) = &inner.observe {
        observe(method, target, response.status);
    }
    let reason = match response.status {
        200 => "OK",
        204 => "No Content",
        404 => "Not Found",
        _ => "Error",
    };
    let header = format!(
        "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len()
    );
    writer.write_all(header.as_bytes()).await?;
    writer.write_all(&response.body).await?;
    writer.shutdown().await
}

/// The name a login check (hasJoined) asks about.
fn joining_name(target: &str) -> Option<String> {
    let (path, query) = target.split_once('?')?;
    if path.trim_matches('/') != "sessionserver/session/minecraft/hasJoined" {
        return None;
    }
    query.split('&').find_map(|pair| pair.strip_prefix("username=")).map(str::to_string)
}

/// A name Minecraft would take (worth waiting for).
fn wisp_name(name: &str) -> bool {
    (1..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn answer(inner: &Inner, method: &str, target: &str, body: &[u8]) -> Response {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let param = |key: &str| query.split('&').find_map(|pair| pair.split_once('=').filter(|(name, _)| *name == key).map(|(_, value)| value));
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
    match (method, segments.as_slice()) {
        ("GET", [""]) => Response { status: 200, content_type: "application/json; charset=utf-8", body: inner.metadata.clone().into_bytes() },
        ("GET", ["sessionserver", "session", "minecraft", "profile", uuid]) => {
            let signed = param("unsigned") == Some("false");
            profile(inner, Who::Uuid(uuid), signed)
        }
        ("GET", ["sessionserver", "session", "minecraft", "hasJoined"]) => match param("username") {
            Some(name) => profile(inner, Who::Name(name), true),
            None => Response::empty(204),
        },
        ("POST", ["sessionserver", "session", "minecraft", "join"]) => Response::empty(204),
        ("POST", ["api", "profiles", "minecraft"]) => {
            let names: Vec<String> = serde_json::from_slice(body).unwrap_or_default();
            let found: Vec<Value> = names
                .iter()
                .take(10)
                .filter_map(|name| (inner.lookup)(Who::Name(name)))
                .map(|player| json!({ "id": player.uuid, "name": player.name }))
                .collect();
            Response::json(Value::Array(found))
        }
        ("GET", ["api", "users", "profiles", "minecraft", name]) => match (inner.lookup)(Who::Name(name)) {
            Some(player) => Response::json(json!({ "id": player.uuid, "name": player.name })),
            None => Response::empty(204),
        },
        ("GET", ["textures", uuid, hash]) => match (inner.lookup)(Who::Uuid(uuid)).and_then(|player| player.skin) {
            Some(skin) if texture_hash(&skin.png) == *hash => Response { status: 200, content_type: "image/png", body: skin.png },
            _ => Response::empty(404),
        },
        // The game checks skin data against these keys (Minecraft 1.19 and later).
        // Its own, and any others it was told to trust (servers hosted with Wisp that sign with
        // their own key): the game takes skin data signed by any of them.
        ("GET", ["minecraftservices", "publickeys"]) => {
            let mut keys = vec![inner.public_der.clone()];
            if let Some(trusted) = &inner.trusted {
                for key in trusted.read().unwrap_or_else(|poisoned| poisoned.into_inner()).iter() {
                    if !keys.contains(key) {
                        keys.push(key.clone());
                    }
                }
            }
            let listed: Vec<Value> = keys.iter().map(|key| json!({ "publicKey": key })).collect();
            Response::json(json!({
                "profilePropertyKeys": listed,
                "playerCertificateKeys": [{ "publicKey": inner.public_der }],
            }))
        }
        ("GET", ["minecraftservices", "player", "attributes"]) => Response::json(json!({
            "privileges": {
                "onlineChat": { "enabled": true },
                "multiplayerServer": { "enabled": true },
                "multiplayerRealms": { "enabled": false },
                "telemetry": { "enabled": false },
            },
            "profanityFilterPreferences": { "profanityFilterOn": false },
        })),
        ("GET", ["minecraftservices", "privacy", "blocklist"]) => Response::json(json!({ "blockedProfiles": [] })),
        _ => Response::empty(404),
    }
}

/// A player's profile with their skin, signed when the game asks for it signed.
fn profile(inner: &Inner, who: Who<'_>, signed: bool) -> Response {
    let Some(player) = (inner.lookup)(who) else {
        return Response::empty(204);
    };
    let mut textures = serde_json::Map::new();
    if let Some(skin) = &player.skin {
        let mut entry = json!({ "url": format!("http://127.0.0.1:{}/textures/{}/{}", inner.port, player.uuid, texture_hash(&skin.png)) });
        if skin.slim {
            entry["metadata"] = json!({ "model": "slim" });
        }
        textures.insert("SKIN".into(), entry);
    }
    let value = STANDARD.encode(
        json!({
            "timestamp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_millis() as u64),
            "profileId": player.uuid,
            "profileName": player.name,
            "textures": textures,
        })
        .to_string(),
    );
    let mut property = json!({ "name": "textures", "value": value });
    if signed {
        property["signature"] = json!(STANDARD.encode(inner.signer.sign(value.as_bytes()).to_bytes()));
    }
    Response::json(json!({ "id": player.uuid, "name": player.name, "properties": [property] }))
}

fn texture_hash(png: &[u8]) -> String {
    crate::launch::download::hex(&Sha1::digest(png))
}

impl SkinServer {
    /// The JVM arguments that start the game with authlib-injector pointed here.
    pub fn jvm_args(&self, injector: &Path) -> Vec<String> {
        vec![
            format!("-javaagent:{}={}", injector.display(), self.api_root),
            format!("-Dauthlibinjector.yggdrasil.prefetched={}", STANDARD.encode(&self.metadata)),
            // Otherwise it names the server on the title screen.
            "-Dauthlibinjector.noShowServerName".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::pkcs1v15::{Signature, VerifyingKey};
    use rsa::pkcs8::DecodePublicKey;
    use rsa::signature::Verifier;

    async fn get(port: u16, method: &str, path: &str, body: &str) -> (u16, Vec<u8>) {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let request = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n{body}", body.len());
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.unwrap();
        let split = raw.windows(4).position(|window| window == b"\r\n\r\n").unwrap();
        let status = std::str::from_utf8(&raw[9..12]).unwrap().parse().unwrap();
        (status, raw[split + 4..].to_vec())
    }

    #[tokio::test]
    async fn answers_the_game_with_a_signed_skin() {
        let dir = std::env::temp_dir().join(format!("wisp-skin-server-{}", std::process::id()));
        let key = load_or_make_key(&dir.join("key.der")).unwrap();
        // The second time, the same key comes back from disk.
        assert_eq!(load_or_make_key(&dir.join("key.der")).unwrap(), key);
        let png = vec![0x89, b'P', b'N', b'G', 1, 2, 3];
        let skin = png.clone();
        let lookup: Lookup = Arc::new(move |who| match who {
            Who::Uuid("0123456789abcdef0123456789abcdef") | Who::Name("Wisp") | Who::Joining("Wisp") => Some(Player { uuid: "0123456789abcdef0123456789abcdef".into(), name: "Wisp".into(), skin: Some(Skin { png: skin.clone(), slim: true }) }),
            _ => None,
        });
        let server = start(key, lookup).await.unwrap();
        let port: u16 = server.api_root.trim_end_matches('/').rsplit(':').next().unwrap().parse().unwrap();

        let (status, body) = get(port, "GET", "/", "").await;
        assert_eq!(status, 200);
        let metadata: Value = serde_json::from_slice(&body).unwrap();
        let public = RsaPublicKey::from_public_key_pem(metadata["signaturePublickey"].as_str().unwrap()).unwrap();

        let (status, body) = get(port, "GET", "/sessionserver/session/minecraft/profile/0123456789abcdef0123456789abcdef?unsigned=false", "").await;
        assert_eq!(status, 200);
        let profile: Value = serde_json::from_slice(&body).unwrap();
        let property = &profile["properties"][0];
        let value = property["value"].as_str().unwrap();
        let signature = Signature::try_from(STANDARD.decode(property["signature"].as_str().unwrap()).unwrap().as_slice()).unwrap();
        VerifyingKey::<sha1_10::Sha1>::new(public).verify(value.as_bytes(), &signature).expect("signed with the published key");
        let textures: Value = serde_json::from_slice(&STANDARD.decode(value).unwrap()).unwrap();
        assert_eq!(textures["textures"]["SKIN"]["metadata"]["model"], "slim");
        let url = textures["textures"]["SKIN"]["url"].as_str().unwrap();
        let path = url.split_once(&format!("127.0.0.1:{port}")).unwrap().1;
        assert_eq!(get(port, "GET", path, "").await, (200, png));

        assert_eq!(get(port, "GET", "/sessionserver/session/minecraft/profile/ffffffffffffffffffffffffffffffff", "").await.0, 204);
        let (status, body) = get(port, "POST", "/api/profiles/minecraft", r#"["Wisp","Nobody"]"#).await;
        assert_eq!((status, serde_json::from_slice::<Value>(&body).unwrap()), (200, json!([{ "id": "0123456789abcdef0123456789abcdef", "name": "Wisp" }])));
        assert_eq!(get(port, "GET", "/minecraftservices/player/attributes", "").await.0, 200);
        assert_eq!(get(port, "GET", "/somewhere/else", "").await.0, 404);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_login_waits_for_the_players_skin_to_be_handed_in() {
        let guests: Guests = Arc::default();
        let known = guests.clone();
        let lookup: Lookup = Arc::new(move |who| {
            let guests = known.read().unwrap();
            match who {
                Who::Joining(name) => guests.values().find(|guest| guest.name == name).cloned(),
                // Anyone else asked for by name is a stand-in without a skin.
                Who::Name(name) => Some(guests.values().find(|guest| guest.name == name).cloned().unwrap_or(Player { uuid: "f".repeat(32), name: name.into(), skin: None })),
                Who::Uuid(uuid) => guests.get(uuid).cloned(),
            }
        });
        let key = RsaPrivateKey::new(&mut rand_core::OsRng, 1024).unwrap();
        let server = start_with(key, lookup, None, None, Duration::from_secs(3)).await.unwrap();
        // Their Wisp hands the skin in a moment after the login check starts.
        let handing_in = {
            let guests = guests.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(600)).await;
                let skin = Skin { png: vec![1, 2, 3], slim: false };
                guests.write().unwrap().insert("a".repeat(32), Player { uuid: "a".repeat(32), name: "Friend".into(), skin: Some(skin) });
            })
        };
        let started = std::time::Instant::now();
        let (status, body) = get(server.port, "GET", "/sessionserver/session/minecraft/hasJoined?username=Friend&serverId=x", "").await;
        handing_in.await.unwrap();
        assert_eq!(status, 200);
        let profile: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(profile["id"], "a".repeat(32), "the handed-in player, not a stand-in");
        assert!(started.elapsed() >= Duration::from_millis(500) && started.elapsed() < Duration::from_secs(3));
        // Nobody hands one in: after the wait, a stand-in without a skin.
        let (status, body) = get(server.port, "GET", "/sessionserver/session/minecraft/hasJoined?username=Stranger&serverId=x", "").await;
        assert_eq!(status, 200);
        assert_eq!(serde_json::from_slice::<Value>(&body).unwrap()["id"], "f".repeat(32));
    }

    #[tokio::test]
    async fn the_game_trusts_the_keys_it_is_told_to() {
        let key = RsaPrivateKey::new(&mut rand_core::OsRng, 1024).unwrap();
        let other = RsaPrivateKey::new(&mut rand_core::OsRng, 1024).unwrap();
        let trusted: Trusted = Arc::default();
        let lookup: Lookup = Arc::new(|_| None);
        let server = start_with(key, lookup, None, Some(trusted.clone()), HAND_IN_WAIT).await.unwrap();
        trusted.write().unwrap().push(public_der(&other).unwrap());
        let (_, body) = get(server.port, "GET", "/minecraftservices/publickeys", "").await;
        let keys: Value = serde_json::from_slice(&body).unwrap();
        let listed: Vec<&str> = keys["profilePropertyKeys"].as_array().unwrap().iter().map(|key| key["publicKey"].as_str().unwrap()).collect();
        assert_eq!(listed, vec![server.public_key.as_str(), public_der(&other).unwrap().as_str()]);
    }

    #[test]
    fn the_shared_key_is_the_same_every_time() {
        // Pinned (made in another run): if this changes, testers on builds either side of the
        // change stop trusting each other's skins when joining from Minecraft's own list, until
        // both update.
        let key = shared_key(b"wisp test seed").unwrap();
        let fingerprint = crate::launch::hex(&<sha2::Sha256 as sha2::Digest>::digest(public_der(&key).unwrap().as_bytes())[..8]);
        assert_eq!(fingerprint, "fd031b11b57909f3");
    }
}
