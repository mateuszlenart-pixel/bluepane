use std::path::PathBuf;

use std::path::Path;

use bluepane_ops::{self as ops, Progress};
use bluepane_vfs::Volume;

use crate::{is_parent_entry, Command, EngineError, Keymap, Panel};

/// A question waiting for the user's answer (`confirm` or `cancel`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pending {
    /// Both panels show the same directory: make copies of these paths next to the originals.
    Duplicate(Vec<PathBuf>),
}

/// The whole application state except the UI: two panels, which one is active,
/// the key map and the last status message.
#[derive(Debug)]
pub struct Session {
    pub panels: [Panel; 2],
    pub active: usize,
    pub keymap: Keymap,
    pub status: String,
    /// Mounted volumes, re-read on `refresh`.
    pub volumes: Vec<Volume>,
    /// Set while the user is asked a question; the question text is in `status`.
    pub pending: Option<Pending>,
}

/// Index of the volume that contains `cwd` (the one with the longest matching path).
fn volume_index(volumes: &[Volume], cwd: &Path) -> Option<usize> {
    volumes
        .iter()
        .enumerate()
        .filter(|(_, v)| cwd.starts_with(&v.path))
        .max_by_key(|(_, v)| v.path.components().count())
        .map(|(i, _)| i)
}

impl Session {
    pub fn new(left: impl Into<PathBuf>, right: impl Into<PathBuf>) -> Self {
        Session {
            panels: [Panel::new(left), Panel::new(right)],
            active: 0,
            keymap: Keymap::default(),
            status: String::new(),
            volumes: bluepane_vfs::volumes(),
            pending: None,
        }
    }

    /// Index into `volumes` of the volume the given panel is on.
    pub fn volume_of(&self, panel: usize) -> Option<usize> {
        volume_index(&self.volumes, &self.panels[panel].cwd)
    }

    pub fn active_panel(&self) -> &Panel {
        &self.panels[self.active]
    }

    pub fn active_panel_mut(&mut self) -> &mut Panel {
        &mut self.panels[self.active]
    }

    /// Parses and executes a command line. On error the message is also put in `status`.
    pub fn run_line(&mut self, line: &str) -> Result<(), EngineError> {
        let result = Command::parse(line).and_then(|cmd| self.execute(cmd));
        if let Err(err) = &result {
            self.status = err.0.clone();
        }
        result
    }

    pub fn execute(&mut self, command: Command) -> Result<(), EngineError> {
        // Any command other than the answer drops an unanswered question.
        if !matches!(command, Command::Confirm | Command::Cancel) {
            self.pending = None;
        }
        match command {
            Command::Confirm => self.confirm()?,
            Command::Cancel => {
                if self.pending.take().is_some() {
                    self.status = "Anulowano".into();
                }
            }
            Command::SwitchPanel => self.active = 1 - self.active,
            Command::Up => {
                let panel = self.active_panel_mut();
                if !panel.filter_backspace() {
                    panel.up();
                }
            }
            Command::Enter => self.active_panel_mut().enter(),
            Command::Cursor(delta) => self.active_panel_mut().move_cursor(delta),
            Command::ToggleSelect => self.active_panel_mut().toggle_select(),
            Command::Refresh => {
                self.volumes = bluepane_vfs::volumes();
                self.panels.iter_mut().for_each(Panel::reload);
            }
            Command::Back => self.active_panel_mut().back()?,
            Command::Forward => self.active_panel_mut().forward()?,
            Command::Filter(text) => self.active_panel_mut().set_filter(&text),
            Command::Volume(which) => self.go_to_volume(&which)?,
            Command::Select(name) => {
                let panel = self.active_panel_mut();
                let Some(index) = panel
                    .entries
                    .iter()
                    .position(|e| e.name == name && !is_parent_entry(e))
                else {
                    return Err(EngineError(format!("brak elementu: {name}")));
                };
                panel.cursor = index;
                panel.selected.insert(panel.entries[index].path.clone());
            }
            Command::Cd(path) => {
                let panel = self.active_panel_mut();
                let target = if path.is_absolute() {
                    path
                } else {
                    panel.cwd.join(path)
                };
                if !target.is_dir() {
                    return Err(EngineError(format!("to nie katalog: {}", target.display())));
                }
                panel.change_dir(target);
            }
            Command::Copy => self.transfer(false)?,
            Command::Move => self.transfer(true)?,
            Command::Mkdir(name) => {
                let cwd = self.active_panel().cwd.clone();
                ops::mkdir(&cwd, &name).map_err(|e| EngineError(e.to_string()))?;
                self.panels.iter_mut().for_each(Panel::reload);
                self.status = format!("Utworzono katalog {name}");
            }
            Command::Delete => self.delete()?,
        }
        Ok(())
    }

