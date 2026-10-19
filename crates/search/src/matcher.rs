//! Matching names (masks or regular expressions) and text.

use regex::{Regex, RegexBuilder};

use crate::TextQuery;

/// The letters a Polish letter is written as without its accent, both ways: with `fold`
/// set, `o` and `ó` both stand for `[oó]`. Other characters have no class.
fn polish_class(c: char) -> Option<&'static str> {
    Some(match c {
        'a' | 'ą' => "[aą]",
        'c' | 'ć' => "[cć]",
        'e' | 'ę' => "[eę]",
        'l' | 'ł' => "[lł]",
        'n' | 'ń' => "[nń]",
        'o' | 'ó' => "[oó]",
        's' | 'ś' => "[sś]",
        'z' | 'ź' | 'ż' => "[zźż]",
        'A' | 'Ą' => "[AĄ]",
        'C' | 'Ć' => "[CĆ]",
        'E' | 'Ę' => "[EĘ]",
        'L' | 'Ł' => "[LŁ]",
        'N' | 'Ń' => "[NŃ]",
        'O' | 'Ó' => "[OÓ]",
        'S' | 'Ś' => "[SŚ]",
        'Z' | 'Ź' | 'Ż' => "[ZŹŻ]",
        _ => return None,
    })
}

/// A list of `;`-separated masks (`*`, `?`) compiled to one regular expression.
pub(crate) fn mask_regex(masks: &str, case_sensitive: bool) -> Result<Option<Regex>, String> {
    mask_regex_folding(masks, case_sensitive, false)
}

/// Like [`mask_regex`]; with `fold_accents` a letter matches itself with or without its
/// Polish accent (`zolw` finds `żółw` and the other way round). The names are not changed.
pub(crate) fn mask_regex_folding(
    masks: &str,
    case_sensitive: bool,
    fold_accents: bool,
) -> Result<Option<Regex>, String> {
    let patterns: Vec<String> = masks
        .split(';')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| if p == "*.*" { "*" } else { p })
        .map(|p| {
            p.chars()
                .map(|c| match c {
                    '*' => ".*".to_string(),
                    '?' => ".".to_string(),
                    other => match polish_class(other).filter(|_| fold_accents) {
                        Some(class) => class.to_string(),
                        None => regex::escape(&other.to_string()),
                    },
                })
                .collect::<String>()
        })
        .collect();
    if patterns.is_empty() {
        return Ok(None);
    }
    build(&format!("^(?:{})$", patterns.join("|")), case_sensitive).map(Some)
}

/// `Ok(None)` means "every name".
pub(crate) fn name_matcher(
    name: &str,
    as_regex: bool,
    case_sensitive: bool,
    fold_accents: bool,
) -> Result<Option<Regex>, String> {
    if name.trim().is_empty() {
        return Ok(None);
    }
    if as_regex {
        build(name, case_sensitive).map(Some)
    } else {
        mask_regex_folding(name, case_sensitive, fold_accents)
    }
}

pub(crate) fn text_matcher(query: &TextQuery) -> Result<Regex, String> {
    let core = if query.regex {
        query.pattern.clone()
    } else {
        regex::escape(&query.pattern)
    };
    let pattern = if query.whole_words {
        format!(r"\b(?:{core})\b")
    } else {
        core
    };
    build(&pattern, query.case_sensitive)
}

fn build(pattern: &str, case_sensitive: bool) -> Result<Regex, String> {
    RegexBuilder::new(pattern)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|e| format!("niepoprawne wyrażenie: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(masks: &str, case: bool, tests: &[&str]) -> Vec<bool> {
        let re = name_matcher(masks, false, case, false).unwrap().unwrap();
        tests.iter().map(|t| re.is_match(t)).collect()
    }

    #[test]
    fn masks_match_whole_names_ignoring_case() {
        assert_eq!(
            names("*.rs;*.md", false, &["a.rs", "B.MD", "a.rsx", "rs"]),
            [true, true, false, false]
        );
        assert_eq!(
            names("report??.txt", false, &["report01.txt", "report1.txt"]),
            [true, false]
        );
        assert_eq!(names("*.*", false, &["noext", "a.b"]), [true, true]);
        assert_eq!(
            names("a.b", false, &["a.b", "axb"]),
            [true, false],
            "dot is literal"
        );
    }

    #[test]
    fn case_sensitive_masks() {
        assert_eq!(names("*.TXT", true, &["a.TXT", "a.txt"]), [true, false]);
    }

    #[test]
    fn empty_name_means_everything_and_regex_is_checked() {
        assert!(name_matcher("  ", false, false, false).unwrap().is_none());
        assert!(name_matcher("(", true, false, false).is_err());
        let re = name_matcher(r"^img_\d+\.jpg$", true, false, false)
            .unwrap()
            .unwrap();
        assert!(re.is_match("IMG_01.jpg"));
    }

    #[test]
    fn text_options() {
        let mut q = TextQuery::new("a.b");
        assert!(
            !text_matcher(&q).unwrap().is_match("axb"),
            "literal by default"
        );
        q.regex = true;
        assert!(text_matcher(&q).unwrap().is_match("axb"));
        let mut q = TextQuery::new("cat");
        q.whole_words = true;
        let re = text_matcher(&q).unwrap();
        assert!(re.is_match("a cat sat") && !re.is_match("concatenate"));
        q.case_sensitive = true;
        assert!(!text_matcher(&q).unwrap().is_match("A CAT"));
    }

    #[test]
    fn masks_can_ignore_polish_accents_both_ways_without_losing_exact_matches() {
        let folding = name_matcher("zolw*", false, false, true).unwrap().unwrap();
        assert!(folding.is_match("żółw.txt"));
        assert!(folding.is_match("Żółw.txt"), "case is still ignored");
        assert!(folding.is_match("zolw.txt"));
        let accented = name_matcher("żółw*", false, false, true).unwrap().unwrap();
        assert!(accented.is_match("zolw.txt"), "the other way round too");
        assert!(!folding.is_match("zalw.txt"), "other letters stay distinct");
        // Without folding the accent matters.
        let exact = name_matcher("zolw*", false, false, false).unwrap().unwrap();
        assert!(!exact.is_match("żółw.txt"));
        // With case sensitivity the accent is still folded, the case is not.
        let both = name_matcher("Łódź*", false, true, true).unwrap().unwrap();
        assert!(both.is_match("Lodz.txt") && !both.is_match("lodz.txt"));
        // Regular expressions are taken as written.
        let re = name_matcher("^zolw", true, false, true).unwrap().unwrap();
        assert!(!re.is_match("żółw"));
    }
}
