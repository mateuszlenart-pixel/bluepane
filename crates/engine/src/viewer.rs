//! File viewer (F3): shows a window of a file of any size.
//!
//! The viewer never reads the whole file. It keeps a byte offset, seeks there and reads only
//! as many bytes as the visible rows need, so opening a multi-gigabyte file is instant and
//! uses a few kilobytes. Text mode splits lines on `\n`; hex mode shows 16 bytes per row.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::highlight::{self, highlight_line, Lang, Span, SpanKind};
use crate::EngineError;

/// Longest line the viewer keeps in memory. Longer lines are cut for display (the rest is
/// skipped when scrolling), so a 1 GB single-line file cannot exhaust memory.
const MAX_LINE_BYTES: usize = 16 * 1024;
/// Block size for scanning backwards and for searching.
const BLOCK: usize = 64 * 1024;
/// How much of the file one search call scans before it reports progress and stops.
const SEARCH_LIMIT: u64 = 256 * 1024 * 1024;
const HEX_ROW: u64 = 16;
/// Tab stops in text mode.
const TAB_WIDTH: usize = 4;
/// Bytes inspected to decide between text and hex on open.
const SNIFF_BYTES: usize = 8 * 1024;
/// Colouring of a window needs to know whether a block comment is open at its top; that is
/// worked out by reading from the last known place, but not further than this.
const COLOUR_SCAN_MAX: u64 = 8 * 1024 * 1024;
/// A Markdown file larger than this is shown as text.
const MARKDOWN_MAX: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Text,
    Hex,
    /// Markdown shown as a reader sees it; the offset counts rendered rows, not bytes.
    Rendered,
}

/// One displayed row. `offset` is the byte position of its first byte in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub offset: u64,
    pub text: String,
    /// Colours of parts of `text` (characters), when it is code, Markdown or edited hex.
    pub spans: Vec<Span>,
}

/// What a search did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Found {
    At(u64),
    /// Nothing in the scanned part; the search can continue from here.
    Partial(u64),
    Nowhere,
}

#[derive(Debug, Clone)]
struct Search {
    needle: Vec<u8>,
    text: String,
    forward: bool,
    /// Where `next`/`prev` continue after an unfinished search.
    resume: Option<u64>,
}

#[derive(Debug)]
pub struct Viewer {
    path: PathBuf,
    file: File,
    size: u64,
    mode: ViewMode,
    offset: u64,
    rows: usize,
    cols: usize,
    hscroll: usize,
    cache: Option<(CacheKey, Vec<Row>)>,
    search: Option<Search>,
    last_match: Option<u64>,
    /// The language of the file, for colouring (none for plain text).
    lang: Option<&'static Lang>,
    /// Colouring is on (it is by default for a file with a known language).
    highlight: bool,
    /// Where a block comment was last known to be open or not: (offset of a line start, open).
    block_checkpoint: (u64, bool),
    /// The Markdown rendered for a width, as rows numbered from 0.
    markdown: Option<(usize, Vec<Row>)>,
    /// Bytes changed in the hex editor and not saved yet (offset → new value).
    edits: BTreeMap<u64, u8>,
    /// The hex editor is on: where the cursor is, and the first digit typed of a byte.
    editor: Option<Editor>,
    /// Raised on every change that affects what hex rows show.
    edit_version: u64,
    /// A copy with the changes written next to the file, waiting for the answer.
    saved_copy: Option<PathBuf>,
    /// The edit count at which closing was warned about, so asking twice closes.
    close_warned: Option<u64>,
    /// Result of the last search or error, for the status line.
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CacheKey {
    offset: u64,
    rows: usize,
    cols: usize,
    hscroll: usize,
    mode: ViewMode,
    highlight: bool,
    edit_version: u64,
    cursor: Option<u64>,
}

/// The hex editor's cursor.
#[derive(Debug, Clone, Copy)]
struct Editor {
    cursor: u64,
    /// The first hex digit of a byte being typed.
    high: Option<u8>,
}

impl Viewer {
    /// Opens a file. Files that look binary start in hex mode.
    pub fn open(path: &Path) -> Result<Viewer, EngineError> {
        let describe = |e: std::io::Error| EngineError(format!("{}: {e}", path.display()));
        let meta = std::fs::metadata(path).map_err(describe)?;
        if meta.is_dir() {
            return Err(EngineError(format!(
                "to katalog, nie plik: {}",
                path.display()
            )));
        }
        let mut file = File::open(path).map_err(describe)?;
        let size = file.metadata().map_err(describe)?.len();
        let mut sniff = vec![0u8; SNIFF_BYTES.min(size as usize)];
        file.read_exact(&mut sniff).map_err(describe)?;
        let mode = if sniff.contains(&0) {
            ViewMode::Hex
        } else if crate::markdown::is_markdown(path) && size <= MARKDOWN_MAX {
            ViewMode::Rendered
        } else {
            ViewMode::Text
        };
        Ok(Viewer {
            path: path.to_path_buf(),
            file,
            size,
            mode,
            offset: 0,
            rows: 24,
            cols: 100,
            hscroll: 0,
            cache: None,
            search: None,
            last_match: None,
            lang: highlight::lang_of(path),
            highlight: true,
            block_checkpoint: (0, false),
            markdown: None,
            edits: BTreeMap::new(),
            editor: None,
            edit_version: 0,
            saved_copy: None,
            close_warned: None,
            message: String::new(),
        })
    }

    /// The name of the language the text is coloured as, if it is.
    pub fn language(&self) -> Option<&'static str> {
        self.lang.filter(|_| self.highlight).map(|l| l.name)
    }

    /// Turns colouring on or off; false when the file has no known language.
    pub fn toggle_highlight(&mut self) -> bool {
        if self.lang.is_none() {
            self.message = "ten rodzaj pliku nie ma kolorowania składni".into();
            return false;
        }
        self.highlight = !self.highlight;
        self.message = if self.highlight {
            "kolorowanie składni włączone".into()
        } else {
            "kolorowanie składni wyłączone".into()
        };
        true
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn mode(&self) -> ViewMode {
        self.mode
    }

    /// Byte offset of the first visible row.
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Position in the file, 0 to 100.
    pub fn percent(&self) -> u64 {
        if let (ViewMode::Rendered, Some((_, rows))) = (self.mode, &self.markdown) {
            return (self.offset * 100)
                .checked_div(rows.len() as u64)
                .unwrap_or(100);
        }
        if self.size == 0 {
            100
        } else {
            (u128::from(self.offset) * 100 / u128::from(self.size)) as u64
        }
    }

    /// Offset of the last search hit, so the UI can highlight its row.
    pub fn match_offset(&self) -> Option<u64> {
        self.last_match
    }

    /// Tells the viewer how many rows and columns fit on screen.
    pub fn set_viewport(&mut self, rows: usize, cols: usize) {
        self.rows = rows.max(1);
        self.cols = cols.max(8);
    }

    /// The rows to draw. Cached until the position, size or mode changes.
    pub fn visible_rows(&mut self) -> &[Row] {
        let key = CacheKey {
            offset: self.offset,
            rows: self.rows,
            cols: self.cols,
            hscroll: self.hscroll,
            mode: self.mode,
            highlight: self.highlight,
            edit_version: self.edit_version,
            cursor: self.editor.map(|e| e.cursor),
        };
        if self.cache.as_ref().map(|(k, _)| *k) != Some(key) {
            let rows = match self.render() {
                Ok(rows) => rows,
                Err(e) => {
                    self.message = format!("błąd odczytu: {e}");
                    Vec::new()
                }
            };
            self.cache = Some((key, rows));
        }
        self.cache.as_ref().map_or(&[], |(_, rows)| rows.as_slice())
    }

    fn render(&mut self) -> std::io::Result<Vec<Row>> {
        match self.mode {
            ViewMode::Hex => self.render_hex(),
            ViewMode::Text => self.render_text(),
            ViewMode::Rendered => {
                self.ensure_markdown()?;
                let rows = self
                    .markdown
                    .as_ref()
                    .map_or(&[][..], |(_, r)| r.as_slice());
                let from = (self.offset as usize).min(rows.len());
                Ok(rows[from..(from + self.rows).min(rows.len())].to_vec())
            }
        }
    }

    /// Renders the Markdown for the current width unless that is already done.
    fn ensure_markdown(&mut self) -> std::io::Result<()> {
        if self.markdown.as_ref().is_some_and(|(w, _)| *w == self.cols) {
            return Ok(());
        }
        let mut bytes = Vec::new();
        File::open(&self.path)?
            .take(MARKDOWN_MAX)
            .read_to_end(&mut bytes)?;
        let text = match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(e) => bluepane_search::decode_cp1250(e.as_bytes()),
        };
        let rows = crate::markdown::render(&text, self.cols)
            .into_iter()
            .enumerate()
            .map(|(i, line)| Row {
                offset: i as u64,
                text: line.text,
                spans: line.spans,
            })
            .collect();
        self.markdown = Some((self.cols, rows));
        Ok(())
    }

