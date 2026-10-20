//! User settings kept between runs, in one file (`settings.conf` in the config directory):
//! the colour theme and the keys the user bound or took off.
//!
//! ```text
//! # comment
//! theme = dark
//! key Ctrl+K = sort name       (binds Ctrl+K)
//! key F9 =                     (takes F9 off: nothing happens)
//! key Ctrl+L = "cd "           (quotes keep a trailing space: the command line gets `cd `)
//! columns = ext size date      (the columns of the file list, beside the name)
//! density = compact            (rows of the file list: compact or comfortable)
//! textsize = 120               (text size in percent, 80 to 160)
//! rowheight = 26              (height of a list row in points, 16 to 40)
//! accent = #3d8bfd            (accent colour: cursor frame, focus, links)
//! quickview = tab              (Quick View in its own tab; `panel`: over the other panel)
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

/// A column of the file list beside the name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Column {
    /// The extension.
    Ext,
    Size,
    /// When the file was last changed.
    Modified,
    /// When the file was created (where the file system keeps it).
    Created,
}

impl Column {
    /// Every column, in the order they stand in a row (left to right after the name).
    pub const ALL: [Column; 4] = [Column::Ext, Column::Size, Column::Created, Column::Modified];

    pub fn word(self) -> &'static str {
        match self {
            Column::Ext => "ext",
            Column::Size => "size",
            Column::Modified => "date",
            Column::Created => "created",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Column::Ext => "Typ",
            Column::Size => "Rozmiar",
            Column::Modified => "Data",
            Column::Created => "Utworzono",
        }
    }

    pub fn from_word(word: &str) -> Option<Column> {
        Column::ALL
            .into_iter()
            .find(|c| c.word() == word.to_ascii_lowercase())
    }
}

/// The columns shown until the user picks others.
pub const DEFAULT_COLUMNS: [Column; 3] = [Column::Ext, Column::Size, Column::Modified];

/// How tall the rows of the file list are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}

impl Density {
    pub fn word(self) -> &'static str {
        match self {
            Density::Comfortable => "comfortable",
            Density::Compact => "compact",
        }
    }

    fn from_word(word: &str) -> Option<Density> {
        match word.to_ascii_lowercase().as_str() {
            "comfortable" | "wygodna" => Some(Density::Comfortable),
            "compact" | "zwarta" => Some(Density::Compact),
            _ => None,
        }
    }
}

/// Allowed text sizes (percent) and row heights (points).
pub const TEXT_SIZE: std::ops::RangeInclusive<u16> = 80..=160;
pub const ROW_HEIGHT: std::ops::RangeInclusive<u16> = 16..=40;

/// `#rrggbb` (the `#` is optional) as red, green, blue.
pub(crate) fn parse_color(text: &str) -> Option<[u8; 3]> {
    let hex = text.strip_prefix('#').unwrap_or(text);
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?])
}

