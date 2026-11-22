//! Friends and parties without a server.
//!
//! Each Wisp makes its own key; its public half, as an `npub…` code, is the friend code people
//! swap. Friend requests, "what I'm playing" and party invites travel through public Nostr
//! relays inside NIP-59 gift wraps: end-to-end encrypted (NIP-44) and sent from a throwaway
//! key, so a relay sees neither what's said nor who sent it, only who it's for. Nobody at Wisp
//! runs anything, and the feature is off until the player turns it on.
//!
//! This module holds the state and the rules (pure, so they're tested without a network) and
//! the wire format; the app drives the relay connection.

use std::path::{Path, PathBuf};
use std::time::Duration;

use nostr::nips::nip59::{GiftWrapBuilder, UnwrappedGift};
use nostr::prelude::*;
use serde::{Deserialize, Serialize};

pub use nostr_sdk::prelude::ClientNotification;
pub use nostr_sdk::prelude::StreamExt;

/// Everything the relays send, as a stream.
pub type Notifications = std::pin::Pin<Box<dyn futures_core::Stream<Item = ClientNotification> + Send>>;

/// Relays used until the player picks their own. Any Nostr relay that accepts gift wraps works.
pub const DEFAULT_RELAYS: [&str; 3] = ["wss://relay.damus.io", "wss://nos.lol", "wss://relay.primal.net"];
/// The kind of the (never published) inner event: Wisp's own.
pub const RUMOR_KIND: u16 = 4510;
/// A friend who hasn't said they're around for this long is shown offline.
pub const PRESENCE_TTL_MS: i64 = 6 * 60 * 1000;
/// How often presence is re-sent while Wisp is open.
pub const PRESENCE_EVERY: Duration = Duration::from_secs(4 * 60);
const NAME_LIMIT: usize = 32;
const NOTE_LIMIT: usize = 200;
const PARTY_LIMIT: usize = 12;
const SEEN_LIMIT: usize = 2000;
/// The longest chat message.
pub const CHAT_TEXT_LIMIT: usize = 1000;
/// Messages kept per conversation.
const CHAT_KEPT: usize = 300;
/// Asking to join and answering only count while fresh: an old one sitting on a relay is ignored.
pub const JOIN_FRESH_MS: i64 = 2 * 60 * 1000;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Profile {
    pub name: String,
    /// The face of the skin they wear, as a tiny PNG data URL.
    pub face: Option<String>,
}

impl Profile {
    /// Trims what a stranger sent before it's stored or shown.
    fn cleaned(mut self) -> Self {
        self.name = self.name.trim().chars().filter(|c| !c.is_control()).take(NAME_LIMIT).collect();
        if self.name.is_empty() {
            self.name = "Wisp player".into();
        }
        self.face = self.face.filter(|face| face.starts_with("data:image/png;base64,") && face.len() < 4096);
        self
    }
}

/// What a friend is up to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum Status {
    Online,
    Playing {
        /// The instance, e.g. "Fabric 26.3", if they share it.
        label: Option<String>,
        /// The world, if they share it.
        world: Option<String>,
        since: i64,
    },
    Offline,
}

/// A server someone hosts, as friends see it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCard {
    pub id: String,
    pub name: String,
    pub game_version: String,
    /// "vanilla", "fabric", "quilt", "forge" or "neoforge".
    pub loader: String,
    #[serde(default)]
    pub loader_version: Option<String>,
    /// Up and taking players right now.
    #[serde(default)]
    pub online: bool,
    #[serde(default)]
    pub players: u32,
    #[serde(default)]
    pub max_players: u32,
    /// How many mods its instance has (friends who join get them).
    #[serde(default)]
    pub mods: u32,
    /// A plugin server's software ("paper", "purpur", "folia", "spigot"); players join with
    /// plain Minecraft.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub software: Option<String>,
}

impl ServerCard {
    fn cleaned(mut self) -> Self {
        self.name = self.name.trim().chars().filter(|c| !c.is_control()).take(48).collect();
        if self.name.is_empty() {
            self.name = "Server".into();
        }
        self.id = self.id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(64).collect();
        self.game_version = self.game_version.chars().filter(|c| !c.is_control()).take(32).collect();
        self.loader = self.loader.chars().filter(char::is_ascii_lowercase).take(16).collect();
        self.software = self.software.map(|software| software.chars().filter(char::is_ascii_lowercase).take(16).collect::<String>()).filter(|software| !software.is_empty());
        self.loader_version = self.loader_version.map(|version| version.chars().filter(|c| !c.is_control()).take(48).collect());
        self
    }
}

