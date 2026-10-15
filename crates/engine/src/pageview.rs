//! Documents shown as pages (PDF, and Word / Excel / OpenDocument through LibreOffice).
//!
//! The engine only prepares pictures: the window turns them into textures. Rendering runs on
//! its own threads, so turning a page never freezes the interface.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use bluepane_docs::{PageImage, PdfDocument};

use crate::session::Session;
use crate::EngineError;

/// How big a page is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageZoom {
    /// As wide as the window.
    Fit,
    /// A percentage of the page's natural size (100 = 96 dots per inch).
    Percent(u32),
}

impl PageZoom {
    pub const MIN: u32 = 25;
    pub const MAX: u32 = 400;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageState {
    /// LibreOffice is turning the document into a PDF.
    Converting,
    Ready,
    Failed(String),
}

/// A rendered page and what it was rendered for.
#[derive(Debug, Clone)]
pub struct RenderedPage {
    pub page: usize,
    /// The width in pixels it was asked for.
    pub width: u32,
    pub image: Arc<PageImage>,
}

#[derive(Debug)]
struct Shared {
    state: PageState,
    doc: Option<PdfDocument>,
    rendered: Option<RenderedPage>,
    /// The picture being drawn right now: (page, width).
    rendering: Option<(usize, u32)>,
}

/// Where a navigation command goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageMove {
    Next,
    Prev,
    First,
    Last,
    /// A page number, 1-based.
    To(usize),
}

/// How the zoom changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomChange {
    In,
    Out,
    Fit,
    Percent(u32),
}

#[derive(Debug)]
pub struct PageViewer {
    /// The file that was asked for (the PDF itself, or the document that was converted).
    pub source: PathBuf,
    /// 0-based.
    pub page: usize,
    pub zoom: PageZoom,
    /// For a converted document: show its text when the conversion fails.
    fallback_to_text: bool,
    shared: Arc<Mutex<Shared>>,
    cancel: Arc<AtomicBool>,
}

