//! F3 on a document (Word, OpenDocument, Excel, PDF): its text is extracted into a plain text
//! file in a scratch folder and shown in the viewer. Everything else opens as before.

use std::path::Path;

use crate::session::Session;
use crate::{EngineError, Viewer};

/// A viewer for `file`. A document is converted to text inside `scratch` first (the folder
/// is created if needed; the caller removes it when the viewer is closed).
pub(crate) fn viewer_for_file(file: &Path, scratch: &Path) -> Result<Viewer, EngineError> {
    if bluepane_docs::kind_of(file).is_none() {
        return Viewer::open(file);
    }
    let text = bluepane_docs::extract(file).map_err(|e| EngineError(e.to_string()))?;
    std::fs::create_dir_all(scratch)
        .map_err(|e| EngineError(format!("nie udało się przygotować podglądu: {e}")))?;
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "dokument".into());
    let out = scratch.join(format!("{name}.txt"));
    std::fs::write(&out, text.text)
        .map_err(|e| EngineError(format!("nie udało się przygotować podglądu: {e}")))?;
    Viewer::open(&out)
}

impl Session {
    /// A viewer for a local file. For a document the converted text lives in a scratch folder
    /// that is removed with the next preview and when the program ends.
    pub(crate) fn viewer_for_local(&mut self, file: &Path) -> Result<Viewer, EngineError> {
        if bluepane_docs::kind_of(file).is_none() {
            return Viewer::open(file);
        }
        let scratch = std::env::temp_dir().join(format!(
            "bluepane-view-{}-{}",
            std::process::id(),
            crate::session::next_preview_number()
        ));
        match viewer_for_file(file, &scratch) {
            Ok(viewer) => {
                self.remove_preview_dir();
                self.preview_dir = Some(scratch);
                Ok(viewer)
            }
            Err(error) => {
                let _ = std::fs::remove_dir_all(&scratch);
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn docx(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("letter.docx");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        zip.start_file(
            "word/document.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(
            b"<w:document><w:body><w:p><w:r><w:t>Dear reader</w:t></w:r></w:p></w:body></w:document>",
        )
        .unwrap();
        zip.finish().unwrap();
        path
    }

    #[test]
    fn f3_on_a_document_shows_its_text_and_cleans_up() {
        // Without LibreOffice (which a machine running the tests may have) the text is shown.
        std::env::set_var("BLUEPANE_NO_OFFICE", "1");
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left");
        std::fs::create_dir_all(&left).unwrap();
        docx(&left);
        std::fs::write(left.join("plain.txt"), "plain text").unwrap();
        let mut s = Session::new(&left, tmp.path().join("right"));
        s.run_line("select letter.docx").unwrap();
        s.run_line("view").unwrap();
        assert!(s.viewer.is_some(), "{}", s.status);
        let dir = s.preview_dir.clone().unwrap();
        let shown = std::fs::read_to_string(dir.join("letter.docx.txt")).unwrap();
        assert!(shown.contains("Dear reader"), "{shown}");
        // A plain file opens as before and the scratch copy goes away.
        s.viewer = None;
        s.run_line("select plain.txt").unwrap();
        s.run_line("view").unwrap();
        assert!(s.viewer.is_some());
        s.remove_preview_dir();
        assert!(!dir.exists());
    }

    #[test]
    fn f3_on_an_rtf_file_shows_its_text() {
        std::env::set_var("BLUEPANE_NO_OFFICE", "1");
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::write(
            left.join("note.rtf"),
            "{\\rtf1\\ansi{\\fonttbl{\\f0 Arial;}}\\f0 Dear reader\\par Second line\\par}",
        )
        .unwrap();
        let mut s = Session::new(&left, tmp.path().join("right"));
        s.run_line("select note.rtf").unwrap();
        s.run_line("view").unwrap();
        let dir = s.preview_dir.clone().unwrap();
        let shown = std::fs::read_to_string(dir.join("note.rtf.txt")).unwrap();
        assert_eq!(shown, "Dear reader\nSecond line\n");
        s.remove_preview_dir();
    }

    #[test]
    fn a_broken_document_is_an_error_and_leaves_nothing_behind() {
        std::env::set_var("BLUEPANE_NO_OFFICE", "1");
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::write(left.join("bad.docx"), "not a zip").unwrap();
        let mut s = Session::new(&left, tmp.path().join("right"));
        s.run_line("select bad.docx").unwrap();
        let err = s.run_line("view").unwrap_err();
        assert!(err.0.contains("dokument"), "{err}");
        assert!(s.viewer.is_none() && s.preview_dir.is_none());
    }
}
