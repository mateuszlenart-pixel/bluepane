//! File masks for selecting entries: `*.txt`, `report??.md`, `*.jpg;*.png`.

/// A list of glob patterns separated by `;`. `*` matches any run of characters, `?` exactly
/// one. Matching ignores case. `*.*` means "everything", as in Total Commander.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mask {
    patterns: Vec<Vec<char>>,
}

impl Mask {
    /// Parses a mask. Returns `None` when it contains no pattern at all.
    pub fn parse(text: &str) -> Option<Mask> {
        let patterns: Vec<Vec<char>> = text
            .split(';')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(|p| if p == "*.*" { "*" } else { p })
            .map(|p| p.to_lowercase().chars().collect())
            .collect();
        (!patterns.is_empty()).then_some(Mask { patterns })
    }

    pub fn matches(&self, name: &str) -> bool {
        let name: Vec<char> = name.to_lowercase().chars().collect();
        self.patterns.iter().any(|p| glob(p, &name))
    }
}

/// Iterative glob matcher with single-star backtracking (linear space, no recursion).
fn glob(pattern: &[char], text: &[char]) -> bool {
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, t));
            p += 1;
        } else if let Some((star_p, star_t)) = star {
            p = star_p + 1;
            t = star_t + 1;
            star = Some((star_p, star_t + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(text: &str) -> Mask {
        Mask::parse(text).unwrap()
    }

    #[test]
    fn star_and_question_mark() {
        assert!(m("*.txt").matches("notes.txt"));
        assert!(m("*.txt").matches(".txt"));
        assert!(!m("*.txt").matches("notes.txt.bak"));
        assert!(m("report??.md").matches("report01.md"));
        assert!(!m("report??.md").matches("report1.md"));
        assert!(m("a*b*c").matches("aXXbYYc"));
        assert!(!m("a*b*c").matches("aXXbYY"));
        assert!(m("*").matches(""));
    }

    #[test]
    fn matching_ignores_case() {
        assert!(m("*.TXT").matches("Notes.txt"));
        assert!(m("readme*").matches("README.md"));
    }

    #[test]
    fn several_patterns_and_star_dot_star() {
        let mask = m("*.jpg; *.png ;");
        assert!(mask.matches("a.jpg") && mask.matches("b.PNG"));
        assert!(!mask.matches("c.gif"));
        // "*.*" selects files without an extension too.
        assert!(m("*.*").matches("Makefile"));
    }

    #[test]
    fn literal_pattern_must_match_the_whole_name() {
        assert!(m("a.txt").matches("A.txt"));
        assert!(!m("a.txt").matches("ba.txt"));
    }

    #[test]
    fn empty_mask_is_rejected() {
        assert!(Mask::parse("").is_none());
        assert!(Mask::parse(" ; ").is_none());
    }

    #[test]
    fn many_stars_do_not_blow_up() {
        let name = "a".repeat(5000);
        assert!(!m("*a*a*a*a*a*b").matches(&name));
    }
}
