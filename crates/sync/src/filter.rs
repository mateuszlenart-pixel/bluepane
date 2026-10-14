//! Include / exclude filter for synchronization, written the way FreeFileSync users expect:
//! patterns separated by `;` or new lines, `*` and `?` wildcards, matching is case-insensitive.
//!
//! A pattern without a `/` matches the name of any file or directory on the way (so
//! `*.tmp` hides every `.tmp` file at any depth). A pattern with a `/` is anchored at the
//! synchronized folder (`build/*.o`), and a trailing `/` (`node_modules/`) matches
//! directories only. Excluding a directory excludes everything below it.

#[derive(Debug, Clone, PartialEq, Eq)]
struct Pattern {
    text: Vec<char>,
    anchored: bool,
    dirs_only: bool,
}

impl Pattern {
    fn parse(raw: &str) -> Option<Pattern> {
        let mut text = raw.trim().replace('\\', "/");
        let dirs_only = text.ends_with('/');
        if dirs_only {
            text.pop();
        }
        let text = text.trim_start_matches('/').to_lowercase();
        if text.is_empty() {
            return None;
        }
        Some(Pattern {
            anchored: text.contains('/'),
            dirs_only,
            text: text.chars().collect(),
        })
    }

    /// Does the pattern hit this entry or one of its parent directories?
    fn hits(&self, path: &str, is_dir: bool) -> bool {
        let lower = path.to_lowercase();
        let parts: Vec<&str> = lower.split('/').collect();
        for end in 1..=parts.len() {
            let dir_here = end < parts.len() || is_dir;
            if self.dirs_only && !dir_here {
                continue;
            }
            let hit = if self.anchored {
                wildcard(&self.text, &chars(&parts[..end].join("/")))
            } else {
                wildcard(&self.text, &chars(parts[end - 1]))
            };
            if hit {
                return true;
            }
        }
        false
    }
}

fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

/// `*` matches any run of characters (also across `/`), `?` exactly one.
fn wildcard(pattern: &[char], text: &[char]) -> bool {
    let (mut p, mut t) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            mark = t;
            p += 1;
        } else if let Some(s) = star {
            p = s + 1;
            mark += 1;
            t = mark;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    include: Vec<Pattern>,
    exclude: Vec<Pattern>,
}

impl Filter {
    /// `include` empty means "everything"; `exclude` always wins.
    pub fn new(include: &str, exclude: &str) -> Filter {
        let parse = |text: &str| {
            text.split([';', '\n'])
                .filter_map(Pattern::parse)
                .collect::<Vec<_>>()
        };
        Filter {
            include: parse(include),
            exclude: parse(exclude),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty()
    }

    /// Is a directory worth entering? Only excluded directories are skipped; the include
    /// list cannot rule a directory out, because a file deeper down may still match it.
    pub fn enters_dir(&self, path: &str) -> bool {
        !self.exclude.iter().any(|p| p.hits(path, true))
    }

    /// Does the filter let this entry take part in the comparison?
    pub fn allows(&self, path: &str, is_dir: bool) -> bool {
        if self.exclude.iter().any(|p| p.hits(path, is_dir)) {
            return false;
        }
        // Directories stay when the include list is set so that matching files below them
        // keep their place; they are dropped later if nothing underneath survived.
        is_dir || self.include.is_empty() || self.include.iter().any(|p| p.hits(path, is_dir))
    }

    pub fn has_include(&self) -> bool {
        !self.include.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_patterns_match_any_depth_and_are_case_insensitive() {
        let f = Filter::new("", "*.TMP; thumbs.db");
        assert!(!f.allows("a.tmp", false));
        assert!(!f.allows("deep/er/b.Tmp", false));
        assert!(!f.allows("x/Thumbs.db", false));
        assert!(f.allows("a.txt", false));
    }

    #[test]
    fn excluding_a_directory_excludes_everything_below_it() {
        let f = Filter::new("", "node_modules/\nbuild/*.o");
        assert!(!f.allows("web/node_modules", true));
        assert!(!f.allows("web/node_modules/x/y.js", false));
        assert!(!f.enters_dir("node_modules"));
        // A file merely called like the directory is not a directory.
        assert!(f.allows("node_modules", false));
        assert!(!f.allows("build/a.o", false));
        assert!(
            f.allows("other/build/a.o", false),
            "slash patterns are anchored"
        );
        assert!(f.allows("build/a.c", false));
    }

    #[test]
    fn include_list_limits_files_but_not_directories() {
        let f = Filter::new("*.rs;docs/", "");
        assert!(f.allows("src", true));
        assert!(f.allows("src/main.rs", false));
        assert!(!f.allows("src/main.o", false));
        assert!(
            f.allows("docs/readme.txt", false),
            "everything under an included dir"
        );
        assert!(f.has_include());
    }

    #[test]
    fn exclude_beats_include() {
        let f = Filter::new("*.rs", "gen_*");
        assert!(!f.allows("gen_a.rs", false));
        assert!(f.allows("a.rs", false));
    }

    #[test]
    fn wildcards() {
        let w = |p: &str, t: &str| wildcard(&chars(p), &chars(t));
        assert!(w("*", ""));
        assert!(w("a*c", "abbbc"));
        assert!(w("a?c", "abc"));
        assert!(!w("a?c", "ac"));
        assert!(w("*.t*t", "x.txt"));
        assert!(!w("*.txt", "x.txt2"));
    }

    #[test]
    fn empty_filter_allows_everything() {
        let f = Filter::new("  ;; ", "\n");
        assert!(f.is_empty());
        assert!(f.allows("anything/at/all", false));
    }
}
