//! Name masks such as `[N]_[C:3]` or `[YMD] [N1-8]`.

use crate::Stamp;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Token {
    Literal(String),
    Name(Range),
    Ext(Range),
    Parent(Range),
    Counter(CounterSpec),
    Date(Vec<DatePart>),
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
                }
            }
        }
        out
    }

    /// Counter overrides in the mask, if any (the first one wins).
    pub(crate) fn counter_spec(&self) -> Option<CounterSpec> {
        self.0.iter().find_map(|t| match t {
            Token::Counter(spec) => Some(*spec),
            _ => None,
        })
    }
}

fn parse_field(inner: &str) -> Result<Token, String> {
    let unknown = || format!("nieznane pole w masce: [{inner}]");
    if inner.chars().all(|c| "YMDhms".contains(c)) {
        let parts = inner
            .chars()
            .map(|c| match c {
                'Y' => DatePart::Year,
                'M' => DatePart::Month,
                'D' => DatePart::Day,
                'h' => DatePart::Hour,
                'm' => DatePart::Minute,
                _ => DatePart::Second,
            })
            .collect();
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

    #[test]
    fn bad_masks_report_what_is_wrong() {
        for bad in ["[N", "[X]", "[N0]", "[N3-x]", "[C1+x]", "[Nx]"] {
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
