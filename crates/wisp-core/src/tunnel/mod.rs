//! A direct, encrypted link between two Wisps, so a friend can join a server hosted on this
//! computer with nobody running anything in between and no ports opened by hand.
//!
//! Each side opens a UDP socket, learns how it looks from the internet (`stun`), asks its router to
//! let the other side in where it can (`upnp`), and makes itself a certificate. The two exchange
//! their addresses and the SHA-256 of their certificates through the friends channel (end-to-end
//! encrypted, see `social`), then both send packets at each other so each router opens a way in
//! ("hole punching"), and the friend connects with QUIC. Each side accepts only the certificate it
//! was told about, so nobody else can use the link, and everything on it is encrypted.
//!
//! On the link, the friend opens streams: the game's own connection, piped to the server; the
//! list of what the server's instance holds (so the friend's Wisp can match it); and single files
//! from that list that no download site has.
//!
//! When neither router lets the direct link through, the same streams run through the server's
//! own address instead (`door`): a playit tunnel, the router's opened port, or the same network.

pub mod door;
pub mod stun;
pub mod upnp;

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use rustls::DigitallySignedStruct;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use serde::{Deserialize, Serialize};
use sha1::Digest as _;
use sha2::Digest as _;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// The link's protocol, as both sides name it in the TLS handshake.
pub const ALPN: &[u8] = b"wisp-link/1";
const MAX_CANDIDATES: usize = 8;
const MAX_MANIFEST: usize = 16 * 1024 * 1024;
/// How long the friend tries before giving up on reaching the host.
const CONNECT_WITHIN: Duration = Duration::from_secs(20);

#[derive(Debug, thiserror::Error)]
pub enum TunnelError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("couldn't set up the link: {0}")]
    Setup(String),
    #[error("couldn't reach their computer: neither router let the connection through")]
    Unreachable,
    #[error("their Wisp didn't let this computer in")]
    Refused,
    #[error("the link dropped: {0}")]
    Lost(String),
    #[error("they don't have that file")]
    NoSuchFile,
    #[error("the file arrived damaged")]
    Damaged,
}

/// What one side tells the other, through the friends channel, to be reached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    /// Addresses to try, best first: on the same network, through the router's opening, and as
    /// the internet sees this side.
    pub candidates: Vec<String>,
    /// SHA-256 of this side's certificate, hex: the only certificate the other side accepts.
    pub fingerprint: String,
    /// The host's server addresses (`host` or `host:port`), where its door answers when the
    /// direct link can't form: a playit tunnel, the router's opened port, the same network.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub doors: Vec<String>,
}

impl Offer {
    fn addresses(&self) -> Vec<SocketAddr> {
        let mut seen = HashSet::new();
        self.candidates.iter().take(MAX_CANDIDATES).filter_map(|text| text.parse().ok()).filter(|address| seen.insert(*address)).collect()
    }
}

/// How the link could be set up on this side, for telling the player.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reach {
    /// The address the internet sees, from STUN.
    pub public: Option<String>,
    /// The router changes the outside port per destination: direct links rarely work through it.
    pub strict_router: bool,
    /// The router opened a way in (UPnP).
    pub router_opened: bool,
}

pub struct OpenOptions {
    /// Ask STUN servers for the public address (off in tests).
    pub stun: bool,
    /// Ask the router to let the other side in (off in tests).
    pub upnp: bool,
    /// Offer this computer's loopback address too (tests only: both sides on one computer).
    pub loopback: bool,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self { stun: true, upnp: true, loopback: false }
    }
}

struct Identity {
    cert: CertificateDer<'static>,
    key: PrivatePkcs8KeyDer<'static>,
    fingerprint: String,
}

/// Who this side is on links (its certificate) and whom it lets in, shared by the direct link and
/// the server's door, so letting a friend in lets them in either way.
#[derive(Clone)]
pub struct Keys {
    identity: Arc<Identity>,
    allowed: Arc<RwLock<HashSet<String>>>,
}

impl Keys {
    pub fn new() -> Result<Self, TunnelError> {
        Ok(Self { identity: Arc::new(identity()?), allowed: Arc::new(RwLock::new(HashSet::new())) })
    }

    pub fn fingerprint(&self) -> &str {
        &self.identity.fingerprint
    }

    /// Lets the holder of this certificate in (the host does this for each friend it lets in).
    pub fn allow(&self, fingerprint: &str) {
        self.allowed.write().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(fingerprint.to_ascii_lowercase());
    }

    pub fn forbid(&self, fingerprint: &str) {
        self.allowed.write().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&fingerprint.to_ascii_lowercase());
    }

    /// TLS for taking connections: this side's certificate, and only those it let in.
    fn server_tls(&self, provider: &Arc<CryptoProvider>, alpn: &[u8]) -> Result<rustls::ServerConfig, TunnelError> {
        let mut tls = rustls::ServerConfig::builder_with_provider(provider.clone())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|error| TunnelError::Setup(error.to_string()))?
            .with_client_cert_verifier(Arc::new(PinnedClients { allowed: self.allowed.clone(), provider: provider.clone() }))
            .with_single_cert(vec![self.identity.cert.clone()], PrivateKeyDer::Pkcs8(self.identity.key.clone_key()))
            .map_err(|error| TunnelError::Setup(error.to_string()))?;
        tls.alpn_protocols = vec![alpn.to_vec()];
        Ok(tls)
    }

    /// Whether this certificate was let in.
    pub fn allows(&self, fingerprint: &str) -> bool {
        self.allowed.read().unwrap_or_else(|poisoned| poisoned.into_inner()).contains(&fingerprint.to_ascii_lowercase())
    }

    /// TLS for the door: this side's certificate, and any client's (what it may do is checked per
    /// stream).
    fn door_tls(&self, provider: &Arc<CryptoProvider>) -> Result<rustls::ServerConfig, TunnelError> {
        let mut tls = rustls::ServerConfig::builder_with_provider(provider.clone())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|error| TunnelError::Setup(error.to_string()))?
            .with_client_cert_verifier(Arc::new(AnyClient { provider: provider.clone() }))
            .with_single_cert(vec![self.identity.cert.clone()], PrivateKeyDer::Pkcs8(self.identity.key.clone_key()))
            .map_err(|error| TunnelError::Setup(error.to_string()))?;
        tls.alpn_protocols = vec![door::ALPN.to_vec()];
        Ok(tls)
    }

    /// TLS for a first meeting with a server's door: whatever certificate it shows, noted in `seen`.
    fn first_meeting_tls(&self, provider: &Arc<CryptoProvider>, seen: Arc<std::sync::Mutex<Option<String>>>) -> Result<rustls::ClientConfig, TunnelError> {
        let mut tls = rustls::ClientConfig::builder_with_provider(provider.clone())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|error| TunnelError::Setup(error.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(FirstMeeting { seen, provider: provider.clone() }))
            .with_client_auth_cert(vec![self.identity.cert.clone()], PrivateKeyDer::Pkcs8(self.identity.key.clone_key()))
            .map_err(|error| TunnelError::Setup(error.to_string()))?;
        tls.alpn_protocols = vec![door::ALPN.to_vec()];
        Ok(tls)
    }

    /// TLS for connecting: only the certificate the other side promised, showing this side's own.
    fn client_tls(&self, provider: &Arc<CryptoProvider>, fingerprint: &str, alpn: &[u8]) -> Result<rustls::ClientConfig, TunnelError> {
        let mut tls = rustls::ClientConfig::builder_with_provider(provider.clone())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|error| TunnelError::Setup(error.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(PinnedServer { fingerprint: fingerprint.to_ascii_lowercase(), provider: provider.clone() }))
            .with_client_auth_cert(vec![self.identity.cert.clone()], PrivateKeyDer::Pkcs8(self.identity.key.clone_key()))
            .map_err(|error| TunnelError::Setup(error.to_string()))?;
        tls.alpn_protocols = vec![alpn.to_vec()];
        Ok(tls)
    }
}

