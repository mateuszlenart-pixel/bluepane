//! The "Places" side bar (Ctrl+B): drives with their free space, favorite folders, saved
//! servers and the folders visited last, each one a command line that goes there. The
//! favorites are kept in `favorites.conf` of the config directory.

use std::path::{Path, PathBuf};

use bluepane_vfs::DiskSpace;

use crate::session::Session;
use crate::EngineError;

const FAVORITES_FILE: &str = "favorites.conf";
/// Recently visited folders shown in the bar.
const RECENT_PLACES: usize = 8;

/// What the `fav` command does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FavoriteAction {
    /// Adds the folder (the active panel's when `None`), or removes it when it is a favorite.
    Toggle(Option<PathBuf>),
    Remove(PathBuf),
}

/// The favorite folders, in the order they were added.
#[derive(Debug, Default, Clone)]
pub struct Favorites {
    pub paths: Vec<PathBuf>,
    file: Option<PathBuf>,
}

impl Favorites {
    pub fn load(file: &Path) -> Self {
        let paths = std::fs::read_to_string(file)
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .collect();
        Favorites {
            paths,
            file: Some(file.to_path_buf()),
        }
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.paths.iter().any(|p| p == path)
    }

    fn save(&self) {
        let Some(file) = &self.file else { return };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let text: Vec<String> = self.paths.iter().map(|p| p.display().to_string()).collect();
        let _ = std::fs::write(file, text.join("\n"));
    }
}

/// One row of the bar.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub label: String,
    /// The command line that goes there.
    pub line: String,
    /// Free and total space, for drives.
    pub space: Option<DiskSpace>,
    /// The command line that takes the row off the list (favorites).
    pub remove: Option<String>,
    /// The active panel is here now.
    pub current: bool,
}

/// A titled group of rows.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceSection {
    pub title: &'static str,
    pub places: Vec<Place>,
}

impl Session {
    /// The sections of the bar (a section with no rows is left out).
    pub fn places(&self) -> Vec<PlaceSection> {
        let here = self.active_panel().location();
        let mut sections = Vec::new();

        let current_volume = self.volume_of(self.active);
        let drives: Vec<Place> = self
            .volumes
            .iter()
            .enumerate()
            .map(|(i, volume)| Place {
                label: volume.name.clone(),
                line: format!("volume {}", i + 1),
                space: self.disk_usage.get(&volume.path),
                remove: None,
                current: current_volume == Some(i),
            })
            .collect();
        sections.push(PlaceSection {
            title: "Dyski",
            places: drives,
        });

        let favorites: Vec<Place> = self
            .favorites
            .paths
            .iter()
            .map(|path| Place {
                label: folder_label(path),
                line: format!("cd {}", path.display()),
                space: None,
                remove: Some(format!("fav del {}", path.display())),
                current: *path == here,
            })
            .collect();
        sections.push(PlaceSection {
            title: "Ulubione",
            places: favorites,
        });

        let servers: Vec<Place> = self
            .sites
            .sites
            .iter()
            .map(|site| Place {
                label: site.name.clone(),
                line: format!("connect {}", site.name),
                space: None,
                remove: None,
                current: false,
            })
            .collect();
        sections.push(PlaceSection {
            title: "Serwery",
            places: servers,
        });

        let recent: Vec<Place> = self
            .visits
            .latest(RECENT_PLACES * 3)
            .into_iter()
            .filter(|p| !self.favorites.contains(p) && p.is_dir())
            .take(RECENT_PLACES)
            .map(|path| Place {
                label: folder_label(&path),
                line: format!("cd {}", path.display()),
                space: None,
                remove: None,
                current: path == here,
            })
            .collect();
        sections.push(PlaceSection {
            title: "Ostatnie",
            places: recent,
        });

        sections.retain(|s| !s.places.is_empty());
        sections
    }

    pub(crate) fn favorite_action(&mut self, action: FavoriteAction) -> Result<(), EngineError> {
        match action {
            FavoriteAction::Toggle(path) => {
                let path = match path {
                    Some(path) => self.resolve_dir(&path)?,
                    None => {
                        let panel = self.active_panel();
                        if panel.remote.is_some()
                            || panel.archive.is_some()
                            || panel.virtual_list.is_some()
                        {
                            return Err(EngineError(
                                "ulubione mogą być tylko katalogi na dysku".into(),
                            ));
                        }
                        panel.cwd.clone()
                    }
                };
                if let Some(index) = self.favorites.paths.iter().position(|p| *p == path) {
                    self.favorites.paths.remove(index);
                    self.status = format!("Usunięto z ulubionych: {}", path.display());
                } else {
                    self.favorites.paths.push(path.clone());
                    self.status = format!("Dodano do ulubionych: {}", path.display());
                }
            }
            FavoriteAction::Remove(path) => {
                let before = self.favorites.paths.len();
                self.favorites.paths.retain(|p| *p != path);
                if self.favorites.paths.len() == before {
                    return Err(EngineError(format!(
                        "{} nie jest w ulubionych",
                        path.display()
                    )));
                }
                self.status = format!("Usunięto z ulubionych: {}", path.display());
            }
        }
        self.favorites.save();
        Ok(())
    }

