//! Runs the instance page's backend on a copy of a real instance (the original is only read):
//! names everything in it through Modrinth, looks for updates, plans a move to another Minecraft
//! version, exports it as a .mrpack and reads that back, lists its logs, and looks for a newer
//! version of its modpack.
//!
//!   cargo run -p wisp-core --example instance_check -- <instance id> [game version to plan a move to]
//!
//! Set WISP_DATA to read another data folder.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use wisp_core::instances::{GAME_DIR, InstanceStore};
use wisp_core::{game_logs, inventory, modpack};

fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("WISP_DATA") {
        return dir.into();
    }
    PathBuf::from(std::env::var_os("HOME").expect("HOME is set")).join("Library/Application Support/app.wisp.launcher")
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|since| since.as_millis() as i64).unwrap_or_default()
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let id = args.first().cloned().unwrap_or_else(|| "fabulously-optimized".into());
    let root = std::env::temp_dir().join(format!("wisp-instance-check-{}", std::process::id()));
    copy_dir(&data_dir().join("instances").join(&id), &root.join("instances").join(&id)).expect("copy the instance");
    let store = InstanceStore::new(root.join("instances"));
    let instance = store.get(&id).expect("the copied instance");
    let dir = store.dir_of(&id).unwrap();
    let client = wisp_core::http_client().expect("client");
    println!("{} ({} {:?} {})", instance.name, instance.game_version, instance.loader, instance.loader_version.clone().unwrap_or_default());

    let started = Instant::now();
    let mut scan = inventory::scan(&dir);
    let read = started.elapsed();
    scan.identify(&client, now_ms()).await;
    let identified = started.elapsed();
    let mrpack = instance.modpack.as_ref().map(|origin| data_dir().join("cache/modpacks").join(format!("{}.mrpack", origin.version_id)));
    let pack = inventory::pack_files(&dir, mrpack.as_deref().filter(|path| path.is_file()));
    let items = scan.items(&instance, &dir, pack.as_ref());
    scan.save(&dir);
    // WISP_ITEMS_JSON=<file>: the list as the instance page gets it (for trying the page in a browser).
    if let Some(out) = std::env::var_os("WISP_ITEMS_JSON") {
        let page_items: Vec<inventory::Item> = items.iter().cloned().map(|mut item| {
            item.icon_file = None;
            item
        }).collect();
        std::fs::write(&out, serde_json::to_vec(&page_items).unwrap()).expect("write the items");
        println!("items written to {}", PathBuf::from(out).display());
    }
    println!("\n{} files read in {:.2}s, named through Modrinth in {:.2}s (pack's own list: {})", items.len(), read.as_secs_f32(), identified.as_secs_f32(), if pack.is_some() { "known" } else { "guessed" });
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for item in &items {
        *counts.entry(format!("{:?} / {:?}{}", item.kind, item.origin, if item.enabled { "" } else { " / off" })).or_default() += 1;
    }
    for (key, count) in counts {
        println!("  {count:>3}  {key}");
    }
    let unnamed: Vec<&str> = items.iter().filter(|item| item.project_id.is_none()).map(|item| item.file.as_str()).collect();
    println!("  not on Modrinth: {unnamed:?}");
    println!("  libraries: {:?}", items.iter().filter(|item| item.library).map(|item| item.name.as_str()).collect::<Vec<_>>());
    for item in items.iter().filter(|item| !item.needed_by.is_empty()).take(4) {
        println!("  {} is needed by {:?}", item.name, item.needed_by);
    }
    for item in items.iter().take(3) {
        println!("  e.g. {} {} [{}] icon {:?} / {:?}: {}", item.name, item.version, item.file, item.icon_url, item.icon_file, item.description.chars().take(70).collect::<String>());
    }
    // A second look costs nothing.
    let again = Instant::now();
    let mut second = inventory::scan(&dir);
    second.identify(&client, now_ms()).await;
    println!("  second look: {:.3}s", again.elapsed().as_secs_f32());

    match scan.updates(&client, &instance).await {
        Ok(updates) => {
            println!("\n{} updates", updates.len());
            for update in updates.iter().take(6) {
                println!("  {} -> {}", update.file, update.version_number);
            }
        }
        Err(error) => println!("\nupdates failed: {error}"),
    }

    if let Some(target) = args.get(1) {
        match scan.plan_move(&client, &items, target, instance.loader).await {
            Ok(plan) => println!(
                "\nmoving to {target}: {} updated, {} already right, {} without a build {:?}, {} unknown",
                plan.updates.len(),
                plan.unchanged.len(),
                plan.without_build.len(),
                plan.without_build.iter().map(|item| item.name.as_str()).collect::<Vec<_>>(),
                plan.unknown.len()
            ),
            Err(error) => println!("\nplanning the move failed: {error}"),
        }
    }

    let out = root.join(format!("{}.mrpack", instance.name));
    let options = modpack::ExportOptions { version: "check".into(), configs: true, options: false };
    match modpack::export_mrpack(&instance, &dir, &scan, &out, &options) {
        Ok(exported) => {
            let mut archive = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
            let summary = modpack::summarize(&mut archive).and_then(Result::ok);
            println!(
                "\nexported: {} downloads, {} carried inside, {:.1} MB; reads back as {:?}",
                exported.downloads,
                exported.bundled,
                exported.bytes as f64 / 1e6,
                summary.map(|summary| (summary.game_version, summary.loader))
            );
        }
        Err(error) => println!("\nexport failed: {error}"),
    }

    let logs = game_logs::list(&dir.join(GAME_DIR));
    println!("\n{} logs: {:?}", logs.len(), logs.iter().take(4).map(|log| (&log.name, log.kind)).collect::<Vec<_>>());
    if let Some(latest) = logs.first() {
        let text = game_logs::read(&dir.join(GAME_DIR), &latest.name).unwrap_or_default();
        println!("  {} reads as {} lines", latest.name, text.lines().count());
    }

    match modpack::pack_updates(&client, &instance).await {
        Ok(updates) => println!(
            "\npack updates: for this Minecraft {:?}, newer Minecraft {:?}",
            updates.same_game.map(|release| release.version_number),
            updates.newer_game.map(|release| (release.version_number, release.game_versions))
        ),
        Err(error) => println!("\npack updates failed: {error}"),
    }
    println!("\ncleaning up {}", root.display());
    let _ = std::fs::remove_dir_all(&root);
}
