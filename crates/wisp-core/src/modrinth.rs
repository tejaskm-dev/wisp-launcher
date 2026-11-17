//! Searching Modrinth for modpacks, mods, resource packs, shaders and data packs. These types
//! are Wisp's shared shapes for projects: CurseForge's are mapped onto them (see `curseforge`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::http::{Error, HttpClient};
use crate::url_encode;

const API: &str = "https://api.modrinth.com/v2";
/// Mod loaders, shader loaders, "minecraft" (resource packs) and "datapack", as Modrinth names
/// them. Wisp's instances run the first four; the rest can still be browsed.
pub const LOADERS: &[&str] = &[
    "fabric",
    "forge",
    "neoforge",
    "quilt",
    "iris",
    "optifine",
    "canvas",
    "vanilla",
    "minecraft",
    "datapack",
    "babric",
    "bta-babric",
    "java-agent",
    "legacy-fabric",
    "liteloader",
    "modloader",
    "nilloader",
    "ornithe",
    "rift",
];
const MAX_LIMIT: u32 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectType {
    Modpack,
    Mod,
    Resourcepack,
    Shader,
    Datapack,
    Plugin,
    /// Saved worlds (maps), which only CurseForge lists.
    World,
    /// Anything Modrinth adds later.
    #[serde(other)]
    Other,
}

impl ProjectType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Modpack => "modpack",
            Self::Mod => "mod",
            Self::Resourcepack => "resourcepack",
            Self::Shader => "shader",
            Self::Datapack => "datapack",
            Self::Plugin => "plugin",
            Self::World => "world",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchSort {
    #[default]
    Relevance,
    Downloads,
    Follows,
    Newest,
    Updated,
}

impl SearchSort {
    fn as_str(self) -> &'static str {
        match self {
            Self::Relevance => "relevance",
            Self::Downloads => "downloads",
            Self::Follows => "follows",
            Self::Newest => "newest",
            Self::Updated => "updated",
        }
    }
}

/// Where projects come from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    #[default]
    Modrinth,
    Curseforge,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchQuery {
    #[serde(default)]
    pub source: Source,
    #[serde(default)]
    pub query: String,
    pub project_type: ProjectType,
    #[serde(default)]
    pub sort: SearchSort,
    /// Projects for any of these loaders (from `LOADERS`; others are ignored).
    #[serde(default)]
    pub loaders: Vec<String>,
    /// Projects in every one of these categories (Modrinth category slugs).
    #[serde(default)]
    pub categories: Vec<String>,
    /// Only projects that support this Minecraft version, e.g. "26.3".
    #[serde(default)]
    pub game_version: Option<String>,
    /// "client" or "server": only projects that run on that side.
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub offset: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    20
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub author: String,
    pub description: String,
    pub project_type: ProjectType,
    pub downloads: u64,
    pub follows: u64,
    /// RFC 3339 time of the last update.
    pub updated: String,
    /// Lowercase loader ids, e.g. `["fabric", "neoforge"]`.
    pub loaders: Vec<String>,
    /// Newest Minecraft version the project supports.
    pub game_version: Option<String>,
    pub tags: Vec<String>,
    /// A 350px gallery thumbnail, small enough to keep image memory low.
    pub image_url: Option<String>,
    pub icon_url: Option<String>,
    /// The project's accent colour as 0xRRGGBB.
    pub color: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub hits: Vec<Project>,
    pub total: u64,
    pub offset: u32,
}

#[derive(Deserialize)]
struct SearchResponse {
    hits: Vec<SearchHit>,
    total_hits: u64,
    offset: u32,
}

#[derive(Deserialize)]
struct SearchHit {
    project_id: String,
    slug: String,
    title: String,
    author: String,
    #[serde(default)]
    description: String,
    project_type: ProjectType,
    downloads: u64,
    #[serde(default)]
    follows: u64,
    #[serde(default)]
    date_modified: String,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    display_categories: Vec<String>,
    #[serde(default)]
    versions: Vec<String>,
    icon_url: Option<String>,
    featured_gallery: Option<String>,
    #[serde(default)]
    gallery: Vec<String>,
    color: Option<u32>,
}

