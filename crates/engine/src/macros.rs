//! Macros: the command lines you run are recorded into a `.bp` script (`macro record Porządki`,
//! work as usual, `macro stop`) and played back later (`macro play Porządki`), also from
//! `bluepane-cli run-script`. A script is plain text, one command per line; empty lines and lines
//! starting with `#` are ignored, so it can be edited by hand (`macro edit`).
//!
//! Parameters in a line: `%1`..`%9` the arguments of `macro play`, `%d` the folder of the active
//! panel, `%o` the folder of the other panel, `%f` the name under the cursor, `%%` a percent sign.
//!
//! Only what goes through the command line is recorded (keys, palette, the box, typed commands);
//! mouse clicks on rows are not, so a macro uses keys such as `cursor` and `select` instead.

use std::path::PathBuf;

use crate::session::Session;
use crate::EngineError;

/// How deep macros may call macros (`macro play` inside a script).
const MAX_DEPTH: u8 = 4;
const EXTENSION: &str = "bp";

/// What `macro` asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacroAction {
    List,
    Record(String),
    Stop,
    /// Drop the recording without saving.
    Discard,
    Play(String, Vec<String>),
    Show(String),
    Edit(String),
    Delete(String),
}

/// A recording in progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Recording {
    name: String,
    lines: Vec<String>,
}

/// Lines that are about recording itself or about the front end, never part of a macro.
fn recordable(line: &str) -> bool {
    let word = line.split_whitespace().next().unwrap_or("");
    !matches!(
        word,
        "" | "macro" | "mac" | "box" | "palette" | "jump" | "keys" | "help" | "quit"
    )
}

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
            "nazwa makra „{name}” może mieć tylko litery, cyfry, spacje, - i _"
        )))
    }
}

impl Session {
    fn macros_dir(&self) -> Result<PathBuf, EngineError> {
        self.config_dir()
            .map(|dir| dir.join("macros"))
            .ok_or_else(|| EngineError("nie ma katalogu konfiguracji".into()))
    }

    pub(crate) fn macro_path(&self, name: &str) -> Result<PathBuf, EngineError> {
        self.macro_file(name)
    }

    /// A macro name checked for use in a file name.
    pub(crate) fn macro_name(&self, name: &str) -> Result<String, EngineError> {
        check_name(name)
    }

    fn macro_file(&self, name: &str) -> Result<PathBuf, EngineError> {
        Ok(self.macros_dir()?.join(format!("{name}.{EXTENSION}")))
    }

