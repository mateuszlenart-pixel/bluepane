//! SHA-256 sums of files, computed in the background on request (`hash`). Reading every file
//! of a folder just to fill a column would be a scan nobody asked for, so the column only shows
//! sums that were asked for; a sum is forgotten when the file's size or date changes.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use crate::panel::is_parent_entry;
use crate::session::Session;
use crate::EngineError;

/// What `hash` can be told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAction {
    /// Count the sums of the marked files (or the one under the cursor).
    Run,
    /// Stop counting.
    Stop,
    /// Put `sum  name` lines of the marked files that have a sum on the clipboard.
    Copy,
}

type Stamp = (u64, Option<SystemTime>);

/// Sums found so far and the work in progress. Shared with the counting thread.
#[derive(Debug, Clone, Default)]
pub struct Hashes {
    sums: Arc<Mutex<HashMap<PathBuf, (Stamp, String)>>>,
    running: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
    version: Arc<AtomicU64>,
}

impl Hashes {
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed) > 0
    }

    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }

    /// The sum of the file as it is now, if it was counted and the file has not changed since.
    pub fn get(&self, path: &Path, size: u64, modified: Option<SystemTime>) -> Option<String> {
        let sums = self.sums.lock().ok()?;
        let (stamp, sum) = sums.get(path)?;
        (*stamp == (size, modified)).then(|| sum.clone())
    }

    pub(crate) fn stop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::default();
    }

    fn start(&self, files: Vec<(PathBuf, Stamp)>) {
        let (sums, running, cancel, version) = (
            self.sums.clone(),
            self.running.clone(),
            self.cancel.clone(),
            self.version.clone(),
        );
        running.fetch_add(1, Ordering::Relaxed);
        std::thread::spawn(move || {
            for (path, stamp) in files {
                match sha256_of(&path, &cancel) {
                    Ok(Some(sum)) => {
                        if let Ok(mut map) = sums.lock() {
                            map.insert(path, (stamp, sum));
                        }
                        version.fetch_add(1, Ordering::Relaxed);
                    }
                    // Cancelled: stop. Unreadable: leave this one without a sum.
                    Ok(None) => break,
                    Err(_) => continue,
                }
            }
            running.fetch_sub(1, Ordering::Relaxed);
        });
    }
}

/// The SHA-256 of a file as 64 lowercase hex digits; `None` when `cancel` was set meanwhile.
pub fn sha256_of(path: &Path, cancel: &AtomicBool) -> std::io::Result<Option<String>> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(Some(
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    ))
}

impl Session {
    pub(crate) fn hash_action(&mut self, action: HashAction) -> Result<(), EngineError> {
        match action {
            HashAction::Stop => {
                self.hashes.stop();
                self.status = "Liczenie sum przerwane".into();
                Ok(())
            }
            HashAction::Run => {
                let panel = self.active_panel();
                if panel.archive.is_some() || panel.remote.is_some() {
                    return Err(EngineError(
                        "sumy SHA-256 liczy się tylko dla plików na dysku".into(),
                    ));
                }
                let files: Vec<(PathBuf, Stamp)> = panel
                    .target_entries()
                    .into_iter()
                    .filter(|e| !e.is_dir() && !is_parent_entry(e))
                    .map(|e| (e.path.clone(), (e.size, e.modified)))
                    .collect();
                if files.is_empty() {
                    return Err(EngineError(
                        "wskaż plik albo zaznacz pliki, których sumę policzyć".into(),
                    ));
                }
                self.status = format!(
                    "Liczę SHA-256: {} (hash stop przerywa; kolumna: columns sha256)",
                    files.len()
                );
                self.hashes.start(files);
                Ok(())
            }
            HashAction::Copy => {
                let panel = self.active_panel();
                let lines: Vec<String> = panel
                    .target_entries()
                    .into_iter()
                    .filter_map(|e| {
                        self.hashes
                            .get(&e.path, e.size, e.modified)
                            .map(|sum| format!("{sum}  {}", e.name))
                    })
                    .collect();
                if lines.is_empty() {
                    return Err(EngineError(
                        "te pliki nie mają jeszcze sumy (najpierw: hash)".into(),
                    ));
                }
                self.status = format!("Skopiowano sumy SHA-256: {}", lines.len());
                self.put_on_clipboard(lines.join("\n"));
                Ok(())
            }
        }
    }

