//! Drag and drop between the panels and from the system's file manager. Both end as a copy
//! or a move job: `dragto` takes what the active panel acts on to the other panel (or to a
//! folder row of a panel), `drop` copies files that came from outside into a panel.

use std::path::PathBuf;

use crate::job::{JobKind, JobSpec};
use crate::panel::is_parent_entry;
use crate::session::Session;
use crate::EngineError;

/// Where a drop lands: a panel (0 = left, 1 = right) and, optionally, a folder row of it
/// (counted from 1, like `click`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropTarget {
    pub panel: usize,
    pub row: Option<usize>,
}

/// Splits `"a b" c "d"` into words, keeping the spaces inside quotes.
pub(crate) fn split_quoted(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut started = false;
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    words.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        words.push(current);
    }
    words
}

impl Session {
    /// The folder a drop lands in. Only folders on a drive can receive a drop this way.
    fn drop_folder(&self, target: DropTarget) -> Result<PathBuf, EngineError> {
        let panel = &self.panels[target.panel];
        if panel.remote.is_some() || panel.archive.is_some() || panel.virtual_list.is_some() {
            return Err(EngineError(
                "upuszczanie działa tylko w katalogach na dysku (serwery i archiwa: F5 / F6)"
                    .into(),
            ));
        }
        let Some(row) = target.row else {
            return Ok(panel.cwd.clone());
        };
        let entry = row
            .checked_sub(1)
            .and_then(|i| panel.entries.get(i))
            .filter(|e| e.is_dir() && !is_parent_entry(e))
            .ok_or_else(|| EngineError("to nie jest katalog, do którego można upuścić".into()))?;
        Ok(entry.path.clone())
    }

    /// `dragto copy|move <panel> [row]`.
    pub(crate) fn drag_to(
        &mut self,
        remove_source: bool,
        target: DropTarget,
    ) -> Result<(), EngineError> {
        let from = self.active;
        if target.row.is_none() {
            // The other panel as a whole: exactly what F5 / F6 do (servers and archives too).
            if target.panel == from {
                return Ok(());
            }
            return self.transfer(remove_source);
        }
        let dest = self.drop_folder(target)?;
        let source = &self.panels[from];
        if source.remote.is_some() || source.archive.is_some() || source.virtual_list.is_some() {
            return Err(EngineError(
                "na katalog można upuścić tylko pliki z dysku (serwery i archiwa: F5 / F6)".into(),
            ));
        }
        let sources = source.targets();
        if sources.is_empty() {
            return Err(EngineError("nic nie zaznaczono".into()));
        }
        if sources.iter().any(|s| dest.starts_with(s)) {
            return Err(EngineError(
                "nie można upuścić katalogu na siebie ani na jego podkatalog".into(),
            ));
        }
        if sources.iter().all(|s| s.parent() == Some(dest.as_path())) {
            return Err(EngineError("te pliki już są w tym katalogu".into()));
        }
        self.enqueue(JobSpec {
            kind: if remove_source {
                JobKind::Move
            } else {
                JobKind::Copy
            },
            sources,
            dest: Some(dest),
            archive: None,
        });
        Ok(())
    }

