//! Quick View (Ctrl+Q): the other panel shows the file under the cursor of the active one,
//! live, instead of its own list: text or hex of any file, the first page of a PDF, the text
//! of a document, how many things a folder holds. Local files only; nothing is changed.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::command::{Command, QuickAction};
use crate::pageview::{PageMove, PageViewer};
use crate::panel::is_parent_entry;
use crate::session::Session;
use crate::viewer::Viewer;
use crate::{EngineError, FileClass};

/// How long the cursor must rest on a file before it is previewed, so that holding an arrow
/// key does not open every file on the way.
pub const QUICK_DELAY: Duration = Duration::from_millis(150);

/// Longest text shown for a document (lines).
const MAX_LINES: usize = 3000;

/// What the preview panel shows.
#[derive(Debug)]
pub enum QuickContent {
    /// Any file, read piece by piece (hex for binary files).
    Text(Viewer),
    /// The text of a document, already extracted; `top` is the first visible line.
    Lines { lines: Vec<String>, top: usize },
    /// A PDF as pictures of its pages.
    Pages(PageViewer),
    /// Nothing to show, and why.
    Note(String),
}

/// What is shown and for which path; `None` path means a note about something else.
#[derive(Debug)]
pub struct QuickView {
    pub key: QuickKey,
    pub title: String,
    pub content: QuickContent,
}

/// What the cursor points at, as far as the preview cares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickKey {
    File(PathBuf),
    Dir(PathBuf),
    /// A note that does not depend on a file.
    Other(String),
}

impl Session {
    /// What the preview should show for the active panel's cursor.
    fn quick_key(&self) -> QuickKey {
        let panel = self.active_panel();
        if panel.remote.is_some() || panel.archive.is_some() {
            return QuickKey::Other(
                "Quick View działa tylko na plikach lokalnych (nie na serwerze ani w archiwum)"
                    .into(),
            );
        }
        match panel.current() {
            None => QuickKey::Other("Nic nie wskazano".into()),
            Some(entry) if is_parent_entry(entry) => {
                QuickKey::Other("Wskaż plik lub katalog, żeby zobaczyć podgląd".into())
            }
            Some(entry) if entry.is_dir() => QuickKey::Dir(entry.path.clone()),
            Some(entry) => QuickKey::File(entry.path.clone()),
        }
    }

    /// Ctrl+Q and `quick`.
    pub(crate) fn quick_action(&mut self, action: QuickAction) -> Result<(), EngineError> {
        match action {
            QuickAction::Toggle => {
                if self.quick.take().is_some() {
                    self.quick_want = None;
                    self.status = "Quick View wyłączony".into();
                } else {
                    let key = self.quick_key();
                    self.quick = Some(build(&key));
                    self.status =
                        "Quick View: drugi panel pokazuje plik pod kursorem (Ctrl+Q zamyka)".into();
                }
            }
            QuickAction::Scroll(delta) => match &mut self.quick {
                Some(QuickView {
                    content: QuickContent::Text(viewer),
                    ..
                }) => viewer.scroll(delta),
                Some(QuickView {
                    content: QuickContent::Lines { lines, top },
                    ..
                }) => {
                    let last = lines.len().saturating_sub(1) as i64;
                    *top = (*top as i64 + delta).clamp(0, last) as usize;
                }
                Some(_) => {}
                None => return Err(EngineError("Quick View nie jest włączony (Ctrl+Q)".into())),
            },
            QuickAction::Page(delta) => match &mut self.quick {
                Some(QuickView {
                    content: QuickContent::Pages(viewer),
                    ..
                }) => viewer.go(if delta >= 0 {
                    PageMove::Next
                } else {
                    PageMove::Prev
                }),
                Some(_) => {}
                None => return Err(EngineError("Quick View nie jest włączony (Ctrl+Q)".into())),
            },
        }
        Ok(())
    }

    /// Called every frame: when the cursor has rested on a new file, previews it.
    pub(crate) fn update_quick(&mut self) {
        let Some(shown) = &self.quick else { return };
        let key = self.quick_key();
        if shown.key == key {
            self.quick_want = None;
            return;
        }
        match &self.quick_want {
            Some((wanted, since)) if *wanted == key => {
                if since.elapsed() >= self.quick_delay {
                    self.quick = Some(build(&key));
                    self.quick_want = None;
                }
            }
            _ => self.quick_want = Some((key, Instant::now())),
        }
    }

