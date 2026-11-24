//! Friends inside the app: the relay connection while Friends is on, applying what arrives,
//! telling friends what you're up to (every few minutes, and when a game starts or stops), and
//! the commands the Friends page calls. The rules and the wire format live in
//! wisp_core::social.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as SyncMutex};

use serde::Serialize;
use tauri::async_runtime::{JoinHandle, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use wisp_core::home::instance_label;
use wisp_core::social::{
    self, Applied, ChatLog, ChatMessage, ChatStore, ClientNotification, Friend, FriendRequest, Message, Net, Party, PartyInvite, Profile,
    Received, ServerCard, ServerInvite, SocialData, SocialStore, Status, StreamExt,
};

use crate::{Launches, Paths, now_ms};

/// Sent whenever friends change; the payload is a notice worth a toast, if there is one.
pub const SOCIAL_EVENT: &str = "social";
/// Sent when a chat message arrives; the payload is its conversation.
pub const CHAT_EVENT: &str = "chat";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    title: String,
    body: Option<String>,
}

/// A friend's latest status, the servers they host that we can join, and when they said so.
type Seen = (Status, Vec<ServerCard>, i64);

pub struct Social {
    store: SocialStore,
    data: SyncMutex<SocialData>,
    net: Mutex<Option<Arc<Net>>>,
    tasks: SyncMutex<Vec<JoinHandle<()>>>,
    presence: SyncMutex<HashMap<String, Seen>>,
    /// When each friend last got our presence (Unix ms), so hellos don't echo.
    greeted: SyncMutex<HashMap<String, i64>>,
    chat_store: ChatStore,
    chat: SyncMutex<ChatLog>,
}

impl Social {
    pub fn new(dir: std::path::PathBuf) -> Self {
        let store = SocialStore::new(&dir);
        let data = store.load();
        let chat_store = ChatStore::new(&dir);
        let chat = chat_store.load();
        Self {
            store,
            data: SyncMutex::new(data),
            net: Mutex::new(None),
            tasks: SyncMutex::new(Vec::new()),
            presence: SyncMutex::new(HashMap::new()),
            greeted: SyncMutex::new(HashMap::new()),
            chat_store,
            chat: SyncMutex::new(chat),
        }
    }

    /// Changes the chat log and saves it.
    fn chat<T>(&self, change: impl FnOnce(&mut ChatLog) -> T) -> T {
        let mut log = self.chat.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let result = change(&mut log);
        let _ = self.chat_store.save(&log);
        result
    }

    fn data(&self) -> std::sync::MutexGuard<'_, SocialData> {
        self.data.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Changes the data and saves it.
    fn change<T>(&self, change: impl FnOnce(&mut SocialData) -> T) -> T {
        let mut data = self.data();
        let result = change(&mut data);
        let _ = self.store.save(&data);
        result
    }

    fn my_code(&self) -> String {
        self.change(|data| social::code_of(&data.keys().public_key()))
    }
}

fn notify(app: &AppHandle, notice: Option<Notice>) {
    let _ = app.emit(SOCIAL_EVENT, notice);
}

fn notice(title: impl Into<String>, body: Option<String>) -> Option<Notice> {
    Some(Notice { title: title.into(), body })
}

/// The name friends see: the one set on the Friends page, else the Minecraft account's.
fn profile(app: &AppHandle, data: &SocialData) -> Profile {
    let mut profile = data.profile.clone();
    if profile.name.trim().is_empty() {
        profile.name = app
            .try_state::<Paths>()
            .and_then(|paths| paths.accounts().active())
            .map_or_else(|| "Wisp player".to_string(), |account| account.name);
    }
    profile
}

/// A friend's name, or a short form of their code.
pub(crate) fn friend_name(app: &AppHandle, code: &str) -> String {
    app.state::<Social>().data().friends.iter().find(|friend| friend.code == code).map_or_else(|| social::short_code(code), |friend| friend.profile.name.clone())
}

async fn net(app: &AppHandle) -> Option<Arc<Net>> {
    app.state::<Social>().net.lock().await.clone()
}

pub(crate) async fn send(app: &AppHandle, code: &str, message: &Message) -> Result<(), String> {
    let net = net(app).await.ok_or("Friends is off.")?;
    let to = social::parse_code(code).map_err(|error| error.to_string())?;
    net.send(&to, message).await.map_err(|error| format!("Couldn't reach the relays: {error}"))
}

