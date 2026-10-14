//! Matching names (masks or regular expressions) and text.

use regex::{Regex, RegexBuilder};

use crate::TextQuery;

/// A list of `;`-separated masks (`*`, `?`) compiled to one regular expression.
pub(crate) fn mask_regex(masks: &str, case_sensitive: bool) -> Result<Option<Regex>, String> {
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
                    other => regex::escape(&other.to_string()),
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
) -> Result<Option<Regex>, String> {
    if name.trim().is_empty() {
        return Ok(None);
    }
    if as_regex {
        build(name, case_sensitive).map(Some)
    } else {
        mask_regex(name, case_sensitive)
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
        let re = name_matcher(masks, false, case).unwrap().unwrap();
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
        assert!(name_matcher("  ", false, false).unwrap().is_none());
        assert!(name_matcher("(", true, false).is_err());
        let re = name_matcher(r"^img_\d+\.jpg$", true, false)
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
}
