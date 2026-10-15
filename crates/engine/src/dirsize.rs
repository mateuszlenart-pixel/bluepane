//! Sizes of folders, counted in the background on request (`dirsize`), and the one-line
//! summary under a panel. A listing alone knows only file sizes; counting a folder means
//! walking everything below it, so it is never done unasked.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::format::human_size;
use crate::panel::is_parent_entry;
use crate::session::Session;
use crate::EngineError;

/// Folder sizes found so far and the work in progress. Shared with the counting thread.
#[derive(Debug, Clone, Default)]
pub struct DirSizes {
    sizes: Arc<Mutex<HashMap<PathBuf, u64>>>,
    running: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
}

impl DirSizes {
    pub fn get(&self, path: &Path) -> Option<u64> {
        self.sizes.lock().ok()?.get(path).copied()
    }

    /// True while a counting thread is still walking folders.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed) > 0
    }

    /// Forgets what was counted (the files may have changed) and stops any walk.
    pub(crate) fn clear(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        // A fresh flag for the next walk; the old thread still sees its own.
        self.cancel = Arc::default();
        if let Ok(mut sizes) = self.sizes.lock() {
            sizes.clear();
        }
    }

    fn start(&self, folders: Vec<PathBuf>) {
        let (sizes, running, cancel) = (
            self.sizes.clone(),
            self.running.clone(),
            self.cancel.clone(),
        );
        running.fetch_add(1, Ordering::Relaxed);
        std::thread::spawn(move || {
            for folder in folders {
                match walk(&folder, &cancel) {
                    Some(total) => {
                        if let Ok(mut map) = sizes.lock() {
                            map.insert(folder, total);
                        }
                    }
                    None => break,
                }
            }
            running.fetch_sub(1, Ordering::Relaxed);
        });
    }
}

/// Total size of the files below `root`; links are not followed. `None` when cancelled.
fn walk(root: &Path, cancel: &AtomicBool) -> Option<u64> {
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        // A folder that cannot be read (no permission) counts as empty.
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                stack.push(entry.path());
            } else if kind.is_file() {
                total += entry.metadata().map_or(0, |m| m.len());
            }
        }
    }
    Some(total)
}

/// "1 plik", "2 pliki", "5 plików".
fn plural(n: usize, one: &str, few: &str, many: &str) -> String {
    let word = match (n, n % 10, n % 100) {
        (1, _, _) => one,
        (_, 2..=4, r) if !(12..=14).contains(&r) => few,
        _ => many,
    };
    format!("{n} {word}")
}

impl Session {
    /// `dirsize`: counts the folders among the selected entries (or under the cursor).
    pub(crate) fn count_dir_sizes(&mut self) -> Result<(), EngineError> {
        let panel = self.active_panel();
        if panel.archive.is_some() || panel.remote.is_some() {
            return Err(EngineError(
                "rozmiary katalogów liczy się tylko w panelu lokalnym".into(),
            ));
        }
        let folders: Vec<PathBuf> = panel
            .target_entries()
            .into_iter()
            .filter(|e| e.is_dir() && !is_parent_entry(e))
            .map(|e| e.path.clone())
            .collect();
        if folders.is_empty() {
            return Err(EngineError(
                "wskaż katalog albo zaznacz katalogi, których rozmiar policzyć".into(),
            ));
        }
        self.status = format!(
            "Liczę rozmiar: {}…",
            plural(folders.len(), "katalog", "katalogi", "katalogów")
        );
        self.dir_sizes.start(folders);
        Ok(())
    }

    /// The size of an entry for the size column and the bars: a file's own size, or a
    /// folder's when it has been counted.
    pub fn entry_size(&self, entry: &bluepane_vfs::Entry) -> Option<u64> {
        if entry.is_dir() {
            self.dir_sizes.get(&entry.path)
        } else {
            Some(entry.size)
        }
    }

