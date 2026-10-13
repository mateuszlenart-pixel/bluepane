//! File viewer (F3): shows a window of a file of any size.
//!
//! The viewer never reads the whole file. It keeps a byte offset, seeks there and reads only
//! as many bytes as the visible rows need, so opening a multi-gigabyte file is instant and
//! uses a few kilobytes. Text mode splits lines on `\n`; hex mode shows 16 bytes per row.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Text,
    Hex,
}

/// One displayed row. `offset` is the byte position of its first byte in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub offset: u64,
    pub text: String,
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
            message: String::new(),
        })
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
        }
    }

    fn render_hex(&mut self) -> std::io::Result<Vec<Row>> {
        let want = self.rows as u64 * HEX_ROW;
        let len = want.min(self.size.saturating_sub(self.offset));
        let mut bytes = vec![0u8; len as usize];
        self.file.seek(SeekFrom::Start(self.offset))?;
        self.file.read_exact(&mut bytes)?;
        Ok(bytes
            .chunks(HEX_ROW as usize)
            .enumerate()
            .map(|(i, chunk)| Row {
                offset: self.offset + i as u64 * HEX_ROW,
                text: hex_line(self.offset + i as u64 * HEX_ROW, chunk),
            })
            .collect())
    }

    fn render_text(&mut self) -> std::io::Result<Vec<Row>> {
        let mut reader = BufReader::with_capacity(BLOCK, &self.file);
        reader.seek(SeekFrom::Start(self.offset))?;
        let mut rows = Vec::with_capacity(self.rows);
        let mut pos = self.offset;
        while rows.len() < self.rows && pos < self.size {
            let line = read_line_capped(&mut reader)?;
            let shown: String = display_text(&line.bytes)
                .chars()
                .skip(self.hscroll)
                .take(self.cols * 2)
                .collect();
            rows.push(Row {
                offset: pos,
                text: shown,
            });
            pos += line.consumed;
        }
        Ok(rows)
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
        };
        if let Err(e) = result {
            self.message = format!("błąd odczytu: {e}");
        }
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
        let target = if let Some(percent) = spec.strip_suffix('%') {
            let percent: u64 = percent.trim().parse().map_err(|_| bad())?;
            (u128::from(self.size) * u128::from(percent.min(100)) / 100) as u64
        } else {
            spec.parse::<u64>().map_err(|_| bad())?.min(self.size)
        };
        self.offset = match self.mode {
            ViewMode::Hex => target / HEX_ROW * HEX_ROW,
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

    /// Text of the last search, to pre-fill the search dialog.
    pub fn last_search(&self) -> Option<&str> {
        self.search.as_ref().map(|s| s.text.as_str())
    }

    /// Repeats the last search in the given direction (continuing an unfinished one).
    pub fn find_again(&mut self, forward: bool) {
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
                    ViewMode::Text => self.line_start_at_or_before(pos).unwrap_or(0),
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
}
