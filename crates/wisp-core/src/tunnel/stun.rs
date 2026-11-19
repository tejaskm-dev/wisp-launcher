//! How this computer looks from the internet: a STUN binding request (RFC 5389) to a public STUN
//! server, sent from the same socket the link will use, so the address that comes back is the one
//! the router gave that socket.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use rand_core::RngCore;

/// Public STUN servers anyone may use; asking two shows whether the router hands out the same
/// outside port to every destination (most do, and then a direct link works).
pub const SERVERS: [&str; 3] = ["stun.l.google.com:19302", "stun.cloudflare.com:3478", "stun1.l.google.com:19302"];
const MAGIC: u32 = 0x2112_A442;
const ANSWER_WITHIN: Duration = Duration::from_millis(1500);

/// A binding request with this transaction id.
pub fn request(transaction: &[u8; 12]) -> [u8; 20] {
    let mut packet = [0u8; 20];
    packet[..2].copy_from_slice(&0x0001u16.to_be_bytes());
    packet[4..8].copy_from_slice(&MAGIC.to_be_bytes());
    packet[8..].copy_from_slice(transaction);
    packet
}

/// The address a binding response reports, if `bytes` is the answer to `transaction`.
pub fn parse_response(bytes: &[u8], transaction: &[u8; 12]) -> Option<SocketAddr> {
    if bytes.len() < 20 || bytes[..2] != 0x0101u16.to_be_bytes() || bytes[4..8] != MAGIC.to_be_bytes() || bytes[8..20] != transaction[..] {
        return None;
    }
    let length = usize::from(u16::from_be_bytes([bytes[2], bytes[3]]));
    let body = bytes.get(20..20 + length)?;
    let mut offset = 0;
    let mut plain = None;
    while offset + 4 <= body.len() {
        let kind = u16::from_be_bytes([body[offset], body[offset + 1]]);
        let size = usize::from(u16::from_be_bytes([body[offset + 2], body[offset + 3]]));
        let value = body.get(offset + 4..offset + 4 + size)?;
        match kind {
            0x0020 => return address(value, Some(transaction)),
            0x0001 => plain = address(value, None),
            _ => {}
        }
        offset += 4 + size.div_ceil(4) * 4;
    }
    plain
}

/// A (XOR-)MAPPED-ADDRESS value.
fn address(value: &[u8], xor: Option<&[u8; 12]>) -> Option<SocketAddr> {
    let family = *value.get(1)?;
    let mut port = u16::from_be_bytes([*value.get(2)?, *value.get(3)?]);
    let magic = MAGIC.to_be_bytes();
    if xor.is_some() {
        port ^= u16::from_be_bytes([magic[0], magic[1]]);
    }
    match family {
        0x01 => {
            let mut ip: [u8; 4] = value.get(4..8)?.try_into().ok()?;
            if xor.is_some() {
                for (byte, mask) in ip.iter_mut().zip(magic) {
                    *byte ^= mask;
                }
            }
            Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::from(ip)), port))
        }
        0x02 => {
            let mut ip: [u8; 16] = value.get(4..20)?.try_into().ok()?;
            if let Some(transaction) = xor {
                let mask: Vec<u8> = magic.iter().chain(transaction.iter()).copied().collect();
                for (byte, mask) in ip.iter_mut().zip(mask) {
                    *byte ^= mask;
                }
            }
            Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::from(ip)), port))
        }
        _ => None,
    }
}

/// What the internet sees of this socket, asked of two STUN servers. Two different answers mean
/// the router changes the outside port per destination (a "symmetric" NAT), where a direct link
/// rarely works.
pub struct Seen {
    pub addresses: Vec<SocketAddr>,
}

impl Seen {
    pub fn public(&self) -> Option<SocketAddr> {
        self.addresses.first().copied()
    }

    pub fn symmetric(&self) -> bool {
        self.addresses.windows(2).any(|pair| pair[0] != pair[1])
    }
}

/// Asks STUN servers (over IPv4) what address this socket has outside the router: all at once,
/// keeping the first two answers that come back.
pub async fn ask(socket: &tokio::net::UdpSocket) -> Seen {
    let lookup = |server: &'static str| async move {
        let found = tokio::time::timeout(Duration::from_secs(2), tokio::net::lookup_host(server)).await.ok()?.ok()?;
        found.into_iter().find(SocketAddr::is_ipv4)
    };
    let (first, second, third) = tokio::join!(lookup(SERVERS[0]), lookup(SERVERS[1]), lookup(SERVERS[2]));
    let mut waiting: Vec<(SocketAddr, [u8; 12])> = Vec::new();
    for target in [first, second, third].into_iter().flatten() {
        let mut transaction = [0u8; 12];
        rand_core::OsRng.fill_bytes(&mut transaction);
        if socket.send_to(&request(&transaction), target).await.is_ok() {
            waiting.push((target, transaction));
        }
    }
    let mut addresses = Vec::new();
    let _ = tokio::time::timeout(ANSWER_WITHIN, async {
        let mut buffer = [0u8; 576];
        while addresses.len() < 2 && !waiting.is_empty() {
            let Ok((size, from)) = socket.recv_from(&mut buffer).await else {
                return;
            };
            let Some(index) = waiting.iter().position(|(target, _)| *target == from) else {
                continue;
            };
            if let Some(seen) = parse_response(&buffer[..size], &waiting[index].1) {
                addresses.push(seen);
                waiting.remove(index);
            }
        }
    })
    .await;
    Seen { addresses }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_xor_mapped_addresses() {
        let transaction = [7u8; 12];
        let packet = request(&transaction);
        assert_eq!(&packet[..2], &[0, 1]);
        assert_eq!(&packet[4..8], &[0x21, 0x12, 0xA4, 0x42]);

        // A response saying 203.0.113.5:51234, XORed the way RFC 5389 does it.
        let ip = [203u8, 0, 113, 5];
        let magic = MAGIC.to_be_bytes();
        let port = 51234u16 ^ u16::from_be_bytes([magic[0], magic[1]]);
        let mut value = vec![0, 1];
        value.extend(port.to_be_bytes());
        value.extend(ip.iter().zip(magic).map(|(byte, mask)| byte ^ mask));
        let mut response = vec![0x01, 0x01];
        // An attribute to skip first (SOFTWARE, 5 bytes padded to 8), then the address.
        let mut body = vec![0x80, 0x22, 0, 5, b'w', b'i', b's', b'p', b'!', 0, 0, 0];
        body.extend([0x00, 0x20, 0, value.len() as u8]);
        body.extend(&value);
        response.extend((body.len() as u16).to_be_bytes());
        response.extend(magic);
        response.extend(transaction);
        response.extend(&body);
        assert_eq!(parse_response(&response, &transaction), Some("203.0.113.5:51234".parse().unwrap()));
        // Someone else's answer, or a damaged one, doesn't count.
        assert_eq!(parse_response(&response, &[8u8; 12]), None);
        assert_eq!(parse_response(&response[..30], &transaction), None);

        let seen = Seen { addresses: vec!["203.0.113.5:1".parse().unwrap(), "203.0.113.5:2".parse().unwrap()] };
        assert!(seen.symmetric());
    }
}
