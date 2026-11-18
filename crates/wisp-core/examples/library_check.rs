//! Prints what Wisp reads from a data folder's worlds and screenshots, without changing anything.
//!
//!   cargo run -p wisp-core --example library_check -- [data folder]
//!
//! The data folder defaults to Wisp's own on this computer.

use wisp_core::accounts::AccountStore;
use wisp_core::home::HomeSnapshot;
use wisp_core::instances::InstanceStore;
use wisp_core::journal::Journal;

fn main() {
    let data = std::env::args().nth(1).map(std::path::PathBuf::from).unwrap_or_else(|| {
        let home = std::env::var("HOME").expect("HOME");
        std::path::Path::new(&home).join("Library/Application Support/app.wisp.launcher")
    });
    let instances = InstanceStore::new(data.join("instances"));
    let home = HomeSnapshot::load(&instances, &AccountStore::new(data.join("accounts.json")), &Journal::new(data.join("journal")), &wisp_core::sync::overview(&data));
    println!("{}", serde_json::to_string_pretty(&home.worlds).unwrap());
    for instance in &home.instances {
        let game = instances.game_dir(&instance.id).unwrap();
        println!("{}: {} screenshots", instance.name, wisp_core::screenshots::list(&game).len());
    }
}