    /// The sum shown in the SHA-256 column: the first 16 digits, or nothing if not counted.
    pub fn hash_cell(&self, entry: &bluepane_vfs::Entry) -> String {
        if entry.is_dir() {
            return String::new();
        }
        self.hashes
            .get(&entry.path, entry.size, entry.modified)
            .map(|sum| sum[..16].to_string())
            .unwrap_or_default()
    }
}

pub(crate) fn parse_hash(rest: &str) -> Result<HashAction, EngineError> {
    match rest.trim() {
        "" | "run" => Ok(HashAction::Run),
        "stop" => Ok(HashAction::Stop),
        "copy" => Ok(HashAction::Copy),
        _ => Err(EngineError("hash: użycie: hash [stop|copy]".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait(s: &Session) {
        let start = std::time::Instant::now();
        while s.hashes.is_running() {
            assert!(start.elapsed().as_secs() < 20, "hashing did not finish");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    fn session() -> (tempfile::TempDir, Session) {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("abc.txt"), "abc").unwrap();
        std::fs::write(tmp.path().join("Zażółć gęślą.txt"), "").unwrap();
        std::fs::create_dir(tmp.path().join("dir")).unwrap();
        let session = Session::new(tmp.path(), tmp.path());
        (tmp, session)
    }

    #[test]
    fn known_sums_are_computed_for_marked_files_only_when_asked() {
        let (tmp, mut s) = session();
        let entry = |s: &Session, name: &str| {
            s.panels[0]
                .entries
                .iter()
                .find(|e| e.name == name)
                .unwrap()
                .clone()
        };
        assert_eq!(
            s.hash_cell(&entry(&s, "abc.txt")),
            "",
            "nothing is counted unasked"
        );
        s.run_line("select abc.txt").unwrap();
        s.run_line("hash").unwrap();
        wait(&s);
        assert_eq!(s.hash_cell(&entry(&s, "abc.txt")), "ba7816bf8f01cfea");
        assert_eq!(s.hash_cell(&entry(&s, "Zażółć gęślą.txt")), "");
        // The empty file, once asked, has the well-known sum.
        s.run_line("unselectall").unwrap();
        s.run_line("selmask Zażółć*").unwrap();
        s.run_line("hash").unwrap();
        wait(&s);
        assert_eq!(
            s.hash_cell(&entry(&s, "Zażółć gęślą.txt")),
            "e3b0c44298fc1c14"
        );
        let _ = tmp;
    }

    #[test]
    fn a_changed_file_loses_its_sum_and_copy_puts_full_lines_on_the_clipboard() {
        let (tmp, mut s) = session();
        s.run_line("select abc.txt").unwrap();
        s.run_line("hash").unwrap();
        wait(&s);
        s.run_line("hash copy").unwrap();
        let text = s.take_clipboard().unwrap();
        assert_eq!(
            text,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  abc.txt"
        );
        std::fs::write(tmp.path().join("abc.txt"), "abcd").unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("select abc.txt").unwrap();
        assert!(
            s.run_line("hash copy").is_err(),
            "the old sum no longer applies"
        );
    }

    #[test]
    fn folders_and_nothing_to_hash_are_clear_errors_and_stop_is_harmless() {
        let (_tmp, mut s) = session();
        s.run_line("select dir").unwrap();
        assert!(s.run_line("hash").is_err());
        assert!(s.run_line("hash copy").is_err());
        assert!(s.run_line("hash wrong").is_err());
        s.run_line("hash stop").unwrap();
    }

    #[test]
    fn cancelling_stops_a_long_read() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("big");
        std::fs::write(&path, vec![1u8; 4 << 20]).unwrap();
        let cancel = AtomicBool::new(true);
        assert_eq!(sha256_of(&path, &cancel).unwrap(), None);
        assert!(sha256_of(&tmp.path().join("missing"), &AtomicBool::new(false)).is_err());
    }
}
