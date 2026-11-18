//! Running sync: for every world and instance, working out what should happen and doing it,
//! then describing where everything stands for the sync page and the badges.
//!
//! It adapts to what it finds rather than following a fixed script:
//! - before a game starts, only that instance and its worlds are synced, newest copy first;
//! - after it closes, what it changed goes up straight away;
//! - worlds being played on another computer are left alone until it's done with them;
//! - copies that turn out identical are just recorded, never sent again;
//! - files a cloud app hasn't finished bringing are waited for, not half-read;
//! - big downloads (a new world or instance over a gigabyte) wait to be asked for.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::folders::{FolderKind, kind_of};
use super::instances::{self, InstanceManifest};
use super::plan::{Local, Plan, plan};
use super::remote::{Commit, DeviceRecord, ItemKind, Playing, Remote, RemoteError, heads};
use super::state::{FolderChoice, State, Store, Synced, random_id};
use super::worlds::{self, World, Who, WorldManifest, content_hash, world_key};
use super::{Fetch, SyncError};
use crate::device::DeviceInfo;
use crate::instances::{GAME_DIR, Instance, InstanceStore};
use crate::settings::SyncPrefs;
use crate::snapshots::Snapshots;

const STATUS: &str = "status.json";
/// New worlds and instances bigger than this wait to be asked for.
const BIG: u64 = 1024 * 1024 * 1024;
/// A computer that checked in this recently while playing is still playing.
const PLAYING_FRESH_MS: i64 = 3 * 60 * 1000;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// What sync uses on this computer.
pub struct Context<'a> {
    /// Wisp's data folder.
    pub data: &'a Path,
    pub instances: &'a InstanceStore,
    pub snapshots: &'a Snapshots,
    pub prefs: &'a SyncPrefs,
    pub device: &'a DeviceInfo,
    pub fetch: &'a dyn Fetch,
    /// Told what's moving as it moves (None when nothing is).
    pub activity: &'a dyn Fn(Option<Activity>),
    pub now: fn() -> i64,
}

/// What to sync this time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope<'a> {
    Everything,
    /// One instance (by its id here) and its worlds: before it starts and after it closes.
    Instance(&'a str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    /// Sync isn't set up on this computer.
    #[default]
    Off,
    Idle,
    Working,
    /// A cloud app is still bringing files; sync tries again shortly.
    Waiting,
    /// The folder isn't there (a drive unplugged, a cloud app signed out).
    Unreachable,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    Up,
    Down,
}

/// Something moving right now.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub key: String,
    pub name: String,
    pub direction: Direction,
    pub done: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSeen {
    pub id: String,
    pub name: String,
    pub os: String,
    pub seen: i64,
    /// Names of the instances it's playing right now.
    pub playing: Vec<String>,
    pub this: bool,
}

/// One version to choose from when a world changed in two places.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub commit: String,
    pub device: String,
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum ItemState {
    /// The same here and in the cloud, as of `at` (made by `device`).
    Synced { at: i64, device: String },
    /// Waiting to go up: it's being played here, or this kind of thing isn't synced.
    Pending,
    /// Newer in the cloud; comes down when it can (always before it's played).
    CloudNewer { at: i64, device: String },
    /// Changed here and on another computer. Worlds are never merged: pick one, or keep both.
    Conflict { choices: Vec<Choice>, here_at: Option<i64> },
    /// Being played on another computer right now.
    InUse { device: String },
    /// A cloud app is still bringing some of its files.
    Waiting { files: u32 },
    /// In the cloud but not on this computer.
    CloudOnly { at: i64, device: String, bytes: u64, needs_instance: bool },
    /// Taken out of the cloud (by `device`); this computer's copy stays.
    Removed { device: String },
    Failed { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemStatus {
    pub key: String,
    pub kind: ItemKind,
    pub name: String,
    /// The instance on this computer (its id here), when there is one.
    pub instance_id: Option<String>,
    /// For a world, its folder.
    pub world: Option<String>,
    pub state: ItemState,
}

/// Everything the sync page and badges show.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub phase: Phase,
    pub folder: Option<FolderChoice>,
    /// Plain words about the phase, when there's something to say.
    pub message: Option<String>,
    pub last_synced: Option<i64>,
    pub devices: Vec<DeviceSeen>,
    pub items: Vec<ItemStatus>,
    /// How much space the sync folder takes.
    pub usage: Option<u64>,
}

/// What stands in the way of starting an instance.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchCheck {
    /// Worlds changed here and elsewhere; the player chooses before playing them.
    pub conflicts: Vec<ItemStatus>,
    /// Another computer is playing this instance right now.
    pub in_use: Option<String>,
    /// Some files are still arriving: the cloud's newest copy isn't all here yet.
    pub waiting: u32,
}

/// How to settle a world that changed in two places.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "keep", rename_all = "kebab-case")]
pub enum Resolution {
    /// This computer's copy becomes the newest everywhere.
    Here,
    /// The copy from `commit` replaces this one (kept as a restore point).
    Cloud { commit: String },
    /// This copy becomes a world of its own beside the cloud's.
    Both { commit: String },
}

fn save_overview(store: &Store, overview: &Overview) {
    let path = store.dir().join(STATUS);
    let _ = std::fs::create_dir_all(store.dir());
    let temp = path.with_extension("json.tmp");
    if std::fs::write(&temp, serde_json::to_vec(overview).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(temp, path);
    }
}

