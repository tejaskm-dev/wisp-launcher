//! The journal: Wisp's history of your game, like a git log. Play sessions with the worlds and
//! screenshots in them, the notes you leave, restore points, and mods and modpacks coming and
//! going. Kept as one JSON object per line in journal/events.jsonl, oldest first, so writing is
//! a single append.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use serde::{Deserialize, Serialize};

use crate::snapshots::SnapshotReason;

const EVENTS: &str = "events.jsonl";
/// Notes are for a sentence or two, not novels.
pub const NOTE_LIMIT: usize = 2000;

/// One writer at a time, across every Journal value in the process.
static WRITE: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    /// Unix milliseconds.
    pub at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    /// The world (its folder in saves/) this is about, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world: Option<String>,
    pub event: Event,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum Event {
    /// A game ran from `started_at` to `ended_at`.
    Session {
        started_at: i64,
        ended_at: i64,
        crashed: bool,
        /// Worlds saved during the session (their folders).
        worlds: Vec<String>,
        /// Worlds made during the session.
        #[serde(default)]
        new_worlds: Vec<String>,
        /// Screenshots taken during the session (files in the instance's screenshots folder).
        screenshots: Vec<String>,
        /// The computer it was played on.
        device: String,
        /// The instance as people know it, e.g. "Fabric 26.3".
        label: String,
    },
    Note {
        text: String,
        /// The session it's about, if it was left after one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session: Option<String>,
    },
    RestorePoint {
        snapshot: String,
        reason: SnapshotReason,
        files: u32,
        total_bytes: u64,
        added_bytes: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    Restored {
        snapshot: String,
    },
    ContentAdded {
        project_id: String,
        title: String,
        project_type: String,
        version: String,
        /// How many required projects came along with it.
        #[serde(default)]
        dependencies: u32,
    },
    ContentRemoved {
        project_id: String,
        title: String,
        project_type: String,
    },
    InstanceCreated {
        name: String,
        label: String,
    },
    ModpackInstalled {
        title: String,
        version: String,
    },
    WorldImported {
        name: String,
    },
    WorldExported {
        name: String,
    },
    WorldDeleted {
        name: String,
    },
}

impl Event {
    pub fn kind(&self) -> &'static str {
        match self {
            Event::Session { .. } => "session",
            Event::Note { .. } => "note",
            Event::RestorePoint { .. } => "restore-point",
            Event::Restored { .. } => "restored",
            Event::ContentAdded { .. } => "content-added",
            Event::ContentRemoved { .. } => "content-removed",
            Event::InstanceCreated { .. } => "instance-created",
            Event::ModpackInstalled { .. } => "modpack-installed",
            Event::WorldImported { .. } => "world-imported",
            Event::WorldExported { .. } => "world-exported",
            Event::WorldDeleted { .. } => "world-deleted",
        }
    }
}

/// Which entries to list.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JournalQuery {
    pub instance_id: Option<String>,
    pub world: Option<String>,
    /// Only these kinds ("session", "note", ...); empty for all.
    pub kinds: Vec<String>,
    /// Only entries before this time (for paging back).
    pub before: Option<i64>,
    pub limit: Option<usize>,
}

impl JournalQuery {
    fn matches(&self, entry: &Entry) -> bool {
        self.instance_id.as_ref().is_none_or(|id| entry.instance_id.as_ref() == Some(id))
            && self.world.as_ref().is_none_or(|world| {
                entry.world.as_ref() == Some(world)
                    || matches!(&entry.event, Event::Session { worlds, .. } if worlds.contains(world))
            })
            && (self.kinds.is_empty() || self.kinds.iter().any(|kind| kind == entry.event.kind()))
            && self.before.is_none_or(|before| entry.at < before)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("couldn't update the journal: {0}")]
    Io(#[from] std::io::Error),
    #[error("that entry is gone")]
    Missing,
    #[error("only notes can be changed")]
    NotANote,
    #[error("a note can't be empty")]
    Empty,
}

pub struct Journal {
    dir: PathBuf,
}

/// A new entry id: time first, so ids sort by when they happened.
pub fn new_id(at: i64) -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.subsec_nanos());
    let salt = nanos ^ COUNTER.fetch_add(1, Ordering::Relaxed).wrapping_mul(2_654_435_761) ^ std::process::id();
    format!("{:011x}{:08x}", at.max(0), salt)
}