    /// How many rows the rendered Markdown has (only in that mode, once drawn).
    pub fn rendered_rows(&self) -> Option<usize> {
        match (self.mode, &self.markdown) {
            (ViewMode::Rendered, Some((_, rows))) => Some(rows.len()),
            _ => None,
        }
    }

    /// Switches between the rendered and the plain text of a Markdown file.
    pub fn toggle_render(&mut self) {
        if !crate::markdown::is_markdown(&self.path) {
            self.message = "to nie jest plik Markdown".into();
            return;
        }
        if self.size > MARKDOWN_MAX {
            self.message = "plik jest za duży na renderowanie Markdown".into();
            return;
        }
        self.mode = if self.mode == ViewMode::Rendered {
            ViewMode::Text
        } else {
            ViewMode::Rendered
        };
        self.offset = 0;
        self.hscroll = 0;
    }

    fn render_hex(&mut self) -> std::io::Result<Vec<Row>> {
        let want = self.rows as u64 * HEX_ROW;
        let len = want.min(self.size.saturating_sub(self.offset));
        let mut bytes = vec![0u8; len as usize];
        self.file.seek(SeekFrom::Start(self.offset))?;
        self.file.read_exact(&mut bytes)?;
        // What was typed in the editor shows before it is saved.
        for (&at, &value) in self.edits.range(self.offset..self.offset + len) {
            bytes[(at - self.offset) as usize] = value;
        }
        let cursor = self.editor.map(|e| e.cursor);
        Ok(bytes
            .chunks(HEX_ROW as usize)
            .enumerate()
            .map(|(i, chunk)| {
                let row = self.offset + i as u64 * HEX_ROW;
                let mut spans = Vec::new();
                for k in 0..chunk.len() {
                    let at = row + k as u64;
                    let kind = if cursor == Some(at) {
                        SpanKind::Cursor
                    } else if self.edits.contains_key(&at) {
                        SpanKind::Modified
                    } else {
                        continue;
                    };
                    // Columns of the byte in the hex part and in the text part of the row.
                    let hex = 10 + 3 * k + usize::from(k >= 8);
                    spans.push(Span {
                        start: hex,
                        end: hex + 2,
                        kind,
                    });
                    spans.push(Span {
                        start: 61 + k,
                        end: 62 + k,
                        kind,
                    });
                }
                Row {
                    offset: row,
                    text: hex_line(row, chunk),
                    spans,
                }
            })
            .collect())
    }

    fn render_text(&mut self) -> std::io::Result<Vec<Row>> {
        let lang = self.lang.filter(|_| self.highlight);
        let mut open = match lang {
            Some(lang) => self.block_open_at(lang, self.offset)?,
            None => false,
        };
        let mut reader = BufReader::with_capacity(BLOCK, &self.file);
        reader.seek(SeekFrom::Start(self.offset))?;
        let mut rows = Vec::with_capacity(self.rows);
        let mut pos = self.offset;
        let limit = self.cols * 2;
        while rows.len() < self.rows && pos < self.size {
            let line = read_line_capped(&mut reader)?;
            let full = display_text(&line.bytes);
            let spans = match lang {
                Some(lang) => {
                    let (spans, still_open) = highlight_line(lang, &full, open);
                    open = still_open;
                    spans
                        .into_iter()
                        .filter_map(|s| {
                            // The window starts `hscroll` characters in and shows `limit`.
                            let (start, end) = (
                                s.start.saturating_sub(self.hscroll),
                                s.end.saturating_sub(self.hscroll).min(limit),
                            );
                            (start < end).then_some(Span { start, end, ..s })
                        })
                        .collect()
                }
                None => Vec::new(),
            };
            let shown: String = full.chars().skip(self.hscroll).take(limit).collect();
            rows.push(Row {
                offset: pos,
                text: shown,
                spans,
            });
            pos += line.consumed;
        }
        Ok(rows)
    }

    /// Whether a block comment is open at the start of the line at `offset`, worked out from
    /// the last place that was known (and from the start of the file when going back).
    fn block_open_at(&mut self, lang: &Lang, offset: u64) -> std::io::Result<bool> {
        if !lang.has_block_comments() || offset == 0 {
            return Ok(false);
        }
        let (mut at, mut open) = self.block_checkpoint;
        if at > offset {
            (at, open) = (0, false);
        }
        if offset - at > COLOUR_SCAN_MAX {
            // Too far to read: assume the code is not inside a comment.
            return Ok(false);
        }
        let mut reader = BufReader::with_capacity(BLOCK, &self.file);
        reader.seek(SeekFrom::Start(at))?;
        while at < offset {
            let line = read_line_capped(&mut reader)?;
            if line.consumed == 0 {
                break;
            }
            open = highlight_line(lang, &display_text(&line.bytes), open).1;
            at += line.consumed;
        }
        self.block_checkpoint = (at, open);
        Ok(open)
    }

    /// Scrolls by whole rows (negative = up).
    pub fn scroll(&mut self, delta: i64) {
        let result = match self.mode {
            ViewMode::Hex => {
                let max = self.max_hex_offset();
                let target = self.offset as i128 + i128::from(delta) * i128::from(HEX_ROW);
                self.offset = target.clamp(0, i128::from(max)) as u64;
                Ok(())
            }
            ViewMode::Text => self.scroll_text(delta),
            ViewMode::Rendered => self.ensure_markdown().map(|()| {
                let max = self.rendered_last_offset();
                self.offset =
                    (self.offset as i128 + i128::from(delta)).clamp(0, i128::from(max)) as u64;
            }),
        };
        if let Err(e) = result {
            self.message = format!("błąd odczytu: {e}");
        }
    }

    /// The first row of the last page of the rendered Markdown.
    fn rendered_last_offset(&self) -> u64 {
        self.markdown
            .as_ref()
            .map_or(0, |(_, rows)| rows.len().saturating_sub(self.rows) as u64)
    }

