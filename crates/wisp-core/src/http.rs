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

/// Who's asking, with a way to reach Wisp's makers (Modrinth and PaperMC ask API clients for both).
const USER_AGENT: &str = concat!("wisp-launcher/", env!("CARGO_PKG_VERSION"), " (wisplaunchercontact@gmail.com)");

/// One shared client for the API calls: reused connections, gzip, and a User-Agent that says who's
/// asking.
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
