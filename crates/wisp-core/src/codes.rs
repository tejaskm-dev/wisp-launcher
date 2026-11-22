//! Join codes: a server hosted with Wisp has a code like `WISP-7K2M-9QXA-4F2P` that anyone with
//! Wisp types in to join it, wherever it can be reached right now. No domain, and nobody at Wisp
//! runs anything: the code is made from a key the server keeps, and the server says where it is in
//! an event on the same public Nostr relays the friends channel uses.
//!
//! The event names neither the code nor the server: it's filed under a hash of the code, signed
//! with the server's key (whose hash is the code, so nobody else can answer for it), and its
//! contents are encrypted to a key made from the code. So only someone who has the code finds the
//! server and reads where it is.

use nostr::nips::nip44;
use nostr::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use std::time::Duration;

/// NIP-78's kind for application data: replaceable, one per server.
const KIND: u16 = 30078;
/// Crockford's base 32: no I, L, O or U to misread.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// How long a server's word lasts on the relays without being said again.
const LASTS: Duration = Duration::from_secs(2 * 24 * 60 * 60);
const ASK_WITHIN: Duration = Duration::from_secs(8);

#[derive(Debug, thiserror::Error)]
pub enum CodeError {
    #[error("{0}")]
    Nostr(String),
    #[error("the relays couldn't be reached")]
    Offline,
}

/// What a code tells whoever has it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeCard {
    pub name: String,
    pub game_version: String,
    /// "vanilla", "fabric"…
    pub loader: String,
    /// A plugin server's software ("paper"…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub software: Option<String>,
    /// Where it can be reached, best first: `host` or `host:port`.
    pub addresses: Vec<String>,
    pub online: bool,
    #[serde(default)]
    pub players: u32,
    #[serde(default)]
    pub max_players: u32,
    #[serde(default)]
    pub mods: u32,
    /// Unix milliseconds, when the server said it.
    pub at: i64,
}

/// A server's own key, from the secret it keeps in its folder (hex), or a new one.
pub fn server_keys(secret: Option<&str>) -> Keys {
    secret.and_then(|hex| SecretKey::from_hex(hex.trim()).ok()).map(Keys::new).unwrap_or_else(Keys::generate)
}

/// The secret to keep for a server's key.
pub fn secret_of(keys: &Keys) -> String {
    keys.secret_key().to_secret_hex()
}

/// The code for a server's key: 60 bits of its hash, as `WISP-XXXX-XXXX-XXXX`.
pub fn code_of(public: &PublicKey) -> String {
    let digest = sha2::Sha256::digest(public.to_bytes());
    let bits = digest.iter().take(8).fold(0u64, |bits, byte| (bits << 8) | u64::from(*byte));
    let letters: String = (0..12).map(|index| ALPHABET[((bits >> (59 - 5 * index)) & 31) as usize] as char).collect();
    format!("WISP-{}-{}-{}", &letters[0..4], &letters[4..8], &letters[8..12])
}

/// A code as someone typed it, tidied up (any case, with or without dashes or "WISP", O for 0, I
/// or L for 1), or None when it can't be one.
pub fn normalize(text: &str) -> Option<String> {
    let typed: String = text.trim().to_ascii_uppercase().chars().filter(char::is_ascii_alphanumeric).collect();
    let typed = typed.strip_prefix("WISP").unwrap_or(&typed);
    let letters: String = typed
        .chars()
        .map(|c| match c {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        })
        .collect();
    if letters.len() != 12 || !letters.bytes().all(|byte| ALPHABET.contains(&byte)) {
        return None;
    }
    Some(format!("WISP-{}-{}-{}", &letters[0..4], &letters[4..8], &letters[8..12]))
}

/// Where the server's word is filed: a hash of the code, so the relays never see the code.
fn lookup(code: &str) -> String {
    crate::launch::hex(&sha2::Sha256::digest(format!("wisp-code-lookup:{code}").as_bytes())[..20])
}

/// The key the server's word is encrypted to: made from the code, so only its holders read it.
fn reader(code: &str) -> Keys {
    let mut seed = sha2::Sha256::digest(format!("wisp-code-reader:{code}").as_bytes()).to_vec();
    loop {
        if let Ok(secret) = SecretKey::from_slice(&seed) {
            return Keys::new(secret);
        }
        seed = sha2::Sha256::digest(&seed).to_vec();
    }
}

/// The event a server publishes to say where it is.
pub fn card_event(keys: &Keys, card: &CodeCard) -> Result<Event, CodeError> {
    let code = code_of(&keys.public_key());
    let text = serde_json::to_string(card).map_err(|error| CodeError::Nostr(error.to_string()))?;
    let content = nip44::encrypt(keys.secret_key(), &reader(&code).public_key(), text, nip44::Version::V2).map_err(|error| CodeError::Nostr(error.to_string()))?;
    let expires = Timestamp::from_secs(Timestamp::now().as_secs() + LASTS.as_secs());
    EventBuilder::new(Kind::Custom(KIND), content)
        .tags([Tag::identifier(lookup(&code)), Tag::expiration(expires)])
        .finalize(keys)
        .map_err(|error| CodeError::Nostr(error.to_string()))
}