    fn scroll_text(&mut self, delta: i64) -> std::io::Result<()> {
        if delta >= 0 {
            let mut reader = BufReader::with_capacity(BLOCK, &self.file);
            reader.seek(SeekFrom::Start(self.offset))?;
            let mut pos = self.offset;
            for _ in 0..delta {
                // Never scroll past the last line.
                let line = read_line_capped(&mut reader)?;
                if pos + line.consumed >= self.size {
                    break;
                }
                pos += line.consumed;
            }
            self.offset = pos;
        } else {
            for _ in 0..delta.unsigned_abs() {
                if self.offset == 0 {
                    break;
                }
                self.offset = self.line_start_at_or_before(self.offset - 1)?;
            }
        }
        Ok(())
    }

    /// Scrolls by pages (a page is the visible rows minus one for context).
    pub fn page(&mut self, pages: i64) {
        let rows = self.rows.saturating_sub(1).max(1) as i64;
        self.scroll(pages * rows);
    }

    /// Scrolls sideways by characters (text mode).
    pub fn scroll_columns(&mut self, delta: i64) {
        self.hscroll = (self.hscroll as i64 + delta).max(0) as usize;
    }

    pub fn top(&mut self) {
        self.offset = 0;
        self.hscroll = 0;
    }

    /// Shows the last rows of the file.
    pub fn bottom(&mut self) {
        match self.mode {
            ViewMode::Hex => self.offset = self.max_hex_offset(),
            ViewMode::Rendered => {
                if self.ensure_markdown().is_ok() {
                    self.offset = self.rendered_last_offset();
                }
            }
            ViewMode::Text => {
                if let Err(e) = self.bottom_text() {
                    self.message = format!("błąd odczytu: {e}");
                }
            }
        }
    }

    fn bottom_text(&mut self) -> std::io::Result<()> {
        if self.size == 0 {
            self.offset = 0;
            return Ok(());
        }
        // The final newline ends the last line instead of starting an empty one.
        let mut last = [0u8; 1];
        self.file.seek(SeekFrom::Start(self.size - 1))?;
        self.file.read_exact(&mut last)?;
        let end = if last[0] == b'\n' {
            self.size - 1
        } else {
            self.size
        };
        let mut start = self.line_start_at_or_before(end)?;
        for _ in 1..self.rows {
            if start == 0 {
                break;
            }
            start = self.line_start_at_or_before(start - 1)?;
        }
        self.offset = start;
        Ok(())
    }

    /// Jumps to a byte offset (`1048576`) or a percentage (`50%`).
    pub fn goto(&mut self, spec: &str) -> Result<(), EngineError> {
        let spec = spec.trim();
        let bad =
            || EngineError("goto: oczekiwano przesunięcia w bajtach lub procentu, np. 50%".into());
        if self.mode == ViewMode::Rendered {
            self.ensure_markdown()
                .map_err(|e| EngineError(e.to_string()))?;
            let total = self.markdown.as_ref().map_or(0, |(_, r)| r.len() as u64);
            let row = match spec.strip_suffix('%') {
                Some(percent) => {
                    total * percent.trim().parse::<u64>().map_err(|_| bad())?.min(100) / 100
                }
                None => spec.parse::<u64>().map_err(|_| bad())?,
            };
            self.offset = row.min(self.rendered_last_offset());
            return Ok(());
        }
        let target = if let Some(percent) = spec.strip_suffix('%') {
            let percent: u64 = percent.trim().parse().map_err(|_| bad())?;
            (u128::from(self.size) * u128::from(percent.min(100)) / 100) as u64
        } else {
            spec.parse::<u64>().map_err(|_| bad())?.min(self.size)
        };
        self.offset = match self.mode {
            ViewMode::Hex => target / HEX_ROW * HEX_ROW,
            ViewMode::Rendered => unreachable!("handled above"),
            ViewMode::Text => self
                .line_start_at_or_before(target)
                .map_err(|e| EngineError(e.to_string()))?,
        };
        Ok(())
    }

    /// Switches between text and hex, keeping the position.
    pub fn toggle_mode(&mut self) {
        match self.mode {
            ViewMode::Text => {
                self.mode = ViewMode::Hex;
                self.offset = self.offset / HEX_ROW * HEX_ROW;
            }
            ViewMode::Hex => {
                self.mode = ViewMode::Text;
                self.offset = self.line_start_at_or_before(self.offset).unwrap_or(0);
            }
            ViewMode::Rendered => {
                self.mode = ViewMode::Hex;
                self.offset = 0;
            }
        }
        self.hscroll = 0;
    }

    fn max_hex_offset(&self) -> u64 {
        let rows = self.rows as u64 * HEX_ROW;
        self.size.saturating_sub(rows).div_ceil(HEX_ROW) * HEX_ROW
    }

    /// Start of the line that contains byte `pos` (the byte after the previous `\n`).
    fn line_start_at_or_before(&mut self, pos: u64) -> std::io::Result<u64> {
        let mut end = pos;
        let mut block = vec![0u8; BLOCK];
        while end > 0 {
            let start = end.saturating_sub(BLOCK as u64);
            let len = (end - start) as usize;
            self.file.seek(SeekFrom::Start(start))?;
            self.file.read_exact(&mut block[..len])?;
            if let Some(i) = block[..len].iter().rposition(|&b| b == b'\n') {
                return Ok(start + i as u64 + 1);
            }
            end = start;
        }
        Ok(0)
    }

    /// Starts a new search for `text` (case-insensitive for ASCII) and shows the first hit.
    pub fn find(&mut self, text: &str, forward: bool) {
        if text.is_empty() {
            self.message = "pusty tekst do szukania".into();
            return;
        }
        self.leave_rendered();
        let from = if forward {
            self.match_or_view_start().saturating_add(1)
        } else {
            self.match_or_view_start()
        };
        self.search = Some(Search {
            needle: text.as_bytes().to_vec(),
            text: text.to_string(),
            forward,
            resume: None,
        });
        self.run_search(from);
    }

    /// Whether closing may go ahead: at once when nothing is unsaved, else the first ask warns
    /// and the second (with nothing changed in between) throws the changes away.
    pub fn confirm_close(&mut self) -> bool {
        if self.edits.is_empty() {
            return true;
        }
        if self.close_warned == Some(self.edit_version) {
            return true;
        }
        self.close_warned = Some(self.edit_version);
        self.message = format!(
            "niezapisane zmiany: {} bajtów. Zamknij jeszcze raz, żeby je porzucić, albo Ctrl+S, żeby zapisać",
            self.edits.len()
        );
        false
    }

    /// Whether the hex editor is on.
    pub fn editing(&self) -> bool {
        self.editor.is_some()
    }

    /// Bytes changed and not yet saved.
    pub fn edit_count(&self) -> usize {
        self.edits.len()
    }

    /// A copy with the changes was written and waits for the answer (`save_confirm`).
    pub fn awaiting_save(&self) -> bool {
        self.saved_copy.is_some()
    }

