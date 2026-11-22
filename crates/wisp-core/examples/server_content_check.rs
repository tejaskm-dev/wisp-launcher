//! Installs real projects from Modrinth into throwaway server folders, the way a server's
//! Plugins or Mods tab does: a Paper plugin, and a Fabric mod with what it requires.
//!
//!   cargo run -p wisp-core --example server_content_check

use wisp_core::hosting::content;
use wisp_core::hosting::software::Software;
use wisp_core::hosting::{NewServer, ServerStore};
use wisp_core::instances::Loader;

#[tokio::main]
async fn main() {
    let client = wisp_core::download_client().expect("client");
    let root = std::env::temp_dir().join(format!("wisp-content-check-{}", std::process::id()));
    let store = ServerStore::new(&root);
    for (name, loader, loader_version, software, project) in [
        ("Paper", Loader::Vanilla, None, Some(Software::Paper), "luckperms"),
        ("Fabric", Loader::Fabric, Some("0.16.10".to_string()), None, "lithium"),
    ] {
        let server = store
            .create(NewServer { name: name.into(), game_version: "1.21.4".into(), loader, loader_version, instance_id: None, icon: None, properties: Vec::new(), software }, 0)
            .expect("create");
        let dir = store.dir_of(&server.id).expect("dir");
        match content::install(&client, &server, &dir, project).await {
            Ok(added) => println!("{name}: installed {added:?}"),
            Err(error) => println!("{name}: failed: {error}"),
        }
        for item in content::list(&server, &dir) {
            println!("  {} {} ({}){}", item.name, item.version, item.file, if item.project_id.is_some() { ", from Modrinth" } else { "" });
        }
    }
    let _ = std::fs::remove_dir_all(root);
}
