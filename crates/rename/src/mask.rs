//! Name masks such as `[N]_[C:3]` or `[YMD] [N1-8]`.

use crate::{Meta, Stamp};

/// Which characters of a field to take. Positions are 1-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Range {
    Whole,
    /// `[N3]`: one character.
    At(usize),
    /// `[N2-5]` or `[N2-]`: from..=to (to = end when `None`).
    Span(usize, Option<usize>),
    /// `[N2,3]`: start and length.
    Len(usize, usize),
}

impl Range {
    fn parse(text: &str) -> Result<Range, String> {
        let bad = || format!("niepoprawny zakres znaków: {text:?}");
        let number = |s: &str| s.parse::<usize>().ok().filter(|n| *n > 0).ok_or_else(bad);
        if text.is_empty() {
            Ok(Range::Whole)
        } else if let Some((a, b)) = text.split_once('-') {
            let from = number(a)?;
            if b.is_empty() {
                Ok(Range::Span(from, None))
            } else {
                Ok(Range::Span(from, Some(number(b)?)))
            }
        } else if let Some((a, b)) = text.split_once(',') {
            Ok(Range::Len(number(a)?, number(b)?))
        } else {
            Ok(Range::At(number(text)?))
        }
    }

    fn apply(self, text: &str) -> String {
        let chars = text.chars();
        match self {
            Range::Whole => text.to_string(),
            Range::At(n) => chars.skip(n - 1).take(1).collect(),
            Range::Span(from, to) => {
                let skip = from - 1;
                match to {
                    Some(to) if to >= from => chars.skip(skip).take(to - from + 1).collect(),
                    Some(_) => String::new(),
                    None => chars.skip(skip).collect(),
                }
            }
            Range::Len(from, len) => chars.skip(from - 1).take(len).collect(),
        }
    }
}

/// `[C10+5:3]`: parts left out fall back to the form's counter settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct CounterSpec {
    pub start: Option<i64>,
    pub step: Option<i64>,
    pub digits: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DatePart {
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
}

impl DatePart {
    fn is_date(self) -> bool {
        matches!(self, DatePart::Year | DatePart::Month | DatePart::Day)
    }

    fn value(self, stamp: &Stamp) -> String {
        match self {
            DatePart::Year => format!("{:04}", stamp.year),
            DatePart::Month => format!("{:02}", stamp.month),
            DatePart::Day => format!("{:02}", stamp.day),
            DatePart::Hour => format!("{:02}", stamp.hour),
            DatePart::Minute => format!("{:02}", stamp.minute),
            DatePart::Second => format!("{:02}", stamp.second),
        }
    }
}

/// A text or number read from inside the file: `[EXIF:Make]`, `[TAG:artist]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetaField {
    Make,
    Model,
    Artist,
    Title,
    Album,
    Genre,
    Track,
    Year,
}

impl MetaField {
    fn value(self, meta: &Meta) -> String {
        let text = |v: &Option<String>| v.clone().unwrap_or_default();
        match self {
            MetaField::Make => text(&meta.make),
            MetaField::Model => text(&meta.model),
            MetaField::Artist => text(&meta.artist),
            MetaField::Title => text(&meta.title),
            MetaField::Album => text(&meta.album),
            MetaField::Genre => text(&meta.genre),
            // Two digits keep tracks in order when sorted by name.
            MetaField::Track => meta.track.map(|t| format!("{t:02}")).unwrap_or_default(),
            MetaField::Year => meta.year.map(|y| format!("{y:04}")).unwrap_or_default(),
        }
    }
}

/// Text from a tag can hold characters a file name cannot: they become `_`.
fn file_safe(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Token {
    Literal(String),
    Name(Range),
    Ext(Range),
    Parent(Range),
    Counter(CounterSpec),
    Date(Vec<DatePart>),
    /// `[EXIF:YMD]`: when a photo was taken.
    Taken(Vec<DatePart>),
    Meta(MetaField),
}

/// A parsed mask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Mask(pub Vec<Token>);

/// What a mask can draw from for one item.
pub(crate) struct Source<'a> {
    pub name: &'a str,
    pub ext: &'a str,
    pub parent: &'a str,
    pub modified: Option<&'a Stamp>,
    /// What was read from inside the file, if a mask asked for it.
    pub meta: Option<&'a Meta>,
    pub counter: i64,
    pub counter_digits: usize,
}

