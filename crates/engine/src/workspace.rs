//! Workspaces: the tabs of both panels (folder, sorting and view of each tab, which tab is
//! shown, which panel is active), the Places bar and the window layout, saved under a name
//! (`workspace save Praca`) and as the last session, put back at the next start.
//!
//! A workspace is a small text file in `workspaces/` of the config directory:
//!
//! ```text
//! version = 1
//! active = 0
//! places = off
//! tab = 0 * name asc list /home/ala/projekty      (side, * = the tab shown, sort, view, path)
//! tab = 1 - date desc grid /home/ala/Zdjęcia
//! ```
//!
//! Only folders on a disk are kept (a tab showing a server or an archive is saved as the
//! folder it was opened from). A folder that is gone by the time the workspace is loaded is
//! left out, with a word in the status line.

use std::path::PathBuf;

use crate::panel::{Panel, PanelView, Sort, SortKey};
use crate::session::Session;
use crate::EngineError;

/// The name the last session is kept under (not offered in the list).
const LAST: &str = "ostatnia sesja";
const VERSION: u32 = 1;

/// What `workspace` asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceAction {
    List,
    Save(String),
    Load(String),
    Delete(String),
    /// Put the last session back at start (`on`) or not.
    Restore(bool),
}

#[derive(Debug, Clone, PartialEq)]
struct TabSnap {
    dir: PathBuf,
    sort: Sort,
    view: PanelView,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Snapshot {
    active: usize,
    places: bool,
    /// Per side: the tabs in order and which one is shown.
    sides: [(Vec<TabSnap>, usize); 2],
}

fn sort_word(key: SortKey) -> &'static str {
    match key {
        SortKey::Name => "name",
        SortKey::Ext => "ext",
        SortKey::Size => "size",
        SortKey::Date => "date",
    }
}

impl Snapshot {
    fn render(&self) -> String {
        let mut out = format!(
            "# Obszar roboczy bluepane\nversion = {VERSION}\nactive = {}\nplaces = {}\n",
            self.active,
            if self.places { "on" } else { "off" }
        );
        for (side, (tabs, shown)) in self.sides.iter().enumerate() {
            for (i, tab) in tabs.iter().enumerate() {
                out.push_str(&format!(
                    "tab = {side} {} {} {} {} {}\n",
                    if i == *shown { "*" } else { "-" },
                    sort_word(tab.sort.key),
                    if tab.sort.descending { "desc" } else { "asc" },
                    match tab.view {
                        PanelView::List => "list",
                        PanelView::Grid => "grid",
                    },
                    tab.dir.display()
                ));
            }
        }
        out
    }

    /// Reads a workspace file; unknown lines are skipped, a newer version is refused.
    fn parse(text: &str) -> Result<Snapshot, EngineError> {
        let mut snap = Snapshot::default();
        for line in text.lines().map(str::trim) {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            match key {
                "version" => {
                    let version: u32 = value.parse().unwrap_or(0);
                    if version > VERSION {
                        return Err(EngineError(format!(
                            "obszar roboczy jest z nowszej wersji programu (format {version})"
                        )));
                    }
                }
                "active" => snap.active = usize::from(value == "1"),
                "places" => snap.places = value == "on",
                "tab" => {
                    let mut words = value.splitn(6, ' ');
                    let (Some(side), Some(mark), Some(key), Some(dir), Some(view), Some(path)) = (
                        words.next(),
                        words.next(),
                        words.next(),
                        words.next(),
                        words.next(),
                        words.next(),
                    ) else {
                        continue;
                    };
                    let side = usize::from(side == "1");
                    let tab = TabSnap {
                        dir: PathBuf::from(path),
                        sort: Sort {
                            key: SortKey::parse(key).unwrap_or(SortKey::Name),
                            descending: dir == "desc",
                        },
                        view: if view == "grid" {
                            PanelView::Grid
                        } else {
                            PanelView::List
                        },
                    };
                    let (tabs, shown) = &mut snap.sides[side];
                    if mark == "*" {
                        *shown = tabs.len();
                    }
                    tabs.push(tab);
                }
                _ => {}
            }
        }
        Ok(snap)
    }
}

