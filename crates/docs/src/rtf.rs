//! RTF documents: the text, without formatting, pictures, fonts or headers and footers.

use std::path::Path;

use crate::DocError;

pub(crate) fn extract(path: &Path) -> Result<String, DocError> {
    let bytes = std::fs::read(path)?;
    if !bytes.starts_with(b"{\\rtf") {
        return Err(DocError::Corrupt("to nie jest plik RTF".into()));
    }
    let text = text_of(&bytes);
    if text.trim().is_empty() {
        return Err(DocError::Corrupt("dokument nie zawiera tekstu".into()));
    }
    Ok(text)
}

/// Groups whose content is not text of the document.
const SKIPPED: &[&str] = &[
    "fonttbl",
    "colortbl",
    "stylesheet",
    "info",
    "pict",
    "header",
    "headerl",
    "headerr",
    "headerf",
    "footer",
    "footerl",
    "footerr",
    "footerf",
    "object",
    "listtable",
    "listoverridetable",
    "generator",
    "themedata",
    "datastore",
    "latentstyles",
    "fldinst",
    "revtbl",
    "rsidtbl",
    "filetbl",
    "xe",
    "tc",
    "private",
];

#[derive(Clone)]
struct State {
    /// The text of this group is dropped.
    skip: bool,
    /// How many characters follow a `\u` escape as its plain replacement.
    uc: usize,
}