    /// Jumps the active panel to a volume given as a 1-based number or a name.
    fn go_to_volume(&mut self, which: &str) -> Result<(), EngineError> {
        let found = match which.parse::<usize>() {
            Ok(n) => n.checked_sub(1).and_then(|i| self.volumes.get(i)),
            Err(_) => self
                .volumes
                .iter()
                .find(|v| v.name.eq_ignore_ascii_case(which) || v.path == Path::new(which)),
        };
        let Some(volume) = found else {
            let known: Vec<String> = self
                .volumes
                .iter()
                .enumerate()
                .map(|(i, v)| format!("{} {}", i + 1, v.name))
                .collect();
            return Err(EngineError(format!(
                "nie ma wolumenu: {which} (dostępne: {})",
                known.join(", ")
            )));
        };
        let path = volume.path.clone();
        if !path.is_dir() {
            return Err(EngineError(format!(
                "wolumen niedostępny: {}",
                path.display()
            )));
        }
        self.active_panel_mut().change_dir(path);
        Ok(())
    }

    /// Copy or move the active panel's targets into the other panel's directory.
    fn transfer(&mut self, remove_source: bool) -> Result<(), EngineError> {
        let source = self.active_panel();
        let dest = self.panels[1 - self.active].cwd.clone();
        let targets = source.targets();
        if targets.is_empty() {
            return Err(EngineError("nic nie zaznaczono".into()));
        }
        if dest == source.cwd {
            if remove_source {
                return Err(EngineError("oba panele wskazują ten sam katalog".into()));
            }
            return self.propose_duplicates(targets);
        }
        let progress = Progress::new();
        let mut done = 0;
        let mut first_error = None;
        for target in &targets {
            let result = if remove_source {
                ops::move_to(target, &dest, &progress)
            } else {
                ops::copy(target, &dest, &progress)
            };
            match result {
                Ok(_) => done += 1,
                Err(e) => {
                    first_error.get_or_insert(e.to_string());
                }
            }
        }
        self.active_panel_mut().selected.clear();
        self.panels.iter_mut().for_each(Panel::reload);
        let verb = if remove_source {
            "Przeniesiono"
        } else {
            "Skopiowano"
        };
        match first_error {
            None => {
                self.status = format!("{verb}: {done}");
                Ok(())
            }
            Some(msg) => Err(EngineError(format!(
                "{verb}: {done} z {}, błąd: {msg}",
                targets.len()
            ))),
        }
    }

    /// F5 with the same directory in both panels: ask before making `(kopia)` duplicates.
    fn propose_duplicates(&mut self, targets: Vec<PathBuf>) -> Result<(), EngineError> {
        let first = ops::duplicate_path(&targets[0]).map_err(|e| EngineError(e.to_string()))?;
        let first_name = first
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.status = if targets.len() == 1 {
            format!("Oba panele pokazują ten sam katalog. Utworzyć kopię „{first_name}”? (Enter = tak, Esc = nie)")
        } else {
            format!(
                "Oba panele pokazują ten sam katalog. Utworzyć kopie ({}) z dopiskiem „(kopia)”, np. „{first_name}”? (Enter = tak, Esc = nie)",
                targets.len()
            )
        };
        self.pending = Some(Pending::Duplicate(targets));
        Ok(())
    }

    fn confirm(&mut self) -> Result<(), EngineError> {
        let Some(Pending::Duplicate(targets)) = self.pending.take() else {
            return Err(EngineError("nie ma o co pytać".into()));
        };
        let progress = Progress::new();
        let mut done = 0;
        let mut first_error = None;
        for target in &targets {
            match ops::duplicate(target, &progress) {
                Ok(_) => done += 1,
                Err(e) => {
                    first_error.get_or_insert(e.to_string());
                }
            }
        }
        self.active_panel_mut().selected.clear();
        self.panels.iter_mut().for_each(Panel::reload);
        match first_error {
            None => {
                self.status = format!("Utworzono kopie: {done}");
                Ok(())
            }
            Some(msg) => Err(EngineError(format!(
                "Utworzono kopie: {done} z {}, błąd: {msg}",
                targets.len()
            ))),
        }
    }