    /// The names of the saved macros, sorted.
    pub fn saved_macros(&self) -> Vec<String> {
        let Ok(dir) = self.macros_dir() else {
            return Vec::new();
        };
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                name.strip_suffix(&format!(".{EXTENSION}"))
                    .map(str::to_string)
            })
            .collect();
        names.sort();
        names
    }

    /// Whether a macro is being recorded (for the status bar).
    pub fn recording_macro(&self) -> Option<&str> {
        self.recording.as_ref().map(|r| r.name.as_str())
    }

    /// Called for every command line that was typed or bound to a key.
    pub(crate) fn record_line(&mut self, line: &str) {
        if self.macro_depth > 0 || !recordable(line) {
            return;
        }
        if let Some(recording) = &mut self.recording {
            recording.lines.push(line.trim().to_string());
        }
    }

    /// The commands of a script (no empty lines or comments), parameters not yet filled in.
    pub fn script_commands(text: &str) -> Vec<String> {
        text.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_string)
            .collect()
    }

    /// The commands of a script with the parameters filled in from the state right now.
    pub fn script_lines(&self, text: &str, args: &[String]) -> Vec<String> {
        Self::script_commands(text)
            .iter()
            .map(|l| self.fill_parameters(l, args))
            .collect()
    }

    /// Replaces `%1`..`%9`, `%d`, `%o`, `%f` and `%%` in one line of a script.
    pub fn fill_parameters(&self, line: &str, args: &[String]) -> String {
        let here = self.active_panel().location().display().to_string();
        let other = self.panels[1 - self.active]
            .location()
            .display()
            .to_string();
        let under = self
            .active_panel()
            .current()
            .map(|e| e.name.clone())
            .unwrap_or_default();
        let mut out = String::new();
        let mut chars = line.chars();
        while let Some(c) = chars.next() {
            if c != '%' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('%') => out.push('%'),
                Some('d') => out.push_str(&here),
                Some('o') => out.push_str(&other),
                Some('f') => out.push_str(&under),
                Some(n @ '1'..='9') => {
                    let i = n as usize - '1' as usize;
                    out.push_str(args.get(i).map_or("", String::as_str));
                }
                Some(other) => {
                    out.push('%');
                    out.push(other);
                }
                None => out.push('%'),
            }
        }
        out
    }

    /// Runs the lines of a script in order. Stops at the first line that fails.
    pub fn play_script(&mut self, text: &str, args: &[String]) -> Result<usize, EngineError> {
        if self.macro_depth >= MAX_DEPTH {
            return Err(EngineError(
                "makra wywołują się zbyt głęboko (czy makro nie woła samo siebie?)".into(),
            ));
        }
        let lines = Self::script_commands(text);
        self.macro_depth += 1;
        let mut done = 0;
        let mut result = Ok(());
        for line in &lines {
            // Parameters like %d depend on the state after the earlier lines, so each line is
            // filled in just before it runs.
            let line = &self.fill_parameters(line, args);
            if let Err(err) = self.run_line(line) {
                result = Err(EngineError(format!("{line}: {}", err.0)));
                break;
            }
            done += 1;
        }
        self.macro_depth -= 1;
        result.map(|()| done)
    }

    pub(crate) fn macro_action(&mut self, action: MacroAction) -> Result<(), EngineError> {
        match action {
            MacroAction::List => {
                let names = self.saved_macros();
                self.status = format!(
                    "Makra: {} (macro record <nazwa>, stop, play <nazwa> [argumenty])",
                    if names.is_empty() {
                        "brak".to_string()
                    } else {
                        names.join(", ")
                    }
                );
            }
            MacroAction::Record(name) => {
                let name = check_name(&name)?;
                if self.recording.is_some() {
                    return Err(EngineError("nagrywanie już trwa: macro stop".into()));
                }
                self.recording = Some(Recording {
                    name: name.clone(),
                    lines: Vec::new(),
                });
                self.status = format!("Nagrywam makro „{name}”; zakończ komendą macro stop");
            }
            MacroAction::Stop => {
                let Some(recording) = self.recording.take() else {
                    return Err(EngineError("nic nie jest nagrywane".into()));
                };
                let path = self.macro_file(&recording.name)?;
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)
                        .map_err(|e| EngineError(format!("nie można zapisać makra: {e}")))?;
                }
                let mut text = format!("# Makro bluepane „{}”\n", recording.name);
                for line in &recording.lines {
                    text.push_str(line);
                    text.push('\n');
                }
                std::fs::write(&path, text)
                    .map_err(|e| EngineError(format!("nie można zapisać makra: {e}")))?;
                self.status = format!(
                    "Zapisano makro „{}” ({} poleceń)",
                    recording.name,
                    recording.lines.len()
                );
            }
            MacroAction::Discard => {
                self.recording = None;
                self.status = "Odrzucono nagrywanie".into();
            }
            MacroAction::Play(name, args) => {
                let name = check_name(&name)?;
                let text = std::fs::read_to_string(self.macro_file(&name)?)
                    .map_err(|_| EngineError(format!("nie ma makra „{name}”")))?;
                let count = self.play_script(&text, &args)?;
                self.status = format!("Makro „{name}”: wykonano {count} poleceń");
            }
            MacroAction::Show(name) => {
                let name = check_name(&name)?;
                let text = std::fs::read_to_string(self.macro_file(&name)?)
                    .map_err(|_| EngineError(format!("nie ma makra „{name}”")))?;
                let lines = self.script_lines(&text, &[]);
                self.status = format!("Makro „{name}”: {}", lines.join("  |  "));
            }
            MacroAction::Edit(name) => {
                let name = check_name(&name)?;
                let path = self.macro_file(&name)?;
                if !path.exists() {
                    return Err(EngineError(format!("nie ma makra „{name}”")));
                }
                let dir = path.parent().map(PathBuf::from).unwrap_or_default();
                self.run_line(&format!("cd {}", dir.display()))?;
                self.active_panel_mut().put_cursor_on(&path);
                self.execute(crate::Command::Edit)?;
            }
            MacroAction::Delete(name) => {
                let name = check_name(&name)?;
                std::fs::remove_file(self.macro_file(&name)?)
                    .map_err(|_| EngineError(format!("nie ma makra „{name}”")))?;
                self.status = format!("Usunięto makro „{name}”");
            }
        }
        Ok(())
    }
}

