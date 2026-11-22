//! A friend joins a host's server end to end, both on this computer: the ask and the answer go
//! through the public Nostr relays as gift wraps (as between two Wisps), then the two open the
//! direct link, the friend reads the host's mod list and plays through the link to a stand-in
//! server. Only the router isn't asked to open anything (that needs two networks to mean much).
//!
//!   cargo run -p wisp-core --example join_smoke

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use nostr::prelude::Keys;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wisp_core::social::{self, ClientNotification, Message, Net, Player, ServerCard, StreamExt};
use wisp_core::tunnel::{Endpoint, OpenOptions, Service};

async fn next_message(keys: &Keys, events: &mut social::Notifications) -> social::Received {
    tokio::time::timeout(Duration::from_secs(30), async {
        while let Some(notification) = events.next().await {
            if let ClientNotification::Event { event, .. } = notification
                && let Some(received) = social::unwrap(keys, &event)
            {
                return received;
            }
        }
        panic!("the relays closed");
    })
    .await
    .expect("nothing arrived within 30 s")
}

#[tokio::main]
async fn main() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let relays: Vec<String> = social::DEFAULT_RELAYS.iter().map(|relay| relay.to_string()).collect();
    let (host_keys, friend_keys) = (Keys::generate(), Keys::generate());
    let (host_net, mut host_events) = Net::connect(host_keys.clone(), &relays).await.expect("host connects");
    let (friend_net, mut friend_events) = Net::connect(friend_keys.clone(), &relays).await.expect("friend connects");
    tokio::time::sleep(Duration::from_secs(3)).await;

    // A stand-in Minecraft server that answers each read with the bytes reversed.
    let server = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let target = server.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = server.accept().await {
            tokio::spawn(async move {
                let mut buffer = [0u8; 64];
                while let Ok(read) = socket.read(&mut buffer).await {
                    if read == 0 {
                        break;
                    }
                    let mut answer = buffer[..read].to_vec();
                    answer.reverse();
                    let _ = socket.write_all(&answer).await;
                }
            });
        }
    });

    let started = Instant::now();
    let options = || OpenOptions { stun: true, upnp: false, loopback: false };
    let friend = Endpoint::open(options()).await.expect("friend's side opens");
    let ask = Message::JoinRequest {
        server_id: "smoke".into(),
        session: "session-1".into(),
        offer: friend.offer(),
        player: Some(Player { name: "Friend".into(), uuid: "0".repeat(32) }),
    };
    friend_net.send(&host_keys.public_key(), &ask).await.expect("friend asks");
    println!("friend asked ({} ways to reach it)", friend.offer().candidates.len());

    let asked = next_message(&host_keys, &mut host_events).await;
    let Message::JoinRequest { session, offer, player, .. } = asked.message else {
        panic!("the host got something else");
    };
    println!("host got the ask after {:.1}s from {}", started.elapsed().as_secs_f64(), player.map(|player| player.name).unwrap_or_default());
    let host = Arc::new(Endpoint::open(options()).await.expect("host's side opens"));
    host.allow(&offer.fingerprint);
    let service = Arc::new(Service::default());
    *service.target.write().unwrap() = Some(target);
    *service.manifest.write().unwrap() = br#"{"serverId":"smoke","files":[]}"#.to_vec();
    {
        let host = host.clone();
        let service = service.clone();
        tokio::spawn(async move {
            while let Some(link) = host.accept().await {
                let service = service.clone();
                tokio::spawn(async move { link.serve(service).await });
            }
        });
    }
    // The host dials the friend too, as the app does.
    {
        let host = host.clone();
        let service = service.clone();
        tokio::spawn(async move {
            if let Ok(link) = host.connect(&offer).await {
                link.serve(service).await;
            }
        });
    }
    let card = ServerCard { id: "smoke".into(), name: "Smoke".into(), game_version: "26.3".into(), loader: "fabric".into(), loader_version: None, online: true, players: 0, max_players: 10, mods: 0, software: None };
    host_net.send(&friend_keys.public_key(), &Message::JoinAnswer { session, offer: Some(host.offer()), server: Some(card), error: None }).await.expect("host answers");

    let answered = next_message(&friend_keys, &mut friend_events).await;
    let Message::JoinAnswer { offer: Some(host_offer), .. } = answered.message else {
        panic!("the friend got something else");
    };
    println!("friend got the answer after {:.1}s", started.elapsed().as_secs_f64());
    friend.allow(&host_offer.fingerprint);
    let link = friend.meet(&host_offer).await.expect("the two meet");
    println!("linked after {:.1}s via {} (round trip {:?})", started.elapsed().as_secs_f64(), link.remote(), link.rtt());
    let manifest = link.manifest().await.expect("mod list");
    println!("mod list: {}", String::from_utf8_lossy(&manifest));
    let (port, _forwarding) = link.forward().await.expect("forward");
    let mut game = tokio::net::TcpStream::connect(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port)).await.unwrap();
    game.write_all(b"minecraft").await.unwrap();
    let mut answer = [0u8; 9];
    game.read_exact(&mut answer).await.unwrap();
    assert_eq!(&answer, b"tfarcenim");
    println!("played through the link: sent \"minecraft\", the server said \"{}\"", String::from_utf8_lossy(&answer));
    link.close();
    host.close().await;
    friend.close().await;
    host_net.shutdown().await;
    friend_net.shutdown().await;
    println!("done in {:.1}s", started.elapsed().as_secs_f64());
}