impl Mask {
    pub(crate) fn parse(text: &str) -> Result<Mask, String> {
        let mut tokens = Vec::new();
        let mut literal = String::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '[' {
                literal.push(c);
                continue;
            }
            let mut inner = String::new();
            let mut closed = false;
            for c in chars.by_ref() {
                if c == ']' {
                    closed = true;
                    break;
                }
                inner.push(c);
            }
            if !closed {
                return Err(format!("brak zamykającego ] w masce {text:?}"));
            }
            match inner.as_str() {
                // `[[]` and `[]]` stand for literal brackets.
                "[" => literal.push('['),
                "" if chars.peek() == Some(&']') => {
                    chars.next();
                    literal.push(']');
                }
                "" => return Err(format!("puste pole [] w masce {text:?}")),
                _ => {
                    if !literal.is_empty() {
                        tokens.push(Token::Literal(std::mem::take(&mut literal)));
                    }
                    tokens.push(parse_field(&inner)?);
                }
            }
        }
        if !literal.is_empty() {
            tokens.push(Token::Literal(literal));
        }
        Ok(Mask(tokens))
    }

    pub(crate) fn render(&self, src: &Source<'_>) -> String {
        let mut out = String::new();
        for token in &self.0 {
            match token {
                Token::Literal(text) => out.push_str(text),
                Token::Name(range) => out.push_str(&range.apply(src.name)),
                Token::Ext(range) => out.push_str(&range.apply(src.ext)),
                Token::Parent(range) => out.push_str(&range.apply(src.parent)),
                Token::Counter(spec) => {
                    // `src.counter` already honours the spec's start and step.
                    let digits = spec.digits.unwrap_or(src.counter_digits);
                    out.push_str(&format!("{:0>digits$}", src.counter));
                }
                Token::Date(parts) => {
                    if let Some(stamp) = src.modified {
                        push_date(&mut out, parts, stamp);
                    }
                }
                Token::Taken(parts) => {
                    if let Some(stamp) = src.meta.and_then(|m| m.taken.as_ref()) {
                        push_date(&mut out, parts, stamp);
                    }
                }
                Token::Meta(field) => {
                    if let Some(meta) = src.meta {
                        out.push_str(&file_safe(&field.value(meta)));
                    }
                }
            }
        }
        out
    }

    /// True when the mask draws on data read from inside the file.
    pub(crate) fn needs_meta(&self) -> bool {
        self.0
            .iter()
            .any(|t| matches!(t, Token::Taken(_) | Token::Meta(_)))
    }

    /// Counter overrides in the mask, if any (the first one wins).
    pub(crate) fn counter_spec(&self) -> Option<CounterSpec> {
        self.0.iter().find_map(|t| match t {
            Token::Counter(spec) => Some(*spec),
            _ => None,
        })
    }
}

/// `YMDhms` letters as date parts (empty when some letter is something else).
fn date_parts(text: &str) -> Vec<DatePart> {
    if text.is_empty() || !text.chars().all(|c| "YMDhms".contains(c)) {
        return Vec::new();
    }
    text.chars()
        .map(|c| match c {
            'Y' => DatePart::Year,
            'M' => DatePart::Month,
            'D' => DatePart::Day,
            'h' => DatePart::Hour,
            'm' => DatePart::Minute,
            _ => DatePart::Second,
        })
        .collect()
}

/// Joins date and time parts with separators that are safe in a file name.
fn push_date(out: &mut String, parts: &[DatePart], stamp: &Stamp) {
    let mut prev: Option<DatePart> = None;
    for part in parts {
        if let Some(prev) = prev {
            out.push(match (prev.is_date(), part.is_date()) {
                (true, true) => '-',
                (false, false) => '.',
                _ => '_',
            });
        }
        out.push_str(&part.value(stamp));
        prev = Some(*part);
    }
}

