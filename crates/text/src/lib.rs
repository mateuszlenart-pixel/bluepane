//! Single-byte encodings that more than one crate needs (search decodes whole files, the
//! RTF reader single escaped bytes). One table here instead of a copy in each.

/// Characters for the Windows-1250 (Central European) bytes 0x80..=0xFF; U+FFFD marks the
/// five undefined positions.
const CP1250_HIGH: [char; 128] = [
    '€', '\u{FFFD}', '‚', '\u{FFFD}', '„', '…', '†', '‡', '\u{FFFD}', '‰', 'Š', '‹', 'Ś', 'Ť', 'Ž',
    'Ź', '\u{FFFD}', '‘', '’', '“', '”', '•', '–', '—', '\u{FFFD}', '™', 'š', '›', 'ś', 'ť', 'ž',
    'ź', '\u{A0}', 'ˇ', '˘', 'Ł', '¤', 'Ą', '¦', '§', '¨', '©', 'Ş', '«', '¬', '\u{AD}', '®', 'Ż',
    '°', '±', '˛', 'ł', '´', 'µ', '¶', '·', '¸', 'ą', 'ş', '»', 'Ľ', '˝', 'ľ', 'ż', 'Ŕ', 'Á', 'Â',
    'Ă', 'Ä', 'Ĺ', 'Ć', 'Ç', 'Č', 'É', 'Ę', 'Ë', 'Ě', 'Í', 'Î', 'Ď', 'Đ', 'Ń', 'Ň', 'Ó', 'Ô', 'Ő',
    'Ö', '×', 'Ř', 'Ů', 'Ú', 'Ű', 'Ü', 'Ý', 'Ţ', 'ß', 'ŕ', 'á', 'â', 'ă', 'ä', 'ĺ', 'ć', 'ç', 'č',
    'é', 'ę', 'ë', 'ě', 'í', 'î', 'ď', 'đ', 'ń', 'ň', 'ó', 'ô', 'ő', 'ö', '÷', 'ř', 'ů', 'ú', 'ű',
    'ü', 'ý', 'ţ', '˙',
];

/// The characters of Windows-1252 in 0x80..0xA0 that differ from Latin-1.
const CP1252_C1: [char; 32] = [
    '€', '\u{FFFD}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{FFFD}', 'Ž',
    '\u{FFFD}', '\u{FFFD}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{FFFD}',
    'ž', 'Ÿ',
];

/// One Windows-1250 byte.
pub fn cp1250_char(byte: u8) -> char {
    if byte < 0x80 {
        char::from(byte)
    } else {
        CP1250_HIGH[usize::from(byte - 0x80)]
    }
}

/// One Windows-1252 byte (Western European; the rest of the upper half is Latin-1).
pub fn cp1252_char(byte: u8) -> char {
    match byte {
        0x80..=0x9F => CP1252_C1[usize::from(byte - 0x80)],
        other => char::from(other),
    }
}

/// Windows-1250 text as a `String`.
pub fn decode_cp1250(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| cp1250_char(b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polish_letters_decode() {
        let small = [0xB9, 0xE6, 0xEA, 0xB3, 0xF1, 0xF3, 0x9C, 0x9F, 0xBF];
        assert_eq!(decode_cp1250(&small), "ąćęłńóśźż");
        let big = [0xA5, 0xC6, 0xCA, 0xA3, 0xD1, 0xD3, 0x8C, 0x8F, 0xAF];
        assert_eq!(decode_cp1250(&big), "ĄĆĘŁŃÓŚŹŻ");
        assert_eq!(decode_cp1250(b"plain ASCII"), "plain ASCII");
    }

    #[test]
    fn the_tables_have_the_right_shape() {
        assert_eq!(CP1250_HIGH.len(), 128);
        assert_eq!(cp1250_char(0x80), '€');
        assert_eq!(cp1250_char(0xFF), '˙');
        assert_eq!(cp1250_char(0x81), '\u{FFFD}');
        assert_eq!(cp1252_char(0x80), '€');
        assert_eq!(cp1252_char(0xE9), 'é', "Latin-1 range");
        assert_eq!(cp1252_char(b'a'), 'a');
    }
}
