//! Checks the network parsers against the real services. They need the network, so they are
//! ignored by default: `cargo test -p wisp-core -- --ignored`.

use wisp_core::modrinth::{ProjectType, SearchQuery, SearchSort};
use wisp_core::{feeds, http_client, modrinth, versions};

#[tokio::test]
#[ignore = "needs network access"]
async fn modrinth_returns_featured_modpacks() {
    let client = http_client().unwrap();
    let packs = feeds::featured_modpacks(&client, 3).await.unwrap();
    assert_eq!(packs.len(), 3);
    for pack in &packs {
        assert!(!pack.title.is_empty());
        assert!(pack.downloads > 0);
        println!(
            "{} by {} · {:?} · {:?}",
            pack.title, pack.author, pack.loaders, pack.game_version
        );
    }
}

#[tokio::test]
#[ignore = "needs network access"]
async fn modrinth_search_filters_by_type_and_loader() {
    let client = http_client().unwrap();
    let query = SearchQuery {
        source: Default::default(),
        query: "sodium".into(),
        project_type: ProjectType::Mod,
        sort: SearchSort::Relevance,
        loaders: vec!["fabric".into()],
        categories: vec!["optimization".into()],
        game_version: Some("26.3".into()),
        environment: Some("client".into()),
        offset: 0,
        limit: 5,
    };
    let results = modrinth::search(&client, &query).await.unwrap();
    assert!(results.total > 0);
    assert!(
        results
            .hits
            .iter()
            .all(|hit| hit.project_type == ProjectType::Mod)
    );
    assert!(
        results
            .hits
            .iter()
            .any(|hit| hit.title.to_lowercase().contains("sodium"))
    );
}

#[tokio::test]
#[ignore = "needs network access"]
async fn mojang_returns_latest_release_and_snapshot() {
    let client = http_client().unwrap();
    let notes = feeds::java_news(&client).await.unwrap();
    assert!(notes.iter().any(|note| note.kind == "release"));
    for note in &notes {
        assert!(note.date >= notes.last().unwrap().date, "newest first");
        println!("{} ({}) {}", note.title, note.kind, note.date);
    }
}

#[tokio::test]
#[ignore = "needs network access"]
async fn mojang_lists_versions_and_fabric_supports_the_latest_release() {
    let client = http_client().unwrap();
    let list = versions::game_versions(&client).await.unwrap();
    assert!(
        list.versions
            .iter()
            .any(|version| version.id == list.latest_release)
    );
    let loader = versions::latest_fabric_loader(&client, &list.latest_release)
        .await
        .unwrap();
    println!(
        "latest release {} · Fabric loader {loader:?}",
        list.latest_release
    );
    assert!(loader.is_some());
    assert_eq!(
        versions::latest_fabric_loader(&client, "not-a-version")
            .await
            .unwrap(),
        None
    );
}
