//! Exercises Modrinth installs end to end in a throwaway folder: adds a mod with a required
//! dependency to a Fabric instance, removes it again, then installs a modpack as an instance.
//! Nothing is launched; only files are downloaded and checked.
//!
//!   cargo run -p wisp-core --example content_smoke -- [modpack-slug]

use std::time::Instant;

use wisp_core::content::{self, ContentIndex};
use wisp_core::instances::{GAME_DIR, InstanceStore, Loader, NewInstance};
use wisp_core::modpack::{self, InstallUpdate, Request};

#[tokio::main]
async fn main() {
    let modpack_slug = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "fabulously-optimized".into());
    let root = std::env::temp_dir().join(format!("wisp-content-smoke-{}", std::process::id()));
    let instances = InstanceStore::new(root.join("instances"));
    let client = wisp_core::download_client().expect("client");
    let started = Instant::now();

    let instance = instances
        .create(
            NewInstance {
                name: "Smoke".into(),
                game_version: "26.3".into(),
                loader: Loader::Fabric,
                loader_version: None,
                icon: None,
            },
            Some("0.20.1".into()),
            0,
        )
        .expect("instance");
    let dir = instances.dir_of(&instance.id).expect("dir");

    // Mod Menu requires Fabric API, so this should bring both.
    let request = |project_id| content::InstallRequest {
        project_id,
        version_id: None,
        world: None,
        concurrency: 8,
        curseforge_key: None,
    };
    let added = content::install(&client, &instance, &dir, request("modmenu"))
        .await
        .expect("install modmenu");
    for item in &added {
        let on_disk = dir.join(GAME_DIR).join(&item.file).is_file();
        println!(
            "added {} {} ({}) explicit={} on_disk={on_disk}",
            item.title, item.version_number, item.file, item.explicit
        );
    }
    let index = ContentIndex::new(&dir);
    println!("index has {} entries", index.list().len());
    index.remove(&added[0].project_id, None).expect("remove");
    println!(
        "after removing {}: {} entries",
        added[0].title,
        index.list().len()
    );

    // A Vanilla instance must refuse mods with a clear message.
    let vanilla = instances
        .create(
            NewInstance {
                name: "Plain".into(),
                game_version: "26.3".into(),
                loader: Loader::Vanilla,
                loader_version: None,
                icon: None,
            },
            None,
            0,
        )
        .expect("vanilla instance");
    let refused = content::install(
        &client,
        &vanilla,
        &instances.dir_of(&vanilla.id).unwrap(),
        request("sodium"),
    )
    .await;
    println!(
        "vanilla + sodium: {}",
        refused
            .err()
            .map(|error| error.to_string())
            .unwrap_or("installed?!".into())
    );

    // A modpack becomes its own instance.
    let report = |update: InstallUpdate| {
        if let InstallUpdate::Downloading {
            done_files,
            total_files,
            ..
        } = &update
        {
            if done_files == total_files {
                println!("  downloaded {done_files} files");
            }
        } else {
            println!("  {update:?}");
        }
    };
    let cache = root.join("cache");
    let pack = modpack::install(
        &client,
        &instances,
        Request {
            project_id: &modpack_slug,
            version_id: None,
            cache_dir: &cache,
            concurrency: 16,
            now_ms: 0,
            curseforge_key: None,
        },
        &report,
    )
    .await
    .expect("modpack")
    .instance;
    let game_dir = instances.game_dir(&pack.id).unwrap();
    let mods = std::fs::read_dir(game_dir.join("mods"))
        .map(|entries| entries.count())
        .unwrap_or(0);
    let config = game_dir.join("config").is_dir();
    println!(
        "modpack {} → instance {} ({} {:?} {:?}), {mods} mods, config folder: {config}",
        modpack_slug, pack.id, pack.game_version, pack.loader, pack.loader_version
    );
    println!(
        "done in {:.1}s; cleaning up {}",
        started.elapsed().as_secs_f32(),
        root.display()
    );
    let _ = std::fs::remove_dir_all(&root);
}