fn fingerprint_of(cert: &[u8]) -> String {
    crate::launch::hex(sha2::Sha256::digest(cert).as_slice())
}

fn identity() -> Result<Identity, TunnelError> {
    let made = rcgen::generate_simple_self_signed(vec!["wisp".to_string()]).map_err(|error| TunnelError::Setup(error.to_string()))?;
    let cert = made.cert.der().clone();
    let fingerprint = fingerprint_of(&cert);
    Ok(Identity { cert, key: PrivatePkcs8KeyDer::from(made.key_pair.serialize_der()), fingerprint })
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::aws_lc_rs::default_provider())
}

fn transport() -> Arc<quinn::TransportConfig> {
    let mut transport = quinn::TransportConfig::default();
    // Often enough that routers keep the way in open while nobody's talking.
    transport.keep_alive_interval(Some(Duration::from_secs(8)));
    transport.max_idle_timeout(Some(quinn::IdleTimeout::try_from(Duration::from_secs(45)).expect("45 s fits")));
    transport.max_concurrent_bidi_streams(256u32.into());
    Arc::new(transport)
}

/// Checks the host's certificate against the fingerprint it sent.
#[derive(Debug)]
struct PinnedServer {
    fingerprint: String,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for PinnedServer {
    fn verify_server_cert(&self, end_entity: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: &ServerName<'_>, _: &[u8], _: UnixTime) -> Result<ServerCertVerified, rustls::Error> {
        if fingerprint_of(end_entity) == self.fingerprint {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General("not the Wisp we were told about".into()))
        }
    }

    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

/// Lets in only friends whose certificates the host was told about.
#[derive(Debug)]
struct PinnedClients {
    allowed: Arc<RwLock<HashSet<String>>>,
    provider: Arc<CryptoProvider>,
}

impl ClientCertVerifier for PinnedClients {
    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        &[]
    }

    fn verify_client_cert(&self, end_entity: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: UnixTime) -> Result<ClientCertVerified, rustls::Error> {
        let allowed = self.allowed.read().unwrap_or_else(|poisoned| poisoned.into_inner());
        if allowed.contains(&fingerprint_of(end_entity)) {
            Ok(ClientCertVerified::assertion())
        } else {
            Err(rustls::Error::General("not someone this server let in".into()))
        }
    }

    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

/// Takes any certificate a client proves it holds (the door: whether it may do anything is up to
/// what it asks, see `door`).
#[derive(Debug)]
struct AnyClient {
    provider: Arc<CryptoProvider>,
}

impl ClientCertVerifier for AnyClient {
    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        &[]
    }