/// What we're doing right now, for presence.
fn my_status(app: &AppHandle, share: bool) -> Status {
    let Some(launches) = app.try_state::<Launches>() else {
        return Status::Online;
    };
    let running = launches
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .find_map(|(id, slot)| match slot.last {
            wisp_core::launch::LaunchUpdate::Running { started_at } => Some((id.clone(), slot.world.clone(), started_at)),
            _ => None,
        });
    let Some((id, world, since)) = running else {
        return Status::Online;
    };
    if !share {
        return Status::Playing { label: None, world: None, since };
    }
    let paths = app.try_state::<Paths>();
    let instance = paths.as_ref().and_then(|paths| paths.instances().get(&id));
    let world_name = match (&world, &paths) {
        (Some(folder), Some(paths)) => paths
            .instances()
            .game_dir(&id)
            .and_then(|game| wisp_core::worlds::world_dir(&game, folder))
            .and_then(|dir| wisp_core::worlds::read_info(&dir))
            .map(|info| info.name),
        _ => None,
    };
    Status::Playing { label: instance.as_ref().map(instance_label), world: world_name, since }
}

/// Tells every friend what we're up to, and which of our servers they can join.
pub async fn announce(app: &AppHandle) {
    announce_with(app, false).await;
}

/// `hello` asks friends to answer with theirs at once (we've just come online).
async fn announce_with(app: &AppHandle, hello: bool) {
    let Some(net) = net(app).await else {
        return;
    };
    let (friends, profile, status) = {
        let social = app.state::<Social>();
        let data = social.data();
        let status = my_status(app, data.share_activity);
        let friends: Vec<String> = data.friends.iter().map(|friend| friend.code.clone()).collect();
        (friends, profile(app, &data), status)
    };
    for code in friends {
        let hosting = crate::hosting::cards_for(app, &code);
        if let Ok(to) = social::parse_code(&code) {
            let _ = net.send(&to, &Message::Presence { profile: profile.clone(), status: status.clone(), hosting, hello }).await;
            app.state::<Social>().greeted.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(code, now_ms());
        }
    }
}

/// Answers a friend who's just come online with our presence, so each sees the other at once.
/// Not if they had it in the last minute (their hello and our heartbeat can cross).
fn greet(app: &AppHandle, code: &str) {
    let now = now_ms();
    {
        let social = app.state::<Social>();
        let mut greeted = social.greeted.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if greeted.get(code).is_some_and(|at| now - at < 60_000) {
            return;
        }
        greeted.insert(code.to_string(), now);
    }
    let (app, code) = (app.clone(), code.to_string());
    tauri::async_runtime::spawn(async move {
        let (profile, status) = {
            let social = app.state::<Social>();
            let data = social.data();
            (profile(&app, &data), my_status(&app, data.share_activity))
        };
        let hosting = crate::hosting::cards_for(&app, &code);
        let _ = send(&app, &code, &Message::Presence { profile, status, hosting, hello: false }).await;
    });
}

/// The face of the skin worn now (with any worn cosmetic), as friends see it.
fn worn_face(app: &AppHandle) -> Option<String> {
    use base64::Engine;
    let paths = app.try_state::<Paths>()?;
    let worn = crate::worn_skin(&paths.data)?;
    let face = wisp_core::skins::face_png(&worn.png)?;
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(face)))
}

/// Puts the worn skin's face on our profile; true if it changed.
fn refresh_face(app: &AppHandle) -> bool {
    let Some(face) = worn_face(app) else {
        return false;
    };
    let social = app.state::<Social>();
    social.change(|data| {
        if data.profile.face.as_deref() == Some(face.as_str()) {
            return false;
        }
        data.profile.face = Some(face);
        true
    })
}

/// After the worn skin or cosmetic changes: friends see the new face straight away.
pub fn skin_changed(app: &AppHandle) {
    if !app.state::<Social>().data().enabled {
        return;
    }
    if refresh_face(app) {
        notify(app, None);
        announce_soon(app);
    }
}

/// Presence soon, without holding up whoever asked (a game starting, say).
pub fn announce_soon(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { announce(&app).await });
}

/// Sends the party's member list to everyone in it (we lead it).
async fn share_party(app: &AppHandle) {
    let (party, me) = {
        let social = app.state::<Social>();
        let party = social.data().party.clone();
        (party, social.my_code())
    };
    let Some(party) = party else {
        return;
    };
    for member in party.members.iter().filter(|member| **member != me) {
        let _ = send(app, member, &Message::PartyUpdate { party: party.clone() }).await;
    }
}

