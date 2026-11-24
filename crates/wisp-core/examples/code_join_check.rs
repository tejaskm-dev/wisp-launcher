//! Joining by code end to end on one computer, over the real relays: a server's door publishes
//! its code, a player looks the code up, reaches the door it names, is let in, and gets the mod
//! list, as the app does (nothing personal is sent: a made-up server, encrypted).
//!
//!   cargo run -p wisp-core --example code_join_check

use std::sync::Arc;

use wisp_core::codes::{self, CodeCard, ServerCode};
use wisp_core::tunnel::door::Door;
use wisp_core::tunnel::{self, Ask, Keys, Service};

#[tokio::main]
async fn main() {
    let relays: Vec<String> = wisp_core::social::DEFAULT_RELAYS.iter().map(|relay| relay.to_string()).collect();
    let dir = std::env::temp_dir().join(format!("wisp-code-join-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    // The host: a door in front of a stand-in server, letting in whoever asks as "Friend".
    let keys = Keys::new().unwrap();
    let service = Arc::new(Service::default());
    *service.manifest.write().unwrap() = br#"{"mods":["lithium.jar"]}"#.to_vec();
    *service.on_ask.write().unwrap() = Some(Arc::new(|ask: &Ask| ask.name == "Friend"));
    let backend = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    let door = Door::open(0, backend, &keys, service).await.unwrap();
    let code = ServerCode::load_or_make(&dir.join(".wisp-code")).unwrap();
    let card = CodeCard {
        name: "Code join check".into(),
        game_version: "1.21.4".into(),
        loader: "fabric".into(),
        software: None,
        addresses: vec!["192.0.2.1:25565".into(), format!("127.0.0.1:{}", door.port())],
        online: true,
        starting: false,
        players: 0,
        max_players: 10,
        mods: 1,
        at: 1,
    };
    code.publish(&card, &relays).await.expect("publish");
    println!("Host published {}", code.code());
    // The same key next time: the code doesn't change.
    assert_eq!(ServerCode::load_or_make(&dir.join(".wisp-code")).unwrap().code(), code.code());

    // The player: types the code (in any case, without dashes), finds the server, joins.
    let typed = code.code().to_lowercase().replace('-', "");
    let normalized = codes::normalize(&typed).expect("a code");
    let found = codes::resolve(&normalized, &relays).await.expect("resolve").expect("found");
    println!("Player found {} at {:?}", found.name, found.addresses);
    let player = Keys::new().unwrap();
    let mut joined = None;
    for address in &found.addresses {
        match tunnel::ask_door(address, &player, &Ask { name: "Friend".into(), uuid: "ab".repeat(16) }).await {
            Ok((link, _)) => {
                joined = Some((address.clone(), link));
                break;
            }
            Err(error) => println!("  {address}: {error}"),
        }
    }
    let (address, link) = joined.expect("one address lets the player in");
    let manifest = link.manifest().await.expect("manifest");
    println!("Joined through {address}; the mod list says {}", String::from_utf8_lossy(&manifest));
    let _ = std::fs::remove_dir_all(dir);
}