/// Where things stood after the last sync (cheap: nothing is checked).
pub fn overview(data: &Path) -> Overview {
    let store = Store::new(data);
    let state = store.load();
    if state.folder.is_none() {
        return Overview::default();
    }
    let mut overview: Overview = std::fs::read(store.dir().join(STATUS)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
    overview.folder = state.folder.clone();
    overview.last_synced = state.last_synced;
    if overview.phase == Phase::Off {
        overview.phase = Phase::Idle;
    }
    overview
}

/// Turns sync on with `path` (a Wisp folder, new or one another computer made), then syncs.
pub fn connect(ctx: &Context, path: &Path) -> Result<Overview, SyncError> {
    let store = Store::new(ctx.data);
    let mut state = store.load();
    let parent = path.parent().unwrap_or(path);
    let (kind, label) = if path.file_name().is_some_and(|name| name == super::FOLDER_NAME) { kind_of(parent) } else { kind_of(path) };
    let remote = Remote::new(path, &state.device_id);
    remote.set_up((ctx.now)())?;
    // A different folder than before starts afresh: what was synced there means nothing here.
    if state.folder.as_ref().is_none_or(|folder| folder.path != path) {
        state.items.clear();
        state.declined.clear();
        for entry in std::fs::read_dir(store.dir().join("bases")).into_iter().flatten().flatten() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    state.folder = Some(FolderChoice { path: path.to_path_buf(), kind, label, since: (ctx.now)() });
    store.save(&state)?;
    run(ctx, Scope::Everything, &HashSet::new(), &HashSet::new())
}

/// Turns sync off here. Nothing is deleted: the sync folder and every world stay as they are.
pub fn disconnect(ctx: &Context) -> Result<Overview, SyncError> {
    let store = Store::new(ctx.data);
    let mut state = store.load();
    if let Some(folder) = &state.folder {
        // Tell the other computers this one has stopped playing anything.
        let remote = Remote::new(&folder.path, &state.device_id);
        if remote.reachable() {
            let _ = remote.write_device(&DeviceRecord { id: state.device_id.clone(), name: ctx.device.name.clone(), os: ctx.device.os.clone(), seen: (ctx.now)(), playing: Vec::new() });
        }
    }
    state.folder = None;
    store.save(&state)?;
    let overview = Overview::default();
    save_overview(&store, &overview);
    Ok(overview)
}

fn instance_key(sync_id: &str) -> String {
    format!("i-{sync_id}")
}

/// Writes this computer's record: that it's around, and which instances it's playing.
pub fn check_in(ctx: &Context, playing: &[String]) -> Result<(), SyncError> {
    let store = Store::new(ctx.data);
    let state = store.load();
    let Some(folder) = &state.folder else { return Ok(()) };
    let remote = Remote::new(&folder.path, &state.device_id);
    if !remote.reachable() {
        return Ok(());
    }
    write_record(ctx, &remote, &state, playing)
}

fn write_record(ctx: &Context, remote: &Remote, state: &State, playing: &[String]) -> Result<(), SyncError> {
    let now = (ctx.now)();
    let previous: HashMap<String, i64> = remote
        .devices()
        .into_iter()
        .find(|record| record.id == state.device_id)
        .map(|record| record.playing.into_iter().map(|play| (play.item, play.since)).collect())
        .unwrap_or_default();
    let playing = playing
        .iter()
        .filter_map(|id| ctx.instances.get(id))
        .filter_map(|instance| {
            let key = instance_key(instance.sync_id.as_deref()?);
            let since = previous.get(&key).copied().unwrap_or(now);
            Some(Playing { item: key, name: instance.name, since })
        })
        .collect();
    remote.write_device(&DeviceRecord { id: state.device_id.clone(), name: ctx.device.name.clone(), os: ctx.device.os.clone(), seen: now, playing })?;
    Ok(())
}

/// A world on this computer.
struct LocalWorld {
    instance: Instance,
    folder: String,
    dir: PathBuf,
    name: String,
}

struct Run<'a, 'c> {
    ctx: &'a Context<'c>,
    store: Store,
    state: State,
    remote: Remote,
    now: i64,
    busy: &'a HashSet<String>,
    /// Items the player asked for, even if big or once declined.
    asked: &'a HashSet<String>,
    /// Instances being played on other computers: item key to computer name.
    elsewhere: HashMap<String, String>,
    waiting: u32,
}

impl Run<'_, '_> {
    fn who(&self) -> Who<'_> {
        Who { device: &self.state.device_id, device_name: &self.ctx.device.name, now: self.now }
    }

    fn record(&mut self, key: &str, commit: &Commit) {
        self.state.items.insert(key.to_string(), Synced { base: commit.id.clone(), content: commit.content.clone(), at: commit.at, device: commit.device_name.clone() });
        let _ = self.store.save(&self.state);
    }

    fn synced_state(&self, key: &str) -> ItemState {
        match self.state.items.get(key) {
            Some(synced) => ItemState::Synced { at: synced.at, device: synced.device.clone() },
            None => ItemState::Pending,
        }
    }

    // ── Instances ──

    /// Gives every instance here a sync id: the id of the same instance in the cloud when one
    /// clearly is (same name, version and loader, not claimed by another instance here),
    /// otherwise a new one.
    fn link_instances(&mut self) {
        let locals = self.ctx.instances.list();
        let claimed: HashSet<String> = locals.iter().filter_map(|instance| instance.sync_id.clone()).collect();
        let mut cloud: Vec<(String, InstanceManifest)> = Vec::new();
        if locals.iter().any(|instance| instance.sync_id.is_none()) {
            for key in self.remote.items().into_iter().filter(|key| key.starts_with("i-")) {
                let sync_id = key.trim_start_matches("i-").to_string();
                if claimed.contains(&sync_id) {
                    continue;
                }
                if let Some(head) = heads(&self.remote.commits(&key)).into_iter().find(|commit| !commit.removed)
                    && let Ok(manifest) = self.remote.get_json::<InstanceManifest>(&head.manifest)
                {
                    cloud.push((sync_id, manifest));
                }
            }
        }
        for instance in locals.into_iter().filter(|instance| instance.sync_id.is_none()) {
            let matched = cloud.iter().position(|(_, manifest)| instances::looks_like(&instance, manifest));
            let sync_id = match matched {
                Some(index) => cloud.remove(index).0,
                None => random_id(),
            };
            let _ = self.ctx.instances.update(&instance.id, |instance| instance.sync_id = Some(sync_id));
        }
    }

    /// The cloud's current version of an instance: its newest commit, or several branches merged.
    fn cloud_instance(&self, commits: &[Commit], base: &InstanceManifest) -> Result<Option<(Vec<Commit>, InstanceManifest)>, SyncError> {
        let live: Vec<Commit> = heads(commits).into_iter().filter(|commit| !commit.removed).cloned().collect();
        let Some(first) = live.first() else { return Ok(None) };
        let mut merged: InstanceManifest = self.remote.get_json(&first.manifest)?;
        for other in &live[1..] {
            let theirs: InstanceManifest = self.remote.get_json(&other.manifest)?;
            merged = instances::merge(base, &merged, &theirs, false);
        }
        Ok(Some((live, merged)))
    }

    fn sync_instance(&mut self, key: &str, local: Option<&Instance>) -> ItemStatus {
        let commits = self.remote.commits(key);
        let sync_id = key.trim_start_matches("i-").to_string();
        let latest_name = commits.last().map(|commit| commit.name.clone());
        let name = local.map(|instance| instance.name.clone()).or(latest_name).unwrap_or_else(|| "Instance".into());
        let mut status = ItemStatus { key: key.to_string(), kind: ItemKind::Instance, name, instance_id: local.map(|instance| instance.id.clone()), world: None, state: ItemState::Pending };
        if let Some(instance) = local
            && self.busy.contains(&instance.id)
        {
            status.state = self.synced_state(key);
            return status;
        }
        if heads(&commits).first().is_some_and(|head| head.removed) {
            status.state = ItemState::Removed { device: commits.last().map(|commit| commit.device_name.clone()).unwrap_or_default() };
            return status;
        }
        status.state = match self.instance_step(key, &sync_id, local, &commits) {
            Ok(state) => state,
            Err(SyncError::Remote(RemoteError::Missing)) => {
                self.waiting += 1;
                ItemState::Waiting { files: 1 }
            }
            Err(SyncError::Changing) => ItemState::Pending,
            Err(error) => ItemState::Failed { message: error.to_string() },
        };
        status
    }

    fn instance_step(&mut self, key: &str, sync_id: &str, local: Option<&Instance>, commits: &[Commit]) -> Result<ItemState, SyncError> {
        let had = self.state.items.get(key).cloned();
        let base: InstanceManifest = had.as_ref().and_then(|_| self.store.base_manifest(key)).unwrap_or_default();
        let cloud = self.cloud_instance(commits, &base)?;
        let Some(instance) = local else {
            // Only in the cloud: bring it here, unless it was removed here or is big.
            let Some((live, manifest)) = cloud else { return Ok(ItemState::Pending) };
            let head = &live[0];
            let asked = self.asked.contains(key);
            let declined = (had.is_some() || self.state.declined.contains(key)) && !asked;
            if declined || !self.ctx.prefs.instances || manifest.definition.is_none() || (manifest.total_bytes() > BIG && !asked) {
                return Ok(ItemState::CloudOnly { at: head.at, device: head.device_name.clone(), bytes: manifest.total_bytes(), needs_instance: false });
            }
            let created = instances::create_from(self.ctx.instances, sync_id, &manifest, self.now)?;
            self.state.declined.remove(key);
            let dir = self.ctx.instances.dir_of(&created.id).ok_or_else(|| SyncError::Other("the new instance went missing".into()))?;
            return self.instance_merge(key, sync_id, &created, &dir, Some((live, manifest)), &InstanceManifest::default(), false);
        };
        let dir = self.ctx.instances.dir_of(&instance.id).ok_or_else(|| SyncError::Other("that instance is gone".into()))?;
        self.instance_merge(key, sync_id, instance, &dir, cloud, &base, had.is_some())
    }

    #[allow(clippy::too_many_arguments)]
    fn instance_merge(&mut self, key: &str, sync_id: &str, instance: &Instance, dir: &Path, cloud: Option<(Vec<Commit>, InstanceManifest)>, base: &InstanceManifest, have_base: bool) -> Result<ItemState, SyncError> {
        let mut cache = self.store.hash_cache();
        let mut known = std::mem::take(&mut self.state.known_files);
        if let Some((_, manifest)) = &cloud {
            for (hash, url) in instances::links(manifest) {
                known.insert(hash, Some(url));
            }
        }
        let current = instances::local_manifest(instance, dir, self.ctx.prefs, &mut cache, &mut known, self.ctx.fetch);
        self.state.known_files = known;
        cache.save();
        let (current, resolved) = current?;
        let (parents, theirs) = match &cloud {
            Some((live, manifest)) => (live.iter().map(|commit| commit.id.clone()).collect::<Vec<_>>(), Some(manifest)),
            None => (Vec::new(), None),
        };
        // First meeting of two copies: the cloud's wins where they differ; after that, this
        // computer's changes are the newer ones.
        let merged = match theirs {
            Some(theirs) => instances::merge(base, &current, theirs, have_base),
            None => current.clone(),
        };
        if merged != current {
            let name = instance.name.clone();
            let progress = reporter(self.ctx, key, &name, Direction::Down);
            let applied = instances::apply(self.ctx.instances, &instance.id, &current, &merged, &self.remote, self.ctx.fetch, &progress)?;
            (self.ctx.activity)(None);
            if applied.waiting > 0 {
                self.waiting += applied.waiting;
                return Ok(ItemState::Waiting { files: applied.waiting });
            }
        }
        let needs_commit = theirs.is_none_or(|theirs| *theirs != merged) || parents.len() > 1;
        if needs_commit && !resolved {
            // Modrinth couldn't be asked which mods it has, and copying every jar into the sync
            // folder instead would waste space: send it once Modrinth answers.
            return Ok(ItemState::Pending);
        }
        let commit = if needs_commit {
            let game_dir = dir.join(GAME_DIR);
            let name = merged.name().unwrap_or(&instance.name).to_string();
            let progress = reporter(self.ctx, key, &name, Direction::Up);
            let who = self.who();
            let commit = instances::upload(&self.remote, &game_dir, key, sync_id, &merged, parents, &who, &progress)?;
            (self.ctx.activity)(None);
            commit
        } else {
            let (live, _) = cloud.as_ref().expect("no commit needed means the cloud has it");
            live[0].clone()
        };
        self.store.set_base_manifest(key, &merged)?;
        self.record(key, &commit);
        Ok(ItemState::Synced { at: commit.at, device: commit.device_name.clone() })
    }

    // ── Worlds ──

    fn sync_world(&mut self, key: &str, instance: &Instance, local: Option<&LocalWorld>) -> ItemStatus {
        let commits = self.remote.commits(key);
        let cloud_name = commits.last().map(|commit| commit.name.clone());
        let folder = local.map(|world| world.folder.clone()).or_else(|| commits.last().and_then(|commit| commit.world.clone())).unwrap_or_default();
        let name = local.map(|world| world.name.clone()).or(cloud_name).unwrap_or_else(|| folder.clone());
        let mut status = ItemStatus { key: key.to_string(), kind: ItemKind::World, name, instance_id: Some(instance.id.clone()), world: Some(folder.clone()), state: ItemState::Pending };
        if self.busy.contains(&instance.id) {
            status.state = self.synced_state(key);
            return status;
        }
        status.state = match self.world_step(key, instance, &folder, local, &commits) {
            Ok(state) => state,
            Err(SyncError::Changing) => ItemState::Pending,
            Err(error) => ItemState::Failed { message: error.to_string() },
        };
        status
    }

    fn world_step(&mut self, key: &str, instance: &Instance, folder: &str, local: Option<&LocalWorld>, commits: &[Commit]) -> Result<ItemState, SyncError> {
        let sync_id = instance.sync_id.clone().unwrap_or_default();
        let game_dir = self.ctx.instances.game_dir(&instance.id).ok_or_else(|| SyncError::Other("that instance is gone".into()))?;
        let dir = local.map(|world| world.dir.clone()).unwrap_or_else(|| game_dir.join(crate::worlds::SAVES).join(folder));
        let files = match local {
            Some(_) => Some(self.ctx.snapshots.scan(&dir, &instance.id, folder)?),
            None => None,
        };
        let content = files.as_deref().map(content_hash);
        let base = self.state.items.get(key).cloned();
        let decision = plan(commits, Local { content: content.as_deref(), base: base.as_ref().map(|synced| (synced.base.as_str(), synced.content.as_str())) });
        let by_id = |id: &str| commits.iter().find(|commit| commit.id == id).cloned();
        let elsewhere = self.elsewhere.get(&instance_key(&sync_id)).cloned();
        let name = local.map(|world| world.name.clone()).or_else(|| commits.last().map(|commit| commit.name.clone())).unwrap_or_else(|| folder.to_string());
        let world = World { key, instance_id: &instance.id, instance_sync: &sync_id, folder, dir: &dir, name: &name };

        match decision {
            Plan::InStep => Ok(self.synced_state(key)),
            Plan::Adopt { head } => {
                let commit = by_id(&head).expect("planned from these commits");
                self.record(key, &commit);
                Ok(ItemState::Synced { at: commit.at, device: commit.device_name })
            }
            Plan::Upload { parents } => {
                let who = self.who();
                let progress = reporter(self.ctx, key, &name, Direction::Up);
                let commit = worlds::upload(&self.remote, self.ctx.snapshots, &world, parents, &who, &progress)?;
                (self.ctx.activity)(None);
                self.record(key, &commit);
                Ok(ItemState::Synced { at: commit.at, device: commit.device_name })
            }
            Plan::Download { head } => {
                let commit = by_id(&head).expect("planned from these commits");
                if let Some(device) = elsewhere {
                    return Ok(ItemState::InUse { device });
                }
                self.bring(key, &world, &commit, false)
            }
            Plan::Join { heads: ids } => {
                let tips: Vec<Commit> = ids.iter().filter_map(|id| by_id(id)).collect();
                let refs: Vec<&Commit> = tips.iter().collect();
                let who = self.who();
                let joined = worlds::join(&self.remote, &refs, &tips[0], &who)?;
                if content.as_deref() == Some(joined.content.as_str()) {
                    self.record(key, &joined);
                    Ok(ItemState::Synced { at: joined.at, device: joined.device_name })
                } else {
                    // Take it from the top with the branches tied: usually it's now a plain download.
                    let commits = self.remote.commits(key);
                    self.world_step(key, instance, folder, local, &commits)
                }
            }
            Plan::Conflict { heads: ids } => {
                let choices = ids
                    .iter()
                    .filter_map(|id| by_id(id))
                    .map(|commit| Choice { commit: commit.id, device: commit.device_name, at: commit.at })
                    .collect();
                let here_at = std::fs::metadata(dir.join("level.dat")).ok().and_then(|meta| meta.modified().ok()).and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok()).map(|elapsed| elapsed.as_millis() as i64);
                Ok(ItemState::Conflict { choices, here_at })
            }
            Plan::New { head } | Plan::Gone { head } => {
                let commit = by_id(&head).expect("planned from these commits");
                let asked = self.asked.contains(key);
                let declined = (base.is_some() || self.state.declined.contains(key)) && !asked;
                if declined || !self.ctx.prefs.worlds || (commit.bytes > BIG && !asked) || elsewhere.is_some() {
                    return Ok(ItemState::CloudOnly { at: commit.at, device: commit.device_name, bytes: commit.bytes, needs_instance: false });
                }
                self.state.declined.remove(key);
                self.bring(key, &world, &commit, false)
            }
            Plan::Removed => {
                let device = heads(commits).first().map(|commit| commit.device_name.clone()).unwrap_or_default();
                Ok(ItemState::Removed { device })
            }
        }
    }

    /// Brings a commit's version of a world here.
    fn bring(&mut self, key: &str, world: &World, commit: &Commit, keep_current: bool) -> Result<ItemState, SyncError> {
        let progress = reporter(self.ctx, key, world.name, Direction::Down);
        let waiting = worlds::download(&self.remote, self.ctx.snapshots, world, commit, keep_current, self.now, &progress)?;
        (self.ctx.activity)(None);
        if let Some(files) = waiting {
            self.waiting += files;
            return Ok(ItemState::Waiting { files });
        }
        self.record(key, commit);
        Ok(ItemState::Synced { at: commit.at, device: commit.device_name.clone() })
    }

    /// Clears out history nobody needs, about once a week.
    fn tidy(&mut self) {
        if self.now - self.state.last_tidy < 7 * DAY_MS {
            return;
        }
        let remote = &self.remote;
        let freed = remote.tidy(self.now - 30 * DAY_MS, 10, |manifest, kind| match kind {
            ItemKind::World => remote.get_json::<WorldManifest>(manifest).ok().map(|manifest| manifest.files.into_iter().map(|file| file.hash).collect()),
            ItemKind::Instance => remote.get_json::<InstanceManifest>(manifest).ok().map(|manifest| instances::stored_hashes(&manifest)),
        });
        if freed.is_ok() {
            self.state.last_tidy = self.now;
        }
    }
}

