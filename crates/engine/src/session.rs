use std::path::PathBuf;

use bluepane_ops::{self as ops, Progress};

use crate::{Command, EngineError, Keymap, Panel};

/// The whole application state except the UI: two panels, which one is active,
/// the key map and the last status message.
#[derive(Debug)]
pub struct Session {
    pub panels: [Panel; 2],
    pub active: usize,
    pub keymap: Keymap,
    pub status: String,
}

impl Session {
    pub fn new(left: impl Into<PathBuf>, right: impl Into<PathBuf>) -> Self {
        Session {
            panels: [Panel::new(left), Panel::new(right)],
            active: 0,
            keymap: Keymap::default(),
            status: String::new(),
        }
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
        match command {
            Command::SwitchPanel => self.active = 1 - self.active,
            Command::Up => self.active_panel_mut().up(),
            Command::Enter => self.active_panel_mut().enter(),
            Command::Cursor(delta) => self.active_panel_mut().move_cursor(delta),
            Command::ToggleSelect => self.active_panel_mut().toggle_select(),
            Command::Refresh => self.panels.iter_mut().for_each(Panel::reload),
            Command::Select(name) => {
                let panel = self.active_panel_mut();
                let Some(index) = panel.entries.iter().position(|e| e.name == name) else {
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

    /// Copy or move the active panel's targets into the other panel's directory.
    fn transfer(&mut self, remove_source: bool) -> Result<(), EngineError> {
        let source = self.active_panel();
        let dest = self.panels[1 - self.active].cwd.clone();
        if dest == source.cwd {
            return Err(EngineError("oba panele wskazują ten sam katalog".into()));
        }
        let targets = source.targets();
        if targets.is_empty() {
            return Err(EngineError("nic nie zaznaczono".into()));
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
        assert_eq!(s.panels[1].entries.len(), 1);
        assert!(s.panels[0].selected.is_empty());
    }

    #[test]
    fn move_uses_cursor_when_nothing_is_selected() {
        let (tmp, mut s) = two_dirs();
        s.run_line("move").unwrap();
        assert!(!tmp.path().join("left/a.txt").exists());
        assert!(tmp.path().join("right/a.txt").exists());
    }

    #[test]
    fn same_directory_is_rejected() {
        let (tmp, mut s) = two_dirs();
        s.run_line(&format!("cd {}", tmp.path().join("left").display()))
            .unwrap();
        s.run_line("switch").unwrap();
        s.run_line(&format!("cd {}", tmp.path().join("left").display()))
            .unwrap();
        assert!(s.run_line("copy").is_err());
        assert!(!s.status.is_empty());
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
}
