//! Small user settings kept between runs (`settings.conf` in the config directory):
//! for now the name of the colour theme. The engine only keeps the name; the palettes
//! belong to the front end.

use std::path::Path;

use crate::session::Session;
use crate::EngineError;

/// The themes the front end has a palette for. Adding a theme means adding its name here
/// and its palette in the front end (a test there checks that both agree).
pub const THEMES: &[&str] = &["dark"];

/// The theme used until the user picks another.
pub const DEFAULT_THEME: &str = "dark";

/// The theme named in a settings file, if the file has a valid one.
pub(crate) fn load_theme(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        let value = value.trim().to_ascii_lowercase();
        (key.trim() == "theme" && THEMES.contains(&value.as_str())).then_some(value)
    })
}

fn save_theme(path: &Path, theme: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, format!("theme = {theme}\n"))
}

impl Session {
    /// The colour theme in use (a name from [`THEMES`]).
    pub fn theme(&self) -> &str {
        &self.theme
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
        self.status = format!("Motyw: {name}");
        if let Some(dir) = self.config_dir() {
            if let Err(e) = save_theme(&dir.join("settings.conf"), &name) {
                return Err(EngineError(format!(
                    "motyw zmieniony, ale nie udało się go zapisać: {e}"
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;

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
    fn the_theme_is_remembered_in_the_config_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("config");
        let mut s = Session::new(tmp.path(), tmp.path());
        s.use_config_dir(&config);
        s.run_line("theme dark").unwrap();
        assert_eq!(
            std::fs::read_to_string(config.join("settings.conf")).unwrap(),
            "theme = dark\n"
        );
        assert_eq!(
            load_theme(&config.join("settings.conf")).as_deref(),
            Some("dark")
        );
        // A damaged or unknown value is ignored instead of breaking the start.
        std::fs::write(config.join("settings.conf"), "theme = purple\n").unwrap();
        assert_eq!(load_theme(&config.join("settings.conf")), None);
        let mut again = Session::new(tmp.path(), tmp.path());
        again.use_config_dir(&config);
        assert_eq!(again.theme(), DEFAULT_THEME);
    }
}
