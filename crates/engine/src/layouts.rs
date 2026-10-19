//! Named window layouts (`layout save <name>`, `layout load <name>`, `layout reset`, ...).
//! The front end turns its dock into text and back; the engine only keeps the texts, one
//! file per name in `layouts/` of the config directory, and passes the requests on.

use std::path::PathBuf;

use crate::session::Session;
use crate::EngineError;

/// What `layout` asks the front end to do with its dock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutAction {
    /// The two panels side by side again.
    Reset,
    /// Save the dock under this name.
    Save(String),
    /// Put the dock saved under this name back.
    Load(String),
}

/// A name that is a plain file name: letters, digits, spaces, `-` and `_`.
fn check_name(name: &str) -> Result<String, EngineError> {
    let name = name.trim();
    let fine = !name.is_empty()
        && name.chars().count() <= 60
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_');
    if fine {
        Ok(name.to_string())
    } else {
        Err(EngineError(format!(
            "nazwa układu „{name}” może mieć tylko litery, cyfry, spacje, - i _"
        )))
    }
}

impl Session {
    fn layouts_dir(&self) -> Result<PathBuf, EngineError> {
        self.config_dir()
            .map(|dir| dir.join("layouts"))
            .ok_or_else(|| EngineError("nie ma katalogu konfiguracji".into()))
    }

    /// The names of the saved layouts, sorted.
    pub fn saved_layouts(&self) -> Vec<String> {
        let Ok(dir) = self.layouts_dir() else {
            return Vec::new();
        };
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                name.strip_suffix(".json").map(str::to_string)
            })
            .collect();
        names.sort();
        names
    }

    /// Keeps a layout's text under its name (the front end calls this for `layout save`).
    pub fn store_layout(&mut self, name: &str, text: &str) -> Result<(), EngineError> {
        let dir = self.layouts_dir()?;
        std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::write(dir.join(format!("{name}.json")), text))
            .map_err(|e| EngineError(format!("nie udało się zapisać układu: {e}")))?;
        self.status = format!("Zapisano układ „{name}”");
        Ok(())
    }

    /// The text saved under a name (the front end calls this for `layout load`).
    pub fn layout_text(&self, name: &str) -> Result<String, EngineError> {
        let path = self.layouts_dir()?.join(format!("{name}.json"));
        std::fs::read_to_string(path).map_err(|_| {
            EngineError(format!(
                "nie ma układu „{name}” (zapisane: {})",
                self.layout_list_text()
            ))
        })
    }

    fn layout_list_text(&self) -> String {
        let names = self.saved_layouts();
        if names.is_empty() {
            "brak".to_string()
        } else {
            names.join(", ")
        }
    }

    /// `layout [list|reset|save <nazwa>|load <nazwa>|delete <nazwa>]`.
    pub(crate) fn layout_command(&mut self, words: &str) -> Result<(), EngineError> {
        let (verb, name) = words
            .trim()
            .split_once(char::is_whitespace)
            .map_or((words.trim(), ""), |(v, n)| (v, n.trim()));
        match verb.to_ascii_lowercase().as_str() {
            "" | "list" => {
                self.status = format!(
                    "Układy okna: {} (layout save|load|delete <nazwa>, layout reset)",
                    self.layout_list_text()
                );
            }
            "reset" => {
                self.layout_request = Some(LayoutAction::Reset);
                self.status = "Układ okna: dwa panele obok siebie".into();
            }
            "save" => self.layout_request = Some(LayoutAction::Save(check_name(name)?)),
            "load" => {
                let name = check_name(name)?;
                // Fails here, with the list of names, when there is no such layout.
                self.layout_text(&name)?;
                self.layout_request = Some(LayoutAction::Load(name));
            }
            "delete" => {
                let name = check_name(name)?;
                let path = self.layouts_dir()?.join(format!("{name}.json"));
                std::fs::remove_file(&path)
                    .map_err(|_| EngineError(format!("nie ma układu „{name}”")))?;
                self.status = format!("Usunięto układ „{name}”");
            }
            other => {
                return Err(EngineError(format!(
                    "layout: nieznane „{other}” (list, reset, save, load, delete)"
                )))
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(tmp: &std::path::Path) -> Session {
        std::fs::create_dir_all(tmp.join("l")).unwrap();
        let mut s = Session::new(tmp.join("l"), tmp.join("l"));
        s.use_config_dir(&tmp.join("config"));
        s
    }

    #[test]
    fn save_and_load_go_to_the_front_end_and_texts_are_kept_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("layout save Praca z serwerem").unwrap();
        assert_eq!(
            s.layout_request.take(),
            Some(LayoutAction::Save("Praca z serwerem".into()))
        );
        s.store_layout("Praca z serwerem", "{\"x\":1}").unwrap();
        assert_eq!(s.saved_layouts(), ["Praca z serwerem"]);
        s.run_line("layout load Praca z serwerem").unwrap();
        assert_eq!(
            s.layout_request.take(),
            Some(LayoutAction::Load("Praca z serwerem".into()))
        );
        assert_eq!(s.layout_text("Praca z serwerem").unwrap(), "{\"x\":1}");
        s.run_line("layout").unwrap();
        assert!(s.status.contains("Praca z serwerem"), "{}", s.status);
        s.run_line("layout reset").unwrap();
        assert_eq!(s.layout_request.take(), Some(LayoutAction::Reset));
        s.run_line("layout delete Praca z serwerem").unwrap();
        assert!(s.saved_layouts().is_empty());
    }

    #[test]
    fn bad_names_and_missing_layouts_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        assert!(s.run_line("layout save ../poza").is_err());
        assert!(s.run_line("layout save").is_err());
        assert!(s.run_line("layout load nie-ma").is_err());
        assert!(s.layout_request.is_none(), "nothing asked of the front end");
        assert!(s.run_line("layout delete nie-ma").is_err());
        assert!(s.run_line("layout zrob").is_err());
    }
}
