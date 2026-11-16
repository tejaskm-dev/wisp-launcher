use std::time::Duration;

pub use reqwest::Client as HttpClient;

/// A network failure, shown to the player as a short message.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network request failed: {0}")]
    Http(#[from] reqwest::Error),
    /// A service answered, but said no (for example, CurseForge refusing an API key).
    #[error("{0}")]
    Refused(String),
}

const USER_AGENT: &str = concat!("wisp-launcher/", env!("CARGO_PKG_VERSION"));

/// One shared client for the API calls: reused connections, gzip, and a plain User-Agent
/// (Modrinth asks API clients to identify themselves).
pub fn http_client() -> Result<HttpClient, Error> {
    Ok(HttpClient::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()?)
}

/// For game downloads: no overall time limit (a big file on a slow line takes a while), but a
/// stalled connection still gives up.
pub fn download_client() -> Result<HttpClient, Error> {
    Ok(HttpClient::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .build()?)
}