/// `macro [list|record <nazwa>|stop|discard|play <nazwa> [argumenty]|show|edit|delete <nazwa>]`
pub(crate) fn parse_macro(rest: &str) -> Result<MacroAction, EngineError> {
    let rest = rest.trim();
    let (verb, tail) = rest
        .split_once(char::is_whitespace)
        .map_or((rest, ""), |(v, t)| (v, t.trim()));
    Ok(match verb.to_ascii_lowercase().as_str() {
        "" | "list" => MacroAction::List,
        "record" | "rec" => MacroAction::Record(tail.to_string()),
        "stop" => MacroAction::Stop,
        "discard" => MacroAction::Discard,
        "play" | "run" => {
            // Arguments follow the name after a `;` or, when the name is one word, after it.
            let (name, args) = match tail.split_once(';') {
                Some((name, args)) => (name.trim(), args.split_whitespace().collect::<Vec<_>>()),
                None => {
                    let mut words = tail.split_whitespace();
                    (words.next().unwrap_or(""), words.collect())
                }
            };
            MacroAction::Play(
                name.to_string(),
                args.into_iter().map(str::to_string).collect(),
            )
        }
        "show" => MacroAction::Show(tail.to_string()),
        "edit" => MacroAction::Edit(tail.to_string()),
        "delete" => MacroAction::Delete(tail.to_string()),
        other => {
            return Err(EngineError(format!(
                "macro: nieznane „{other}” (list, record, stop, discard, play, show, edit, delete)"
            )))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn session(tmp: &Path) -> Session {
        for dir in ["l", "r", "l/Zażółć gęślą", "r/cel"] {
            std::fs::create_dir_all(tmp.join(dir)).unwrap();
        }
        std::fs::write(tmp.join("l/a.txt"), "a").unwrap();
        std::fs::write(tmp.join("l/Zażółć.txt"), "z").unwrap();
        let mut s = Session::new(tmp.join("l"), tmp.join("r"));
        s.use_config_dir(&tmp.join("config"));
        s
    }

    #[test]
    fn recorded_commands_are_saved_and_played_back_without_the_macro_commands() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("macro record Zażółć 1").unwrap();
        assert_eq!(s.recording_macro(), Some("Zażółć 1"));
        s.run_line("cd Zażółć gęślą").unwrap();
        s.run_line("box abc").unwrap();
        s.run_line("macro stop").unwrap();
        assert_eq!(s.recording_macro(), None);
        let text = std::fs::read_to_string(tmp.path().join("config/macros/Zażółć 1.bp")).unwrap();
        assert!(
            text.contains("cd Zażółć gęślą") && !text.contains("box") && !text.contains("macro")
        );
        assert_eq!(s.saved_macros(), vec!["Zażółć 1".to_string()]);

        s.run_line("cd ..").unwrap();
        // A name with spaces is closed with `;` (arguments may follow it).
        s.run_line("macro play Zażółć 1;").unwrap();
        assert!(s.active_panel().location().ends_with("Zażółć gęślą"));
    }

    #[test]
    fn parameters_are_filled_in_from_arguments_folders_and_the_cursor() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("cursor 2").ok();
        let lines = s.script_lines(
            "# komentarz\n\nmkdir %1\ncd %o\nnote 100%% %f %z\n",
            &["Łódź".into()],
        );
        assert_eq!(lines[0], "mkdir Łódź");
        assert_eq!(lines[1], format!("cd {}", tmp.path().join("r").display()));
        assert!(lines[2].starts_with("note 100% ") && lines[2].ends_with("%z"));
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn a_macro_with_arguments_runs_and_a_failing_line_stops_it() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        let macros = tmp.path().join("config/macros");
        std::fs::create_dir_all(&macros).unwrap();
        std::fs::write(macros.join("nowy.bp"), "mkdir %1\nzzzbad\nmkdir nigdy\n").unwrap();
        let err = s.run_line("macro play nowy Łódź ąę").unwrap_err();
        assert!(err.0.contains("zzzbad"), "{}", err.0);
        assert!(tmp.path().join("l/Łódź").is_dir());
        assert!(!tmp.path().join("l/nigdy").exists());
    }

    #[test]
    fn a_macro_calling_itself_is_stopped_and_missing_macros_are_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        let macros = tmp.path().join("config/macros");
        std::fs::create_dir_all(&macros).unwrap();
        std::fs::write(macros.join("pętla.bp"), "macro play pętla\n").unwrap();
        let err = s.run_line("macro play pętla").unwrap_err();
        assert!(err.0.contains("głęboko"), "{}", err.0);
        assert!(s.run_line("macro play nie ma").is_err());
        assert!(s.run_line("macro record ../zle").is_err());
        assert!(s.run_line("macro stop").is_err());
    }

    #[test]
    fn list_show_delete_and_discard_work() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("macro record a").unwrap();
        s.run_line("macro record b").unwrap_err();
        s.run_line("macro discard").unwrap();
        assert!(s.saved_macros().is_empty());
        s.run_line("macro record a").unwrap();
        s.run_line("cd ..").unwrap();
        s.run_line("macro stop").unwrap();
        s.run_line("macro show a").unwrap();
        assert!(s.status.contains("cd .."), "{}", s.status);
        s.run_line("macro list").unwrap();
        assert!(s.status.contains('a'));
        s.run_line("macro delete a").unwrap();
        assert!(s.saved_macros().is_empty());
        assert!(s.run_line("macro bogus").is_err());
    }
}
