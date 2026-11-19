//! A host's skin server for multiplayer, as Wisp runs it in builds with offline accounts: it
//! answers for the host, for a friend who handed in a skin over the link, and for anyone else by
//! name as an offline player. Prints what to start Java with and every question it's asked, for
//! checking a game's or a server's sign-in against it. Stops after 90 seconds.
//!
//!   cargo run -p wisp-core --example skin_join_smoke -- <path to authlib-injector.jar>

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use wisp_core::accounts::{offline_uuid, valid_username};
use wisp_core::skin_server::{self, Guests, Lookup, Observe, Player, Skin, Who};

fn skin(rgb: [u8; 3]) -> Vec<u8> {
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(64, 64, image::Rgba([rgb[0], rgb[1], rgb[2], 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("png");
    png
}

#[tokio::main]
async fn main() {
    let injector = std::env::args().nth(1).expect("the authlib-injector jar");
    let host = Player { uuid: offline_uuid("Host"), name: "Host".into(), skin: Some(Skin { png: skin([229, 72, 77]), slim: false }) };
    let friend = Player { uuid: offline_uuid("Friend"), name: "Friend".into(), skin: Some(Skin { png: skin([90, 160, 220]), slim: true }) };
    println!("FRIEND {}", friend.uuid);
    let guests: Guests = Arc::new(RwLock::new(HashMap::from([(friend.uuid.clone(), friend)])));
    // The same order as Wisp's own lookup: the host, the guests, then anyone by name as offline.
    let lookup: Lookup = Arc::new(move |who| {
        let mine = match who {
            Who::Uuid(asked) => asked.replace('-', "").eq_ignore_ascii_case(&host.uuid),
            Who::Name(asked) => asked.eq_ignore_ascii_case(&host.name),
        };
        if mine {
            return Some(host.clone());
        }
        let guests = guests.read().unwrap();
        let found = match who {
            Who::Uuid(asked) => guests.get(&asked.replace('-', "").to_lowercase()).cloned(),
            Who::Name(asked) => guests.values().find(|guest| guest.name.eq_ignore_ascii_case(asked)).cloned(),
        };
        found.or_else(|| match who {
            Who::Name(name) if valid_username(name) => Some(Player { uuid: offline_uuid(name), name: name.to_string(), skin: None }),
            _ => None,
        })
    });
    let key = skin_server::load_or_make_key(&std::env::temp_dir().join("wisp-skin-join-smoke-key.der")).expect("key");
    let observe: Observe = Arc::new(|method, path, status| println!("ASKED {method} {path} -> {status}"));
    let server = skin_server::start_observed(key, lookup, Some(observe)).await.expect("server");
    println!("ROOT {}", server.api_root);
    for arg in server.jvm_args(std::path::Path::new(&injector)) {
        println!("ARG {arg}");
    }
    tokio::time::sleep(std::time::Duration::from_secs(90)).await;
}
