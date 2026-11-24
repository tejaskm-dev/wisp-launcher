//! Checks a server hosted with Wisp from the outside, the way a joining Wisp reaches it: pings the
//! address (does the answer carry Wisp's mark?) and knocks on its door (does the host's Wisp answer
//! through it?). For addresses that go through a tunnel, such as playit's, which a game can pass
//! while Wisp's own connection might not. The knock asks as a made-up player, so a server with a
//! whitelist turns it away, which still shows the door answered.
//!
//!   cargo run -p wisp-core --example door_check -- <address>

use std::time::Instant;

use wisp_core::tunnel::{self, Ask, Keys, TunnelError};

#[tokio::main]
async fn main() {
    let Some(address) = std::env::args().nth(1) else {
        eprintln!("usage: door_check <address>");
        std::process::exit(2);
    };

    let status = wisp_core::servers::statuses(vec![address.clone()]).await.remove(0);
    if !status.online {
        println!("ping:  no answer ({})", status.error.unwrap_or_default());
        println!("The server isn't up at that address, so there's no door to knock on.");
        return;
    }
    println!("ping:  up, {} ms, {}", status.ping_ms.unwrap_or(0), if status.wisp { "with Wisp's mark" } else { "WITHOUT Wisp's mark" });

    let keys = Keys::new().expect("keys");
    let ask = Ask { name: "Wisp_Check".into(), uuid: "0".repeat(31) + "1" };
    let started = Instant::now();
    let knocked = tunnel::ask_door(&address, &keys, &ask).await;
    let took = started.elapsed().as_millis();
    match knocked {
        Ok((link, _)) => {
            let manifest = link.manifest().await.map(|bytes| bytes.len());
            println!("knock: let in after {took} ms; mod list {}", manifest.map_or_else(|error| format!("failed: {error}"), |bytes| format!("{bytes} bytes")));
            link.close();
            println!("Wisp's link works through this address: joins bring skins and mods.");
        }
        Err(TunnelError::Refused) => {
            println!("knock: turned away after {took} ms (the whitelist), but the host's Wisp answered");
            println!("Wisp's link works through this address.");
        }
        Err(error) => {
            println!("knock: failed after {took} ms: {error}");
            println!("Wisp's link does NOT get through this address: joins fall back to a plain game, without skins or mods.");
        }
    }
}
