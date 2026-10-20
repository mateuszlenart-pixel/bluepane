//! Duplicate finder (`dupes`): files of the active folder (and everything below it) that have
//! the same content. Candidates are narrowed in three steps, each one cheaper than the next:
//! equal size, equal SHA-256, equal bytes (the last step makes a hash collision harmless).
//! The scan runs only after the command, in the background, and can be stopped. Copies are
//! marked by hand or by a rule (`keep newest`) and go to the trash, never all copies of a file.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use crate::session::Session;
use crate::EngineError;

/// What `dupes` asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DupAction {
    /// Scan the folder of the active panel.
    Open,
    Close,
    /// Stop a scan that is still running (what was found stays).
    Stop,
    /// Mark every copy except one chosen by the rule, in every group.
    Keep(KeepRule),
    /// Remove all marks.
    Clear,
    /// Toggle the mark of file `file` of group `group` (both counted from 1).
    Mark(usize, usize),
    /// Move the marked copies to the trash.
    Trash,
}

/// Which copy of a group stays when the rest is marked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeepRule {
    Newest,
    Oldest,
    /// The one with the shortest path (closest to the top).
    Shortest,
    /// The first by path, alphabetically.
    First,
}

/// One copy of a duplicated file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DupFile {
    pub path: PathBuf,
    pub modified: Option<SystemTime>,
}

/// Files with identical content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DupGroup {
    pub size: u64,
    pub files: Vec<DupFile>,
}

impl DupGroup {
    /// Bytes that would be freed by keeping one copy.
    pub fn wasted(&self) -> u64 {
        self.size * (self.files.len() as u64).saturating_sub(1)
    }
}

const PHASE_LISTING: u8 = 0;
const PHASE_HASHING: u8 = 1;
const PHASE_DONE: u8 = 2;

/// An open duplicate finder. The scanning thread fills `groups`.
#[derive(Debug)]
pub struct Duplicates {
    pub root: PathBuf,
    groups: Arc<Mutex<Vec<DupGroup>>>,
    marks: HashSet<PathBuf>,
    phase: Arc<AtomicU8>,
    cancel: Arc<AtomicBool>,
    version: Arc<AtomicU64>,
    /// Files listed so far, then candidates still to be read.
    progress: Arc<AtomicU64>,
}

impl Duplicates {
    fn start(root: PathBuf) -> Duplicates {
        let found = Duplicates {
            root: root.clone(),
            groups: Arc::default(),
            marks: HashSet::new(),
            phase: Arc::new(AtomicU8::new(PHASE_LISTING)),
            cancel: Arc::default(),
            version: Arc::default(),
            progress: Arc::default(),
        };
        let (groups, phase, cancel, version, progress) = (
            found.groups.clone(),
            found.phase.clone(),
            found.cancel.clone(),
            found.version.clone(),
            found.progress.clone(),
        );
        std::thread::spawn(move || {
            scan(&root, &groups, &phase, &cancel, &version, &progress);
            phase.store(PHASE_DONE, Ordering::Relaxed);
            version.fetch_add(1, Ordering::Relaxed);
        });
        found
    }

