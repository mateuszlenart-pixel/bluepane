//! What the bar above a panel shows: the path as clickable segments (breadcrumbs) and how
//! full the drive is. Free space is asked in the background and remembered for a few
//! seconds, because a drive that does not answer (a dead network share) must not freeze
//! the window.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bluepane_vfs::DiskSpace;

use crate::session::Session;

/// One clickable part of a path: what to show and where a click goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crumb {
    pub label: String,
    pub path: PathBuf,
}

/// The segments of `path` from the root down, each with the path up to and including it.
/// A Windows drive (`C:\`) is one segment, the root `/` of Linux is one segment.
pub fn crumbs(path: &Path) -> Vec<Crumb> {
    let mut out: Vec<Crumb> = Vec::new();
    let mut so_far = PathBuf::new();
    for component in path.components() {
        so_far.push(component.as_os_str());
        match component {
            Component::Prefix(prefix) => out.push(Crumb {
                label: prefix.as_os_str().to_string_lossy().into_owned(),
                path: so_far.clone(),
            }),
            Component::RootDir => {
                if let Some(drive) = out.last_mut() {
                    // `C:` and the root after it are one place: `C:\`.
                    drive.path = so_far.clone();
                } else {
                    out.push(Crumb {
                        label: std::path::MAIN_SEPARATOR.to_string(),
                        path: so_far.clone(),
                    });
                }
            }
            Component::Normal(name) => out.push(Crumb {
                label: name.to_string_lossy().into_owned(),
                path: so_far.clone(),
            }),
            Component::CurDir | Component::ParentDir => {}
        }
    }
    out
}

/// How long a drive's free space is trusted before it is asked again.
const FRESH: Duration = Duration::from_secs(10);

/// When a drive was last asked, and what it answered (`None`: it did not say).
type Answer = (Instant, Option<DiskSpace>);

#[derive(Debug, Clone, Default)]
pub struct DiskUsage {
    known: Arc<Mutex<HashMap<PathBuf, Answer>>>,
    asking: Arc<Mutex<HashSet<PathBuf>>>,
}

impl DiskUsage {
    /// The last known answer for the drive holding `path`; starts a background question
    /// when there is none or it is old. Never waits.
    pub fn get(&self, path: &Path) -> Option<DiskSpace> {
        let cached = self.known.lock().ok()?.get(path).copied();
        let stale = cached.is_none_or(|(when, _)| when.elapsed() > FRESH);
        if stale {
            self.ask(path);
        }
        cached.and_then(|(_, space)| space)
    }

    fn ask(&self, path: &Path) {
        {
            let Ok(mut asking) = self.asking.lock() else {
                return;
            };
            if !asking.insert(path.to_path_buf()) {
                return;
            }
        }
        let (known, asking, path) = (self.known.clone(), self.asking.clone(), path.to_path_buf());
        std::thread::spawn(move || {
            let space = bluepane_vfs::disk_space(&path);
            if let Ok(mut known) = known.lock() {
                known.insert(path.clone(), (Instant::now(), space));
            }
            if let Ok(mut asking) = asking.lock() {
                asking.remove(&path);
            }
        });
    }

    /// True while an answer is awaited (the window redraws until it arrives).
    pub fn is_asking(&self) -> bool {
        self.asking.lock().is_ok_and(|a| !a.is_empty())
    }
}

impl Session {
    /// Free space of the drive of a local panel; `None` for servers, archives and result
    /// lists, and until the first answer arrives.
    pub fn volume_space(&self, side: usize) -> Option<DiskSpace> {
        let panel = &self.panels[side];
        if panel.remote.is_some() || panel.archive.is_some() || panel.virtual_list.is_some() {
            return None;
        }
        self.disk_usage.get(&panel.cwd)
    }

