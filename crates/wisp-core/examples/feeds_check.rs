//! Fetches the feeds the UI reads straight from the real services, to catch shape mismatches
//! the browser preview (which maps JSON itself) can't.
#[tokio::main]
async fn main() {
    let client = wisp_core::http_client().expect("client");
    let categories = wisp_core::modrinth::categories(&client).await;
    println!("categories: {:?}", categories.as_ref().map(|found| found.len()).map_err(|error| error.to_string()));
    let tags = wisp_core::modrinth::loader_tags(&client).await;
    println!("loader tags: {:?}", tags.as_ref().map(|found| found.len()).map_err(|error| error.to_string()));
    let summaries = wisp_core::modrinth::projects(&client, &["AANobbMI".into(), "P7dR8mSH".into()]).await;
    println!("summaries: {:?}", summaries.map(|found| found.into_iter().map(|p| p.title).collect::<Vec<_>>()).map_err(|error| error.to_string()));
}