/// Who's asking to join, as Minecraft knows them (so the host can let them in by name).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    pub name: String,
    /// 32 hex digits.
    pub uuid: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Party {
    pub id: String,
    pub name: String,
    /// The friend code of whoever made it; they keep the member list.
    pub leader: String,
    pub members: Vec<String>,
    pub created: i64,
}

/// What travels inside a gift wrap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum Message {
    Request { profile: Profile, note: Option<String> },
    Accept { profile: Profile },
    Remove,
    Presence {
        profile: Profile,
        status: Status,
        /// Servers they host that friends can join right now.
        #[serde(default)]
        hosting: Vec<ServerCard>,
    },
    PartyInvite { party: Party },
    PartyJoin { party_id: String },
    /// From the leader: who's in the party now (empty when it ends).
    PartyUpdate { party: Party },
    PartyLeave { party_id: String },
    /// An invite to a server the sender hosts.
    ServerInvite { server: ServerCard },
    /// Asks to join a server the receiver hosts, with how to reach the asker (see `tunnel`).
    JoinRequest {
        server_id: String,
        /// Ties the answer to this ask.
        session: String,
        offer: crate::tunnel::Offer,
        #[serde(default)]
        player: Option<Player>,
    },
    /// The host's answer: how to reach it, or why not.
    JoinAnswer {
        session: String,
        #[serde(default)]
        offer: Option<crate::tunnel::Offer>,
        #[serde(default)]
        server: Option<ServerCard>,
        #[serde(default)]
        error: Option<String>,
    },
    /// A chat message to a friend, or to everyone in a party (sent to each of them).
    Chat {
        #[serde(default)]
        party_id: Option<String>,
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Friend {
    pub code: String,
    pub profile: Profile,
    pub since: i64,
    #[serde(default)]
    pub last_seen: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendRequest {
    pub code: String,
    pub profile: Profile,
    #[serde(default)]
    pub note: Option<String>,
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartyInvite {
    pub party: Party,
    pub from: String,
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInvite {
    pub server: ServerCard,
    pub from: String,
    pub at: i64,
}

/// Everything kept on disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SocialData {
    pub enabled: bool,
    /// The secret key, hex. Never leaves this computer.
    pub secret: Option<String>,
    pub profile: Profile,
    /// Show friends the instance and world being played.
    pub share_activity: bool,
    pub friends: Vec<Friend>,
    pub incoming: Vec<FriendRequest>,
    pub outgoing: Vec<FriendRequest>,
    pub party: Option<Party>,
    pub invites: Vec<PartyInvite>,
    /// Invites to friends' servers.
    pub server_invites: Vec<ServerInvite>,
    pub relays: Vec<String>,
    /// Inner event ids already handled: the same wrap can arrive from several relays.
    pub seen: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SocialError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("that isn't a friend code (they start with npub1)")]
    BadCode,
    #[error("that's your own code")]
    OwnCode,
    #[error("you're already friends")]
    AlreadyFriends,
    #[error("only the party's leader can do that")]
    NotLeader,
    #[error("the party is full")]
    PartyFull,
    #[error("{0}")]
    Other(String),
}

/// What an incoming message changed, for the app to react to (reply, refresh, tell the player).
#[derive(Debug, Clone, PartialEq)]
pub enum Applied {
    Nothing,
    NewRequest { from: String, name: String },
    /// A request from someone we'd asked too, or a repeat from a friend: we're friends; reply Accept.
    BecameFriends { code: String, name: String, reply: bool },
    Accepted { code: String, name: String },
    Removed { code: String },
    Presence { code: String, status: Status },
    Invited { from: String, party: String },
    /// We lead the party and its members changed: tell everyone.
    PartyChanged,
    PartyUpdated,
    PartyEnded,
    ServerInvited { from: String, server: String },
    /// A friend wants to join a server we host (the app checks it and answers).
    JoinRequested { from: String, server_id: String, session: String, offer: crate::tunnel::Offer, player: Option<Player> },
    /// A host answered our ask to join.
    JoinAnswered { from: String, session: String, offer: Option<crate::tunnel::Offer>, server: Option<ServerCard>, error: Option<String> },
    /// A chat message, already cleaned, for `conversation` ("party:<id>" or "friend:<code>").
    Chat { from: String, conversation: String, text: String },
}

/// The friend code (npub) for a public key.
pub fn code_of(public_key: &PublicKey) -> String {
    public_key.to_bech32().unwrap_or_else(|_| public_key.to_hex())
}

/// Reads a friend code people paste: an npub, or a hex key, with stray spaces or `nostr:` in front.
pub fn parse_code(text: &str) -> Result<PublicKey, SocialError> {
    let cleaned = text.trim().trim_start_matches("nostr:");
    PublicKey::parse(cleaned).map_err(|_| SocialError::BadCode)
}

pub struct SocialStore {
    path: PathBuf,
}

impl SocialStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { path: dir.into().join("social.json") }
    }

    pub fn load(&self) -> SocialData {
        let mut data: SocialData = std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_else(|| SocialData { share_activity: true, ..Default::default() });
        if data.relays.is_empty() {
            data.relays = DEFAULT_RELAYS.iter().map(|relay| relay.to_string()).collect();
        }
        data
    }

    /// Saves atomically, readable only by this user (it holds the secret key).
    pub fn save(&self, data: &SocialData) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let temp = self.path.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_vec_pretty(data).map_err(std::io::Error::other)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(temp, &self.path)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl SocialData {
    /// This Wisp's keys, made the first time they're needed.
    pub fn keys(&mut self) -> Keys {
        if let Some(keys) = self.secret.as_deref().and_then(|secret| Keys::parse(secret).ok()) {
            return keys;
        }
        let keys = Keys::generate();
        self.secret = Some(keys.secret_key().to_secret_hex());
        keys
    }

    pub fn is_friend(&self, code: &str) -> bool {
        self.friends.iter().any(|friend| friend.code == code)
    }

    /// Records an outgoing request (the app sends it).
    pub fn request(&mut self, code: &str, my_code: &str, now: i64) -> Result<(), SocialError> {
        if code == my_code {
            return Err(SocialError::OwnCode);
        }
        if self.is_friend(code) {
            return Err(SocialError::AlreadyFriends);
        }
        self.outgoing.retain(|request| request.code != code);
        self.outgoing.push(FriendRequest { code: code.to_string(), profile: Profile { name: short_code(code), face: None }, note: None, at: now });
        Ok(())
    }

    /// Accepts an incoming request (the app sends Accept).
    pub fn accept(&mut self, code: &str, now: i64) -> Option<Friend> {
        let index = self.incoming.iter().position(|request| request.code == code)?;
        let request = self.incoming.remove(index);
        let friend = Friend { code: request.code, profile: request.profile, since: now, last_seen: None };
        self.friends.push(friend.clone());
        Some(friend)
    }

    pub fn decline(&mut self, code: &str) {
        self.incoming.retain(|request| request.code != code);
    }

    pub fn remove(&mut self, code: &str) {
        self.friends.retain(|friend| friend.code != code);
        self.outgoing.retain(|request| request.code != code);
        self.incoming.retain(|request| request.code != code);
        self.invites.retain(|invite| invite.from != code);
        self.server_invites.retain(|invite| invite.from != code);
    }

    /// Starts a party with us as its leader.
    pub fn create_party(&mut self, name: &str, my_code: &str, now: i64) -> Party {
        let name: String = name.trim().chars().take(NAME_LIMIT).collect();
        let party = Party {
            id: crate::journal::new_id(now),
            name: if name.is_empty() { "Party".into() } else { name },
            leader: my_code.to_string(),
            members: vec![my_code.to_string()],
            created: now,
        };
        self.party = Some(party.clone());
        party
    }

    /// Takes an invite: we're in that party now (the app tells its leader).
    pub fn join(&mut self, party_id: &str, my_code: &str) -> Option<Party> {
        let index = self.invites.iter().position(|invite| invite.party.id == party_id)?;
        let mut party = self.invites.remove(index).party;
        if !party.members.iter().any(|member| member == my_code) {
            party.members.push(my_code.to_string());
        }
        self.party = Some(party.clone());
        Some(party)
    }

    /// Marks a message handled; false if it had been already.
    fn first_time(&mut self, id: &str) -> bool {
        if self.seen.iter().any(|seen| seen == id) {
            return false;
        }
        self.seen.push(id.to_string());
        if self.seen.len() > SEEN_LIMIT {
            let extra = self.seen.len() - SEEN_LIMIT;
            self.seen.drain(..extra);
        }
        true
    }

    /// Applies a message from `from` (a friend code) received at `now`. `id` is the message's own
    /// id, so a repeat is ignored. Unknown senders can only send requests.
    pub fn apply(&mut self, id: &str, from: &str, my_code: &str, message: Message, now: i64) -> Applied {
        if from == my_code || !self.first_time(id) {
            return Applied::Nothing;
        }
        let friend = self.is_friend(from);
        match message {
            Message::Request { profile, note } => {
                let profile = profile.cleaned();
                let name = profile.name.clone();
                if friend || self.outgoing.iter().any(|request| request.code == from) {
                    self.outgoing.retain(|request| request.code != from);
                    if !friend {
                        self.friends.push(Friend { code: from.to_string(), profile, since: now, last_seen: Some(now) });
                    }
                    return Applied::BecameFriends { code: from.to_string(), name, reply: true };
                }
                let note = note.map(|note| note.trim().chars().take(NOTE_LIMIT).collect::<String>()).filter(|note| !note.is_empty());
                self.incoming.retain(|request| request.code != from);
                self.incoming.push(FriendRequest { code: from.to_string(), profile, note, at: now });
                Applied::NewRequest { from: from.to_string(), name }
            }
            Message::Accept { profile } => {
                let profile = profile.cleaned();
                let name = profile.name.clone();
                if let Some(known) = self.friends.iter_mut().find(|known| known.code == from) {
                    known.profile = profile;
                    return Applied::Nothing;
                }
                if !self.outgoing.iter().any(|request| request.code == from) {
                    return Applied::Nothing;
                }
                self.outgoing.retain(|request| request.code != from);
                self.incoming.retain(|request| request.code != from);
                self.friends.push(Friend { code: from.to_string(), profile, since: now, last_seen: Some(now) });
                Applied::Accepted { code: from.to_string(), name }
            }
            Message::Remove => {
                if !friend && !self.outgoing.iter().any(|request| request.code == from) && !self.incoming.iter().any(|request| request.code == from) {
                    return Applied::Nothing;
                }
                self.remove(from);
                if let Some(party) = &mut self.party
                    && party.leader == my_code
                {
                    party.members.retain(|member| member != from);
                }
                Applied::Removed { code: from.to_string() }
            }
            Message::Presence { profile, status, .. } => {
                let Some(known) = self.friends.iter_mut().find(|known| known.code == from) else {
                    return Applied::Nothing;
                };
                known.profile = profile.cleaned();
                known.last_seen = Some(now);
                Applied::Presence { code: from.to_string(), status }
            }
            Message::PartyInvite { party } => {
                if !friend || party.leader != from {
                    return Applied::Nothing;
                }
                let name = party.name.clone();
                self.invites.retain(|invite| invite.party.id != party.id);
                self.invites.push(PartyInvite { party, from: from.to_string(), at: now });
                Applied::Invited { from: from.to_string(), party: name }
            }
            Message::PartyJoin { party_id } => {
                let Some(party) = &mut self.party else {
                    return Applied::Nothing;
                };
                if !friend || party.id != party_id || party.leader != my_code {
                    return Applied::Nothing;
                }
                if !party.members.iter().any(|member| member == from) && party.members.len() < PARTY_LIMIT {
                    party.members.push(from.to_string());
                }
                Applied::PartyChanged
            }
            Message::PartyUpdate { party } => {
                if party.leader != from {
                    return Applied::Nothing;
                }
                let ours = self.party.as_ref().is_some_and(|current| current.id == party.id);
                if !ours {
                    // An invite we haven't answered gets the new roster too.
                    if let Some(invite) = self.invites.iter_mut().find(|invite| invite.party.id == party.id) {
                        invite.party = party;
                    }
                    return Applied::Nothing;
                }
                if party.members.iter().any(|member| member == my_code) {
                    self.party = Some(party);
                    Applied::PartyUpdated
                } else {
                    self.party = None;
                    Applied::PartyEnded
                }
            }
            Message::PartyLeave { party_id } => {
                let Some(party) = &mut self.party else {
                    return Applied::Nothing;
                };
                if party.id != party_id {
                    return Applied::Nothing;
                }
                if party.leader == my_code {
                    party.members.retain(|member| member != from);
                    return Applied::PartyChanged;
                }
                if party.leader == from {
                    self.party = None;
                    return Applied::PartyEnded;
                }
                Applied::Nothing
            }
            Message::ServerInvite { server } => {
                if !friend {
                    return Applied::Nothing;
                }
                let server = server.cleaned();
                let name = server.name.clone();
                self.server_invites.retain(|invite| !(invite.from == from && invite.server.id == server.id));
                self.server_invites.push(ServerInvite { server, from: from.to_string(), at: now });
                Applied::ServerInvited { from: from.to_string(), server: name }
            }
            Message::JoinRequest { server_id, session, offer, player } => {
                if !friend {
                    return Applied::Nothing;
                }
                Applied::JoinRequested { from: from.to_string(), server_id, session, offer, player }
            }
            Message::JoinAnswer { session, offer, server, error } => {
                if !friend {
                    return Applied::Nothing;
                }
                let error = error.map(|error| error.chars().filter(|c| !c.is_control()).take(NOTE_LIMIT).collect());
                Applied::JoinAnswered { from: from.to_string(), session, offer, server: server.map(ServerCard::cleaned), error }
            }
            Message::Chat { party_id, text } => {
                let text = clean_chat(&text);
                if text.is_empty() {
                    return Applied::Nothing;
                }
                let conversation = match party_id {
                    // Party chat counts from whoever is in our party with us.
                    Some(party_id) => {
                        let member = self.party.as_ref().is_some_and(|party| party.id == party_id && party.members.iter().any(|member| member == from));
                        if !member {
                            return Applied::Nothing;
                        }
                        format!("party:{party_id}")
                    }
                    None if friend => format!("friend:{from}"),
                    None => return Applied::Nothing,
                };
                Applied::Chat { from: from.to_string(), conversation, text }
            }
        }
    }
}

/// Chat text as it's kept and shown: trimmed, no control characters but line breaks, not too long.
pub fn clean_chat(text: &str) -> String {
    text.trim().chars().filter(|c| *c == '\n' || !c.is_control()).take(CHAT_TEXT_LIMIT).collect::<String>().trim().to_string()
}

/// One chat message, sent or received.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    /// The sender's friend code.
    pub from: String,
    pub text: String,
    /// Unix milliseconds, when it was written.
    pub at: i64,
}

