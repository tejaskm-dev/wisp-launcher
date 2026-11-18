//! Runs Wisp's fixer over real modpacks the way a launch does: the quick look at the mods folder,
//! the repair, then the look again. A pack that starts in its own launcher should need no fixing,
//! so anything found here is the checker being wrong (or the pack really being broken).
//!
//! Each pack is installed into a throwaway folder and removed after its check. Nothing launches.
//!
//!   cargo run -p wisp-core --example doctor_smoke -- [modpack-slug[@version-id] ...]
//!   cargo run -p wisp-core --example doctor_smoke -- --copy <instances folder> <instance id>

use std::path::Path;
use std::time::Instant;

use wisp_core::doctor::{self, Fixer, Problem};
use wisp_core::instances::{Instance, InstanceStore};
use wisp_core::modpack::{self, Request};

/// Popular packs across loaders and Minecraft versions (most of Modrinth's most-followed).
const DEFAULT_PACKS: &[&str] = &[
    "fabulously-optimized",
    "vanilla-perfected",
    "spunky-optimized",
    "fresh-smooth",
    "cobblemon-fabric",
    "aged",
    "homestead",
    "create-live-5",
];

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let client = wisp_core::download_client().expect("client");
    let root = std::env::temp_dir().join(format!("wisp-doctor-smoke-{}", std::process::id()));
    let instances = InstanceStore::new(root.join("instances"));
    let cache = root.join("cache");
    let mut clean = 0;
    let mut total = 0;

    if args.first().map(String::as_str) == Some("--copy") {
        let (Some(from), Some(id)) = (args.get(1), args.get(2)) else {
            eprintln!("usage: --copy <instances folder> <instance id>");
            return;
        };
        copy_dir(&Path::new(from).join(id), &root.join("instances").join(id)).expect("copy the instance");
        let instance = instances.get(id).expect("the copied instance");
        total += 1;
        if examine(&client, &instances, &instance).await {
            clean += 1;
        }
    } else {
        let packs: Vec<String> = if args.is_empty() { DEFAULT_PACKS.iter().map(|slug| slug.to_string()).collect() } else { args };
        for pack in packs {
            let (slug, version) = match pack.split_once('@') {
                Some((slug, version)) => (slug.to_string(), Some(version.to_string())),
                None => (pack.clone(), None),
            };
            let started = Instant::now();
            let installed = modpack::install(
                &client,
                &instances,
                Request { project_id: &slug, version_id: version.as_deref(), cache_dir: &cache, concurrency: 16, now_ms: 0, curseforge_key: None },
                &|_| {},
            )
            .await;
            let instance = match installed {
                Ok(installed) => installed.instance,
                Err(error) => {
                    println!("\n== {slug}: couldn't install: {error}");
                    continue;
                }
            };
            println!(
                "\n== {} ({} {:?} {}) installed in {:.0}s",
                instance.name,
                instance.game_version,
                instance.loader,
                instance.loader_version.clone().unwrap_or_default(),
                started.elapsed().as_secs_f32()
            );
            total += 1;
            if examine(&client, &instances, &instance).await {
                clean += 1;
            }
            let _ = instances.delete(&instance.id);
        }
    }
    println!("\n{clean} of {total} needed no fixing. Cleaning up {}", root.display());
    let _ = std::fs::remove_dir_all(&root);
}

/// Checks, repairs and checks again. True when the first check found nothing.
async fn examine(client: &wisp_core::HttpClient, instances: &InstanceStore, instance: &Instance) -> bool {
    let dir = instances.dir_of(&instance.id).expect("dir");
    let started = Instant::now();
    let mods = doctor::scan(&dir);
    let problems = doctor::check(instance, &dir);
    println!("   {} jars read, checked in {:.2}s", mods.len(), started.elapsed().as_secs_f32());
    if problems.is_empty() {
        println!("   nothing to fix");
        return true;
    }
    for problem in &problems {
        println!("   found: {problem:?}");
        print_why(problem, &mods);
    }
    println!("   counts as {} to fix", doctor::count(&problems));
    let fixer = Fixer { client, store: instances, instance_id: &instance.id, concurrency: 16, curseforge_key: None, memory_mb: 4096, system_mb: Some(16384) };
    let before = files_in(&dir.join("minecraft/mods"));
    let report = fixer.repair(problems).await;
    let after_files = files_in(&dir.join("minecraft/mods"));
    for (name, size) in &after_files {
        match before.iter().find(|(old, _)| old == name) {
            None => println!("   mods/ gained: {name} ({size} bytes)"),
            Some((_, old)) if old != size => println!("   mods/ changed: {name} ({old} -> {size} bytes)"),
            Some(_) => {}
        }
    }
    for (name, _) in &before {
        if !after_files.iter().any(|(now, _)| now == name) {
            println!("   mods/ lost: {name}");
        }
    }
    for line in &report.found {
        println!("   report found: {line}");
    }
    for line in &report.fixed {
        println!("   fixed: {line}");
    }
    for offer in &report.offers {
        println!("   offer: {} ({})", offer.label, offer.detail);
    }
    for line in &report.unsolved {
        println!("   unsolved: {line}");
    }
    let after = instances.get(&instance.id).map(|instance| doctor::check(&instance, &dir)).unwrap_or_default();
    println!("   after fixing: {} problems {after:?}", after.len());
    false
}

/// For a missing mod: which jars need it, and which jars come close to providing it.
fn print_why(problem: &Problem, mods: &[doctor::ModInfo]) {
    if let Problem::Missing { id, .. } = problem {
        let wanting: Vec<String> = mods
            .iter()
            .flat_map(|info| info.requires.iter().filter(|requirement| &requirement.id == id).map(|requirement| format!("{} ({:?} metadata)", info.file, requirement.platform)))
            .collect();
        let close: Vec<String> = mods
            .iter()
            .filter(|info| info.file.contains(id.as_str()) || info.id.contains(id.as_str()) || info.provides.iter().any(|provided| provided.contains(id.as_str())))
            .map(|info| format!("{} (id {:?}, provides {:?}, loaders {:?})", info.file, info.id, info.provides, info.loaders))
            .collect();
        println!("      wanted by {wanting:?}");
        println!("      close matches: {close:?}");
    }
}

/// Every file in a folder with its size, to see what a repair touched.
fn files_in(dir: &Path) -> Vec<(String, u64)> {
    let mut files: Vec<(String, u64)> = std::fs::read_dir(dir)
        .map(|entries| entries.flatten().filter_map(|entry| Some((entry.file_name().to_string_lossy().into_owned(), entry.metadata().ok()?.len()))).collect())
        .unwrap_or_default();
    files.sort();
    files
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
