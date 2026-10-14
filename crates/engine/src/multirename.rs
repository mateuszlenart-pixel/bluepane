//! The multi-rename window's state and commands (`multirename`, `mrset`, `mrname`,
//! `mrrun`, `mrundo`, `mrclose`). The name building and the safe apply live in
//! `bluepane-rename`; this module connects them to the panels.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use bluepane_rename::{CaseMode, Item, Plan, Renamed, Settings, Stamp};
use chrono::{DateTime, Datelike, Local, Timelike};

use crate::panel::is_parent_entry;
use crate::session::Session;
use crate::EngineError;

/// One file in the window, with what the list shows besides the names.
#[derive(Debug, Clone)]
pub struct RenameEntry {
    pub item: Item,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

/// The multi-rename window: the files, the settings and the names they would get.
#[derive(Debug, Clone)]
pub struct RenameView {
    pub dir: PathBuf,
    /// Same order and length as `plan.rows`.
    pub entries: Vec<RenameEntry>,
    pub settings: Settings,
    /// Names typed by hand for single rows, keyed by the current path.
    pub overrides: HashMap<PathBuf, String>,
    pub plan: Plan,
    /// Why the settings cannot be used (a broken mask or regular expression).
    pub error: Option<String>,
    /// What the last run did, so `mrundo` can take it back.
    pub undo: Vec<Renamed>,
    /// A line for the window about the last run or undo.
    pub summary: Option<String>,
}

impl RenameView {
    fn replan(&mut self) {
        let items: Vec<Item> = self.entries.iter().map(|e| e.item.clone()).collect();
        match bluepane_rename::plan(&items, &self.settings, &self.overrides) {
            Ok(plan) => {
                self.plan = plan;
                self.error = None;
            }
            Err(text) => self.error = Some(text),
        }
    }

    /// Points the rows at the new paths after a run or an undo.
    fn move_entries(&mut self, moves: &[(PathBuf, PathBuf)]) {
        for entry in &mut self.entries {
            if let Some((_, to)) = moves.iter().find(|(from, _)| *from == entry.item.path) {
                entry.item.path = to.clone();
            }
        }
        self.overrides.clear();
    }
}

fn stamp_of(time: SystemTime) -> Stamp {
    let local: DateTime<Local> = time.into();
    Stamp {
        year: local.year(),
        month: local.month(),
        day: local.day(),
        hour: local.hour(),
        minute: local.minute(),
        second: local.second(),
    }
}

fn parse_bool(field: &str, value: &str) -> Result<bool, EngineError> {
    match value.to_lowercase().as_str() {
        "" | "on" | "1" | "true" | "yes" | "tak" => Ok(true),
        "off" | "0" | "false" | "no" | "nie" => Ok(false),
        _ => Err(EngineError(format!(
            "mrset {field}: oczekiwano on albo off"
        ))),
    }
}

fn parse_number<T: std::str::FromStr>(field: &str, value: &str) -> Result<T, EngineError> {
    value
        .parse()
        .map_err(|_| EngineError(format!("mrset {field}: oczekiwano liczby")))
}

impl Session {
    /// `multirename`: open the window for the selection (or the entry under the cursor).
    pub(crate) fn start_multirename(&mut self) -> Result<(), EngineError> {
        self.deny_remote("zmiana nazw wielu plików")?;
        if self.active_panel().archive.is_some() {
            return Err(EngineError(
                "zmiana nazw działa na katalogach, nie wewnątrz archiwum (użyj F2)".into(),
            ));
        }
        let panel = self.active_panel();
        let entries: Vec<RenameEntry> = panel
            .target_entries()
            .into_iter()
            .filter(|e| !is_parent_entry(e))
            .map(|e| RenameEntry {
                item: Item {
                    path: e.path.clone(),
                    is_dir: e.is_dir(),
                    modified: e.modified.map(stamp_of),
                },
                size: e.size,
                modified: e.modified,
            })
            .collect();
        if entries.is_empty() {
            return Err(EngineError("nic nie zaznaczono".into()));
        }
        let mut view = RenameView {
            dir: panel.cwd.clone(),
            entries,
            settings: Settings::default(),
            overrides: HashMap::new(),
            plan: Plan::default(),
            error: None,
            undo: Vec::new(),
            summary: None,
        };
        view.replan();
        self.status = format!(
            "Zmiana nazw: {} plików (Start = wykonaj)",
            view.entries.len()
        );
        self.rename = Some(view);
        Ok(())
    }