pub(crate) fn text_of(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut stack: Vec<State> = Vec::new();
    let mut state = State { skip: false, uc: 1 };
    let mut codepage = 1252;
    // Replacement characters still to be dropped after a `\u` escape.
    let mut skip_chars = 0usize;
    let mut i = 0;
    // `{\*\word ...}` is a destination to be ignored; the flag waits for the next word.
    let mut starred = false;

    while i < bytes.len() {
        let byte = bytes[i];
        i += 1;
        match byte {
            b'{' => {
                stack.push(state.clone());
                starred = false;
            }
            b'}' => {
                if let Some(previous) = stack.pop() {
                    state = previous;
                }
                skip_chars = 0;
            }
            b'\r' | b'\n' => {}
            b'\\' => {
                let Some(&next) = bytes.get(i) else { break };
                i += 1;
                match next {
                    b'\\' | b'{' | b'}' => {
                        emit(&mut out, &state, &mut skip_chars, char::from(next));
                    }
                    b'\'' => {
                        let hex = bytes
                            .get(i..i + 2)
                            .and_then(|h| std::str::from_utf8(h).ok());
                        if let Some(value) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                            i += 2;
                            emit(&mut out, &state, &mut skip_chars, decode(value, codepage));
                        }
                    }
                    b'*' => starred = true,
                    b'~' => emit(&mut out, &state, &mut skip_chars, '\u{a0}'),
                    b'_' => emit(&mut out, &state, &mut skip_chars, '-'),
                    b'-' | b'|' | b':' => {}
                    b'\r' | b'\n' => emit(&mut out, &state, &mut skip_chars, '\n'),
                    letter if letter.is_ascii_alphabetic() => {
                        let start = i - 1;
                        while bytes.get(i).is_some_and(u8::is_ascii_alphabetic) {
                            i += 1;
                        }
                        let word = std::str::from_utf8(&bytes[start..i]).unwrap_or("");
                        let negative = bytes.get(i) == Some(&b'-');
                        if negative {
                            i += 1;
                        }
                        let digits_start = i;
                        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                            i += 1;
                        }
                        let number: Option<i64> = std::str::from_utf8(&bytes[digits_start..i])
                            .ok()
                            .and_then(|d| d.parse().ok())
                            .map(|n: i64| if negative { -n } else { n });
                        // One space after a control word belongs to it.
                        if bytes.get(i) == Some(&b' ') {
                            i += 1;
                        }
                        if starred || SKIPPED.contains(&word) {
                            state.skip = true;
                        }
                        starred = false;
                        match word {
                            "par" | "line" | "sect" | "page" => {
                                emit(&mut out, &state, &mut skip_chars, '\n')
                            }
                            "tab" | "cell" => emit(&mut out, &state, &mut skip_chars, '\t'),
                            "row" => emit(&mut out, &state, &mut skip_chars, '\n'),
                            "emdash" => emit(&mut out, &state, &mut skip_chars, '—'),
                            "endash" => emit(&mut out, &state, &mut skip_chars, '–'),
                            "bullet" => emit(&mut out, &state, &mut skip_chars, '•'),
                            "lquote" => emit(&mut out, &state, &mut skip_chars, '‘'),
                            "rquote" => emit(&mut out, &state, &mut skip_chars, '’'),
                            "ldblquote" => emit(&mut out, &state, &mut skip_chars, '“'),
                            "rdblquote" => emit(&mut out, &state, &mut skip_chars, '”'),
                            "ansicpg" => codepage = number.unwrap_or(1252),
                            "uc" => state.uc = number.unwrap_or(1).clamp(0, 8) as usize,
                            "u" => {
                                if let Some(n) = number {
                                    let code = if n < 0 { n + 65536 } else { n };
                                    if let Some(c) =
                                        u32::try_from(code).ok().and_then(char::from_u32)
                                    {
                                        emit(&mut out, &state, &mut skip_chars, c);
                                    }
                                    skip_chars = state.uc;
                                }
                            }
                            // Binary data follows: skip it whole.
                            "bin" => i = i.saturating_add(number.unwrap_or(0).max(0) as usize),
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            other => {
                emit(&mut out, &state, &mut skip_chars, decode(other, codepage));
                starred = false;
            }
        }
    }
    tidy(&out)
}

fn emit(out: &mut String, state: &State, skip_chars: &mut usize, c: char) {
    if state.skip {
        return;
    }
    if *skip_chars > 0 {
        *skip_chars -= 1;
        return;
    }
    out.push(c);
}

/// No more than one blank line in a row, no trailing blanks.
fn tidy(text: &str) -> String {
    let mut out = String::new();
    let mut blank = 0;
    for line in text.split('\n') {
        let line = line.trim_end();
        if line.is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim_start_matches('\n').trim_end().to_string() + "\n"
}

fn decode(byte: u8, codepage: i64) -> char {
    match codepage {
        1250 => bluepane_text::cp1250_char(byte),
        // Latin-1 range for everything else (Western European, the usual default).
        _ => bluepane_text::cp1252_char(byte),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(rtf: &str) -> String {
        text_of(rtf.as_bytes())
    }

    #[test]
    fn paragraphs_and_formatting_words_become_plain_text() {
        let rtf = "{\\rtf1\\ansi\\deff0{\\fonttbl{\\f0 Arial;}}{\\colortbl;\\red0\\green0\\blue0;}\\f0\\fs24 Hello \\b world\\b0\\par Second\\tab line\\par\\par\\par Third\\par}";
        assert_eq!(text(rtf), "Hello world\nSecond\tline\n\nThird\n");
    }

    #[test]
    fn polish_letters_come_from_the_codepage_and_from_unicode_escapes() {
        let rtf = "{\\rtf1\\ansi\\ansicpg1250 \\'bf\\'f3\\'b3w \\u261?\\u322?\\u-10179?}";
        // ż ó ł w, then ą ł (unicode with a one character fallback), then a surrogate half
        let out = text(rtf);
        assert!(out.starts_with("żółw ął"), "{out}");
    }

    #[test]
    fn headers_pictures_and_hidden_destinations_are_dropped() {
        let rtf = "{\\rtf1{\\header Page header}{\\*\\generator Word}{\\pict\\jpegblip ffd8ffe0}Body text\\par{\\footer Page footer}}";
        assert_eq!(text(rtf), "Body text\n");
    }

    #[test]
    fn escapes_and_special_characters() {
        let rtf = "{\\rtf1 a\\{b\\}c \\\\ \\emdash \\bullet \\~x}";
        assert_eq!(text(rtf), "a{b}c \\ —•\u{a0}x\n");
    }

    #[test]
    fn a_file_that_is_not_rtf_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.rtf");
        std::fs::write(&path, "plain text").unwrap();
        assert!(matches!(extract(&path), Err(DocError::Corrupt(_))));
        std::fs::write(&path, "{\\rtf1 }").unwrap();
        assert!(matches!(extract(&path), Err(DocError::Corrupt(_))));
        std::fs::write(&path, "{\\rtf1 Hi}").unwrap();
        assert_eq!(extract(&path).unwrap(), "Hi\n");
    }
}