/// Every conversation's messages ("party:<id>" or "friend:<code>"), and when each was last read.
/// Kept apart from the rest of the friends data, which is saved far more often.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChatLog {
    pub conversations: std::collections::HashMap<String, Vec<ChatMessage>>,
    pub read: std::collections::HashMap<String, i64>,
}

impl ChatLog {
    /// Adds a message in time order; false if it was already there.
    pub fn add(&mut self, conversation: &str, message: ChatMessage) -> bool {
        let messages = self.conversations.entry(conversation.to_string()).or_default();
        if messages.iter().any(|known| known.id == message.id) {
            return false;
        }
        let at = messages.partition_point(|known| known.at <= message.at);
        messages.insert(at, message);
        if messages.len() > CHAT_KEPT {
            let extra = messages.len() - CHAT_KEPT;
            messages.drain(..extra);
        }
        true
    }

    pub fn messages(&self, conversation: &str) -> &[ChatMessage] {
        self.conversations.get(conversation).map_or(&[], Vec::as_slice)
    }

    /// Messages from others since the conversation was last read.
    pub fn unread(&self, conversation: &str, me: &str) -> usize {
        let read = self.read.get(conversation).copied().unwrap_or(0);
        self.messages(conversation).iter().filter(|message| message.from != me && message.at > read).count()
    }