    /// True while a new preview is waiting for the cursor to settle (keep redrawing).
    pub fn quick_pending(&self) -> bool {
        self.quick_want.is_some()
    }
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn note(key: &QuickKey, title: String, text: impl Into<String>) -> QuickView {
    QuickView {
        key: key.clone(),
        title,
        content: QuickContent::Note(text.into()),
    }
}

/// The preview of what `key` names.
pub(crate) fn build(key: &QuickKey) -> QuickView {
    match key {
        QuickKey::Other(text) => note(key, "Quick View".into(), text.clone()),
        QuickKey::Dir(path) => {
            let count = std::fs::read_dir(path).map(|r| r.count());
            let text = match count {
                Ok(n) => format!("Katalog: {n} elementów"),
                Err(e) => format!("Nie można odczytać katalogu: {e}"),
            };
            note(key, name_of(path), text)
        }
        QuickKey::File(path) => build_file(key, path),
    }
}

fn build_file(key: &QuickKey, path: &Path) -> QuickView {
    let title = name_of(path);
    let size = std::fs::metadata(path).map_or(0, |m| m.len());
    if bluepane_docs::is_picture(path) {
        if let Ok(viewer) = PageViewer::from_picture(path) {
            return QuickView {
                key: key.clone(),
                title,
                content: QuickContent::Pages(viewer),
            };
        }
    }
    let kind = bluepane_docs::kind_of(path);
    if kind == Some(bluepane_docs::Kind::Pdf) {
        if size > bluepane_docs::MAX_PDF {
            return note(
                key,
                title,
                "PDF jest za duży na szybki podgląd (F3 otwiera go w całości)",
            );
        }
        return match std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| PageViewer::from_pdf(path, bytes).map_err(|e| e.0))
        {
            Ok(viewer) => QuickView {
                key: key.clone(),
                title,
                content: QuickContent::Pages(viewer),
            },
            Err(error) => note(key, title, error),
        };
    }
    if kind.is_some() {
        return match bluepane_docs::extract(path) {
            Ok(doc) => QuickView {
                key: key.clone(),
                title,
                content: QuickContent::Lines {
                    lines: doc
                        .text
                        .lines()
                        .take(MAX_LINES)
                        .map(str::to_string)
                        .collect(),
                    top: 0,
                },
            },
            Err(error) => note(key, title, error.to_string()),
        };
    }
    let class = crate::class_of_name(&title);
    if matches!(
        class,
        FileClass::Image | FileClass::Audio | FileClass::Video
    ) {
        return note(
            key,
            title,
            format!(
                "Plik multimedialny, {}: bez podglądu (F3 pokaże zawartość jako hex)",
                crate::human_size(size)
            ),
        );
    }
    match Viewer::open(path) {
        Ok(viewer) => QuickView {
            key: key.clone(),
            title,
            content: QuickContent::Text(viewer),
        },
        Err(error) => note(key, title, error.0),
    }
}

impl Command {
    pub(crate) fn parse_quick(rest: &str) -> Result<Command, EngineError> {
        let usage =
            || EngineError("quick: użycie: quick [scroll <wiersze> | page <1 albo -1>]".into());
        let mut words = rest.split_whitespace();
        let action = match (words.next(), words.next()) {
            (None, _) => QuickAction::Toggle,
            (Some("scroll"), Some(n)) => QuickAction::Scroll(n.parse().map_err(|_| usage())?),
            (Some("page"), Some(n)) => QuickAction::Page(n.parse().map_err(|_| usage())?),
            _ => return Err(usage()),
        };
        Ok(Command::Quick(action))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Session) {
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left");
        std::fs::create_dir_all(left.join("folder")).unwrap();
        std::fs::write(left.join("folder/inner.txt"), "x").unwrap();
        std::fs::write(left.join("a.txt"), "first line\nsecond line\n").unwrap();
        std::fs::write(left.join("b.bin"), [0u8, 1, 2, 3, 255]).unwrap();
        std::fs::write(
            left.join("doc.pdf"),
            include_bytes!("../../docs/tests/data/protected.pdf"),
        )
        .unwrap();
        std::fs::write(left.join("song.mp3"), "not really audio").unwrap();
        let mut s = Session::new(&left, tmp.path().join("right"));
        s.quick_delay = Duration::ZERO;
        (tmp, s)
    }

