//! Runs "Speed up" end to end in a throwaway folder: a vanilla instance (which moves to Fabric
//! first) and a NeoForge one each get the performance mods that have builds for them. Nothing is
//! launched; only files are downloaded and checked.
//!
//!   cargo run -p wisp-core --example speed_up_smoke -- [minecraft version]

use wisp_core::content::ContentIndex;
use wisp_core::instances::{InstanceStore, Loader, NewInstance};

#[tokio::main]
async fn main() {
    let game = std::env::args().nth(1).unwrap_or_else(|| "1.21.1".into());
    let root = std::env::temp_dir().join(format!("wisp-speed-up-smoke-{}", std::process::id()));
    let instances = InstanceStore::new(root.join("instances"));
    let client = wisp_core::download_client().expect("client");

    for (loader, loader_version) in [(Loader::Vanilla, None), (Loader::NeoForge, Some("21.1.77".to_string()))] {
        let instance = instances
            .create(NewInstance { name: format!("{} {game}", loader.display_name()), game_version: game.clone(), loader, loader_version: loader_version.clone(), icon: None }, loader_version, 0)
            .expect("instance");
        let started = std::time::Instant::now();
        match wisp_core::performance::speed_up(&client, &instances, &instance.id, 8).await {
            Ok(sped) => {
                println!(
                    "{}: now {} {}, moved to Fabric: {}",
                    instance.name,
                    sped.instance.loader.display_name(),
                    sped.instance.loader_version.as_deref().unwrap_or("-"),
                    sped.moved_to_fabric
                );
                println!("  added: {}", sped.added.iter().map(|item| format!("{} {}", item.title, item.version_number)).collect::<Vec<_>>().join(", "));
                println!("  had: {:?}  skipped: {:?}  ({:.1}s)", sped.had, sped.skipped, started.elapsed().as_secs_f32());
                let files = ContentIndex::new(instances.dir_of(&instance.id).unwrap()).list();
                println!("  index: {} entries", files.len());
                // A second run finds everything in place.
                let again = wisp_core::performance::speed_up(&client, &instances, &instance.id, 8).await.expect("again");
                println!("  second run: added {}, had {}", again.added.len(), again.had.len());
            }
            Err(error) => println!("{}: failed: {error}", instance.name),
        }
    }
    let _ = std::fs::remove_dir_all(root);
}
