use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use bluepane_vfs::Entry;

/// One of the two file panels.
#[derive(Debug, Clone)]
pub struct Panel {
    pub cwd: PathBuf,
    pub entries: Vec<Entry>,
    pub cursor: usize,
    pub selected: BTreeSet<PathBuf>,
    /// Set when the current directory could not be listed.
    pub error: Option<String>,
}

impl Panel {
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        let mut panel = Panel {
            cwd: cwd.into(),
            entries: Vec::new(),
            cursor: 0,
            selected: BTreeSet::new(),
            error: None,
        };
        panel.reload();
        panel
    }

    /// Re-reads the directory, keeping the cursor and the still-valid selection.
    pub fn reload(&mut self) {
        match bluepane_vfs::list_dir(&self.cwd) {
            Ok(entries) => {
                self.entries = entries;
                self.error = None;
            }
            Err(err) => {
                self.entries.clear();
                self.error = Some(err.to_string());
            }
        }
        self.selected
            .retain(|path| self.entries.iter().any(|e| &e.path == path));
        self.cursor = self.cursor.min(self.entries.len().saturating_sub(1));
    }

    /// The entry under the cursor.
    pub fn current(&self) -> Option<&Entry> {
        self.entries.get(self.cursor)
    }

    /// What an operation acts on: the selection, or the entry under the cursor.
    pub fn targets(&self) -> Vec<PathBuf> {
        if self.selected.is_empty() {
            self.current()
                .map(|e| vec![e.path.clone()])
                .unwrap_or_default()
        } else {
            self.selected.iter().cloned().collect()
        }
    }

    pub fn change_dir(&mut self, dir: PathBuf) {
        self.cwd = dir;
        self.cursor = 0;
        self.selected.clear();
        self.reload();
    }

    pub fn enter(&mut self) {
        if let Some(entry) = self.current().filter(|e| e.is_dir()) {
            let dir = entry.path.clone();
            self.change_dir(dir);
        }
    }

    /// Goes to the parent directory and puts the cursor on the directory just left.
    pub fn up(&mut self) {
        let Some(parent) = self.cwd.parent().map(Path::to_path_buf) else {
            return;
        };
        let left = std::mem::replace(&mut self.cwd, PathBuf::new());
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
    pub fn toggle_select(&mut self) {
        if let Some(path) = self.current().map(|e| e.path.clone()) {
            if !self.selected.remove(&path) {
                self.selected.insert(path);
            }
            self.move_cursor(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (tempfile::TempDir, Panel) {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("dir")).unwrap();
        std::fs::write(tmp.path().join("a.txt"), "a").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "b").unwrap();
        let panel = Panel::new(tmp.path());
        (tmp, panel)
    }

    #[test]
    fn cursor_is_clamped() {
        let (_tmp, mut panel) = sample();
        panel.move_cursor(-5);
        assert_eq!(panel.cursor, 0);
        panel.move_cursor(100);
        assert_eq!(panel.cursor, 2);
    }

    #[test]
    fn enter_and_up_round_trip() {
        let (tmp, mut panel) = sample();
        assert_eq!(panel.current().unwrap().name, "dir");
        panel.enter();
        assert_eq!(panel.cwd, tmp.path().join("dir"));
        panel.up();
        assert_eq!(panel.cwd, tmp.path());
        assert_eq!(panel.current().unwrap().name, "dir");
    }

    #[test]
    fn toggle_selects_and_advances() {
        let (_tmp, mut panel) = sample();
        panel.move_cursor(1);
        panel.toggle_select();
        assert_eq!(panel.selected.len(), 1);
        assert_eq!(panel.cursor, 2);
        panel.move_cursor(-1);
        panel.toggle_select();
        assert!(panel.selected.is_empty());
    }

    #[test]
    fn targets_prefer_selection_over_cursor() {
        let (_tmp, mut panel) = sample();
        assert_eq!(panel.targets().len(), 1);
        panel.move_cursor(1);
        panel.toggle_select();
        panel.toggle_select();
        assert_eq!(panel.targets().len(), 2);
    }
}
