//! `details`: everything the program can read about one file, shown in the viewer as text: the
//! basics (size, dates, permissions) and, for photos, the EXIF fields (camera, exposure, date,
//! GPS...) and, for audio, its properties (length, bitrate...) and tags. Nothing is written.

use crate::session::Session;
use crate::{format_mode, format_modified, human_size, EngineError};

impl Session {
    /// The file the command is about: the one open in the viewer, else the one under the cursor.
    fn details_target(&self) -> Result<std::path::PathBuf, EngineError> {
        if self.preview_dir.is_none() {
            if let Some(viewer) = &self.viewer {
                return Ok(viewer.path().to_path_buf());
            }
        }
        let panel = self.active_panel();
        // A list of search results holds ordinary files; a server or an archive does not.
        if panel.remote.is_some() || panel.archive.is_some() {
            return Err(EngineError(
                "szczegóły pliku działają dla plików na dysku".into(),
            ));
        }
        let entry = panel
            .current()
            .filter(|e| e.name != "..")
            .ok_or_else(|| EngineError("nic nie wskazano".into()))?;
        if entry.is_dir() {
            return Err(EngineError(format!("to katalog, nie plik: {}", entry.name)));
        }
        Ok(entry.path.clone())
    }

    pub(crate) fn file_details(&mut self) -> Result<(), EngineError> {
        let path = self.details_target()?;
        let entry = bluepane_vfs::entry_of(&path)
            .map_err(|e| EngineError(format!("{}: {e}", path.display())))?;
        let mut lines: Vec<(String, String)> = vec![
            ("Nazwa".into(), entry.name.clone()),
            ("Ścieżka".into(), path.display().to_string()),
            (
                "Rozmiar".into(),
                format!("{} ({} bajtów)", human_size(entry.size), entry.size),
            ),
            ("Zmieniony".into(), format_modified(entry.modified)),
            ("Utworzony".into(), format_modified(entry.created)),
        ];
        if entry.mode.is_some() {
            lines.push(("Prawa".into(), format_mode(entry.mode)));
        }
        if let Some(owner) = &entry.owner {
            lines.push(("Właściciel".into(), owner.clone()));
        }
        let media = bluepane_rename::describe(&path);
        let width = lines
            .iter()
            .chain(&media)
            .map(|(label, _)| label.chars().count())
            .max()
            .unwrap_or(0);
        let mut text = String::new();
        for (label, value) in &lines {
            text.push_str(&format!("{label:<width$}  {value}\n"));
        }
        if media.is_empty() {
            text.push_str("\n(brak danych EXIF ani tagów dźwiękowych w tym pliku)\n");
        } else {
            text.push('\n');
            for (label, value) in &media {
                text.push_str(&format!("{label:<width$}  {value}\n"));
            }
        }
        self.show_git_text("szczegóły.txt", &text)?;
        self.status = format!("Szczegóły: {} (Esc zamyka)", entry.name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> (tempfile::TempDir, Session) {
        let tmp = tempfile::tempdir().unwrap();
        let (left, right) = (tmp.path().join("left"), tmp.path().join("right"));
        std::fs::create_dir_all(&left).unwrap();
        std::fs::create_dir_all(&right).unwrap();
        let s = Session::new(&left, &right);
        (tmp, s)
    }

    fn shown(s: &mut Session) -> String {
        let viewer = s.viewer.as_mut().expect("the details open in the viewer");
        viewer.set_viewport(60, 120);
        viewer
            .visible_rows()
            .iter()
            .map(|r| r.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_plain_file_shows_its_basics_and_says_it_has_no_media_data() {
        let (tmp, mut s) = session();
        std::fs::write(tmp.path().join("left/zażółć gęślą.txt"), "treść").unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("select zażółć gęślą.txt").unwrap();
        s.run_line("details").unwrap();
        let text = shown(&mut s);
        assert!(text.contains("zażółć gęślą.txt"), "{text}");
        assert!(
            text.contains("Rozmiar") && text.contains("(7 bajtów)"),
            "{text}"
        );
        assert!(text.contains("brak danych EXIF"), "{text}");
        s.run_line("viewer close").unwrap();
        assert!(s.viewer.is_none());
    }

    #[test]
    fn folders_and_nothing_under_the_cursor_are_errors() {
        let (tmp, mut s) = session();
        std::fs::create_dir_all(tmp.path().join("left/katalog")).unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("select katalog").unwrap();
        assert!(s.run_line("details").unwrap_err().0.contains("katalog"));
    }
}