/// What an event says, if it's the server's own word for this code.
pub fn read_card(code: &str, event: &Event) -> Option<CodeCard> {
    if event.kind != Kind::Custom(KIND) || code_of(&event.pubkey) != code || event.verify().is_err() {
        return None;
    }
    let text = nip44::decrypt(reader(code).secret_key(), &event.pubkey, &event.content).ok()?;
    serde_json::from_str(&text).ok()
}

/// A server's code identity: the key it keeps in a file in its folder.
pub struct ServerCode {
    keys: Keys,
}

impl ServerCode {
    /// The server's key from `path`, or a new one kept there (readable only by this user).
    pub fn load_or_make(path: &std::path::Path) -> std::io::Result<Self> {
        if let Ok(text) = std::fs::read_to_string(path)
            && let Ok(secret) = SecretKey::from_hex(text.trim())
        {
            return Ok(Self { keys: Keys::new(secret) });
        }
        let keys = Keys::generate();
        std::fs::write(path, secret_of(&keys))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(Self { keys })
    }

    pub fn code(&self) -> String {
        code_of(&self.keys.public_key())
    }

    /// Says where the server is, for whoever has its code.
    pub async fn publish(&self, card: &CodeCard, relays: &[String]) -> Result<(), CodeError> {
        publish(&self.keys, card, relays).await
    }
}

async fn client(relays: &[String]) -> Result<nostr_sdk::prelude::Client, CodeError> {
    // Two TLS backends are compiled in; rustls needs telling which to use (once per process).
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let client = nostr_sdk::prelude::Client::new();
    let mut added = 0;
    for relay in relays {
        if client.add_relay(relay.as_str()).await.is_ok() {
            added += 1;
        }
    }
    if added == 0 {
        return Err(CodeError::Offline);
    }
    client.connect().await;
    Ok(client)
}

/// Says where the server is, on the relays.
pub async fn publish(keys: &Keys, card: &CodeCard, relays: &[String]) -> Result<(), CodeError> {
    let event = card_event(keys, card)?;
    let client = client(relays).await?;
    let sent = tokio::time::timeout(ASK_WITHIN, client.send_event(&event)).await;
    client.shutdown().await;
    match sent {
        Ok(Ok(output)) if !output.success.is_empty() => Ok(()),
        Ok(Ok(_)) | Err(_) => Err(CodeError::Offline),
        Ok(Err(error)) => Err(CodeError::Nostr(error.to_string())),
    }
}

/// Finds the server a code belongs to, as it last said. None when no relay has heard of it.
pub async fn resolve(code: &str, relays: &[String]) -> Result<Option<CodeCard>, CodeError> {
    let client = client(relays).await?;
    let filter = Filter::new().kind(Kind::Custom(KIND)).identifier(lookup(code));
    let found = client.fetch_events(filter).timeout(ASK_WITHIN).await;
    client.shutdown().await;
    let events = found.map_err(|error| CodeError::Nostr(error.to_string()))?;
    Ok(events.into_iter().filter_map(|event| read_card(code, &event)).max_by_key(|card| card.at))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card() -> CodeCard {
        CodeCard {
            name: "Island".into(),
            game_version: "26.3".into(),
            loader: "fabric".into(),
            software: None,
            addresses: vec!["salmon-crop.gl.joinmc.link".into()],
            online: true,
            players: 1,
            max_players: 10,
            mods: 40,
            at: 5,
        }
    }

    #[test]
    fn codes_read_back_and_only_the_server_can_answer_for_its_code() {
        let server = Keys::generate();
        let code = code_of(&server.public_key());
        assert!(code.starts_with("WISP-") && code.len() == 19);
        assert_eq!(normalize(&code.to_lowercase().replace('-', " ")), Some(code.clone()));
        assert_eq!(normalize(&code.replace('0', "O")), Some(code.clone()));
        assert_eq!(normalize("WISP-1234"), None);
        assert_eq!(server_keys(Some(&secret_of(&server))).public_key(), server.public_key());

        let event = card_event(&server, &card()).unwrap();
        // The relays see neither the code nor where the server is.
        assert!(!event.content.contains("joinmc") && !event.tags.iter().any(|tag| tag.as_slice().iter().any(|part| part.contains(&code))));
        assert_eq!(read_card(&code, &event), Some(card()));
        // Someone else's event under the same lookup isn't taken for the server's.
        let impostor = Keys::generate();
        let text = serde_json::to_string(&CodeCard { addresses: vec!["evil.example".into()], ..card() }).unwrap();
        let content = nip44::encrypt(impostor.secret_key(), &reader(&code).public_key(), text, nip44::Version::V2).unwrap();
        let forged = EventBuilder::new(Kind::Custom(KIND), content).tags([Tag::identifier(lookup(&code))]).finalize(&impostor).unwrap();
        assert_eq!(read_card(&code, &forged), None);
        // Nor does another code read it.
        let other = code_of(&Keys::generate().public_key());
        assert_eq!(read_card(&other, &event), None);
    }
}