    fn delete(&mut self) -> Result<(), EngineError> {
        let targets = self.active_panel().targets();
        if targets.is_empty() {
            return Err(EngineError("nic nie zaznaczono".into()));
        }
        let mut done = 0;
        let mut first_error = None;
        for target in &targets {
            match ops::delete_to_trash(target) {
                Ok(()) => done += 1,
                Err(e) => {
                    first_error.get_or_insert(e.to_string());
                }
            }
        }
        self.active_panel_mut().selected.clear();
        self.panels.iter_mut().for_each(Panel::reload);
        match first_error {
            None => {
                self.status = format!("Do kosza: {done}");
                Ok(())
            }
            Some(msg) => Err(EngineError(format!(
                "Do kosza: {done} z {}, błąd: {msg}",
                targets.len()
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_dirs() -> (tempfile::TempDir, Session) {
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left");
        let right = tmp.path().join("right");
        std::fs::create_dir(&left).unwrap();
        std::fs::create_dir(&right).unwrap();
        std::fs::write(left.join("a.txt"), "a").unwrap();
        std::fs::write(left.join("b.txt"), "b").unwrap();
        let session = Session::new(left, right);
        (tmp, session)
    }

    #[test]
    fn copy_goes_to_the_other_panel() {
        let (tmp, mut s) = two_dirs();
        s.run_line("select a.txt").unwrap();
        s.run_line("copy").unwrap();
        assert!(tmp.path().join("right/a.txt").exists());
        assert!(tmp.path().join("left/a.txt").exists());
        assert_eq!(s.panels[1].entries.len(), 2); // ".." and a.txt
        assert!(s.panels[0].selected.is_empty());
    }

    #[test]
    fn move_uses_cursor_when_nothing_is_selected() {
        let (tmp, mut s) = two_dirs();
        s.run_line("cursor 1").unwrap();
        s.run_line("move").unwrap();
        assert!(!tmp.path().join("left/a.txt").exists());
        assert!(tmp.path().join("right/a.txt").exists());
    }

    /// Both panels on `left`, cursor on a.txt.
    fn same_dir() -> (tempfile::TempDir, Session) {
        let (tmp, mut s) = two_dirs();
        s.run_line("switch").unwrap();
        s.run_line(&format!("cd {}", tmp.path().join("left").display()))
            .unwrap();
        s.run_line("switch").unwrap();
        s.run_line("cursor 1").unwrap();
        (tmp, s)
    }

    #[test]
    fn move_in_the_same_directory_is_rejected() {
        let (_tmp, mut s) = same_dir();
        assert!(s.run_line("move").is_err());
        assert!(s.status.contains("ten sam katalog"));
        assert!(s.pending.is_none());
    }

    #[test]
    fn copy_in_the_same_directory_asks_before_duplicating() {
        let (tmp, mut s) = same_dir();
        s.run_line("copy").unwrap();
        assert!(s.pending.is_some());
        assert!(s.status.contains("a (kopia).txt"), "{}", s.status);
        // Nothing is created until the user answers.
        assert!(!tmp.path().join("left/a (kopia).txt").exists());

        s.run_line("confirm").unwrap();
        assert!(s.pending.is_none());
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("left/a (kopia).txt")).unwrap(),
            "a"
        );
        assert!(tmp.path().join("left/a.txt").exists());
        // Both panels were reloaded.
        assert!(s.panels[0]
            .entries
            .iter()
            .any(|e| e.name == "a (kopia).txt"));
        assert!(s.panels[1]
            .entries
            .iter()
            .any(|e| e.name == "a (kopia).txt"));
    }

    #[test]
    fn declining_creates_nothing() {
        let (tmp, mut s) = same_dir();
        s.run_line("copy").unwrap();
        s.run_line("cancel").unwrap();
        assert!(s.pending.is_none());
        assert_eq!(s.status, "Anulowano");
        assert!(!tmp.path().join("left/a (kopia).txt").exists());
        assert!(s.run_line("confirm").is_err());
    }

    #[test]
    fn any_other_command_drops_the_question() {
        let (tmp, mut s) = same_dir();
        s.run_line("copy").unwrap();
        s.run_line("cursor 1").unwrap();
        assert!(s.pending.is_none());
        assert!(s.run_line("confirm").is_err());
        assert!(!tmp.path().join("left/a (kopia).txt").exists());
    }

    #[test]
    fn duplicates_every_selected_entry_and_never_overwrites() {
        let (tmp, mut s) = same_dir();
        std::fs::write(tmp.path().join("left/a (kopia).txt"), "old").unwrap();
        s.run_line("cursor -1").unwrap();
        s.run_line("select a.txt").unwrap();
        s.run_line("select b.txt").unwrap();
        s.run_line("copy").unwrap();
        s.run_line("confirm").unwrap();
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("left/a (kopia).txt")).unwrap(),
            "old"
        );
        assert!(tmp.path().join("left/a (kopia 2).txt").exists());
        assert!(tmp.path().join("left/b (kopia).txt").exists());
    }

    #[test]
    fn copy_with_nothing_to_copy_does_not_ask() {
        let (_tmp, mut s) = same_dir();
        s.run_line("cursor -1").unwrap(); // on ".."
        assert!(s.run_line("copy").is_err());
        assert!(s.pending.is_none());
    }

    #[test]
    fn mkdir_and_cd_work() {
        let (tmp, mut s) = two_dirs();
        s.run_line("mkdir new").unwrap();
        assert!(tmp.path().join("left/new").is_dir());
        s.run_line("cd new").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left/new"));
        assert!(s.run_line("cd nope").is_err());
    }

    #[test]
    fn errors_land_in_status() {
        let (_tmp, mut s) = two_dirs();
        assert!(s.run_line("frobnicate").is_err());
        assert!(s.status.contains("nieznana"));
    }

    #[test]
    fn parent_row_is_not_an_operation_target() {
        let (tmp, mut s) = two_dirs();
        // The cursor starts on "..": nothing to copy, and nothing must happen to the parent.
        assert!(s.run_line("copy").is_err());
        assert!(s.run_line("delete").is_err());
        assert!(s.run_line("select ..").is_err());
        assert!(tmp.path().join("left/a.txt").exists());
    }

    #[test]
    fn back_and_forward_follow_the_history() {
        let (tmp, mut s) = two_dirs();
        s.run_line("mkdir sub").unwrap();
        s.run_line("cd sub").unwrap();
        s.run_line("back").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left"));
        s.run_line("forward").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left/sub"));
        assert!(s.run_line("forward").is_err());
        assert!(s.status.contains("historii"));
        // The other panel has its own history.
        s.run_line("switch").unwrap();
        assert!(s.run_line("back").is_err());
    }

    #[test]
    fn backspace_edits_the_filter_before_going_up() {
        let (tmp, mut s) = two_dirs();
        s.run_line("filter b").unwrap();
        assert_eq!(s.active_panel().entries.len(), 2); // ".." and b.txt
        s.run_line("up").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left"));
        assert!(s.active_panel().filter.is_empty());
        s.run_line("up").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path());
    }