impl Journal {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn file(&self) -> PathBuf {
        self.dir.join(EVENTS)
    }

    /// Everything, oldest first. Lines that don't parse (written by a newer Wisp) are skipped.
    fn read_all(&self) -> Vec<Entry> {
        let Ok(text) = std::fs::read_to_string(self.file()) else {
            return Vec::new();
        };
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    /// Adds an entry at the end.
    pub fn append(&self, instance_id: Option<&str>, world: Option<&str>, at: i64, event: Event) -> Result<Entry, JournalError> {
        let entry = Entry {
            id: new_id(at),
            at,
            instance_id: instance_id.map(str::to_string),
            world: world.map(str::to_string),
            event,
        };
        let line = serde_json::to_string(&entry).map_err(std::io::Error::other)?;
        let _guard = WRITE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        std::fs::create_dir_all(&self.dir)?;
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(self.file())?;
        writeln!(file, "{line}")?;
        Ok(entry)
    }

    /// Matching entries, newest first.
    pub fn list(&self, query: &JournalQuery) -> Vec<Entry> {
        let mut entries: Vec<Entry> = self.read_all().into_iter().filter(|entry| query.matches(entry)).collect();
        entries.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.id.cmp(&a.id)));
        if let Some(limit) = query.limit {
            entries.truncate(limit);
        }
        entries
    }

    /// The newest note about a world, if there is one.
    pub fn latest_note(&self, instance_id: &str, world: &str) -> Option<Entry> {
        self.read_all()
            .into_iter()
            .filter(|entry| {
                matches!(entry.event, Event::Note { .. })
                    && entry.instance_id.as_deref() == Some(instance_id)
                    && entry.world.as_deref() == Some(world)
            })
            .max_by_key(|entry| entry.at)
    }

    /// Rewrites the file with one entry changed or dropped; only notes can be.
    fn rewrite(&self, id: &str, change: impl FnOnce(&mut Vec<Entry>, usize) -> Result<(), JournalError>) -> Result<(), JournalError> {
        let _guard = WRITE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut entries = self.read_all();
        let index = entries.iter().position(|entry| entry.id == id).ok_or(JournalError::Missing)?;
        if !matches!(entries[index].event, Event::Note { .. }) {
            return Err(JournalError::NotANote);
        }
        change(&mut entries, index)?;
        let mut text = String::new();
        for entry in &entries {
            text.push_str(&serde_json::to_string(entry).map_err(std::io::Error::other)?);
            text.push('\n');
        }
        let temp = self.dir.join(format!("{EVENTS}.tmp"));
        std::fs::write(&temp, text)?;
        std::fs::rename(temp, self.file())?;
        Ok(())
    }

    pub fn edit_note(&self, id: &str, text: &str) -> Result<(), JournalError> {
        let text = clean_note(text)?;
        self.rewrite(id, |entries, index| {
            if let Event::Note { text: old, .. } = &mut entries[index].event {
                *old = text;
            }
            Ok(())
        })
    }

    pub fn delete_note(&self, id: &str) -> Result<(), JournalError> {
        self.rewrite(id, |entries, index| {
            entries.remove(index);
            Ok(())
        })
    }
}

/// Trims a note and caps its length.
pub fn clean_note(text: &str) -> Result<String, JournalError> {
    let text: String = text.trim().chars().take(NOTE_LIMIT).collect();
    if text.is_empty() {
        return Err(JournalError::Empty);
    }
    Ok(text)
}