impl Drop for PageViewer {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl PageViewer {
    fn new(source: PathBuf, state: PageState, doc: Option<PdfDocument>, fallback: bool) -> Self {
        PageViewer {
            source,
            page: 0,
            zoom: PageZoom::Fit,
            fallback_to_text: fallback,
            shared: Arc::new(Mutex::new(Shared {
                state,
                doc,
                rendered: None,
                rendering: None,
            })),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A PDF file, read now (opening it only parses the page list).
    pub fn from_pdf(source: &Path, bytes: Vec<u8>) -> Result<PageViewer, EngineError> {
        let doc = PdfDocument::open(bytes).map_err(|e| EngineError(e.to_string()))?;
        Ok(PageViewer::from_document(source, doc))
    }

    /// An already opened PDF.
    pub fn from_document(source: &Path, doc: PdfDocument) -> PageViewer {
        PageViewer::new(source.to_path_buf(), PageState::Ready, Some(doc), false)
    }

    /// A document that LibreOffice turns into a PDF in `scratch` on a thread of its own.
    pub fn converting(source: &Path, program: PathBuf, scratch: PathBuf) -> PageViewer {
        let viewer = PageViewer::new(source.to_path_buf(), PageState::Converting, None, true);
        let (shared, cancel, source) = (
            viewer.shared.clone(),
            viewer.cancel.clone(),
            source.to_path_buf(),
        );
        std::thread::spawn(move || {
            let result = bluepane_docs::to_pdf(&program, &source, &scratch, &|| {
                cancel.load(Ordering::Relaxed)
            })
            .and_then(|pdf| {
                let bytes = std::fs::read(&pdf)?;
                PdfDocument::open(bytes)
            });
            let mut shared = shared.lock().unwrap_or_else(|e| e.into_inner());
            match result {
                Ok(doc) => {
                    shared.doc = Some(doc);
                    shared.state = PageState::Ready;
                }
                Err(error) => shared.state = PageState::Failed(error.to_string()),
            }
        });
        viewer
    }

    pub fn state(&self) -> PageState {
        self.lock().state.clone()
    }

    pub fn pages(&self) -> usize {
        self.lock().doc.as_ref().map_or(0, PdfDocument::pages)
    }

    /// The current page's size in points, when known.
    pub fn page_size(&self) -> Option<(f32, f32)> {
        self.lock().doc.as_ref()?.page_size(self.page)
    }

    pub fn go(&mut self, target: PageMove) {
        let pages = self.pages();
        if pages == 0 {
            return;
        }
        let last = pages - 1;
        self.page = match target {
            PageMove::Next => (self.page + 1).min(last),
            PageMove::Prev => self.page.saturating_sub(1),
            PageMove::First => 0,
            PageMove::Last => last,
            PageMove::To(n) => n.saturating_sub(1).min(last),
        };
    }

    pub fn change_zoom(&mut self, change: ZoomChange) {
        let current = match self.zoom {
            PageZoom::Fit => 100,
            PageZoom::Percent(p) => p,
        };
        self.zoom = match change {
            ZoomChange::Fit => PageZoom::Fit,
            ZoomChange::In => {
                PageZoom::Percent((current * 5 / 4).clamp(PageZoom::MIN, PageZoom::MAX))
            }
            ZoomChange::Out => {
                PageZoom::Percent((current * 4 / 5).clamp(PageZoom::MIN, PageZoom::MAX))
            }
            ZoomChange::Percent(p) => PageZoom::Percent(p.clamp(PageZoom::MIN, PageZoom::MAX)),
        };
    }

    /// Asks for the current page at this width (pixels); call every frame. Starts a render
    /// when the picture on hand is for another page or width.
    pub fn want(&self, width: u32) {
        let page = self.page;
        let mut shared = self.lock();
        let Some(doc) = shared.doc.clone() else {
            return;
        };
        if shared
            .rendered
            .as_ref()
            .is_some_and(|r| r.page == page && r.width == width)
            || shared.rendering == Some((page, width))
            || shared.rendering.is_some()
        {
            return;
        }
        shared.rendering = Some((page, width));
        drop(shared);
        let shared = self.shared.clone();
        std::thread::spawn(move || {
            let result = doc.render(page, width);
            let mut shared = shared.lock().unwrap_or_else(|e| e.into_inner());
            shared.rendering = None;
            match result {
                Ok(image) => {
                    shared.rendered = Some(RenderedPage {
                        page,
                        width,
                        image: Arc::new(image),
                    });
                }
                Err(error) => shared.state = PageState::Failed(error.to_string()),
            }
        });
    }

    /// The newest picture (it may be for the previous page or width for a moment).
    pub fn rendered(&self) -> Option<RenderedPage> {
        self.lock().rendered.clone()
    }

    pub fn is_rendering(&self) -> bool {
        let shared = self.lock();
        shared.rendering.is_some() || shared.state == PageState::Converting
    }
}

impl Session {
    /// Opens a PDF file of the panel; a protected one makes the interface ask for the
    /// password (`password` is what was typed, `None` the first time).
    pub(crate) fn open_local_pdf(
        &mut self,
        path: &Path,
        password: Option<&str>,
    ) -> Result<(), EngineError> {
        let bytes = Self::read_pdf(path)?;
        match PdfDocument::open_with_password(bytes, password.unwrap_or("")) {
            Ok(doc) => {
                self.show_pages(PageViewer::from_document(path, doc));
                Ok(())
            }
            Err(bluepane_docs::DocError::Encrypted | bluepane_docs::DocError::WrongPassword)
                if self.pending.is_none() || password.is_some() =>
            {
                self.pending = Some(crate::session::Pending::Input {
                    kind: crate::session::InputKind::PdfPassword(path.to_path_buf()),
                    text: String::new(),
                });
                if password.is_some() {
                    // The dialog stays open; the error is shown in it.
                    return Err(EngineError(
                        "Błędne hasło: spróbuj ponownie (Esc = anuluj)".into(),
                    ));
                }
                self.status =
                    "Ten PDF jest chroniony hasłem: wpisz je (Enter = otwórz, Esc = anuluj)".into();
                Ok(())
            }
            Err(error) => Err(EngineError(error.to_string())),
        }
    }

    fn read_pdf(path: &Path) -> Result<Vec<u8>, EngineError> {
        let size = std::fs::metadata(path)
            .map_err(|e| EngineError(format!("{}: {e}", path.display())))?
            .len();
        if size > bluepane_docs::MAX_PDF {
            return Err(EngineError(format!(
                "plik ma {} MB, za duży na podgląd (limit {} MB): otwórz go w zewnętrznym programie (F4)",
                size / (1024 * 1024),
                bluepane_docs::MAX_PDF / (1024 * 1024)
            )));
        }
        std::fs::read(path).map_err(|e| EngineError(format!("{}: {e}", path.display())))
    }

    /// Opens a document through LibreOffice when it is installed; `false` when it is not
    /// (the caller then shows the text).
    pub(crate) fn open_office_pages(&mut self, path: &Path) -> bool {
        let Some(program) = bluepane_docs::office_program() else {
            return false;
        };
        let scratch = std::env::temp_dir().join(format!(
            "bluepane-view-{}-{}",
            std::process::id(),
            crate::session::next_preview_number()
        ));
        self.remove_preview_dir();
        self.preview_dir = Some(scratch.clone());
        self.show_pages(PageViewer::converting(path, program, scratch));
        true
    }

    /// Like [`open_office_pages`](Self::open_office_pages) for a file that lies in `dir`, a
    /// folder the caller owns (and removes with the preview): the conversion happens inside it.
    fn open_office_pages_in(&mut self, path: &Path, dir: &Path) -> bool {
        let Some(program) = bluepane_docs::office_program() else {
            return false;
        };
        self.show_pages(PageViewer::converting(path, program, dir.join("pages")));
        true
    }

    /// A document that was fetched into `dir` (out of an archive or from a server) for F3:
    /// PDF and, with LibreOffice, Office documents as pages; anything else as text. `origin`
    /// is "z archiwum" or "z serwera". Returns the status line and whether it is an error.
    pub(crate) fn show_fetched_document(
        &mut self,
        file: &Path,
        dir: PathBuf,
        origin: &str,
    ) -> (String, bool) {
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let kind = bluepane_docs::kind_of(file);
        if kind.is_some() {
            self.remove_preview_dir();
            self.preview_dir = Some(dir.clone());
            let opened = if kind == Some(bluepane_docs::Kind::Pdf) {
                self.open_local_pdf(file, None).map(|()| true)
            } else {
                Ok(self.open_office_pages_in(file, &dir))
            };
            match opened {
                // A protected PDF: the password question is on screen, keep its wording.
                Ok(true) if self.pending.is_some() => return (self.status.clone(), false),
                Ok(true) => return (format!("Podgląd {origin}: {name}"), false),
                // No LibreOffice: the text of the document below.
                Ok(false) => {}
                Err(error) => {
                    self.remove_preview_dir();
                    return (format!("Podgląd {origin} nie powiódł się: {error}"), true);
                }
            }
        }
        match crate::documents::viewer_for_file(file, &dir) {
            Ok(viewer) => {
                self.remove_preview_dir();
                self.preview_dir = Some(dir);
                self.viewer = Some(viewer);
                (format!("Podgląd {origin}: {name}"), false)
            }
            Err(error) => {
                self.remove_preview_dir();
                let _ = std::fs::remove_dir_all(&dir);
                (format!("Podgląd {origin} nie powiódł się: {error}"), true)
            }
        }
    }

    fn show_pages(&mut self, viewer: PageViewer) {
        let name = viewer
            .source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.status = match viewer.state() {
            PageState::Converting => format!("Konwersja dokumentu do podglądu: {name}…"),
            _ => format!("Podgląd stron: {name}"),
        };
        self.pages = Some(viewer);
    }

    /// Called every frame: a conversion that failed falls back to the text of the document.
    pub(crate) fn poll_pages(&mut self) {
        let Some(viewer) = &self.pages else { return };
        let PageState::Failed(error) = viewer.state() else {
            return;
        };
        let (source, fallback) = (viewer.source.clone(), viewer.fallback_to_text);
        self.pages = None;
        if fallback {
            // Show the text instead; the scratch folder of the conversion is reused.
            let scratch = self.preview_dir.clone().unwrap_or_else(std::env::temp_dir);
            match crate::documents::viewer_for_file(&source, &scratch) {
                Ok(viewer) => {
                    self.viewer = Some(viewer);
                    self.status = format!("Strony niedostępne ({error}); pokazuję tekst dokumentu");
                    self.status_is_error = false;
                    return;
                }
                Err(text_error) => {
                    self.status = format!("Podgląd nie powiódł się: {text_error}");
                }
            }
        } else {
            self.status = format!("Podgląd nie powiódł się: {error}");
        }
        self.status_is_error = true;
        self.remove_preview_dir();
    }

    pub(crate) fn page_move(&mut self, target: PageMove) -> Result<(), EngineError> {
        let viewer = self.pages.as_mut().ok_or_else(|| {
            EngineError("podgląd stron nie jest otwarty (F3 na PDF lub dokumencie)".into())
        })?;
        viewer.go(target);
        Ok(())
    }

    pub(crate) fn page_zoom(&mut self, change: ZoomChange) -> Result<(), EngineError> {
        let viewer = self.pages.as_mut().ok_or_else(|| {
            EngineError("podgląd stron nie jest otwarty (F3 na PDF lub dokumencie)".into())
        })?;
        viewer.change_zoom(change);
        Ok(())
    }

    pub(crate) fn page_close(&mut self) {
        self.pages = None;
        self.remove_preview_dir();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::Duration;

    /// A PDF with `pages` pages, each saying "Page N".
    fn pdf(pages: usize) -> Vec<u8> {
        let mut objects: Vec<String> = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".into(),
            format!(
                "<< /Type /Pages /Kids [{}] /Count {pages} >>",
                (0..pages)
                    .map(|i| format!("{} 0 R", 4 + i * 2))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        ];
        for i in 0..pages {
            let stream = format!("BT /F1 24 Tf 72 500 Td (Page {}) Tj ET", i + 1);
            objects.push(format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 600] /Contents {} 0 R /Resources << /Font << /F1 3 0 R >> >> >>",
                5 + i * 2
            ));
            objects.push(format!(
                "<< /Length {} >>\nstream\n{stream}\nendstream",
                stream.len()
            ));
        }
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for offset in offsets {
            out.extend(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        out
    }

    fn wait_for_picture(viewer: &PageViewer, width: u32) -> RenderedPage {
        for _ in 0..1000 {
            viewer.want(width);
            if let Some(rendered) = viewer.rendered() {
                if rendered.page == viewer.page && rendered.width == width {
                    return rendered;
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("the page was not rendered");
    }

    #[test]
    fn a_protected_pdf_asks_for_the_password_and_opens_with_the_right_one() {
        use crate::session::{InputKind, Pending};
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::write(
            left.join("locked.pdf"),
            include_bytes!("../../docs/tests/data/protected.pdf"),
        )
        .unwrap();
        let mut s = Session::new(&left, tmp.path().join("right"));
        s.run_line("select locked.pdf").unwrap();
        s.run_line("view").unwrap();
        assert!(s.pages.is_none());
        assert!(
            matches!(
                &s.pending,
                Some(Pending::Input {
                    kind: InputKind::PdfPassword(_),
                    ..
                })
            ),
            "{}",
            s.status
        );
        // Nothing typed: the dialog stays.
        assert!(s.run_line("confirm").is_err());
        assert!(s.pending.is_some());
        // A wrong password asks again and says so.
        if let Some(Pending::Input { text, .. }) = &mut s.pending {
            *text = "nope".into();
        }
        assert!(s.run_line("confirm").is_err());
        assert!(s.pages.is_none());
        assert!(s.pending.is_some());
        assert!(s.status_is_error, "{}", s.status);
        assert!(s.status.contains("Błędne hasło"), "{}", s.status);
        // The right one opens the pages.
        if let Some(Pending::Input { text, .. }) = &mut s.pending {
            *text = "sekret".into();
        }
        s.run_line("confirm").unwrap();
        assert!(s.pending.is_none());
        assert_eq!(s.pages.as_ref().unwrap().pages(), 1);
        // Escape on the dialog opens nothing.
        s.run_line("pageclose").unwrap();
        s.run_line("view").unwrap();
        assert!(s.pending.is_some());
        s.run_line("cancel").unwrap();
        assert!(s.pending.is_none() && s.pages.is_none());
    }

    #[test]
    fn a_protected_pdf_fetched_from_an_archive_or_server_asks_for_the_password_too() {
        use crate::session::{InputKind, Pending};
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("fetched");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("locked.pdf");
        std::fs::write(&file, include_bytes!("../../docs/tests/data/protected.pdf")).unwrap();
        let mut s = Session::new(tmp.path(), tmp.path());
        let (status, is_error) = s.show_fetched_document(&file, dir.clone(), "z serwera");
        assert!(!is_error, "{status}");
        assert!(status.contains("hasłem"), "{status}");
        assert!(matches!(
            &s.pending,
            Some(Pending::Input {
                kind: InputKind::PdfPassword(_),
                ..
            })
        ));
        // The fetched copy must survive the question.
        assert!(file.exists());
        if let Some(Pending::Input { text, .. }) = &mut s.pending {
            *text = "sekret".into();
        }
        s.run_line("confirm").unwrap();
        assert_eq!(s.pages.as_ref().unwrap().pages(), 1);
        s.run_line("pageclose").unwrap();
        assert!(!dir.exists(), "the fetched copy goes away with the preview");
    }

    #[test]
    fn a_fetched_word_document_shows_its_text_without_libreoffice() {
        std::env::set_var("BLUEPANE_NO_OFFICE", "1");
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("fetched");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("note.rtf");
        std::fs::write(&file, "{\\rtf1 Hello from the server\\par}").unwrap();
        let mut s = Session::new(tmp.path(), tmp.path());
        let (status, is_error) = s.show_fetched_document(&file, dir.clone(), "z archiwum");
        assert!(!is_error, "{status}");
        assert!(status.contains("z archiwum"), "{status}");
        assert!(s.viewer.is_some() && s.pages.is_none());
        s.remove_preview_dir();
        assert!(!dir.exists());
    }

    #[test]
    fn a_pdf_opens_as_pages_that_can_be_turned_and_zoomed() {
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::write(left.join("book.pdf"), pdf(3)).unwrap();
        let mut s = Session::new(&left, tmp.path().join("right"));
        s.run_line("select book.pdf").unwrap();
        s.run_line("view").unwrap();
        assert!(s.viewer.is_none());
        let viewer = s.pages.as_ref().unwrap();
        assert_eq!(viewer.pages(), 3);
        assert_eq!(viewer.page, 0);
        assert_eq!(viewer.page_size(), Some((400.0, 600.0)));
        let first = wait_for_picture(viewer, 200);
        assert_eq!((first.image.width, first.image.height), (200, 300));

        s.run_line("page next").unwrap();
        s.run_line("page next").unwrap();
        s.run_line("page next").unwrap();
        assert_eq!(s.pages.as_ref().unwrap().page, 2, "stops at the last page");
        s.run_line("page first").unwrap();
        assert_eq!(s.pages.as_ref().unwrap().page, 0);
        s.run_line("page prev").unwrap();
        assert_eq!(s.pages.as_ref().unwrap().page, 0);
        s.run_line("page 2").unwrap();
        assert_eq!(s.pages.as_ref().unwrap().page, 1);
        s.run_line("page last").unwrap();
        let second = wait_for_picture(s.pages.as_ref().unwrap(), 200);
        assert_eq!(second.page, 2);
        assert_ne!(
            second.image.rgba, first.image.rgba,
            "another page, another picture"
        );

        s.run_line("pagezoom 200").unwrap();
        assert_eq!(s.pages.as_ref().unwrap().zoom, PageZoom::Percent(200));
        s.run_line("pagezoom in").unwrap();
        assert_eq!(s.pages.as_ref().unwrap().zoom, PageZoom::Percent(250));
        s.run_line("pagezoom out").unwrap();
        s.run_line("pagezoom 9999").unwrap();
        assert_eq!(
            s.pages.as_ref().unwrap().zoom,
            PageZoom::Percent(PageZoom::MAX)
        );
        s.run_line("pagezoom fit").unwrap();
        assert_eq!(s.pages.as_ref().unwrap().zoom, PageZoom::Fit);

        let dir = std::env::temp_dir();
        s.run_line("pageclose").unwrap();
        assert!(s.pages.is_none());
        assert!(s.preview_dir.is_none());
        assert!(dir.exists());
        assert!(s.run_line("page next").is_err(), "nothing open");
        assert!(s.run_line("pagezoom fit").is_err());
    }

    #[test]
    fn bad_page_commands_are_errors_and_keys_are_bound() {
        let (_tmp, mut s) = {
            let tmp = tempfile::tempdir().unwrap();
            let s = Session::new(tmp.path(), tmp.path());
            (tmp, s)
        };
        for bad in ["page", "page 0", "page x", "pagezoom", "pagezoom big"] {
            assert!(s.run_line(bad).is_err(), "{bad}");
        }
        assert_eq!(s.keymap.lookup_pages("PageDown"), Some("page next"));
        assert_eq!(s.keymap.lookup_pages("Escape"), Some("pageclose"));
        assert_eq!(s.keymap.lookup_pages("F12"), None);
    }

    #[test]
    fn a_damaged_or_huge_pdf_is_an_error_not_a_blank_window() {
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::write(left.join("bad.pdf"), "%PDF-1.4 nonsense").unwrap();
        let mut s = Session::new(&left, tmp.path().join("right"));
        s.run_line("select bad.pdf").unwrap();
        assert!(s.run_line("view").is_err());
        assert!(s.pages.is_none());
    }

    #[test]
    fn when_the_conversion_fails_the_text_of_the_document_is_shown() {
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left");
        std::fs::create_dir_all(&left).unwrap();
        let docx = left.join("letter.docx");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&docx).unwrap());
        zip.start_file(
            "word/document.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"<w:document><w:body><w:p><w:r><w:t>Dear reader</w:t></w:r></w:p></w:body></w:document>")
            .unwrap();
        zip.finish().unwrap();
        let mut s = Session::new(&left, tmp.path().join("right"));
        // A program that does not exist stands for "LibreOffice failed".
        let scratch = tmp.path().join("scratch");
        s.preview_dir = Some(scratch.clone());
        s.pages = Some(PageViewer::converting(
            &docx,
            PathBuf::from("/definitely/not/libreoffice"),
            scratch,
        ));
        for _ in 0..1000 {
            s.poll();
            if s.viewer.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(s.pages.is_none());
        assert!(s.viewer.is_some(), "{}", s.status);
        assert!(s.status.contains("tekst dokumentu"), "{}", s.status);
        let text = std::fs::read_to_string(s.preview_dir.as_ref().unwrap().join("letter.docx.txt"))
            .unwrap();
        assert!(text.contains("Dear reader"));
    }
}
