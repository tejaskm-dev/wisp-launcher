//! Public feeds shown on the home screen: popular modpacks from Modrinth and Java Edition
//! patch notes from Mojang's launcher feed.

use serde::{Deserialize, Serialize};

use crate::http::{Error, HttpClient};
use crate::modrinth::{self, Project, ProjectType, SearchQuery, SearchSort};

const PATCH_NOTES: &str = "https://launchercontent.mojang.com/v2/javaPatchNotes.json";
const MOJANG_CONTENT: &str = "https://launchercontent.mojang.com";

/// The most-followed modpacks on Modrinth.
pub async fn featured_modpacks(client: &HttpClient, limit: u32) -> Result<Vec<Project>, Error> {
    let query = SearchQuery {
        source: Default::default(),
        query: String::new(),
        project_type: ProjectType::Modpack,
        sort: SearchSort::Follows,
        loaders: Vec::new(),
        categories: Vec::new(),
        game_version: None,
        environment: None,
        offset: 0,
        limit,
    };
    Ok(modrinth::search(client, &query).await?.hits)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchNote {
    pub id: String,
    pub title: String,
    pub version: String,
    /// "release" or "snapshot".
    pub kind: String,
    /// RFC 3339 timestamp.
    pub date: String,
    /// The first paragraph of Mojang's summary.
    pub summary: String,
    pub image_url: Option<String>,
}

#[derive(Deserialize)]
struct PatchNotesFeed {
    entries: Vec<PatchNoteEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchNoteEntry {
    id: String,
    title: String,
    version: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    short_text: String,
    image: Option<FeedImage>,
}

#[derive(Deserialize)]
struct FeedImage {
    url: String,
}

/// The newest Java Edition release and the newest snapshot, newest first.
pub async fn java_news(client: &HttpClient) -> Result<Vec<PatchNote>, Error> {
    let feed: PatchNotesFeed = client
        .get(PATCH_NOTES)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let mut entries = feed.entries;
    // RFC 3339 timestamps sort chronologically as plain strings.
    entries.sort_by(|a, b| b.date.cmp(&a.date));
    let release = entries.iter().position(|entry| entry.kind == "release");
    let snapshot = entries.iter().position(|entry| entry.kind == "snapshot");
    let mut picked: Vec<usize> = [release, snapshot].into_iter().flatten().collect();
    picked.sort_unstable();

    Ok(picked
        .into_iter()
        .map(|index| {
            let entry = &entries[index];
            PatchNote {
                id: entry.id.clone(),
                title: entry.title.clone(),
                version: entry.version.clone(),
                kind: entry.kind.clone(),
                date: entry.date.clone(),
                summary: first_paragraph(&entry.short_text),
                image_url: entry
                    .image
                    .as_ref()
                    .map(|image| format!("{MOJANG_CONTENT}{}", image.url)),
            }
        })
        .collect())
}

fn first_paragraph(text: &str) -> String {
    text.replace("\r\n", "\n")
        .split("\n\n")
        .next()
        .unwrap_or_default()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{PatchNote, first_paragraph};

    // The UI reads these exact names (src/data/types.ts), so a rename here must happen there too.
    #[test]
    fn patch_notes_serialize_with_the_field_names_the_ui_reads() {
        let note = serde_json::to_value(PatchNote {
            id: "id".into(),
            title: "Title".into(),
            version: "26.3".into(),
            kind: "release".into(),
            date: "2026-09-15T11:23:02.000Z".into(),
            summary: "Summary".into(),
            image_url: None,
        })
        .unwrap();
        for key in ["kind", "date", "summary", "imageUrl"] {
            assert!(note.get(key).is_some(), "PatchNote is missing `{key}`");
        }
    }

    #[test]
    fn keeps_only_the_first_paragraph() {
        assert_eq!(
            first_paragraph("Dragon fixes.\r\n\r\nHappy dragon slaying!"),
            "Dragon fixes."
        );
        assert_eq!(first_paragraph("  One line  "), "One line");
        assert_eq!(first_paragraph(""), "");
    }
}