    fn rename_view(&mut self) -> Result<&mut RenameView, EngineError> {
        self.rename
            .as_mut()
            .ok_or_else(|| EngineError("najpierw otwórz zmianę nazw (multirename)".into()))
    }

    /// `mrset <field> <value>`
    pub(crate) fn multirename_set(&mut self, field: &str, value: &str) -> Result<(), EngineError> {
        let view = self.rename_view()?;
        let s = &mut view.settings;
        match field {
            "name" => s.name_mask = value.to_string(),
            "ext" => s.ext_mask = value.to_string(),
            "find" => s.find = value.to_string(),
            "replace" => s.replace = value.to_string(),
            "regex" => s.regex = parse_bool(field, value)?,
            "once" => s.once = parse_bool(field, value)?,
            "inclext" => s.include_ext = parse_bool(field, value)?,
            "ignorecase" => s.ignore_case = parse_bool(field, value)?,
            "case" => {
                s.case = match value.to_lowercase().as_str() {
                    "" | "unchanged" | "none" => CaseMode::Unchanged,
                    "upper" => CaseMode::Upper,
                    "lower" => CaseMode::Lower,
                    "words" => CaseMode::Words,
                    "sentence" => CaseMode::Sentence,
                    _ => {
                        return Err(EngineError(
                            "mrset case: unchanged, upper, lower, words albo sentence".into(),
                        ))
                    }
                }
            }
            "start" => s.counter_start = parse_number(field, value)?,
            "step" => s.counter_step = parse_number(field, value)?,
            "digits" => s.counter_digits = parse_number(field, value)?,
            "reset" => {
                *s = Settings::default();
                view.overrides.clear();
            }
            _ => return Err(EngineError(format!("mrset: nieznane pole {field}"))),
        }
        view.replan();
        view.summary = None;
        if let Some(error) = view.error.clone() {
            return Err(EngineError(error));
        }
        Ok(())
    }

    /// `mrname <n> [text]`
    pub(crate) fn multirename_name(
        &mut self,
        n: usize,
        name: Option<String>,
    ) -> Result<(), EngineError> {
        let view = self.rename_view()?;
        let path = view
            .entries
            .get(n - 1)
            .map(|e| e.item.path.clone())
            .ok_or_else(|| EngineError(format!("mrname: nie ma wiersza {n}")))?;
        match name {
            Some(name) => view.overrides.insert(path, name),
            None => view.overrides.remove(&path),
        };
        view.replan();
        view.summary = None;
        Ok(())
    }

    /// `mrrun`: carry out what the window shows.
    pub(crate) fn multirename_run(&mut self) -> Result<(), EngineError> {
        let view = self.rename_view()?;
        if let Some(error) = &view.error {
            return Err(EngineError(error.clone()));
        }
        let conflicts = view.plan.conflicts();
        if conflicts > 0 {
            return Err(EngineError(format!(
                "nie można zmienić nazw: konflikty ({conflicts}), popraw je lub zmień ustawienia"
            )));
        }
        if view.plan.changes() == 0 {
            return Err(EngineError("nie ma nic do zmiany".into()));
        }
        let done = bluepane_rename::apply(&view.plan).map_err(EngineError)?;
        let moves: Vec<(PathBuf, PathBuf)> = done
            .iter()
            .map(|r| (r.from.clone(), r.to.clone()))
            .collect();
        view.move_entries(&moves);
        // The names are final now: start the next round from plain settings.
        view.settings = Settings::default();
        view.summary = Some(format!("Zmieniono nazw: {}", done.len()));
        view.undo = done;
        view.replan();
        let text = view.summary.clone().unwrap_or_default();
        self.reload_panels();
        self.status = text;
        Ok(())
    }

