//! The git status of the files of a folder, for the Git column. It is asked for once per
//! folder (in the background, by the `git` program) after the column was switched on, and
//! again on `gitstatus`; a folder outside a repository or a machine without git shows nothing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::session::Session;
use crate::settings::Column;
use crate::EngineError;

/// Status letters of one folder's entries (a folder gets the letter of what changed inside).
type Marks = HashMap<PathBuf, char>;

/// What was asked of git so far. Shared with the threads that ask.
#[derive(Debug, Clone, Default)]
pub struct GitStatus {
    folders: Arc<Mutex<HashMap<PathBuf, Marks>>>,
    running: Arc<AtomicUsize>,
    version: Arc<AtomicU64>,
}

impl GitStatus {
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed) > 0
    }

    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }

    /// True when `folder` was already asked about (answered or in progress).
    fn knows(&self, folder: &Path) -> bool {
        self.folders
            .lock()
            .is_ok_and(|folders| folders.contains_key(folder))
    }

    pub fn mark(&self, folder: &Path, path: &Path) -> Option<char> {
        self.folders.lock().ok()?.get(folder)?.get(path).copied()
    }

    pub(crate) fn forget(&self, folder: &Path) {
        if let Ok(mut folders) = self.folders.lock() {
            folders.remove(folder);
        }
    }

    fn start(&self, folder: PathBuf) {
        // An empty answer first, so the folder is not asked for again while git works.
        if let Ok(mut folders) = self.folders.lock() {
            folders.insert(folder.clone(), Marks::new());
        }
        let (folders, running, version) = (
            self.folders.clone(),
            self.running.clone(),
            self.version.clone(),
        );
        running.fetch_add(1, Ordering::Relaxed);
        std::thread::spawn(move || {
            let marks = ask_git(&folder).unwrap_or_default();
            if let Ok(mut map) = folders.lock() {
                // Only if the folder was not forgotten meanwhile.
                if let Some(slot) = map.get_mut(&folder) {
                    *slot = marks;
                }
            }
            version.fetch_add(1, Ordering::Relaxed);
            running.fetch_sub(1, Ordering::Relaxed);
        });
    }
}

fn git(folder: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let out = bluepane_vfs::run_hidden("git", args, folder).ok()?;
    out.status.success().then_some(out.stdout)
}

fn ask_git(folder: &Path) -> Option<Marks> {
    // Where `folder` stands inside the repository ("" at the top, else "sub/dir/"). Paths are
    // matched relative to the folder, so the spelling of the absolute path (forward slashes,
    // short names on Windows) does not matter.
    let prefix = git(folder, &["rev-parse", "--show-prefix"])?;
    let prefix = String::from_utf8_lossy(&prefix).trim_end().to_string();
    let status = git(
        folder,
        &["status", "--porcelain=v1", "-z", "--untracked-files=normal"],
    )?;
    Some(parse_porcelain(
        folder,
        &prefix,
        &String::from_utf8_lossy(&status),
    ))
}

/// The letter for a two-letter porcelain status: `?` new, `U` conflict, `D` deleted, `A`
/// added, `R` renamed, `M` changed.
fn letter(x: char, y: char) -> char {
    match (x, y) {
        ('?', _) => '?',
        ('U', _) | (_, 'U') | ('A', 'A') | ('D', 'D') => 'U',
        (_, 'D') | ('D', _) => 'D',
        ('A', _) => 'A',
        ('R', _) | ('C', _) => 'R',
        _ => 'M',
    }
}

/// How serious a letter is when a folder holds several kinds of change.
fn weight(letter: char) -> u8 {
    match letter {
        'U' => 5,
        'D' => 4,
        'M' => 3,
        'R' => 2,
        'A' => 1,
        _ => 0,
    }
}

/// Reads `git status --porcelain=v1 -z`: the paths are relative to the repository root, and
/// `prefix` is where `folder` stands in it. Changes outside the folder are of no interest.
fn parse_porcelain(folder: &Path, prefix: &str, text: &str) -> Marks {
    let mut marks = Marks::new();
    let mut parts = text.split('\0').filter(|p| !p.is_empty());
    while let Some(part) = parts.next() {
        let mut chars = part.chars();
        let (Some(x), Some(y)) = (chars.next(), chars.next()) else {
            continue;
        };
        let name = part.get(3..).unwrap_or("");
        // A rename is followed by the old name as a separate item.
        if x == 'R' || x == 'C' {
            let _ = parts.next();
        }
        let Some(inside) = name.strip_prefix(prefix) else {
            continue;
        };
        let mark = letter(x, y);
        // The entry itself and every folder between it and `folder`.
        let mut path = folder.to_path_buf();
        let mut paths = Vec::new();
        for component in inside.split('/').filter(|c| !c.is_empty()) {
            path.push(component);
            paths.push(path.clone());
        }
        for (depth, path) in paths.into_iter().enumerate() {
            let is_entry_itself = !inside.trim_end_matches('/').is_empty()
                && depth + 1 == inside.trim_end_matches('/').split('/').count();
            let slot = marks.entry(path).or_insert(mark);
            if !is_entry_itself && weight(mark) > weight(*slot) {
                *slot = mark;
            }
        }
    }
    marks
}

impl Session {
    /// `gitstatus`: asks git again about the folder of the active panel.
    pub(crate) fn refresh_git_status(&mut self) -> Result<(), EngineError> {
        let panel = self.active_panel();
        if panel.archive.is_some() || panel.remote.is_some() || panel.virtual_list.is_some() {
            return Err(EngineError(
                "status gita pokazuje się tylko dla katalogu na dysku".into(),
            ));
        }
        let folder = panel.cwd.clone();
        self.git_status.forget(&folder);
        self.git_status.start(folder);
        self.status = "Pytam gita o stan plików…".into();
        Ok(())
    }