    /// The breadcrumbs of a panel: local folders and server folders; archives and result
    /// lists have no segments to click (the caller shows their title).
    pub fn panel_crumbs(&self, side: usize) -> Vec<Crumb> {
        let panel = &self.panels[side];
        if panel.archive.is_some() || panel.virtual_list.is_some() {
            return Vec::new();
        }
        match &panel.remote {
            Some(view) => {
                let mut out = vec![Crumb {
                    label: view.label.clone(),
                    path: PathBuf::from("/"),
                }];
                out.extend(crumbs(Path::new(&view.path)).into_iter().skip(1));
                out
            }
            None => crumbs(&panel.cwd),
        }
    }
}

/// How many sub-folders the breadcrumb menu lists at most.
const SUBFOLDERS_MAX: usize = 300;

impl Session {
    /// The folders inside `parent` for the menu behind a breadcrumb separator, sorted by
    /// name. Only local folders (a server listing is not read here); empty when the folder
    /// cannot be read.
    pub fn crumb_subfolders(&self, side: usize, parent: &Path) -> Vec<Crumb> {
        if self.panels[side].remote.is_some() {
            return Vec::new();
        }
        let Ok(read) = std::fs::read_dir(parent) else {
            return Vec::new();
        };
        let mut folders: Vec<Crumb> = read
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .map(|entry| Crumb {
                label: entry.file_name().to_string_lossy().into_owned(),
                path: entry.path(),
            })
            .collect();
        folders.sort_by_key(|c| crate::palette::fold(&c.label));
        folders.truncate(SUBFOLDERS_MAX);
        folders
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_path_becomes_segments_each_with_its_own_path() {
        let path = std::env::temp_dir().join("alpha").join("beta");
        let list = crumbs(&path);
        let last = list.last().unwrap();
        assert_eq!(last.label, "beta");
        assert_eq!(last.path, path);
        assert_eq!(list[list.len() - 2].label, "alpha");
        assert_eq!(list[list.len() - 2].path, path.parent().unwrap());
        // Every segment is a prefix of the next.
        for pair in list.windows(2) {
            assert!(pair[1].path.starts_with(&pair[0].path), "{pair:?}");
        }
        // The first segment is the root (or the drive), never empty.
        assert!(!list[0].label.is_empty());
        assert_eq!(crumbs(Path::new("")), vec![]);
    }

    #[test]
    fn free_space_arrives_in_the_background_and_is_remembered() {
        let tmp = tempfile::tempdir().unwrap();
        let usage = DiskUsage::default();
        // The first call cannot know yet but starts the question.
        let first = usage.get(tmp.path());
        let mut answer = first;
        for _ in 0..1000 {
            if answer.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
            answer = usage.get(tmp.path());
        }
        let space = answer.expect("the answer arrived");
        assert!(space.total >= space.free);
        assert!(!usage.is_asking() || usage.get(tmp.path()).is_some());
    }

    #[test]
    fn server_panels_start_their_crumbs_with_the_server_and_archives_have_none() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Session::new(tmp.path(), tmp.path());
        let local = s.panel_crumbs(0);
        assert_eq!(local.last().unwrap().path, tmp.path());
        s.panels[0].virtual_list = Some(crate::VirtualList {
            title: "x".into(),
            paths: vec![],
        });
        assert!(s.panel_crumbs(0).is_empty());
        assert!(s.volume_space(0).is_none());
    }

    #[test]
    fn the_menu_behind_a_separator_lists_the_folders_inside_sorted_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        for name in ["beta", "Alfa", "żaba", "gamma"] {
            std::fs::create_dir_all(tmp.path().join(name)).unwrap();
        }
        std::fs::write(tmp.path().join("plik.txt"), "x").unwrap();
        let s = Session::new(tmp.path(), tmp.path());
        let labels: Vec<String> = s
            .crumb_subfolders(0, tmp.path())
            .into_iter()
            .map(|c| c.label)
            .collect();
        assert_eq!(
            labels,
            ["Alfa", "beta", "gamma", "żaba"],
            "files are not listed"
        );
        let first = &s.crumb_subfolders(0, tmp.path())[0];
        assert_eq!(first.path, tmp.path().join("Alfa"));
        assert!(s.crumb_subfolders(0, &tmp.path().join("nie-ma")).is_empty());
    }
}