/// `[EXIF:YMD]`, `[EXIF:Make]`, `[TAG:artist]`...
fn parse_meta_field(group: &str, name: &str, inner: &str) -> Result<Token, String> {
    let unknown = || {
        format!(
            "nieznane pole w masce: [{inner}] (EXIF: YMDhms, Make, Model; TAG: artist, title, album, genre, track, year)"
        )
    };
    if group.eq_ignore_ascii_case("EXIF") {
        let parts = date_parts(name);
        if !parts.is_empty() {
            return Ok(Token::Taken(parts));
        }
        return match name.to_ascii_lowercase().as_str() {
            "make" => Ok(Token::Meta(MetaField::Make)),
            "model" => Ok(Token::Meta(MetaField::Model)),
            _ => Err(unknown()),
        };
    }
    match name.to_ascii_lowercase().as_str() {
        "artist" => Ok(Token::Meta(MetaField::Artist)),
        "title" => Ok(Token::Meta(MetaField::Title)),
        "album" => Ok(Token::Meta(MetaField::Album)),
        "genre" => Ok(Token::Meta(MetaField::Genre)),
        "track" => Ok(Token::Meta(MetaField::Track)),
        "year" => Ok(Token::Meta(MetaField::Year)),
        _ => Err(unknown()),
    }
}

fn parse_field(inner: &str) -> Result<Token, String> {
    let unknown = || format!("nieznane pole w masce: [{inner}]");
    if let Some((group, name)) = inner.split_once(':') {
        if group.eq_ignore_ascii_case("EXIF") || group.eq_ignore_ascii_case("TAG") {
            return parse_meta_field(group, name, inner);
        }
    }
    let parts = date_parts(inner);
    if !parts.is_empty() {
        return Ok(Token::Date(parts));
    }
    let mut chars = inner.chars();
    let head = chars.next().ok_or_else(unknown)?;
    let rest = chars.as_str();
    match head {
        'N' => Range::parse(rest).map(Token::Name),
        'E' => Range::parse(rest).map(Token::Ext),
        'P' => Range::parse(rest).map(Token::Parent),
        'C' => parse_counter(rest).map(Token::Counter),
        _ => Err(unknown()),
    }
}