    /// Turns the hex editor on (in hex mode) or off. Changes stay until saved or reverted.
    pub fn toggle_edit(&mut self) -> Result<(), EngineError> {
        if self.editor.is_some() {
            self.editor = None;
            self.edit_version += 1;
            self.message = if self.edits.is_empty() {
                "edytor hex wyłączony".into()
            } else {
                format!(
                    "edytor hex wyłączony; niezapisane zmiany: {} (viewer save)",
                    self.edits.len()
                )
            };
            return Ok(());
        }
        if self.size == 0 {
            return Err(EngineError("plik jest pusty: nie ma co edytować".into()));
        }
        self.mode = ViewMode::Hex;
        self.hscroll = 0;
        self.offset = self.offset / HEX_ROW * HEX_ROW;
        self.editor = Some(Editor {
            cursor: self.offset.min(self.size - 1),
            high: None,
        });
        self.edit_version += 1;
        self.message = "edytor hex: strzałki przesuwają, cyfry 0-9 a-f zmieniają bajt, Ctrl+S zapisuje kopię, Esc kończy".into();
        Ok(())
    }

    fn require_editor(&mut self) -> Result<Editor, EngineError> {
        self.editor
            .ok_or_else(|| EngineError("edytor hex jest wyłączony (viewer edit)".into()))
    }

    /// Moves the editor's cursor by bytes (±16 is a row) and keeps it on screen.
    pub fn edit_move(&mut self, delta: i64) -> Result<(), EngineError> {
        let mut editor = self.require_editor()?;
        let max = self.size.saturating_sub(1);
        editor.cursor =
            (editor.cursor as i128 + i128::from(delta)).clamp(0, i128::from(max)) as u64;
        editor.high = None;
        self.editor = Some(editor);
        let rows = self.rows as u64;
        let row = editor.cursor / HEX_ROW;
        let first = self.offset / HEX_ROW;
        if row < first {
            self.offset = row * HEX_ROW;
        } else if row >= first + rows {
            self.offset = (row + 1 - rows) * HEX_ROW;
        }
        self.edit_version += 1;
        Ok(())
    }

    /// Types one hex digit: two digits make a byte, which replaces the one under the cursor
    /// and moves the cursor on.
    pub fn edit_type(&mut self, digit: char) -> Result<(), EngineError> {
        let value = digit.to_digit(16).ok_or_else(|| {
            EngineError(format!("„{digit}” nie jest cyfrą szesnastkową (0-9, a-f)"))
        })? as u8;
        let mut editor = self.require_editor()?;
        match editor.high.take() {
            None => {
                editor.high = Some(value);
                self.editor = Some(editor);
                self.message = format!("wpisano {value:x}_ : jeszcze jedna cyfra");
            }
            Some(high) => {
                self.set_byte(editor.cursor, high << 4 | value)
                    .map_err(|e| EngineError(e.to_string()))?;
                self.editor = Some(Editor {
                    high: None,
                    ..editor
                });
                self.edit_move(1)?;
                self.message = format!("zmienione bajty: {}", self.edits.len());
            }
        }
        self.edit_version += 1;
        Ok(())
    }

    /// Changes one byte; a byte set back to the original value is no change at all.
    fn set_byte(&mut self, at: u64, value: u8) -> std::io::Result<()> {
        let mut original = [0u8; 1];
        self.file.seek(SeekFrom::Start(at))?;
        self.file.read_exact(&mut original)?;
        if original[0] == value {
            self.edits.remove(&at);
        } else {
            self.edits.insert(at, value);
        }
        self.edit_version += 1;
        Ok(())
    }

    /// `viewer poke <offset> <hex bytes>`: sets bytes at an offset (the file does not grow).
    pub fn poke(&mut self, offset: u64, bytes: &[u8]) -> Result<(), EngineError> {
        if bytes.is_empty() {
            return Err(EngineError(
                "poke: podaj bajty, np. viewer poke 0x10 ff 00".into(),
            ));
        }
        let end = offset
            .checked_add(bytes.len() as u64)
            .filter(|&e| e <= self.size)
            .ok_or_else(|| {
                EngineError(format!("poke: poza końcem pliku ({} bajtów)", self.size))
            })?;
        for (i, &b) in bytes.iter().enumerate() {
            self.set_byte(offset + i as u64, b)
                .map_err(|e| EngineError(e.to_string()))?;
        }
        self.mode = ViewMode::Hex;
        self.message = format!(
            "zmieniono bajty {offset}..{end} (niezapisane: {})",
            self.edits.len()
        );
        Ok(())
    }

    /// Throws the unsaved changes away.
    pub fn edit_revert(&mut self) {
        self.edits.clear();
        self.edit_version += 1;
        if let Some(editor) = self.editor.as_mut() {
            editor.high = None;
        }
        self.message = "zmiany odrzucone".into();
    }

    /// Writes the file with the changes to a copy next to it (`name.bpedit`); the original is
    /// untouched until [`Viewer::save_confirm`].
    pub fn save_copy(&mut self) -> Result<PathBuf, EngineError> {
        if self.edits.is_empty() {
            return Err(EngineError("nie ma zmian do zapisania".into()));
        }
        let mut name = self.path.as_os_str().to_owned();
        name.push(".bpedit");
        let copy = PathBuf::from(name);
        self.write_copy(&copy).map_err(|e| {
            let _ = std::fs::remove_file(&copy);
            EngineError(format!("nie udało się zapisać kopii: {e}"))
        })?;
        self.saved_copy = Some(copy.clone());
        self.message = format!(
            "kopia ze zmianami ({} bajtów) czeka: Enter podmienia plik (oryginał trafia do kopii .bak), Esc odrzuca",
            self.edits.len()
        );
        Ok(copy)
    }

    fn write_copy(&mut self, copy: &Path) -> std::io::Result<()> {
        use std::io::Write;
        let mut out = std::io::BufWriter::new(File::create_new(copy)?);
        let mut block = vec![0u8; BLOCK];
        let mut at = 0u64;
        self.file.seek(SeekFrom::Start(0))?;
        while at < self.size {
            let len = ((self.size - at) as usize).min(BLOCK);
            self.file.read_exact(&mut block[..len])?;
            for (&pos, &value) in self.edits.range(at..at + len as u64) {
                block[(pos - at) as usize] = value;
            }
            out.write_all(&block[..len])?;
            at += len as u64;
        }
        out.flush()?;
        out.get_ref().sync_all()
    }

    /// Replaces the file with the saved copy; the original stays as `name.bak` (or `.bak2`...).
    /// Returns where the original went.
    pub fn save_confirm(&mut self) -> Result<PathBuf, EngineError> {
        let copy = self.saved_copy.take().ok_or_else(|| {
            EngineError("najpierw viewer save (zapisuje kopię do potwierdzenia)".into())
        })?;
        let backup = (1..)
            .map(|n| {
                let mut name = self.path.as_os_str().to_owned();
                name.push(if n == 1 {
                    ".bak".to_string()
                } else {
                    format!(".bak{n}")
                });
                PathBuf::from(name)
            })
            .find(|p| std::fs::symlink_metadata(p).is_err())
            .unwrap_or_else(|| self.path.clone());
        let describe =
            |e: std::io::Error| EngineError(format!("nie udało się podmienić pliku: {e}"));
        std::fs::rename(&self.path, &backup).map_err(describe)?;
        if let Err(e) = std::fs::rename(&copy, &self.path) {
            // Put the original back: nothing is lost.
            let _ = std::fs::rename(&backup, &self.path);
            self.saved_copy = Some(copy);
            return Err(describe(e));
        }
        self.file = File::open(&self.path).map_err(describe)?;
        self.edits.clear();
        self.edit_version += 1;
        self.cache = None;
        self.message = format!("zapisano; oryginał jest w {}", backup.display());
        Ok(backup)
    }

