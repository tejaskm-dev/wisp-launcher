//! CurseForge (api.curseforge.com): searching, project pages and files, mapped onto the same
//! shapes Wisp uses for Modrinth so the interface shows both alike.
//!
//! Every request needs an API key. CurseForge gives them to app developers for free through
//! console.curseforge.com; Wisp uses one built in at compile time (`WISP_CURSEFORGE_API_KEY`) or
//! the one pasted in Settings. CurseForge projects are "curseforge:<id>" inside Wisp, so pages and
//! installs know where to look.

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::http::{Error, HttpClient};
use crate::modrinth::{
    Category, Dependency, GalleryImage, Project, ProjectDetails, ProjectLink, ProjectSummary,
    ProjectType, ProjectVersion, SearchQuery, SearchResults, SearchSort, VersionFile,
};
use crate::url_encode;

const API: &str = "https://api.curseforge.com/v1";
/// CurseForge's id for Minecraft.
const MINECRAFT: u32 = 432;
pub const ID_PREFIX: &str = "curseforge:";
/// CurseForge caps searches at 10,000 results and pages at 50.
const MAX_RESULTS: u32 = 10_000;
const MAX_PAGE: u32 = 50;

/// The key to use: the one typed in Settings, else the one built into this copy of Wisp.
pub fn api_key(from_settings: &str) -> Option<String> {
    let typed = from_settings.trim();
    if !typed.is_empty() {
        return Some(typed.to_string());
    }
    option_env!("WISP_CURSEFORGE_API_KEY")
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(String::from)
}

pub fn is_curseforge(project_id: &str) -> bool {
    project_id.starts_with(ID_PREFIX)
}

/// "curseforge:238222" → 238222.
pub fn mod_id(project_id: &str) -> Option<u32> {
    project_id.strip_prefix(ID_PREFIX)?.parse().ok()
}

fn wisp_id(mod_id: u32) -> String {
    format!("{ID_PREFIX}{mod_id}")
}

/// CurseForge's "classes" for Minecraft content.
fn class_id(kind: ProjectType) -> Option<u32> {
    Some(match kind {
        ProjectType::Mod => 6,
        ProjectType::Modpack => 4471,
        ProjectType::Resourcepack => 12,
        ProjectType::Shader => 6552,
        ProjectType::Datapack => 6945,
        ProjectType::World => 17,
        ProjectType::Plugin => 5,
        ProjectType::Other => return None,
    })
}

pub(crate) fn project_type(class: Option<u32>) -> ProjectType {
    match class {
        Some(6) => ProjectType::Mod,
        Some(4471) => ProjectType::Modpack,
        Some(12) => ProjectType::Resourcepack,
        Some(6552) => ProjectType::Shader,
        Some(6945) => ProjectType::Datapack,
        Some(17) => ProjectType::World,
        Some(5) => ProjectType::Plugin,
        _ => ProjectType::Other,
    }
}

/// CurseForge's ModLoaderType, by Wisp's (Modrinth's) loader names.
fn loader_type(loader: &str) -> Option<u32> {
    Some(match loader {
        "forge" => 1,
        "cauldron" => 2,
        "liteloader" => 3,
        "fabric" => 4,
        "quilt" => 5,
        "neoforge" => 6,
        _ => return None,
    })
}

fn loader_name(kind: u32) -> Option<&'static str> {
    Some(match kind {
        1 => "forge",
        2 => "cauldron",
        3 => "liteloader",
        4 => "fabric",
        5 => "quilt",
        6 => "neoforge",
        _ => return None,
    })
}

/// Files list loaders among their game versions ("Forge", "1.20.1", "Client").
fn loader_from_label(label: &str) -> Option<&'static str> {
    Some(match label.to_ascii_lowercase().as_str() {
        "forge" => "forge",
        "neoforge" => "neoforge",
        "fabric" => "fabric",
        "quilt" => "quilt",
        "liteloader" => "liteloader",
        "cauldron" => "cauldron",
        _ => return None,
    })
}

fn is_game_version(label: &str) -> bool {
    label.starts_with(|c: char| c.is_ascii_digit()) && label.contains('.')
}

