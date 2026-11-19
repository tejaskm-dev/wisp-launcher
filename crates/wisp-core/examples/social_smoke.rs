//! Two throwaway identities become friends over the public relays: a request, an accept and a
//! presence update, each in an encrypted gift wrap. Proves the relays carry Wisp's messages.
//!
//!   cargo run -p wisp-core --example social_smoke

use std::time::Duration;

use nostr::prelude::Keys;
use wisp_core::social::{self, ClientNotification, Message, Net, Profile, Received, Status, StreamExt};

async fn next_message(keys: &Keys, events: &mut social::Notifications) -> Received {
    tokio::time::timeout(Duration::from_secs(30), async {
        while let Some(notification) = events.next().await {
            if let ClientNotification::Event { event, relay_url, .. } = notification
                && let Some(received) = social::unwrap(keys, &event)
            {
                println!("  (via {relay_url})");
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
    let relays: Vec<String> = social::DEFAULT_RELAYS.iter().map(|relay| relay.to_string()).collect();
    let (alice, bob) = (Keys::generate(), Keys::generate());
    let (a, mut a_events) = Net::connect(alice.clone(), &relays).await.expect("alice connects");
    let (b, mut b_events) = Net::connect(bob.clone(), &relays).await.expect("bob connects");
    tokio::time::sleep(Duration::from_secs(4)).await;
    println!("alice: {} relays, bob: {} relays", a.connected().await, b.connected().await);

    let profile = |name: &str| Profile { name: name.into(), face: None };
    a.send(&bob.public_key(), &Message::Request { profile: profile("Alice"), note: Some("hi from the smoke test".into()) })
        .await
        .expect("alice sends");
    let request = next_message(&bob, &mut b_events).await;
    assert_eq!(request.from, alice.public_key());
    println!("bob got {:?}", request.message);

    b.send(&alice.public_key(), &Message::Accept { profile: profile("Bob") }).await.expect("bob replies");
    let accept = next_message(&alice, &mut a_events).await;
    assert_eq!(accept.from, bob.public_key());
    println!("alice got {:?}", accept.message);

    let status = Status::Playing { label: Some("Fabric 26.3".into()), world: Some("Island".into()), since: 0 };
    b.send(&alice.public_key(), &Message::Presence { profile: profile("Bob"), status, hosting: vec![] }).await.expect("bob shares presence");
    let presence = next_message(&alice, &mut a_events).await;
    println!("alice got {:?}", presence.message);

    a.shutdown().await;
    b.shutdown().await;
    println!("ok: request, accept and presence all crossed the relays");
}