/// Passes progress on as activity, and says when it's done.
fn reporter<'f>(ctx: &'f Context<'_>, key: &'f str, name: &'f str, direction: Direction) -> impl Fn(u64, u64) + 'f {
    move |done, total| {
        (ctx.activity)((total > 0 && done < total).then(|| Activity { key: key.to_string(), name: name.to_string(), direction, done, total }));
    }
}

fn local_worlds(store: &InstanceStore, instance: &Instance) -> Vec<LocalWorld> {
    let Some(game_dir) = store.game_dir(&instance.id) else { return Vec::new() };
    crate::worlds::list_info(&game_dir)
        .into_iter()
        .filter_map(|info| {
            let dir = crate::worlds::world_dir(&game_dir, &info.folder)?;
            Some(LocalWorld { instance: instance.clone(), folder: info.folder, dir, name: info.name })
        })
        .collect()
}

/// Syncs everything (or one instance and its worlds). `busy` are instances being played here:
/// they're left alone. `asked` are items the player asked to bring here.
pub fn run(ctx: &Context, scope: Scope, busy: &HashSet<String>, asked: &HashSet<String>) -> Result<Overview, SyncError> {
    let store = Store::new(ctx.data);
    let state = store.load();
    let Some(folder) = state.folder.clone() else {
        return Ok(Overview::default());
    };
    let now = (ctx.now)();
    let remote = Remote::new(&folder.path, &state.device_id);
    let previous = overview(ctx.data);
    if !remote.reachable() {
        let overview = Overview {
            phase: Phase::Unreachable,
            message: Some(match folder.kind {
                FolderKind::Folder => "Wisp can't find its sync folder. Is the drive it's on connected?".to_string(),
                _ => format!("Wisp can't find its folder in {}. Is {} running and signed in?", folder.label, folder.label),
            }),
            ..previous
        };
        save_overview(&store, &overview);
        return Ok(overview);
    }
    remote.set_up(now)?;
    let me = state.device_id.clone();
    let others: Vec<DeviceRecord> = remote.devices().into_iter().filter(|record| record.id != me).collect();
    let elsewhere: HashMap<String, String> = others
        .iter()
        .filter(|record| now - record.seen < PLAYING_FRESH_MS)
        .flat_map(|record| record.playing.iter().map(|play| (play.item.clone(), record.name.clone())))
        .collect();
    let mut run = Run { ctx, store, state, remote, now, busy, asked, elsewhere, waiting: 0 };
    let busy_list: Vec<String> = busy.iter().cloned().collect();
    write_record(ctx, &run.remote, &run.state, &busy_list)?;
    run.link_instances();

    let in_scope = |instance_id: Option<&str>| match scope {
        Scope::Everything => true,
        Scope::Instance(id) => instance_id == Some(id),
    };
    let mut statuses: Vec<ItemStatus> = Vec::new();

    // Instances first: a world needs its instance.
    let locals = ctx.instances.list();
    let cloud_keys: BTreeSet<String> = run.remote.items().into_iter().collect();
    let mut instance_keys: BTreeSet<String> = locals.iter().filter_map(|instance| instance.sync_id.as_deref().map(instance_key)).collect();
    if scope == Scope::Everything {
        instance_keys.extend(cloud_keys.iter().filter(|key| key.starts_with("i-")).cloned());
    }
    if ctx.prefs.instances || ctx.prefs.options || ctx.prefs.servers || ctx.prefs.resource_packs || ctx.prefs.screenshots {
        for key in &instance_keys {
            let local = locals.iter().find(|instance| instance.sync_id.as_deref().map(instance_key).as_deref() == Some(key.as_str()));
            if !in_scope(local.map(|instance| instance.id.as_str())) {
                continue;
            }
            statuses.push(run.sync_instance(key, local));
        }
    }

    // Then worlds, in every instance here (including ones that just arrived).
    let locals = ctx.instances.list();
    if ctx.prefs.worlds {
        for instance in locals.iter().filter(|instance| in_scope(Some(&instance.id))) {
            let Some(sync_id) = instance.sync_id.clone() else { continue };
            let here = local_worlds(ctx.instances, instance);
            let prefix = format!("w-{sync_id}-");
            let mut keys: BTreeSet<String> = here.iter().map(|world| world_key(&sync_id, &world.folder)).collect();
            keys.extend(cloud_keys.iter().filter(|key| key.starts_with(&prefix)).cloned());
            for key in keys {
                let local = here.iter().find(|world| world_key(&sync_id, &world.folder) == key);
                debug_assert!(local.is_none_or(|world| world.instance.id == instance.id));
                statuses.push(run.sync_world(&key, instance, local));
            }
        }
        // Worlds whose instance isn't on this computer can't be placed; say so.
        if scope == Scope::Everything {
            let here: HashSet<String> = locals.iter().filter_map(|instance| instance.sync_id.clone()).collect();
            for key in cloud_keys.iter().filter(|key| key.starts_with("w-")) {
                let commits = run.remote.commits(key);
                let Some(head) = heads(&commits).into_iter().find(|commit| !commit.removed).cloned() else { continue };
                if here.contains(&head.instance) {
                    continue;
                }
                statuses.push(ItemStatus {
                    key: key.clone(),
                    kind: ItemKind::World,
                    name: head.name.clone(),
                    instance_id: None,
                    world: head.world.clone(),
                    state: ItemState::CloudOnly { at: head.at, device: head.device_name.clone(), bytes: head.bytes, needs_instance: true },
                });
            }
        }
    }

    if scope == Scope::Everything {
        run.tidy();
    }
    let mut state = run.state;
    let phase = if run.waiting > 0 { Phase::Waiting } else { Phase::Idle };
    if run.waiting == 0 {
        state.last_synced = Some(now);
    }
    let _ = run.store.save(&state);

    // Items outside this run keep what they last said.
    let done: HashSet<String> = statuses.iter().map(|status| status.key.clone()).collect();
    if scope != Scope::Everything {
        statuses.extend(previous.items.into_iter().filter(|status| !done.contains(&status.key)));
    }
    let names: HashMap<String, String> = ctx.instances.list().into_iter().filter_map(|instance| Some((instance_key(instance.sync_id.as_deref()?), instance.name))).collect();
    let mut devices: Vec<DeviceSeen> = run
        .remote
        .devices()
        .into_iter()
        .map(|record| DeviceSeen {
            this: record.id == me,
            playing: if now - record.seen < PLAYING_FRESH_MS { record.playing.iter().map(|play| names.get(&play.item).cloned().unwrap_or_else(|| play.name.clone())).collect() } else { Vec::new() },
            id: record.id,
            name: record.name,
            os: record.os,
            seen: record.seen,
        })
        .collect();
    devices.sort_by_key(|device| (!device.this, std::cmp::Reverse(device.seen)));
    // Measuring the folder means walking all of it, so it's done at most hourly.
    let usage = match previous.usage {
        Some(usage) if scope != Scope::Everything => Some(usage),
        _ => Some(run.remote.usage()),
    };
    let message = (run.waiting > 0).then(|| {
        let label = state.folder.as_ref().map_or("the cloud", |folder| folder.label.as_str());
        let files = if run.waiting == 1 { "1 file".to_string() } else { format!("{} files", run.waiting) };
        format!("Waiting for {label} to bring {files} from your other computer.")
    });
    let overview = Overview { phase, folder: state.folder.clone(), message, last_synced: state.last_synced, devices, items: statuses, usage };
    save_overview(&run.store, &overview);
    Ok(overview)
}