    #[test]
    fn operations_ignore_entries_hidden_by_the_filter() {
        let (tmp, mut s) = two_dirs();
        s.run_line("select a.txt").unwrap();
        s.run_line("filter b").unwrap();
        s.run_line("copy").unwrap(); // acts on b.txt under the cursor, not on hidden a.txt
        assert!(tmp.path().join("right/b.txt").exists());
        assert!(!tmp.path().join("right/a.txt").exists());
    }

    #[test]
    fn volume_jump_by_number_and_name() {
        let (tmp, mut s) = two_dirs();
        s.volumes = vec![
            Volume {
                name: "one".into(),
                path: tmp.path().join("left"),
            },
            Volume {
                name: "Two".into(),
                path: tmp.path().join("right"),
            },
        ];
        s.run_line("volume 2").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("right"));
        s.run_line("volume one").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left"));
        s.run_line("volume TWO").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("right"));
        s.run_line("back").unwrap();
        assert_eq!(s.active_panel().cwd, tmp.path().join("left"));

        assert!(s.run_line("volume 0").is_err());
        assert!(s.run_line("volume 3").is_err());
        let err = s.run_line("volume nope").unwrap_err();
        assert!(err.0.contains("dostępne: 1 one, 2 Two"), "{err}");
    }

    #[test]
    fn volume_of_picks_the_longest_prefix() {
        let volumes = vec![
            Volume {
                name: "root".into(),
                path: PathBuf::from("/a"),
            },
            Volume {
                name: "nested".into(),
                path: PathBuf::from("/a/b"),
            },
        ];
        assert_eq!(volume_index(&volumes, Path::new("/a/b/c")), Some(1));
        assert_eq!(volume_index(&volumes, Path::new("/a/x")), Some(0));
        assert_eq!(volume_index(&volumes, Path::new("/z")), None);
    }
}