async fn handle(app: &AppHandle, received: Received) {
    let social = app.state::<Social>();
    let from = social::code_of(&received.from);
    let now = now_ms();
    // A message's own time, for presence; never in the future.
    let at = received.at.min(now);
    // Asking to join and answering only count while someone's waiting on them.
    if matches!(received.message, Message::JoinRequest { .. } | Message::JoinAnswer { .. }) && now - at > social::JOIN_FRESH_MS {
        return;
    }
    let hosting = match &received.message {
        Message::Presence { hosting, .. } => hosting.clone(),
        _ => Vec::new(),
    };
    let message_id = received.id.clone();
    let is_presence = matches!(received.message, Message::Presence { .. });
    let applied = {
        let me = social.my_code();
        social.change(|data| data.apply(&received.id, &from, &me, received.message, if is_presence { at } else { now }))
    };
    let name_of = |code: &str| {
        social
            .data()
            .friends
            .iter()
            .find(|friend| friend.code == code)
            .map_or_else(|| social::short_code(code), |friend| friend.profile.name.clone())
    };
    let shown = match applied {
        Applied::Nothing => return,
        Applied::NewRequest { name, .. } => notice(format!("{name} wants to be friends"), Some("Accept or decline on the Friends page.".into())),
        Applied::BecameFriends { code, name, reply } => {
            if reply {
                let mine = profile(app, &social.data());
                let _ = send(app, &code, &Message::Accept { profile: mine }).await;
                announce_soon(app);
            }
            notice(format!("You and {name} are friends"), None)
        }
        Applied::Accepted { name, .. } => {
            announce_soon(app);
            notice(format!("{name} accepted your request"), Some("You'll see what they're playing on the Friends page.".into()))
        }
        Applied::Removed { code } => {
            social.presence.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&code);
            None
        }
        Applied::Presence { code, status, hello } => {
            // Old news (a message that sat on a relay) still updates "last seen", not the status.
            if now - at < social::PRESENCE_TTL_MS {
                social.presence.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(code.clone(), (status, hosting, at));
                if hello {
                    greet(app, &code);
                }
            }
            None
        }
        Applied::Invited { from, party } => notice(format!("{} invited you to {party}", name_of(&from)), Some("Join it on the Friends page.".into())),
        Applied::PartyChanged => {
            share_party(app).await;
            None
        }
        Applied::PartyUpdated => None,
        Applied::PartyEnded => notice("The party ended", None),
        Applied::ServerInvited { from, server } => notice(format!("{} invited you to {server}", name_of(&from)), Some("Join it from the Friends page.".into())),
        Applied::JoinRequested { from, server_id, session, offer, player } => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move { crate::hosting::answer_join(&app, from, server_id, session, offer, player).await });
            None
        }
        Applied::JoinAnswered { session, offer, server, error, starting, .. } => {
            crate::hosting::answered(app, &session, crate::hosting::Answer { offer, server, error, starting });
            None
        }
        Applied::Chat { from, conversation, text } => {
            let shown = format!("{}: {}", name_of(&from), text.lines().next().unwrap_or_default());
            let added = social.chat(|log| log.add(&conversation, ChatMessage { id: message_id, from, text, at }));
            if !added {
                return;
            }
            let _ = app.emit(CHAT_EVENT, conversation);
            notice(shown.chars().take(120).collect::<String>(), None)
        }
    };
    notify(app, shown);
}

/// Connects and starts listening. Safe to call when already on.
pub async fn start(app: &AppHandle) -> Result<(), String> {
    let social = app.state::<Social>();
    if social.net.lock().await.is_some() {
        return Ok(());
    }
    // The skin may have changed while Friends was off.
    refresh_face(app);
    let (keys, relays) = social.change(|data| (data.keys(), data.relays.clone()));
    let (net, mut events) = Net::connect(keys.clone(), &relays).await.map_err(|error| error.to_string())?;
    let net = Arc::new(net);
    *social.net.lock().await = Some(net);

    let listener = {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(notification) = events.next().await {
                match notification {
                    ClientNotification::Event { event, .. } => {
                        if let Some(received) = social::unwrap(&keys, &event) {
                            handle(&app, received).await;
                        }
                    }
                    ClientNotification::Shutdown => break,
                    _ => {}
                }
            }
        })
    };
    let heartbeat = {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            // Give the relays a moment to connect before the first hello.
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            // The first says hello: friends answer at once, so who's on shows straight away.
            let mut hello = true;
            loop {
                announce_with(&app, hello).await;
                hello = false;
                notify(&app, None);
                tokio::time::sleep(social::PRESENCE_EVERY).await;
            }
        })
    };
    social.tasks.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).extend([listener, heartbeat]);
    Ok(())
}

