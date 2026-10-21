//! Own tool buttons: external programs with parameters, kept in `buttons.conf` of the config
//! directory (one per line, `Name = program arguments`) and shown in a row of their own, in the
//! box (Ctrl+K) and as `button run <name>`. They replace plugins: no code is loaded, a button only
//! starts a program.
//!
//! The line is split into words (double quotes keep spaces in one word). No shell is used, so
//! characters like `;` or `&` in a file name are harmless. In a word:
//! `%f` the full path under the cursor, `%n` its name, `%d` the folder of the active panel,
//! `%o` the folder of the other panel, `%s` the selected files (or the one under the cursor) as
//! separate arguments when it is a word of its own, `%%` a percent sign. The program runs in the
//! folder of the active panel.

use std::path::PathBuf;

use crate::session::Session;
use crate::EngineError;

const FILE: &str = "buttons.conf";

/// One button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Button {
    pub name: String,
    pub line: String,
}

/// What `button` asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ButtonAction {
    List,
    Add(String, String),
    Delete(String),
    Run(String),
}

/// What a button would start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ButtonLaunch {
    pub program: String,
    pub args: Vec<String>,
    pub dir: PathBuf,
}

/// Splits a line into words; `"a b"` is one word, `\"` is not special (Windows paths).
pub(crate) fn split_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    words.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        words.push(current);
    }
    words
}

fn check_name(name: &str) -> Result<String, EngineError> {
    let name = name.trim();
    if !name.is_empty() && name.chars().count() <= 30 && !name.contains(['=', '\n']) {
        Ok(name.to_string())
    } else {
        Err(EngineError(
            "nazwa przycisku: do 30 znaków, bez znaku =".into(),
        ))
    }
}

pub(crate) fn load(path: &std::path::Path) -> Vec<Button> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() || l.starts_with('#') {
                return None;
            }
            let (name, line) = l.split_once('=')?;
            let (name, line) = (name.trim(), line.trim());
            (!name.is_empty() && !line.is_empty()).then(|| Button {
                name: name.to_string(),
                line: line.to_string(),
            })
        })
        .collect()
}

impl Session {
    pub(crate) fn load_buttons(&mut self, dir: &std::path::Path) {
        self.buttons = load(&dir.join(FILE));
    }