/// What a kind of project counts as "loaders" when its files don't name any, so Wisp's
/// compatibility checks treat CurseForge and Modrinth projects alike.
fn implied_loaders(kind: ProjectType) -> &'static [&'static str] {
    match kind {
        ProjectType::Resourcepack => &["minecraft"],
        ProjectType::Shader => &["iris", "optifine"],
        ProjectType::Datapack => &["datapack"],
        _ => &[],
    }
}

// ─── requests ───────────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct Envelope<T> {
    data: T,
    #[serde(default)]
    pagination: Option<Pagination>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pagination {
    #[serde(default)]
    total_count: u64,
}

async fn send<T: DeserializeOwned>(request: reqwest::RequestBuilder) -> Result<Envelope<T>, Error> {
    let response = request.send().await?;
    match response.status().as_u16() {
        401 | 403 => {
            return Err(Error::Refused(
                "CurseForge didn't accept the API key. Check it in Settings → Mod sources.".into(),
            ));
        }
        404 => return Err(Error::Refused("CurseForge doesn't have that.".into())),
        _ => {}
    }
    Ok(response.error_for_status()?.json().await?)
}

async fn get<T: DeserializeOwned>(
    client: &HttpClient,
    key: &str,
    path: &str,
) -> Result<Envelope<T>, Error> {
    send(client.get(format!("{API}{path}")).header("x-api-key", key)).await
}

async fn post<T: DeserializeOwned>(
    client: &HttpClient,
    key: &str,
    path: &str,
    body: serde_json::Value,
) -> Result<Envelope<T>, Error> {
    send(
        client
            .post(format!("{API}{path}"))
            .header("x-api-key", key)
            .json(&body),
    )
    .await
}

