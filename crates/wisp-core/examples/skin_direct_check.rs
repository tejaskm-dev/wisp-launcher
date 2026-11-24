//! Joining a Wisp-hosted server from Minecraft's own server list, with skins, played out on one
//! computer without the game: a host (its skin server signing with the key tester builds share, its
//! door, and a stand-in Minecraft server) and a friend's Wisp doing what it does when its game
//! connects (hosting::bring_skin in the app). The host's server checks the friend's login while
//! the friend's Wisp is still handing in the skin, as happens for real.
//!
//!   cargo run -p wisp-core --example skin_direct_check

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rsa::pkcs1v15::{Signature, VerifyingKey};
use rsa::pkcs8::DecodePublicKey;
use rsa::signature::Verifier;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use wisp_core::accounts::{offline_uuid, valid_username};
use wisp_core::hosting::pack::Pack;
use wisp_core::skin_server::{self, Guests, Lookup, Player, Skin, Who};
use wisp_core::tunnel::door::Door;
use wisp_core::tunnel::{self, Ask, Keys, Service, Upload};

fn skin(rgb: [u8; 3]) -> Vec<u8> {
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(64, 64, image::Rgba([rgb[0], rgb[1], rgb[2], 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("png");
    png
}

/// A stand-in Minecraft server that answers the server list (status and ping), as the door needs.
async fn stand_in_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let _ = read_packet(&mut stream).await; // handshake
                let _ = read_packet(&mut stream).await; // status request
                let json = r#"{"version":{"name":"26.3","protocol":775},"players":{"max":10,"online":0},"description":"Stand-in"}"#;
                let mut body = vec![0u8];
                varint(json.len() as u32, &mut body);
                body.extend_from_slice(json.as_bytes());
                let mut packet = Vec::new();
                varint(body.len() as u32, &mut packet);
                packet.extend(body);
                let _ = stream.write_all(&packet).await;
                if let Ok(ping) = read_packet(&mut stream).await {
                    let mut back = Vec::new();
                    varint(ping.len() as u32, &mut back);
                    back.extend(ping);
                    let _ = stream.write_all(&back).await;
                }
            });
        }
    });
    address
}

fn varint(mut value: u32, out: &mut Vec<u8>) {
    loop {
        if value & !0x7f == 0 {
            out.push(value as u8);
            return;
        }
        out.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
}

async fn read_packet(stream: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let mut length = 0u32;
    for shift in (0..21).step_by(7) {
        let byte = stream.read_u8().await?;
        length |= u32::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            break;
        }
    }
    let mut body = vec![0; length as usize];
    stream.read_exact(&mut body).await?;
    Ok(body)
}

