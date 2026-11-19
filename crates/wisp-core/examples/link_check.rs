//! Opens one side of a friend link on this computer and prints how it could be reached: its
//! address on the network, how the internet sees it (STUN), and whether the router changes the
//! outside port per destination. The router isn't asked to open anything unless WISP_UPNP=1.
//!
//!   cargo run -p wisp-core --example link_check

use wisp_core::tunnel::{Endpoint, OpenOptions};

#[tokio::main]
async fn main() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let upnp = std::env::var("WISP_UPNP").as_deref() == Ok("1");
    let started = std::time::Instant::now();
    let endpoint = Endpoint::open(OpenOptions { stun: true, upnp, loopback: false }).await.expect("open");
    println!("opened in {:.2}s", started.elapsed().as_secs_f64());
    let offer = endpoint.offer();
    let reach = endpoint.reach();
    println!("candidates: {}", offer.candidates.len());
    println!("found a public address: {}", reach.public.is_some());
    println!("router changes ports per destination (strict): {}", reach.strict_router);
    println!("router opened a way in: {}", reach.router_opened);
    endpoint.close().await;
}