    /// The line under a panel: how much is marked, or what the folder holds.
    pub fn panel_summary(&self, side: usize) -> String {
        let panel = &self.panels[side];
        let real: Vec<&bluepane_vfs::Entry> = panel
            .entries
            .iter()
            .filter(|e| !is_parent_entry(e))
            .collect();
        let describe = |list: &[&bluepane_vfs::Entry]| -> (usize, usize, u64) {
            let dirs = list.iter().filter(|e| e.is_dir()).count();
            let bytes: u64 = list.iter().filter_map(|e| self.entry_size(e)).sum();
            (list.len() - dirs, dirs, bytes)
        };
        let marked: Vec<&bluepane_vfs::Entry> = real
            .iter()
            .copied()
            .filter(|e| panel.selected.contains(&e.path))
            .collect();
        let (files, dirs, bytes) = describe(&marked);
        if !marked.is_empty() {
            let mut parts = Vec::new();
            if files > 0 {
                parts.push(plural(files, "plik", "pliki", "plików"));
            }
            if dirs > 0 {
                parts.push(plural(dirs, "katalog", "katalogi", "katalogów"));
            }
            return format!(
                "Zaznaczono: {} ({}) z {}",
                parts.join(", "),
                human_size(bytes),
                plural(real.len(), "elementu", "elementów", "elementów")
            );
        }
        let (files, dirs, bytes) = describe(&real);
        format!(
            "{}: {}, {} ({})",
            plural(real.len(), "element", "elementy", "elementów"),
            plural(dirs, "katalog", "katalogi", "katalogów"),
            plural(files, "plik", "pliki", "plików"),
            human_size(bytes)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn write(path: &Path, bytes: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![b'x'; bytes]).unwrap();
    }

    fn session() -> (tempfile::TempDir, Session) {
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left");
        write(&left.join("a.txt"), 100);
        write(&left.join("big/inner/b.bin"), 3000);
        write(&left.join("big/c.bin"), 1000);
        write(&left.join("small/d.txt"), 10);
        std::fs::create_dir_all(left.join("empty")).unwrap();
        let s = Session::new(&left, tmp.path().join("right"));
        (tmp, s)
    }

    fn wait(s: &Session) {
        for _ in 0..1000 {
            if !s.dir_sizes.is_running() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the count did not finish");
    }

    #[test]
    fn plurals_follow_polish_rules() {
        assert_eq!(plural(1, "plik", "pliki", "plików"), "1 plik");
        assert_eq!(plural(2, "plik", "pliki", "plików"), "2 pliki");
        assert_eq!(plural(5, "plik", "pliki", "plików"), "5 plików");
        assert_eq!(plural(12, "plik", "pliki", "plików"), "12 plików");
        assert_eq!(plural(22, "plik", "pliki", "plików"), "22 pliki");
        assert_eq!(plural(0, "plik", "pliki", "plików"), "0 plików");
    }

    #[test]
    fn folders_are_counted_in_the_background_on_request_only() {
        let (tmp, mut s) = session();
        let big = tmp.path().join("left/big");
        let entry = |s: &Session, name: &str| {
            s.panels[0]
                .entries
                .iter()
                .find(|e| e.name == name)
                .unwrap()
                .clone()
        };
        assert_eq!(s.entry_size(&entry(&s, "big")), None, "not counted unasked");
        assert_eq!(s.entry_size(&entry(&s, "a.txt")), Some(100));
        s.run_line("select big").unwrap();
        s.run_line("select empty").unwrap();
        s.run_line("dirsize").unwrap();
        wait(&s);
        assert_eq!(s.dir_sizes.get(&big), Some(4000));
        assert_eq!(s.entry_size(&entry(&s, "big")), Some(4000));
        assert_eq!(s.entry_size(&entry(&s, "empty")), Some(0));
        assert_eq!(s.entry_size(&entry(&s, "small")), None);
        // Refresh forgets the counts: the files may have changed.
        s.run_line("refresh").unwrap();
        assert_eq!(s.entry_size(&entry(&s, "big")), None);
    }

    #[test]
    fn dirsize_needs_a_folder_and_a_local_panel() {
        let (_tmp, mut s) = session();
        s.run_line("cursor 100").unwrap();
        assert!(s.panels[0].current().is_some_and(|e| !e.is_dir()));
        let err = s.run_line("dirsize").unwrap_err();
        assert!(err.0.contains("katalog"), "{err}");
    }

    #[test]
    fn the_summary_describes_the_folder_and_the_marked_files() {
        let (_tmp, mut s) = session();
        // 1 file of 100 bytes and 3 folders (counted below), `..` does not count.
        let text = s.panel_summary(0);
        assert!(text.starts_with("4 elementy: 3 katalogi, 1 plik"), "{text}");
        s.run_line("select a.txt").unwrap();
        s.run_line("select big").unwrap();
        s.run_line("dirsize").unwrap();
        wait(&s);
        let text = s.panel_summary(0);
        assert!(text.starts_with("Zaznaczono: 1 plik, 1 katalog"), "{text}");
        assert!(text.contains("(4.0 KB)") || text.contains("KB"), "{text}");
        assert!(text.ends_with("z 4 elementów"), "{text}");
    }
}
