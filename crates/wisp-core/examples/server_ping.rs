//! Pings the public servers on Discover (or the addresses given) and prints what they say.
//! `cargo run -p wisp-core --example server_ping [address...]`

#[tokio::main]
async fn main() {
    let given: Vec<String> = std::env::args().skip(1).collect();
    let addresses = if given.is_empty() { wisp_core::servers::public_servers().into_iter().map(|server| server.address).collect() } else { given };
    for status in wisp_core::servers::statuses(addresses).await {
        let motd: String = status.motd.iter().map(|span| span.text.as_str()).collect::<String>().replace('\n', " / ");
        if status.online {
            println!(
                "UP   {:28} {:>6} players  {:>4} ms  {:30}  {}",
                status.address,
                status.players.map_or("?".into(), |count| count.to_string()),
                status.ping_ms.unwrap_or(0),
                status.version.unwrap_or_default().chars().take(30).collect::<String>(),
                motd.chars().take(60).collect::<String>()
            );
        } else {
            println!("DOWN {:28} {}", status.address, status.error.unwrap_or_default());
        }
    }
}
