use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use bluepane_vfs::{Entry, EntryKind};

use crate::EngineError;

/// Name of the synthetic row that leads to the parent directory.
pub const PARENT_NAME: &str = "..";

/// How many visited directories each panel remembers.
const HISTORY_LIMIT: usize = 100;

/// True for the synthetic `..` row (no real entry can have this name).
pub fn is_parent_entry(entry: &Entry) -> bool {
    entry.name == PARENT_NAME
}

/// One of the two file panels.
///
/// `entries` is what the user sees: the `..` row (when there is a parent directory)
/// followed by the directory contents that match `filter`.
#[derive(Debug, Clone)]
pub struct Panel {
    pub cwd: PathBuf,
    pub entries: Vec<Entry>,
    pub cursor: usize,
    pub selected: BTreeSet<PathBuf>,
    /// Quick filter: only entries whose name contains this text (case-insensitive) are shown.
    pub filter: String,
    /// Set when the current directory could not be listed.
    pub error: Option<String>,
    /// The whole directory listing, before filtering and without the `..` row.
    listing: Vec<Entry>,
    history: Vec<PathBuf>,
    history_index: usize,
}

impl Panel {
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        let cwd = cwd.into();
        let mut panel = Panel {
            history: vec![cwd.clone()],
            history_index: 0,
            cwd,
            entries: Vec::new(),
            cursor: 0,
            selected: BTreeSet::new(),
            filter: String::new(),
            error: None,
            listing: Vec::new(),
        };
        panel.reload();
        panel
    }

    /// Re-reads the directory, keeping the cursor entry and the still-valid selection.
    pub fn reload(&mut self) {
        match bluepane_vfs::list_dir(&self.cwd) {
            Ok(entries) => {
                self.listing = entries;
                self.error = None;
            }
            Err(err) => {
                self.listing.clear();
                self.error = Some(err.to_string());
            }
        }
        self.selected
            .retain(|path| self.listing.iter().any(|e| &e.path == path));
        let keep = self.current().map(|e| e.path.clone());
        self.rebuild(keep.as_deref());
    }

    /// Recomputes `entries` from the listing and the filter and puts the cursor on `keep`
    /// (or, when that is gone, near the old position).
    fn rebuild(&mut self, keep: Option<&Path>) {
        let needle = self.filter.to_lowercase();
        self.entries.clear();
        if let Some(parent) = self.cwd.parent() {
            self.entries.push(Entry {
                name: PARENT_NAME.to_string(),
                path: parent.to_path_buf(),
                kind: EntryKind::Dir,
                size: 0,
                modified: None,
                hidden: false,
            });
        }
        self.entries.extend(
            self.listing
                .iter()
                .filter(|e| needle.is_empty() || e.name.to_lowercase().contains(&needle))
                .cloned(),
        );
        if let Some(index) = keep.and_then(|k| self.entries.iter().position(|e| e.path == k)) {
            self.cursor = index;
        }
        self.cursor = self.cursor.min(self.entries.len().saturating_sub(1));
    }

    /// The entry under the cursor.
    pub fn current(&self) -> Option<&Entry> {
        self.entries.get(self.cursor)
    }

    /// What an operation acts on: the visible selected entries, or the entry under the
    /// cursor. The `..` row is never a target. Selected entries hidden by the filter are
    /// left alone, so an operation only touches what the user can see.
    pub fn targets(&self) -> Vec<PathBuf> {
        let selected: Vec<PathBuf> = self
            .entries
            .iter()
            .filter(|e| self.selected.contains(&e.path))
            .map(|e| e.path.clone())
            .collect();
        if !selected.is_empty() {
            return selected;
        }
        self.current()
            .filter(|e| !is_parent_entry(e))
            .map(|e| vec![e.path.clone()])
            .unwrap_or_default()
    }

    /// Navigates to `dir` and records it in the history (forward history is dropped).
    pub fn change_dir(&mut self, dir: PathBuf) {
        if self.history.get(self.history_index) != Some(&dir) {
            self.history.truncate(self.history_index + 1);
            self.history.push(dir.clone());
            if self.history.len() > HISTORY_LIMIT {
                self.history.remove(0);
            }
            self.history_index = self.history.len() - 1;
        }
        self.show_dir(dir);
    }

    /// Switches the listing to `dir` without touching the history.
    fn show_dir(&mut self, dir: PathBuf) {
        self.cwd = dir;
        self.cursor = 0;
        self.selected.clear();
        self.filter.clear();
        self.reload();
    }

    pub fn can_go_back(&self) -> bool {
        self.history_index > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.history_index + 1 < self.history.len()
    }

    /// Goes to the previously visited directory.
    pub fn back(&mut self) -> Result<(), EngineError> {
        self.step_history(-1)
    }

    /// Goes to the directory visited after the current one (after `back`).
    pub fn forward(&mut self) -> Result<(), EngineError> {
        self.step_history(1)
    }

    fn step_history(&mut self, delta: isize) -> Result<(), EngineError> {
        let target = self
            .history_index
            .checked_add_signed(delta)
            .filter(|&i| i < self.history.len())
            .ok_or_else(|| {
                EngineError(if delta < 0 {
                    "brak wcześniejszego katalogu w historii".into()
                } else {
                    "brak późniejszego katalogu w historii".into()
                })
            })?;
        let dir = self.history[target].clone();
        if !dir.is_dir() {
            return Err(EngineError(format!(
                "katalog nie istnieje: {}",
                dir.display()
            )));
        }
        self.history_index = target;
        self.show_dir(dir);
        Ok(())
    }

    /// Enters the directory under the cursor; on the `..` row goes up instead.
    pub fn enter(&mut self) {
        let Some(entry) = self.current().filter(|e| e.is_dir()) else {
            return;
        };
        if is_parent_entry(entry) {
            self.up();
        } else {
            let dir = entry.path.clone();
            self.change_dir(dir);
        }
    }

    /// Goes to the parent directory and puts the cursor on the directory just left.
    pub fn up(&mut self) {
        let Some(parent) = self.cwd.parent().map(Path::to_path_buf) else {
            return;
        };
        let left = self.cwd.clone();
        self.change_dir(parent);
        if let Some(index) = self.entries.iter().position(|e| e.path == left) {
            self.cursor = index;
        }
    }

    pub fn move_cursor(&mut self, delta: i32) {
        let last = self.entries.len().saturating_sub(1) as i64;
        self.cursor = (self.cursor as i64 + i64::from(delta)).clamp(0, last) as usize;
    }

    /// Toggles selection of the entry under the cursor, then moves down (as Total Commander does).
    /// The `..` row cannot be selected; the cursor just moves down.
    pub fn toggle_select(&mut self) {
        if let Some(entry) = self.current().filter(|e| !is_parent_entry(e)) {
            let path = entry.path.clone();
            if !self.selected.remove(&path) {
                self.selected.insert(path);
            }
        }
        self.move_cursor(1);
    }

    /// Sets the quick filter (empty text clears it). With a non-empty filter the cursor
    /// jumps to the first match.
    pub fn set_filter(&mut self, text: &str) {
        let keep = self.current().map(|e| e.path.clone());
        self.filter = text.to_string();
        if self.filter.is_empty() {
            self.rebuild(keep.as_deref());
        } else {
            self.cursor = 0;
            self.rebuild(None);
            let first_match = self.entries.iter().position(|e| !is_parent_entry(e));
            self.cursor = first_match.unwrap_or(0);
        }
    }

    /// Removes the last character of the filter. Returns false when there was no filter.
    pub fn filter_backspace(&mut self) -> bool {
        if self.filter.is_empty() {
            return false;
        }
        let mut text = self.filter.clone();
        text.pop();
        self.set_filter(&text);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Panel on a temp dir holding `dir/`, `a.txt`, `b.txt`, `Alpha.md`.
    /// Rows: `..`, `dir`, `a.txt`, `Alpha.md`, `b.txt`.
    fn sample() -> (tempfile::TempDir, Panel) {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("dir")).unwrap();
        std::fs::write(tmp.path().join("a.txt"), "a").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "b").unwrap();
        std::fs::write(tmp.path().join("Alpha.md"), "x").unwrap();
        let panel = Panel::new(tmp.path());
        (tmp, panel)
    }

    fn names(panel: &Panel) -> Vec<&str> {
        panel.entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn parent_row_comes_first() {
        let (tmp, panel) = sample();
        assert_eq!(names(&panel), ["..", "dir", "a.txt", "Alpha.md", "b.txt"]);
        assert_eq!(panel.entries[0].path, tmp.path().parent().unwrap());
        assert_eq!(panel.cursor, 0);
    }

    #[test]
    fn cursor_is_clamped() {
        let (_tmp, mut panel) = sample();
        panel.move_cursor(-5);
        assert_eq!(panel.cursor, 0);
        panel.move_cursor(100);
        assert_eq!(panel.cursor, 4);
    }

    #[test]
    fn enter_and_up_round_trip() {
        let (tmp, mut panel) = sample();
        panel.move_cursor(1);
        assert_eq!(panel.current().unwrap().name, "dir");
        panel.enter();
        assert_eq!(panel.cwd, tmp.path().join("dir"));
        panel.up();
        assert_eq!(panel.cwd, tmp.path());
        assert_eq!(panel.current().unwrap().name, "dir");
    }

    #[test]
    fn enter_on_parent_row_goes_up() {
        let (tmp, mut panel) = sample();
        panel.enter();
        assert_eq!(panel.cwd, tmp.path().parent().unwrap());
    }

    #[test]
    fn parent_row_is_never_selected_or_targeted() {
        let (_tmp, mut panel) = sample();
        panel.toggle_select();
        assert!(panel.selected.is_empty());
        assert_eq!(panel.cursor, 1);
        panel.cursor = 0;
        assert!(panel.targets().is_empty());
    }

    #[test]
    fn toggle_selects_and_advances() {
        let (_tmp, mut panel) = sample();
        panel.move_cursor(2);
        panel.toggle_select();
        assert_eq!(panel.selected.len(), 1);
        assert_eq!(panel.cursor, 3);
        panel.move_cursor(-1);
        panel.toggle_select();
        assert!(panel.selected.is_empty());
    }

    #[test]
    fn targets_prefer_selection_over_cursor() {
        let (_tmp, mut panel) = sample();
        panel.move_cursor(1);
        assert_eq!(panel.targets().len(), 1);
        panel.toggle_select();
        panel.toggle_select();
        assert_eq!(panel.targets().len(), 2);
    }

    #[test]
    fn filter_matches_case_insensitively_and_keeps_parent_row() {
        let (_tmp, mut panel) = sample();
        panel.set_filter("AL");
        assert_eq!(names(&panel), ["..", "Alpha.md"]);
        assert_eq!(panel.current().unwrap().name, "Alpha.md");
        panel.set_filter(".TXT");
        assert_eq!(names(&panel), ["..", "a.txt", "b.txt"]);
        panel.set_filter("zzz");
        assert_eq!(names(&panel), [".."]);
        assert_eq!(panel.cursor, 0);
    }

    #[test]
    fn clearing_filter_keeps_cursor_on_the_same_entry() {
        let (_tmp, mut panel) = sample();
        panel.set_filter("b.t");
        assert_eq!(panel.current().unwrap().name, "b.txt");
        panel.set_filter("");
        assert_eq!(names(&panel).len(), 5);
        assert_eq!(panel.current().unwrap().name, "b.txt");
    }

    #[test]
    fn filter_backspace_shortens_then_reports_empty() {
        let (_tmp, mut panel) = sample();
        panel.set_filter("a.");
        assert!(panel.filter_backspace());
        assert_eq!(panel.filter, "a");
        assert!(panel.filter_backspace());
        assert!(!panel.filter_backspace());
        assert_eq!(panel.entries.len(), 5);
    }

    #[test]
    fn changing_directory_clears_filter_and_selection() {
        let (_tmp, mut panel) = sample();
        panel.set_filter("dir");
        panel.enter_by_name("dir");
        assert!(panel.filter.is_empty());
        assert!(panel.selected.is_empty());
    }

    #[test]
    fn hidden_selection_is_not_a_target() {
        let (_tmp, mut panel) = sample();
        panel.set_filter("a.txt");
        panel.toggle_select();
        panel.set_filter("b.txt");
        // a.txt is selected but filtered out; the cursor entry b.txt is the target.
        assert_eq!(panel.selected.len(), 1);
        let targets = panel.targets();
        assert_eq!(targets.len(), 1);
        assert!(targets[0].ends_with("b.txt"));
    }

    #[test]
    fn history_goes_back_and_forward() {
        let (tmp, mut panel) = sample();
        panel.enter_by_name("dir");
        panel.up();
        assert!(panel.can_go_back());
        assert!(!panel.can_go_forward());

        panel.back().unwrap();
        assert_eq!(panel.cwd, tmp.path().join("dir"));
        panel.back().unwrap();
        assert_eq!(panel.cwd, tmp.path());
        assert!(!panel.can_go_back());
        assert!(panel.back().is_err());

        panel.forward().unwrap();
        assert_eq!(panel.cwd, tmp.path().join("dir"));
        assert!(panel.can_go_forward());
    }

    #[test]
    fn new_navigation_drops_forward_history() {
        let (tmp, mut panel) = sample();
        panel.enter_by_name("dir");
        panel.back().unwrap();
        assert!(panel.can_go_forward());
        panel.change_dir(tmp.path().parent().unwrap().to_path_buf());
        assert!(!panel.can_go_forward());
        assert!(panel.forward().is_err());
    }

    #[test]
    fn history_skips_removed_directory() {
        let (tmp, mut panel) = sample();
        panel.enter_by_name("dir");
        panel.up();
        std::fs::remove_dir(tmp.path().join("dir")).unwrap();
        let err = panel.back().unwrap_err();
        assert!(err.0.contains("nie istnieje"));
        assert_eq!(panel.cwd, tmp.path());
    }

    impl Panel {
        fn enter_by_name(&mut self, name: &str) {
            self.cursor = self.entries.iter().position(|e| e.name == name).unwrap();
            self.enter();
        }
    }
}
