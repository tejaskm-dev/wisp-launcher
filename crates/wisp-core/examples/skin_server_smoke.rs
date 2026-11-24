//! Starts the skin server with a made-up player wearing a plain 64x64 skin and prints what to
//! start Java with, for trying it against authlib-injector by hand, then every question it's
//! asked. Stops after a minute.
//!
//!   cargo run -p wisp-core --example skin_server_smoke -- <path to authlib-injector.jar>

use std::sync::Arc;

use wisp_core::skin_server::{self, Lookup, Observe, Player, Skin, Who};

#[tokio::main]
async fn main() {
    let injector = std::env::args().nth(1).expect("the authlib-injector jar");
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(64, 64, image::Rgba([229, 72, 77, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("png");
    // An offline account's id, as Wisp makes them. (authlib-injector sends ids with another
    // version bit set to Mojang instead, so a made-up one can miss this server.)
    let uuid = wisp_core::accounts::offline_uuid("Wisp").replace('-', "");
    println!("UUID {uuid}");
    let lookup: Lookup = Arc::new(move |who| match who {
        Who::Uuid(asked) if asked.replace('-', "") == uuid => Some(player(&uuid, &png)),
        Who::Name("Wisp") | Who::Joining("Wisp") => Some(player(&uuid, &png)),
        _ => None,
    });
    let key = skin_server::load_or_make_key(&std::env::temp_dir().join("wisp-skin-smoke-key.der")).expect("key");
    let observe: Observe = Arc::new(|method, path, status| println!("ASKED {method} {path} -> {status}"));
    let server = skin_server::start_observed(key, lookup, Some(observe)).await.expect("server");
    println!("ROOT {}", server.api_root);
    for arg in server.jvm_args(std::path::Path::new(&injector)) {
        println!("ARG {arg}");
    }
    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
}

fn player(uuid: &str, png: &[u8]) -> Player {
    Player { uuid: uuid.into(), name: "Wisp".into(), skin: Some(Skin { png: png.to_vec(), slim: true }) }
}
