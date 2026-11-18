//! Deciding what a world should do, from its history in the sync folder and the copy on this
//! computer. Pure: no files are touched here, so every case can be tested.

use super::remote::{Commit, descends, heads};

/// The copy on this computer.
#[derive(Debug, Clone, Copy)]
pub struct Local<'a> {
    /// What it holds now (its content hash), or None when it isn't on this computer.
    pub content: Option<&'a str>,
    /// The commit it last synced with, and what it held then.
    pub base: Option<(&'a str, &'a str)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Nothing to do.
    InStep,
    /// The cloud already has exactly this: just remember that commit as synced.
    Adopt { head: String },
    /// Send this computer's copy, on top of `parents`.
    Upload { parents: Vec<String> },
    /// The cloud's copy is newer and this one hasn't changed: bring it here.
    Download { head: String },
    /// Computers synced identical copies at the same time: tie the branches back together.
    Join { heads: Vec<String> },
    /// Changed here and elsewhere, or two different copies meeting for the first time. Worlds
    /// are never merged, so someone has to choose.
    Conflict { heads: Vec<String> },
    /// In the cloud and never on this computer.
    New { head: String },
    /// Synced here once, but not on this computer any more (removed here): the cloud's copy is
    /// left alone, and offered back.
    Gone { head: String },
    /// Taken out of the cloud on purpose; this computer keeps its copy and stops syncing it.
    Removed,
}

pub fn plan(commits: &[Commit], local: Local) -> Plan {
    let all = heads(commits);
    if all.is_empty() {
        return match local.content {
            Some(_) => Plan::Upload { parents: Vec::new() },
            None => Plan::InStep,
        };
    }
    // A removal followed by someone syncing it again counts as put back.
    let live: Vec<&Commit> = all.iter().copied().filter(|commit| !commit.removed).collect();
    if live.is_empty() {
        return Plan::Removed;
    }
    if live.len() > 1 {
        let ids = live.iter().map(|commit| commit.id.clone()).collect();
        return if live.iter().all(|commit| commit.content == live[0].content) {
            Plan::Join { heads: ids }
        } else {
            Plan::Conflict { heads: ids }
        };
    }
    let head = live[0];
    let Some(content) = local.content else {
        return match local.base {
            Some(_) => Plan::Gone { head: head.id.clone() },
            None => Plan::New { head: head.id.clone() },
        };
    };
    if content == head.content {
        return match local.base {
            Some((base, _)) if base == head.id => Plan::InStep,
            _ => Plan::Adopt { head: head.id.clone() },
        };
    }
    let Some((base, base_content)) = local.base else {
        // The same world on two computers that never synced it: different copies.
        return Plan::Conflict { heads: vec![head.id.clone()] };
    };
    let changed_here = content != base_content;
    if base == head.id {
        return Plan::Upload { parents: vec![head.id.clone()] };
    }
    // The cloud moved on from where this computer was (or its history was tidied away).
    let moved_on = descends(commits, &head.id, base) || !commits.iter().any(|commit| commit.id == base);
    match (moved_on, changed_here) {
        (true, false) => Plan::Download { head: head.id.clone() },
        _ => Plan::Conflict { heads: vec![head.id.clone()] },
    }
}

#[cfg(test)]
mod tests {
    use super::super::remote::{ItemKind, commit_id};
    use super::*;

    fn make(parents: &[&Commit], at: i64, content: &str) -> Commit {
        let mut commit = Commit {
            id: String::new(),
            item: "w-k".into(),
            kind: ItemKind::World,
            parents: parents.iter().map(|parent| parent.id.clone()).collect(),
            device: "d".into(),
            device_name: "D".into(),
            at,
            instance: "i".into(),
            world: Some("W".into()),
            name: "W".into(),
            manifest: "m".into(),
            content: content.into(),
            files: 1,
            bytes: 1,
            removed: false,
        };
        commit.id = commit_id(&commit);
        commit
    }

    fn here<'a>(content: Option<&'a str>, base: Option<(&'a str, &'a str)>) -> Local<'a> {
        Local { content, base }
    }

    #[test]
    fn first_sync_and_steady_state() {
        assert_eq!(plan(&[], here(Some("a"), None)), Plan::Upload { parents: vec![] });
        assert_eq!(plan(&[], here(None, None)), Plan::InStep);
        let a = make(&[], 1, "a");
        let history = vec![a.clone()];
        assert_eq!(plan(&history, here(Some("a"), Some((&a.id, "a")))), Plan::InStep);
        // Played here since: send it.
        assert_eq!(plan(&history, here(Some("b"), Some((&a.id, "a")))), Plan::Upload { parents: vec![a.id.clone()] });
        // The same copy turned up on both computers: nothing to move.
        assert_eq!(plan(&history, here(Some("a"), None)), Plan::Adopt { head: a.id.clone() });
    }

    #[test]
    fn newer_in_the_cloud_and_conflicts() {
        let a = make(&[], 1, "a");
        let b = make(&[&a], 2, "b");
        let history = vec![a.clone(), b.clone()];
        // Unchanged here, played elsewhere: bring it.
        assert_eq!(plan(&history, here(Some("a"), Some((&a.id, "a")))), Plan::Download { head: b.id.clone() });
        // Played in both places: ask.
        assert_eq!(plan(&history, here(Some("c"), Some((&a.id, "a")))), Plan::Conflict { heads: vec![b.id.clone()] });
        // Two different worlds of the same name meet for the first time: ask.
        assert_eq!(plan(&history, here(Some("z"), None)), Plan::Conflict { heads: vec![b.id.clone()] });
        // History tidied away past where this computer was: unchanged here still means download.
        assert_eq!(plan(std::slice::from_ref(&b), here(Some("a"), Some((&a.id, "a")))), Plan::Download { head: b.id.clone() });
    }

    #[test]
    fn branches_join_when_identical_and_conflict_when_not() {
        let a = make(&[], 1, "a");
        let b = make(&[&a], 2, "same");
        let c = make(&[&a], 3, "same");
        let mut joined = plan(&[a.clone(), b.clone(), c.clone()], here(Some("a"), Some((&a.id, "a"))));
        if let Plan::Join { heads } = &mut joined {
            heads.sort();
        }
        let mut expected = vec![b.id.clone(), c.id.clone()];
        expected.sort();
        assert_eq!(joined, Plan::Join { heads: expected });
        let d = make(&[&a], 4, "different");
        assert!(matches!(plan(&[a.clone(), b.clone(), d], here(Some("a"), Some((&a.id, "a")))), Plan::Conflict { heads } if heads.len() == 2));
    }

    #[test]
    fn missing_here_and_removed() {
        let a = make(&[], 1, "a");
        assert_eq!(plan(std::slice::from_ref(&a), here(None, None)), Plan::New { head: a.id.clone() });
        assert_eq!(plan(std::slice::from_ref(&a), here(None, Some((&a.id, "a")))), Plan::Gone { head: a.id.clone() });
        let mut gone = make(&[&a], 2, "");
        gone.removed = true;
        gone.id = commit_id(&gone);
        assert_eq!(plan(&[a.clone(), gone.clone()], here(Some("a"), Some((&a.id, "a")))), Plan::Removed);
        // Put back after a removal: it's a live world again.
        let back = make(&[&gone], 3, "a");
        assert_eq!(plan(&[a.clone(), gone, back.clone()], here(Some("a"), None)), Plan::Adopt { head: back.id.clone() });
    }
}