    /// Called after every command: with the Git column on, each local folder shown in a panel is
    /// asked about once.
    pub(crate) fn note_git(&mut self) {
        if !self.columns().contains(&Column::Git) {
            return;
        }
        for side in 0..2 {
            let panel = &self.panels[side];
            let local =
                panel.remote.is_none() && panel.archive.is_none() && panel.virtual_list.is_none();
            if local && !self.git_status.knows(&panel.cwd) {
                let folder = panel.cwd.clone();
                self.git_status.start(folder);
            }
        }
    }

    /// The letter in the Git column for an entry of the panel on `side` (empty for none).
    pub fn git_cell(&self, side: usize, entry: &bluepane_vfs::Entry) -> String {
        let panel = &self.panels[side];
        self.git_status
            .mark(&panel.cwd, &entry.path)
            .map(String::from)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) -> bool {
        bluepane_vfs::run_hidden("git", args, dir).is_ok_and(|o| o.status.success())
    }

    fn wait(s: &Session) {
        let start = std::time::Instant::now();
        while s.git_status.is_running() {
            assert!(start.elapsed().as_secs() < 20, "git did not answer");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn porcelain_output_gives_letters_for_files_and_the_folders_above() {
        let folder = Path::new("/repo");
        let text = " M src/a.rs\0?? new.txt\0A  src/deep/b.rs\0 D gone.txt\0R  new_name.rs\0old_name.rs\0UU merge.rs\0";
        let marks = parse_porcelain(folder, "", text);
        assert_eq!(marks[&folder.join("src").join("a.rs")], 'M');
        assert_eq!(marks[&folder.join("new.txt")], '?');
        assert_eq!(marks[&folder.join("src").join("deep").join("b.rs")], 'A');
        assert_eq!(marks[&folder.join("gone.txt")], 'D');
        assert_eq!(marks[&folder.join("new_name.rs")], 'R');
        assert_eq!(marks[&folder.join("merge.rs")], 'U');
        assert!(!marks.contains_key(&folder.join("old_name.rs")));
        // A folder shows the most serious change inside; the folder itself is not marked.
        assert_eq!(marks[&folder.join("src")], 'M');
        assert_eq!(marks[&folder.join("src").join("deep")], 'A');
        assert!(!marks.contains_key(folder));
        assert!(parse_porcelain(folder, "", "").is_empty());
    }

    #[test]
    fn inside_a_subfolder_only_its_own_changes_count_and_paths_are_relative_to_it() {
        let folder = Path::new("/repo/sub");
        let text = " M sub/x.txt\0 M other/y.txt\0?? sub/dir/z.txt\0";
        let marks = parse_porcelain(folder, "sub/", text);
        assert_eq!(marks[&folder.join("x.txt")], 'M');
        assert_eq!(marks[&folder.join("dir")], '?');
        assert_eq!(marks.len(), 3);
        // Untracked folders are reported with a trailing slash.
        let marks = parse_porcelain(folder, "sub/", "?? sub/newdir/\0");
        assert_eq!(marks[&folder.join("newdir")], '?');
    }

    #[test]
    fn a_real_repository_shows_changed_and_new_files_and_a_plain_folder_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        if !run(dir, &["init", "-q"]) {
            return; // no git on this machine: the column simply stays empty
        }
        let _ = run(dir, &["config", "user.email", "t@example.com"]);
        let _ = run(dir, &["config", "user.name", "T"]);
        std::fs::write(dir.join("kept.txt"), "1").unwrap();
        std::fs::create_dir(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/inner.txt"), "1").unwrap();
        assert!(run(dir, &["add", "."]));
        assert!(run(dir, &["commit", "-q", "-m", "first"]));
        std::fs::write(dir.join("kept.txt"), "2").unwrap();
        std::fs::write(dir.join("sub/inner.txt"), "2").unwrap();
        std::fs::write(dir.join("Zażółć.txt"), "new").unwrap();

        let mut s = Session::new(dir, dir);
        s.run_line("columns git").unwrap();
        wait(&s);
        let cell = |s: &Session, name: &str| {
            let entry = s.panels[0].entries.iter().find(|e| e.name == name).unwrap();
            s.git_cell(0, entry)
        };
        assert_eq!(cell(&s, "kept.txt"), "M");
        assert_eq!(cell(&s, "sub"), "M");
        assert_eq!(cell(&s, "Zażółć.txt"), "?");
        // Asking again after a commit shows a clean folder.
        assert!(run(dir, &["add", "."]));
        assert!(run(dir, &["commit", "-q", "-m", "second"]));
        s.run_line("gitstatus").unwrap();
        wait(&s);
        assert_eq!(cell(&s, "kept.txt"), "");
    }

    #[test]
    fn outside_a_repository_and_without_the_column_nothing_is_asked() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "x").unwrap();
        let mut s = Session::new(tmp.path(), tmp.path());
        s.run_line("refresh").unwrap();
        assert!(!s.git_status.is_running() && s.git_status.version() == 0);
        s.run_line("columns git").unwrap();
        wait(&s);
        let entry = s.panels[0].entries[0].clone();
        assert_eq!(s.git_cell(0, &entry), "");
        assert!(s.run_line("gitstatus").is_ok());
    }
}
