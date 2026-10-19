//! "Jump to" (Ctrl+P): one search box over the folders visited most often and most recently,
//! the tabs of the active panel and the saved servers. Enter goes there in the active panel.
//! The visit history is kept between runs in `visits.conf` of the config directory.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::palette::{fold, Palette};
use crate::session::Session;
use crate::EngineError;

const VISITS_FILE: &str = "visits.conf";
/// The history keeps this many folders; the least useful ones are dropped.
const VISITS_MAX: usize = 500;
/// Folders offered without a query.
const EMPTY_QUERY_DIRS: usize = 30;

/// How often and how lately the folders were visited.
#[derive(Debug, Default, Clone)]
pub struct Visits {
    /// Folder -> (number of visits, unix time of the last one).
    map: HashMap<PathBuf, (u32, u64)>,
    path: Option<PathBuf>,
}

impl Visits {
    /// Loads `path` (missing or damaged lines are ignored) and saves there from now on.
    pub fn load(path: &Path) -> Self {
        let mut map = HashMap::new();
        for line in std::fs::read_to_string(path).unwrap_or_default().lines() {
            let mut parts = line.splitn(3, '\t');
            let (Some(count), Some(time), Some(folder)) =
                (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            if let (Ok(count), Ok(time)) = (count.parse(), time.parse()) {
                map.insert(PathBuf::from(folder), (count, time));
            }
        }
        Visits {
            map,
            path: Some(path.to_path_buf()),
        }
    }

    /// Counts one visit of `folder` at unix time `now`.
    pub fn record_at(&mut self, folder: &Path, now: u64) {
        let entry = self.map.entry(folder.to_path_buf()).or_insert((0, now));
        entry.0 += 1;
        entry.1 = now;
        if self.map.len() > VISITS_MAX {
            let worst = self
                .map
                .iter()
                .map(|(p, v)| (p.clone(), score(*v, now)))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(p, _)| p);
            if let Some(worst) = worst {
                self.map.remove(&worst);
            }
        }
        self.save();
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        let mut lines: Vec<String> = self
            .map
            .iter()
            .map(|(folder, (count, time))| format!("{count}\t{time}\t{}", folder.display()))
            .collect();
        lines.sort();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, lines.join("\n"));
    }

    /// Every folder with its "frecency" score at `now`, best first.
    pub fn ranked(&self, now: u64) -> Vec<(PathBuf, f64)> {
        let mut all: Vec<(PathBuf, f64)> = self
            .map
            .iter()
            .map(|(p, v)| (p.clone(), score(*v, now)))
            .collect();
        all.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        all
    }

    /// The `count` folders visited last, newest first.
    pub fn latest(&self, count: usize) -> Vec<PathBuf> {
        let mut all: Vec<(&PathBuf, u64)> = self.map.iter().map(|(p, v)| (p, v.1)).collect();
        all.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        all.into_iter()
            .take(count)
            .map(|(p, _)| p.clone())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// Visits weigh more the more recent they are (the last hour counts four times, the last
/// day twice, the last week once, older ones half).
fn score((count, last): (u32, u64), now: u64) -> f64 {
    let age = now.saturating_sub(last);
    let weight = match age {
        0..=3_599 => 4.0,
        3_600..=86_399 => 2.0,
        86_400..=604_799 => 1.0,
        _ => 0.5,
    };
    f64::from(count) * weight
}

pub(crate) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// One row of the "jump to" list.
#[derive(Debug, Clone, PartialEq)]
pub struct JumpEntry {
    /// "Katalog", "Zakładka" or "Serwer".
    pub kind: &'static str,
    /// What is shown (the folder, the tab's location, the site's name).
    pub label: String,
    /// The command line that goes there.
    pub line: String,
}

impl Session {
    /// The rows of the open "jump to" list (empty when it is closed).
    pub fn jump_entries(&self) -> Vec<JumpEntry> {
        self.jump_entries_at(unix_now())
    }

    pub(crate) fn jump_entries_at(&self, now: u64) -> Vec<JumpEntry> {
        let Some(jump) = &self.jump else {
            return Vec::new();
        };
        self.place_rows(&jump.query, now)
    }

    /// The places (tabs, visited folders, servers) that fit `query`, best first.
    pub(crate) fn place_rows(&self, query: &str, now: u64) -> Vec<JumpEntry> {
        let folded = fold(query);
        let words: Vec<&str> = folded.split_whitespace().collect();
        let fits = |text: &str| {
            let text = fold(text);
            words.iter().all(|w| text.contains(w))
        };
        let mut out = Vec::new();

        let strip = self.tab_strip(self.active);
        if strip.paths.len() > 1 {
            for (index, path) in strip.paths.iter().enumerate() {
                let label = path.display().to_string();
                if index != strip.active && fits(&label) {
                    out.push(JumpEntry {
                        kind: "Zakładka",
                        label,
                        line: format!("tab {}", index + 1),
                    });
                }
            }
        }

        let here = self.active_panel().location();
        let mut dirs = 0;
        for (folder, _) in self.visits.ranked(now) {
            if folder == here || !fits(&folder.display().to_string()) || !folder.is_dir() {
                continue;
            }
            out.push(JumpEntry {
                kind: "Katalog",
                label: folder.display().to_string(),
                line: format!("cd {}", folder.display()),
            });
            dirs += 1;
            if words.is_empty() && dirs >= EMPTY_QUERY_DIRS {
                break;
            }
        }

        for site in &self.sites.sites {
            let label = format!("{} ({})", site.name, site.host);
            if fits(&label) {
                out.push(JumpEntry {
                    kind: "Serwer",
                    label,
                    line: format!("connect {}", site.name),
                });
            }
        }
        // A query that names a folder's last component puts that folder first.
        if let Some(word) = words.first() {
            out.sort_by_key(|e| {
                let last = Path::new(&e.label)
                    .file_name()
                    .map(|n| fold(&n.to_string_lossy()))
                    .unwrap_or_default();
                match (e.kind, last.as_str()) {
                    (_, name) if name == *word => 0,
                    (_, name) if name.starts_with(word) => 1,
                    _ => 2,
                }
            });
        }
        out
    }

    pub(crate) fn jump_open(&mut self, query: String) {
        self.palette = None;
        self.quickbox = None;
        self.jump = Some(Palette { query, selected: 0 });
    }

    pub(crate) fn jump_set_query(&mut self, query: String) {
        match &mut self.jump {
            Some(j) => {
                j.query = query;
                j.selected = 0;
            }
            None => self.jump_open(query),
        }
    }

    pub(crate) fn jump_move(&mut self, by: i32) {
        let count = self.jump_entries().len();
        if let Some(j) = &mut self.jump {
            let last = count.saturating_sub(1) as i64;
            j.selected = (j.selected as i64 + i64::from(by)).clamp(0, last) as usize;
        }
    }

    pub(crate) fn jump_run(&mut self) -> Result<(), EngineError> {
        let entries = self.jump_entries();
        let Some(selected) = self.jump.as_ref().map(|j| j.selected) else {
            return Ok(());
        };
        let Some(entry) = entries.get(selected) else {
            self.status = "Nie ma takiego miejsca".into();
            self.status_is_error = true;
            return Ok(());
        };
        let line = entry.line.clone();
        self.jump = None;
        self.run_line(&line)
    }

    /// Counts the folders the panels are in now as visited (called after every command).
    pub(crate) fn note_visits(&mut self) {
        for side in 0..2 {
            let panel = &self.panels[side];
            let local =
                panel.remote.is_none() && panel.archive.is_none() && panel.virtual_list.is_none();
            let folder = local.then(|| panel.cwd.clone());
            if folder != self.last_visit[side] {
                if let Some(folder) = &folder {
                    self.visits.record_at(folder, unix_now());
                }
                self.last_visit[side] = folder;
            }
        }
    }

    pub(crate) fn load_visits(&mut self, dir: &Path) {
        self.visits = Visits::load(&dir.join(VISITS_FILE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 86_400;

    #[test]
    fn frequent_and_recent_folders_rank_higher() {
        let mut v = Visits::default();
        let now = 100 * DAY;
        v.record_at(Path::new("/old"), now - 60 * DAY);
        for _ in 0..3 {
            v.record_at(Path::new("/old"), now - 60 * DAY);
        }
        v.record_at(Path::new("/often"), now - 3 * DAY);
        v.record_at(Path::new("/often"), now - 3 * DAY);
        v.record_at(Path::new("/just-now"), now - 10);
        let order: Vec<_> = v.ranked(now).into_iter().map(|(p, _)| p).collect();
        assert_eq!(
            order,
            [
                PathBuf::from("/just-now"),
                PathBuf::from("/often"),
                PathBuf::from("/old")
            ]
        );
    }

    #[test]
    fn the_history_is_saved_loaded_and_capped() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("v").join("visits.conf");
        let mut v = Visits::load(&file);
        v.record_at(Path::new("/a b/c"), 10);
        v.record_at(Path::new("/a b/c"), 20);
        let again = Visits::load(&file);
        assert_eq!(again.len(), 1);
        assert_eq!(again.ranked(20)[0].0, PathBuf::from("/a b/c"));
        std::fs::write(&file, "garbage\n1\tx\t/p\n2\t5\t/ok").unwrap();
        assert_eq!(Visits::load(&file).len(), 1, "damaged lines are skipped");

        let mut many = Visits::default();
        for i in 0..VISITS_MAX + 20 {
            many.record_at(Path::new(&format!("/dir{i}")), 1000 + i as u64);
        }
        assert_eq!(many.len(), VISITS_MAX);
    }

    fn session(tmp: &Path) -> Session {
        std::fs::create_dir_all(tmp.join("left")).unwrap();
        std::fs::create_dir_all(tmp.join("right")).unwrap();
        std::fs::create_dir_all(tmp.join("left/projekty/żółw")).unwrap();
        let mut s = Session::new(tmp.join("left"), tmp.join("right"));
        s.use_config_dir(&tmp.join("config"));
        s
    }

    #[test]
    fn visited_folders_are_counted_and_offered() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("cd projekty").unwrap();
        s.run_line("cd żółw").unwrap();
        s.run_line("cd ..").unwrap();
        s.run_line("jump").unwrap();
        let labels: Vec<String> = s.jump_entries().into_iter().map(|e| e.label).collect();
        assert!(labels.iter().any(|l| l.ends_with("żółw")), "{labels:?}");
        // The folder the panel is in now is not offered.
        assert!(
            !labels.iter().any(|l| l.ends_with("projekty")),
            "{labels:?}"
        );
        s.run_line("jump query zolw").unwrap();
        assert_eq!(s.jump_entries().len(), 1, "diacritics are ignored");
    }

    #[test]
    fn a_folder_that_no_longer_exists_is_not_offered() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("cd projekty").unwrap();
        s.run_line("cd żółw").unwrap();
        s.run_line("cd ..").unwrap();
        s.run_line("cd ..").unwrap();
        std::fs::remove_dir_all(tmp.path().join("left/projekty/żółw")).unwrap();
        s.run_line("jump query zolw").unwrap();
        assert!(s.jump_entries().is_empty());
    }

    #[test]
    fn enter_goes_to_the_chosen_folder_and_tabs_are_listed() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("cd projekty").unwrap();
        s.run_line("cd żółw").unwrap();
        s.run_line("cd ../..").unwrap();
        s.run_line("tab new").unwrap();
        s.run_line("cd projekty").unwrap();
        s.run_line("jump query left").unwrap();
        let entries = s.jump_entries();
        assert!(entries.iter().any(|e| e.kind == "Zakładka"), "{entries:?}");
        s.run_line("jump query zolw").unwrap();
        s.run_line("jump run").unwrap();
        assert!(s.jump.is_none());
        assert!(s.active_panel().cwd.ends_with("żółw"));
        // The history survives a restart.
        let again = session(tmp.path());
        assert!(again.visits.len() >= 2);
    }

    #[test]
    fn a_saved_server_is_offered_and_a_missing_choice_is_harmless() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("siteset nas host nas.local").unwrap();
        s.run_line("jump query nas").unwrap();
        let entries = s.jump_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, "Serwer");
        assert_eq!(entries[0].line, "connect nas");
        s.run_line("jump query zzzzzz").unwrap();
        s.run_line("jump run").unwrap();
        assert!(s.jump.is_some(), "nothing was chosen, the list stays");
        s.run_line("jump close").unwrap();
        assert!(s.jump.is_none());
    }
}
