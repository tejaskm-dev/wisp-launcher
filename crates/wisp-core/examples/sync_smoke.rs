//! Cloud sync with real instances, between two pretend computers sharing a temp folder, with mods
//! coming from Modrinth for real. This Mac's Wisp instances are copied (only read) into the first
//! computer; nothing in the real data folder is touched.
//!
//!   cargo run -p wisp-core --example sync_smoke --release

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use wisp_core::device::DeviceInfo;
use wisp_core::instances::{GAME_DIR, InstanceStore};
use wisp_core::settings::SyncPrefs;
use wisp_core::snapshots::Snapshots;
use wisp_core::sync::{self, Activity, Context, Fetch, ItemState, Overview, Scope};
use wisp_core::{HttpClient, http_client, modrinth};

struct Net {
    client: HttpClient,
    runtime: tokio::runtime::Handle,
}

impl Fetch for Net {
    fn download(&self, url: &str, sha1: &str, target: &Path) -> Result<(), String> {
        let bytes = self.runtime.block_on(async { self.client.get(url).send().await?.error_for_status()?.bytes().await }).map_err(|error| error.to_string())?;
        if sync::sha1_of(&bytes) != sha1 {
            return Err("checksum".into());
        }
        std::fs::create_dir_all(target.parent().unwrap()).map_err(|error| error.to_string())?;
        std::fs::write(target, &bytes).map_err(|error| error.to_string())
    }

    fn known(&self, sha1s: &[String]) -> Option<HashMap<String, String>> {
        let found = self.runtime.block_on(modrinth::versions_by_hashes(&self.client, sha1s)).ok()?;
        Some(found.into_iter().filter_map(|(hash, version)| Some((hash.clone(), version.files.iter().find(|file| file.sha1.as_deref() == Some(hash.as_str()))?.url.clone()))).collect())
    }
}

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
        Self { instances: InstanceStore::new(data.join("instances")), snapshots: Snapshots::new(data.join("journal")), data, prefs: SyncPrefs::default(), device: DeviceInfo { name: name.into(), os: "macOS".into() } }
    }

    fn with<T>(&self, fetch: &dyn Fetch, act: impl FnOnce(&Context) -> T) -> T {
        let quiet = |_: Option<Activity>| {};
        let now = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        let ctx = Context { data: &self.data, instances: &self.instances, snapshots: &self.snapshots, prefs: &self.prefs, device: &self.device, fetch, activity: &quiet, now };
        act(&ctx)
    }
}

fn copy_dir(from: &Path, to: &Path) -> u64 {
    let mut bytes = 0;
    let Ok(entries) = std::fs::read_dir(from) else { return 0 };
    std::fs::create_dir_all(to).unwrap();
    for entry in entries.flatten() {
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            bytes += copy_dir(&entry.path(), &target);
        } else if entry.file_name() != "session.lock" {
            bytes += std::fs::copy(entry.path(), &target).unwrap();
        }
    }
    bytes
}

fn summary(label: &str, overview: &Overview, took: f64) {
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    for item in &overview.items {
        let state = serde_json::to_value(&item.state).unwrap()["kind"].as_str().unwrap_or("?").to_string();
        *counts.entry(format!("{:?} {state}", item.kind)).or_default() += 1;
    }
    println!("{label}: {took:.2}s, {:?}, {counts:?}", overview.phase);
    for item in &overview.items {
        if let ItemState::Failed { message } = &item.state {
            println!("  FAILED {}: {message}", item.name);
        }
    }
}

/// path -> sha1 of everything synced in an instance's game folder (mods, config, saves).
fn fingerprint(game: &Path) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    for folder in ["mods", "config", "saves"] {
        let mut pending = vec![game.join(folder)];
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if entry.file_type().unwrap().is_dir() {
                    pending.push(path);
                } else if !matches!(entry.file_name().to_str(), Some("session.lock" | ".DS_Store")) && !path.to_string_lossy().contains("sodium-options") {
                    let relative = path.strip_prefix(game).unwrap().to_string_lossy().into_owned();
                    found.insert(relative, sync::sha1_of(&std::fs::read(&path).unwrap()));
                }
            }
        }
    }
    found
}