async fn http_get(port: u16, path: &str) -> (u16, Vec<u8>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream.write_all(format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").as_bytes()).await.unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.unwrap();
    let split = raw.windows(4).position(|window| window == b"\r\n\r\n").unwrap();
    (std::str::from_utf8(&raw[9..12]).unwrap().parse().unwrap(), raw[split + 4..].to_vec())
}

#[tokio::main]
async fn main() {
    let friend_png = skin([90, 160, 220]);

    // ── The host ──
    let key = skin_server::shared_key(b"skin direct check").expect("key");
    let guests: Guests = Arc::new(RwLock::new(HashMap::new()));
    let host = Player { uuid: offline_uuid("Host"), name: "Host".into(), skin: Some(Skin { png: skin([229, 72, 77]), slim: false }) };
    let lookup: Lookup = {
        let guests = guests.clone();
        Arc::new(move |who| {
            let mine = match who {
                Who::Uuid(asked) => asked.replace('-', "").eq_ignore_ascii_case(&host.uuid),
                Who::Name(asked) | Who::Joining(asked) => asked.eq_ignore_ascii_case(&host.name),
            };
            if mine {
                return Some(host.clone());
            }
            let guests = guests.read().unwrap();
            let found = match who {
                Who::Uuid(asked) => guests.get(&asked.replace('-', "").to_lowercase()).cloned(),
                Who::Name(asked) | Who::Joining(asked) => guests.values().find(|guest| guest.name.eq_ignore_ascii_case(asked)).cloned(),
            };
            found.or_else(|| match who {
                Who::Name(name) if valid_username(name) => Some(Player { uuid: offline_uuid(name), name: name.to_string(), skin: None }),
                _ => None,
            })
        })
    };
    let skins = skin_server::start_with(key, lookup, None, None, skin_server::HAND_IN_WAIT).await.expect("skin server");
    let service = Arc::new(Service::default());
    *service.skins.write().unwrap() = Some(SocketAddr::from(([127, 0, 0, 1], skins.port)));
    *service.skin_key.write().unwrap() = Some(skins.public_key.clone());
    let pack = Pack { server_id: "check".into(), name: "Check".into(), game_version: "26.3".into(), loader: "vanilla".into(), loader_version: None, files: Vec::new(), skin_port: Some(skins.port) };
    *service.manifest.write().unwrap() = serde_json::to_vec(&pack).unwrap();
    {
        let guests = guests.clone();
        *service.on_player.write().unwrap() = Some(Arc::new(move |upload: Upload| {
            let skin = upload.png.as_deref().and_then(|png| STANDARD.decode(png).ok()).filter(|png| skin_server::valid_skin(png)).map(|png| Skin { png, slim: upload.slim });
            guests.write().unwrap().insert(upload.uuid.clone(), Player { uuid: upload.uuid, name: upload.name, skin });
        }));
    }
    *service.on_ask.write().unwrap() = Some(Arc::new(|_: &Ask| true));
    let door = Door::open(0, stand_in_server().await, &Keys::new().unwrap(), service).await.expect("door");
    let address = format!("127.0.0.1:{}", door.port());
    println!("Host: skin server on {}, door on {address}", skins.port);

    // ── The friend's game connects; its server checks the login straight away ──
    let started = Instant::now();
    let login = tokio::spawn({
        let port = skins.port;
        async move { http_get(port, "/sessionserver/session/minecraft/hasJoined?username=Friend&serverId=check").await }
    });

    // ── Meanwhile the friend's Wisp, as hosting::bring_skin does ──
    let status = wisp_core::servers::status(&address).await;
    assert!(status.online && status.wisp, "the door marks its server list answer");
    assert_eq!(status.wisp_skin_key.as_deref(), Some(skins.public_key.as_str()), "with the host's skin key");
    println!("Friend: the server list answer has Wisp's mark and the host's skin key");
    let ask = Ask { name: "Friend".into(), uuid: offline_uuid("Friend") };
    let (link, _) = tunnel::ask_door(&address, &Keys::new().unwrap(), &ask).await.expect("let in");
    let pack: Pack = serde_json::from_slice(&link.manifest().await.expect("list")).expect("pack");
    let host_port = pack.skin_port.expect("the host's skin server port");
    let upload = Upload { name: "Friend".into(), uuid: offline_uuid("Friend"), slim: true, png: Some(STANDARD.encode(&friend_png)) };
    assert!(link.hand_in(&upload).await.expect("hand in"), "the host took the skin");
    // On another computer this is the host's own port; here the host's skin server holds it, so
    // the way in gets another.
    let (forwarded, _task) = link.forward_skins(host_port).await.expect("forward");
    println!("Friend: handed in the skin {} ms after connecting; skins come through port {forwarded}", started.elapsed().as_millis());

    // ── The login finishes with the friend's skin, signed with the shared key ──
    let (status, body) = login.await.unwrap();
    let waited = started.elapsed();
    assert_eq!(status, 200);
    let profile: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(profile["name"], "Friend");
    let property = &profile["properties"][0];
    let value = property["value"].as_str().unwrap();
    let signature = Signature::try_from(STANDARD.decode(property["signature"].as_str().unwrap()).unwrap().as_slice()).unwrap();
    let shared = skin_server::shared_key(b"skin direct check").unwrap();
    let trusted = rsa::RsaPublicKey::from_public_key_der(&STANDARD.decode(skin_server::public_der(&shared).unwrap()).unwrap()).unwrap();
    VerifyingKey::<sha1_10::Sha1>::new(trusted).verify(value.as_bytes(), &signature).expect("signed with the shared key, so the friend's game trusts it");
    let textures: Value = serde_json::from_slice(&STANDARD.decode(value).unwrap()).unwrap();
    assert_eq!(textures["textures"]["SKIN"]["metadata"]["model"], "slim");
    println!("Host's server: the login waited {} ms and got the friend's skin, signed with the shared key", waited.as_millis());

    // ── The friend's game fetches the skin through the link ──
    let url = textures["textures"]["SKIN"]["url"].as_str().unwrap();
    let path = url.split_once(&format!("127.0.0.1:{host_port}")).expect("the host's own port").1;
    let (status, png) = http_get(forwarded, path).await;
    assert_eq!((status, png == friend_png), (200, true), "the skin picture comes through the link");
    println!("Friend's game: fetched its skin picture through the link ({} bytes)", friend_png.len());
    link.close();
    door.close();
    assert!(waited < Duration::from_secs(6), "well inside the host's wait");
    println!("ok: joining from Minecraft's own list brings the skin, and everyone's game trusts it");
}