/// A name that is a plain file name: letters, digits, spaces, `-` and `_`.
fn check_name(name: &str) -> Result<String, EngineError> {
    let name = name.trim();
    let fine = !name.is_empty()
        && name.chars().count() <= 60
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_');
    if fine && name != LAST {
        Ok(name.to_string())
    } else {
        Err(EngineError(format!(
            "nazwa obszaru „{name}” może mieć tylko litery, cyfry, spacje, - i _"
        )))
    }
}

impl Session {
    fn workspaces_dir(&self) -> Result<PathBuf, EngineError> {
        self.config_dir()
            .map(|dir| dir.join("workspaces"))
            .ok_or_else(|| EngineError("nie ma katalogu konfiguracji".into()))
    }

    fn workspace_file(&self, name: &str) -> Result<PathBuf, EngineError> {
        Ok(self.workspaces_dir()?.join(format!("{name}.conf")))
    }

    /// The names of the saved workspaces, sorted (without the last session).
    pub fn saved_workspaces(&self) -> Vec<String> {
        let Ok(dir) = self.workspaces_dir() else {
            return Vec::new();
        };
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                name.strip_suffix(".conf").map(str::to_string)
            })
            .filter(|n| n != LAST)
            .collect();
        names.sort();
        names
    }

    /// Put the last session back at start; on by default.
    pub fn restores_last_session(&self) -> bool {
        self.settings.restore.unwrap_or(true)
    }

    fn snapshot(&self) -> Snapshot {
        let tab_of = |panel: &Panel| TabSnap {
            dir: panel.cwd.clone(),
            sort: panel.sort,
            view: panel.view,
        };
        let mut sides: [(Vec<TabSnap>, usize); 2] = Default::default();
        for (side, slot) in sides.iter_mut().enumerate() {
            let parked = &self.parked[side];
            let mut tabs: Vec<TabSnap> = parked.others.iter().map(tab_of).collect();
            tabs.insert(parked.position, tab_of(&self.panels[side]));
            *slot = (tabs, parked.position);
        }
        Snapshot {
            active: self.active,
            places: self.places_open,
            sides,
        }
    }

    /// Puts the tabs of a snapshot in place; returns how many folders were gone.
    fn apply(&mut self, snap: &Snapshot) -> usize {
        let mut missing = 0;
        for (side, (tabs, shown)) in snap.sides.iter().enumerate() {
            let mut panels = Vec::new();
            let mut position = 0;
            for (i, tab) in tabs.iter().enumerate() {
                if !tab.dir.is_dir() {
                    missing += 1;
                    continue;
                }
                if i == *shown {
                    position = panels.len();
                }
                let mut panel = Panel::new(&tab.dir);
                panel.set_sort(tab.sort.key, Some(tab.sort.descending));
                panel.view = tab.view;
                panels.push(panel);
            }
            if panels.is_empty() {
                // Nothing left on this side: it stays as it is.
                continue;
            }
            let shown_panel = panels.remove(position);
            self.panels[side] = shown_panel;
            self.parked[side].others = panels;
            self.parked[side].position = position;
        }
        self.active = snap.active;
        self.places_open = snap.places;
        missing
    }

    fn write_workspace(&self, name: &str) -> Result<(), EngineError> {
        let path = self.workspace_file(name)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| EngineError(format!("nie udało się zapisać obszaru: {e}")))?;
        }
        std::fs::write(&path, self.snapshot().render())
            .map_err(|e| EngineError(format!("nie udało się zapisać obszaru: {e}")))
    }

    fn read_workspace(&mut self, name: &str) -> Result<usize, EngineError> {
        let text = std::fs::read_to_string(self.workspace_file(name)?)
            .map_err(|_| EngineError(format!("nie ma obszaru roboczego „{name}”")))?;
        let snap = Snapshot::parse(&text)?;
        Ok(self.apply(&snap))
    }

    /// Saves the tabs as the last session (the front end calls this when the window closes).
    pub fn save_last_session(&self) {
        if self.restores_last_session() {
            let _ = self.write_workspace(LAST);
        }
    }

    /// Puts the last session back, if that is switched on and there is one. Call at start
    /// when no folders were given on the command line.
    pub fn restore_last_session(&mut self) {
        if !self.restores_last_session() {
            return;
        }
        if let Ok(missing) = self.read_workspace(LAST) {
            if missing > 0 {
                self.status = format!(
                    "Przywrócono poprzednią sesję; pominięto katalogi, których już nie ma: {missing}"
                );
            }
        }
    }

    pub(crate) fn workspace_action(&mut self, action: WorkspaceAction) -> Result<(), EngineError> {
        match action {
            WorkspaceAction::List => {
                let names = self.saved_workspaces();
                self.status = format!(
                    "Obszary robocze: {} (workspace save|load|delete <nazwa>; przywracanie sesji: {})",
                    if names.is_empty() { "brak".to_string() } else { names.join(", ") },
                    if self.restores_last_session() { "włączone" } else { "wyłączone" }
                );
            }
            WorkspaceAction::Save(name) => {
                let name = check_name(&name)?;
                self.write_workspace(&name)?;
                // The dock layout goes with it (the front end turns the dock into text).
                self.layout_request = Some(crate::LayoutAction::Save(format!("obszar {name}")));
                self.status = format!("Zapisano obszar roboczy „{name}”");
            }
            WorkspaceAction::Load(name) => {
                let name = check_name(&name)?;
                let missing = self.read_workspace(&name)?;
                if self.layout_text(&format!("obszar {name}")).is_ok() {
                    self.layout_request = Some(crate::LayoutAction::Load(format!("obszar {name}")));
                }
                self.status = if missing == 0 {
                    format!("Wczytano obszar roboczy „{name}”")
                } else {
                    format!(
                        "Wczytano obszar roboczy „{name}”; pominięto katalogi, których już nie ma: {missing}"
                    )
                };
            }
            WorkspaceAction::Delete(name) => {
                let name = check_name(&name)?;
                std::fs::remove_file(self.workspace_file(&name)?)
                    .map_err(|_| EngineError(format!("nie ma obszaru roboczego „{name}”")))?;
                let _ = self.run_line(&format!("layout delete obszar {name}"));
                self.status = format!("Usunięto obszar roboczy „{name}”");
            }
            WorkspaceAction::Restore(on) => {
                self.settings.restore = Some(on);
                self.status = if on {
                    "Przy starcie wrócą zakładki z poprzedniej sesji".into()
                } else {
                    "Przy starcie panele otworzą się w katalogu startowym".into()
                };
                self.save_settings()?;
            }
        }
        Ok(())
    }
}