/// Syncs an instance and its worlds just before it starts, and says what stands in the way.
pub fn before_launch(ctx: &Context, instance_id: &str, busy: &HashSet<String>) -> Result<LaunchCheck, SyncError> {
    let overview = run(ctx, Scope::Instance(instance_id), busy, &HashSet::new())?;
    let mut check = LaunchCheck::default();
    for item in overview.items.into_iter().filter(|item| item.instance_id.as_deref() == Some(instance_id)) {
        match &item.state {
            ItemState::Conflict { .. } => check.conflicts.push(item),
            ItemState::InUse { device } => check.in_use = Some(device.clone()),
            ItemState::Waiting { files } => check.waiting += files,
            _ => {}
        }
    }
    Ok(check)
}

/// Brings a world or instance that's only in the cloud here, big or not, even if it was removed
/// here before.
pub fn fetch_item(ctx: &Context, key: &str, busy: &HashSet<String>) -> Result<Overview, SyncError> {
    let store = Store::new(ctx.data);
    let mut state = store.load();
    state.declined.remove(key);
    // Removed here before: forget that, so it counts as new.
    let here = ctx.instances.list().into_iter().any(|instance| instance.sync_id.as_deref().map(instance_key).as_deref() == Some(key));
    if key.starts_with("i-") && !here {
        state.items.remove(key);
        store.forget_base_manifest(key);
    }
    store.save(&state)?;
    let asked = HashSet::from([key.to_string()]);
    run(ctx, Scope::Everything, busy, &asked)
}

