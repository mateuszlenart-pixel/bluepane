//! User settings kept between runs, in one file (`settings.conf` in the config directory):
//! the colour theme and the keys the user bound or took off.
//!
//! ```text
//! # comment
//! theme = dark
//! key Ctrl+K = sort name       (binds Ctrl+K)
//! key F9 =                     (takes F9 off: nothing happens)
//! key Ctrl+L = "cd "           (quotes keep a trailing space: the command line gets `cd `)
//! ```
//!
//! A line that cannot be understood is skipped with a warning; the rest still applies. The
//! engine only keeps the theme's name; the palettes belong to the front end.

use std::path::Path;

use crate::command::{Command, COMMAND_NAMES};
use crate::keymap::Keymap;
use crate::session::Session;
use crate::EngineError;

/// The themes the front end has a palette for. Adding a theme means adding its name here
/// and its palette in the front end (a test there checks that both agree).
pub const THEMES: &[&str] = &["dark"];

/// The theme used until the user picks another.
pub const DEFAULT_THEME: &str = "dark";

/// What the settings file holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    pub theme: Option<String>,
    /// Keys that differ from the built-in map: `Some(line)` binds, `None` takes the key off.
    pub keys: Vec<(String, Option<String>)>,
}

impl Settings {
    /// Sets or removes a key's entry; the order of the others is kept.
    fn set_key(&mut self, key: &str, line: Option<Option<String>>) {
        self.keys.retain(|(k, _)| k != key);
        if let Some(line) = line {
            self.keys.push((key.to_string(), line));
        }
    }
}

/// Names of keys longer than a letter or a digit, as the front end reports them.
const KEY_NAMES: &[&str] = &[
    "ArrowLeft",
    "ArrowRight",
    "ArrowUp",
    "ArrowDown",
    "PageUp",
    "PageDown",
    "Home",
    "End",
    "Insert",
    "Delete",
    "Backspace",
    "Enter",
    "Escape",
    "Space",
    "Tab",
    "Minus",
    "Plus",
    "Equals",
    "Slash",
    "Backslash",
    "Comma",
    "Period",
    "Semicolon",
    "Colon",
    "Quote",
    "Backtick",
    "OpenBracket",
    "CloseBracket",
];

/// A key as the user may type it (`ctrl+k`, `alt+f5`, `shift+5`) in the form the front end
/// reports (`Ctrl+K`, `Alt+F5`, `Shift+Num5`): modifiers in the order Ctrl, Alt, Shift.
pub fn normalize_key(text: &str) -> Result<String, EngineError> {
    let bad = || {
        EngineError(format!(
            "niepoprawny klawisz: {text:?} (np. F5, Ctrl+K, Alt+ArrowLeft)"
        ))
    };
    let parts: Vec<&str> = text.split('+').map(str::trim).collect();
    let (key, modifiers) = parts.split_last().ok_or_else(bad)?;
    if key.is_empty() {
        return Err(bad());
    }
    let (mut ctrl, mut alt, mut shift) = (false, false, false);
    for modifier in modifiers {
        match modifier.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => ctrl = true,
            "alt" => alt = true,
            "shift" => shift = true,
            _ => return Err(bad()),
        }
    }
    let key = if key.chars().count() == 1 {
        let c = key.chars().next().expect("one char");
        if c.is_ascii_digit() {
            format!("Num{c}")
        } else if c.is_ascii_alphabetic() {
            c.to_ascii_uppercase().to_string()
        } else {
            return Err(bad());
        }
    } else if key.len() > 1
        && key.starts_with(['f', 'F'])
        && key[1..].chars().all(|c| c.is_ascii_digit())
    {
        format!("F{}", &key[1..])
    } else if let Some(known) = KEY_NAMES.iter().find(|n| n.eq_ignore_ascii_case(key)) {
        (*known).to_string()
    } else if key.len() > 3
        && key[..3].eq_ignore_ascii_case("num")
        && key[3..].chars().all(|c| c.is_ascii_digit())
    {
        format!("Num{}", &key[3..])
    } else {
        return Err(bad());
    };
    let mut name = String::new();
    if ctrl {
        name.push_str("Ctrl+");
    }
    if alt {
        name.push_str("Alt+");
    }
    if shift {
        name.push_str("Shift+");
    }
    name.push_str(&key);
    Ok(name)
}