/// What the settings file holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    pub theme: Option<String>,
    /// The list's columns beside the name, when the user changed them.
    pub columns: Option<Vec<Column>>,
    pub density: Option<Density>,
    /// Quick View in a tab of its own instead of in place of the other panel.
    pub quick_tab: Option<bool>,
    /// Text size in percent of the normal one.
    pub text_size: Option<u16>,
    /// Height of a list row in points (overrides `density`).
    pub row_height: Option<u16>,
    /// The accent colour as red, green, blue.
    pub accent: Option<[u8; 3]>,
    /// Put the last session back at start (default: yes).
    pub restore: Option<bool>,
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
        } else if left == "columns" {
            let mut columns = Vec::new();
            for word in right.split_whitespace() {
                match Column::from_word(word) {
                    Some(column) if !columns.contains(&column) => columns.push(column),
                    Some(_) => {}
                    None => warnings.push(format!(
                        "wiersz {number}: nie ma kolumny „{word}” (są: ext, size, date, created)"
                    )),
                }
            }
            settings.columns = Some(columns);
        } else if left == "textsize" {
            match right.trim().parse::<u16>() {
                Ok(n) if TEXT_SIZE.contains(&n) => settings.text_size = Some(n),
                _ => warnings.push(format!(
                    "wiersz {number}: textsize to liczba od {} do {}",
                    TEXT_SIZE.start(),
                    TEXT_SIZE.end()
                )),
            }
        } else if left == "rowheight" {
            match right.trim().parse::<u16>() {
                Ok(n) if ROW_HEIGHT.contains(&n) => settings.row_height = Some(n),
                _ => warnings.push(format!(
                    "wiersz {number}: rowheight to liczba od {} do {}",
                    ROW_HEIGHT.start(),
                    ROW_HEIGHT.end()
                )),
            }
        } else if left == "accent" {
            match parse_color(right.trim()) {
                Some(rgb) => settings.accent = Some(rgb),
                None => warnings.push(format!(
                    "wiersz {number}: accent to kolor w postaci #rrggbb, nie „{}”",
                    right.trim()
                )),
            }
        } else if left == "restore" {
            match right.trim() {
                "on" => settings.restore = Some(true),
                "off" => settings.restore = Some(false),
                other => warnings.push(format!(
                    "wiersz {number}: restore to on albo off, nie „{other}”"
                )),
            }
        } else if left == "quickview" {
            match right.trim() {
                "tab" => settings.quick_tab = Some(true),
                "panel" => settings.quick_tab = Some(false),
                other => warnings.push(format!(
                    "wiersz {number}: quickview to tab albo panel, nie „{other}”"
                )),
            }
        } else if left == "density" {
            match Density::from_word(right.trim()) {
                Some(density) => settings.density = Some(density),
                None => warnings.push(format!(
                    "wiersz {number}: gęstość to compact albo comfortable, nie „{}”",
                    right.trim()
                )),
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
    if let Some(columns) = &settings.columns {
        let words: Vec<&str> = columns.iter().map(|c| c.word()).collect();
        out.push_str(&format!("columns = {}\n", words.join(" ")));
    }
    if let Some(n) = settings.text_size {
        out.push_str(&format!("textsize = {n}\n"));
    }
    if let Some(n) = settings.row_height {
        out.push_str(&format!("rowheight = {n}\n"));
    }
    if let Some([r, g, b]) = settings.accent {
        out.push_str(&format!("accent = #{r:02x}{g:02x}{b:02x}\n"));
    }
    if let Some(on) = settings.restore {
        out.push_str(&format!("restore = {}\n", if on { "on" } else { "off" }));
    }
    if let Some(tab) = settings.quick_tab {
        out.push_str(&format!(
            "quickview = {}\n",
            if tab { "tab" } else { "panel" }
        ));
    }
    if let Some(density) = settings.density {
        out.push_str(&format!("density = {}\n", density.word()));
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
    pub(crate) fn save_settings(&self) -> Result<(), EngineError> {
        let Some(dir) = self.config_dir() else {
            return Ok(());
        };
        std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::write(dir.join("settings.conf"), render(&self.settings)))
            .map_err(|e| EngineError(format!("zmiana działa, ale nie udało się jej zapisać: {e}")))
    }

    /// The columns of the file list beside the name, in row order.
    pub fn columns(&self) -> Vec<Column> {
        let chosen = self.settings.columns.as_deref().unwrap_or(&DEFAULT_COLUMNS);
        Column::ALL
            .into_iter()
            .filter(|c| chosen.contains(c))
            .collect()
    }

    /// Quick View opens in a tab of its own (`quick tab`) rather than over the other panel.
    pub fn quick_in_tab(&self) -> bool {
        self.settings.quick_tab.unwrap_or(false)
    }

    /// Text size in percent (100 = normal).
    pub fn text_size(&self) -> u16 {
        self.settings.text_size.unwrap_or(100)
    }

    /// The row height the user set, if any (otherwise the density decides).
    pub fn row_height(&self) -> Option<u16> {
        self.settings.row_height
    }

    /// The accent colour the user set, if any (otherwise the theme's own).
    pub fn accent(&self) -> Option<[u8; 3]> {
        self.settings.accent
    }

    /// `appearance` shows the settings; `appearance textsize|rowheight|accent <value>|reset`
    /// sets one, `appearance reset` brings all back.
    pub(crate) fn set_appearance(&mut self, args: &str) -> Result<(), EngineError> {
        let usage = || {
            EngineError(
                "appearance: użycie: appearance [textsize <80-160>|rowheight <16-40>|accent <#rrggbb>|reset] [reset]"
                    .into(),
            )
        };
        let mut words = args.split_whitespace();
        match (words.next(), words.next(), words.next()) {
            (None, ..) => {}
            (Some("reset"), None, _) => {
                self.settings.text_size = None;
                self.settings.row_height = None;
                self.settings.accent = None;
            }
            (Some("textsize"), Some("reset"), None) => self.settings.text_size = None,
            (Some("rowheight"), Some("reset"), None) => self.settings.row_height = None,
            (Some("accent"), Some("reset"), None) => self.settings.accent = None,
            (Some("textsize"), Some(n), None) => {
                let n = n
                    .trim_end_matches('%')
                    .parse::<u16>()
                    .ok()
                    .filter(|n| TEXT_SIZE.contains(n));
                self.settings.text_size = Some(n.ok_or_else(usage)?);
            }
            (Some("rowheight"), Some(n), None) => {
                let n = n.parse::<u16>().ok().filter(|n| ROW_HEIGHT.contains(n));
                self.settings.row_height = Some(n.ok_or_else(usage)?);
            }
            (Some("accent"), Some(color), None) => {
                self.settings.accent = Some(parse_color(color).ok_or_else(usage)?);
            }
            _ => return Err(usage()),
        }
        let accent = self.accent().map_or("motywu".to_string(), |[r, g, b]| {
            format!("#{r:02x}{g:02x}{b:02x}")
        });
        let rows = self
            .row_height()
            .map_or("z gęstości".to_string(), |n| format!("{n} pt"));
        self.status = format!(
            "Wygląd: tekst {}%, wiersz {rows}, akcent {accent}",
            self.text_size()
        );
        self.save_settings()
    }

    pub fn density(&self) -> Density {
        self.settings.density.unwrap_or_default()
    }

    /// `columns` lists the columns, `columns <name>` shows or hides one, `columns default`
    /// brings the built-in set back.
    pub(crate) fn set_columns(&mut self, word: Option<String>) -> Result<(), EngineError> {
        let describe = |columns: &[Column]| {
            let shown: Vec<&str> = columns.iter().map(|c| c.label()).collect();
            if shown.is_empty() {
                "tylko nazwa".to_string()
            } else {
                format!("Nazwa, {}", shown.join(", "))
            }
        };
        let Some(word) = word else {
            self.status = format!(
                "Kolumny: {} (columns ext|size|date|created przełącza, columns default przywraca)",
                describe(&self.columns())
            );
            return Ok(());
        };
        if word.eq_ignore_ascii_case("default") {
            self.settings.columns = None;
        } else {
            let column = Column::from_word(&word).ok_or_else(|| {
                EngineError(format!(
                    "nie ma kolumny „{word}” (są: ext, size, date, created)"
                ))
            })?;
            let mut columns = self.columns();
            if let Some(at) = columns.iter().position(|c| *c == column) {
                columns.remove(at);
            } else {
                columns.push(column);
            }
            self.settings.columns = Some(columns);
        }
        self.status = format!("Kolumny: {}", describe(&self.columns()));
        self.save_settings()
    }

    /// `density` switches between compact and comfortable rows; with a word sets it.
    pub(crate) fn set_density(&mut self, word: Option<String>) -> Result<(), EngineError> {
        let density = match word {
            Some(word) => Density::from_word(&word).ok_or_else(|| {
                EngineError("density: użycie: density [compact|comfortable]".into())
            })?,
            None => match self.density() {
                Density::Comfortable => Density::Compact,
                Density::Compact => Density::Comfortable,
            },
        };
        self.settings.density = Some(density);
        self.status = match density {
            Density::Compact => "Gęstość listy: zwarta".into(),
            Density::Comfortable => "Gęstość listy: wygodna".into(),
        };
        self.save_settings()
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

    #[test]
    fn columns_and_density_are_parsed_rendered_and_kept_in_row_order() {
        let (settings, warnings) =
            parse("columns = date ext created bogus ext\ndensity = zwarta\ndensity = huge\n");
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert_eq!(
            settings.columns,
            Some(vec![Column::Modified, Column::Ext, Column::Created])
        );
        assert_eq!(settings.density, Some(Density::Compact));
        let again = parse(&render(&settings)).0;
        assert_eq!(again, settings);
    }

    #[test]
    fn appearance_settings_are_checked_saved_and_reset() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("l")).unwrap();
        let mut s = Session::new(tmp.path().join("l"), tmp.path().join("l"));
        s.use_config_dir(&tmp.path().join("config"));
        assert_eq!(
            (s.text_size(), s.row_height(), s.accent()),
            (100, None, None)
        );
        s.run_line("appearance textsize 120").unwrap();
        s.run_line("appearance rowheight 28").unwrap();
        s.run_line("appearance accent #FF8800").unwrap();
        assert!(s.run_line("appearance textsize 300").is_err());
        assert!(s.run_line("appearance rowheight 5").is_err());
        assert!(s.run_line("appearance accent red").is_err());
        assert!(s.run_line("appearance accent #12345").is_err());
        assert!(s.run_line("appearance accent #ąąąąąą").is_err());
        assert_eq!(
            (s.text_size(), s.row_height(), s.accent()),
            (120, Some(28), Some([255, 136, 0]))
        );

        let mut again = Session::new(tmp.path().join("l"), tmp.path().join("l"));
        again.use_config_dir(&tmp.path().join("config"));
        assert_eq!((again.text_size(), again.row_height()), (120, Some(28)));
        again.run_line("appearance accent reset").unwrap();
        assert_eq!(again.accent(), None);
        again.run_line("appearance reset").unwrap();
        assert_eq!((again.text_size(), again.row_height()), (100, None));
        again.run_line("appearance").unwrap();
        assert!(again.status.contains("100%"), "{}", again.status);
    }

    #[test]
    fn a_bad_appearance_line_in_the_file_is_skipped_with_a_warning() {
        let (settings, warnings) =
            parse("textsize = 500\nrowheight = abc\naccent = #zzzzzz\naccent = 3d8bfd\n");
        assert_eq!(settings.text_size, None);
        assert_eq!(settings.row_height, None);
        assert_eq!(settings.accent, Some([0x3d, 0x8b, 0xfd]));
        assert_eq!(warnings.len(), 3);
    }

    #[test]
    fn columns_and_density_commands_change_and_save_the_settings() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("l")).unwrap();
        let mut s = Session::new(tmp.path().join("l"), tmp.path().join("l"));
        s.use_config_dir(&tmp.path().join("config"));
        assert_eq!(s.columns(), DEFAULT_COLUMNS);
        assert_eq!(s.density(), Density::Comfortable);
        s.run_line("columns created").unwrap();
        // Shown in row order, whatever order they were switched on in.
        assert_eq!(
            s.columns(),
            [Column::Ext, Column::Size, Column::Created, Column::Modified]
        );
        s.run_line("columns ext").unwrap();
        assert!(!s.columns().contains(&Column::Ext));
        s.run_line("density").unwrap();
        assert_eq!(s.density(), Density::Compact);
        assert!(s.run_line("columns colour").is_err());
        assert!(s.run_line("density huge").is_err());

        let mut again = Session::new(tmp.path().join("l"), tmp.path().join("l"));
        again.use_config_dir(&tmp.path().join("config"));
        assert_eq!(
            again.columns(),
            [Column::Size, Column::Created, Column::Modified]
        );
        assert_eq!(again.density(), Density::Compact);
        again.run_line("columns default").unwrap();
        assert_eq!(again.columns(), DEFAULT_COLUMNS);
    }
}