    /// Drops the waiting copy; the changes stay in the editor.
    pub fn save_cancel(&mut self) {
        if let Some(copy) = self.saved_copy.take() {
            let _ = std::fs::remove_file(copy);
            self.message = "kopia odrzucona; zmiany zostają w edytorze".into();
        }
    }

    /// Searching works on the bytes of the file, so the rendered view gives way to the text.
    fn leave_rendered(&mut self) {
        if self.mode == ViewMode::Rendered {
            self.mode = ViewMode::Text;
            self.offset = 0;
        }
    }

    /// Text of the last search, to pre-fill the search dialog.
    pub fn last_search(&self) -> Option<&str> {
        self.search.as_ref().map(|s| s.text.as_str())
    }

    /// Repeats the last search in the given direction (continuing an unfinished one).
    pub fn find_again(&mut self, forward: bool) {
        self.leave_rendered();
        let Some(search) = self.search.as_mut() else {
            self.message = "nic jeszcze nie szukano (F7 lub /)".into();
            return;
        };
        let same_direction = search.forward == forward;
        search.forward = forward;
        let resume = search.resume.take().filter(|_| same_direction);
        let from = resume.unwrap_or_else(|| {
            let anchor = self.last_match.unwrap_or(self.offset);
            if forward {
                anchor.saturating_add(1)
            } else {
                anchor
            }
        });
        self.run_search(from);
    }

    fn match_or_view_start(&self) -> u64 {
        self.last_match.unwrap_or(self.offset)
    }

    fn run_search(&mut self, from: u64) {
        let Some(search) = self.search.clone() else {
            return;
        };
        let outcome = if search.forward {
            self.search_forward(&search.needle, from)
        } else {
            self.search_backward(&search.needle, from)
        };
        match outcome {
            Ok(Found::At(pos)) => {
                self.last_match = Some(pos);
                self.offset = match self.mode {
                    ViewMode::Hex => pos / HEX_ROW * HEX_ROW,
                    ViewMode::Text | ViewMode::Rendered => {
                        self.line_start_at_or_before(pos).unwrap_or(0)
                    }
                };
                self.hscroll = 0;
                self.message = format!("znaleziono „{}” przy {}", search.text, pos);
            }
            Ok(Found::Partial(next)) => {
                if let Some(s) = self.search.as_mut() {
                    s.resume = Some(next);
                }
                self.message = format!(
                    "nic w przeszukanej części (do {}). Powtórz (N), by szukać dalej",
                    next
                );
            }
            Ok(Found::Nowhere) => self.message = format!("nie znaleziono „{}”", search.text),
            Err(e) => self.message = format!("błąd odczytu: {e}"),
        }
    }

    /// Scans `[from, size)` and then wraps to `[0, from)`, at most `SEARCH_LIMIT` bytes.
    fn search_forward(&mut self, needle: &[u8], from: u64) -> std::io::Result<Found> {
        let from = from.min(self.size);
        let mut scanned = 0u64;
        for (start, end) in [
            (from, self.size),
            (0, (from + needle.len() as u64).min(self.size)),
        ] {
            let mut pos = start;
            while pos < end {
                if scanned >= SEARCH_LIMIT {
                    return Ok(Found::Partial(pos));
                }
                let len = (end - pos).min(BLOCK as u64 + needle.len() as u64 - 1) as usize;
                let mut block = vec![0u8; len];
                self.file.seek(SeekFrom::Start(pos))?;
                self.file.read_exact(&mut block)?;
                if let Some(i) = find_bytes(&block, needle) {
                    return Ok(Found::At(pos + i as u64));
                }
                let step = len.saturating_sub(needle.len() - 1).max(1) as u64;
                pos += step;
                scanned += step;
            }
        }
        Ok(Found::Nowhere)
    }

    /// Scans backwards from `from` (exclusive), wrapping from the end of the file.
    fn search_backward(&mut self, needle: &[u8], from: u64) -> std::io::Result<Found> {
        let from = from.min(self.size);
        let mut scanned = 0u64;
        for (lo, hi) in [
            (0, from),
            (from.saturating_sub(needle.len() as u64), self.size),
        ] {
            let mut end = hi;
            while end > lo {
                if scanned >= SEARCH_LIMIT {
                    return Ok(Found::Partial(end));
                }
                let start = end
                    .saturating_sub(BLOCK as u64 + needle.len() as u64 - 1)
                    .max(lo);
                let mut block = vec![0u8; (end - start) as usize];
                self.file.seek(SeekFrom::Start(start))?;
                self.file.read_exact(&mut block)?;
                if let Some(i) = rfind_bytes(&block, needle) {
                    return Ok(Found::At(start + i as u64));
                }
                let step = (end - start).saturating_sub(needle.len() as u64 - 1).max(1);
                end -= step.min(end - lo);
                scanned += step;
            }
        }
        Ok(Found::Nowhere)
    }
}

/// A line read from the file: the bytes kept (at most `MAX_LINE_BYTES`) and how many bytes
/// the line occupies in the file including its `\n`.
struct Line {
    bytes: Vec<u8>,
    consumed: u64,
}

fn read_line_capped<R: BufRead>(reader: &mut R) -> std::io::Result<Line> {
    let mut bytes = Vec::new();
    let mut consumed = 0u64;
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            break;
        }
        let (chunk, found_newline) = match buffer.iter().position(|&b| b == b'\n') {
            Some(i) => (&buffer[..i], true),
            None => (buffer, false),
        };
        let room = MAX_LINE_BYTES.saturating_sub(bytes.len());
        bytes.extend_from_slice(&chunk[..chunk.len().min(room)]);
        let used = chunk.len() + usize::from(found_newline);
        reader.consume(used);
        consumed += used as u64;
        if found_newline {
            break;
        }
    }
    Ok(Line { bytes, consumed })
}

/// Text for one row: UTF-8 (invalid bytes become U+FFFD), tabs expanded, control characters
/// shown as `·`, trailing `\r` dropped.
fn display_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
    let mut out = String::with_capacity(bytes.len());
    let mut column = 0;
    for c in String::from_utf8_lossy(bytes).chars() {
        match c {
            '\t' => {
                let n = TAB_WIDTH - column % TAB_WIDTH;
                out.extend(std::iter::repeat_n(' ', n));
                column += n;
            }
            c if c.is_control() => {
                out.push('·');
                column += 1;
            }
            c => {
                out.push(c);
                column += 1;
            }
        }
    }
    out
}

/// `00000010  48 65 6c 6c 6f ...  |Hello...|`
fn hex_line(offset: u64, bytes: &[u8]) -> String {
    let mut hex = String::new();
    for i in 0..HEX_ROW as usize {
        if i == 8 {
            hex.push(' ');
        }
        match bytes.get(i) {
            Some(b) => hex.push_str(&format!("{b:02x} ")),
            None => hex.push_str("   "),
        }
    }
    let ascii: String = bytes
        .iter()
        .map(|&b| {
            if (0x20..0x7f).contains(&b) {
                b as char
            } else {
                '.'
            }
        })
        .collect();
    format!("{offset:08x}  {hex} |{ascii}|")
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle))
}