/// Reconnects at startup if Friends was on when Wisp last closed.
pub fn start_if_enabled(app: &AppHandle) {
    if !app.state::<Social>().data().enabled {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if start(&app).await.is_ok() {
            notify(&app, None);
        }
    });
}

/// Says goodbye (friends see you offline) and disconnects.
pub async fn stop(app: &AppHandle) {
    let social = app.state::<Social>();
    let Some(net) = social.net.lock().await.take() else {
        return;
    };
    let (friends, profile) = {
        let data = social.data();
        (data.friends.iter().map(|friend| friend.code.clone()).collect::<Vec<_>>(), profile(app, &data))
    };
    for code in friends {
        if let Ok(to) = social::parse_code(&code) {
            let _ = net.send(&to, &Message::Presence { profile: profile.clone(), status: Status::Offline, hosting: Vec::new(), hello: false }).await;
        }
    }
    for task in social.tasks.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).drain(..) {
        task.abort();
    }
    net.shutdown().await;
    social.presence.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clear();
}

// ── What the Friends page shows ──

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendView {
    #[serde(flatten)]
    friend: Friend,
    status: Status,
    /// Their servers we can join right now.
    hosting: Vec<ServerCard>,
    /// Chat messages from them not read yet.
    unread: usize,
}

/// A conversation as the list shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSummary {
    conversation: String,
    unread: usize,
    last: Option<ChatMessage>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberView {
    code: String,
    name: String,
    face: Option<String>,
    status: Status,
    me: bool,
    leader: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartyView {
    #[serde(flatten)]
    party: Party,
    members_shown: Vec<MemberView>,
    leading: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SocialView {
    enabled: bool,
    /// Relays connected right now (0 while connecting).
    relays_connected: usize,
    relays: Vec<String>,
    code: String,
    profile: Profile,
    share_activity: bool,
    friends: Vec<FriendView>,
    incoming: Vec<FriendRequest>,
    outgoing: Vec<FriendRequest>,
    party: Option<PartyView>,
    invites: Vec<PartyInvite>,
    server_invites: Vec<ServerInvite>,
    /// The party's chat, when there's a party.
    party_chat: Option<ChatSummary>,
}

fn status_of(social: &Social, code: &str, now: i64) -> Status {
    match social.presence.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(code) {
        Some((status, _, at)) if now - at < social::PRESENCE_TTL_MS => status.clone(),
        _ => Status::Offline,
    }
}

fn hosting_of(social: &Social, code: &str, now: i64) -> Vec<ServerCard> {
    match social.presence.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(code) {
        Some((_, hosting, at)) if now - at < social::PRESENCE_TTL_MS => hosting.clone(),
        _ => Vec::new(),
    }
}

fn summary(social: &Social, conversation: &str, me: &str) -> ChatSummary {
    let log = social.chat.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    ChatSummary { conversation: conversation.to_string(), unread: log.unread(conversation, me), last: log.messages(conversation).last().cloned() }
}

async fn view(app: &AppHandle) -> SocialView {
    let social = app.state::<Social>();
    let connected = match net(app).await {
        Some(net) => net.connected().await,
        None => 0,
    };
    let me = social.my_code();
    let now = now_ms();
    let data = social.data().clone();
    let mine = profile(app, &data);
    let my_status = my_status(app, data.share_activity);
    let mut friends: Vec<FriendView> = data
        .friends
        .iter()
        .map(|friend| FriendView {
            status: status_of(&social, &friend.code, now),
            hosting: hosting_of(&social, &friend.code, now),
            unread: summary(&social, &format!("friend:{}", friend.code), &me).unread,
            friend: friend.clone(),
        })
        .collect();
    // Hosting first, then playing, then online, then the rest by when they were last around.
    friends.sort_by_key(|view| {
        let rank = match view.status {
            _ if !view.hosting.is_empty() => 0,
            Status::Playing { .. } => 1,
            Status::Online => 2,
            Status::Offline => 3,
        };
        (rank, std::cmp::Reverse(view.friend.last_seen.unwrap_or(0)))
    });
    let party = data.party.clone().map(|party| {
        let members_shown = party
            .members
            .iter()
            .map(|code| {
                if *code == me {
                    return MemberView { code: code.clone(), name: mine.name.clone(), face: mine.face.clone(), status: my_status.clone(), me: true, leader: party.leader == me };
                }
                let known = data.friends.iter().find(|friend| friend.code == *code);
                MemberView {
                    code: code.clone(),
                    name: known.map_or_else(|| social::short_code(code), |friend| friend.profile.name.clone()),
                    face: known.and_then(|friend| friend.profile.face.clone()),
                    status: status_of(&social, code, now),
                    me: false,
                    leader: party.leader == *code,
                }
            })
            .collect();
        PartyView { leading: party.leader == me, members_shown, party }
    });
    SocialView {
        enabled: data.enabled,
        relays_connected: connected,
        relays: data.relays.clone(),
        code: me.clone(),
        profile: mine,
        share_activity: data.share_activity,
        friends,
        incoming: data.incoming.clone(),
        outgoing: data.outgoing.clone(),
        party_chat: party.as_ref().map(|party| summary(&social, &format!("party:{}", party.party.id), &me)),
        party,
        invites: data.invites.clone(),
        server_invites: data.server_invites.clone(),
    }
}

// ── Commands ──

#[tauri::command]
pub async fn get_social(app: AppHandle) -> Result<SocialView, String> {
    Ok(view(&app).await)
}

#[tauri::command]
pub async fn set_social_enabled(enabled: bool, app: AppHandle, social: State<'_, Social>) -> Result<SocialView, String> {
    social.change(|data| data.enabled = enabled);
    if enabled {
        if let Err(error) = start(&app).await {
            social.change(|data| data.enabled = false);
            return Err(error);
        }
    } else {
        stop(&app).await;
    }
    Ok(view(&app).await)
}

/// The name (and skin face) friends see.
#[tauri::command]
pub async fn set_social_profile(name: String, face: Option<String>, app: AppHandle, social: State<'_, Social>) -> Result<SocialView, String> {
    social.change(|data| {
        data.profile.name = name.trim().chars().filter(|c| !c.is_control()).take(32).collect();
        data.profile.face = face.filter(|face| face.starts_with("data:image/png;base64,") && face.len() < 4096);
    });
    announce_soon(&app);
    Ok(view(&app).await)
}

#[tauri::command]
pub async fn set_share_activity(share: bool, app: AppHandle, social: State<'_, Social>) -> Result<SocialView, String> {
    social.change(|data| data.share_activity = share);
    announce_soon(&app);
    Ok(view(&app).await)
}

#[tauri::command]
pub async fn add_friend(code: String, note: Option<String>, app: AppHandle, social: State<'_, Social>) -> Result<SocialView, String> {
    let key = social::parse_code(&code).map_err(|error| error.to_string())?;
    let code = social::code_of(&key);
    let me = social.my_code();
    social.change(|data| data.request(&code, &me, now_ms())).map_err(|error| error.to_string())?;
    let mine = profile(&app, &social.data());
    let note = note.map(|note| note.trim().chars().take(200).collect::<String>()).filter(|note| !note.is_empty());
    send(&app, &code, &Message::Request { profile: mine, note }).await?;
    Ok(view(&app).await)
}

#[tauri::command]
pub async fn accept_friend(code: String, app: AppHandle, social: State<'_, Social>) -> Result<SocialView, String> {
    social.change(|data| data.accept(&code, now_ms())).ok_or("That request is gone.")?;
    let mine = profile(&app, &social.data());
    send(&app, &code, &Message::Accept { profile: mine }).await?;
    announce_soon(&app);
    Ok(view(&app).await)
}

#[tauri::command]
pub async fn decline_friend(code: String, app: AppHandle, social: State<'_, Social>) -> Result<SocialView, String> {
    social.change(|data| data.decline(&code));
    Ok(view(&app).await)
}

/// Removes a friend, or takes back a request we sent.
#[tauri::command]
pub async fn remove_friend(code: String, app: AppHandle, social: State<'_, Social>) -> Result<SocialView, String> {
    social.change(|data| data.remove(&code));
    social.presence.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&code);
    social.chat(|log| log.forget(&format!("friend:{code}")));
    let _ = send(&app, &code, &Message::Remove).await;
    Ok(view(&app).await)
}

#[tauri::command]
pub async fn create_party(name: String, app: AppHandle, social: State<'_, Social>) -> Result<SocialView, String> {
    let me = social.my_code();
    social.change(|data| data.create_party(&name, &me, now_ms()));
    Ok(view(&app).await)
}

#[tauri::command]
pub async fn invite_to_party(code: String, app: AppHandle, social: State<'_, Social>) -> Result<SocialView, String> {
    let me = social.my_code();
    let party = social.data().party.clone().ok_or("Start a party first.")?;
    if party.leader != me {
        return Err("Only the party's leader can invite people.".into());
    }
    send(&app, &code, &Message::PartyInvite { party }).await?;
    Ok(view(&app).await)
}

#[tauri::command]
pub async fn join_party(party_id: String, app: AppHandle, social: State<'_, Social>) -> Result<SocialView, String> {
    let me = social.my_code();
    // Joining one party leaves the one we're in.
    if social.data().party.is_some() {
        leave(&app).await;
    }
    let party = social.change(|data| data.join(&party_id, &me)).ok_or("That invite is gone.")?;
    send(&app, &party.leader, &Message::PartyJoin { party_id }).await?;
    Ok(view(&app).await)
}

#[tauri::command]
pub async fn decline_party(party_id: String, app: AppHandle, social: State<'_, Social>) -> Result<SocialView, String> {
    social.change(|data| data.invites.retain(|invite| invite.party.id != party_id));
    Ok(view(&app).await)
}

async fn leave(app: &AppHandle) {
    let social = app.state::<Social>();
    let me = social.my_code();
    let Some(party) = social.change(|data| data.party.take()) else {
        return;
    };
    if party.leader == me {
        // The leader leaving ends it for everyone.
        let ended = Party { members: Vec::new(), ..party.clone() };
        for member in party.members.iter().filter(|member| **member != me) {
            let _ = send(app, member, &Message::PartyUpdate { party: ended.clone() }).await;
        }
    } else {
        let _ = send(app, &party.leader, &Message::PartyLeave { party_id: party.id.clone() }).await;
    }
}

#[tauri::command]
pub async fn leave_party(app: AppHandle) -> Result<SocialView, String> {
    leave(&app).await;
    Ok(view(&app).await)
}

#[tauri::command]
pub async fn decline_server_invite(from: String, server_id: String, app: AppHandle, social: State<'_, Social>) -> Result<SocialView, String> {
    social.change(|data| data.server_invites.retain(|invite| !(invite.from == from && invite.server.id == server_id)));
    Ok(view(&app).await)
}

/// Who a conversation goes to: a friend, or everyone else in our party.
fn recipients(social: &Social, conversation: &str, me: &str) -> Result<(Vec<String>, Option<String>), String> {
    if let Some(code) = conversation.strip_prefix("friend:") {
        if !social.data().is_friend(code) {
            return Err("You aren't friends with them anymore.".into());
        }
        return Ok((vec![code.to_string()], None));
    }
    if let Some(party_id) = conversation.strip_prefix("party:") {
        let party = social.data().party.clone().filter(|party| party.id == party_id).ok_or("That party is over.")?;
        return Ok((party.members.into_iter().filter(|member| member != me).collect(), Some(party.id)));
    }
    Err("There's no such conversation.".into())
}

/// A conversation's messages, oldest first. Reading it marks it read.
#[tauri::command]
pub async fn get_chat(conversation: String, social: State<'_, Social>) -> Result<Vec<ChatMessage>, String> {
    Ok(social.chat(|log| {
        log.mark_read(&conversation, now_ms());
        log.messages(&conversation).to_vec()
    }))
}

#[tauri::command]
pub async fn send_chat(conversation: String, text: String, app: AppHandle, social: State<'_, Social>) -> Result<Vec<ChatMessage>, String> {
    let text = social::clean_chat(&text);
    if text.is_empty() {
        return Err("Write something first.".into());
    }
    let me = social.my_code();
    let (to, party_id) = recipients(&social, &conversation, &me)?;
    let message = Message::Chat { party_id, text: text.clone() };
    let mut delivered = 0;
    let mut failure = None;
    for code in &to {
        match send(&app, code, &message).await {
            Ok(()) => delivered += 1,
            Err(error) => failure = Some(error),
        }
    }
    if delivered == 0 && let Some(error) = failure {
        return Err(error);
    }
    let now = now_ms();
    Ok(social.chat(|log| {
        log.add(&conversation, ChatMessage { id: wisp_core::journal::new_id(now), from: me, text, at: now });
        log.mark_read(&conversation, now);
        log.messages(&conversation).to_vec()
    }))
}