/// `workspace [list|save <nazwa>|load <nazwa>|delete <nazwa>|restore on|off]`
pub(crate) fn parse_workspace(rest: &str) -> Result<WorkspaceAction, EngineError> {
    let rest = rest.trim();
    let (verb, name) = rest
        .split_once(char::is_whitespace)
        .map_or((rest, ""), |(v, n)| (v, n.trim()));
    Ok(match verb.to_ascii_lowercase().as_str() {
        "" | "list" => WorkspaceAction::List,
        "save" => WorkspaceAction::Save(name.to_string()),
        "load" => WorkspaceAction::Load(name.to_string()),
        "delete" => WorkspaceAction::Delete(name.to_string()),
        "restore" => match name {
            "on" => WorkspaceAction::Restore(true),
            "off" => WorkspaceAction::Restore(false),
            _ => return Err(EngineError("workspace restore on|off".into())),
        },
        other => {
            return Err(EngineError(format!(
                "workspace: nieznane „{other}” (list, save, load, delete, restore)"
            )))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn session(tmp: &Path) -> Session {
        for dir in ["l", "r", "l/projekty", "r/Zdjęcia z wakacji"] {
            std::fs::create_dir_all(tmp.join(dir)).unwrap();
        }
        let mut s = Session::new(tmp.join("l"), tmp.join("r"));
        s.use_config_dir(&tmp.join("config"));
        s
    }

    #[test]
    fn a_workspace_brings_back_tabs_sorting_views_and_the_active_panel() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("tab new").unwrap();
        s.run_line("cd projekty").unwrap();
        s.run_line("sort date desc").unwrap();
        s.run_line("switch").unwrap();
        s.run_line("cd Zdjęcia z wakacji").unwrap();
        s.run_line("viewmode grid").unwrap();
        s.run_line("places").unwrap();
        s.run_line("workspace save Praca").unwrap();
        assert!(matches!(
            s.layout_request.take(),
            Some(crate::LayoutAction::Save(_))
        ));
        assert_eq!(s.saved_workspaces(), ["Praca"]);

        let mut again = session(tmp.path());
        again.run_line("workspace load Praca").unwrap();
        assert_eq!(again.active, 1);
        assert!(again.places_open);
        assert!(again.panels[1].cwd.ends_with("Zdjęcia z wakacji"));
        assert_eq!(again.panels[1].view, PanelView::Grid);
        assert_eq!(again.tab_strip(0).titles.len(), 2);
        assert!(again.panels[0].cwd.ends_with("projekty"));
        assert_eq!(again.panels[0].sort.key, SortKey::Date);
        assert!(again.panels[0].sort.descending);
    }

    #[test]
    fn folders_that_are_gone_are_left_out_and_said_so() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        std::fs::create_dir_all(tmp.path().join("l/tymczasowy")).unwrap();
        s.run_line("tab new").unwrap();
        s.run_line("cd tymczasowy").unwrap();
        s.run_line("workspace save Stary").unwrap();
        std::fs::remove_dir_all(tmp.path().join("l/tymczasowy")).unwrap();
        let mut again = session(tmp.path());
        again.run_line("workspace load Stary").unwrap();
        assert!(again.status.contains("pominięto"), "{}", again.status);
        assert_eq!(again.tab_strip(0).titles.len(), 1);
        assert!(again.panels[0].cwd.ends_with("l"));
    }

    #[test]
    fn the_last_session_comes_back_unless_switched_off() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("cd projekty").unwrap();
        s.save_last_session();
        let mut again = session(tmp.path());
        again.restore_last_session();
        assert!(again.panels[0].cwd.ends_with("projekty"));
        assert!(
            again.saved_workspaces().is_empty(),
            "the last session is not in the list"
        );

        again.run_line("workspace restore off").unwrap();
        let mut third = session(tmp.path());
        third.restore_last_session();
        assert!(
            third.panels[0].cwd.ends_with("l"),
            "switched off: the start folder"
        );
    }

    #[test]
    fn bad_names_newer_formats_and_missing_workspaces_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        assert!(s.run_line("workspace save ../x").is_err());
        assert!(s.run_line("workspace save ostatnia sesja").is_err());
        assert!(s.run_line("workspace load nie-ma").is_err());
        assert!(s.run_line("workspace delete nie-ma").is_err());
        assert!(s.run_line("workspace restore maybe").is_err());
        let dir = tmp.path().join("config/workspaces");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Nowy.conf"), "version = 9\n").unwrap();
        assert!(s.run_line("workspace load Nowy").is_err());
    }

    #[test]
    fn the_file_round_trips() {
        let snap = Snapshot {
            active: 1,
            places: true,
            sides: [
                (
                    vec![TabSnap {
                        dir: PathBuf::from("/a b/c"),
                        sort: Sort {
                            key: SortKey::Size,
                            descending: true,
                        },
                        view: PanelView::Grid,
                    }],
                    0,
                ),
                (Vec::new(), 0),
            ],
        };
        assert_eq!(Snapshot::parse(&snap.render()).unwrap(), snap);
    }
}