/// What happened in a game folder during a session: which worlds were saved or made, and which
/// screenshots were taken. Times are Unix milliseconds.
pub struct SessionFacts {
    pub worlds: Vec<String>,
    pub new_worlds: Vec<String>,
    pub screenshots: Vec<String>,
}

pub fn session_facts(game_dir: &Path, started_at: i64, ended_at: i64) -> SessionFacts {
    // A little slack either side: clocks and file times are coarse.
    let from = started_at - 2_000;
    let to = ended_at + 10_000;
    let worlds = crate::worlds::list_info(game_dir);
    SessionFacts {
        worlds: worlds
            .iter()
            .filter(|world| world.last_played.is_some_and(|played| played >= from))
            .map(|world| world.folder.clone())
            .collect(),
        new_worlds: worlds
            .iter()
            .filter(|world| world.created.is_some_and(|created| created >= from))
            .map(|world| world.folder.clone())
            .collect(),
        screenshots: crate::screenshots::taken_between(game_dir, from, to),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_lists_filters_and_edits_notes() {
        let dir = std::env::temp_dir().join(format!("wisp-journal-{}", std::process::id()));
        let journal = Journal::new(&dir);
        let session = Event::Session {
            started_at: 1_000,
            ended_at: 61_000,
            crashed: false,
            worlds: vec!["New World".into()],
            new_worlds: vec![],
            screenshots: vec!["a.png".into()],
            device: "Mac".into(),
            label: "Fabric 26.3".into(),
        };
        journal.append(Some("fabric"), None, 61_000, session).unwrap();
        let note = journal
            .append(Some("fabric"), Some("New World"), 62_000, Event::Note { text: "Built the bridge".into(), session: None })
            .unwrap();
        journal
            .append(
                Some("vanilla"),
                None,
                63_000,
                Event::ContentAdded {
                    project_id: "AANobbMI".into(),
                    title: "Sodium".into(),
                    project_type: "mod".into(),
                    version: "0.6".into(),
                    dependencies: 0,
                },
            )
            .unwrap();
        // Lines from a newer Wisp are skipped, not fatal.
        std::fs::OpenOptions::new()
            .append(true)
            .open(dir.join(EVENTS))
            .unwrap()
            .write_all(b"{\"id\":\"x\",\"at\":1,\"event\":{\"kind\":\"from-the-future\"}}\n")
            .unwrap();

        let all = journal.list(&JournalQuery::default());
        assert_eq!(all.iter().map(|entry| entry.at).collect::<Vec<_>>(), vec![63_000, 62_000, 61_000]);
        // A world's journal includes the sessions that saved it.
        let world = journal.list(&JournalQuery { world: Some("New World".into()), ..Default::default() });
        assert_eq!(world.len(), 2);
        let notes = journal.list(&JournalQuery { kinds: vec!["note".into()], ..Default::default() });
        assert_eq!(notes.len(), 1);
        assert_eq!(journal.list(&JournalQuery { before: Some(62_000), ..Default::default() }).len(), 1);

        journal.edit_note(&note.id, "  Finished the bridge  ").unwrap();
        let latest = journal.latest_note("fabric", "New World").unwrap();
        assert!(matches!(latest.event, Event::Note { ref text, .. } if text == "Finished the bridge"));
        assert!(matches!(journal.edit_note(&all[0].id, "x"), Err(JournalError::NotANote)));
        assert!(matches!(journal.edit_note(&note.id, "   "), Err(JournalError::Empty)));
        journal.delete_note(&note.id).unwrap();
        assert!(journal.latest_note("fabric", "New World").is_none());
        assert_eq!(journal.list(&JournalQuery::default()).len(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn ids_are_unique_and_sort_by_time() {
        let a = new_id(5);
        let b = new_id(5);
        assert_ne!(a, b);
        assert!(new_id(4) < new_id(6));
    }
}