pub async fn search(client: &HttpClient, query: &SearchQuery) -> Result<SearchResults, Error> {
    let mut facets = vec![format!(
        r#"["project_type:{}"]"#,
        query.project_type.as_str()
    )];
    let loaders: Vec<String> = query
        .loaders
        .iter()
        .filter(|loader| LOADERS.contains(&loader.as_str()))
        .map(|loader| format!(r#""categories:{loader}""#))
        .collect();
    if !loaders.is_empty() {
        facets.push(format!("[{}]", loaders.join(",")));
    }
    for category in query.categories.iter().filter(|category| is_slug(category)) {
        facets.push(format!(r#"["categories:{category}"]"#));
    }
    match query.environment.as_deref() {
        Some("client") => facets.push(r#"["client_side:required","client_side:optional"]"#.into()),
        Some("server") => facets.push(r#"["server_side:required","server_side:optional"]"#.into()),
        _ => {}
    }
    if let Some(version) = query
        .game_version
        .as_deref()
        .filter(|version| is_version_id(version))
    {
        facets.push(format!(r#"["versions:{version}"]"#));
    }
    let facets = format!("[{}]", facets.join(","));
    let url = format!(
        "{API}/search?query={}&facets={}&index={}&offset={}&limit={}",
        url_encode(query.query.trim()),
        url_encode(&facets),
        query.sort.as_str(),
        query.offset,
        query.limit.clamp(1, MAX_LIMIT),
    );
    let response: SearchResponse = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(SearchResults {
        total: response.total_hits,
        offset: response.offset,
        hits: response.hits.into_iter().map(Project::from).collect(),
    })
}

/// Category slugs are lowercase words joined by dashes ("game-mechanics").
fn is_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 48
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Version ids are short and plain ("26.3", "1.14 Pre-Release 1"); anything else could break the
/// facet's JSON, so it is ignored.
fn is_version_id(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 64
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' '))
}

impl From<SearchHit> for Project {
    fn from(hit: SearchHit) -> Self {
        let loaders = hit
            .categories
            .iter()
            .filter(|category| LOADERS.contains(&category.as_str()))
            .cloned()
            .collect();
        // Prefer the small thumbnails; full-size gallery images can be several megabytes.
        let image_url = hit
            .featured_gallery
            .or_else(|| hit.gallery.into_iter().find(|url| url.contains("_350.")));
        Self {
            id: hit.project_id,
            slug: hit.slug,
            title: hit.title,
            author: hit.author,
            description: hit.description,
            project_type: hit.project_type,
            downloads: hit.downloads,
            follows: hit.follows,
            updated: hit.date_modified,
            loaders,
            game_version: hit.versions.last().cloned(),
            tags: hit.display_categories,
            image_url,
            icon_url: hit.icon_url,
            color: hit.color,
        }
    }
}

// ─── project pages ──────────────────────────────────────────────────────────────────────────────

/// Everything a project's page shows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDetails {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    /// The long description, in Markdown (with some HTML).
    pub body: String,
    pub project_type: ProjectType,
    pub downloads: u64,
    pub followers: u64,
    pub published: String,
    pub updated: String,
    pub categories: Vec<String>,
    pub loaders: Vec<String>,
    /// Oldest first, as Modrinth lists them.
    pub game_versions: Vec<String>,
    /// "required", "optional" or "unsupported".
    pub client_side: String,
    pub server_side: String,
    pub license: Option<License>,
    pub icon_url: Option<String>,
    pub color: Option<u32>,
    pub gallery: Vec<GalleryImage>,
    pub links: Vec<ProjectLink>,
    pub authors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct License {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryImage {
    /// A small thumbnail.
    pub url: String,
    pub raw_url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub featured: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLink {
    pub label: String,
    pub url: String,
}

#[derive(Deserialize)]
struct RawProject {
    id: String,
    slug: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    body: String,
    project_type: ProjectType,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    followers: u64,
    #[serde(default)]
    published: String,
    #[serde(default)]
    updated: String,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    additional_categories: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    client_side: String,
    #[serde(default)]
    server_side: String,
    license: Option<License>,
    icon_url: Option<String>,
    color: Option<u32>,
    #[serde(default)]
    gallery: Vec<RawGalleryImage>,
    source_url: Option<String>,
    issues_url: Option<String>,
    wiki_url: Option<String>,
    discord_url: Option<String>,
    #[serde(default)]
    donation_urls: Vec<RawDonation>,
}

#[derive(Deserialize)]
struct RawGalleryImage {
    url: String,
    #[serde(default)]
    raw_url: Option<String>,
    title: Option<String>,
    description: Option<String>,
    #[serde(default)]
    featured: bool,
    #[serde(default)]
    ordering: i64,
}

#[derive(Deserialize)]
struct RawDonation {
    platform: Option<String>,
    url: String,
}

#[derive(Deserialize)]
struct RawMember {
    user: RawUser,
    #[serde(default)]
    role: String,
}

#[derive(Deserialize)]
struct RawUser {
    username: String,
}

async fn get_json<T: serde::de::DeserializeOwned>(
    client: &HttpClient,
    url: &str,
) -> Result<T, Error> {
    Ok(client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

/// A project's page, by id or slug.
pub async fn project(client: &HttpClient, id: &str) -> Result<ProjectDetails, Error> {
    let raw: RawProject = get_json(client, &format!("{API}/project/{}", url_encode(id))).await?;
    // The owner first; a failure here just leaves the authors out.
    let authors = match get_json::<Vec<RawMember>>(
        client,
        &format!("{API}/project/{}/members", url_encode(id)),
    )
    .await
    {
        Ok(mut members) => {
            members.sort_by_key(|member| member.role != "Owner");
            members
                .into_iter()
                .map(|member| member.user.username)
                .collect()
        }
        Err(_) => Vec::new(),
    };

    let mut gallery = raw.gallery;
    gallery.sort_by_key(|image| (!image.featured, image.ordering));
    let mut links = Vec::new();
    for (label, url) in [
        ("Source", raw.source_url),
        ("Issues", raw.issues_url),
        ("Wiki", raw.wiki_url),
        ("Discord", raw.discord_url),
    ] {
        if let Some(url) =
            url.filter(|url| url.starts_with("https://") || url.starts_with("http://"))
        {
            links.push(ProjectLink {
                label: label.into(),
                url,
            });
        }
    }
    for donation in raw.donation_urls {
        if donation.url.starts_with("https://") || donation.url.starts_with("http://") {
            let platform = donation.platform.unwrap_or_else(|| "Donate".into());
            links.push(ProjectLink {
                label: format!("Donate ({platform})"),
                url: donation.url,
            });
        }
    }
    let mut categories = raw.categories;
    categories.extend(raw.additional_categories);
    categories.retain(|category| !LOADERS.contains(&category.as_str()));

    Ok(ProjectDetails {
        id: raw.id,
        slug: raw.slug,
        title: raw.title,
        description: raw.description,
        body: raw.body,
        project_type: raw.project_type,
        downloads: raw.downloads,
        followers: raw.followers,
        published: raw.published,
        updated: raw.updated,
        categories,
        loaders: raw.loaders,
        game_versions: raw.game_versions,
        client_side: raw.client_side,
        server_side: raw.server_side,
        license: raw.license,
        icon_url: raw.icon_url,
        color: raw.color,
        gallery: gallery
            .into_iter()
            .map(|image| GalleryImage {
                raw_url: image.raw_url.unwrap_or_else(|| image.url.clone()),
                url: image.url,
                title: image.title,
                description: image.description,
                featured: image.featured,
            })
            .collect(),
        links,
        authors,
    })
}

/// One downloadable release of a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectVersion {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    /// "release", "beta" or "alpha".
    pub version_type: String,
    pub loaders: Vec<String>,
    pub game_versions: Vec<String>,
    pub date_published: String,
    pub downloads: u64,
    pub changelog: String,
    pub files: Vec<VersionFile>,
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionFile {
    pub url: String,
    pub filename: String,
    pub primary: bool,
    pub size: u64,
    pub sha1: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    /// "required", "optional", "incompatible" or "embedded".
    pub dependency_type: String,
}

#[derive(Deserialize)]
struct RawVersion {
    id: String,
    project_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    version_number: String,
    #[serde(default)]
    version_type: String,
    #[serde(default)]
    loaders: Vec<String>,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    date_published: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    changelog: Option<String>,
    #[serde(default)]
    files: Vec<RawFile>,
    #[serde(default)]
    dependencies: Vec<RawDependency>,
}

#[derive(Deserialize)]
struct RawFile {
    url: String,
    filename: String,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    hashes: HashMap<String, String>,
}

#[derive(Deserialize)]
struct RawDependency {
    project_id: Option<String>,
    version_id: Option<String>,
    #[serde(default)]
    dependency_type: String,
}

impl From<RawVersion> for ProjectVersion {
    fn from(raw: RawVersion) -> Self {
        Self {
            id: raw.id,
            project_id: raw.project_id,
            name: raw.name,
            version_number: raw.version_number,
            version_type: raw.version_type,
            loaders: raw.loaders,
            game_versions: raw.game_versions,
            date_published: raw.date_published,
            downloads: raw.downloads,
            changelog: raw.changelog.unwrap_or_default(),
            files: raw
                .files
                .into_iter()
                .map(|file| VersionFile {
                    sha1: file.hashes.get("sha1").cloned(),
                    url: file.url,
                    filename: file.filename,
                    primary: file.primary,
                    size: file.size,
                })
                .collect(),
            dependencies: raw
                .dependencies
                .into_iter()
                .map(|dependency| Dependency {
                    project_id: dependency.project_id,
                    version_id: dependency.version_id,
                    dependency_type: dependency.dependency_type,
                })
                .collect(),
        }
    }
}

impl ProjectVersion {
    /// The file to install: the one marked primary, else the first.
    pub fn primary_file(&self) -> Option<&VersionFile> {
        self.files
            .iter()
            .find(|file| file.primary)
            .or(self.files.first())
    }
}

/// A project's versions, newest first, optionally only those for some loaders or game versions.
pub async fn project_versions(
    client: &HttpClient,
    id: &str,
    loaders: &[&str],
    game_versions: &[&str],
) -> Result<Vec<ProjectVersion>, Error> {
    let mut url = format!("{API}/project/{}/version", url_encode(id));
    let mut params = Vec::new();
    if !loaders.is_empty() {
        params.push(format!(
            "loaders={}",
            url_encode(&serde_json::to_string(loaders).unwrap_or_default())
        ));
    }
    if !game_versions.is_empty() {
        params.push(format!(
            "game_versions={}",
            url_encode(&serde_json::to_string(game_versions).unwrap_or_default())
        ));
    }
    if !params.is_empty() {
        url = format!("{url}?{}", params.join("&"));
    }
    let raw: Vec<RawVersion> = get_json(client, &url).await?;
    Ok(raw.into_iter().map(ProjectVersion::from).collect())
}

pub async fn version(client: &HttpClient, id: &str) -> Result<ProjectVersion, Error> {
    let raw: RawVersion = get_json(client, &format!("{API}/version/{}", url_encode(id))).await?;
    Ok(raw.into())
}

/// The Modrinth version a file belongs to, found by the file's SHA-1; None if Modrinth doesn't
/// have it (a file made by hand, or from somewhere else).
pub async fn version_by_hash(client: &HttpClient, sha1: &str) -> Result<Option<ProjectVersion>, Error> {
    let response = client
        .get(format!("{API}/version_file/{}?algorithm=sha1", url_encode(sha1)))
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let raw: RawVersion = response.error_for_status()?.json().await?;
    Ok(Some(raw.into()))
}

/// A Modrinth category, for the filter sidebar.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub name: String,
    /// Modrinth sends snake_case; Wisp's UI gets camelCase.
    #[serde(alias = "project_type")]
    pub project_type: String,
    /// Groups categories: "categories", "features", "resolutions", "performance impact".
    pub header: String,
    /// An SVG icon.
    #[serde(default)]
    pub icon: String,
    /// Words to show, when the name is an id (CurseForge's categories).
    #[serde(default)]
    pub label: Option<String>,
    /// A picture icon, when there's no SVG (CurseForge's categories).
    #[serde(default)]
    pub icon_url: Option<String>,
}

pub async fn categories(client: &HttpClient) -> Result<Vec<Category>, Error> {
    get_json(client, &format!("{API}/tag/category")).await
}

/// A mod or shader loader as Modrinth lists it, with its logo, for the filter sidebar.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderTag {
    pub name: String,
    /// An SVG icon.
    #[serde(default)]
    pub icon: String,
    #[serde(default, alias = "supported_project_types")]
    pub project_types: Vec<String>,
}

pub async fn loader_tags(client: &HttpClient) -> Result<Vec<LoaderTag>, Error> {
    get_json(client, &format!("{API}/tag/loader")).await
}

/// Short facts about several projects at once (names and icons for a dependency list).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub slug: String,
    pub title: String,
    #[serde(alias = "project_type")]
    pub project_type: ProjectType,
    #[serde(default, alias = "icon_url")]
    pub icon_url: Option<String>,
}

pub async fn projects(client: &HttpClient, ids: &[String]) -> Result<Vec<ProjectSummary>, Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids = serde_json::to_string(ids).unwrap_or_default();
    get_json(client, &format!("{API}/projects?ids={}", url_encode(&ids))).await
}

#[cfg(test)]
mod shape_tests {
    use super::*;

    #[test]
    fn reads_modrinths_snake_case_tags_and_summaries() {
        let category: Result<Category, _> = serde_json::from_str(
            r#"{"icon":"<svg/>","name":"adventure","project_type":"mod","header":"categories"}"#,
        );
        assert_eq!(category.unwrap().project_type, "mod");
        let summary: ProjectSummary = serde_json::from_str(
            r#"{"id":"AANobbMI","slug":"sodium","title":"Sodium","project_type":"mod","icon_url":"https://x/i.png"}"#,
        )
        .unwrap();
        assert_eq!(summary.icon_url.as_deref(), Some("https://x/i.png"));
        // And the UI still gets camelCase.
        let sent = serde_json::to_value(&summary).unwrap();
        assert_eq!(sent["iconUrl"], "https://x/i.png");
    }
}
