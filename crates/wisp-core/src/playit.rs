//! A public address for a server hosted on this computer, through playit.gg, with nothing to
//! install: playit's own agent runs inside Wisp (feature "playit", tester builds only, until
//! playit agrees in writing to Wisp bundling it).
//!
//! Linking: Wisp makes a claim code and opens playit's claim page, where the player approves this
//! computer (signing in, or carrying on as a guest); Wisp then trades the code for the agent's
//! secret. Each server gets a "Minecraft Java" tunnel (free on playit) to its port here, made
//! through playit's API with that secret, and the agent carries players from the tunnel's address
//! to the server while it runs.

use std::sync::Arc;
use std::time::{Duration, Instant};

use playit_agent_core::network::origin_lookup::OriginLookup;
use playit_agent_core::network::tcp::tcp_settings::TcpSettings;
use playit_agent_core::network::udp::udp_settings::UdpSettings;
use playit_agent_core::playit_agent::{PlayitAgent, PlayitAgentSettings};
use playit_api_client::PlayitApi;
use playit_api_client::api::{
    AccountTunnelOriginCreate, AgentOrigin, AgentRunDataV1, AgentTunnelAttr, AgentTunnelConfig, AgentTunnelV1, ApiError, ClaimAgentType, ClaimExchangeError, ClaimSetupError, ClaimSetupResponse, ReqClaimExchange, ReqClaimSetup, ReqTunnelsCreateV1,
    TunnelPortDetails, TunnelType,
};

const API: &str = "https://api.playit.gg";
/// How long the player has to approve Wisp on playit's page.
const CLAIM_WITHIN: Duration = Duration::from_secs(15 * 60);
/// How often the agent rereads which tunnels it carries.
const REFRESH_EVERY: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
pub enum PlayitError {
    #[error("playit.gg didn't answer ({0}). Check the internet connection and try again.")]
    Unreachable(String),
    #[error("Linking was turned down on playit.gg.")]
    Rejected,
    #[error("The link to playit.gg ran out of time. Try again.")]
    Expired,
    #[error("playit.gg wouldn't make the tunnel: {0}.")]
    Tunnel(String),
    #[error("playit.gg doesn't know this Wisp any more. Link it again.")]
    NotLinked,
    #[error("The tunnel didn't come up on playit.gg in time. Try again in a minute.")]
    Slow,
    #[error("playit's agent couldn't start: {0}")]
    Agent(String),
}

/// A code for playit's claim page: 10 hex digits, as playit's own agent makes them.
pub fn claim_code() -> String {
    use rand_core::RngCore;
    let mut bytes = [0u8; 5];
    rand_core::OsRng.fill_bytes(&mut bytes);
    crate::launch::hex(&bytes)
}

/// The page where the player approves this computer.
pub fn claim_url(code: &str) -> String {
    format!("https://playit.gg/claim/{code}")
}

/// Where a claim is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimStep {
    /// The page hasn't been opened yet.
    Visit,
    /// Open, waiting for the player to approve.
    Approve,
    Approved,
}

