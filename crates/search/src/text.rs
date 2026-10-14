//! Searching the text of one stream, line by line, without loading it whole.

use std::io::{self, BufRead, BufReader, Read};

use regex::Regex;

use crate::encoding::decode_cp1250;
use crate::{Encoding, LineHit, TextQuery, MAX_COUNTED_LINES, MAX_SHOWN_LINES};

/// Longest stretch read as one line; longer lines are cut into pieces of this size.
const LINE_CAP: usize = 1 << 20;
/// How much of the start decides whether a file looks binary.
const SNIFF: usize = 8192;
/// Longest line text kept in a [`LineHit`]; the window around the match is shown.
const SNIPPET: usize = 200;

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Scan {
    pub matching_lines: u64,
    pub lines: Vec<LineHit>,
    /// The stream looked binary and was not searched.
    pub binary: bool,
}

#[derive(Clone, Copy)]
enum Wide {
    Le,
    Be,
}

pub(crate) fn scan(
    source: &mut dyn Read,
    query: &TextQuery,
    matcher: &Regex,
    cancelled: &dyn Fn() -> bool,
) -> io::Result<Scan> {
    let mut reader = BufReader::with_capacity(64 * 1024, source);
    let head = reader.fill_buf()?.to_vec();
    let mut encoding = query.encoding;
    let mut wide = None;
    let mut skip = 0;
    if head.starts_with(&[0xEF, 0xBB, 0xBF]) {
        skip = 3;
        encoding = Encoding::Utf8;
    } else if head.starts_with(&[0xFF, 0xFE]) {
        skip = 2;
        wide = Some(Wide::Le);
    } else if head.starts_with(&[0xFE, 0xFF]) {
        skip = 2;
        wide = Some(Wide::Be);
    } else if encoding == Encoding::Utf16 {
        wide = Some(Wide::Le);
    }
    if wide.is_none() && !query.search_binary && head.iter().take(SNIFF).any(|&b| b == 0) {
        return Ok(Scan {
            binary: true,
            ..Scan::default()
        });
    }
    reader.consume(skip);
    let mut out = Scan::default();
    let mut line_number = 0u64;
    let feed = |text: &str, out: &mut Scan, number: u64| {
        if let Some(found) = matcher.find(text) {
            out.matching_lines += 1;
            if out.lines.len() < MAX_SHOWN_LINES {
                out.lines
                    .push(snippet(number, text, found.start(), found.end()));
            }
        }
    };
    match wide {
        Some(order) => {
            let mut units: Vec<u16> = Vec::new();
            let mut odd: Option<u8> = None;
            let mut buf = vec![0u8; 64 * 1024];
            'read: loop {
                if cancelled() {
                    break;
                }
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                let mut bytes = buf[..n].iter().copied();
                while let Some(first) = odd.take().or_else(|| bytes.next()) {
                    let Some(second) = bytes.next() else {
                        odd = Some(first);
                        break;
                    };
                    let unit = match order {
                        Wide::Le => u16::from_le_bytes([first, second]),
                        Wide::Be => u16::from_be_bytes([first, second]),
                    };
                    if unit == 0x0A || units.len() >= LINE_CAP {
                        if unit == 0x0A {
                            line_number += 1;
                        }
                        let text = String::from_utf16_lossy(&units);
                        feed(text.trim_end_matches('\r'), &mut out, line_number);
                        units.clear();
                        if out.matching_lines >= MAX_COUNTED_LINES {
                            break 'read;
                        }
                        if unit != 0x0A {
                            units.push(unit);
                        }
                    } else {
                        units.push(unit);
                    }
                }
            }
            if !units.is_empty() {
                line_number += 1;
                feed(
                    String::from_utf16_lossy(&units).trim_end_matches('\r'),
                    &mut out,
                    line_number,
                );
            }
        }
        None => {
            let mut buf: Vec<u8> = Vec::new();
            // False while the pieces of one very long line are being read.
            let mut at_start = true;
            loop {
                if cancelled() {
                    break;
                }
                buf.clear();
                let complete = read_line_capped(&mut reader, &mut buf)?;
                if buf.is_empty() && !complete {
                    break;
                }
                if at_start {
                    line_number += 1;
                }
                at_start = complete;
                let line = buf.strip_suffix(b"\n").unwrap_or(&buf);
                let line = line.strip_suffix(b"\r").unwrap_or(line);
                let text = match encoding {
                    Encoding::Utf8 => String::from_utf8_lossy(line).into_owned(),
                    Encoding::Cp1250 => decode_cp1250(line),
                    _ => match std::str::from_utf8(line) {
                        Ok(valid) => valid.to_string(),
                        Err(_) => decode_cp1250(line),
                    },
                };
                feed(&text, &mut out, line_number);
                if out.matching_lines >= MAX_COUNTED_LINES {
                    break;
                }
            }
        }
    }
    Ok(out)
}

/// Reads up to and including `\n`, or `LINE_CAP` bytes. Returns whether the line ended
/// (a newline was read); false means a cut piece or the end of the stream.
fn read_line_capped(reader: &mut impl BufRead, buf: &mut Vec<u8>) -> io::Result<bool> {
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(false);
        }
        let room = LINE_CAP - buf.len();
        let window = &available[..available.len().min(room)];
        match window.iter().position(|&b| b == b'\n') {
            Some(at) => {
                buf.extend_from_slice(&window[..=at]);
                reader.consume(at + 1);
                return Ok(true);
            }
            None => {
                let n = window.len();
                buf.extend_from_slice(window);
                reader.consume(n);
                if buf.len() >= LINE_CAP {
                    return Ok(false);
                }
            }
        }
    }
}