/// Takes a world out of the cloud. Every computer keeps the copy it has; they just stop syncing it.
pub fn take_out(ctx: &Context, key: &str) -> Result<Overview, SyncError> {
    let store = Store::new(ctx.data);
    let mut state = store.load();
    let folder = state.folder.clone().ok_or_else(|| SyncError::Other("sync is off".into()))?;
    let remote = Remote::new(&folder.path, &state.device_id);
    let commits = remote.commits(key);
    let tips: Vec<&Commit> = heads(&commits).into_iter().filter(|commit| !commit.removed).collect();
    if let Some(like) = tips.first() {
        let who = Who { device: &state.device_id, device_name: &ctx.device.name, now: (ctx.now)() };
        worlds::remove(&remote, &tips, like, &who)?;
    }
    state.items.remove(key);
    state.declined.insert(key.to_string());
    store.save(&state)?;
    let mut overview = overview(ctx.data);
    if let Some(item) = overview.items.iter_mut().find(|item| item.key == key) {
        item.state = ItemState::Removed { device: ctx.device.name.clone() };
    }
    save_overview(&store, &overview);
    Ok(overview)
}

/// Settles a world that changed here and on another computer.
pub fn resolve(ctx: &Context, key: &str, resolution: Resolution, busy: &HashSet<String>) -> Result<Overview, SyncError> {
    let store = Store::new(ctx.data);
    let state = store.load();
    let folder = state.folder.clone().ok_or_else(|| SyncError::Other("sync is off".into()))?;
    let now = (ctx.now)();
    let remote = Remote::new(&folder.path, &state.device_id);
    let commits = remote.commits(key);
    let tips: Vec<Commit> = heads(&commits).into_iter().filter(|commit| !commit.removed).cloned().collect();
    let (instance, local) = ctx
        .instances
        .list()
        .into_iter()
        .filter(|instance| !busy.contains(&instance.id))
        .find_map(|instance| {
            let sync_id = instance.sync_id.clone()?;
            let world = local_worlds(ctx.instances, &instance).into_iter().find(|world| world_key(&sync_id, &world.folder) == key);
            let owns = world.is_some() || tips.first().is_some_and(|tip| tip.instance == sync_id);
            owns.then_some((instance, world))
        })
        .ok_or_else(|| SyncError::Other("that world's instance isn't here, or is being played".into()))?;
    let sync_id = instance.sync_id.clone().unwrap_or_default();
    let game_dir = ctx.instances.game_dir(&instance.id).ok_or_else(|| SyncError::Other("that instance is gone".into()))?;
    let folder_name = local.as_ref().map(|world| world.folder.clone()).or_else(|| tips.first().and_then(|tip| tip.world.clone())).unwrap_or_default();
    let dir = game_dir.join(crate::worlds::SAVES).join(&folder_name);
    let name = local.as_ref().map(|world| world.name.clone()).unwrap_or_else(|| folder_name.clone());
    let nothing_asked = HashSet::new();
    let mut run = Run { ctx, store, state, remote, now, busy, asked: &nothing_asked, elsewhere: HashMap::new(), waiting: 0 };
    let world = World { key, instance_id: &instance.id, instance_sync: &sync_id, folder: &folder_name, dir: &dir, name: &name };
    let chosen = |commit: &str| tips.iter().find(|tip| tip.id == commit).cloned().ok_or_else(|| SyncError::Other("that version isn't there any more".into()));
    match resolution {
        Resolution::Here => {
            let who = run.who();
            let progress = reporter(run.ctx, key, &name, Direction::Up);
            let commit = worlds::upload(&run.remote, ctx.snapshots, &world, tips.iter().map(|tip| tip.id.clone()).collect(), &who, &progress)?;
            (ctx.activity)(None);
            run.record(key, &commit);
        }
        Resolution::Cloud { commit } => {
            let commit = chosen(&commit)?;
            if let ItemState::Waiting { .. } = run.bring(key, &world, &commit, true)? {
                return Err(SyncError::Other("some of that copy is still on its way; try again in a moment".into()));
            }
            if tips.len() > 1 {
                let refs: Vec<&Commit> = tips.iter().collect();
                let who = run.who();
                let joined = worlds::join(&run.remote, &refs, &commit, &who)?;
                run.record(key, &joined);
            }
        }
        Resolution::Both { commit } => {
            let commit = chosen(&commit)?;
            // This computer's copy becomes its own world first, named after this computer.
            if dir.join("level.dat").is_file() {
                crate::worlds::import_folder(&dir, &game_dir, &format!("{name} ({})", ctx.device.name))?;
            }
            if let ItemState::Waiting { .. } = run.bring(key, &world, &commit, false)? {
                return Err(SyncError::Other("some of that copy is still on its way; try again in a moment".into()));
            }
            if tips.len() > 1 {
                let refs: Vec<&Commit> = tips.iter().collect();
                let who = run.who();
                let joined = worlds::join(&run.remote, &refs, &commit, &who)?;
                run.record(key, &joined);
            }
        }
    }
    let _ = run.store.save(&run.state);
    run_after(ctx, &instance.id, busy)
}

