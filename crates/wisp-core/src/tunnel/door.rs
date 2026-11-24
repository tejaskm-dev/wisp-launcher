//! A server's front door: the port players join on. Wisp listens there itself and passes each
//! game's connection through to the server, which listens on a port of its own on this computer.
//!
//! Another Wisp knocks differently. Its connection opens like a game's, with a Minecraft
//! handshake (so it gets through anything that forwards Minecraft, playit's tunnels and proxies
//! that route by the address included), but the address in it ends in a mark no game sends. Then
//! the connection turns into TLS, each side checking the other's certificate as on the direct
//! link, and on it runs one of the link's streams. So a friend whose router stops the direct link
//! still joins with everything, the mods and their skin, through whatever address the server has.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::ServerName;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::client::TlsStream;

use super::{Keys, Service, TunnelError, answer, answer_stranger, fingerprint_of, provider};
use crate::servers::{ServerAddress, resolve};

/// The link's protocol through a door, as both sides name it in TLS.
pub const ALPN: &[u8] = b"wisp-door/1";
/// What another Wisp puts at the end of the address in its handshake.
const MARK: &str = "\0WISP1";
/// The protocol number in that handshake (any would do: no server reads it).
const PROTOCOL: u32 = 773;
/// The longest first packet read before deciding: a handshake is far shorter.
const LONGEST_FIRST: usize = 1024;
/// How long a connection gets to say what it is.
const FIRST_WITHIN: Duration = Duration::from_secs(10);
const CONNECT_WITHIN: Duration = Duration::from_secs(6);
const TLS_WITHIN: Duration = Duration::from_secs(10);

/// The door on a server's port, while the server runs.
pub struct Door {
    port: u16,
    task: tokio::task::JoinHandle<()>,
}

impl Door {
    /// Opens the door on `port` (on all of this computer's addresses). Games go through to the
    /// server at `server`; other Wisps that `keys` let in get the link's streams from `service`.
    pub async fn open(port: u16, server: SocketAddr, keys: &Keys, service: Arc<Service>) -> Result<Self, TunnelError> {
        let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).await?;
        let port = listener.local_addr()?.port();
        let tls = Arc::new(keys.door_tls(&provider())?);
        let keys = keys.clone();
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    // Out of connections for a moment, say: wait rather than spin.
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                };
                let (tls, service, keys) = (tls.clone(), service.clone(), keys.clone());
                tokio::spawn(async move {
                    let _ = take(stream, server, tls, &service, &keys).await;
                });
            }
        });
        Ok(Self { port, task })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn close(&self) {
        self.task.abort();
    }
}

