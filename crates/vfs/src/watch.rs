//! Watching directories for changes made behind the program's back (another program, a
//! download, a build). The system does the watching (inotify on Linux, ReadDirectoryChangesW
//! on Windows, through the `notify` crate); this only collects which watched directories
//! changed since the last look.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

#[derive(Default)]
struct Shared {
    /// The directories being watched.
    watched: Vec<PathBuf>,
    /// Watched directories where something changed since the last look.
    dirty: HashSet<PathBuf>,
}

/// Watches a few directories (not their subdirectories) and says which of them changed.
pub struct DirWatcher {
    watcher: Option<RecommendedWatcher>,
    shared: Arc<Mutex<Shared>>,
    error: Option<String>,
}

impl std::fmt::Debug for DirWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirWatcher")
            .field("active", &self.watcher.is_some())
            .finish()
    }
}

impl Default for DirWatcher {
    fn default() -> Self {
        DirWatcher::new()
    }
}

/// Whether an event is a change (reading a directory also makes events; those are not).
fn is_change(kind: &EventKind) -> bool {
    !matches!(kind, EventKind::Access(_))
}

impl DirWatcher {
    /// A watcher with nothing to watch yet; when the system refuses to watch at all (for
    /// example the limit of watches is used up) it stays inactive and says why in [`error`].
    ///
    /// [`error`]: DirWatcher::error
    pub fn new() -> DirWatcher {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let sink = Arc::clone(&shared);
        let made = notify::recommended_watcher(move |event: notify::Result<Event>| {
            let Ok(event) = event else { return };
            if !is_change(&event.kind) {
                return;
            }
            let Ok(mut shared) = sink.lock() else { return };
            for path in &event.paths {
                let hit = shared
                    .watched
                    .iter()
                    .find(|dir| path.parent() == Some(dir.as_path()) || path == *dir)
                    .cloned();
                if let Some(dir) = hit {
                    shared.dirty.insert(dir);
                }
            }
        });
        match made {
            Ok(watcher) => DirWatcher {
                watcher: Some(watcher),
                shared,
                error: None,
            },
            Err(e) => DirWatcher {
                watcher: None,
                shared,
                error: Some(e.to_string()),
            },
        }
    }

    /// Whether the system is watching at all.
    pub fn is_active(&self) -> bool {
        self.watcher.is_some()
    }

    /// Why watching is not available, if it is not.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Makes these the watched directories: new ones are added, the others dropped. Ones that
    /// cannot be watched (gone, no permission) are left out quietly. Cheap when nothing changed.
    pub fn set_paths(&mut self, wanted: &[PathBuf]) {
        let Some(watcher) = self.watcher.as_mut() else {
            return;
        };
        let Ok(mut shared) = self.shared.lock() else {
            return;
        };
        let mut next: Vec<PathBuf> = Vec::new();
        for path in wanted {
            if !next.contains(path) {
                next.push(path.clone());
            }
        }
        // A directory that was wanted but could not be watched is tried again each time, so
        // it starts being watched once it exists.
        for old in shared.watched.clone() {
            if !next.contains(&old) {
                let _ = watcher.unwatch(&old);
            }
        }
        let mut watched = Vec::new();
        for path in next {
            if shared.watched.contains(&path)
                || watcher.watch(&path, RecursiveMode::NonRecursive).is_ok()
            {
                watched.push(path);
            }
        }
        shared.dirty.retain(|d| watched.contains(d));
        shared.watched = watched;
    }

    /// The watched directories that changed since the last call.
    pub fn take_changed(&self) -> Vec<PathBuf> {
        let Ok(mut shared) = self.shared.lock() else {
            return Vec::new();
        };
        shared.dirty.drain().collect()
    }

    /// Whether this directory is being watched.
    pub fn watches(&self, path: &Path) -> bool {
        self.shared
            .lock()
            .is_ok_and(|s| s.watched.iter().any(|w| w == path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Waits until `look` finds a change or time runs out.
    fn changed(watcher: &DirWatcher) -> Vec<PathBuf> {
        let end = Instant::now() + Duration::from_secs(10);
        let mut all = Vec::new();
        while Instant::now() < end {
            all.extend(watcher.take_changed());
            if !all.is_empty() {
                return all;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        all
    }

    #[test]
    fn a_new_file_in_a_watched_directory_is_reported_and_reading_is_not() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let mut watcher = DirWatcher::new();
        assert!(watcher.is_active(), "{:?}", watcher.error());
        watcher.set_paths(&[a.clone()]);
        assert!(watcher.watches(&a) && !watcher.watches(&b));

        // Listing the directory is only reading: no change.
        let _ = std::fs::read_dir(&a).unwrap().count();
        std::thread::sleep(Duration::from_millis(300));
        assert!(watcher.take_changed().is_empty());

        // Something in the other directory is not our business.
        std::fs::write(b.join("x.txt"), "x").unwrap();
        std::fs::write(a.join("zażółć gęślą.txt"), "treść").unwrap();
        assert_eq!(changed(&watcher), [a.clone()]);

        // Removing a file counts too.
        std::fs::remove_file(a.join("zażółć gęślą.txt")).unwrap();
        assert_eq!(changed(&watcher), [a]);
    }

    #[test]
    fn the_watched_set_follows_the_wanted_one_and_missing_directories_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let mut watcher = DirWatcher::new();
        let gone = tmp.path().join("nie ma");
        watcher.set_paths(&[a.clone(), gone.clone(), a.clone()]);
        assert!(watcher.watches(&a) && !watcher.watches(&gone));
        watcher.set_paths(&[b.clone()]);
        assert!(!watcher.watches(&a) && watcher.watches(&b));
        std::fs::write(a.join("x"), "x").unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(watcher.take_changed().is_empty(), "a is no longer watched");
        watcher.set_paths(&[]);
        assert!(!watcher.watches(&b));
        // Once the missing directory exists it can be watched.
        std::fs::create_dir_all(&gone).unwrap();
        watcher.set_paths(&[gone.clone()]);
        assert!(watcher.watches(&gone));
    }
}