/// True when `line` is something a key may run: a command, or a command name followed by a
/// space (the command line is filled in for the user to complete).
pub fn is_valid_binding(line: &str) -> bool {
    if line.ends_with(' ') {
        line.split_whitespace()
            .next()
            .is_some_and(|word| COMMAND_NAMES.contains(&word))
    } else {
        Command::parse(line).is_ok()
    }
}

/// Reads a settings file's text. Returns the settings and a warning for every line skipped.
pub fn parse(text: &str) -> (Settings, Vec<String>) {
    let mut settings = Settings::default();
    let mut warnings = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let number = index + 1;
        let line = raw.trim_start();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((left, right)) = line.split_once('=') else {
            warnings.push(format!("wiersz {number}: brak znaku „=”"));
            continue;
        };
        let (left, right) = (left.trim(), right.trim_start());
        if left == "theme" {
            let name = right.trim().to_ascii_lowercase();
            if THEMES.contains(&name.as_str()) {
                settings.theme = Some(name);
            } else {
                warnings.push(format!(
                    "wiersz {number}: nie ma motywu „{name}” (dostępne: {})",
                    THEMES.join(", ")
                ));
            }
        } else if let Some(key) = left.strip_prefix("key ") {
            let key = match normalize_key(key) {
                Ok(key) => key,
                Err(e) => {
                    warnings.push(format!("wiersz {number}: {}", e.0));
                    continue;
                }
            };
            let value = match right
                .strip_prefix('"')
                .and_then(|v| v.trim_end().strip_suffix('"'))
            {
                Some(quoted) => quoted.to_string(),
                None => right.trim_end().to_string(),
            };
            if value.is_empty() {
                settings.set_key(&key, Some(None));
            } else if is_valid_binding(&value) {
                settings.set_key(&key, Some(Some(value)));
            } else {
                warnings.push(format!("wiersz {number}: „{value}” nie jest komendą"));
            }
        } else {
            warnings.push(format!("wiersz {number}: nieznane ustawienie „{left}”"));
        }
    }
    (settings, warnings)
}

/// The text of a settings file.
pub fn render(settings: &Settings) -> String {
    let mut out = String::from(
        "# Ustawienia bluepane (edytuj ostrożnie; błędne wiersze są pomijane z ostrzeżeniem)\n",
    );
    if let Some(theme) = &settings.theme {
        out.push_str(&format!("theme = {theme}\n"));
    }
    for (key, line) in &settings.keys {
        match line {
            None => out.push_str(&format!("key {key} =\n")),
            Some(line) if line.ends_with(' ') => out.push_str(&format!("key {key} = \"{line}\"\n")),
            Some(line) => out.push_str(&format!("key {key} = {line}\n")),
        }
    }
    out
}

/// The built-in key map with the user's changes applied.
pub fn keymap_with(settings: &Settings) -> Keymap {
    let mut map = Keymap::default();
    for (key, line) in &settings.keys {
        match line {
            Some(line) => map.bind(key.clone(), line.clone()),
            None => map.unbind(key),
        }
    }
    map
}

/// The settings in a file, with warnings; a missing file is not an error.
pub(crate) fn load(path: &Path) -> (Settings, Vec<String>) {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(_) => (Settings::default(), Vec::new()),
    }
}

impl Session {
    /// The colour theme in use (a name from [`THEMES`]).
    pub fn theme(&self) -> &str {
        &self.theme
    }

    /// Takes the saved settings (theme and keys) into use; returns the warnings for lines
    /// that could not be understood.
    pub(crate) fn use_settings(&mut self, settings: Settings) {
        if let Some(theme) = &settings.theme {
            self.theme = theme.clone();
        }
        self.keymap = keymap_with(&settings);
        self.settings = settings;
    }

