//! Looks up a player's current skin the way the Skins page does.
//!
//!   cargo run -p wisp-core --example skin_check -- <player name>

#[tokio::main]
async fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "jeb_".into());
    let client = wisp_core::http_client().expect("client");
    match wisp_core::skins::fetch_player_skin(&client, &name).await {
        Ok(skin) => println!("ok: {} ({:?} arms), {} bytes of base64 PNG", skin.name, skin.model, skin.png.len()),
        Err(error) => println!("failed: {error} / {error:?}"),
    }
}