/// Waits for the player to approve this Wisp on playit's claim page, and returns the agent's
/// secret. `version` is how the agent is listed on their account.
pub async fn claim(code: &str, version: &str, step: &(dyn Fn(ClaimStep) + Send + Sync)) -> Result<String, PlayitError> {
    let api = PlayitApi::create(API.into(), None);
    let deadline = Instant::now() + CLAIM_WITHIN;
    let mut said = None;
    loop {
        if Instant::now() > deadline {
            return Err(PlayitError::Expired);
        }
        let now = match api.claim_setup(ReqClaimSetup { code: code.to_string(), agent_type: ClaimAgentType::SelfManaged, version: version.to_string() }).await {
            Ok(ClaimSetupResponse::WaitingForUserVisit) => ClaimStep::Visit,
            Ok(ClaimSetupResponse::WaitingForUser) => ClaimStep::Approve,
            Ok(ClaimSetupResponse::UserAccepted) => break,
            Ok(ClaimSetupResponse::UserRejected) => return Err(PlayitError::Rejected),
            Err(ApiError::Fail(ClaimSetupError::CodeExpired | ClaimSetupError::InvalidCode)) => return Err(PlayitError::Expired),
            // Offline for a moment: keep waiting.
            Err(_) => said.unwrap_or(ClaimStep::Visit),
        };
        if said != Some(now) {
            step(now);
            said = Some(now);
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    step(ClaimStep::Approved);
    loop {
        match api.claim_exchange(ReqClaimExchange { code: code.to_string() }).await {
            Ok(key) => return Ok(key.secret_key),
            Err(ApiError::Fail(ClaimExchangeError::UserRejected)) => return Err(PlayitError::Rejected),
            Err(ApiError::Fail(ClaimExchangeError::CodeExpired | ClaimExchangeError::CodeNotFound)) => return Err(PlayitError::Expired),
            Err(_) if Instant::now() > deadline => return Err(PlayitError::Expired),
            Err(_) => tokio::time::sleep(Duration::from_secs(2)).await,
        }
    }
}

/// A tunnel the agent carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tunnel {
    pub id: String,
    /// What players type: `name.gl.joinmc.link`, say.
    pub address: String,
    pub local_port: u16,
    /// Why playit has it off, if it does.
    pub off_because: Option<String>,
}

fn local_port_of(tunnel: &AgentTunnelV1) -> Option<u16> {
    tunnel.agent_config.fields.iter().find(|field| field.name == "local_port").and_then(|field| field.value.trim().parse().ok())
}

fn tunnels_in(data: &AgentRunDataV1) -> Vec<Tunnel> {
    data.tunnels
        .iter()
        .filter(|tunnel| tunnel.tunnel_type.as_deref() == Some("minecraft-java"))
        .filter_map(|tunnel| {
            Some(Tunnel {
                id: tunnel.id.to_string(),
                address: tunnel.display_address.clone(),
                local_port: local_port_of(tunnel)?,
                off_because: tunnel.disabled_reason.as_ref().map(|reason| reason.to_string()),
            })
        })
        .collect()
}

async fn run_data(api: &PlayitApi) -> Result<AgentRunDataV1, PlayitError> {
    api.v1_agents_rundata().await.map_err(|error| match error {
        playit_api_client::api::ApiErrorNoFail::ApiError(_) => PlayitError::NotLinked,
        other => PlayitError::Unreachable(other.to_string()),
    })
}

/// The Minecraft tunnels this computer's agent carries.
pub async fn tunnels(secret: &str) -> Result<Vec<Tunnel>, PlayitError> {
    let api = PlayitApi::create(API.into(), Some(secret.to_string()));
    Ok(tunnels_in(&run_data(&api).await?))
}

/// The tunnel to `local_port` on this computer, made (named `name`) if there isn't one yet.
pub async fn tunnel_to(secret: &str, name: &str, local_port: u16) -> Result<Tunnel, PlayitError> {
    let api = PlayitApi::create(API.into(), Some(secret.to_string()));
    let data = run_data(&api).await?;
    if let Some(tunnel) = tunnels_in(&data).into_iter().find(|tunnel| tunnel.local_port == local_port) {
        return Ok(tunnel);
    }
    let field = |name: &str, value: String| AgentTunnelAttr { name: name.to_string(), value };
    // playit takes plain ASCII names up to a few dozen characters.
    let name: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '-').take(40).collect();
    let request = ReqTunnelsCreateV1 {
        ports: TunnelPortDetails::TunnelType(TunnelType::MinecraftJava),
        origin: AccountTunnelOriginCreate::Agent(AgentOrigin {
            agent_id: Some(data.agent_id),
            config: AgentTunnelConfig { fields: vec![field("local_ip", "127.0.0.1".into()), field("local_port", local_port.to_string())] },
        }),
        enabled: true,
        alloc: None,
        name: Some(format!("Wisp {}", name.trim())),
        firewall_id: None,
    };
    api.v1_tunnels_create(request).await.map_err(|error| match error {
        ApiError::Fail(reason) => PlayitError::Tunnel(format!("{reason:?}")),
        ApiError::ApiError(_) => PlayitError::NotLinked,
        ApiError::ClientError(error) => PlayitError::Unreachable(format!("{error:?}")),
    })?;
    // It takes playit a moment to give the new tunnel an address.
    let deadline = Instant::now() + Duration::from_secs(90);
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_secs(3)).await;
        if let Ok(data) = run_data(&api).await
            && let Some(tunnel) = tunnels_in(&data).into_iter().find(|tunnel| tunnel.local_port == local_port && !tunnel.address.is_empty())
        {
            return Ok(tunnel);
        }
    }
    Err(PlayitError::Slow)
}

/// playit's agent, carrying players from this computer's tunnels to the servers here.
pub struct Agent {
    cancel: tokio_util::sync::CancellationToken,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Agent {
    /// Starts the agent with this computer's secret.
    pub async fn start(secret: &str) -> Result<Self, PlayitError> {
        let api = PlayitApi::create(API.into(), Some(secret.to_string()));
        let lookup = Arc::new(OriginLookup::default());
        lookup.update_from_run_data(&run_data(&api).await?).await;
        let settings = PlayitAgentSettings { api_url: API.to_string(), secret_key: secret.to_string(), tcp_settings: TcpSettings::default(), udp_settings: UdpSettings::default() };
        let agent = tokio::time::timeout(Duration::from_secs(30), PlayitAgent::new(settings, lookup.clone()))
            .await
            .map_err(|_| PlayitError::Agent("it took too long to reach playit.gg".into()))?
            .map_err(|error| PlayitError::Agent(format!("{error:?}")))?;
        let cancel = agent.cancellation_token();
        let running = tokio::spawn(agent.run());
        let refreshing = {
            let cancel = cancel.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => return,
                        _ = tokio::time::sleep(REFRESH_EVERY) => {}
                    }
                    if let Ok(data) = api.v1_agents_rundata().await {
                        lookup.update_from_run_data(&data).await;
                    }
                }
            })
        };
        Ok(Self { cancel, tasks: vec![running, refreshing] })
    }

    /// Whether it's still going (it ends when playit turns the secret down, say).
    pub fn running(&self) -> bool {
        self.tasks.first().is_some_and(|task| !task.is_finished())
    }

    pub fn stop(&self) {
        self.cancel.cancel();
        for task in &self.tasks {
            task.abort();
        }
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes_claim_codes_like_playits_own() {
        let code = claim_code();
        assert_eq!(code.len(), 10);
        assert!(code.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(code, claim_code());
        assert_eq!(claim_url("0a1b2c3d4e"), "https://playit.gg/claim/0a1b2c3d4e");
    }
}
