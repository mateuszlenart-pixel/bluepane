//! The git status of the files of a folder, for the Git column, and the branch shown beside
//! the path. It is asked for once per folder (in the background, by the `git` program) after
//! the column was switched on, again on `gitstatus`, and again when the folder changes on
//! disk; a folder outside a repository or a machine without git shows nothing.
//!
//! `git add|unstage|commit|diff|log` are the basic actions on the files under the cursor (or
//! the selected ones). They run the `git` program in the folder of the active panel; nothing
//! is pushed or pulled, and a commit takes only what was staged.

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
    /// The branch line of each folder asked about ("main ↑1 ↓2"); absent outside a repository.
    branches: Arc<Mutex<HashMap<PathBuf, String>>>,
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

    /// The branch of the repository `folder` is in, with how far it is ahead of or behind its
    /// upstream (`main ↑1 ↓2`); a detached head shows `(abc1234)`.
    pub fn branch(&self, folder: &Path) -> Option<String> {
        self.branches.lock().ok()?.get(folder).cloned()
    }

    pub(crate) fn forget(&self, folder: &Path) {
        if let Ok(mut folders) = self.folders.lock() {
            folders.remove(folder);
        }
        if let Ok(mut branches) = self.branches.lock() {
            branches.remove(folder);
        }
    }

    pub(crate) fn start(&self, folder: PathBuf) {
        // An empty answer first (when there is none), so the folder is not asked for again
        // while git works; an older answer stays until the new one arrives.
        if let Ok(mut folders) = self.folders.lock() {
            folders.entry(folder.clone()).or_default();
        }
        let (folders, branches, running, version) = (
            self.folders.clone(),
            self.branches.clone(),
            self.running.clone(),
            self.version.clone(),
        );
        running.fetch_add(1, Ordering::Relaxed);
        std::thread::spawn(move || {
            let (marks, branch) = ask_git(&folder);
            if let Ok(mut map) = folders.lock() {
                // Only if the folder was not forgotten meanwhile.
                if let Some(slot) = map.get_mut(&folder) {
                    *slot = marks;
                    if let Ok(mut branches) = branches.lock() {
                        match branch {
                            Some(branch) => branches.insert(folder.clone(), branch),
                            None => branches.remove(&folder),
                        };
                    }
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

/// The marks of the files and the branch line of the folder; empty outside a repository.
fn ask_git(folder: &Path) -> (Marks, Option<String>) {
    // Where `folder` stands inside the repository ("" at the top, else "sub/dir/"). Paths are
    // matched relative to the folder, so the spelling of the absolute path (forward slashes,
    // short names on Windows) does not matter.
    let Some(prefix) = git(folder, &["rev-parse", "--show-prefix"]) else {
        return (Marks::new(), None);
    };
    let prefix = String::from_utf8_lossy(&prefix).trim_end().to_string();
    // `--no-optional-locks`: this runs in the background, next to the user's own `git add` or
    // `commit`; a plain `status` refreshes the index and could hold `index.lock` just then.
    let marks = git(
        folder,
        &[
            "--no-optional-locks",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=normal",
        ],
    )
    .map(|status| parse_porcelain(folder, &prefix, &String::from_utf8_lossy(&status)))
    .unwrap_or_default();
    (marks, branch_line(folder))
}

/// `main`, `main ↑1`, `main ↑1 ↓2` or `(abc1234)` for a detached head.
fn branch_line(folder: &Path) -> Option<String> {
    let text = |args: &[&str]| {
        git(folder, args).map(|out| String::from_utf8_lossy(&out).trim().to_string())
    };
    let name = match text(&["branch", "--show-current"]).filter(|n| !n.is_empty()) {
        Some(name) => name,
        None => format!("({})", text(&["rev-parse", "--short", "HEAD"])?),
    };
    let mut line = name;
    // Without an upstream this fails, and nothing is added. Local data only: no fetch.
    if let Some(counts) = text(&["rev-list", "--left-right", "--count", "@{upstream}...HEAD"]) {
        let mut numbers = counts
            .split_whitespace()
            .map(|n| n.parse::<u64>().unwrap_or(0));
        let (behind, ahead) = (numbers.next().unwrap_or(0), numbers.next().unwrap_or(0));
        if ahead > 0 {
            line.push_str(&format!(" ↑{ahead}"));
        }
        if behind > 0 {
            line.push_str(&format!(" ↓{behind}"));
        }
    }
    Some(line)
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

/// What `git <action>` does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitAction {
    /// Stage the selected files (or the one under the cursor).
    Add,
    /// Take them out of the staging area again (the files keep their changes).
    Unstage,
    /// Commit what is staged, with this message.
    Commit(String),
    /// Show the changes of the file under the cursor (or of the folder) against the last commit.
    Diff,
    /// Show the history of the file under the cursor (or of the folder).
    Log,
}

/// `git add|unstage|commit <message>|diff|log`
pub(crate) fn parse_git(rest: &str) -> Result<GitAction, EngineError> {
    let usage =
        || EngineError("git: użycie: git add | unstage | commit <opis> | diff | log".into());
    let rest = rest.trim();
    let (word, tail) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    let tail = tail.trim();
    match word.to_lowercase().as_str() {
        "add" | "stage" if tail.is_empty() => Ok(GitAction::Add),
        "unstage" | "reset" if tail.is_empty() => Ok(GitAction::Unstage),
        "diff" if tail.is_empty() => Ok(GitAction::Diff),
        "log" if tail.is_empty() => Ok(GitAction::Log),
        "commit" => {
            let message = tail.trim_matches('"').trim();
            if message.is_empty() {
                Err(EngineError(
                    "git commit: podaj opis zmiany, np. git commit \"Poprawka literówki\"".into(),
                ))
            } else {
                Ok(GitAction::Commit(message.to_string()))
            }
        }
        _ => Err(usage()),
    }
}

/// Runs git in `folder`; the error is the first line it complained with.
fn run_git(folder: &Path, args: &[&str]) -> Result<String, EngineError> {
    let out = match bluepane_vfs::run_hidden("git", args, folder) {
        Ok(out) => out,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(EngineError(
                "nie znaleziono programu git: zainstaluj go (git-scm.com)".into(),
            ))
        }
        Err(e) => return Err(EngineError(format!("nie udało się uruchomić gita: {e}"))),
    };
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if out.status.success() {
        return Ok(stdout);
    }
    // Some complaints ("nothing to commit") come on the standard output.
    let stderr = String::from_utf8_lossy(&out.stderr);
    let first = stderr
        .lines()
        .chain(stdout.lines())
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("git zgłosił błąd");
    Err(EngineError(format!("git: {first}")))
}

impl Session {
    /// The folder of the active panel when it is an ordinary one on disk.
    fn git_folder(&self) -> Result<PathBuf, EngineError> {
        let panel = self.active_panel();
        if panel.archive.is_some() || panel.remote.is_some() || panel.virtual_list.is_some() {
            return Err(EngineError(
                "git działa tylko w katalogu na dysku (nie w archiwum, na serwerze ani na liście wyników)"
                    .into(),
            ));
        }
        Ok(panel.cwd.clone())
    }

    /// Asks again about the folder, when the git column is on (the answer is shown by it).
    fn git_changed(&mut self, folder: &Path) {
        if self.columns().contains(&Column::Git) {
            self.git_status.start(folder.to_path_buf());
        }
    }

    pub(crate) fn git_action(&mut self, action: GitAction) -> Result<(), EngineError> {
        let folder = self.git_folder()?;
        let targets: Vec<PathBuf> = self.active_panel().targets();
        let names: Vec<String> = targets.iter().map(|p| p.display().to_string()).collect();
        let with_paths = |first: &[&str]| -> Result<String, EngineError> {
            if names.is_empty() {
                return Err(EngineError("zaznacz plik albo ustaw na nim kursor".into()));
            }
            let mut args: Vec<&str> = first.to_vec();
            args.push("--");
            args.extend(names.iter().map(String::as_str));
            run_git(&folder, &args)
        };
        match action {
            GitAction::Add => {
                with_paths(&["add"])?;
                self.status = format!("Git: dodano do poczekalni ({})", names.len());
                self.git_changed(&folder);
            }
            GitAction::Unstage => {
                with_paths(&["reset", "-q"])?;
                self.status = format!("Git: zdjęto z poczekalni ({})", names.len());
                self.git_changed(&folder);
            }
            GitAction::Commit(message) => {
                let out = run_git(&folder, &["commit", "-m", &message])?;
                let first = out.lines().next().unwrap_or("").trim().to_string();
                self.status = format!("Git: zatwierdzono. {first}");
                self.git_changed(&folder);
            }
            GitAction::Diff => {
                let scope: Vec<String> = match self.active_panel().current() {
                    Some(entry) if entry.name != ".." => vec![entry.path.display().to_string()],
                    _ => Vec::new(),
                };
                let mut args = vec!["diff", "HEAD", "--"];
                args.extend(scope.iter().map(String::as_str));
                // A repository without a commit yet has no HEAD to compare with.
                let diff = run_git(&folder, &args).or_else(|_| {
                    let mut args = vec!["diff", "--"];
                    args.extend(scope.iter().map(String::as_str));
                    run_git(&folder, &args)
                })?;
                if diff.trim().is_empty() {
                    self.status = "Git: brak zmian względem ostatniego zatwierdzenia".into();
                } else {
                    self.show_git_text("zmiany.diff", &diff)?;
                    self.status =
                        "Git: zmiany względem ostatniego zatwierdzenia (Esc zamyka)".into();
                }
            }
            GitAction::Log => {
                let scope: Vec<String> = match self.active_panel().current() {
                    Some(entry) if entry.name != ".." => vec![entry.path.display().to_string()],
                    _ => Vec::new(),
                };
                let mut args = vec!["log", "--oneline", "--decorate", "-n", "300", "--"];
                args.extend(scope.iter().map(String::as_str));
                let log = run_git(&folder, &args)?;
                if log.trim().is_empty() {
                    self.status = "Git: brak historii".into();
                } else {
                    self.show_git_text("historia.txt", &log)?;
                    self.status = "Git: historia (Esc zamyka)".into();
                }
            }
        }
        Ok(())
    }

    /// Shows git's output in the viewer, through a file in a scratch folder.
    pub(crate) fn show_git_text(&mut self, name: &str, text: &str) -> Result<(), EngineError> {
        let scratch = std::env::temp_dir().join(format!(
            "bluepane-view-{}-{}",
            std::process::id(),
            crate::session::next_preview_number()
        ));
        let result = std::fs::create_dir_all(&scratch)
            .and_then(|()| std::fs::write(scratch.join(name), text))
            .map_err(|e| EngineError(format!("nie udało się przygotować podglądu: {e}")))
            .and_then(|()| crate::Viewer::open(&scratch.join(name)));
        match result {
            Ok(viewer) => {
                self.remove_preview_dir();
                self.preview_dir = Some(scratch);
                self.viewer = Some(viewer);
                Ok(())
            }
            Err(e) => {
                let _ = std::fs::remove_dir_all(&scratch);
                Err(e)
            }
        }
    }

    /// The branch line shown beside the path of the panel on `side` (needs the git column).
    pub fn git_branch(&self, side: usize) -> Option<String> {
        let panel = &self.panels[side];
        let local =
            panel.remote.is_none() && panel.archive.is_none() && panel.virtual_list.is_none();
        local.then(|| self.git_status.branch(&panel.cwd)).flatten()
    }

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

    /// A repository with one commit; `None` when this machine has no git.
    fn repository() -> Option<tempfile::TempDir> {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        if !run(dir, &["init", "-q", "-b", "main"]) {
            return None;
        }
        let _ = run(dir, &["config", "user.email", "t@example.com"]);
        let _ = run(dir, &["config", "user.name", "T"]);
        let _ = run(dir, &["config", "commit.gpgsign", "false"]);
        std::fs::write(dir.join("kept.txt"), "1\n").unwrap();
        assert!(run(dir, &["add", "."]));
        assert!(run(dir, &["commit", "-q", "-m", "pierwszy"]));
        Some(tmp)
    }

    fn mark(s: &Session, name: &str) -> String {
        let entry = s.panels[0].entries.iter().find(|e| e.name == name).unwrap();
        s.git_cell(0, entry)
    }

    #[test]
    fn the_branch_is_shown_for_a_repository_and_for_nothing_else() {
        let Some(tmp) = repository() else { return };
        let dir = tmp.path();
        let mut s = Session::new(dir, dir);
        assert_eq!(
            s.git_branch(0),
            None,
            "nothing is asked until the column is on"
        );
        s.run_line("columns git").unwrap();
        wait(&s);
        assert_eq!(s.git_branch(0).as_deref(), Some("main"));
        // A detached head shows the short hash.
        assert!(run(dir, &["checkout", "-q", "--detach"]));
        s.run_line("gitstatus").unwrap();
        wait(&s);
        let line = s.git_branch(0).unwrap();
        assert!(line.starts_with('(') && line.ends_with(')'), "{line}");
        // A folder that is no repository has no branch.
        let plain = tempfile::tempdir().unwrap();
        s.run_line(&format!("cd {}", plain.path().display()))
            .unwrap();
        wait(&s);
        assert_eq!(s.git_branch(0), None);
    }

    #[test]
    fn add_unstage_and_commit_work_on_the_selected_files_with_polish_names() {
        let Some(tmp) = repository() else { return };
        let dir = tmp.path();
        std::fs::write(dir.join("kept.txt"), "2\n").unwrap();
        std::fs::write(dir.join("Zażółć gęślą.txt"), "nowy").unwrap();
        let mut s = Session::new(dir, dir);
        s.run_line("columns git").unwrap();
        wait(&s);
        assert_eq!(mark(&s, "Zażółć gęślą.txt"), "?");

        s.run_line("select Zażółć gęślą.txt").unwrap();
        s.run_line("git add").unwrap();
        wait(&s);
        assert_eq!(mark(&s, "Zażółć gęślą.txt"), "A");
        assert_eq!(mark(&s, "kept.txt"), "M", "the other file was not staged");
        s.run_line("git unstage").unwrap();
        wait(&s);
        assert_eq!(mark(&s, "Zażółć gęślą.txt"), "?");

        s.run_line("git add").unwrap();
        s.run_line("git commit \"Dodano plik z polską nazwą: zażółć\"")
            .unwrap();
        wait(&s);
        assert!(s.status.contains("zatwierdzono"), "{}", s.status);
        assert_eq!(mark(&s, "Zażółć gęślą.txt"), "");
        assert_eq!(
            mark(&s, "kept.txt"),
            "M",
            "an unstaged change stays out of the commit"
        );
        let log = bluepane_vfs::run_hidden("git", &["log", "--format=%s", "-n", "1"], dir).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&log.stdout).trim(),
            "Dodano plik z polską nazwą: zażółć"
        );
        // Nothing staged: git's complaint is the error.
        let err = s.run_line("git commit nic").unwrap_err();
        assert!(err.0.starts_with("git:"), "{err}");
    }

    #[test]
    fn diff_and_log_open_in_the_viewer_and_say_when_there_is_nothing_to_show() {
        let Some(tmp) = repository() else { return };
        let dir = tmp.path();
        let mut s = Session::new(dir, dir);
        s.run_line("select kept.txt").unwrap();
        s.run_line("git diff").unwrap();
        assert!(
            s.viewer.is_none() && s.status.contains("brak zmian"),
            "{}",
            s.status
        );
        std::fs::write(dir.join("kept.txt"), "2\n").unwrap();
        s.run_line("git diff").unwrap();
        assert!(s.viewer.is_some(), "{}", s.status);
        s.viewer = None;
        s.run_line("git log").unwrap();
        let viewer = s.viewer.as_ref().expect("the history opens in the viewer");
        assert!(viewer.path().to_string_lossy().contains("historia"));
        s.remove_preview_dir();
    }

    #[test]
    fn git_actions_refuse_outside_a_disk_folder_and_parse_strictly() {
        assert_eq!(parse_git("add").unwrap(), GitAction::Add);
        assert_eq!(parse_git("Unstage").unwrap(), GitAction::Unstage);
        assert_eq!(
            parse_git("commit \"Zażółć gęślą\"").unwrap(),
            GitAction::Commit("Zażółć gęślą".into())
        );
        assert_eq!(
            parse_git("commit dwa słowa").unwrap(),
            GitAction::Commit("dwa słowa".into())
        );
        for bad in ["", "commit", "commit   ", "add x", "push", "pull"] {
            assert!(parse_git(bad).is_err(), "{bad:?}");
        }
        let plain = tempfile::tempdir().unwrap();
        std::fs::write(plain.path().join("a.txt"), "x").unwrap();
        let mut s = Session::new(plain.path(), plain.path());
        s.run_line("select a.txt").unwrap();
        let err = s.run_line("git add").unwrap_err();
        assert!(err.0.starts_with("git:"), "not a repository: {err}");
    }
}