    fn verify_client_cert(&self, _: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: UnixTime) -> Result<ClientCertVerified, rustls::Error> {
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

/// Meeting a server's door for the first time, by its address: takes the certificate it shows and
/// remembers which, so every later connection holds it to that one.
#[derive(Debug)]
struct FirstMeeting {
    seen: Arc<std::sync::Mutex<Option<String>>>,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for FirstMeeting {
    fn verify_server_cert(&self, end_entity: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: &ServerName<'_>, _: &[u8], _: UnixTime) -> Result<ServerCertVerified, rustls::Error> {
        *self.seen.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(fingerprint_of(end_entity));
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

/// This computer's address on its network: the one it would use to reach the internet. No packet
/// is sent to find it.
pub fn lan_address() -> Option<Ipv4Addr> {
    let socket = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(1, 1, 1, 1), 80)).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

/// One side of a link: a QUIC endpoint on a UDP socket the routers have been asked about.
pub struct Endpoint {
    endpoint: quinn::Endpoint,
    /// The same socket, for the hole-punching packets.
    punch: std::net::UdpSocket,
    keys: Keys,
    candidates: Vec<SocketAddr>,
    mapping: Option<upnp::Mapping>,
    reach: Reach,
    provider: Arc<CryptoProvider>,
}

impl Endpoint {
    /// Opens a socket, finds the ways it can be reached, and listens on it.
    pub async fn open(options: OpenOptions) -> Result<Self, TunnelError> {
        Self::open_with(options, Keys::new()?).await
    }

    /// The same, as `keys` (a server's door shares them, so a friend let in can use either).
    pub async fn open_with(options: OpenOptions, keys: Keys) -> Result<Self, TunnelError> {
        let provider = provider();
        let std_socket = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        std_socket.set_nonblocking(true)?;
        let port = std_socket.local_addr()?.port();
        let socket = tokio::net::UdpSocket::from_std(std_socket)?;
        let mut reach = Reach::default();
        let mut candidates = Vec::new();
        if options.loopback {
            candidates.push(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port));
        }
        let lan = lan_address();
        if let Some(lan) = lan {
            candidates.push(SocketAddr::new(lan.into(), port));
        }
        let mapping = match lan.filter(|_| options.upnp) {
            Some(lan) => upnp::map(upnp::Protocol::UDP, SocketAddr::new(lan.into(), port), "Wisp friend link").await,
            None => None,
        };
        if let Some(mapping) = &mapping {
            reach.router_opened = true;
            candidates.push(mapping.external());
        }
        if options.stun {
            let seen = stun::ask(&socket).await;
            reach.strict_router = seen.symmetric();
            if let Some(public) = seen.public() {
                reach.public = Some(public.to_string());
                candidates.push(public);
            }
        }
        let socket = socket.into_std()?;
        let punch = socket.try_clone()?;

        let server_tls = keys.server_tls(&provider, ALPN)?;
        let server_crypto = quinn::crypto::rustls::QuicServerConfig::try_from(server_tls).map_err(|error| TunnelError::Setup(error.to_string()))?;
        let mut server_config = quinn::ServerConfig::with_crypto(Arc::new(server_crypto));
        server_config.transport_config(transport());
        let endpoint = quinn::Endpoint::new(quinn::EndpointConfig::default(), Some(server_config), socket, Arc::new(quinn::TokioRuntime))?;
        let mut seen = HashSet::new();
        candidates.retain(|candidate| seen.insert(*candidate));
        Ok(Self { endpoint, punch, keys, candidates, mapping, reach, provider })
    }

    /// What the other side needs to reach this one.
    pub fn offer(&self) -> Offer {
        Offer { candidates: self.candidates.iter().map(SocketAddr::to_string).collect(), fingerprint: self.keys.fingerprint().to_string(), doors: Vec::new() }
    }

    pub fn reach(&self) -> Reach {
        self.reach.clone()
    }

    /// Lets the holder of this certificate connect (the host does this for each friend it lets in).
    pub fn allow(&self, fingerprint: &str) {
        self.keys.allow(fingerprint);
    }

    pub fn forbid(&self, fingerprint: &str) {
        self.keys.forbid(fingerprint);
    }

    /// Sends a few small packets at the other side for a few seconds, so this side's router expects
    /// its answers. The link ignores them.
    pub fn punch(&self, offer: &Offer) {
        let Ok(socket) = self.punch.try_clone() else {
            return;
        };
        let targets = offer.addresses();
        tokio::spawn(async move {
            for _ in 0..25 {
                for target in &targets {
                    let _ = socket.send_to(&[0x57], target);
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        });
    }

    /// Connects to the other side (the friend's way in), trying its addresses at once and keeping
    /// the first that answers with the certificate it promised.
    pub async fn connect(&self, to: &Offer) -> Result<Link, TunnelError> {
        self.punch(to);
        let client_tls = self.keys.client_tls(&self.provider, &to.fingerprint, ALPN)?;
        let client_crypto = quinn::crypto::rustls::QuicClientConfig::try_from(client_tls).map_err(|error| TunnelError::Setup(error.to_string()))?;
        let mut client_config = quinn::ClientConfig::new(Arc::new(client_crypto));
        client_config.transport_config(transport());

        let mut attempts = tokio::task::JoinSet::new();
        for (index, address) in to.addresses().into_iter().enumerate() {
            let Ok(connecting) = self.endpoint.connect_with(client_config.clone(), address, "wisp") else {
                continue;
            };
            attempts.spawn(async move {
                // Nearer addresses first: the ones further down the list wait a moment, so a
                // friend on the same network connects there rather than out and back in.
                tokio::time::sleep(Duration::from_millis(150 * index as u64)).await;
                let link = Link { way: Way::Direct(connecting.await.map_err(|_| TunnelError::Unreachable)?) };
                // TLS lets this side finish before the host has checked its certificate; the
                // host's hello is what says it was let in.
                match tokio::time::timeout(Duration::from_secs(6), link.hello()).await {
                    Ok(Ok(())) => Ok(link),
                    _ => Err(TunnelError::Refused),
                }
            });
        }
        let mut refused = false;
        let first = tokio::time::timeout(CONNECT_WITHIN, async {
            while let Some(attempt) = attempts.join_next().await {
                match attempt {
                    Ok(Ok(link)) => return Some(link),
                    Ok(Err(TunnelError::Refused)) => refused = true,
                    _ => {}
                }
            }
            None
        })
        .await;
        attempts.abort_all();
        match first {
            Ok(Some(link)) => Ok(link),
            _ if refused => Err(TunnelError::Refused),
            _ => Err(TunnelError::Unreachable),
        }
    }

    /// Meets the other side from both directions at once: connects to it, and takes its
    /// connection if it gets through first. A strict router on one side stops a connection
    /// started there, not one started from the other side, so this works whenever either side's
    /// router is relaxed (or opened a way in). Both sides need to have `allow`ed each other.
    pub async fn meet(&self, to: &Offer) -> Result<Link, TunnelError> {
        // This side answers the other's hello on whichever link forms.
        let answering = Arc::new(Service::default());
        let connect = self.connect(to);
        tokio::pin!(connect);
        let deadline = tokio::time::sleep(CONNECT_WITHIN + Duration::from_secs(5));
        tokio::pin!(deadline);
        let mut connect_over = false;
        let mut failure = TunnelError::Unreachable;
        loop {
            tokio::select! {
                result = &mut connect, if !connect_over => match result {
                    Ok(link) => {
                        link.answer_hellos(answering);
                        return Ok(link);
                    }
                    Err(error) => {
                        connect_over = true;
                        failure = error;
                    }
                },
                incoming = self.accept() => {
                    let Some(link) = incoming else {
                        return Err(failure);
                    };
                    link.answer_hellos(answering.clone());
                    if matches!(tokio::time::timeout(Duration::from_secs(6), link.hello()).await, Ok(Ok(()))) {
                        return Ok(link);
                    }
                }
                _ = &mut deadline => return Err(failure),
            }
        }
    }

    /// Reaches the host whichever way works: the direct link (`meet`), and at the same time its
    /// server's door when it named any. The direct link is kept when both form, being no detour.
    pub async fn link_to(&self, to: &Offer) -> Result<Link, TunnelError> {
        if to.doors.is_empty() {
            return self.meet(to).await;
        }
        let direct = self.meet(to);
        let door = self.through_door(to);
        tokio::pin!(direct);
        tokio::pin!(door);
        let mut direct_failed: Option<TunnelError> = None;
        let mut door_failed: Option<TunnelError> = None;
        // Refused says more than unreachable: the host's Wisp was there and said no.
        let worse = |one: TunnelError, other: TunnelError| if matches!(other, TunnelError::Refused) { other } else { one };
        loop {
            tokio::select! {
                result = &mut direct, if direct_failed.is_none() => match result {
                    Ok(link) => return Ok(link),
                    Err(error) => match door_failed.take() {
                        Some(door_error) => return Err(worse(error, door_error)),
                        None => direct_failed = Some(error),
                    },
                },
                result = &mut door, if door_failed.is_none() => match result {
                    Ok(link) => {
                        if direct_failed.is_some() {
                            return Ok(link);
                        }
                        // Give the direct link a moment to form as well.
                        return match tokio::time::timeout(Duration::from_secs(2), &mut direct).await {
                            Ok(Ok(direct)) => {
                                link.close();
                                Ok(direct)
                            }
                            _ => Ok(link),
                        };
                    }
                    Err(error) => match direct_failed.take() {
                        Some(direct_error) => return Err(worse(direct_error, error)),
                        None => door_failed = Some(error),
                    },
                },
            }
        }
    }

    /// Reaches the host through its server's door: every address it named at once (the earlier
    /// ones a moment sooner), keeping the first where the Wisp it described answers.
    pub async fn through_door(&self, to: &Offer) -> Result<Link, TunnelError> {
        let tls = Arc::new(self.keys.client_tls(&self.provider, &to.fingerprint, door::ALPN)?);
        let mut attempts = tokio::task::JoinSet::new();
        for (index, address) in to.doors.iter().take(MAX_CANDIDATES).enumerate() {
            let (tls, address) = (tls.clone(), address.clone());
            attempts.spawn(async move {
                tokio::time::sleep(Duration::from_millis(250 * index as u64)).await;
                let started = std::time::Instant::now();
                // Reaching it and the TLS handshake check it's the Wisp described; its answer to
                // the hello says it let this side in.
                let (dialer, stream) = door::Dialer::knock(&address, tls).await?;
                match tokio::time::timeout(Duration::from_secs(8), hello_over(stream)).await {
                    // Three round trips: the connection, TLS, the hello.
                    Ok(Ok(())) => Ok(Link { way: Way::Door(Arc::new(Doorway { dialer, rtt: started.elapsed() / 3, done: tokio::sync::watch::Sender::new(false) })) }),
                    _ => Err(TunnelError::Refused),
                }
            });
        }
        let mut failure = TunnelError::Unreachable;
        while let Some(attempt) = attempts.join_next().await {
            match attempt {
                Ok(Ok(link)) => {
                    attempts.abort_all();
                    return Ok(link);
                }
                Ok(Err(TunnelError::Refused)) => failure = TunnelError::Refused,
                _ => {}
            }
        }
        Err(failure)
    }

    /// The next friend to connect (host side), once their certificate checks out.
    pub async fn accept(&self) -> Option<Link> {
        loop {
            let incoming = self.endpoint.accept().await?;
            if let Ok(connection) = incoming.await {
                return Some(Link { way: Way::Direct(connection) });
            }
        }
    }

    /// Closes the link's socket and takes the router's opening away.
    pub async fn close(&self) {
        self.endpoint.close(0u32.into(), b"bye");
        if let Some(mapping) = &self.mapping {
            mapping.remove().await;
        }
    }

    /// Keeps the router's opening (renew it before its lease runs out).
    pub async fn renew(&self) {
        if let Some(mapping) = &self.mapping {
            mapping.renew().await;
        }
    }
}

const HELLO: u8 = 0;
const PLAY: u8 = 1;
const MANIFEST: u8 = 2;
const FILE: u8 = 3;
const SKINS: u8 = 4;
const PLAYER: u8 = 5;
/// Asking to join a server by its address, through its door (not through friends).
const ASK: u8 = 6;
const MAX_ASK: usize = 4 * 1024;
/// The largest player (name, UUID, skin) a friend may hand in.
const MAX_UPLOAD: usize = 128 * 1024;
/// What the host says to a hello: the link's version.
const VERSION: u8 = 1;

/// A player a friend hands in to the host: who they are in the game, and the skin they wear.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Upload {
    pub name: String,
    /// 32 hex digits.
    pub uuid: String,
    /// Slim arms (Alex's model).
    pub slim: bool,
    /// The skin, a base64 PNG.
    pub png: Option<String>,
}

/// Told about each player handed in, once checked.
pub type OnPlayer = Arc<dyn Fn(Upload) + Send + Sync>;

/// Who asks to join a server by its address: who they play as.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ask {
    pub name: String,
    /// 32 hex digits.
    pub uuid: String,
}

/// Decides who joining by address gets in (true lets them in, mods, skin and all).
pub type OnAsk = Arc<dyn Fn(&Ask) -> bool + Send + Sync>;

/// What the host lets friends on the link reach: the server, the list of what its instance holds,
/// those files, and (in builds where players sign in through Wisp) its skin server.
#[derive(Default)]
pub struct Service {
    /// Where the game's connections go (the server, on this computer).
    pub target: RwLock<Option<SocketAddr>>,
    /// The instance's list, as JSON.
    pub manifest: RwLock<Vec<u8>>,
    /// Files friends may fetch, by SHA-1.
    pub files: RwLock<HashMap<String, PathBuf>>,
    /// The host's skin server, where friends' games sign in and fetch skins.
    pub skins: RwLock<Option<SocketAddr>>,
    /// Which player each friend may hand in: their certificate's fingerprint → their UUID.
    pub players: RwLock<HashMap<String, String>>,
    pub on_player: RwLock<Option<OnPlayer>>,
    /// Joining by address: whether to let someone in. Without one, nobody is.
    pub on_ask: RwLock<Option<OnAsk>>,
}

/// An open link between two Wisps.
#[derive(Clone)]
pub struct Link {
    way: Way,
}

#[derive(Clone)]
enum Way {
    /// The direct link: one QUIC connection, a stream for each thing.
    Direct(quinn::Connection),
    /// Through the server's door: a TLS connection for each thing.
    Door(Arc<Doorway>),
}

struct Doorway {
    dialer: door::Dialer,
    rtt: Duration,
    /// Set once this side is done with the link.
    done: tokio::sync::watch::Sender<bool>,
}

type Reader = Box<dyn AsyncRead + Unpin + Send>;
type Writer = Box<dyn AsyncWrite + Unpin + Send>;

/// Asks over a fresh connection whether the host let this side in.
async fn hello_over<S: AsyncRead + AsyncWrite + Unpin>(mut stream: S) -> Result<(), TunnelError> {
    stream.write_all(&[HELLO]).await?;
    stream.flush().await?;
    match stream.read_u8().await? {
        VERSION => Ok(()),
        _ => Err(TunnelError::Refused),
    }
}

impl Link {
    pub fn remote(&self) -> SocketAddr {
        match &self.way {
            Way::Direct(connection) => connection.remote_address(),
            Way::Door(doorway) => doorway.dialer.address(),
        }
    }

    /// Whether this goes through the server's door rather than straight between the two.
    pub fn through_door(&self) -> bool {
        matches!(self.way, Way::Door(_))
    }

    /// The round trip, as the link measures it.
    pub fn rtt(&self) -> Duration {
        match &self.way {
            Way::Direct(connection) => connection.rtt(),
            Way::Door(doorway) => doorway.rtt,
        }
    }

    pub fn close(&self) {
        match &self.way {
            Way::Direct(connection) => connection.close(0u32.into(), b"bye"),
            Way::Door(doorway) => {
                doorway.done.send_replace(true);
            }
        }
    }

    /// Waits until the link is gone, either side having closed it or it having dropped.
    pub async fn closed(&self) {
        match &self.way {
            Way::Direct(connection) => {
                let _ = connection.closed().await;
            }
            Way::Door(doorway) => {
                let mut watching = doorway.done.subscribe();
                let _ = watching.wait_for(|done| *done).await;
            }
        }
    }

    /// Answers the other side's hellos on this link (the friend's side of `meet`).
    fn answer_hellos(&self, service: Arc<Service>) {
        let link = self.clone();
        tokio::spawn(async move { link.serve(service).await });
    }

    /// The fingerprint of the other side's certificate.
    fn peer_fingerprint(&self) -> Option<String> {
        let Way::Direct(connection) = &self.way else {
            return None;
        };
        let identity = connection.peer_identity()?;
        let certificates = identity.downcast::<Vec<CertificateDer<'static>>>().ok()?;
        certificates.first().map(|certificate| fingerprint_of(certificate))
    }

    /// Host side: answers the friend's streams until the link closes. (Through a door, the door
    /// answers each connection itself.)
    pub async fn serve(&self, service: Arc<Service>) {
        let Way::Direct(connection) = &self.way else {
            return;
        };
        let peer = Arc::new(self.peer_fingerprint().unwrap_or_default());
        while let Ok((send, recv)) = connection.accept_bi().await {
            let service = service.clone();
            let peer = peer.clone();
            tokio::spawn(async move {
                let _ = answer(send, recv, &service, &peer).await;
            });
        }
    }

    /// Friend side: hands the host who this side plays as, and its skin. False when the host
    /// didn't take it.
    pub async fn hand_in(&self, upload: &Upload) -> Result<bool, TunnelError> {
        let bytes = serde_json::to_vec(upload).map_err(|error| TunnelError::Setup(error.to_string()))?;
        let (mut send, mut recv) = self.open().await?;
        send.write_all(&[PLAYER]).await?;
        send.write_u32(bytes.len() as u32).await?;
        send.write_all(&bytes).await?;
        let _ = send.shutdown().await;
        Ok(recv.read_u8().await? == 0)
    }

    /// Friend side: checks the host let this side in (it only answers those it did).
    async fn hello(&self) -> Result<(), TunnelError> {
        let (mut send, mut recv) = self.open().await?;
        send.write_all(&[HELLO]).await?;
        let _ = send.shutdown().await;
        match recv.read_u8().await? {
            VERSION => Ok(()),
            _ => Err(TunnelError::Refused),
        }
    }

    /// Friend side: the host's list of what its instance holds.
    pub async fn manifest(&self) -> Result<Vec<u8>, TunnelError> {
        let (mut send, mut recv) = self.open().await?;
        send.write_all(&[MANIFEST]).await?;
        let _ = send.shutdown().await;
        let size = recv.read_u32().await? as usize;
        if size > MAX_MANIFEST {
            return Err(TunnelError::Lost("the list is too big".into()));
        }
        let mut bytes = vec![0; size];
        recv.read_exact(&mut bytes).await?;
        Ok(bytes)
    }

    /// Friend side: fetches one file from the host by its SHA-1, checked, into `path`.
    pub async fn fetch(&self, sha1: &str, path: &Path) -> Result<u64, TunnelError> {
        if sha1.len() != 40 || !sha1.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(TunnelError::NoSuchFile);
        }
        let (mut send, mut recv) = self.open().await?;
        send.write_all(&[FILE]).await?;
        send.write_all(sha1.to_ascii_lowercase().as_bytes()).await?;
        let _ = send.shutdown().await;
        if recv.read_u8().await? != 0 {
            return Err(TunnelError::NoSuchFile);
        }
        let size = recv.read_u64().await?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let part = path.with_extension("part");
        let mut file = tokio::fs::File::create(&part).await?;
        let mut hasher = sha1::Sha1::new();
        let mut left = size;
        let mut buffer = vec![0u8; 64 * 1024];
        while left > 0 {
            let want = buffer.len().min(usize::try_from(left).unwrap_or(usize::MAX));
            let read = recv.read(&mut buffer[..want]).await.map_err(|error| TunnelError::Lost(error.to_string()))?;
            if read == 0 {
                drop(file);
                let _ = tokio::fs::remove_file(&part).await;
                return Err(TunnelError::Lost("the file stopped part way".into()));
            }
            hasher.update(&buffer[..read]);
            file.write_all(&buffer[..read]).await?;
            left -= read as u64;
        }
        file.flush().await?;
        drop(file);
        if !crate::launch::hex(hasher.finalize().as_slice()).eq_ignore_ascii_case(sha1) {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(TunnelError::Damaged);
        }
        tokio::fs::rename(&part, path).await?;
        Ok(size)
    }

    /// Friend side: listens on this computer for the game, and sends each of its connections over
    /// the link to the server. Returns the port the game should join (on 127.0.0.1).
    pub async fn forward(&self) -> Result<(u16, tokio::task::JoinHandle<()>), TunnelError> {
        self.forward_kind(PLAY, None).await
    }

    /// Friend side: the host's skin server, reachable on this computer at `port` when that's free
    /// (the skin addresses the host hands out name its port), else at another.
    pub async fn forward_skins(&self, port: u16) -> Result<(u16, tokio::task::JoinHandle<()>), TunnelError> {
        self.forward_kind(SKINS, Some(port)).await
    }

    async fn forward_kind(&self, kind: u8, port: Option<u16>) -> Result<(u16, tokio::task::JoinHandle<()>), TunnelError> {
        let listener = match port {
            Some(port) => match TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await {
                Ok(listener) => listener,
                Err(_) => TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?,
            },
            None => TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?,
        };
        let port = listener.local_addr()?.port();
        let link = self.clone();
        let task = tokio::spawn(async move {
            while let Ok((game, _)) = listener.accept().await {
                let link = link.clone();
                tokio::spawn(async move {
                    let Ok((mut send, recv)) = link.open().await else {
                        return;
                    };
                    if send.write_all(&[kind]).await.is_ok() && send.flush().await.is_ok() {
                        pipe(game, send, recv).await;
                    }
                });
            }
        });
        Ok((port, task))
    }

    async fn open(&self) -> Result<(Writer, Reader), TunnelError> {
        match &self.way {
            Way::Direct(connection) => {
                let (send, recv) = connection.open_bi().await.map_err(|error| TunnelError::Lost(error.to_string()))?;
                Ok((Box::new(send), Box::new(recv)))
            }
            Way::Door(doorway) => {
                if *doorway.done.borrow() {
                    return Err(TunnelError::Lost("closed".into()));
                }
                let (read, write) = tokio::io::split(doorway.dialer.dial().await?);
                Ok((Box::new(write), Box::new(read)))
            }
        }
    }
}

/// Host side: one stream from a friend (`peer` is their certificate's fingerprint), on the direct
/// link or through the door.
pub(crate) async fn answer<W: AsyncWrite + Unpin + Send, R: AsyncRead + Unpin + Send>(mut send: W, mut recv: R, service: &Service, peer: &str) -> Result<(), TunnelError> {
    match recv.read_u8().await? {
        SKINS => {
            let target = *service.skins.read().unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(target) = target {
                pipe(TcpStream::connect(target).await?, send, recv).await;
            }
        }
        PLAYER => {
            let size = recv.read_u32().await? as usize;
            if size > MAX_UPLOAD {
                send.write_u8(1).await?;
                let _ = send.shutdown().await;
                return Ok(());
            }
            let mut bytes = vec![0; size];
            recv.read_exact(&mut bytes).await?;
            let taken = match serde_json::from_slice::<Upload>(&bytes) {
                Ok(upload) => {
                    let uuid = upload.uuid.replace('-', "").to_ascii_lowercase();
                    // A friend hands in only themselves: the player they asked to join as.
                    let expected = service.players.read().unwrap_or_else(|poisoned| poisoned.into_inner()).get(peer).cloned();
                    let callback = service.on_player.read().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
                    match callback {
                        Some(callback) if expected.as_deref() == Some(uuid.as_str()) => {
                            callback(Upload { uuid, ..upload });
                            true
                        }
                        _ => false,
                    }
                }
                Err(_) => false,
            };
            send.write_u8(if taken { 0 } else { 1 }).await?;
            let _ = send.shutdown().await;
        }
        HELLO => {
            send.write_u8(VERSION).await?;
            let _ = send.shutdown().await;
        }
        PLAY => {
            let target = *service.target.read().unwrap_or_else(|poisoned| poisoned.into_inner());
            let Some(target) = target else {
                return Ok(());
            };
            let server = TcpStream::connect(target).await?;
            pipe(server, send, recv).await;
        }
        MANIFEST => {
            let bytes = service.manifest.read().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
            send.write_u32(bytes.len() as u32).await?;
            send.write_all(&bytes).await?;
            let _ = send.shutdown().await;
        }
        FILE => {
            let mut hash = [0u8; 40];
            recv.read_exact(&mut hash).await.map_err(|error| TunnelError::Lost(error.to_string()))?;
            let hash = String::from_utf8_lossy(&hash).to_ascii_lowercase();
            let path = service.files.read().unwrap_or_else(|poisoned| poisoned.into_inner()).get(&hash).cloned();
            match path.and_then(|path| std::fs::metadata(&path).ok().map(|meta| (path, meta.len()))) {
                Some((path, size)) => {
                    let mut file = tokio::fs::File::open(path).await?;
                    send.write_u8(0).await?;
                    send.write_u64(size).await?;
                    tokio::io::copy(&mut file, &mut send).await?;
                }
                None => send.write_u8(1).await?,
            }
            let _ = send.shutdown().await;
        }
        _ => {}
    }
    Ok(())
}

/// Host side, at the door: someone not let in yet may only ask to be (joining by address).
pub(crate) async fn answer_stranger<W: AsyncWrite + Unpin + Send, R: AsyncRead + Unpin + Send>(mut send: W, mut recv: R, service: &Service, keys: &Keys, peer: &str) -> Result<(), TunnelError> {
    if recv.read_u8().await? != ASK {
        return Ok(());
    }
    let size = recv.read_u32().await? as usize;
    if size > MAX_ASK {
        return Ok(());
    }
    let mut bytes = vec![0; size];
    recv.read_exact(&mut bytes).await?;
    let let_in = match serde_json::from_slice::<Ask>(&bytes) {
        Ok(ask) if !peer.is_empty() => {
            let ask = Ask { uuid: ask.uuid.replace('-', "").to_ascii_lowercase(), ..ask };
            let decide = service.on_ask.read().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
            let yes = decide.is_some_and(|decide| decide(&ask));
            if yes {
                keys.allow(peer);
                service.players.write().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(peer.to_string(), ask.uuid);
            }
            yes
        }
        _ => false,
    };
    send.write_u8(if let_in { 0 } else { 1 }).await?;
    let _ = send.shutdown().await;
    Ok(())
}

/// Joins a server by its address, through its door: meets it (noting the certificate it shows, so
/// it's held to that one from then on), asks to join as `ask`, and checks it said yes. Fails fast
/// as Unreachable at an address that isn't a Wisp server's door, and as Refused when the server
/// said no. Returns the link and the server's certificate's fingerprint.
pub async fn ask_door(address: &str, keys: &Keys, ask: &Ask) -> Result<(Link, String), TunnelError> {
    let provider = provider();
    let seen = Arc::new(std::sync::Mutex::new(None));
    let first = Arc::new(keys.first_meeting_tls(&provider, seen.clone())?);
    let started = std::time::Instant::now();
    let (dialer, mut stream) = tokio::time::timeout(Duration::from_secs(8), door::Dialer::knock(address, first)).await.map_err(|_| TunnelError::Unreachable)??;
    let fingerprint = seen.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone().ok_or(TunnelError::Unreachable)?;
    let bytes = serde_json::to_vec(ask).map_err(|error| TunnelError::Setup(error.to_string()))?;
    stream.write_all(&[ASK]).await?;
    stream.write_u32(bytes.len() as u32).await?;
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    let answered = tokio::time::timeout(Duration::from_secs(10), stream.read_u8()).await.map_err(|_| TunnelError::Unreachable)?;
    match answered {
        Ok(0) => {}
        Ok(_) => return Err(TunnelError::Refused),
        Err(_) => return Err(TunnelError::Unreachable),
    }
    drop(stream);
    let pinned = Arc::new(keys.client_tls(&provider, &fingerprint, door::ALPN)?);
    let dialer = dialer.holding(pinned);
    let link = Link { way: Way::Door(Arc::new(Doorway { dialer, rtt: started.elapsed() / 4, done: tokio::sync::watch::Sender::new(false) })) };
    link.hello().await.map_err(|_| TunnelError::Refused)?;
    Ok((link, fingerprint))
}

/// Copies bytes both ways between a TCP connection and a stream on the link until both ends are
/// done.
async fn pipe<W: AsyncWrite + Unpin, R: AsyncRead + Unpin>(tcp: TcpStream, mut send: W, mut recv: R) {
    let _ = tcp.set_nodelay(true);
    let (mut tcp_read, mut tcp_write) = tcp.into_split();
    let up = async {
        let _ = tokio::io::copy(&mut tcp_read, &mut send).await;
        let _ = send.shutdown().await;
    };
    let down = async {
        let _ = tokio::io::copy(&mut recv, &mut tcp_write).await;
        let _ = tcp_write.shutdown().await;
    };
    tokio::join!(up, down);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local() -> OpenOptions {
        OpenOptions { stun: false, upnp: false, loopback: true }
    }

    #[tokio::test]
    async fn friends_reach_the_server_its_list_and_files_and_strangers_dont() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        // A stand-in server that answers each line with it reversed.
        let server = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
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

        let dir = std::env::temp_dir().join(format!("wisp-link-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let shared = dir.join("shared.jar");
        std::fs::write(&shared, b"a mod nobody else has").unwrap();
        let hash = crate::launch::hex(sha1::Sha1::digest(b"a mod nobody else has").as_slice());

        let host = Endpoint::open(local()).await.unwrap();
        let service = Arc::new(Service::default());
        *service.target.write().unwrap() = Some(target);
        *service.manifest.write().unwrap() = br#"{"mods":1}"#.to_vec();
        service.files.write().unwrap().insert(hash.clone(), shared.clone());

        let friend = Endpoint::open(local()).await.unwrap();
        let stranger = Endpoint::open(local()).await.unwrap();
        host.allow(&friend.offer().fingerprint);
        let host = Arc::new(host);
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

        let link = friend.connect(&host.offer()).await.unwrap();
        assert_eq!(link.manifest().await.unwrap(), br#"{"mods":1}"#);
        let fetched = dir.join("got/shared.jar");
        assert_eq!(link.fetch(&hash, &fetched).await.unwrap(), 21);
        assert_eq!(std::fs::read(&fetched).unwrap(), b"a mod nobody else has");
        assert!(matches!(link.fetch(&"0".repeat(40), &dir.join("got/none.jar")).await, Err(TunnelError::NoSuchFile)));

        let (port, _forwarding) = link.forward().await.unwrap();
        let mut game = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await.unwrap();
        game.write_all(b"hello").await.unwrap();
        let mut answer = [0u8; 5];
        game.read_exact(&mut answer).await.unwrap();
        assert_eq!(&answer, b"olleh");

        // The skin server, at the port asked for (or another, when that's taken here).
        *service.skins.write().unwrap() = Some(target);
        let (skins_port, _skins) = link.forward_skins(port).await.unwrap();
        assert_ne!(skins_port, port, "the game's own port is taken, so it gets another");
        let mut skins = TcpStream::connect((Ipv4Addr::LOCALHOST, skins_port)).await.unwrap();
        skins.write_all(b"skin").await.unwrap();
        let mut answer = [0u8; 4];
        skins.read_exact(&mut answer).await.unwrap();
        assert_eq!(&answer, b"niks");

        // A friend hands in themselves, never someone else.
        let taken = Arc::new(std::sync::Mutex::new(Vec::new()));
        {
            let taken = taken.clone();
            *service.on_player.write().unwrap() = Some(Arc::new(move |upload: Upload| taken.lock().unwrap().push(upload)));
        }
        service.players.write().unwrap().insert(friend.offer().fingerprint, "ab".repeat(16));
        let me = Upload { name: "Friend".into(), uuid: "AB".repeat(16), slim: true, png: None };
        assert!(link.hand_in(&me).await.unwrap());
        assert!(!link.hand_in(&Upload { uuid: "cd".repeat(16), ..me.clone() }).await.unwrap());
        assert_eq!(taken.lock().unwrap().len(), 1);
        assert_eq!(taken.lock().unwrap()[0].uuid, "ab".repeat(16));

        // Someone the host didn't let in gets nowhere, even knowing the host's address and
        // certificate; and nobody gets in with the wrong certificate for the host.
        assert!(matches!(stranger.connect(&host.offer()).await, Err(TunnelError::Refused)));
        let impostor = Offer { fingerprint: "00".repeat(32), ..host.offer() };
        assert!(matches!(friend.connect(&impostor).await, Err(TunnelError::Unreachable)));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn through_the_servers_door_friends_get_everything_and_games_go_straight_through() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        // A stand-in server that answers each read with it reversed.
        let server = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let target = server.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = server.accept().await {
                tokio::spawn(async move {
                    let mut buffer = [0u8; 256];
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
        let dir = std::env::temp_dir().join(format!("wisp-door-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let shared = dir.join("shared.jar");
        std::fs::write(&shared, b"a mod through the door").unwrap();
        let hash = crate::launch::hex(sha1::Sha1::digest(b"a mod through the door").as_slice());

        let keys = Keys::new().unwrap();
        let service = Arc::new(Service::default());
        *service.target.write().unwrap() = Some(target);
        *service.skins.write().unwrap() = Some(target);
        *service.manifest.write().unwrap() = br#"{"door":true}"#.to_vec();
        service.files.write().unwrap().insert(hash.clone(), shared.clone());
        let door = door::Door::open(0, target, &keys, service.clone()).await.unwrap();

        // A game's own connection goes straight to the server, first packet and all.
        let mut game = TcpStream::connect((Ipv4Addr::LOCALHOST, door.port())).await.unwrap();
        game.write_all(&[3, 0, 1, 2]).await.unwrap();
        let mut answer = [0u8; 4];
        game.read_exact(&mut answer).await.unwrap();
        assert_eq!(answer, [2, 1, 0, 3]);
        // So does an old game's ping.
        let mut old = TcpStream::connect((Ipv4Addr::LOCALHOST, door.port())).await.unwrap();
        old.write_all(&[0xFE, 0x01]).await.unwrap();
        let mut answer = [0u8; 2];
        old.read_exact(&mut answer).await.unwrap();
        assert_eq!(answer, [0x01, 0xFE]);

        // A friend the host let in, knowing only the door's address (no direct link).
        let friend = Endpoint::open(local()).await.unwrap();
        keys.allow(&friend.offer().fingerprint);
        let to = Offer { candidates: vec!["127.0.0.1:9".into()], fingerprint: keys.fingerprint().to_string(), doors: vec![format!("127.0.0.1:{}", door.port())] };
        let link = friend.link_to(&to).await.unwrap();
        assert!(link.through_door());
        assert_eq!(link.manifest().await.unwrap(), br#"{"door":true}"#);
        let fetched = dir.join("got/shared.jar");
        assert_eq!(link.fetch(&hash, &fetched).await.unwrap(), 22);
        assert_eq!(std::fs::read(&fetched).unwrap(), b"a mod through the door");
        let (port, _forwarding) = link.forward().await.unwrap();
        let mut game = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await.unwrap();
        game.write_all(b"hello").await.unwrap();
        let mut answer = [0u8; 5];
        game.read_exact(&mut answer).await.unwrap();
        assert_eq!(&answer, b"olleh");
        let (skins_port, _skins) = link.forward_skins(0).await.unwrap();
        let mut skins = TcpStream::connect((Ipv4Addr::LOCALHOST, skins_port)).await.unwrap();
        skins.write_all(b"skin").await.unwrap();
        let mut answer = [0u8; 4];
        skins.read_exact(&mut answer).await.unwrap();
        assert_eq!(&answer, b"niks");
        let taken = Arc::new(std::sync::Mutex::new(Vec::new()));
        {
            let taken = taken.clone();
            *service.on_player.write().unwrap() = Some(Arc::new(move |upload: Upload| taken.lock().unwrap().push(upload)));
        }
        service.players.write().unwrap().insert(friend.offer().fingerprint, "ab".repeat(16));
        let me = Upload { name: "Friend".into(), uuid: "ab".repeat(16), slim: false, png: None };
        assert!(link.hand_in(&me).await.unwrap());
        assert!(!link.hand_in(&Upload { uuid: "cd".repeat(16), ..me }).await.unwrap());
        assert_eq!(taken.lock().unwrap().len(), 1);

        // Someone the host didn't let in is turned away, and a door that isn't the host's Wisp
        // (the wrong certificate) isn't used.
        let stranger = Endpoint::open(local()).await.unwrap();
        assert!(matches!(stranger.through_door(&to).await, Err(TunnelError::Refused)));
        let impostor = Offer { fingerprint: "00".repeat(32), ..to.clone() };
        assert!(matches!(friend.through_door(&impostor).await, Err(TunnelError::Unreachable)));
        link.close();
        assert!(link.manifest().await.is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn joining_by_address_asks_the_door_and_the_host_decides() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        // A plain Minecraft server stand-in, for what isn't a Wisp door: it hangs up on garbage.
        let plain = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let plain_address = plain.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = plain.accept().await {
                tokio::spawn(async move {
                    let mut buffer = [0u8; 512];
                    let _ = socket.read(&mut buffer).await;
                });
            }
        });
        let keys = Keys::new().unwrap();
        let service = Arc::new(Service::default());
        *service.manifest.write().unwrap() = br#"{"public":true}"#.to_vec();
        *service.on_ask.write().unwrap() = Some(Arc::new(|ask: &Ask| ask.name == "Friend"));
        let door = door::Door::open(0, plain_address, &keys, service.clone()).await.unwrap();
        let address = format!("127.0.0.1:{}", door.port());

        let joiner = Keys::new().unwrap();
        let (link, fingerprint) = ask_door(&address, &joiner, &Ask { name: "Friend".into(), uuid: "AB".repeat(16) }).await.unwrap();
        assert_eq!(fingerprint, keys.fingerprint());
        assert_eq!(link.manifest().await.unwrap(), br#"{"public":true}"#);
        // They may hand in only who they asked to join as.
        assert_eq!(service.players.read().unwrap().get(joiner.fingerprint()).map(String::as_str), Some("ab".repeat(16).as_str()));

        let refused = ask_door(&address, &Keys::new().unwrap(), &Ask { name: "Stranger".into(), uuid: "cd".repeat(16) }).await;
        assert!(matches!(refused, Err(TunnelError::Refused)));
        let started = std::time::Instant::now();
        let not_wisp = ask_door(&plain_address.to_string(), &Keys::new().unwrap(), &Ask { name: "Friend".into(), uuid: "ab".repeat(16) }).await;
        assert!(matches!(not_wisp, Err(TunnelError::Unreachable)), "{:?}", not_wisp.err());
        assert!(started.elapsed() < Duration::from_secs(9));
    }

    #[tokio::test]
    async fn the_host_can_reach_the_friend_when_the_friend_cant_reach_the_host() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let host = Arc::new(Endpoint::open(local()).await.unwrap());
        let friend = Endpoint::open(local()).await.unwrap();
        host.allow(&friend.offer().fingerprint);
        friend.allow(&host.offer().fingerprint);
        let service = Arc::new(Service::default());
        *service.manifest.write().unwrap() = b"[]".to_vec();
        // The host dials the friend (as it does after answering a join), and serves that link.
        {
            let host = host.clone();
            let service = service.clone();
            let to = friend.offer();
            tokio::spawn(async move {
                if let Ok(link) = host.connect(&to).await {
                    link.serve(service).await;
                }
            });
        }
        // The friend's own way to the host is blocked (a strict router): only a dead address.
        let blocked = Offer { candidates: vec!["127.0.0.1:9".into()], fingerprint: host.offer().fingerprint, doors: Vec::new() };
        let link = friend.meet(&blocked).await.unwrap();
        assert_eq!(link.manifest().await.unwrap(), b"[]");
    }
}