fn main() {
    let real = PathBuf::from(std::env::var("HOME").unwrap()).join("Library/Application Support/app.wisp.launcher/instances");
    let root = std::env::temp_dir().join("wisp-sync-smoke");
    let _ = std::fs::remove_dir_all(&root);
    let cloud = root.join("Cloud/Wisp");
    let laptop = Computer::new(&root, "Laptop");
    let desktop = Computer::new(&root, "Desktop");

    let mut copied = 0;
    for entry in std::fs::read_dir(&real).expect("no Wisp instances on this Mac").flatten() {
        let from = entry.path();
        if !from.join("instance.json").is_file() {
            continue;
        }
        let to = laptop.data.join("instances").join(entry.file_name());
        std::fs::create_dir_all(to.join(GAME_DIR)).unwrap();
        for file in ["instance.json", "content.json"] {
            if from.join(file).is_file() {
                copied += std::fs::copy(from.join(file), to.join(file)).unwrap();
            }
        }
        for part in ["mods", "config", "saves", "resourcepacks", "shaderpacks"] {
            copied += copy_dir(&from.join(GAME_DIR).join(part), &to.join(GAME_DIR).join(part));
        }
        for file in ["options.txt", "servers.dat"] {
            if from.join(GAME_DIR).join(file).is_file() {
                copied += std::fs::copy(from.join(GAME_DIR).join(file), to.join(GAME_DIR).join(file)).unwrap();
            }
        }
    }
    println!("copied {} instances, {:.1} MB", laptop.instances.list().len(), copied as f64 / 1e6);

    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    let net = Net { client: http_client().unwrap(), runtime: runtime.handle().clone() };
    let none = HashSet::new();

    let started = Instant::now();
    let first = laptop.with(&net, |ctx| sync::connect(ctx, &cloud).unwrap());
    summary("laptop turns sync on", &first, started.elapsed().as_secs_f64());
    println!("  sync folder: {:.1} MB", wisp_core::storage::dir_size(&cloud) as f64 / 1e6);

    let started = Instant::now();
    let again = laptop.with(&net, |ctx| sync::run(ctx, Scope::Everything, &none, &none).unwrap());
    summary("laptop, nothing changed", &again, started.elapsed().as_secs_f64());

    let started = Instant::now();
    let joined = desktop.with(&net, |ctx| sync::connect(ctx, &cloud).unwrap());
    summary("desktop joins", &joined, started.elapsed().as_secs_f64());

    // Every instance arrived with the same mods, configs and worlds.
    let mut same = 0;
    for there in desktop.instances.list() {
        let here = laptop.instances.list().into_iter().find(|instance| instance.sync_id == there.sync_id).expect("matched by sync id");
        let (a, b) = (fingerprint(&laptop.instances.game_dir(&here.id).unwrap()), fingerprint(&desktop.instances.game_dir(&there.id).unwrap()));
        let missing: Vec<&String> = a.keys().filter(|path| b.get(*path) != a.get(*path)).collect();
        if missing.is_empty() {
            same += 1;
        } else {
            println!("  {} differs on the desktop: {} files, e.g. {:?}", there.name, missing.len(), &missing[..missing.len().min(5)]);
        }
    }
    println!("  {same} of {} instances identical on both", desktop.instances.list().len());

    let started = Instant::now();
    let steady = desktop.with(&net, |ctx| sync::run(ctx, Scope::Everything, &none, &none).unwrap());
    summary("desktop, nothing changed", &steady, started.elapsed().as_secs_f64());

    // Play on the desktop: change a region file in the biggest world.
    fn first_region(dir: &Path) -> Option<PathBuf> {
        for entry in std::fs::read_dir(dir).ok()?.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|extension| extension == "mca") {
                return Some(path);
            }
            if path.is_dir()
                && let Some(found) = first_region(&path)
            {
                return Some(found);
            }
        }
        None
    }
    if let Some((instance, region)) = desktop.instances.list().into_iter().find_map(|instance| {
        let saves = desktop.instances.game_dir(&instance.id)?.join("saves");
        Some((instance, first_region(&saves)?))
    }) {
        let mut bytes = std::fs::read(&region).unwrap();
        bytes[100] ^= 0xff;
        std::fs::write(&region, &bytes).unwrap();
        let started = Instant::now();
        let after = desktop.with(&net, |ctx| sync::run(ctx, Scope::Instance(&instance.id), &none, &none).unwrap());
        summary("desktop sends a played world", &after, started.elapsed().as_secs_f64());
        let started = Instant::now();
        let pulled = laptop.with(&net, |ctx| sync::run(ctx, Scope::Everything, &none, &none).unwrap());
        summary("laptop brings it", &pulled, started.elapsed().as_secs_f64());
        let laptop_region = laptop.instances.list().into_iter().find(|candidate| candidate.sync_id == instance.sync_id).and_then(|here| laptop.instances.game_dir(&here.id)).map(|game| game.join(region.strip_prefix(desktop.instances.game_dir(&instance.id).unwrap()).unwrap()));
        println!("  region file matches: {}", laptop_region.is_some_and(|path| std::fs::read(path).ok() == Some(bytes)));
    }
    println!("  sync folder now: {:.1} MB", wisp_core::storage::dir_size(&cloud) as f64 / 1e6);
    let _ = std::fs::remove_dir_all(&root);
}