    fn save_buttons(&self) -> Result<(), EngineError> {
        let dir = self
            .config_dir()
            .ok_or_else(|| EngineError("nie ma katalogu konfiguracji".into()))?;
        let mut text = String::from("# Własne przyciski bluepane: Nazwa = program argumenty\n");
        for b in &self.buttons {
            text.push_str(&format!("{} = {}\n", b.name, b.line));
        }
        std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::write(dir.join(FILE), text))
            .map_err(|e| EngineError(format!("nie można zapisać przycisków: {e}")))
    }

    /// The command that starts a button, with the parameters filled in from the panels.
    pub fn button_launch(&self, name: &str) -> Result<ButtonLaunch, EngineError> {
        let button = self
            .buttons
            .iter()
            .find(|b| b.name.to_lowercase() == name.trim().to_lowercase())
            .ok_or_else(|| EngineError(format!("nie ma przycisku „{name}”")))?;
        self.expand_line(&button.line)
    }

    /// Splits a command line into program and arguments and fills in `%f %n %d %o %s` from the
    /// panels. It runs in the folder of the active panel, which must be on the disk.
    pub(crate) fn expand_line(&self, line: &str) -> Result<ButtonLaunch, EngineError> {
        let dir = self.active_panel().location();
        if !dir.is_dir() {
            return Err(EngineError(
                "działa w katalogu na dysku, nie w archiwum ani na serwerze".into(),
            ));
        }
        let other = self.panels[1 - self.active].location();
        let targets = self.active_panel().target_entries();
        let under = self.active_panel().current().filter(|e| e.name != "..");
        let mut words = split_words(line).into_iter();
        let program = words
            .next()
            .ok_or_else(|| EngineError("brak programu do uruchomienia".into()))?;
        let mut args = Vec::new();
        for word in words {
            if word == "%s" {
                args.extend(targets.iter().map(|e| e.path.display().to_string()));
                continue;
            }
            let mut out = String::new();
            let mut chars = word.chars();
            while let Some(c) = chars.next() {
                if c != '%' {
                    out.push(c);
                    continue;
                }
                match chars.next() {
                    Some('%') => out.push('%'),
                    Some('f') => out.push_str(
                        &under
                            .map(|e| e.path.display().to_string())
                            .unwrap_or_default(),
                    ),
                    Some('n') => out.push_str(under.map_or("", |e| e.name.as_str())),
                    Some('d') => out.push_str(&dir.display().to_string()),
                    Some('o') => out.push_str(&other.display().to_string()),
                    Some('s') => out.push_str(
                        &targets
                            .iter()
                            .map(|e| e.path.display().to_string())
                            .collect::<Vec<_>>()
                            .join(" "),
                    ),
                    Some(x) => {
                        out.push('%');
                        out.push(x);
                    }
                    None => out.push('%'),
                }
            }
            args.push(out);
        }
        Ok(ButtonLaunch { program, args, dir })
    }

    pub(crate) fn button_action(&mut self, action: ButtonAction) -> Result<(), EngineError> {
        match action {
            ButtonAction::List => {
                self.status = if self.buttons.is_empty() {
                    "Brak własnych przycisków (button add <nazwa> = <program argumenty>)".into()
                } else {
                    format!(
                        "Przyciski: {}",
                        self.buttons
                            .iter()
                            .map(|b| format!("{} = {}", b.name, b.line))
                            .collect::<Vec<_>>()
                            .join("  |  ")
                    )
                };
            }
            ButtonAction::Add(name, line) => {
                let name = check_name(&name)?;
                if line.trim().is_empty() {
                    return Err(EngineError(
                        "button add <nazwa> = <program argumenty>".into(),
                    ));
                }
                let button = Button {
                    name: name.clone(),
                    line: line.trim().to_string(),
                };
                match self.buttons.iter_mut().find(|b| b.name == name) {
                    Some(existing) => *existing = button,
                    None => self.buttons.push(button),
                }
                self.save_buttons()?;
                self.status = format!("Zapisano przycisk „{name}”");
            }
            ButtonAction::Delete(name) => {
                let before = self.buttons.len();
                self.buttons.retain(|b| b.name != name.trim());
                if self.buttons.len() == before {
                    return Err(EngineError(format!("nie ma przycisku „{}”", name.trim())));
                }
                self.save_buttons()?;
                self.status = format!("Usunięto przycisk „{}”", name.trim());
            }
            ButtonAction::Run(name) => {
                let launch = self.button_launch(&name)?;
                bluepane_vfs::spawn_program(&launch.program, &launch.args, &launch.dir)
                    .map_err(|e| EngineError(format!("nie udało się uruchomić: {e}")))?;
                self.status = format!("Uruchomiono „{}”", name.trim());
            }
        }
        Ok(())
    }
}