    fn shown(s: &Session) -> &QuickView {
        s.quick.as_ref().expect("quick view is on")
    }

    #[test]
    fn it_toggles_and_follows_the_cursor_after_a_short_wait() {
        let (_tmp, mut s) = setup();
        assert!(s.quick.is_none());
        s.run_line("quick").unwrap();
        assert!(
            matches!(shown(&s).content, QuickContent::Note(_)),
            "on `..`"
        );
        // Put the cursor on a.txt (the row after `..`, folders first: folder, a.txt, ...).
        let index = s.panels[0]
            .entries
            .iter()
            .position(|e| e.name == "a.txt")
            .unwrap();
        s.panels[0].cursor = index;
        s.poll();
        assert!(
            s.quick_pending(),
            "the new file is waiting for the cursor to settle"
        );
        s.poll();
        let QuickContent::Text(viewer) = &shown(&s).content else {
            panic!("{:?}", shown(&s).content);
        };
        assert!(viewer.path().ends_with("a.txt"));
        assert_eq!(shown(&s).title, "a.txt");
        assert!(!s.quick_pending());
        s.run_line("quick").unwrap();
        assert!(s.quick.is_none());
    }

    #[test]
    fn each_kind_of_item_gets_its_own_preview() {
        let (_tmp, mut s) = setup();
        s.run_line("quick").unwrap();
        let preview = |s: &mut Session, name: &str| -> String {
            let index = s.panels[0]
                .entries
                .iter()
                .position(|e| e.name == name)
                .unwrap();
            s.panels[0].cursor = index;
            s.poll();
            s.poll();
            match &shown(s).content {
                QuickContent::Text(_) => "text".into(),
                QuickContent::Lines { .. } => "lines".into(),
                QuickContent::Pages(v) => format!("pages {}", v.pages()),
                QuickContent::Note(text) => format!("note: {text}"),
            }
        };
        assert_eq!(preview(&mut s, "a.txt"), "text");
        assert_eq!(preview(&mut s, "b.bin"), "text", "binary files open in hex");
        assert_eq!(preview(&mut s, "folder"), "note: Katalog: 1 elementów");
        assert!(preview(&mut s, "song.mp3").starts_with("note: Plik multimedialny"));
        // The sample PDF is password protected: a note, not a crash or a blank panel.
        assert!(preview(&mut s, "doc.pdf").starts_with("note:"));
    }

    #[test]
    fn scrolling_works_on_text_and_is_refused_when_off() {
        let (_tmp, mut s) = setup();
        assert!(s.run_line("quick scroll 3").is_err());
        s.run_line("quick").unwrap();
        let index = s.panels[0]
            .entries
            .iter()
            .position(|e| e.name == "a.txt")
            .unwrap();
        s.panels[0].cursor = index;
        s.poll();
        s.poll();
        s.run_line("quick scroll 1").unwrap();
        s.run_line("quick scroll -5").unwrap();
        s.run_line("quick page 1").unwrap();
        assert!(s.run_line("quick scroll x").is_err());
        assert!(s.run_line("quick sideways").is_err());
    }

    #[test]
    fn a_document_shows_its_text() {
        let (tmp, mut s) = setup();
        std::fs::write(
            tmp.path().join("left/note.rtf"),
            "{\\rtf1 Hello quick\\par second}",
        )
        .unwrap();
        s.run_line("refresh").unwrap();
        s.run_line("quick").unwrap();
        let index = s.panels[0]
            .entries
            .iter()
            .position(|e| e.name == "note.rtf")
            .unwrap();
        s.panels[0].cursor = index;
        s.poll();
        s.poll();
        let QuickContent::Lines { lines, .. } = &shown(&s).content else {
            panic!("{:?}", shown(&s).content);
        };
        assert_eq!(lines[0], "Hello quick");
    }
}