/// The line, cut to a window around the match when it is long. Offsets are in characters.
fn snippet(line: u64, text: &str, start: usize, end: usize) -> LineHit {
    let start_chars = text[..start].chars().count();
    let end_chars = start_chars + text[start..end].chars().count();
    let total = text[..start].chars().count() + text[start..].chars().count();
    if total <= SNIPPET {
        return LineHit {
            line,
            text: text.to_string(),
            start: start_chars,
            end: end_chars,
        };
    }
    let from = start_chars.saturating_sub(60);
    let to = (end_chars + 120).min(total);
    let window: String = text.chars().skip(from).take(to - from).collect();
    LineHit {
        line,
        text: window,
        start: start_chars - from,
        end: end_chars - from,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::text_matcher;

    fn run(bytes: &[u8], query: &TextQuery) -> Scan {
        let matcher = text_matcher(query).unwrap();
        scan(&mut &bytes[..], query, &matcher, &|| false).unwrap()
    }

    #[test]
    fn finds_lines_with_numbers() {
        let scan = run(
            b"one\ntwo needle\nthree\r\nneedle again\n",
            &TextQuery::new("needle"),
        );
        assert_eq!(scan.matching_lines, 2);
        assert_eq!(scan.lines[0].line, 2);
        assert_eq!(scan.lines[0].text, "two needle");
        assert_eq!((scan.lines[0].start, scan.lines[0].end), (4, 10));
        assert_eq!(scan.lines[1].line, 4);
        assert_eq!(scan.lines[1].text, "needle again", "CR is trimmed");
    }

    #[test]
    fn the_last_line_needs_no_newline() {
        let scan = run(b"a\nneedle", &TextQuery::new("needle"));
        assert_eq!(scan.lines[0].line, 2);
    }

    #[test]
    fn utf8_and_cp1250_files_are_both_found_in_auto_mode() {
        let query = TextQuery::new("zażółć");
        let utf8 = run("tu jest zażółć gęślą\n".as_bytes(), &query);
        assert_eq!(utf8.matching_lines, 1);
        // The same sentence in Windows-1250.
        let cp1250: Vec<u8> = b"tu jest za\xBF\xF3\xB3\xE6 g\xEA\x9Cl\xB9\n".to_vec();
        let found = run(&cp1250, &query);
        assert_eq!(found.matching_lines, 1);
        assert_eq!(found.lines[0].text, "tu jest zażółć gęślą");
        // Forcing UTF-8 does not find the 1250 text.
        let forced = TextQuery {
            encoding: Encoding::Utf8,
            ..query
        };
        assert_eq!(run(&cp1250, &forced).matching_lines, 0);
    }

    #[test]
    fn case_insensitive_matching_covers_polish_letters() {
        let found = run("ŁÓDŹ\n".as_bytes(), &TextQuery::new("łódź"));
        assert_eq!(found.matching_lines, 1);
    }

    #[test]
    fn utf16_with_a_bom_is_read() {
        for big_endian in [false, true] {
            let mut bytes = if big_endian {
                vec![0xFE, 0xFF]
            } else {
                vec![0xFF, 0xFE]
            };
            for unit in "first\r\nsecond needle\nthird".encode_utf16() {
                let pair = if big_endian {
                    unit.to_be_bytes()
                } else {
                    unit.to_le_bytes()
                };
                bytes.extend_from_slice(&pair);
            }
            let found = run(&bytes, &TextQuery::new("needle"));
            assert_eq!(found.matching_lines, 1, "big_endian={big_endian}");
            assert_eq!(found.lines[0].line, 2);
            assert_eq!(found.lines[0].text, "second needle");
        }
    }

    #[test]
    fn a_utf8_bom_is_skipped() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"needle");
        let query = TextQuery {
            regex: true,
            ..TextQuery::new("^needle")
        };
        assert_eq!(run(&bytes, &query).matching_lines, 1);
    }

    #[test]
    fn binary_data_is_skipped_unless_asked() {
        let bytes = b"needle\0\0\0 more";
        let skipped = run(bytes, &TextQuery::new("needle"));
        assert!(skipped.binary && skipped.matching_lines == 0);
        let query = TextQuery {
            search_binary: true,
            ..TextQuery::new("needle")
        };
        assert_eq!(run(bytes, &query).matching_lines, 1);
    }

    #[test]
    fn long_lines_are_cut_around_the_match() {
        let line = format!("{}needle{}\n", "x".repeat(500), "y".repeat(500));
        let found = run(line.as_bytes(), &TextQuery::new("needle"));
        let hit = &found.lines[0];
        assert!(hit.text.chars().count() <= 200, "{}", hit.text.len());
        let chars: Vec<char> = hit.text.chars().collect();
        assert_eq!(
            chars[hit.start..hit.end].iter().collect::<String>(),
            "needle"
        );
    }

    #[test]
    fn huge_lines_do_not_blow_memory_and_matches_are_capped() {
        let big = vec![b'a'; LINE_CAP * 2 + 10];
        assert_eq!(run(&big, &TextQuery::new("b")).matching_lines, 0);
        let many = "needle\n".repeat(5000);
        let found = run(many.as_bytes(), &TextQuery::new("needle"));
        assert_eq!(found.matching_lines, MAX_COUNTED_LINES);
        assert_eq!(found.lines.len(), MAX_SHOWN_LINES);
    }

    #[test]
    fn cancelling_stops_reading() {
        let query = TextQuery::new("needle");
        let matcher = text_matcher(&query).unwrap();
        let found = scan(&mut &b"needle\n"[..], &query, &matcher, &|| true).unwrap();
        assert_eq!(found.matching_lines, 0);
    }
}