    pub fn mark_read(&mut self, conversation: &str, now: i64) {
        self.read.insert(conversation.to_string(), now);
    }

    /// Forgets a conversation (a friend removed, a party over).
    pub fn forget(&mut self, conversation: &str) {
        self.conversations.remove(conversation);
        self.read.remove(conversation);
    }
}

pub struct ChatStore {
    path: PathBuf,
}

impl ChatStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { path: dir.into().join("chat.json") }
    }

    pub fn load(&self) -> ChatLog {
        std::fs::read(&self.path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    /// Saves atomically, readable only by this user.
    pub fn save(&self, log: &ChatLog) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let temp = self.path.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_vec(log).map_err(std::io::Error::other)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(temp, &self.path)
    }
}

/// A short form of a friend code for showing before a name is known: "npub1abc…wxyz".
pub fn short_code(code: &str) -> String {
    if code.len() <= 16 {
        return code.to_string();
    }
    format!("{}…{}", &code[..9], &code[code.len() - 4..])
}

/// Wraps a message for one friend. Requests and invites last a month on relays; presence ten
/// minutes.
pub fn wrap(keys: &Keys, to: &PublicKey, message: &Message) -> Result<Event, SocialError> {
    let content = serde_json::to_string(message).map_err(|error| SocialError::Other(error.to_string()))?;
    let rumor = EventBuilder::new(Kind::Custom(RUMOR_KIND), content).finalize_unsigned(keys.public_key());
    let lasts = match message {
        Message::Presence { .. } => Duration::from_secs(15 * 60),
        // Only useful for the minute or two someone waits to join.
        Message::JoinRequest { .. } | Message::JoinAnswer { .. } => Duration::from_secs(10 * 60),
        _ => Duration::from_secs(30 * 24 * 60 * 60),
    };
    // The wrap's own time is backdated at random (up to two days) to hide when it was sent, and
    // the builder counts expiry from that, which would expire presence before it's sent. So the
    // expiry is set here, from now.
    let expires = Timestamp::from_secs(Timestamp::now().as_secs() + lasts.as_secs());
    GiftWrapBuilder::new(*to, rumor)
        .extra_tags([Tag::expiration(expires)])
        .finalize(keys)
        .map_err(|error| SocialError::Other(error.to_string()))
}