    /// `mrundo`: take back the last run.
    pub(crate) fn multirename_undo(&mut self) -> Result<(), EngineError> {
        let view = self.rename_view()?;
        if view.undo.is_empty() {
            return Err(EngineError("nie ma czego cofnąć".into()));
        }
        bluepane_rename::undo(&view.undo).map_err(EngineError)?;
        let moves: Vec<(PathBuf, PathBuf)> = view
            .undo
            .iter()
            .map(|r| (r.to.clone(), r.from.clone()))
            .collect();
        let count = view.undo.len();
        view.move_entries(&moves);
        view.undo.clear();
        view.summary = Some(format!("Cofnięto zmianę nazw: {count}"));
        view.replan();
        let text = view.summary.clone().unwrap_or_default();
        self.reload_panels();
        self.status = text;
        Ok(())
    }

    fn reload_panels(&mut self) {
        self.panels.iter_mut().for_each(crate::Panel::reload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn session() -> (tempfile::TempDir, Session) {
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left");
        let right = tmp.path().join("right");
        fs::create_dir(&left).unwrap();
        fs::create_dir(&right).unwrap();
        for (name, text) in [("a.txt", "a"), ("b.txt", "b"), ("c.md", "c")] {
            fs::write(left.join(name), text).unwrap();
        }
        (tmp, Session::new(left, right))
    }

    fn names(dir: PathBuf) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn new_names(s: &Session) -> Vec<String> {
        s.rename
            .as_ref()
            .unwrap()
            .plan
            .rows
            .iter()
            .map(|r| r.new_name.clone())
            .collect()
    }

    #[test]
    fn needs_a_selection_or_a_cursor() {
        let (_tmp, mut s) = session();
        assert!(s.run_line("multirename").is_err(), "cursor is on ..");
        assert!(s.rename.is_none());
        assert!(s.run_line("mrset name x").is_err(), "window is not open");
    }

    #[test]
    fn previews_without_touching_files() {
        let (tmp, mut s) = session();
        s.run_line("selectall").unwrap();
        s.run_line("multirename").unwrap();
        s.run_line("mrset name photo_[C]").unwrap();
        s.run_line("mrset digits 3").unwrap();
        assert_eq!(
            new_names(&s),
            ["photo_001.txt", "photo_002.txt", "photo_003.md"]
        );
        assert_eq!(names(tmp.path().join("left")), ["a.txt", "b.txt", "c.md"]);
    }

    #[test]
    fn run_renames_refreshes_the_panel_and_can_be_undone() {
        let (tmp, mut s) = session();
        s.run_line("selectall").unwrap();
        s.run_line("multirename").unwrap();
        s.run_line("mrset name new_[N]").unwrap();
        s.run_line("mrset ext [E]").unwrap();
        s.run_line("mrrun").unwrap();
        let left = tmp.path().join("left");
        assert_eq!(names(left.clone()), ["new_a.txt", "new_b.txt", "new_c.md"]);
        assert!(s.panels[0].entries.iter().any(|e| e.name == "new_a.txt"));
        let view = s.rename.as_ref().unwrap();
        assert_eq!(view.summary.as_deref(), Some("Zmieniono nazw: 3"));
        assert_eq!(view.plan.changes(), 0, "settings were reset after the run");
        assert!(s.run_line("mrrun").is_err(), "nothing left to do");

        s.run_line("mrundo").unwrap();
        assert_eq!(names(left), ["a.txt", "b.txt", "c.md"]);
        assert!(s.run_line("mrundo").is_err());
    }

    #[test]
    fn search_replace_case_and_extension_settings() {
        let (tmp, mut s) = session();
        s.run_line("selectall").unwrap();
        s.run_line("multirename").unwrap();
        s.run_line("mrset find txt").unwrap();
        s.run_line("mrset replace md").unwrap();
        s.run_line("mrset inclext on").unwrap();
        s.run_line("mrset case upper").unwrap();
        s.run_line("mrrun").unwrap();
        // `a.txt` and `b.txt` become `A.MD`/`B.MD`; `c.md` becomes `C.MD`.
        assert_eq!(names(tmp.path().join("left")), ["A.MD", "B.MD", "C.MD"]);
    }

    #[test]
    fn conflicts_block_the_run() {
        let (tmp, mut s) = session();
        s.run_line("selectall").unwrap();
        s.run_line("multirename").unwrap();
        s.run_line("mrset name same").unwrap();
        s.run_line("mrset ext txt").unwrap();
        let err = s.run_line("mrrun").unwrap_err();
        assert!(err.0.contains("konflikt"), "{err}");
        assert_eq!(names(tmp.path().join("left")), ["a.txt", "b.txt", "c.md"]);
    }

    #[test]
    fn broken_settings_are_reported_and_block_the_run() {
        let (_tmp, mut s) = session();
        s.run_line("selectall").unwrap();
        s.run_line("multirename").unwrap();
        assert!(s.run_line("mrset name [N").is_err());
        assert!(s.rename.as_ref().unwrap().error.is_some());
        assert!(s.run_line("mrrun").is_err());
        s.run_line("mrset name [N]").unwrap();
        assert!(s.rename.as_ref().unwrap().error.is_none());
        assert!(
            s.run_line("mrset find (").is_ok(),
            "plain text, not a regex"
        );
        assert!(s.run_line("mrset regex on").is_err());
        assert!(s.run_line("mrset bogus 1").is_err());
        assert!(s.run_line("mrset digits x").is_err());
    }

    #[test]
    fn single_names_can_be_typed_by_hand() {
        let (tmp, mut s) = session();
        s.run_line("selectall").unwrap();
        s.run_line("multirename").unwrap();
        s.run_line("mrname 2 hand_made.txt").unwrap();
        assert_eq!(new_names(&s), ["a.txt", "hand_made.txt", "c.md"]);
        s.run_line("mrrun").unwrap();
        assert_eq!(
            names(tmp.path().join("left")),
            ["a.txt", "c.md", "hand_made.txt"]
        );
        assert!(s.run_line("mrname 9 x").is_err());
        assert!(s.run_line("mrname").is_err());
    }

    #[test]
    fn quoted_values_keep_their_spaces() {
        let (_tmp, mut s) = session();
        s.run_line("selectall").unwrap();
        s.run_line("multirename").unwrap();
        s.run_line("mrset find .").unwrap();
        s.run_line("mrset replace \" \"").unwrap();
        s.run_line("mrset inclext on").unwrap();
        assert_eq!(new_names(&s), ["a txt", "b txt", "c md"]);
    }

    #[test]
    fn the_cursor_entry_is_used_without_a_selection() {
        let (tmp, mut s) = session();
        s.run_line("select b.txt").unwrap();
        s.run_line("unselectall").unwrap();
        s.run_line("multirename").unwrap();
        s.run_line("mrset name only").unwrap();
        s.run_line("mrrun").unwrap();
        assert_eq!(
            names(tmp.path().join("left")),
            ["a.txt", "c.md", "only.txt"]
        );
    }

    #[test]
    fn close_drops_the_window_and_the_key_is_bound() {
        let (_tmp, mut s) = session();
        s.run_line("selectall").unwrap();
        s.run_line("multirename").unwrap();
        s.run_line("mrclose").unwrap();
        assert!(s.rename.is_none());
        assert_eq!(s.keymap.lookup("Ctrl+F2"), Some("multirename"));
    }
}
