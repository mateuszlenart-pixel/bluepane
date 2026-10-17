//! The right-click menu: which actions fit what is under the pointer. The items are plain
//! command lines, so the menu, the keys and the command line share one implementation.

use crate::format::{format_modified, human_size};
use crate::session::Session;
use crate::{class_of, EngineError, FileClass};

/// One row of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub label: String,
    /// The command line it runs (one ending with a space is completed in the command line).
    pub line: String,
    /// The key that does the same, for display.
    pub shortcut: Option<String>,
    /// A separator is drawn before this row.
    pub separator: bool,
}

struct Builder<'a> {
    session: &'a Session,
    items: Vec<MenuItem>,
    new_group: bool,
}

impl Builder<'_> {
    fn group(&mut self) {
        self.new_group = !self.items.is_empty();
    }

    fn add(&mut self, label: &str, line: &str) {
        self.items.push(MenuItem {
            label: label.to_string(),
            line: line.to_string(),
            shortcut: self.session.keymap.key_for(line),
            separator: std::mem::take(&mut self.new_group),
        });
    }
}

impl Session {
    /// The menu for a right click on the active panel: on a row (`on_row`, the cursor is
    /// already on it) or on the empty part of the list.
    pub fn context_menu(&self, on_row: bool) -> Vec<MenuItem> {
        let panel = self.active_panel();
        let local =
            panel.remote.is_none() && panel.archive.is_none() && panel.virtual_list.is_none();
        let in_archive = panel.archive.is_some();
        let remote = panel.remote.is_some();
        let mut b = Builder {
            session: self,
            items: Vec::new(),
            new_group: false,
        };

        let targets = panel.target_entries();
        if !on_row || targets.is_empty() {
            b.add("Nowy katalog", "mkdir");
            b.add("Odśwież", "refresh");
            b.group();
            b.add("Zaznacz wszystko", "selectall");
            b.add("Odwróć zaznaczenie", "invert");
            b.group();
            b.add("Szukaj plików…", "find");
            if !in_archive && !remote {
                b.add("Synchronizuj katalogi…", "sync");
            }
            b.add("Nowa zakładka", "tab new");
            if local {
                b.group();
                b.add("Dodaj ten katalog do ulubionych", "fav");
            }
            return b.items;
        }

        let single = targets.len() == 1;
        let first = targets[0];
        let class = class_of(first);
        if single {
            if first.is_dir() {
                b.add("Otwórz", "enter");
                b.add("Otwórz w nowej zakładce", "tab open");
            } else if class == FileClass::Archive {
                b.add("Otwórz archiwum", "enter");
                if local {
                    b.add("Rozpakuj do drugiego panelu", "unpack");
                    b.add("Sprawdź archiwum", "testarchive");
                }
            } else {
                if local {
                    b.add("Otwórz w programie", "open");
                }
                b.add("Podgląd", "view");
                if !in_archive {
                    b.add("Edytuj", "edit");
                }
            }
            b.group();
        }
        b.add("Kopiuj do drugiego panelu", "copy");
        if !in_archive {
            b.add("Przenieś do drugiego panelu", "move");
        }
        if !in_archive {
            if single {
                b.add("Zmień nazwę", "rename");
            } else {
                b.add("Zmień nazwy zbiorowo…", "multirename");
            }
        }
        if local {
            b.add("Spakuj…", "pack");
        }
        if !in_archive {
            b.add("Usuń", "delete");
        }
        b.group();
        b.add("Kopiuj ścieżkę", "copynames path");
        b.add("Kopiuj nazwę", "copynames file");
        if remote {
            b.add("Uprawnienia (chmod)…", "chmod ");
        }
        if single {
            if local && first.is_dir() {
                b.add(
                    "Dodaj do ulubionych",
                    &format!("fav {}", first.path.display()),
                );
            }
            b.group();
            b.add("Właściwości", "info");
        }
        b.items
    }

    /// `info`: what the entry under the cursor is, or a summary of the marked ones.
    pub(crate) fn show_info(&mut self) -> Result<(), EngineError> {
        let panel = self.active_panel();
        let targets = panel.target_entries();
        let text = match targets.as_slice() {
            [] => return Err(EngineError("nic nie wskazano".into())),
            [entry] => {
                let kind = if entry.is_dir() { "katalog" } else { "plik" };
                let size = if entry.is_dir() {
                    match self.dir_sizes.get(&entry.path) {
                        Some(bytes) => format!(", {}", human_size(bytes)),
                        None => String::new(),
                    }
                } else {
                    format!(", {} ({} B)", human_size(entry.size), entry.size)
                };
                format!(
                    "{}: {kind}{size}, zmieniony {}, {}",
                    entry.name,
                    format_modified(entry.modified),
                    entry.path.display()
                )
            }
            many => {
                let dirs = many.iter().filter(|e| e.is_dir()).count();
                let files = many.len() - dirs;
                let bytes: u64 = many.iter().filter(|e| !e.is_dir()).map(|e| e.size).sum();
                format!(
                    "Zaznaczono {}: {dirs} katalogów, {files} plików, razem {}",
                    many.len(),
                    human_size(bytes)
                )
            }
        };
        self.status = text;
        Ok(())
    }