    /// An existing folder named by the user (relative to the active panel's folder).
    fn resolve_dir(&self, path: &Path) -> Result<PathBuf, EngineError> {
        let full = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.active_panel().cwd.join(path)
        };
        if full.is_dir() {
            Ok(crate::session::normalize_path(&full))
        } else {
            Err(EngineError(format!(
                "{} nie jest katalogiem",
                full.display()
            )))
        }
    }

    pub(crate) fn load_favorites(&mut self, dir: &Path) {
        self.favorites = Favorites::load(&dir.join(FAVORITES_FILE));
    }
}

/// The last component of the path, or the whole path for a root.
fn folder_label(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(tmp: &Path) -> Session {
        std::fs::create_dir_all(tmp.join("left/docs")).unwrap();
        std::fs::create_dir_all(tmp.join("right")).unwrap();
        let mut s = Session::new(tmp.join("left"), tmp.join("right"));
        s.use_config_dir(&tmp.join("config"));
        s
    }

    fn titles(s: &Session) -> Vec<&'static str> {
        s.places().iter().map(|p| p.title).collect()
    }

    #[test]
    fn a_folder_is_added_to_the_favorites_and_removed_again_and_survives_a_restart() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        assert!(!titles(&s).contains(&"Ulubione"));
        s.run_line("fav docs").unwrap();
        let favorites = s
            .places()
            .into_iter()
            .find(|p| p.title == "Ulubione")
            .unwrap();
        assert_eq!(favorites.places.len(), 1);
        assert_eq!(favorites.places[0].label, "docs");
        assert!(favorites.places[0].line.starts_with("cd "));
        assert!(favorites.places[0]
            .remove
            .as_ref()
            .unwrap()
            .starts_with("fav del "));
        // A second session over the same config directory sees it.
        let again = session(tmp.path());
        assert_eq!(again.favorites.paths.len(), 1);
        s.run_line("fav docs").unwrap();
        assert!(s.favorites.paths.is_empty(), "the same command toggles");
        assert!(!titles(&s).contains(&"Ulubione"));
    }

    #[test]
    fn fav_without_a_path_takes_the_active_folder_and_del_removes_by_path() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("fav").unwrap();
        assert_eq!(s.favorites.paths, [tmp.path().join("left")]);
        assert!(s
            .places()
            .iter()
            .any(|p| p.title == "Ulubione" && p.places[0].current));
        s.run_line(&format!("fav del {}", tmp.path().join("left").display()))
            .unwrap();
        assert!(s.favorites.paths.is_empty());
        assert!(s.run_line("fav del /nie/ma/takiego").is_err());
        assert!(s.run_line("fav nie-ma-takiego").is_err());
    }

    #[test]
    fn drives_are_listed_with_the_current_one_marked_and_recent_folders_follow() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("cd docs").unwrap();
        s.run_line("cd ..").unwrap();
        let sections = s.places();
        assert_eq!(sections[0].title, "Dyski");
        assert!(sections[0].places[0].line.starts_with("volume "));
        let recent = sections.iter().find(|p| p.title == "Ostatnie").unwrap();
        assert!(recent.places.iter().any(|p| p.label == "docs"));
        // A favorite is not repeated among the recent ones.
        s.run_line("fav docs").unwrap();
        let recent = s.places().into_iter().find(|p| p.title == "Ostatnie");
        assert!(recent.is_none_or(|r| r.places.iter().all(|p| p.label != "docs")));
    }

    #[test]
    fn saved_servers_are_listed_and_the_bar_opens_and_closes() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("siteset nas host nas.local").unwrap();
        let servers = s
            .places()
            .into_iter()
            .find(|p| p.title == "Serwery")
            .unwrap();
        assert_eq!(servers.places[0].line, "connect nas");
        assert!(!s.places_open);
        s.run_line("places").unwrap();
        assert!(s.places_open);
        s.run_line("places").unwrap();
        assert!(!s.places_open, "the same command toggles");
    }

    #[test]
    fn favorites_cannot_be_servers_or_archives() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.panels[0].virtual_list = Some(crate::VirtualList {
            title: "wyniki".into(),
            paths: Vec::new(),
        });
        assert!(s.run_line("fav").is_err());
    }
}