fn rfind_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .rposition(|w| w.eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A text file with the lines `line 0000` ... `line 0099`.
    fn numbered(lines: usize) -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("t.txt");
        let mut file = File::create(&path).unwrap();
        for i in 0..lines {
            writeln!(file, "line {i:04}").unwrap();
        }
        (tmp, path)
    }

    fn first_texts(viewer: &mut Viewer, n: usize) -> Vec<String> {
        viewer
            .visible_rows()
            .iter()
            .take(n)
            .map(|r| r.text.clone())
            .collect()
    }

    #[test]
    fn shows_the_first_rows_in_text_mode() {
        let (_tmp, path) = numbered(100);
        let mut v = Viewer::open(&path).unwrap();
        v.set_viewport(5, 80);
        assert_eq!(v.mode(), ViewMode::Text);
        let rows = v.visible_rows().to_vec();
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].text, "line 0000");
        assert_eq!(rows[1].offset, 10);
        assert_eq!(rows[4].text, "line 0004");
    }

    #[test]
    fn scrolling_down_and_up_by_rows() {
        let (_tmp, path) = numbered(100);
        let mut v = Viewer::open(&path).unwrap();
        v.set_viewport(5, 80);
        v.scroll(3);
        assert_eq!(first_texts(&mut v, 1), ["line 0003"]);
        v.scroll(-2);
        assert_eq!(first_texts(&mut v, 1), ["line 0001"]);
        v.scroll(-50);
        assert_eq!(v.offset(), 0);
        v.scroll(1000);
        // Stops on the last line instead of scrolling into nothing.
        assert_eq!(first_texts(&mut v, 1), ["line 0099"]);
    }

    #[test]
    fn pages_keep_one_row_of_context() {
        let (_tmp, path) = numbered(100);
        let mut v = Viewer::open(&path).unwrap();
        v.set_viewport(10, 80);
        v.page(1);
        assert_eq!(first_texts(&mut v, 1), ["line 0009"]);
        v.page(-1);
        assert_eq!(v.offset(), 0);
    }

    #[test]
    fn top_and_bottom() {
        let (_tmp, path) = numbered(100);
        let mut v = Viewer::open(&path).unwrap();
        v.set_viewport(5, 80);
        v.bottom();
        let rows = first_texts(&mut v, 10);
        assert_eq!(rows.first().unwrap(), "line 0095");
        assert_eq!(rows.last().unwrap(), "line 0099");
        assert_eq!(v.percent(), 95);
        v.top();
        assert_eq!(v.offset(), 0);
    }

    #[test]
    fn a_file_without_a_final_newline_ends_on_its_last_line() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("t.txt");
        std::fs::write(&path, "a\nb\nlast").unwrap();
        let mut v = Viewer::open(&path).unwrap();
        v.set_viewport(2, 80);
        v.bottom();
        assert_eq!(first_texts(&mut v, 5), ["b", "last"]);
    }

    #[test]
    fn empty_and_tiny_files_are_fine() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("empty");
        std::fs::write(&path, "").unwrap();
        let mut v = Viewer::open(&path).unwrap();
        v.set_viewport(5, 80);
        assert!(v.visible_rows().is_empty());
        v.scroll(5);
        v.scroll(-5);
        v.bottom();
        v.toggle_mode();
        assert_eq!(v.offset(), 0);
        assert_eq!(v.percent(), 100);
    }

    #[test]
    fn crlf_tabs_and_control_characters_are_tidied() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("t.txt");
        std::fs::write(&path, "a\tb\r\nbell\x07!\r\n").unwrap();
        let mut v = Viewer::open(&path).unwrap();
        v.set_viewport(5, 80);
        assert_eq!(first_texts(&mut v, 5), ["a   b", "bell·!"]);
    }

    #[test]
    fn invalid_utf8_and_unicode_do_not_break_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("t.txt");
        std::fs::write(&path, b"za\xc5\xbc\xc3\xb3\xc5\x82\xc4\x87 \xff\nok\n").unwrap();
        let mut v = Viewer::open(&path).unwrap();
        v.set_viewport(5, 80);
        assert_eq!(first_texts(&mut v, 5), ["zażółć \u{fffd}", "ok"]);
    }

    #[test]
    fn horizontal_scroll_skips_characters() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("t.txt");
        std::fs::write(&path, "0123456789\n").unwrap();
        let mut v = Viewer::open(&path).unwrap();
        v.set_viewport(2, 80);
        v.scroll_columns(4);
        assert_eq!(first_texts(&mut v, 1), ["456789"]);
        v.scroll_columns(-100);
        assert_eq!(first_texts(&mut v, 1), ["0123456789"]);
    }

    #[test]
    fn a_huge_single_line_is_capped_in_memory_but_still_scrolls_past() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("t.txt");
        let mut file = File::create(&path).unwrap();
        file.write_all(&vec![b'x'; 5 * 1024 * 1024]).unwrap();
        file.write_all(b"\nafter\n").unwrap();
        let mut v = Viewer::open(&path).unwrap();
        v.set_viewport(3, 80);
        let rows = v.visible_rows().to_vec();
        assert!(rows[0].text.chars().count() <= 2 * 80);
        assert_eq!(rows[1].text, "after");
        v.scroll(1);
        assert_eq!(first_texts(&mut v, 1), ["after"]);
        v.scroll(-1);
        assert_eq!(v.offset(), 0);
    }

    #[test]
    fn binary_files_open_in_hex_and_the_mode_toggles() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("b.bin");
        let data: Vec<u8> = (0..=255u8).collect();
        std::fs::write(&path, &data).unwrap();
        let mut v = Viewer::open(&path).unwrap();
        v.set_viewport(3, 80);
        assert_eq!(v.mode(), ViewMode::Hex);
        let rows = v.visible_rows().to_vec();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].offset, 16);
        assert!(rows[0]
            .text
            .starts_with("00000000  00 01 02 03 04 05 06 07  08"));
        assert!(rows[1].text.starts_with("00000010  10 11"));
        // 0x20 (space) is printable, so it shows as itself.
        assert!(rows[2].text.ends_with("| !\"#$%&'()*+,-./|"));
        v.toggle_mode();
        assert_eq!(v.mode(), ViewMode::Text);
        v.toggle_mode();
        assert_eq!(v.offset(), 0);
    }

    #[test]
    fn hex_scrolls_by_sixteen_bytes_and_stops_at_the_end() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("b.bin");
        std::fs::write(&path, vec![0u8; 100]).unwrap();
        let mut v = Viewer::open(&path).unwrap();
        v.set_viewport(2, 80);
        v.scroll(1);
        assert_eq!(v.offset(), 16);
        v.scroll(100);
        assert_eq!(v.offset(), 80);
        let rows = v.visible_rows().to_vec();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].offset, 96);
        assert!(rows[1].text.contains("|....|"));
        v.top();
        v.bottom();
        assert_eq!(v.offset(), 80);
    }

    #[test]
    fn goto_accepts_offsets_and_percentages() {
        let (_tmp, path) = numbered(100);
        let mut v = Viewer::open(&path).unwrap();
        v.set_viewport(5, 80);
        v.goto("50%").unwrap();
        assert_eq!(first_texts(&mut v, 1), ["line 0050"]);
        v.goto("25").unwrap();
        assert_eq!(first_texts(&mut v, 1), ["line 0002"]); // start of the line holding byte 25
        v.goto("100%").unwrap();
        assert!(v.goto("abc").is_err());
        assert!(v.goto("-5").is_err());
    }

    #[test]
    fn search_finds_next_and_previous_and_wraps() {
        let (_tmp, path) = numbered(100);
        let mut v = Viewer::open(&path).unwrap();
        v.set_viewport(5, 80);
        v.find("LINE 0042", true); // case-insensitive
        assert_eq!(first_texts(&mut v, 1), ["line 0042"]);
        assert_eq!(v.match_offset(), Some(420));
        v.find_again(true);
        assert!(v.message.contains("znaleziono"));
        // The same text again wraps around to the same (only) hit.
        assert_eq!(first_texts(&mut v, 1), ["line 0042"]);

        v.find("line 004", true);
        let first = v.offset();
        v.find_again(true);
        assert!(v.offset() > first);
        v.find_again(false);
        assert_eq!(v.offset(), first);
    }

    #[test]
    fn search_reports_when_nothing_matches() {
        let (_tmp, path) = numbered(10);
        let mut v = Viewer::open(&path).unwrap();
        v.find("zzz", true);
        assert!(v.message.contains("nie znaleziono"));
        assert_eq!(v.offset(), 0);
        v.find("", true);
        assert!(v.message.contains("pusty"));
        let mut other = Viewer::open(&path).unwrap();
        other.find_again(true);
        assert!(other.message.contains("nic jeszcze"));
    }

    #[test]
    fn search_works_in_hex_mode_and_across_block_boundaries() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("b.bin");
        let mut data = vec![0u8; 3 * BLOCK];
        // Place the needle so it straddles the first block boundary.
        let at = BLOCK - 3;
        data[at..at + 6].copy_from_slice(b"NeedLe");
        std::fs::write(&path, &data).unwrap();
        let mut v = Viewer::open(&path).unwrap();
        v.set_viewport(4, 80);
        assert_eq!(v.mode(), ViewMode::Hex);
        v.find("needle", true);
        assert_eq!(v.match_offset(), Some(at as u64));
        assert_eq!(v.offset(), (at as u64) / 16 * 16);
        v.find("needle", false);
        assert_eq!(
            v.match_offset(),
            Some(at as u64),
            "backward search wraps to the same hit"
        );
    }

    #[test]
    fn a_huge_sparse_file_opens_and_navigates_instantly() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("huge.bin");
        let file = File::create(&path).unwrap();
        file.set_len(512 * 1024 * 1024).unwrap();
        let started = std::time::Instant::now();
        let mut v = Viewer::open(&path).unwrap();
        v.set_viewport(30, 100);
        assert_eq!(v.visible_rows().len(), 30);
        v.goto("50%").unwrap();
        v.bottom();
        v.scroll(-5);
        v.top();
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn directories_and_missing_files_are_errors() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(Viewer::open(tmp.path()).is_err());
        let err = Viewer::open(&tmp.path().join("nope")).unwrap_err();
        assert!(err.0.contains("nope"));
    }

    fn open_text(name: &str, text: &str) -> (tempfile::TempDir, Viewer) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        std::fs::write(&path, text).unwrap();
        let mut viewer = Viewer::open(&path).unwrap();
        viewer.set_viewport(30, 100);
        (dir, viewer)
    }

    fn kinds_in(row: &Row) -> Vec<(String, SpanKind)> {
        let chars: Vec<char> = row.text.chars().collect();
        row.spans
            .iter()
            .map(|s| (chars[s.start..s.end].iter().collect(), s.kind))
            .collect()
    }

    #[test]
    fn code_is_coloured_by_its_extension_and_plain_text_is_not() {
        let (_dir, mut v) = open_text(
            "a.rs",
            "fn main() { // zażółć\n    let x = \"gęślą\"; /* blok\nciąg dalszy */ let y = 1;\n}\n",
        );
        assert_eq!(v.language(), Some("Rust"));
        let rows = v.visible_rows().to_vec();
        assert!(kinds_in(&rows[0]).contains(&("fn".to_string(), SpanKind::Keyword)));
        assert_eq!(kinds_in(&rows[0]).last().unwrap().1, SpanKind::Comment);
        assert!(kinds_in(&rows[1]).contains(&("\"gęślą\"".to_string(), SpanKind::String)));
        // The block comment goes over the line break and ends where it closes.
        assert!(kinds_in(&rows[2]).contains(&("ciąg dalszy */".to_string(), SpanKind::Comment)));
        assert!(kinds_in(&rows[2]).contains(&("let".to_string(), SpanKind::Keyword)));
        assert!(kinds_in(&rows[2]).contains(&("1".to_string(), SpanKind::Number)));
        // The colour can be switched off, and a plain file has none to switch.
        assert!(v.toggle_highlight());
        assert!(v.visible_rows().iter().all(|r| r.spans.is_empty()));
        assert_eq!(v.language(), None);
        let (_dir, mut plain) = open_text("notes.txt", "fn let 12 // nic\n");
        assert!(plain.visible_rows()[0].spans.is_empty());
        assert!(!plain.toggle_highlight());
    }

    #[test]
    fn a_window_inside_a_block_comment_knows_it_and_horizontal_scroll_shifts_the_colours() {
        let mut text = String::from("/* start\n");
        for i in 0..200 {
            text.push_str(&format!("wiersz {i} fn let\n"));
        }
        text.push_str("koniec */ let z\n");
        let (_dir, mut v) = open_text("big.js", &text);
        v.set_viewport(10, 100);
        v.scroll(100);
        let rows = v.visible_rows().to_vec();
        assert!(
            rows.iter()
                .all(|r| r.spans.len() == 1 && r.spans[0].kind == SpanKind::Comment),
            "the whole window is inside the comment: {:?}",
            rows.iter().map(kinds_in).collect::<Vec<_>>()
        );
        // Going back to the top and forward again gives the same answer.
        v.top();
        assert_eq!(kinds_in(&v.visible_rows()[0])[0].1, SpanKind::Comment);
        v.bottom();
        let last = v
            .visible_rows()
            .iter()
            .find(|r| r.text.starts_with("koniec"))
            .unwrap()
            .clone();
        assert_eq!(
            kinds_in(&last)[0],
            ("koniec */".to_string(), SpanKind::Comment)
        );
        assert_eq!(kinds_in(&last)[1], ("let".to_string(), SpanKind::Keyword));
        // Scrolled sideways, the spans move with the text.
        let (_dir, mut v) = open_text("a.py", "xx = def_ if 1 else 2  # koniec\n");
        v.scroll_columns(3);
        let row = v.visible_rows()[0].clone();
        assert!(
            kinds_in(&row).contains(&("if".to_string(), SpanKind::Keyword)),
            "{row:?}"
        );
    }

    #[test]
    fn a_markdown_file_opens_rendered_and_toggles_to_its_text() {
        let (_dir, mut v) = open_text(
            "README.md",
            "# Tytuł zażółć\n\nTo jest **mocne** zdanie `kod`.\n\n- jeden\n- dwa\n",
        );
        assert_eq!(v.mode(), ViewMode::Rendered);
        let rows = v.visible_rows().to_vec();
        assert_eq!(rows[0].text, "Tytuł zażółć");
        assert_eq!(rows[0].spans[0].kind, SpanKind::Heading);
        assert!(rows.iter().any(|r| r.text == "To jest mocne zdanie kod."));
        assert!(rows.iter().any(|r| r.text == "• jeden"));
        assert!(v.rendered_rows().unwrap() >= 6);
        v.toggle_render();
        assert_eq!(v.mode(), ViewMode::Text);
        assert_eq!(v.visible_rows()[0].text, "# Tytuł zażółć");
        v.toggle_render();
        assert_eq!(v.mode(), ViewMode::Rendered);
        // Searching works on the text, so it leaves the rendered view.
        v.find("mocne", true);
        assert_eq!(v.mode(), ViewMode::Text);
        assert!(v.match_offset().is_some());
        // Other files have nothing to render.
        let (_dir, mut txt) = open_text("a.txt", "# nie markdown\n");
        assert_eq!(txt.mode(), ViewMode::Text);
        txt.toggle_render();
        assert!(txt.message.contains("nie jest plik Markdown"));
    }

    #[test]
    fn rendered_markdown_scrolls_wraps_to_the_width_and_jumps() {
        let mut text = String::new();
        for i in 0..100 {
            text.push_str(&format!(
                "Akapit numer {i} z dość długim tekstem, który się zawija w wąskim oknie.\n\n"
            ));
        }
        let (_dir, mut v) = open_text("long.md", &text);
        v.set_viewport(10, 30);
        let total = {
            v.visible_rows();
            v.rendered_rows().unwrap()
        };
        assert!(total > 300, "{total}");
        assert!(v
            .visible_rows()
            .iter()
            .all(|r| r.text.chars().count() <= 30));
        v.scroll(5);
        assert_eq!(v.offset(), 5);
        v.scroll(-50);
        assert_eq!(v.offset(), 0);
        v.page(1);
        assert_eq!(v.offset(), 9);
        v.bottom();
        assert_eq!(v.offset() as usize, total - 10);
        v.scroll(1000);
        assert_eq!(v.offset() as usize, total - 10, "never past the last page");
        v.goto("50%").unwrap();
        assert!((v.percent() as i64 - 50).abs() <= 3, "{}", v.percent());
        v.goto("7").unwrap();
        assert_eq!(v.offset(), 7);
        // A wider window renders again for its width.
        v.set_viewport(10, 200);
        assert!(v
            .visible_rows()
            .iter()
            .all(|r| r.text.chars().count() <= 200));
        assert!(v.rendered_rows().unwrap() < total);
        v.toggle_mode();
        assert_eq!(v.mode(), ViewMode::Hex);
    }

    fn edit_target(content: &[u8]) -> (tempfile::TempDir, PathBuf, Viewer) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dane.bin");
        std::fs::write(&path, content).unwrap();
        let mut viewer = Viewer::open(&path).unwrap();
        viewer.set_viewport(8, 100);
        (dir, path, viewer)
    }

    #[test]
    fn typing_hex_digits_changes_bytes_and_shows_them_before_saving() {
        let (_dir, path, mut v) = edit_target(&(0u8..40).collect::<Vec<_>>());
        v.toggle_edit().unwrap();
        assert!(v.editing() && v.mode() == ViewMode::Hex);
        v.edit_type('f').unwrap();
        v.edit_type('F').unwrap();
        v.edit_type('0').unwrap();
        v.edit_type('2').unwrap();
        assert_eq!(v.edit_count(), 2);
        let row = v.visible_rows()[0].clone();
        assert!(row.text.starts_with("00000000  ff 02 02"), "{}", row.text);
        // Both the hex digits and the text column of a changed byte are marked, the cursor too.
        let kinds: Vec<_> = row.spans.iter().map(|s| (s.start, s.kind)).collect();
        assert!(
            kinds.contains(&(10, SpanKind::Modified)) && kinds.contains(&(61, SpanKind::Modified))
        );
        assert!(kinds.contains(&(16, SpanKind::Cursor)), "{kinds:?}");
        // Setting a byte back to what it was is no change at all.
        v.edit_move(-2).unwrap();
        v.edit_type('0').unwrap();
        v.edit_type('0').unwrap();
        assert_eq!(v.edit_count(), 1);
        assert!(v.edit_type('g').is_err());
        // The file on disk is untouched until it is saved.
        assert_eq!(std::fs::read(&path).unwrap()[0], 0);
        v.edit_revert();
        assert_eq!(v.edit_count(), 0);
        assert!(!v.visible_rows()[0].text.contains("ff"));
    }

    #[test]
    fn the_cursor_stays_in_the_file_and_the_view_follows_it() {
        let (_dir, _path, mut v) = edit_target(&[0u8; 400]);
        assert!(v.edit_move(1).is_err(), "the editor is off");
        v.toggle_edit().unwrap();
        v.edit_move(-5).unwrap();
        v.edit_move(10_000).unwrap();
        let last = v.visible_rows().last().unwrap().offset;
        assert_eq!(last, 399 / 16 * 16, "the last row shows the cursor");
        v.edit_move(-399).unwrap();
        assert_eq!(v.offset(), 0);
        v.toggle_edit().unwrap();
        assert!(!v.editing());
        let (_dir, _path, mut empty) = edit_target(b"");
        assert!(empty.toggle_edit().is_err());
    }

    #[test]
    fn poke_sets_bytes_within_the_file_only() {
        let (_dir, _path, mut v) = edit_target(b"0123456789");
        v.poke(2, &[0xAA, 0xBB]).unwrap();
        assert!(
            v.visible_rows()[0].text.contains("30 31 aa bb 34"),
            "{}",
            v.visible_rows()[0].text
        );
        assert!(v.poke(9, &[1, 2]).is_err(), "it would grow the file");
        assert!(v.poke(0, &[]).is_err());
        assert_eq!(v.edit_count(), 2);
    }

    #[test]
    fn saving_goes_through_a_copy_a_confirmation_and_a_backup() {
        let (dir, path, mut v) = edit_target(b"stary plik z zawartoscia");
        assert!(v.save_copy().is_err(), "nothing to save");
        v.poke(0, b"N").unwrap();
        let copy = v.save_copy().unwrap();
        assert!(v.awaiting_save());
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"stary plik z zawartoscia",
            "original untouched"
        );
        assert_eq!(std::fs::read(&copy).unwrap(), b"Ntary plik z zawartoscia");
        // Saying no drops the copy and keeps the changes.
        v.save_cancel();
        assert!(!copy.exists() && !v.awaiting_save() && v.edit_count() == 1);
        // Saying yes replaces the file and keeps the old one as a backup.
        v.save_copy().unwrap();
        let backup = v.save_confirm().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"Ntary plik z zawartoscia");
        assert_eq!(std::fs::read(&backup).unwrap(), b"stary plik z zawartoscia");
        assert!(backup.ends_with("dane.bin.bak"));
        assert_eq!(v.edit_count(), 0);
        assert!(
            v.visible_rows()[0].text.contains("4e 74 61 72"),
            "the view shows the new file"
        );
        // A second round does not overwrite the first backup.
        v.poke(1, b"X").unwrap();
        v.save_copy().unwrap();
        assert!(v.save_confirm().unwrap().ends_with("dane.bin.bak2"));
        assert!(v.save_confirm().is_err(), "nothing waits");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 3);
    }

    #[test]
    fn closing_with_unsaved_changes_asks_twice() {
        let (_dir, _path, mut v) = edit_target(b"abc");
        assert!(v.confirm_close());
        v.poke(0, b"z").unwrap();
        assert!(!v.confirm_close(), "the first ask warns");
        assert!(v.message.contains("niezapisane"));
        assert!(v.confirm_close(), "the second one lets go");
        v.poke(1, b"y").unwrap();
        assert!(!v.confirm_close(), "a new change warns again");
    }
}
