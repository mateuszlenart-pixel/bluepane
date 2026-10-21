//! Watching the folders shown in the panels: when something else changes them (a download,
//! a build, another program) the list refreshes by itself. Only the folders of the two active
//! panels are watched, not their subfolders, and not archives, servers or result lists.
//! `watch off` turns it off (kept in `settings.conf`); a burst of changes is read once.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::panel::Panel;
use crate::session::Session;
use crate::EngineError;

/// How long changes are collected before the lists are read again.
pub const WATCH_DEBOUNCE: Duration = Duration::from_millis(250);

/// What `watch` asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchAction {
    Show,
    On,
    Off,
}

/// A panel whose folder is an ordinary one on disk.
fn watched_folder(panel: &Panel) -> Option<PathBuf> {
    (panel.remote.is_none() && panel.archive.is_none() && panel.virtual_list.is_none())
        .then(|| panel.cwd.clone())
}

impl Session {
    /// Whether the panels refresh themselves (default: yes).
    pub fn watching(&self) -> bool {
        self.settings.watch.unwrap_or(true)
    }

    /// Called every frame: keeps the watched folders in step with the panels and reloads the
    /// panels whose folder changed on disk. Cheap when nothing happens.
    pub(crate) fn poll_watch(&mut self) {
        if !self.watching() {
            return;
        }
        let wanted: Vec<PathBuf> = self.panels.iter().filter_map(watched_folder).collect();
        self.watcher.set_paths(&wanted);
        let changed = self.watcher.take_changed();
        if !changed.is_empty() {
            self.watch_changed.extend(changed);
            self.watch_since.get_or_insert_with(Instant::now);
        }
        let ready = self
            .watch_since
            .is_some_and(|since| since.elapsed() >= WATCH_DEBOUNCE);
        if !ready {
            return;
        }
        self.watch_since = None;
        let changed = std::mem::take(&mut self.watch_changed);
        let mut reloaded = Vec::new();
        for panel in &mut self.panels {
            if watched_folder(panel).is_some_and(|dir| changed.contains(&dir)) {
                panel.reload();
                reloaded.push(panel.cwd.clone());
            }
        }
        // The git marks and branch follow the files (the old ones stay until git answers).
        if self.columns().contains(&crate::settings::Column::Git) {
            for dir in reloaded {
                self.git_status.start(dir);
            }
        }
    }

    pub(crate) fn watch_action(&mut self, action: WatchAction) -> Result<(), EngineError> {
        match action {
            WatchAction::Show => {
                self.status = match (self.watching(), self.watcher.error()) {
                    (true, Some(why)) => {
                        format!("Samoodświeżanie włączone, ale system go nie daje: {why}")
                    }
                    (true, None) => {
                        "Listy odświeżają się same po zmianach na dysku (watch off wyłącza)".into()
                    }
                    (false, _) => "Listy nie odświeżają się same (watch on włącza)".into(),
                };
            }
            WatchAction::On | WatchAction::Off => {
                let on = action == WatchAction::On;
                self.settings.watch = Some(on);
                if !on {
                    self.watcher.set_paths(&[]);
                    self.watch_changed.clear();
                    self.watch_since = None;
                }
                self.status = if on {
                    "Listy będą się odświeżać same po zmianach na dysku".into()
                } else {
                    "Listy nie odświeżają się już same (F9, komenda refresh, odświeża ręcznie)"
                        .into()
                };
                self.save_settings()?;
            }
        }
        Ok(())
    }
}

