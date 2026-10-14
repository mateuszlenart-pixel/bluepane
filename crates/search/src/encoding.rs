//! Windows-1250 (Central European) decoding; UTF-8 and UTF-16 come from std.

/// Characters for the bytes 0x80..=0xFF; U+FFFD marks the five undefined positions.
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

pub fn decode_cp1250(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| {
            if b < 0x80 {
                char::from(b)
            } else {
                CP1250_HIGH[usize::from(b - 0x80)]
            }
        })
        .collect()
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
    fn the_table_has_the_right_shape() {
        assert_eq!(CP1250_HIGH.len(), 128);
        assert_eq!(decode_cp1250(&[0x80]), "€");
        assert_eq!(decode_cp1250(&[0xFF]), "˙");
        assert_eq!(decode_cp1250(&[0x81]), "\u{FFFD}");
    }
}