    /// `open`: the file under the cursor in the system's default application.
    pub(crate) fn open_default(&mut self) -> Result<(), EngineError> {
        self.deny_remote("otwieranie w programie")?;
        self.deny_in_archive("otwieranie w programie")?;
        let path = self.entry_to_open()?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        bluepane_vfs::open_external(&path, None)
            .map_err(|e| EngineError(format!("nie udało się otworzyć: {e}")))?;
        self.status = format!("Otwarto w programie: {name}");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(tmp: &std::path::Path) -> Session {
        std::fs::create_dir_all(tmp.join("left/katalog")).unwrap();
        std::fs::create_dir_all(tmp.join("right")).unwrap();
        std::fs::write(tmp.join("left/a.txt"), "hello").unwrap();
        std::fs::write(tmp.join("left/b.txt"), "b").unwrap();
        let mut s = Session::new(tmp.join("left"), tmp.join("right"));
        s.use_config_dir(&tmp.join("config"));
        s
    }

    fn lines(items: &[MenuItem]) -> Vec<&str> {
        items.iter().map(|i| i.line.as_str()).collect()
    }

    #[test]
    fn a_file_a_folder_and_the_empty_list_get_different_menus() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("select a.txt").unwrap();
        s.run_line("unselectall").unwrap();
        let file = s.context_menu(true);
        for wanted in [
            "open", "view", "edit", "copy", "move", "rename", "delete", "info",
        ] {
            assert!(
                lines(&file).contains(&wanted),
                "{wanted} in {:?}",
                lines(&file)
            );
        }
        assert!(!lines(&file).contains(&"enter"));

        s.run_line("select katalog").unwrap();
        s.run_line("unselectall").unwrap();
        let folder = s.context_menu(true);
        assert!(lines(&folder).contains(&"enter") && lines(&folder).contains(&"tab open"));
        assert!(lines(&folder).iter().any(|l| l.starts_with("fav ")));
        assert!(!lines(&folder).contains(&"open"));

        let empty = s.context_menu(false);
        assert_eq!(lines(&empty)[0], "mkdir");
        assert!(lines(&empty).contains(&"refresh") && lines(&empty).contains(&"selectall"));
        assert!(!lines(&empty).contains(&"delete"));
    }

    #[test]
    fn several_marked_files_offer_group_actions_not_single_file_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("select a.txt").unwrap();
        s.run_line("select b.txt").unwrap();
        let menu = s.context_menu(true);
        assert!(lines(&menu).contains(&"multirename"));
        for single in ["rename", "view", "open", "info"] {
            assert!(!lines(&menu).contains(&single), "{single}");
        }
        assert!(lines(&menu).contains(&"delete") && lines(&menu).contains(&"copy"));
    }

    #[test]
    fn items_show_the_key_that_does_the_same_and_groups_are_separated() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("select a.txt").unwrap();
        s.run_line("unselectall").unwrap();
        let menu = s.context_menu(true);
        let copy = menu.iter().find(|i| i.line == "copy").unwrap();
        assert_eq!(copy.shortcut.as_deref(), Some("F5"));
        assert!(!menu[0].separator, "no separator before the first row");
        assert!(menu.iter().skip(1).any(|i| i.separator));
    }

    #[test]
    fn info_describes_one_entry_or_a_selection_and_open_refuses_folders_and_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("select a.txt").unwrap();
        s.run_line("unselectall").unwrap();
        s.run_line("info").unwrap();
        assert!(s.status.starts_with("a.txt: plik, "), "{}", s.status);
        assert!(s.status.contains("(5 B)"), "{}", s.status);
        s.run_line("selectall").unwrap();
        s.run_line("info").unwrap();
        assert!(s.status.starts_with("Zaznaczono 3:"), "{}", s.status);
        s.run_line("unselectall").unwrap();
        s.run_line("select katalog").unwrap();
        s.run_line("unselectall").unwrap();
        // A folder is entered, not opened in a program (and nothing is started by the test).
        assert!(s.run_line("open").is_err());
        assert!(s.status.contains("katalog"), "{}", s.status);
    }
}