/// `watch [on|off]`
pub(crate) fn parse_watch(rest: &str) -> Result<WatchAction, EngineError> {
    match rest.trim().to_lowercase().as_str() {
        "" => Ok(WatchAction::Show),
        "on" => Ok(WatchAction::On),
        "off" => Ok(WatchAction::Off),
        _ => Err(EngineError("watch: użycie: watch [on|off]".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> (tempfile::TempDir, Session) {
        let tmp = tempfile::tempdir().unwrap();
        let (left, right) = (tmp.path().join("left"), tmp.path().join("right"));
        std::fs::create_dir_all(&left).unwrap();
        std::fs::create_dir_all(&right).unwrap();
        let mut s = Session::new(&left, &right);
        s.use_config_dir(&tmp.path().join("config"));
        (tmp, s)
    }

    /// Polls like the window does until `done` holds or time runs out.
    fn poll_until(s: &mut Session, done: impl Fn(&Session) -> bool) -> bool {
        let end = Instant::now() + Duration::from_secs(10);
        while Instant::now() < end {
            s.poll();
            if done(s) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    fn names(s: &Session, side: usize) -> Vec<String> {
        s.panels[side]
            .entries
            .iter()
            .filter(|e| e.name != "..")
            .map(|e| e.name.clone())
            .collect()
    }

    #[test]
    fn a_file_made_behind_the_programs_back_shows_up_and_a_removed_one_goes() {
        let (tmp, mut s) = session();
        s.poll(); // starts watching both folders
        std::fs::write(tmp.path().join("left/zażółć gęślą.txt"), "treść").unwrap();
        assert!(
            poll_until(&mut s, |s| names(s, 0).contains(&"zażółć gęślą.txt".into())),
            "{:?}",
            names(&s, 0)
        );
        assert!(names(&s, 1).is_empty(), "the other panel is untouched");
        std::fs::write(tmp.path().join("right/b.txt"), "b").unwrap();
        assert!(poll_until(&mut s, |s| names(s, 1) == ["b.txt"]));
        std::fs::remove_file(tmp.path().join("left/zażółć gęślą.txt")).unwrap();
        assert!(poll_until(&mut s, |s| names(s, 0).is_empty()));
    }

    #[test]
    fn the_cursor_stays_on_its_file_when_the_list_refreshes() {
        let (tmp, mut s) = session();
        for n in ["a.txt", "c.txt"] {
            std::fs::write(tmp.path().join("left").join(n), "x").unwrap();
        }
        s.run_line("refresh").unwrap();
        s.run_line("select c.txt").unwrap();
        s.poll();
        std::fs::write(tmp.path().join("left/b.txt"), "x").unwrap();
        assert!(poll_until(&mut s, |s| names(s, 0).len() == 3));
        assert_eq!(s.panels[0].current().unwrap().name, "c.txt");
    }

    #[test]
    fn watch_off_stops_it_and_the_choice_is_kept_in_the_settings() {
        let (tmp, mut s) = session();
        assert!(s.watching());
        s.run_line("watch off").unwrap();
        assert!(!s.watching());
        s.poll();
        std::fs::write(tmp.path().join("left/a.txt"), "x").unwrap();
        for _ in 0..30 {
            s.poll();
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(names(&s, 0).is_empty(), "nothing refreshes by itself");
        let mut again = Session::new(tmp.path().join("left"), tmp.path().join("right"));
        again.use_config_dir(&tmp.path().join("config"));
        assert!(!again.watching());
        again.run_line("watch on").unwrap();
        assert!(again.watching());
        assert!(again.run_line("watch maybe").is_err());
        again.run_line("watch").unwrap();
        assert!(again.status.contains("same"), "{}", again.status);
    }

    #[test]
    fn archives_and_servers_are_not_watched() {
        let (tmp, mut s) = session();
        s.poll();
        let dir = s.panels[0].cwd.clone();
        s.panels[0].virtual_list = Some(crate::panel::VirtualList {
            title: "wyniki".into(),
            paths: Vec::new(),
        });
        s.poll();
        assert!(
            !s.watcher.watches(&dir),
            "a result list has no folder to watch"
        );
        s.panels[0].virtual_list = None;
        s.poll();
        assert!(s.watcher.watches(&dir));
        let _ = tmp;
    }

    #[test]
    fn parsing() {
        assert_eq!(parse_watch("").unwrap(), WatchAction::Show);
        assert_eq!(parse_watch(" ON ").unwrap(), WatchAction::On);
        assert_eq!(parse_watch("off").unwrap(), WatchAction::Off);
        assert!(parse_watch("x").is_err());
    }
}