// ─── raw shapes ─────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RawMod {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    download_count: f64,
    #[serde(default)]
    thumbs_up_count: f64,
    #[serde(default)]
    pub class_id: Option<u32>,
    #[serde(default)]
    categories: Vec<RawCategory>,
    #[serde(default)]
    authors: Vec<RawAuthor>,
    #[serde(default)]
    logo: Option<RawImage>,
    #[serde(default)]
    screenshots: Vec<RawImage>,
    #[serde(default)]
    links: Option<RawLinks>,
    #[serde(default)]
    latest_files_indexes: Vec<RawFileIndex>,
    #[serde(default)]
    date_created: String,
    #[serde(default)]
    date_modified: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCategory {
    id: u32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    icon_url: Option<String>,
    #[serde(default)]
    is_class: Option<bool>,
    #[serde(default)]
    display_index: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawAuthor {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawImage {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    thumbnail_url: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLinks {
    #[serde(default)]
    website_url: Option<String>,
    #[serde(default)]
    wiki_url: Option<String>,
    #[serde(default)]
    issues_url: Option<String>,
    #[serde(default)]
    source_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawFileIndex {
    #[serde(default)]
    game_version: String,
    #[serde(default)]
    mod_loader: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RawFile {
    pub id: u32,
    pub mod_id: u32,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    pub file_name: String,
    #[serde(default)]
    release_type: u8,
    #[serde(default)]
    hashes: Vec<RawHash>,
    #[serde(default)]
    file_date: String,
    #[serde(default)]
    pub file_length: u64,
    #[serde(default)]
    download_count: f64,
    /// Missing when the author only allows downloads from CurseForge's website.
    #[serde(default)]
    pub download_url: Option<String>,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    dependencies: Vec<RawDependency>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawHash {
    value: String,
    algo: u8,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDependency {
    mod_id: u32,
    relation_type: u8,
}

impl RawFile {
    pub(crate) fn sha1(&self) -> Option<String> {
        self.hashes
            .iter()
            .find(|hash| hash.algo == 1)
            .map(|hash| hash.value.clone())
    }

    fn into_version(self, kind: ProjectType) -> ProjectVersion {
        let mut loaders: Vec<String> = self
            .game_versions
            .iter()
            .filter_map(|label| loader_from_label(label))
            .map(String::from)
            .collect();
        if loaders.is_empty() {
            loaders = implied_loaders(kind)
                .iter()
                .map(|loader| loader.to_string())
                .collect();
        }
        ProjectVersion {
            id: self.id.to_string(),
            project_id: wisp_id(self.mod_id),
            name: self.display_name.clone(),
            version_number: self.file_name.clone(),
            version_type: match self.release_type {
                2 => "beta",
                3 => "alpha",
                _ => "release",
            }
            .into(),
            loaders,
            game_versions: self
                .game_versions
                .iter()
                .filter(|label| is_game_version(label))
                .cloned()
                .collect(),
            date_published: self.file_date.clone(),
            downloads: self.download_count as u64,
            changelog: String::new(),
            files: vec![VersionFile {
                sha1: self.sha1(),
                url: self.download_url.clone().unwrap_or_default(),
                filename: self.file_name.clone(),
                primary: true,
                size: self.file_length,
                sha512: None,
            }],
            dependencies: self
                .dependencies
                .iter()
                .filter_map(|dependency| {
                    let kind = match dependency.relation_type {
                        3 => "required",
                        2 => "optional",
                        5 => "incompatible",
                        1 | 6 => "embedded",
                        _ => return None,
                    };
                    Some(Dependency {
                        project_id: Some(wisp_id(dependency.mod_id)),
                        version_id: None,
                        dependency_type: kind.into(),
                    })
                })
                .collect(),
        }
    }
}

impl RawMod {
    fn loaders(&self) -> Vec<String> {
        let mut loaders: Vec<String> = Vec::new();
        for index in &self.latest_files_indexes {
            if let Some(name) = index.mod_loader.and_then(loader_name)
                && !loaders.iter().any(|known| known == name)
            {
                loaders.push(name.to_string());
            }
        }
        if loaders.is_empty() {
            let kind = project_type(self.class_id);
            loaders = implied_loaders(kind)
                .iter()
                .map(|loader| loader.to_string())
                .collect();
        }
        loaders
    }

    /// Supported Minecraft versions, oldest first (as Modrinth lists them).
    fn game_versions(&self) -> Vec<String> {
        let mut versions: Vec<String> = Vec::new();
        for index in &self.latest_files_indexes {
            if is_game_version(&index.game_version) && !versions.contains(&index.game_version) {
                versions.push(index.game_version.clone());
            }
        }
        versions.sort_by(|a, b| crate::loaders::compare_versions(a, b));
        versions
    }

    fn into_project(self) -> Project {
        let loaders = self.loaders();
        let game_version = self.game_versions().pop();
        Project {
            id: wisp_id(self.id),
            slug: self.slug,
            title: self.name,
            author: self
                .authors
                .first()
                .map(|author| author.name.clone())
                .unwrap_or_default(),
            description: self.summary,
            project_type: project_type(self.class_id),
            downloads: self.download_count as u64,
            follows: self.thumbs_up_count as u64,
            updated: self.date_modified,
            loaders,
            game_version,
            tags: self
                .categories
                .iter()
                .map(|category| category.slug.clone())
                .collect(),
            image_url: self
                .screenshots
                .iter()
                .find_map(|shot| shot.thumbnail_url.clone()),
            icon_url: self.logo.and_then(|logo| logo.thumbnail_url.or(logo.url)),
            color: None,
        }
    }
}

// ─── public calls ───────────────────────────────────────────────────────────────────────────────

pub async fn search(
    client: &HttpClient,
    key: &str,
    query: &SearchQuery,
) -> Result<SearchResults, Error> {
    let limit = query.limit.clamp(1, MAX_PAGE);
    // CurseForge refuses pages past its 10,000-result window.
    let offset = query.offset.min(MAX_RESULTS.saturating_sub(limit));
    let mut params = vec![
        format!("gameId={MINECRAFT}"),
        format!("index={offset}"),
        format!("pageSize={limit}"),
        format!(
            "sortField={}",
            match query.sort {
                SearchSort::Relevance => 2, // Popularity: CurseForge has no relevance order
                SearchSort::Downloads => 6,
                SearchSort::Follows => 12, // Rating
                SearchSort::Newest => 11,
                SearchSort::Updated => 3,
            }
        ),
        "sortOrder=desc".into(),
    ];
    if let Some(class) = class_id(query.project_type) {
        params.push(format!("classId={class}"));
    }
    if !query.query.trim().is_empty() {
        params.push(format!("searchFilter={}", url_encode(query.query.trim())));
    }
    let categories: Vec<u32> = query
        .categories
        .iter()
        .filter_map(|id| id.parse().ok())
        .take(10)
        .collect();
    if !categories.is_empty() {
        params.push(format!(
            "categoryIds={}",
            url_encode(&serde_json::to_string(&categories).unwrap_or_default())
        ));
    }
    let loaders: Vec<u32> = query
        .loaders
        .iter()
        .filter_map(|loader| loader_type(loader))
        .take(5)
        .collect();
    if !loaders.is_empty() {
        params.push(format!(
            "modLoaderTypes={}",
            url_encode(&serde_json::to_string(&loaders).unwrap_or_default())
        ));
    }
    if let Some(version) = query
        .game_version
        .as_deref()
        .filter(|version| is_game_version(version))
    {
        params.push(format!("gameVersion={}", url_encode(version)));
    }
    let found: Envelope<Vec<RawMod>> =
        get(client, key, &format!("/mods/search?{}", params.join("&"))).await?;
    let total = found
        .pagination
        .map_or(0, |pagination| pagination.total_count)
        .min(u64::from(MAX_RESULTS));
    Ok(SearchResults {
        hits: found.data.into_iter().map(RawMod::into_project).collect(),
        total,
        offset,
    })
}

/// A class's categories, in the shape of Modrinth's (the id goes in `name`, the words in `label`).
pub async fn categories(
    client: &HttpClient,
    key: &str,
    kind: ProjectType,
) -> Result<Vec<Category>, Error> {
    let Some(class) = class_id(kind) else {
        return Ok(Vec::new());
    };
    let found: Envelope<Vec<RawCategory>> = get(
        client,
        key,
        &format!("/categories?gameId={MINECRAFT}&classId={class}"),
    )
    .await?;
    let mut categories: Vec<RawCategory> = found
        .data
        .into_iter()
        .filter(|category| category.is_class != Some(true))
        .collect();
    categories.sort_by(|a, b| {
        a.display_index
            .cmp(&b.display_index)
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(categories
        .into_iter()
        .map(|category| Category {
            name: category.id.to_string(),
            project_type: kind.as_str().into(),
            header: "categories".into(),
            icon: String::new(),
            label: Some(category.name),
            icon_url: category.icon_url,
        })
        .collect())
}

pub(crate) async fn raw_mod(client: &HttpClient, key: &str, id: u32) -> Result<RawMod, Error> {
    Ok(get::<RawMod>(client, key, &format!("/mods/{id}"))
        .await?
        .data)
}

/// A project's page, with its description (HTML, which Wisp's Markdown renderer passes through
/// its sanitiser).
pub async fn project(client: &HttpClient, key: &str, id: u32) -> Result<ProjectDetails, Error> {
    let raw = raw_mod(client, key, id).await?;
    // A missing description just leaves the page short.
    let body = get::<String>(client, key, &format!("/mods/{id}/description"))
        .await
        .map(|description| description.data)
        .unwrap_or_default();
    Ok(details(raw, body))
}

/// The same without the description, for installs.
pub(crate) async fn project_lite(
    client: &HttpClient,
    key: &str,
    id: u32,
) -> Result<ProjectDetails, Error> {
    Ok(details(raw_mod(client, key, id).await?, String::new()))
}

fn details(raw: RawMod, body: String) -> ProjectDetails {
    let kind = project_type(raw.class_id);
    let loaders = raw.loaders();
    let game_versions = raw.game_versions();
    let mut links = Vec::new();
    if let Some(found) = &raw.links {
        for (label, url) in [
            ("CurseForge", &found.website_url),
            ("Wiki", &found.wiki_url),
            ("Issues", &found.issues_url),
            ("Source", &found.source_url),
        ] {
            if let Some(url) = url.as_ref().filter(|url| url.starts_with("https://")) {
                links.push(ProjectLink {
                    label: label.into(),
                    url: url.clone(),
                });
            }
        }
    }
    ProjectDetails {
        id: wisp_id(raw.id),
        slug: raw.slug,
        title: raw.name,
        description: raw.summary,
        body,
        project_type: kind,
        downloads: raw.download_count as u64,
        followers: raw.thumbs_up_count as u64,
        published: raw.date_created,
        updated: raw.date_modified,
        categories: raw
            .categories
            .iter()
            .map(|category| category.slug.clone())
            .collect(),
        loaders,
        game_versions,
        client_side: String::new(),
        server_side: String::new(),
        license: None,
        icon_url: raw
            .logo
            .as_ref()
            .and_then(|logo| logo.thumbnail_url.clone().or(logo.url.clone())),
        color: None,
        gallery: raw
            .screenshots
            .into_iter()
            .filter_map(|shot| {
                let raw_url = shot.url.clone()?;
                Some(GalleryImage {
                    url: shot.thumbnail_url.unwrap_or_else(|| raw_url.clone()),
                    raw_url,
                    title: shot.title,
                    description: shot.description,
                    featured: false,
                })
            })
            .collect(),
        links,
        authors: raw.authors.into_iter().map(|author| author.name).collect(),
    }
}

/// A project's files, newest first, optionally only those for some loaders or game versions.
pub async fn versions(
    client: &HttpClient,
    key: &str,
    id: u32,
    loaders: &[&str],
    game_versions: &[&str],
) -> Result<Vec<ProjectVersion>, Error> {
    let kind = project_type(raw_mod(client, key, id).await?.class_id);
    let mut path = format!("/mods/{id}/files?pageSize={MAX_PAGE}");
    // The API filters by one game version at most; more are filtered here.
    if let [game] = game_versions {
        path.push_str(&format!("&gameVersion={}", url_encode(game)));
    }
    let found: Envelope<Vec<RawFile>> = get(client, key, &path).await?;
    let mut versions: Vec<ProjectVersion> = found
        .data
        .into_iter()
        .map(|file| file.into_version(kind))
        .filter(|version| {
            (loaders.is_empty()
                || version
                    .loaders
                    .iter()
                    .any(|loader| loaders.contains(&loader.as_str())))
                && (game_versions.is_empty()
                    || version
                        .game_versions
                        .iter()
                        .any(|game| game_versions.contains(&game.as_str())))
        })
        .collect();
    versions.sort_by(|a, b| b.date_published.cmp(&a.date_published));
    Ok(versions)
}

pub async fn version(
    client: &HttpClient,
    key: &str,
    mod_id: u32,
    file_id: u32,
) -> Result<ProjectVersion, Error> {
    let kind = project_type(raw_mod(client, key, mod_id).await?.class_id);
    let file = get::<RawFile>(client, key, &format!("/mods/{mod_id}/files/{file_id}")).await?;
    Ok(file.data.into_version(kind))
}

/// Names and icons of several projects at once (a version's dependencies).
pub async fn summaries(
    client: &HttpClient,
    key: &str,
    ids: &[u32],
) -> Result<Vec<ProjectSummary>, Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(mods(client, key, ids)
        .await?
        .into_iter()
        .map(|raw| ProjectSummary {
            id: wisp_id(raw.id),
            slug: raw.slug.clone(),
            title: raw.name.clone(),
            project_type: project_type(raw.class_id),
            icon_url: raw.logo.and_then(|logo| logo.thumbnail_url.or(logo.url)),
            description: raw.summary.clone(),
            categories: Vec::new(),
        })
        .collect())
}

/// Several files at once (a modpack's mod list).
pub(crate) async fn files(
    client: &HttpClient,
    key: &str,
    ids: &[u32],
) -> Result<Vec<RawFile>, Error> {
    let mut found = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(500) {
        let page: Envelope<Vec<RawFile>> = post(
            client,
            key,
            "/mods/files",
            serde_json::json!({ "fileIds": chunk }),
        )
        .await?;
        found.extend(page.data);
    }
    Ok(found)
}

/// Several projects at once (to know which folder each of a modpack's files goes in).
pub(crate) async fn mods(
    client: &HttpClient,
    key: &str,
    ids: &[u32],
) -> Result<Vec<RawMod>, Error> {
    let mut found = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(500) {
        let page: Envelope<Vec<RawMod>> =
            post(client, key, "/mods", serde_json::json!({ "modIds": chunk })).await?;
        found.extend(page.data);
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH_HIT: &str = r#"{
      "id": 238222, "gameId": 432, "name": "Just Enough Items (JEI)", "slug": "jei",
      "links": {"websiteUrl": "https://www.curseforge.com/minecraft/mc-mods/jei", "wikiUrl": "", "issuesUrl": "https://github.com/mezz/JustEnoughItems/issues", "sourceUrl": null},
      "summary": "View Items and Recipes", "downloadCount": 412345678.0, "thumbsUpCount": 5321,
      "classId": 6, "authors": [{"id": 1, "name": "mezz", "url": "https://x"}],
      "categories": [{"id": 421, "gameId": 432, "name": "API and Library", "slug": "library-api", "iconUrl": "https://media.forgecdn.net/a.png", "classId": 6, "parentCategoryId": 6}],
      "logo": {"id": 1, "modId": 238222, "title": "", "thumbnailUrl": "https://media.forgecdn.net/thumb.png", "url": "https://media.forgecdn.net/full.png"},
      "screenshots": [],
      "latestFilesIndexes": [
        {"gameVersion": "1.21.1", "fileId": 2, "filename": "jei-1.21.1-neoforge.jar", "releaseType": 1, "modLoader": 6},
        {"gameVersion": "1.20.1", "fileId": 1, "filename": "jei-1.20.1-forge.jar", "releaseType": 1, "modLoader": 1},
        {"gameVersion": "1.20.1", "fileId": 3, "filename": "jei-1.20.1-fabric.jar", "releaseType": 1, "modLoader": 4}
      ],
      "dateCreated": "2015-11-18T00:00:00Z", "dateModified": "2026-11-01T10:00:00Z", "allowModDistribution": true
    }"#;

    #[test]
    fn maps_a_search_hit_onto_wisps_project() {
        let raw: RawMod = serde_json::from_str(SEARCH_HIT).unwrap();
        let project = raw.into_project();
        assert_eq!(project.id, "curseforge:238222");
        assert_eq!(project.project_type, ProjectType::Mod);
        assert_eq!(project.loaders, ["neoforge", "forge", "fabric"]);
        assert_eq!(project.game_version.as_deref(), Some("1.21.1"));
        assert_eq!(project.downloads, 412_345_678);
        assert_eq!(
            project.icon_url.as_deref(),
            Some("https://media.forgecdn.net/thumb.png")
        );
        assert_eq!(mod_id(&project.id), Some(238222));
    }

    #[test]
    fn maps_files_onto_versions() {
        let file: RawFile = serde_json::from_str(
            r#"{"id": 5001, "gameId": 432, "modId": 238222, "isAvailable": true, "displayName": "JEI 15.2.0.27",
                "fileName": "jei-1.20.1-forge-15.2.0.27.jar", "releaseType": 2, "fileStatus": 4,
                "hashes": [{"value": "md5md5", "algo": 2}, {"value": "abc123", "algo": 1}],
                "fileDate": "2024-01-01T00:00:00Z", "fileLength": 1234, "downloadCount": 99,
                "downloadUrl": null, "gameVersions": ["Forge", "1.20.1", "Client"],
                "dependencies": [{"modId": 306612, "relationType": 3}, {"modId": 9, "relationType": 2}]}"#,
        )
        .unwrap();
        let version = file.into_version(ProjectType::Mod);
        assert_eq!(version.loaders, ["forge"]);
        assert_eq!(version.game_versions, ["1.20.1"]);
        assert_eq!(version.version_type, "beta");
        assert_eq!(version.files[0].sha1.as_deref(), Some("abc123"));
        assert!(
            version.files[0].url.is_empty(),
            "no download URL means the author blocks other apps"
        );
        assert_eq!(
            version.dependencies[0].project_id.as_deref(),
            Some("curseforge:306612")
        );
        assert_eq!(version.dependencies[0].dependency_type, "required");
    }

    #[test]
    fn resource_packs_and_data_packs_get_wisps_implied_loaders() {
        let file: RawFile = serde_json::from_str(
            r#"{"id": 1, "modId": 2, "fileName": "pack.zip", "gameVersions": ["1.21.1"]}"#,
        )
        .unwrap();
        assert_eq!(
            file.clone().into_version(ProjectType::Resourcepack).loaders,
            ["minecraft"]
        );
        assert_eq!(
            file.into_version(ProjectType::Datapack).loaders,
            ["datapack"]
        );
    }

    #[test]
    fn keys_and_ids() {
        assert_eq!(api_key("  abc  ").as_deref(), Some("abc"));
        assert!(is_curseforge("curseforge:1") && !is_curseforge("AANobbMI"));
        assert_eq!(mod_id("curseforge:nope"), None);
        assert_eq!(class_id(ProjectType::World), Some(17));
        assert_eq!(project_type(Some(6945)), ProjectType::Datapack);
    }
}
