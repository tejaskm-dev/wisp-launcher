//! Undoing Wisp's fixes, and doing the ones it offers instead of making.

use tauri::{AppHandle, State};
use wisp_core::doctor::{self, Action, Change};
use wisp_core::journal::{Event, JournalQuery};

use crate::{Launches, Paths, library, now_ms};

const CLOSE_GAME: &str = "Close Minecraft first.";

/// Puts back what a fix changed.
#[tauri::command]
pub async fn undo_fix(entry_id: String, app: AppHandle, paths: State<'_, Paths>, launches: State<'_, Launches>) -> Result<(), String> {
    let journal = paths.journal();
    let entry = journal.get(&entry_id).ok_or("That fix isn't in the journal anymore.")?;
    let Event::Fixed { changes, .. } = &entry.event else {
        return Err("That isn't one of Wisp's fixes.".into());
    };
    let instance_id = entry.instance_id.clone().ok_or("That fix isn't tied to an instance.")?;
    if library::busy(&launches, &instance_id) {
        return Err(CLOSE_GAME.into());
    }
    let undone = journal
        .list(&JournalQuery { kinds: vec!["fix-undone".into()], ..Default::default() })
        .iter()
        .any(|other| matches!(&other.event, Event::FixUndone { fix } if *fix == entry_id));
    if undone {
        return Err("That fix was already undone.".into());
    }
    doctor::undo(&paths.instances(), &instance_id, changes)?;
    library::note_event(&app, Some(&instance_id), None, Event::FixUndone { fix: entry_id });
    Ok(())
}

/// Does one of the fixes Wisp offered after a crash. Returns its journal entry, for Undo.
#[tauri::command]
pub async fn apply_fix(instance_id: String, action: Action, app: AppHandle, paths: State<'_, Paths>, launches: State<'_, Launches>) -> Result<Option<String>, String> {
    if library::busy(&launches, &instance_id) {
        return Err(CLOSE_GAME.into());
    }
    let change = doctor::apply(&paths.instances(), &instance_id, &action)?;
    let sentence = match &change {
        Change::Disabled { name, .. } => format!("Turned off {name}."),
        _ => "Made the change.".into(),
    };
    let event = Event::Fixed { found: Vec::new(), fixed: vec![sentence], changes: vec![change], after_crash: true };
    let entry = paths.journal().append(Some(&instance_id), None, now_ms(), event).ok();
    library::journal_changed(&app);
    Ok(entry.map(|entry| entry.id))
}
