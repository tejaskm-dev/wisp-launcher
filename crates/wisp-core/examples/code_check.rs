//! Publishes a throwaway server's word on the public relays and looks it up by its code, as a
//! host and a player's Wisp do (nothing personal is sent: a made-up server, encrypted).
//!
//!   cargo run -p wisp-core --example code_check

use wisp_core::codes::{self, CodeCard};

#[tokio::main]
async fn main() {
    let relays: Vec<String> = wisp_core::social::DEFAULT_RELAYS.iter().map(|relay| relay.to_string()).collect();
    let keys = codes::server_keys(None);
    let code = codes::code_of(&keys.public_key());
    println!("Code: {code}");
    let card = CodeCard {
        name: "Code check".into(),
        game_version: "1.21.4".into(),
        loader: "fabric".into(),
        software: None,
        addresses: vec!["example.invalid:25565".into()],
        online: true,
        players: 0,
        max_players: 10,
        mods: 0,
        at: 1,
    };
    let started = std::time::Instant::now();
    match codes::publish(&keys, &card, &relays).await {
        Ok(()) => println!("Published in {:.1}s", started.elapsed().as_secs_f64()),
        Err(error) => println!("Publishing failed: {error}"),
    }
    let started = std::time::Instant::now();
    match codes::resolve(&code, &relays).await {
        Ok(Some(found)) => println!("Found in {:.1}s: {} at {:?}, matches: {}", started.elapsed().as_secs_f64(), found.name, found.addresses, found == card),
        Ok(None) => println!("Not found"),
        Err(error) => println!("Looking up failed: {error}"),
    }
    let unknown = codes::code_of(&codes::server_keys(None).public_key());
    println!("An unknown code: {:?}", codes::resolve(&unknown, &relays).await.map(|found| found.is_some()));
}