fn run_after(ctx: &Context, instance_id: &str, busy: &HashSet<String>) -> Result<Overview, SyncError> {
    run(ctx, Scope::Instance(instance_id), busy, &HashSet::new())
}

/// The world sync state for a world's badge, by instance id and folder.
pub fn world_states(overview: &Overview) -> HashMap<(String, String), ItemState> {
    overview
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::World)
        .filter_map(|item| Some(((item.instance_id.clone()?, item.world.clone()?), item.state.clone())))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicI64, Ordering};

    use super::*;
    use crate::instances::{Loader, NewInstance};
    use crate::servers::{ListedServer, listed, set_listed};

    static CLOCK: AtomicI64 = AtomicI64::new(1_700_000_000_000);

    fn tick() -> i64 {
        CLOCK.fetch_add(1000, Ordering::SeqCst)
    }

    /// Modrinth has none of the test files, and there's no internet to download from.
    struct NoMods;
    impl Fetch for NoMods {
        fn download(&self, _url: &str, _sha1: &str, _target: &Path) -> Result<(), String> {
            Err("no internet in tests".into())
        }
        fn known(&self, _sha1s: &[String]) -> Option<HashMap<String, String>> {
            Some(HashMap::new())
        }
    }

    /// One computer: its own data folder, instances and restore points.
    struct Computer {
        data: PathBuf,
        instances: InstanceStore,
        snapshots: Snapshots,
        prefs: SyncPrefs,
        device: DeviceInfo,
    }

    impl Computer {
        fn new(root: &Path, name: &str) -> Self {
            let data = root.join(name);
            std::fs::create_dir_all(&data).unwrap();
            Self {
                instances: InstanceStore::new(data.join("instances")),
                snapshots: Snapshots::new(data.join("journal")),
                data,
                prefs: SyncPrefs::default(),
                device: DeviceInfo { name: name.into(), os: "macOS".into() },
            }
        }

        fn with<T>(&self, act: impl FnOnce(&Context) -> T) -> T {
            let quiet = |_: Option<Activity>| {};
            let ctx = Context { data: &self.data, instances: &self.instances, snapshots: &self.snapshots, prefs: &self.prefs, device: &self.device, fetch: &NoMods, activity: &quiet, now: tick };
            act(&ctx)
        }

        fn sync(&self) -> Overview {
            self.with(|ctx| run(ctx, Scope::Everything, &HashSet::new(), &HashSet::new()).unwrap())
        }

        fn only(&self) -> Instance {
            let list = self.instances.list();
            assert_eq!(list.len(), 1, "one instance expected");
            list.into_iter().next().unwrap()
        }

        fn game(&self) -> PathBuf {
            self.instances.game_dir(&self.only().id).unwrap()
        }

        fn world(&self, folder: &str) -> PathBuf {
            self.game().join("saves").join(folder)
        }
    }

    fn state_of<'a>(overview: &'a Overview, name: &str) -> &'a ItemState {
        &overview.items.iter().find(|item| item.name == name).unwrap_or_else(|| panic!("no item {name} in {:#?}", overview.items)).state
    }

    fn synced(state: &ItemState) -> bool {
        matches!(state, ItemState::Synced { .. })
    }

    #[test]
    fn two_computers_share_worlds_instances_options_and_servers() {
        let root = std::env::temp_dir().join(format!("wisp-sync-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let cloud = root.join("iCloud Drive/Wisp");
        let laptop = Computer::new(&root, "Laptop");
        let desktop = Computer::new(&root, "Desktop");

        // The laptop has a Fabric instance with a world, options, a server, a config and a jar.
        laptop
            .instances
            .create(NewInstance { name: "Survival".into(), game_version: "1.21.1".into(), loader: Loader::Fabric, loader_version: Some("0.16.5".into()), icon: None }, Some("0.16.5".into()), tick())
            .unwrap();
        let game = laptop.game();
        let world = laptop.world("Cherry Hills");
        std::fs::create_dir_all(world.join("region")).unwrap();
        std::fs::write(world.join("level.dat"), b"not really nbt").unwrap();
        std::fs::write(world.join("region/r.0.0.mca"), vec![1u8; 40_000]).unwrap();
        std::fs::write(game.join("options.txt"), "version:3955\nfov:0.5\nguiScale:3\nkey_key.jump:key.keyboard.space\n").unwrap();
        set_listed(&game, &[ListedServer { name: "Hub".into(), ip: "hub.example.net".into(), icon: None, accept_textures: None }]).unwrap();
        std::fs::create_dir_all(game.join("config")).unwrap();
        std::fs::write(game.join("config/minimap.toml"), "zoom = 2").unwrap();
        std::fs::write(game.join("config/sodium-options.json"), "{\"quality\": \"fancy\"}").unwrap();
        std::fs::create_dir_all(game.join("mods")).unwrap();
        std::fs::write(game.join("mods/my-own-mod.jar"), b"PK custom jar").unwrap();

        // Turning sync on sends everything.
        let first = laptop.with(|ctx| connect(ctx, &cloud).unwrap());
        assert!(synced(state_of(&first, "Survival")), "{first:#?}");
        assert!(synced(state_of(&first, "Cherry Hills")), "{first:#?}");
        assert_eq!(first.folder.as_ref().unwrap().path, cloud);

        // The desktop already has its own options with a different GUI scale.
        let other = desktop.with(|ctx| connect(ctx, &cloud).unwrap());
        assert!(synced(state_of(&other, "Survival")), "{other:#?}");
        assert!(synced(state_of(&other, "Cherry Hills")), "{other:#?}");
        let there = desktop.only();
        assert_eq!((there.name.as_str(), there.loader, there.loader_version.as_deref()), ("Survival", Loader::Fabric, Some("0.16.5")));
        assert_eq!(there.sync_id, laptop.only().sync_id);
        let desk_game = desktop.game();
        assert_eq!(std::fs::read(desktop.world("Cherry Hills").join("region/r.0.0.mca")).unwrap().len(), 40_000);
        assert_eq!(std::fs::read_to_string(desk_game.join("config/minimap.toml")).unwrap(), "zoom = 2");
        assert!(!desk_game.join("config/sodium-options.json").exists(), "graphics settings stay on each computer");
        assert_eq!(std::fs::read(desk_game.join("mods/my-own-mod.jar")).unwrap(), b"PK custom jar");
        let options = std::fs::read_to_string(desk_game.join("options.txt")).unwrap();
        assert!(options.contains("fov:0.5") && options.contains("key_key.jump:key.keyboard.space") && !options.contains("guiScale"), "{options}");
        assert_eq!(listed(&desk_game).len(), 1);

        // Nothing changed: syncing again moves nothing and stays in step.
        let again = laptop.sync();
        assert!(synced(state_of(&again, "Cherry Hills")));

        // Played on the desktop: a region changed, a server and a key binding added, GUI scale set there.
        std::fs::write(desktop.world("Cherry Hills").join("region/r.0.0.mca"), vec![2u8; 41_000]).unwrap();
        std::fs::write(desktop.world("Cherry Hills").join("region/r.1.0.mca"), vec![3u8; 500]).unwrap();
        let mut servers = listed(&desk_game);
        servers.push(ListedServer { name: "Friends".into(), ip: "friends.example.net".into(), icon: None, accept_textures: Some(1) });
        set_listed(&desk_game, &servers).unwrap();
        let options = std::fs::read_to_string(desk_game.join("options.txt")).unwrap();
        std::fs::write(desk_game.join("options.txt"), format!("{options}guiScale:2\nkey_key.sneak:key.keyboard.left.shift\n")).unwrap();
        let instance_id = desktop.only().id;
        desktop.with(|ctx| run(ctx, Scope::Instance(&instance_id), &HashSet::new(), &HashSet::new()).unwrap());

        // The laptop brings it all in, and keeps its own GUI scale.
        let back = laptop.sync();
        assert!(synced(state_of(&back, "Cherry Hills")), "{back:#?}");
        assert_eq!(std::fs::read(world.join("region/r.0.0.mca")).unwrap(), vec![2u8; 41_000]);
        assert!(world.join("region/r.1.0.mca").exists());
        let options = std::fs::read_to_string(game.join("options.txt")).unwrap();
        assert!(options.contains("guiScale:3") && options.contains("key_key.sneak:key.keyboard.left.shift"), "{options}");
        assert_eq!(listed(&game).iter().map(|server| server.name.as_str()).collect::<Vec<_>>(), vec!["Hub", "Friends"]);
        // The version it replaced is a restore point, as is the one that came in.
        assert!(laptop.snapshots.for_world(&laptop.only().id, "Cherry Hills").len() >= 2);

        // Played on both without syncing in between: a conflict, never a merge.
        std::fs::write(world.join("region/r.0.0.mca"), vec![4u8; 41_000]).unwrap();
        std::fs::write(desktop.world("Cherry Hills").join("region/r.0.0.mca"), vec![5u8; 41_000]).unwrap();
        desktop.sync();
        let clash = laptop.sync();
        let ItemState::Conflict { choices, .. } = state_of(&clash, "Cherry Hills").clone() else { panic!("expected a conflict: {clash:#?}") };
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].device, "Desktop");
        let key = clash.items.iter().find(|item| item.name == "Cherry Hills").unwrap().key.clone();

        // Keep both: the laptop's copy becomes its own world; the desktop's takes the old name.
        let settled = laptop.with(|ctx| resolve(ctx, &key, Resolution::Both { commit: choices[0].commit.clone() }, &HashSet::new()).unwrap());
        assert!(synced(state_of(&settled, "Cherry Hills")), "{settled:#?}");
        assert_eq!(std::fs::read(world.join("region/r.0.0.mca")).unwrap(), vec![5u8; 41_000]);
        let copy = laptop.world("Cherry Hills (Laptop)");
        assert_eq!(std::fs::read(copy.join("region/r.0.0.mca")).unwrap(), vec![4u8; 41_000]);
        // The copy goes up as a world of its own, and reaches the desktop.
        laptop.sync();
        let desk = desktop.sync();
        assert!(synced(state_of(&desk, "Cherry Hills (Laptop)")), "{desk:#?}");
        assert!(desktop.world("Cherry Hills (Laptop)").join("level.dat").exists());

        // Both send the same change at the same moment: the branches are tied, not a conflict.
        std::fs::write(world.join("region/r.2.0.mca"), b"same on both").unwrap();
        std::fs::write(desktop.world("Cherry Hills").join("region/r.2.0.mca"), b"same on both").unwrap();
        laptop.with(|ctx| run(ctx, Scope::Instance(&laptop.only().id), &HashSet::new(), &HashSet::new()).unwrap());
        // (The desktop hadn't seen the laptop's commit yet: it was offline, so to speak.)
        let desk = desktop.sync();
        assert!(synced(state_of(&desk, "Cherry Hills")), "{desk:#?}");
        assert!(synced(state_of(&laptop.sync(), "Cherry Hills")));

        // A change whose files the cloud app hasn't finished bringing: wait, don't half-apply.
        std::fs::write(desktop.world("Cherry Hills").join("region/r.3.0.mca"), vec![9u8; 7000]).unwrap();
        desktop.sync();
        let hash = crate::snapshots::hash_file(&desktop.world("Cherry Hills").join("region/r.3.0.mca")).unwrap();
        let object = cloud.join("objects").join(&hash[..2]).join(&hash[2..]);
        let held = root.join("held-back");
        std::fs::rename(&object, &held).unwrap();
        let waiting = laptop.sync();
        assert!(matches!(state_of(&waiting, "Cherry Hills"), ItemState::Waiting { .. }), "{waiting:#?}");
        assert_eq!(waiting.phase, Phase::Waiting);
        assert!(!world.join("region/r.3.0.mca").exists());
        std::fs::rename(&held, &object).unwrap();
        let arrived = laptop.sync();
        assert!(synced(state_of(&arrived, "Cherry Hills")), "{arrived:#?}");
        assert_eq!(std::fs::read(world.join("region/r.3.0.mca")).unwrap().len(), 7000);

        // While the desktop plays the instance, the laptop leaves its worlds alone.
        let desk_id = desktop.only().id;
        desktop.with(|ctx| check_in(ctx, std::slice::from_ref(&desk_id)).unwrap());
        std::fs::write(desktop.world("Cherry Hills").join("region/r.4.0.mca"), b"mid-session").unwrap();
        desktop.with(|ctx| run(ctx, Scope::Instance(&desk_id), &HashSet::new(), &HashSet::new()).unwrap());
        desktop.with(|ctx| check_in(ctx, std::slice::from_ref(&desk_id)).unwrap());
        let meanwhile = laptop.sync();
        assert!(matches!(state_of(&meanwhile, "Cherry Hills"), ItemState::InUse { device } if device == "Desktop"), "{meanwhile:#?}");
        let check = laptop.with(|ctx| before_launch(ctx, &laptop.only().id, &HashSet::new()).unwrap());
        assert_eq!(check.in_use.as_deref(), Some("Desktop"));
        desktop.with(|ctx| check_in(ctx, &[]).unwrap());
        assert!(synced(state_of(&laptop.sync(), "Cherry Hills")));

        // Taken out of the cloud: both keep their copies and stop syncing it.
        let gone = laptop.with(|ctx| take_out(ctx, &key).unwrap());
        assert!(matches!(state_of(&gone, "Cherry Hills"), ItemState::Removed { .. }));
        let desk = desktop.sync();
        assert!(matches!(state_of(&desk, "Cherry Hills"), ItemState::Removed { .. }), "{desk:#?}");
        assert!(desktop.world("Cherry Hills").join("level.dat").exists());

        // Turning sync off leaves everything where it is.
        laptop.with(|ctx| disconnect(ctx).unwrap());
        assert_eq!(overview(&laptop.data).phase, Phase::Off);
        assert!(cloud.join("wisp-sync.json").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_unplugged_folder_is_reported_not_fatal() {
        let root = std::env::temp_dir().join(format!("wisp-sync-gone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let computer = Computer::new(&root, "Laptop");
        let cloud = root.join("USB/Wisp");
        computer.with(|ctx| connect(ctx, &cloud).unwrap());
        std::fs::remove_dir_all(root.join("USB")).unwrap();
        let overview = computer.sync();
        assert_eq!(overview.phase, Phase::Unreachable);
        assert!(overview.message.unwrap().contains("connected"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