/// `""`, `"10"`, `"10+5"`, `"10+5:3"`, `":3"`.
fn parse_counter(text: &str) -> Result<CounterSpec, String> {
    let bad = || format!("niepoprawny licznik: [C{text}] (np. [C], [C10+5:3])");
    let mut spec = CounterSpec::default();
    let (head, digits) = match text.split_once(':') {
        Some((head, digits)) => (head, Some(digits)),
        None => (text, None),
    };
    if let Some(digits) = digits {
        spec.digits = Some(digits.parse().map_err(|_| bad())?);
    }
    let (start, step) = match head.split_once('+') {
        Some((start, step)) => (start, Some(step)),
        None => (head, None),
    };
    if !start.is_empty() {
        spec.start = Some(start.parse().map_err(|_| bad())?);
    }
    if let Some(step) = step {
        spec.step = Some(step.parse().map_err(|_| bad())?);
    }
    Ok(spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(mask: &str, name: &str, ext: &str) -> String {
        let stamp = Stamp {
            year: 2026,
            month: 10,
            day: 4,
            hour: 7,
            minute: 5,
            second: 9,
        };
        Mask::parse(mask).unwrap().render(&Source {
            name,
            ext,
            parent: "Photos",
            modified: Some(&stamp),
            meta: None,
            counter: 7,
            counter_digits: 3,
        })
    }

    #[test]
    fn name_and_extension_fields_take_ranges() {
        assert_eq!(render("[N]", "holiday", "jpg"), "holiday");
        assert_eq!(render("[N1-3]", "holiday", "jpg"), "hol");
        assert_eq!(render("[N5-]", "holiday", "jpg"), "day");
        assert_eq!(render("[N2,3]", "holiday", "jpg"), "oli");
        assert_eq!(render("[N3]", "holiday", "jpg"), "l");
        assert_eq!(render("[N30]", "holiday", "jpg"), "");
        assert_eq!(render("[E]", "holiday", "jpg"), "jpg");
        assert_eq!(render("[E1-2]", "holiday", "jpg"), "jp");
        assert_eq!(render("[P]", "x", ""), "Photos");
    }

    #[test]
    fn ranges_count_characters_not_bytes() {
        assert_eq!(render("[N2-3]", "zażółć", ""), "aż");
    }

    #[test]
    fn literals_and_escaped_brackets() {
        assert_eq!(render("img_[N]", "a", ""), "img_a");
        assert_eq!(render("[[][N][]]", "a", ""), "[a]");
    }

    #[test]
    fn counters_pad_to_the_requested_width() {
        assert_eq!(render("[C]", "a", ""), "007");
        assert_eq!(render("[C:5]", "a", ""), "00007");
        assert_eq!(render("[C10+2:4]", "a", ""), "0007");
    }

    #[test]
    fn dates_and_times_use_safe_separators() {
        assert_eq!(render("[Y]", "a", ""), "2026");
        assert_eq!(render("[YMD]", "a", ""), "2026-10-04");
        assert_eq!(render("[hms]", "a", ""), "07.05.09");
        assert_eq!(render("[YMDhms]", "a", ""), "2026-10-04_07.05.09");
    }

    fn render_meta(mask: &str, meta: &Meta) -> String {
        Mask::parse(mask).unwrap().render(&Source {
            name: "IMG_1",
            ext: "jpg",
            parent: "P",
            modified: None,
            meta: Some(meta),
            counter: 1,
            counter_digits: 1,
        })
    }

    #[test]
    fn exif_and_tag_fields_come_from_the_file_and_are_safe_in_a_name() {
        let meta = Meta {
            taken: Some(Stamp {
                year: 2026,
                month: 10,
                day: 4,
                hour: 7,
                minute: 5,
                second: 9,
            }),
            make: Some("Canon".into()),
            model: Some("EOS 5D".into()),
            artist: Some("AC/DC: Żółć".into()),
            title: Some("Gęślą?".into()),
            album: None,
            genre: None,
            track: Some(3),
            year: Some(1999),
        };
        assert_eq!(render_meta("[EXIF:YMD]_[N]", &meta), "2026-10-04_IMG_1");
        assert_eq!(render_meta("[exif:YMDhms]", &meta), "2026-10-04_07.05.09");
        assert_eq!(
            render_meta("[EXIF:Make] [EXIF:Model]", &meta),
            "Canon EOS 5D"
        );
        assert_eq!(
            render_meta("[TAG:track] - [TAG:artist] - [TAG:title]", &meta),
            "03 - AC_DC_ Żółć - Gęślą_"
        );
        assert_eq!(render_meta("[TAG:year]", &meta), "1999");
        // A missing value is simply nothing.
        assert_eq!(render_meta("a[TAG:album]b", &meta), "ab");
        assert_eq!(render_meta("a[EXIF:YMD]b", &Meta::default()), "ab");
        assert!(Mask::parse("[EXIF:YMD]").unwrap().needs_meta());
        assert!(Mask::parse("[TAG:title]").unwrap().needs_meta());
        assert!(!Mask::parse("[YMD][N]").unwrap().needs_meta());
    }

    #[test]
    fn bad_masks_report_what_is_wrong() {
        for bad in [
            "[N",
            "[X]",
            "[N0]",
            "[N3-x]",
            "[C1+x]",
            "[Nx]",
            "[EXIF:]",
            "[EXIF:Foo]",
            "[TAG:YMD]",
            "[TAG:]",
        ] {
            assert!(Mask::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn counter_spec_is_read_from_the_mask() {
        let mask = Mask::parse("a[C10+2:4]").unwrap();
        assert_eq!(
            mask.counter_spec(),
            Some(CounterSpec {
                start: Some(10),
                step: Some(2),
                digits: Some(4)
            })
        );
        assert_eq!(Mask::parse("[N]").unwrap().counter_spec(), None);
    }
}