    /// Writes the settings file (if there is a config directory).
    fn save_settings(&self) -> Result<(), EngineError> {
        let Some(dir) = self.config_dir() else {
            return Ok(());
        };
        std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::write(dir.join("settings.conf"), render(&self.settings)))
            .map_err(|e| EngineError(format!("zmiana działa, ale nie udało się jej zapisać: {e}")))
    }

    /// `theme` shows the current theme and the available ones; `theme <name>` switches.
    pub(crate) fn set_theme(&mut self, name: Option<String>) -> Result<(), EngineError> {
        let Some(name) = name else {
            self.status = format!("Motyw: {} (dostępne: {})", self.theme, THEMES.join(", "));
            return Ok(());
        };
        let name = name.to_ascii_lowercase();
        if !THEMES.contains(&name.as_str()) {
            return Err(EngineError(format!(
                "nie ma motywu „{name}” (dostępne: {})",
                THEMES.join(", ")
            )));
        }
        self.theme = name.clone();
        self.settings.theme = Some(name.clone());
        self.status = format!("Motyw: {name}");
        self.save_settings()
    }

    /// `bind` lists the changes, `bind <klawisz>` shows a key, `bind <klawisz> <komenda>`
    /// binds, `bind <klawisz> default` brings the built-in binding back.
    pub(crate) fn bind_key(
        &mut self,
        args: Option<(String, Option<String>)>,
    ) -> Result<(), EngineError> {
        let Some((key, line)) = args else {
            let count = self.settings.keys.len();
            self.status = if count == 0 {
                "Skróty: wszystkie domyślne. Zmiana: bind <klawisz> <komenda>, np. bind Ctrl+K sort name".into()
            } else {
                let list: Vec<String> = self
                    .settings
                    .keys
                    .iter()
                    .map(|(k, l)| format!("{k} = {}", l.as_deref().unwrap_or("(wyłączony)")))
                    .collect();
                format!("Własne skróty: {}", list.join(" · "))
            };
            return Ok(());
        };
        let key = normalize_key(&key)?;
        let Some(line) = line else {
            self.status = match self.keymap.lookup(&key) {
                Some(line) => format!("{key} = {line}"),
                None => format!("{key}: nic nie jest przypisane"),
            };
            return Ok(());
        };
        let replaced = self.keymap.lookup(&key).map(str::to_owned);
        if line == "default" {
            self.settings.set_key(&key, None);
            self.keymap = keymap_with(&self.settings);
            self.status = format!(
                "{key}: skrót domyślny ({})",
                self.keymap.lookup(&key).unwrap_or("brak")
            );
        } else {
            if !is_valid_binding(&line) {
                return Err(EngineError(format!(
                    "„{line}” nie jest komendą (lista: bluepane-cli commands)"
                )));
            }
            self.settings.set_key(&key, Some(Some(line.clone())));
            self.keymap.bind(key.clone(), line.clone());
            self.status = match replaced {
                Some(old) if old != line => format!("{key} = {line} (było: {old})"),
                _ => format!("{key} = {line}"),
            };
        }
        self.save_settings()
    }

    /// `unbind <klawisz>`: the key does nothing any more.
    pub(crate) fn unbind_key(&mut self, key: &str) -> Result<(), EngineError> {
        let key = normalize_key(key)?;
        self.settings.set_key(&key, Some(None));
        self.keymap.unbind(&key);
        self.status = format!("{key}: wyłączony (przywrócenie: bind {key} default)");
        self.save_settings()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_normalised_to_the_names_the_front_end_reports() {
        for (typed, wanted) in [
            ("f5", "F5"),
            ("ctrl+k", "Ctrl+K"),
            ("SHIFT+alt+ctrl+a", "Ctrl+Alt+Shift+A"),
            ("alt+arrowleft", "Alt+ArrowLeft"),
            ("pagedown", "PageDown"),
            ("ctrl+num3", "Ctrl+Num3"),
            ("Alt+ArrowLeft", "Alt+ArrowLeft"),
            ("ctrl+5", "Ctrl+Num5"),
            ("escape", "Escape"),
            ("Ctrl + L", "Ctrl+L"),
        ] {
            assert_eq!(normalize_key(typed).unwrap(), wanted, "{typed}");
        }
        for bad in ["", "ctrl+", "meta+k", "+", "ctrl+?", "ctrl+banana"] {
            assert!(normalize_key(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_file_round_trips_and_bad_lines_are_skipped_with_warnings() {
        let text =
            "# komentarz\ntheme = dark\nkey ctrl+k = sort name\nkey F9 =\nkey Ctrl+L = \"cd \"\n\
                    key Ctrl+J = nonsense\nkey = x\ncolour = blue\ntheme = neon\nbroken line\n";
        let (settings, warnings) = parse(text);
        assert_eq!(settings.theme.as_deref(), Some("dark"));
        assert_eq!(
            settings.keys,
            vec![
                ("Ctrl+K".to_string(), Some("sort name".to_string())),
                ("F9".to_string(), None),
                ("Ctrl+L".to_string(), Some("cd ".to_string())),
            ]
        );
        assert_eq!(warnings.len(), 5, "{warnings:?}");
        assert!(warnings.iter().any(|w| w.contains("nie jest komendą")));
        assert!(warnings.iter().any(|w| w.contains("nieznane ustawienie")));
        let again = parse(&render(&settings));
        assert_eq!(again.0, settings);
        assert!(again.1.is_empty(), "{:?}", again.1);
    }

    #[test]
    fn the_keymap_is_the_default_one_with_the_changes_on_top() {
        let (settings, _) = parse("key F9 =\nkey F5 = move\nkey Ctrl+K = sort name\n");
        let map = keymap_with(&settings);
        assert_eq!(map.lookup("F9"), None);
        assert_eq!(map.lookup("F5"), Some("move"));
        assert_eq!(map.lookup("Ctrl+K"), Some("sort name"));
        assert_eq!(map.lookup("F8"), Some("delete"), "the rest stays");
    }

    #[test]
    fn the_theme_command_parses_and_unknown_names_are_refused() {
        assert_eq!(Command::parse("theme"), Ok(Command::Theme(None)));
        assert_eq!(
            Command::parse("theme Dark"),
            Ok(Command::Theme(Some("Dark".into())))
        );
        let mut s = Session::new("/tmp", "/tmp");
        assert_eq!(s.theme(), DEFAULT_THEME);
        s.run_line("theme").unwrap();
        assert!(s.status.contains("dark"), "{}", s.status);
        let err = s.run_line("theme neon").unwrap_err();
        assert!(err.0.contains("dostępne: dark"), "{err}");
        s.run_line("theme DARK").unwrap();
        assert_eq!(s.theme(), "dark");
    }

    #[test]
    fn bind_and_unbind_change_the_keys_and_survive_a_restart() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("config");
        let mut s = Session::new(tmp.path(), tmp.path());
        s.use_config_dir(&config);
        s.run_line("theme dark").unwrap();
        s.run_line("bind ctrl+k sort name").unwrap();
        assert_eq!(s.keymap.lookup("Ctrl+K"), Some("sort name"));
        s.run_line("bind F5 move").unwrap();
        assert!(s.status.contains("było: copy"), "{}", s.status);
        s.run_line("unbind f9").unwrap();
        assert_eq!(s.keymap.lookup("F9"), None);
        // Showing a key and the list change nothing.
        s.run_line("bind ctrl+k").unwrap();
        assert!(s.status.contains("sort name"), "{}", s.status);
        s.run_line("bind").unwrap();
        assert!(s.status.contains("Własne skróty"), "{}", s.status);
        assert!(s.run_line("bind Ctrl+J fly").is_err());
        assert!(s.run_line("bind meta+x copy").is_err());

        // A new session reads the same file: keys and theme are both there.
        let mut again = Session::new(tmp.path(), tmp.path());
        again.use_config_dir(&config);
        assert_eq!(again.keymap.lookup("Ctrl+K"), Some("sort name"));
        assert_eq!(again.keymap.lookup("F5"), Some("move"));
        assert_eq!(again.keymap.lookup("F9"), None);
        assert_eq!(again.theme(), "dark");

        // `default` brings the built-in binding back, and that is saved too.
        again.run_line("bind F5 default").unwrap();
        assert_eq!(again.keymap.lookup("F5"), Some("copy"));
        again.run_line("bind F9 default").unwrap();
        assert_eq!(again.keymap.lookup("F9"), Some("refresh"));
        let mut third = Session::new(tmp.path(), tmp.path());
        third.use_config_dir(&config);
        assert_eq!(third.keymap.lookup("F5"), Some("copy"));
        assert_eq!(third.keymap.lookup("Ctrl+K"), Some("sort name"));
    }

    #[test]
    fn a_damaged_settings_file_still_starts_and_reports_what_it_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("config");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(
            config.join("settings.conf"),
            "key Ctrl+J = nonsense\ntheme = dark\n",
        )
        .unwrap();
        let mut s = Session::new(tmp.path(), tmp.path());
        s.use_config_dir(&config);
        assert_eq!(s.theme(), "dark");
        assert!(s.status.contains("settings.conf"), "{}", s.status);
        assert!(s.status.contains("nie jest komendą"), "{}", s.status);
        assert_eq!(s.keymap.lookup("Ctrl+J"), Some("queue"), "default kept");
    }
}
