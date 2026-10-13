//! What both folders looked like right after the last successful synchronization.
//!
//! Two-way sync needs it to tell "deleted on the left" from "new on the right": without a
//! memory both look the same (present on one side only). The snapshot is stored in a small
//! text file inside both folders and is only trusted when it was written for the very same
//! pair of folders.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::compare::{Category, Diff};
use crate::scan::{Kind, Node};

/// File name of the snapshot inside each synchronized folder (never synchronized itself).
pub const STATE_FILE: &str = ".bluepane-sync.db";

const HEADER: &str = "bluepane-sync 1";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncState {
    pub left: BTreeMap<String, Node>,
    pub right: BTreeMap<String, Node>,
}

impl SyncState {
    /// A snapshot of the items that are equal on both sides right now. Items that still
    /// differ (skipped, in conflict, failed) are deliberately left out: remembering them
    /// would make the next run believe nobody ever touched them.
    pub fn from_diffs(diffs: &[Diff]) -> SyncState {
        let mut state = SyncState::default();
        for diff in diffs.iter().filter(|d| d.category == Category::Equal) {
            if let (Some(l), Some(r)) = (diff.left, diff.right) {
                state.left.insert(diff.path.clone(), l);
                state.right.insert(diff.path.clone(), r);
            }
        }
        state
    }

    /// Writes the snapshot into both folders. Failing to write is not fatal for the sync
    /// that just happened, so the caller may ignore the error (two-way will then be careful).
    pub fn save(&self, left_root: &Path, right_root: &Path) -> io::Result<()> {
        let text = self.render(left_root, right_root);
        let a = fs::write(left_root.join(STATE_FILE), &text);
        let b = fs::write(right_root.join(STATE_FILE), &text);
        a.and(b)
    }

    /// Reads the snapshot written for exactly this pair of folders, from either of them.
    pub fn load(left_root: &Path, right_root: &Path) -> Option<SyncState> {
        [left_root, right_root].iter().find_map(|root| {
            let text = fs::read_to_string(root.join(STATE_FILE)).ok()?;
            SyncState::parse(&text, left_root, right_root)
        })
    }

    /// Forgets the snapshot (both folders).
    pub fn forget(left_root: &Path, right_root: &Path) {
        let _ = fs::remove_file(left_root.join(STATE_FILE));
        let _ = fs::remove_file(right_root.join(STATE_FILE));
    }

    fn render(&self, left_root: &Path, right_root: &Path) -> String {
        let mut out = format!(
            "{HEADER}\n{}\n{}\n",
            left_root.display(),
            right_root.display()
        );
        for (side, nodes) in [('L', &self.left), ('R', &self.right)] {
            for (path, node) in nodes {
                let (secs, nanos) = node.modified.map_or((0, 0), split_time);
                let kind = if node.kind == Kind::Dir { 'D' } else { 'F' };
                out.push_str(&format!(
                    "{side}\t{kind}\t{}\t{}\t{}\t{}\t{path}\n",
                    node.size,
                    node.modified.is_some() as u8,
                    secs,
                    nanos
                ));
            }
        }
        out
    }

    fn parse(text: &str, left_root: &Path, right_root: &Path) -> Option<SyncState> {
        let mut lines = text.lines();
        if lines.next()? != HEADER
            || lines.next()? != left_root.display().to_string()
            || lines.next()? != right_root.display().to_string()
        {
            return None;
        }
        let mut state = SyncState::default();
        for line in lines {
            let mut f = line.splitn(7, '\t');
            let (side, kind, size, has_time, secs, nanos, path) = (
                f.next()?,
                f.next()?,
                f.next()?,
                f.next()?,
                f.next()?,
                f.next()?,
                f.next()?,
            );
            let modified = (has_time == "1").then(|| {
                SystemTime::UNIX_EPOCH
                    + Duration::new(secs.parse().unwrap_or(0), nanos.parse().unwrap_or(0))
            });
            let node = Node {
                kind: if kind == "D" { Kind::Dir } else { Kind::File },
                size: size.parse().ok()?,
                modified,
            };
            match side {
                "L" => state.left.insert(path.to_string(), node),
                "R" => state.right.insert(path.to_string(), node),
                _ => return None,
            };
        }
        Some(state)
    }
}

fn split_time(t: SystemTime) -> (u64, u32) {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map_or((0, 0), |d| (d.as_secs(), d.subsec_nanos()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(size: u64, secs: u64) -> Node {
        Node {
            kind: Kind::File,
            size,
            modified: Some(SystemTime::UNIX_EPOCH + Duration::new(secs, 123)),
        }
    }

    #[test]
    fn a_snapshot_round_trips_including_odd_names() {
        let tmp = tempfile::tempdir().unwrap();
        let (l, r) = (tmp.path().join("l"), tmp.path().join("r"));
        fs::create_dir_all(&l).unwrap();
        fs::create_dir_all(&r).unwrap();
        let mut state = SyncState::default();
        state
            .left
            .insert("a b/ąę.txt".into(), node(5, 1_700_000_000));
        state
            .right
            .insert("a b/ąę.txt".into(), node(5, 1_700_000_001));
        state.left.insert(
            "dir".into(),
            Node {
                kind: Kind::Dir,
                size: 0,
                modified: None,
            },
        );
        state.save(&l, &r).unwrap();
        assert_eq!(SyncState::load(&l, &r), Some(state.clone()));
        // Either copy is enough.
        fs::remove_file(l.join(STATE_FILE)).unwrap();
        assert_eq!(SyncState::load(&l, &r), Some(state));
    }

    #[test]
    fn a_snapshot_of_another_pair_of_folders_is_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let (l, r, other) = (
            tmp.path().join("l"),
            tmp.path().join("r"),
            tmp.path().join("other"),
        );
        for d in [&l, &r, &other] {
            fs::create_dir_all(d).unwrap();
        }
        SyncState::default().save(&l, &r).unwrap();
        fs::copy(l.join(STATE_FILE), other.join(STATE_FILE)).unwrap();
        // Both files on offer were written for (l, r): neither fits (other, r).
        assert_eq!(SyncState::load(&other, &r), None);
    }

    #[test]
    fn a_damaged_file_is_not_trusted() {
        let tmp = tempfile::tempdir().unwrap();
        let (l, r) = (tmp.path().join("l"), tmp.path().join("r"));
        fs::create_dir_all(&l).unwrap();
        fs::create_dir_all(&r).unwrap();
        SyncState::default().save(&l, &r).unwrap();
        let mut text = fs::read_to_string(l.join(STATE_FILE)).unwrap();
        text.push_str("L\tF\tnot-a-number\t0\t0\t0\tx\n");
        fs::write(l.join(STATE_FILE), text).unwrap();
        fs::remove_file(r.join(STATE_FILE)).unwrap();
        assert_eq!(SyncState::load(&l, &r), None);
    }
}
