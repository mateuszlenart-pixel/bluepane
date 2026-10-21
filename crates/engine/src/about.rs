//! `about`: the version, the licence and where things live, shown in the viewer. The same
//! numbers are printed by `bluepane --version`.

use crate::session::Session;
use crate::EngineError;

/// The version of this build (one number for the whole workspace, see `Cargo.toml`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The licence of the program.
pub const LICENSE: &str = env!("CARGO_PKG_LICENSE");

/// The project's home page.
pub const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// What `bluepane --version` prints.
pub fn version_line() -> String {
    format!("bluepane {VERSION}")
}

/// The text of the "about" window.
pub fn about_text(config_dir: Option<&std::path::Path>) -> String {
    let config = config_dir.map_or_else(|| "(brak)".to_string(), |d| d.display().to_string());
    format!(
        "bluepane {VERSION}\n\
         Dwupanelowy menedżer plików w stylu Total Commandera (Rust i egui).\n\
         \n\
         Licencja:       {LICENSE} (teksty w plikach LICENSE-MIT i LICENSE-APACHE)\n\
         Strona:         {REPOSITORY}\n\
         System:         {} ({})\n\
         Ustawienia:     {config}\n\
         \n\
         Lista użytych bibliotek i ich licencji: `cargo tree` w repozytorium,\n\
         pełne zestawienie jest w Cargo.lock.\n\
         \n\
         Bez samodzielnego sprawdzania aktualizacji: nowe wersje są na stronie projektu\n\
         w zakładce Releases.\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

impl Session {
    /// Shows the "about" text in the viewer (Esc closes it).
    pub(crate) fn show_about(&mut self) -> Result<(), EngineError> {
        let text = about_text(self.config_dir().as_deref());
        self.show_git_text("o-programie.txt", &text)?;
        self.status = format!("{} (Esc zamyka)", version_line());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;

    #[test]
    fn the_version_comes_from_the_workspace() {
        assert!(VERSION.split('.').count() == 3, "{VERSION}");
        assert_eq!(version_line(), format!("bluepane {VERSION}"));
        assert!(LICENSE.contains("MIT") && LICENSE.contains("Apache"));
        assert!(REPOSITORY.starts_with("https://"), "repo: {REPOSITORY:?}");
    }

    #[test]
    fn the_text_names_version_licence_and_config_folder() {
        let text = about_text(Some(std::path::Path::new("/x/ustawienia")));
        assert!(text.starts_with(&format!("bluepane {VERSION}")));
        assert!(text.contains("MIT OR Apache-2.0") && text.contains("ustawienia"));
        assert!(about_text(None).contains("(brak)"));
    }

    #[test]
    fn the_about_command_opens_the_text_in_the_viewer() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("l")).unwrap();
        std::fs::create_dir_all(tmp.path().join("r")).unwrap();
        let mut s = Session::new(tmp.path().join("l"), tmp.path().join("r"));
        s.use_config_dir(&tmp.path().join("config"));
        s.run_line("about").unwrap();
        let text = std::fs::read_to_string(s.viewer.as_ref().unwrap().path()).unwrap();
        assert!(text.contains(VERSION) && text.contains("Licencja"));
        assert!(s.status.contains(VERSION));
        assert_eq!(Command::parse("about").unwrap(), Command::About);
    }
}