    /// Changes whenever a group is found or the scan ends.
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }

    pub fn is_running(&self) -> bool {
        self.phase.load(Ordering::Relaxed) != PHASE_DONE
    }

    pub fn is_marked(&self, path: &Path) -> bool {
        self.marks.contains(path)
    }

    fn snapshot(&self) -> Vec<DupGroup> {
        self.groups.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

impl Drop for Duplicates {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// The scan itself: list, group by size, hash, confirm byte by byte. Symbolic links and
/// empty files are skipped (a link is not a second copy, and empty files are all "equal").
fn scan(
    root: &Path,
    groups: &Mutex<Vec<DupGroup>>,
    phase: &AtomicU8,
    cancel: &AtomicBool,
    version: &AtomicU64,
    progress: &AtomicU64,
) {
    let mut by_size: HashMap<u64, Vec<DupFile>> = HashMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                stack.push(entry.path());
            } else if kind.is_file() {
                let Ok(meta) = entry.metadata() else { continue };
                if meta.len() > 0 {
                    by_size.entry(meta.len()).or_default().push(DupFile {
                        path: entry.path(),
                        modified: meta.modified().ok(),
                    });
                    progress.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
    by_size.retain(|_, files| files.len() > 1);
    progress.store(
        by_size.values().map(|f| f.len() as u64).sum(),
        Ordering::Relaxed,
    );
    phase.store(PHASE_HASHING, Ordering::Relaxed);

    let mut sizes: Vec<u64> = by_size.keys().copied().collect();
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    for size in sizes {
        let files = by_size.remove(&size).unwrap_or_default();
        let mut by_hash: HashMap<String, Vec<DupFile>> = HashMap::new();
        for file in files {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let sum = sha256_of(&file.path, cancel);
            progress.fetch_sub(1, Ordering::Relaxed);
            match sum {
                Some(Some(sum)) => by_hash.entry(sum).or_default().push(file),
                // Cancelled.
                Some(None) => return,
                // Unreadable: it cannot be confirmed as a copy.
                None => {}
            }
        }
        for candidates in by_hash.into_values().filter(|f| f.len() > 1) {
            for mut group in confirm(candidates, cancel) {
                group.sort_by(|a, b| a.path.cmp(&b.path));
                if let Ok(mut shared) = groups.lock() {
                    shared.push(DupGroup { size, files: group });
                }
                version.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    if let Ok(mut shared) = groups.lock() {
        // The biggest waste first; ties by the first path so the order is stable.
        shared.sort_by(|a, b| {
            b.wasted()
                .cmp(&a.wasted())
                .then_with(|| a.files[0].path.cmp(&b.files[0].path))
        });
    }
}

/// Splits files with the same hash into sets whose bytes really are equal; sets of one file
/// are dropped.
fn confirm(files: Vec<DupFile>, cancel: &AtomicBool) -> Vec<Vec<DupFile>> {
    let mut sets: Vec<Vec<DupFile>> = Vec::new();
    for file in files {
        let mut placed = false;
        for set in &mut sets {
            if same_bytes(&set[0].path, &file.path, cancel) {
                set.push(file.clone());
                placed = true;
                break;
            }
        }
        if !placed {
            sets.push(vec![file]);
        }
    }
    sets.retain(|s| s.len() > 1);
    sets
}

/// `Some(Some(sum))` for a sum, `Some(None)` when cancelled, `None` when unreadable.
fn sha256_of(path: &Path, cancel: &AtomicBool) -> Option<Option<String>> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Some(None);
        }
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(Some(
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    ))
}

/// True when both files can be read and have exactly the same bytes.
fn same_bytes(a: &Path, b: &Path, cancel: &AtomicBool) -> bool {
    let (Ok(mut fa), Ok(mut fb)) = (std::fs::File::open(a), std::fs::File::open(b)) else {
        return false;
    };
    let (mut ba, mut bb) = (vec![0u8; 1 << 16], vec![0u8; 1 << 16]);
    loop {
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        let Ok(na) = read_full(&mut fa, &mut ba) else {
            return false;
        };
        let Ok(nb) = read_full(&mut fb, &mut bb) else {
            return false;
        };
        if na != nb || ba[..na] != bb[..nb] {
            return false;
        }
        if na == 0 {
            return true;
        }
    }
}

/// Reads until the buffer is full or the file ends, so two readers stay in step.
fn read_full(file: &mut std::fs::File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = file.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    Ok(filled)
}

/// What the tool shows above the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DupSummary {
    pub groups: usize,
    /// Bytes freed by keeping one copy of every group.
    pub wasted: u64,
    pub marked: usize,
    /// Bytes freed by removing the marked copies.
    pub marked_bytes: u64,
    pub running: bool,
    /// Files still to be read while hashing, or listed so far while listing.
    pub progress: u64,
    pub listing: bool,
}

impl Session {
    /// The groups found so far, the biggest waste first once the scan is done.
    pub fn dup_groups(&self) -> Vec<DupGroup> {
        self.duplicates
            .as_ref()
            .map(|d| d.snapshot())
            .unwrap_or_default()
    }

    pub fn dup_summary(&self) -> Option<DupSummary> {
        let found = self.duplicates.as_ref()?;
        let groups = found.snapshot();
        let (mut marked, mut marked_bytes) = (0, 0);
        for group in &groups {
            for file in &group.files {
                if found.marks.contains(&file.path) {
                    marked += 1;
                    marked_bytes += group.size;
                }
            }
        }
        Some(DupSummary {
            groups: groups.len(),
            wasted: groups.iter().map(DupGroup::wasted).sum(),
            marked,
            marked_bytes,
            running: found.is_running(),
            progress: found.progress.load(Ordering::Relaxed),
            listing: found.phase.load(Ordering::Relaxed) == PHASE_LISTING,
        })
    }

    pub(crate) fn dup_action(&mut self, action: DupAction) -> Result<(), EngineError> {
        match action {
            DupAction::Open => {
                let panel = self.active_panel();
                if panel.archive.is_some() || panel.remote.is_some() {
                    return Err(EngineError(
                        "szukanie duplikatów działa tylko na katalogu na dysku".into(),
                    ));
                }
                let root = panel.location();
                self.status = format!("Duplikaty: szukam w {}…", root.display());
                self.duplicates = Some(Duplicates::start(root));
            }
            DupAction::Close => self.duplicates = None,
            DupAction::Stop => {
                let found = self.dup_open()?;
                found.cancel.store(true, Ordering::Relaxed);
                found.phase.store(PHASE_DONE, Ordering::Relaxed);
                self.status = "Duplikaty: przerwano, zostają znalezione grupy".into();
            }
            DupAction::Keep(rule) => {
                let found = self.dup_open_mut()?;
                let groups = found.snapshot();
                found.marks.clear();
                for group in &groups {
                    let keep = keeper(group, rule);
                    for (i, file) in group.files.iter().enumerate() {
                        if i != keep {
                            found.marks.insert(file.path.clone());
                        }
                    }
                }
                let marked = found.marks.len();
                self.status = format!("Duplikaty: zaznaczono {marked} kopii do usunięcia");
            }
            DupAction::Clear => {
                self.dup_open_mut()?.marks.clear();
                self.status = "Duplikaty: odznaczono wszystko".into();
            }
            DupAction::Mark(group, file) => {
                let found = self.dup_open_mut()?;
                let path = found
                    .snapshot()
                    .get(group.wrapping_sub(1))
                    .and_then(|g| g.files.get(file.wrapping_sub(1)))
                    .map(|f| f.path.clone())
                    .ok_or_else(|| EngineError(format!("brak pliku {file} w grupie {group}")))?;
                if !found.marks.remove(&path) {
                    found.marks.insert(path);
                }
            }
            DupAction::Trash => self.dup_trash()?,
        }
        Ok(())
    }

    fn dup_open(&self) -> Result<&Duplicates, EngineError> {
        self.duplicates
            .as_ref()
            .ok_or_else(|| EngineError("duplikaty nie są otwarte (komenda dupes)".into()))
    }

    fn dup_open_mut(&mut self) -> Result<&mut Duplicates, EngineError> {
        self.duplicates
            .as_mut()
            .ok_or_else(|| EngineError("duplikaty nie są otwarte (komenda dupes)".into()))
    }

    fn dup_trash(&mut self) -> Result<(), EngineError> {
        let found = self.dup_open()?;
        let groups = found.snapshot();
        // Never remove the last copy: refuse when a group is marked entirely.
        for (i, group) in groups.iter().enumerate() {
            if group.files.iter().all(|f| found.marks.contains(&f.path)) {
                return Err(EngineError(format!(
                    "w grupie {} zaznaczono wszystkie kopie; jedna musi zostać",
                    i + 1
                )));
            }
        }
        let marked: Vec<PathBuf> = groups
            .iter()
            .flat_map(|g| g.files.iter())
            .filter(|f| found.marks.contains(&f.path))
            .map(|f| f.path.clone())
            .collect();
        if marked.is_empty() {
            return Err(EngineError(
                "nic nie zaznaczono (dupes keep newest|oldest|shortest|first)".into(),
            ));
        }
        let mut failed = Vec::new();
        let mut gone = HashSet::new();
        for path in marked {
            match bluepane_ops::delete_to_trash(&path) {
                Ok(()) => {
                    gone.insert(path);
                }
                Err(e) => failed.push(format!("{}: {e}", path.display())),
            }
        }
        let found = self.dup_open_mut()?;
        if let Ok(mut shared) = found.groups.lock() {
            for group in shared.iter_mut() {
                group.files.retain(|f| !gone.contains(&f.path));
            }
            shared.retain(|g| g.files.len() > 1);
        }
        found.marks.retain(|p| !gone.contains(p));
        found.version.fetch_add(1, Ordering::Relaxed);
        self.panels.iter_mut().for_each(crate::Panel::reload);
        self.status = format!("Duplikaty: do kosza przeniesiono {}", gone.len());
        if failed.is_empty() {
            Ok(())
        } else {
            Err(EngineError(format!(
                "nie udało się usunąć {}: {}",
                failed.len(),
                failed.join("; ")
            )))
        }
    }
}

/// Index of the copy that stays.
fn keeper(group: &DupGroup, rule: KeepRule) -> usize {
    let indices = 0..group.files.len();
    let pick = match rule {
        KeepRule::Newest => {
            indices.max_by_key(|&i| (group.files[i].modified, std::cmp::Reverse(i)))
        }
        KeepRule::Oldest => indices.min_by_key(|&i| (group.files[i].modified, i)),
        KeepRule::Shortest => indices.min_by_key(|&i| {
            let p = &group.files[i].path;
            (p.as_os_str().len(), p.clone())
        }),
        KeepRule::First => indices.min_by_key(|&i| group.files[i].path.clone()),
    };
    pick.unwrap_or(0)
}

/// `dupes [close|stop|keep newest|oldest|shortest|first|clear|mark <grupa> <plik>|trash]`
pub(crate) fn parse_dupes(rest: &str) -> Result<DupAction, EngineError> {
    let rest = rest.trim();
    let usage = "dupes: użycie: dupes [close|stop|keep newest|oldest|shortest|first|clear|mark <grupa> <plik>|trash]";
    let mut words = rest.split_whitespace();
    let verb = words.next().unwrap_or("").to_lowercase();
    let arg = words.next().map(str::to_lowercase);
    let arg2 = words.next();
    Ok(match (verb.as_str(), arg.as_deref(), arg2) {
        ("" | "open", None, _) => DupAction::Open,
        ("close", None, _) => DupAction::Close,
        ("stop", None, _) => DupAction::Stop,
        ("clear", None, _) => DupAction::Clear,
        ("trash", None, _) => DupAction::Trash,
        ("keep", Some("newest"), _) => DupAction::Keep(KeepRule::Newest),
        ("keep", Some("oldest"), _) => DupAction::Keep(KeepRule::Oldest),
        ("keep", Some("shortest"), _) => DupAction::Keep(KeepRule::Shortest),
        ("keep", Some("first"), _) => DupAction::Keep(KeepRule::First),
        ("mark", Some(g), Some(f)) => match (g.parse(), f.parse()) {
            (Ok(g), Ok(f)) => DupAction::Mark(g, f),
            _ => return Err(EngineError(usage.into())),
        },
        _ => return Err(EngineError(usage.into())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, Instant};

    fn session() -> (tempfile::TempDir, Session) {
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left");
        let right = tmp.path().join("right");
        fs::create_dir_all(left.join("podkatalog")).unwrap();
        fs::create_dir_all(&right).unwrap();
        let s = Session::new(&left, &right);
        (tmp, s)
    }

    fn wait(s: &Session) {
        let start = Instant::now();
        while s.duplicates.as_ref().is_some_and(Duplicates::is_running) {
            assert!(start.elapsed() < Duration::from_secs(20), "scan hangs");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn names(group: &DupGroup, root: &Path) -> Vec<String> {
        group
            .files
            .iter()
            .map(|f| {
                f.path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    }

    #[test]
    fn equal_content_is_grouped_and_same_size_with_other_bytes_is_not() {
        let (tmp, mut s) = session();
        let left = tmp.path().join("left");
        fs::write(left.join("zażółć.txt"), "gęślą jaźń").unwrap();
        fs::write(left.join("podkatalog/kopia.txt"), "gęślą jaźń").unwrap();
        // Same length, different bytes: must not be a duplicate.
        fs::write(left.join("inny.txt"), "GĘŚLĄ JAŹŃ").unwrap();
        fs::write(left.join("pusty1"), "").unwrap();
        fs::write(left.join("pusty2"), "").unwrap();
        fs::write(left.join("sam.bin"), "unikat").unwrap();
        s.run_line("dupes").unwrap();
        wait(&s);
        let groups = s.dup_groups();
        assert_eq!(groups.len(), 1, "{groups:?}");
        assert_eq!(
            names(&groups[0], &left),
            ["podkatalog/kopia.txt", "zażółć.txt"]
        );
        let sum = s.dup_summary().unwrap();
        assert_eq!(
            (sum.groups, sum.wasted, sum.running),
            (1, "gęślą jaźń".len() as u64, false)
        );
    }

    #[test]
    fn keep_rules_pick_the_copy_that_stays_and_trash_takes_the_rest() {
        let (tmp, mut s) = session();
        let left = tmp.path().join("left");
        for name in ["a.txt", "b.txt", "c.txt"] {
            fs::write(left.join(name), "to samo").unwrap();
        }
        let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let new = SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000);
        fs::File::options()
            .write(true)
            .open(left.join("b.txt"))
            .unwrap()
            .set_modified(new)
            .unwrap();
        for name in ["a.txt", "c.txt"] {
            fs::File::options()
                .write(true)
                .open(left.join(name))
                .unwrap()
                .set_modified(old)
                .unwrap();
        }
        s.run_line("dupes").unwrap();
        wait(&s);
        s.run_line("dupes keep newest").unwrap();
        let found = s.duplicates.as_ref().unwrap();
        assert!(!found.is_marked(&left.join("b.txt")));
        assert!(found.is_marked(&left.join("a.txt")) && found.is_marked(&left.join("c.txt")));
        s.run_line("dupes keep oldest").unwrap();
        let found = s.duplicates.as_ref().unwrap();
        // Ties among the oldest go to the first path.
        assert!(!found.is_marked(&left.join("a.txt")));
        assert!(found.is_marked(&left.join("b.txt")) && found.is_marked(&left.join("c.txt")));
        s.run_line("dupes keep first").unwrap();
        assert_eq!(s.dup_summary().unwrap().marked, 2);

        s.run_line("dupes trash").unwrap();
        assert!(left.join("a.txt").exists());
        assert!(!left.join("b.txt").exists() && !left.join("c.txt").exists());
        assert!(s.dup_groups().is_empty(), "a lone file is no group");
    }

    #[test]
    fn the_last_copy_is_never_trashed_and_marking_toggles() {
        let (tmp, mut s) = session();
        let left = tmp.path().join("left");
        fs::write(left.join("a.txt"), "dwa").unwrap();
        fs::write(left.join("b.txt"), "dwa").unwrap();
        s.run_line("dupes").unwrap();
        wait(&s);
        assert!(s.run_line("dupes trash").is_err(), "nothing marked");
        s.run_line("dupes mark 1 1").unwrap();
        s.run_line("dupes mark 1 2").unwrap();
        let err = s.run_line("dupes trash").unwrap_err();
        assert!(err.to_string().contains("jedna musi zostać"), "{err}");
        assert!(left.join("a.txt").exists() && left.join("b.txt").exists());
        s.run_line("dupes mark 1 2").unwrap();
        assert_eq!(s.dup_summary().unwrap().marked, 1);
        s.run_line("dupes clear").unwrap();
        assert_eq!(s.dup_summary().unwrap().marked, 0);
        assert!(s.run_line("dupes mark 5 1").is_err());
    }

    #[test]
    fn parsing_and_closing() {
        assert_eq!(parse_dupes("").unwrap(), DupAction::Open);
        assert_eq!(
            parse_dupes("keep Newest").unwrap(),
            DupAction::Keep(KeepRule::Newest)
        );
        assert_eq!(parse_dupes("mark 2 3").unwrap(), DupAction::Mark(2, 3));
        assert!(parse_dupes("keep random").is_err());
        assert!(parse_dupes("mark x y").is_err());
        let (_tmp, mut s) = session();
        assert!(s.run_line("dupes keep newest").is_err(), "not open");
        s.run_line("dupes").unwrap();
        s.run_line("dupes close").unwrap();
        assert!(s.duplicates.is_none());
    }

    #[test]
    fn same_bytes_tells_a_difference_far_into_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let mut a = vec![7u8; 200_000];
        let b = a.clone();
        fs::write(tmp.path().join("a"), &a).unwrap();
        fs::write(tmp.path().join("b"), &b).unwrap();
        let cancel = AtomicBool::new(false);
        assert!(same_bytes(
            &tmp.path().join("a"),
            &tmp.path().join("b"),
            &cancel
        ));
        a[150_000] = 8;
        fs::write(tmp.path().join("a"), &a).unwrap();
        assert!(!same_bytes(
            &tmp.path().join("a"),
            &tmp.path().join("b"),
            &cancel
        ));
    }
}
