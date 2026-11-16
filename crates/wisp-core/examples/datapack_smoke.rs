//! Adds a Modrinth data pack to a throwaway world, then removes it again.
//!
//!   cargo run -p wisp-core --example datapack_smoke -- [project-slug] [game-version]

use wisp_core::content::{self, ContentIndex, InstallRequest};
use wisp_core::instances::{GAME_DIR, InstanceStore, Loader, NewInstance};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let slug = args.next().unwrap_or_else(|| "terralith".into());
    let game = args.next().unwrap_or_else(|| "1.21.1".into());
    let root = std::env::temp_dir().join(format!("wisp-datapack-smoke-{}", std::process::id()));
    let instances = InstanceStore::new(root.join("instances"));
    let client = wisp_core::download_client().expect("client");
    let instance = instances
        .create(
            NewInstance {
                name: "Packs".into(),
                game_version: game,
                loader: Loader::Vanilla,
                loader_version: None,
                icon: None,
            },
            None,
            0,
        )
        .expect("instance");
    let dir = instances.dir_of(&instance.id).expect("dir");
    let world = dir.join(GAME_DIR).join("saves/My World");
    std::fs::create_dir_all(&world).unwrap();
    std::fs::write(world.join("level.dat"), b"stub").unwrap();
    println!("worlds: {:?}", wisp_core::worlds::list(&dir.join(GAME_DIR)));

    let request = |world| InstallRequest {
        project_id: &slug,
        version_id: None,
        world,
        concurrency: 4,
        curseforge_key: None,
    };
    let refused = content::install(&client, &instance, &dir, request(None)).await;
    println!(
        "without a world: {}",
        refused
            .err()
            .map(|error| error.to_string())
            .unwrap_or("installed?!".into())
    );
    let added = content::install(&client, &instance, &dir, request(Some("My World")))
        .await
        .expect("install");
    for item in &added {
        let size = std::fs::metadata(dir.join(GAME_DIR).join(&item.file))
            .map(|meta| meta.len())
            .unwrap_or(0);
        println!(
            "added {} {} → {} ({size} bytes, world {:?})",
            item.title, item.version_number, item.file, item.world
        );
    }
    let index = ContentIndex::new(&dir);
    index
        .remove(&added[0].project_id, Some("My World"))
        .expect("remove");
    println!(
        "after removing: {} entries, file gone: {}",
        index.list().len(),
        !dir.join(GAME_DIR).join(&added[0].file).exists()
    );
    let missing = content::install(&client, &instance, &dir, request(Some("Nope"))).await;
    println!(
        "missing world: {}",
        missing
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default()
    );
    let _ = std::fs::remove_dir_all(root);
}