impl Drop for Door {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// One connection at the door: a game's goes through to the server as it came; another Wisp's
/// gets its stream.
async fn take(mut stream: TcpStream, server: SocketAddr, tls: Arc<rustls::ServerConfig>, service: &Service, keys: &Keys) -> Result<(), TunnelError> {
    let _ = stream.set_nodelay(true);
    let Ok(Ok((start, wisp))) = tokio::time::timeout(FIRST_WITHIN, first_packet(&mut stream)).await else {
        return Ok(());
    };
    if !wisp {
        let mut to_server = TcpStream::connect(server).await?;
        let _ = to_server.set_nodelay(true);
        to_server.write_all(&start).await?;
        if next_state(&start) == Some(1) {
            let skin_key = service.skin_key.read().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
            return mark_status(stream, to_server, skin_key).await;
        }
        let _ = tokio::io::copy_bidirectional(&mut stream, &mut to_server).await;
        return Ok(());
    }
    let acceptor = tokio_rustls::TlsAcceptor::from(tls);
    let Ok(Ok(tls)) = tokio::time::timeout(TLS_WITHIN, acceptor.accept(stream)).await else {
        return Ok(());
    };
    let peer = tls.get_ref().1.peer_certificates().and_then(|certificates| certificates.first()).map(|certificate| fingerprint_of(certificate)).unwrap_or_default();
    let (read, write) = tokio::io::split(tls);
    if keys.allows(&peer) {
        answer(write, read, service, &peer).await
    } else {
        answer_stranger(write, read, service, keys, &peer).await
    }
}

/// Reads a connection's first packet, exactly (nothing past it), and says whether another Wisp
/// sent it. Anything that isn't a modern handshake, an old game's ping say, is passed on as it came.
async fn first_packet(stream: &mut TcpStream) -> std::io::Result<(Vec<u8>, bool)> {
    let mut read = Vec::with_capacity(64);
    let first = stream.read_u8().await?;
    read.push(first);
    // Games before 1.7 ping with 0xFE.
    if first == 0xFE {
        return Ok((read, false));
    }
    let mut length = u32::from(first & 0x7f);
    let mut byte = first;
    let mut shift = 7;
    while byte & 0x80 != 0 {
        if shift > 14 {
            return Ok((read, false));
        }
        byte = stream.read_u8().await?;
        read.push(byte);
        length |= u32::from(byte & 0x7f) << shift;
        shift += 7;
    }
    let length = length as usize;
    if length == 0 || length > LONGEST_FIRST {
        return Ok((read, false));
    }
    let body = read.len();
    read.resize(body + length, 0);
    stream.read_exact(&mut read[body..]).await?;
    let wisp = handshake_address(&read[body..]).is_some_and(|address| address.ends_with(MARK));
    Ok((read, wisp))
}

/// A server-list ping through the door: the server's answer gets a `"wisp"` field (games ignore
/// fields they don't know), so a Wisp listing the server knows it's hosted with Wisp, and that
/// Play can bring the server's mods and the player's skin along.
async fn mark_status(client: TcpStream, server: TcpStream, skin_key: Option<String>) -> Result<(), TunnelError> {
    let (mut client_read, mut client_write) = client.into_split();
    let (mut server_read, mut server_write) = server.into_split();
    // The game's status request (and later its ping) go on through as they come.
    let up = tokio::spawn(async move {
        let _ = tokio::io::copy(&mut client_read, &mut server_write).await;
        let _ = server_write.shutdown().await;
    });
    let answer = read_packet(&mut server_read).await?;
    let marked = marked_status(&answer, skin_key.as_deref()).unwrap_or(answer);
    client_write.write_all(&marked).await?;
    let _ = tokio::io::copy(&mut server_read, &mut client_write).await;
    let _ = client_write.shutdown().await;
    up.abort();
    Ok(())
}

/// One whole packet, its length included, as it came.
async fn read_packet(stream: &mut (impl tokio::io::AsyncRead + Unpin)) -> std::io::Result<Vec<u8>> {
    let mut packet = Vec::new();
    let mut length = 0u32;
    for shift in (0..21).step_by(7) {
        let byte = stream.read_u8().await?;
        packet.push(byte);
        length |= u32::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            break;
        }
    }
    // A status answer with a big server icon is tens of kilobytes; nothing needs more.
    let length = (length as usize).min(2 * 1024 * 1024);
    let body = packet.len();
    packet.resize(body + length, 0);
    stream.read_exact(&mut packet[body..]).await?;
    Ok(packet)
}

/// The status answer with the mark added (with the key its skin data is signed with, when it signs
/// players in), or None if it isn't one.
fn marked_status(packet: &[u8], skin_key: Option<&str>) -> Option<Vec<u8>> {
    let mut at = 0;
    read_varint(packet, &mut at)?;
    if read_varint(packet, &mut at)? != 0 {
        return None;
    }
    let length = read_varint(packet, &mut at)? as usize;
    let text = std::str::from_utf8(packet.get(at..at.checked_add(length)?)?).ok()?;
    let mut status: serde_json::Value = serde_json::from_str(text).ok()?;
    let mut mark = serde_json::json!({ "door": 1 });
    if let Some(key) = skin_key {
        mark["skinKey"] = key.into();
    }
    status.as_object_mut()?.insert("wisp".into(), mark);
    let text = serde_json::to_string(&status).ok()?;
    let mut body = Vec::with_capacity(text.len() + 8);
    write_varint(0, &mut body);
    write_varint(text.len() as u32, &mut body);
    body.extend_from_slice(text.as_bytes());
    let mut out = Vec::with_capacity(body.len() + 3);
    write_varint(body.len() as u32, &mut out);
    out.extend(body);
    Some(out)
}