/// A message unwrapped: its id, who sent it (verified), when they wrote it (Unix ms), and what.
pub struct Received {
    pub id: String,
    pub from: PublicKey,
    pub at: i64,
    pub message: Message,
}

/// Opens a gift wrap meant for us. None if it isn't one of Wisp's.
pub fn unwrap(keys: &Keys, event: &Event) -> Option<Received> {
    if event.kind != Kind::GiftWrap {
        return None;
    }
    let UnwrappedGift { sender, rumor } = UnwrappedGift::from_gift_wrap(keys, event).ok()?;
    if rumor.kind != Kind::Custom(RUMOR_KIND) {
        return None;
    }
    let message: Message = serde_json::from_str(&rumor.content).ok()?;
    let id = rumor.id.map(|id| id.to_hex()).unwrap_or_else(|| format!("{}:{}", sender.to_hex(), rumor.created_at.as_secs()));
    Some(Received { id, from: sender, at: rumor.created_at.as_secs() as i64 * 1000, message })
}

/// The relay connection: listens for gift wraps addressed to us and sends ours.
pub struct Net {
    client: nostr_sdk::prelude::Client,
    keys: Keys,
}

impl Net {
    /// Connects to the relays and asks each for wraps addressed to us from the last few days
    /// (wraps carry a randomised time up to two days back, so the window is wide; repeats are
    /// dropped by `SocialData::apply`). Take `notifications` before the first message arrives.
    pub async fn connect(keys: Keys, relays: &[String]) -> Result<(Self, Notifications), SocialError> {
        // Two TLS backends are compiled in; rustls needs telling which to use (once per process).
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let client = nostr_sdk::prelude::Client::new();
        let notifications = client.notifications();
        let mut added = 0;
        for relay in relays {
            if client.add_relay(relay.as_str()).await.is_ok() {
                added += 1;
            }
        }
        if added == 0 {
            return Err(SocialError::Other("none of the relays could be added".into()));
        }
        client.connect().await;
        let since = Timestamp::from_secs(Timestamp::now().as_secs().saturating_sub(3 * 24 * 60 * 60));
        let filter = Filter::new().kind(Kind::GiftWrap).pubkey(keys.public_key()).since(since);
        client.subscribe(filter).await.map_err(|error| SocialError::Other(error.to_string()))?;
        Ok((Self { client, keys }, notifications))
    }

