//! Starts the skin server with a made-up player wearing a plain 64x64 skin and prints what to
//! start Java with, for trying it against authlib-injector by hand. Stops after a minute.
//!
//!   cargo run -p wisp-core --example skin_server_smoke -- <path to authlib-injector.jar>

use std::sync::Arc;

use wisp_core::skin_server::{self, Lookup, Player, Skin, Who};

#[tokio::main]
async fn main() {
    let injector = std::env::args().nth(1).expect("the authlib-injector jar");
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(64, 64, image::Rgba([229, 72, 77, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("png");
    let lookup: Lookup = Arc::new(move |who| match who {
        Who::Uuid(uuid) if uuid.replace('-', "") == "0123456789abcdef0123456789abcdef" => Some(player(&png)),
        Who::Name("Wisp") => Some(player(&png)),
        _ => None,
    });
    let key = skin_server::load_or_make_key(&std::env::temp_dir().join("wisp-skin-smoke-key.der")).expect("key");
    let server = skin_server::start(key, lookup).await.expect("server");
    println!("ROOT {}", server.api_root);
    for arg in server.jvm_args(std::path::Path::new(&injector)) {
        println!("ARG {arg}");
    }
    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
}

fn player(png: &[u8]) -> Player {
    Player { uuid: "0123456789abcdef0123456789abcdef".into(), name: "Wisp".into(), skin: Some(Skin { png: png.to_vec(), slim: false }) }
}