    /// `drop <panel> [row] "path"...`: copies files from outside into a panel's folder.
    pub(crate) fn drop_files(
        &mut self,
        target: DropTarget,
        paths: Vec<PathBuf>,
    ) -> Result<(), EngineError> {
        let dest = self.drop_folder(target)?;
        let sources: Vec<PathBuf> = paths
            .into_iter()
            .filter(|p| std::fs::symlink_metadata(p).is_ok())
            .filter(|p| p.parent() != Some(dest.as_path()))
            .filter(|p| !dest.starts_with(p))
            .collect();
        if sources.is_empty() {
            return Err(EngineError(
                "nie ma czego kopiować (pliki już są w tym katalogu albo nie istnieją)".into(),
            ));
        }
        self.enqueue(JobSpec {
            kind: JobKind::Copy,
            sources,
            dest: Some(dest),
            archive: None,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(tmp: &std::path::Path) -> Session {
        std::fs::create_dir_all(tmp.join("left/katalog")).unwrap();
        std::fs::create_dir_all(tmp.join("right/cel")).unwrap();
        std::fs::create_dir_all(tmp.join("outside")).unwrap();
        std::fs::write(tmp.join("left/a.txt"), "aaa").unwrap();
        std::fs::write(tmp.join("left/b.txt"), "bbb").unwrap();
        std::fs::write(tmp.join("outside/z.txt"), "zzz").unwrap();
        let mut s = Session::new(tmp.join("left"), tmp.join("right"));
        s.use_config_dir(&tmp.join("config"));
        s
    }

    fn row_of(s: &Session, panel: usize, name: &str) -> usize {
        s.panels[panel]
            .entries
            .iter()
            .position(|e| e.name == name)
            .unwrap()
            + 1
    }

    #[test]
    fn quoted_words_keep_their_spaces() {
        assert_eq!(split_quoted(r#"a "b c" d"#), ["a", "b c", "d"]);
        assert_eq!(split_quoted(r#""" x"#), ["", "x"]);
        assert!(split_quoted("   ").is_empty());
    }

    #[test]
    fn dragging_to_the_other_panel_copies_or_moves_like_f5_and_f6() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("select a.txt").unwrap();
        s.run_line("dragto copy 2").unwrap();
        s.wait_idle();
        assert!(tmp.path().join("right/a.txt").exists() && tmp.path().join("left/a.txt").exists());
        s.run_line("unselectall").unwrap();
        s.run_line("select b.txt").unwrap();
        s.run_line("dragto move 2").unwrap();
        s.wait_idle();
        assert!(tmp.path().join("right/b.txt").exists());
        assert!(!tmp.path().join("left/b.txt").exists());
        // Dropping on the panel it came from does nothing.
        s.run_line("dragto copy 1").unwrap();
    }

    #[test]
    fn dropping_on_a_folder_row_goes_into_that_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("select a.txt").unwrap();
        let row = row_of(&s, 1, "cel");
        s.run_line(&format!("dragto copy 2 {row}")).unwrap();
        s.wait_idle();
        assert!(tmp.path().join("right/cel/a.txt").exists());
        // Inside the same panel: the file moves into the sub-folder.
        s.run_line("unselectall").unwrap();
        s.run_line("select b.txt").unwrap();
        let row = row_of(&s, 0, "katalog");
        s.run_line(&format!("dragto move 1 {row}")).unwrap();
        s.wait_idle();
        assert!(tmp.path().join("left/katalog/b.txt").exists());
        assert!(!tmp.path().join("left/b.txt").exists());
    }

    #[test]
    fn nonsense_drops_are_refused_without_touching_anything() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("select katalog").unwrap();
        let row = row_of(&s, 0, "katalog");
        // A folder onto itself.
        assert!(s.run_line(&format!("dragto move 1 {row}")).is_err());
        // A file row is not a folder to drop into.
        let file_row = row_of(&s, 0, "a.txt");
        assert!(s.run_line(&format!("dragto copy 1 {file_row}")).is_err());
        // Nothing marked and the cursor on `..`.
        s.run_line("unselectall").unwrap();
        s.run_line("cursor -100").unwrap();
        assert!(s.run_line("dragto copy 2").is_err());
        // Files that are already in the folder.
        s.run_line("select a.txt").unwrap();
        let parent_row = row_of(&s, 0, "..");
        assert!(s.run_line(&format!("dragto copy 1 {parent_row}")).is_err());
        assert!(tmp.path().join("left/a.txt").exists());
    }

    #[test]
    fn files_from_the_system_are_copied_into_the_chosen_panel_or_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        let z = tmp.path().join("outside/z.txt");
        s.run_line(&format!("drop 2 \"{}\"", z.display())).unwrap();
        s.wait_idle();
        assert!(tmp.path().join("right/z.txt").exists() && z.exists());
        let row = row_of(&s, 1, "cel");
        s.run_line(&format!("drop 2 {row} \"{}\"", z.display()))
            .unwrap();
        s.wait_idle();
        assert!(tmp.path().join("right/cel/z.txt").exists());
        // Already there, missing, or nothing at all.
        assert!(s
            .run_line(&format!(
                "drop 2 \"{}\"",
                tmp.path().join("right/z.txt").display()
            ))
            .is_err());
        assert!(s.run_line("drop 1 \"/nie/ma/takiego\"").is_err());
        assert!(s.run_line("drop 1").is_err());
    }
}