    pub async fn send(&self, to: &PublicKey, message: &Message) -> Result<(), SocialError> {
        let event = wrap(&self.keys, to, message)?;
        self.client.send_event(&event).await.map_err(|error| SocialError::Other(error.to_string()))?;
        Ok(())
    }

    pub fn keys(&self) -> &Keys {
        &self.keys
    }

    /// How many relays are connected right now.
    pub async fn connected(&self) -> usize {
        self.client.relays().await.values().filter(|relay| relay.status().is_connected()).count()
    }

    pub async fn shutdown(&self) {
        self.client.shutdown().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(name: &str) -> Profile {
        Profile { name: name.into(), face: None }
    }

    #[test]
    fn wraps_and_unwraps_only_for_the_receiver() {
        let (alice, bob, eve) = (Keys::generate(), Keys::generate(), Keys::generate());
        let message = Message::Request { profile: profile("Alice"), note: Some("hi!".into()) };
        let wrapped = wrap(&alice, &bob.public_key(), &message).unwrap();
        // The outside names only Bob, and was signed by a throwaway key, not Alice's.
        assert_eq!(wrapped.kind, Kind::GiftWrap);
        assert_ne!(wrapped.pubkey, alice.public_key());
        let received = unwrap(&bob, &wrapped).unwrap();
        assert_eq!(received.from, alice.public_key());
        assert_eq!(received.message, message);
        assert!(unwrap(&eve, &wrapped).is_none());
    }

    #[test]
    fn requests_accepts_presence_and_parties() {
        let (alice, bob) = ("npub1alice", "npub1bob");
        let mut a = SocialData::default();
        let mut b = SocialData::default();

        // Alice asks Bob; Bob sees a request; Bob accepts; Alice gets the accept.
        a.request(bob, alice, 1).unwrap();
        assert!(matches!(a.request(alice, alice, 1), Err(SocialError::OwnCode)));
        let applied = b.apply("m1", alice, bob, Message::Request { profile: profile("Alice\n\u{7}"), note: Some("  play?  ".into()) }, 2);
        assert_eq!(applied, Applied::NewRequest { from: alice.into(), name: "Alice".into() });
        assert_eq!(b.incoming[0].note.as_deref(), Some("play?"));
        // The same wrap from a second relay changes nothing.
        assert_eq!(b.apply("m1", alice, bob, Message::Request { profile: profile("Alice"), note: None }, 2), Applied::Nothing);
        b.accept(alice, 3).unwrap();
        assert!(b.is_friend(alice));
        assert_eq!(a.apply("m2", bob, alice, Message::Accept { profile: profile("Bob") }, 4), Applied::Accepted { code: bob.into(), name: "Bob".into() });
        assert!(a.is_friend(bob) && a.outgoing.is_empty());
        // An accept nobody asked for is ignored.
        assert_eq!(a.apply("m3", "npub1mallory", alice, Message::Accept { profile: profile("M") }, 4), Applied::Nothing);
        assert!(!a.is_friend("npub1mallory"));

        // Presence only counts from friends.
        let playing = Status::Playing { label: Some("Fabric 26.3".into()), world: Some("Island".into()), since: 5 };
        assert!(matches!(a.apply("m4", bob, alice, Message::Presence { profile: profile("Bob"), status: playing.clone(), hosting: vec![] }, 6), Applied::Presence { .. }));
        assert_eq!(a.friends[0].last_seen, Some(6));
        assert_eq!(a.apply("m5", "npub1stranger", alice, Message::Presence { profile: profile("S"), status: playing, hosting: vec![] }, 6), Applied::Nothing);

        // Alice starts a party and invites Bob; Bob joins; Alice adds him and tells everyone.
        let party = a.create_party("Build night", alice, 7);
        assert_eq!(b.apply("m6", alice, bob, Message::PartyInvite { party: party.clone() }, 8), Applied::Invited { from: alice.into(), party: "Build night".into() });
        let joined = b.join(&party.id, bob).unwrap();
        assert_eq!(joined.members, vec![alice.to_string(), bob.to_string()]);
        assert_eq!(a.apply("m7", bob, alice, Message::PartyJoin { party_id: party.id.clone() }, 9), Applied::PartyChanged);
        let roster = a.party.clone().unwrap();
        assert_eq!(roster.members.len(), 2);
        assert_eq!(b.apply("m8", alice, bob, Message::PartyUpdate { party: roster.clone() }, 10), Applied::PartyUpdated);
        // Only the leader's word counts for the roster.
        assert_eq!(b.apply("m9", "npub1other", bob, Message::PartyUpdate { party: roster.clone() }, 10), Applied::Nothing);
        // Bob leaves; Alice drops him. Then Alice ends it for Bob by leaving.
        assert_eq!(a.apply("m10", bob, alice, Message::PartyLeave { party_id: party.id.clone() }, 11), Applied::PartyChanged);
        assert_eq!(a.party.as_ref().unwrap().members, vec![alice.to_string()]);
        assert_eq!(b.apply("m11", alice, bob, Message::PartyLeave { party_id: party.id.clone() }, 12), Applied::PartyEnded);
        assert!(b.party.is_none());

        // Removing someone ends the friendship on both sides.
        assert_eq!(b.apply("m12", alice, bob, Message::Remove, 13), Applied::Removed { code: alice.into() });
        assert!(!b.is_friend(alice));
    }

    fn card(id: &str) -> ServerCard {
        ServerCard {
            id: id.into(),
            name: " Build\u{7} night ".into(),
            game_version: "26.3".into(),
            loader: "fabric".into(),
            loader_version: Some("0.20.1".into()),
            online: true,
            players: 1,
            max_players: 10,
            mods: 40,
            software: None,
        }
    }

    #[test]
    fn server_invites_join_requests_and_chat() {
        let (alice, bob, eve) = ("npub1alice", "npub1bob", "npub1eve");
        let mut b = SocialData { friends: vec![Friend { code: alice.into(), profile: profile("Alice"), since: 1, last_seen: None }], ..Default::default() };
        // Invites and asks only count from friends.
        assert_eq!(b.apply("i1", alice, bob, Message::ServerInvite { server: card("build") }, 5), Applied::ServerInvited { from: alice.into(), server: "Build night".into() });
        assert_eq!(b.server_invites.len(), 1);
        assert_eq!(b.apply("i2", alice, bob, Message::ServerInvite { server: card("build") }, 6), Applied::ServerInvited { from: alice.into(), server: "Build night".into() });
        assert_eq!(b.server_invites.len(), 1, "a second invite to the same server replaces the first");
        assert_eq!(b.apply("i3", eve, bob, Message::ServerInvite { server: card("x") }, 6), Applied::Nothing);
        let offer = crate::tunnel::Offer { candidates: vec!["192.168.1.2:5000".into()], fingerprint: "ab".repeat(32), doors: Vec::new() };
        let asked = b.apply("j1", alice, bob, Message::JoinRequest { server_id: "build".into(), session: "s1".into(), offer: offer.clone(), player: None }, 7);
        assert!(matches!(asked, Applied::JoinRequested { ref server_id, .. } if server_id == "build"));
        assert_eq!(b.apply("j2", eve, bob, Message::JoinRequest { server_id: "build".into(), session: "s2".into(), offer, player: None }, 7), Applied::Nothing);
        let answered = b.apply("j3", alice, bob, Message::JoinAnswer { session: "s1".into(), offer: None, server: None, error: Some("It isn't running\n".into()) }, 8);
        assert_eq!(answered, Applied::JoinAnswered { from: alice.into(), session: "s1".into(), offer: None, server: None, error: Some("It isn't running".into()) });

        // Chat: from friends directly, and from people in our party.
        assert_eq!(b.apply("c1", alice, bob, Message::Chat { party_id: None, text: "  hi\u{1b}[31m  ".into() }, 9), Applied::Chat { from: alice.into(), conversation: format!("friend:{alice}"), text: "hi[31m".into() });
        assert_eq!(b.apply("c2", eve, bob, Message::Chat { party_id: None, text: "hello?".into() }, 9), Applied::Nothing);
        b.party = Some(Party { id: "p".into(), name: "P".into(), leader: alice.into(), members: vec![alice.into(), bob.into(), eve.into()], created: 1 });
        assert_eq!(b.apply("c3", eve, bob, Message::Chat { party_id: Some("p".into()), text: "party time".into() }, 10), Applied::Chat { from: eve.into(), conversation: "party:p".into(), text: "party time".into() });
        assert_eq!(b.apply("c4", eve, bob, Message::Chat { party_id: Some("other".into()), text: "x".into() }, 10), Applied::Nothing);
        assert_eq!(b.apply("c5", alice, bob, Message::Chat { party_id: None, text: "   ".into() }, 10), Applied::Nothing);
        // Removing a friend drops their server invites.
        b.remove(alice);
        assert!(b.server_invites.is_empty());

        // A presence from an older Wisp (no servers in it) still reads.
        let old: Message = serde_json::from_str(r#"{"type":"presence","profile":{"name":"Old"},"status":{"state":"online"}}"#).unwrap();
        assert_eq!(old, Message::Presence { profile: profile("Old"), status: Status::Online, hosting: vec![] });
    }

    #[test]
    fn chat_logs_keep_order_and_count_unread() {
        let mut log = ChatLog::default();
        let message = |id: &str, from: &str, at: i64| ChatMessage { id: id.into(), from: from.into(), text: id.into(), at };
        assert!(log.add("party:p", message("b", "npub1bob", 20)));
        assert!(log.add("party:p", message("a", "npub1alice", 10)));
        assert!(!log.add("party:p", message("a", "npub1alice", 10)), "the same message twice is kept once");
        assert_eq!(log.messages("party:p").iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(log.unread("party:p", "npub1alice"), 1);
        log.mark_read("party:p", 20);
        assert_eq!(log.unread("party:p", "npub1alice"), 0);
        for n in 0..400 {
            log.add("friend:x", message(&n.to_string(), "npub1x", n));
        }
        assert_eq!(log.messages("friend:x").len(), 300);
        assert_eq!(log.messages("friend:x")[0].id, "100");
        log.forget("party:p");
        assert!(log.messages("party:p").is_empty());
        assert_eq!(clean_chat("  two\nlines\r\n "), "two\nlines");
    }

    #[test]
    fn asking_each_other_makes_friends_and_codes_parse() {
        let (alice, bob) = ("npub1alice", "npub1bob");
        let mut a = SocialData::default();
        a.request(bob, alice, 1).unwrap();
        // Bob asked at the same time: that's a yes.
        assert_eq!(
            a.apply("x", bob, alice, Message::Request { profile: profile("Bob"), note: None }, 2),
            Applied::BecameFriends { code: bob.into(), name: "Bob".into(), reply: true }
        );
        assert!(a.is_friend(bob));

        let keys = Keys::generate();
        let code = code_of(&keys.public_key());
        assert!(code.starts_with("npub1"));
        assert_eq!(parse_code(&format!("  nostr:{code} ")).unwrap(), keys.public_key());
        assert_eq!(parse_code(&keys.public_key().to_hex()).unwrap(), keys.public_key());
        assert!(matches!(parse_code("not a code"), Err(SocialError::BadCode)));
        assert_eq!(short_code(&code).chars().count(), 14);

        let dir = std::env::temp_dir().join(format!("wisp-social-{}", std::process::id()));
        let store = SocialStore::new(&dir);
        let mut data = store.load();
        assert!(data.share_activity && data.relays.len() == 3);
        let first = data.keys();
        store.save(&data).unwrap();
        assert_eq!(store.load().keys().public_key(), first.public_key());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