/// `button [list|add <nazwa> = <program argumenty>|delete <nazwa>|run <nazwa>]`
pub(crate) fn parse_button(rest: &str) -> Result<ButtonAction, EngineError> {
    let rest = rest.trim();
    let (verb, tail) = rest
        .split_once(char::is_whitespace)
        .map_or((rest, ""), |(v, t)| (v, t.trim()));
    Ok(match verb.to_ascii_lowercase().as_str() {
        "" | "list" => ButtonAction::List,
        "add" => match tail.split_once('=') {
            Some((name, line)) => {
                ButtonAction::Add(name.trim().to_string(), line.trim().to_string())
            }
            None => {
                return Err(EngineError(
                    "button add <nazwa> = <program argumenty>".into(),
                ))
            }
        },
        "delete" => ButtonAction::Delete(tail.to_string()),
        "run" => ButtonAction::Run(tail.to_string()),
        other => {
            return Err(EngineError(format!(
                "button: nieznane „{other}” (list, add, delete, run)"
            )))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn session(tmp: &Path) -> Session {
        for dir in ["l", "r"] {
            std::fs::create_dir_all(tmp.join(dir)).unwrap();
        }
        std::fs::write(tmp.join("l/Zażółć gęślą.txt"), "a").unwrap();
        std::fs::write(tmp.join("l/b; rm -rf.txt"), "b").unwrap();
        let mut s = Session::new(tmp.join("l"), tmp.join("r"));
        s.use_config_dir(&tmp.join("config"));
        s
    }

    #[test]
    fn quotes_keep_spaces_in_one_word() {
        assert_eq!(
            split_words(r#"edytor "C:\Program Files\x y" -n  a"#),
            vec!["edytor", r"C:\Program Files\x y", "-n", "a"]
        );
        assert_eq!(split_words(r#"a "" b"#), vec!["a", "", "b"]);
        assert!(split_words("   ").is_empty());
    }

    #[test]
    fn buttons_are_saved_and_come_back_in_a_new_session() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("button add Edytor = code %f").unwrap();
        s.run_line("button add Zażółć = tool %s").unwrap();
        s.run_line("button add Edytor = code --reuse %f").unwrap();
        assert_eq!(s.buttons.len(), 2);
        s.run_line("button list").unwrap();
        assert!(s.status.contains("Zażółć = tool %s"), "{}", s.status);

        let mut again = Session::new(tmp.path().join("l"), tmp.path().join("r"));
        again.use_config_dir(&tmp.path().join("config"));
        assert_eq!(again.buttons, s.buttons);
        again.run_line("button delete Zażółć").unwrap();
        assert_eq!(again.buttons.len(), 1);
        assert!(again.run_line("button delete nie ma").is_err());
        assert!(again.run_line("button add bez znaku").is_err());
    }

    #[test]
    fn parameters_reach_the_program_as_separate_safe_arguments() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("button add T = prog -f %f -n %n -d %d -o %o -x %% %s end")
            .unwrap();
        s.run_line("cursor 1").unwrap();
        s.run_line("select b; rm -rf.txt").ok();
        let l = tmp.path().join("l");
        let launch = s.button_launch("t").unwrap();
        assert_eq!(launch.program, "prog");
        assert_eq!(launch.dir, l);
        let at = |i: usize| launch.args[i].as_str();
        assert_eq!(at(2), "-n");
        assert_eq!(at(5), l.display().to_string());
        assert_eq!(at(7), tmp.path().join("r").display().to_string());
        assert_eq!(at(9), "%");
        assert_eq!(launch.args.last().unwrap(), "end");
        // Every selected or current file is a single argument, spaces and `;` included.
        assert!(launch.args.iter().all(|a| !a.contains("  ")), "{launch:?}");
    }

    #[test]
    fn a_selection_becomes_one_argument_per_file() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("button add S = prog %s").unwrap();
        s.run_line("selectall").unwrap();
        let launch = s.button_launch("S").unwrap();
        assert_eq!(launch.args.len(), 2, "{launch:?}");
        assert!(launch.args.iter().any(|a| a.ends_with("Zażółć gęślą.txt")));
    }

    #[test]
    fn missing_buttons_and_programs_are_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        assert!(s.run_line("button run nic").is_err());
        s.run_line("button add Zły = no-such-program-bluepane %f")
            .unwrap();
        let err = s.run_line("button run Zły").unwrap_err();
        assert!(err.0.contains("no-such-program-bluepane"), "{}", err.0);
    }

    #[test]
    fn the_box_offers_buttons_by_name_without_polish_letters() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("button add Zażółć = prog").unwrap();
        s.run_line("box zazolc").unwrap();
        let rows = s.box_entries();
        let row = rows
            .iter()
            .find(|e| e.kind == "Przycisk")
            .expect("a button row");
        assert_eq!(
            row.target,
            crate::quickbox::BoxTarget::Line("button run Zażółć".into())
        );
    }
}