/// What a handshake asks for next: 1 status (the server list), 2 login, 3 a transfer.
fn next_state(packet: &[u8]) -> Option<u32> {
    let mut at = 0;
    read_varint(packet, &mut at)?;
    if read_varint(packet, &mut at)? != 0 {
        return None;
    }
    read_varint(packet, &mut at)?;
    let length = read_varint(packet, &mut at)? as usize;
    at = at.checked_add(length)?.checked_add(2)?;
    read_varint(packet, &mut at)
}

fn read_varint(bytes: &[u8], at: &mut usize) -> Option<u32> {
    let mut value = 0u32;
    for shift in (0..35).step_by(7) {
        let byte = *bytes.get(*at)?;
        *at += 1;
        value |= u32::from(byte & 0x7f).checked_shl(shift).unwrap_or(0);
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

fn write_varint(mut value: u32, out: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// The server address in a handshake packet's body.
fn handshake_address(body: &[u8]) -> Option<String> {
    let mut at = 0;
    if read_varint(body, &mut at)? != 0 {
        return None;
    }
    read_varint(body, &mut at)?;
    let length = read_varint(body, &mut at)? as usize;
    let bytes = body.get(at..at.checked_add(length)?)?;
    String::from_utf8(bytes.to_vec()).ok()
}

/// The handshake another Wisp opens with: to `host` (as it was typed, since proxies route by it)
/// on `port`, marked.
fn preamble(host: &str, port: u16) -> Vec<u8> {
    let address: String = host.chars().take(200).collect::<String>() + MARK;
    let mut body = Vec::with_capacity(address.len() + 16);
    write_varint(0, &mut body);
    write_varint(PROTOCOL, &mut body);
    write_varint(address.len() as u32, &mut body);
    body.extend_from_slice(address.as_bytes());
    body.extend_from_slice(&port.to_be_bytes());
    // "Login", as a game joining would say.
    write_varint(2, &mut body);
    let mut packet = Vec::with_capacity(body.len() + 3);
    write_varint(body.len() as u32, &mut packet);
    packet.extend(body);
    packet
}

/// How a friend's Wisp opens connections through a host's door.
pub(crate) struct Dialer {
    address: SocketAddr,
    /// The host as it was given, for the handshake.
    host: String,
    tls: Arc<rustls::ClientConfig>,
}

impl Dialer {
    /// Finds the door at `text` (`host` or `host:port`, looked up the way the game does), and
    /// opens a first connection through it, which shows it's the Wisp `tls` expects.
    pub(crate) async fn knock(text: &str, tls: Arc<rustls::ClientConfig>) -> Result<(Self, TlsStream<TcpStream>), TunnelError> {
        let parsed = ServerAddress::parse(text).map_err(|_| TunnelError::Unreachable)?;
        let (target, port) = resolve(&parsed).await;
        let mut found: Vec<SocketAddr> = tokio::time::timeout(CONNECT_WITHIN, tokio::net::lookup_host((target.as_str(), port))).await.map_err(|_| TunnelError::Unreachable)??.collect();
        // IPv4 first: tunnels and routers' openings are nearly always there.
        found.sort_by_key(|address| !address.is_ipv4());
        for address in found.into_iter().take(3) {
            let dialer = Self { address, host: parsed.host.clone(), tls: tls.clone() };
            if let Ok(stream) = dialer.dial().await {
                return Ok((dialer, stream));
            }
        }
        Err(TunnelError::Unreachable)
    }

    pub(crate) fn address(&self) -> SocketAddr {
        self.address
    }

    /// The same door, held to another TLS setup (the certificate met the first time).
    pub(crate) fn holding(self, tls: Arc<rustls::ClientConfig>) -> Self {
        Self { tls, ..self }
    }

    /// A fresh connection through the door, for one stream.
    pub(crate) async fn dial(&self) -> Result<TlsStream<TcpStream>, TunnelError> {
        let mut stream = tokio::time::timeout(CONNECT_WITHIN, TcpStream::connect(self.address)).await.map_err(|_| TunnelError::Unreachable)?.map_err(|_| TunnelError::Unreachable)?;
        let _ = stream.set_nodelay(true);
        stream.write_all(&preamble(&self.host, self.address.port())).await?;
        let name = ServerName::try_from("wisp").map_err(|error| TunnelError::Setup(error.to_string()))?;
        let connector = tokio_rustls::TlsConnector::from(self.tls.clone());
        // A handshake that fails means it isn't the Wisp described.
        tokio::time::timeout(TLS_WITHIN, connector.connect(name, stream)).await.map_err(|_| TunnelError::Unreachable)?.map_err(|_| TunnelError::Unreachable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_handshake_reads_back_and_a_games_doesnt_carry_the_mark() {
        let packet = preamble("abc.joinmc.link", 25565);
        let mut at = 0;
        let length = read_varint(&packet, &mut at).unwrap() as usize;
        assert_eq!(packet.len(), at + length);
        assert_eq!(handshake_address(&packet[at..]).unwrap(), "abc.joinmc.link\0WISP1");
        // A Forge game's handshake has a mark of its own, which isn't Wisp's.
        let mut forge = Vec::new();
        write_varint(0, &mut forge);
        write_varint(767, &mut forge);
        let address = "play.example.net\0FML3\0";
        write_varint(address.len() as u32, &mut forge);
        forge.extend_from_slice(address.as_bytes());
        forge.extend_from_slice(&25565u16.to_be_bytes());
        write_varint(2, &mut forge);
        let read = handshake_address(&forge).unwrap();
        assert_eq!(read, address);
        assert!(!read.ends_with(MARK));
        // A game's own handshake for the server list, and the answer marked.
        let mut status = Vec::new();
        write_varint(0, &mut status);
        write_varint(767, &mut status);
        write_varint(9, &mut status);
        status.extend_from_slice(b"localhost");
        status.extend_from_slice(&25565u16.to_be_bytes());
        write_varint(1, &mut status);
        let mut handshake = Vec::new();
        write_varint(status.len() as u32, &mut handshake);
        handshake.extend(status);
        assert_eq!(next_state(&handshake), Some(1));
        assert_eq!(next_state(&preamble("x", 1)), Some(2));
        let json = r#"{"version":{"name":"1.21.4","protocol":769},"description":"Hi"}"#;
        let mut body = Vec::new();
        write_varint(0, &mut body);
        write_varint(json.len() as u32, &mut body);
        body.extend_from_slice(json.as_bytes());
        let mut answer = Vec::new();
        write_varint(body.len() as u32, &mut answer);
        answer.extend(body);
        let marked = marked_status(&answer, Some("KEY")).unwrap();
        let mut at = 0;
        read_varint(&marked, &mut at).unwrap();
        read_varint(&marked, &mut at).unwrap();
        let length = read_varint(&marked, &mut at).unwrap() as usize;
        let value: serde_json::Value = serde_json::from_slice(&marked[at..at + length]).unwrap();
        assert_eq!(value["wisp"]["door"], 1);
        assert_eq!(value["wisp"]["skinKey"], "KEY");
        assert_eq!(value["description"], "Hi");
        let mut value = Vec::new();
        write_varint(300, &mut value);
        assert_eq!(value, [0xAC, 0x02]);
        assert_eq!(read_varint(&value, &mut 0), Some(300));
        assert_eq!(read_varint(&[0x80, 0x80, 0x80, 0x80, 0x80, 0x01], &mut 0), None);
    }
}
