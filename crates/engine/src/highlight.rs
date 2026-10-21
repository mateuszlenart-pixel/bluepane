//! Syntax colouring for the viewer: a small tokenizer, line by line, that marks keywords,
//! strings, comments and numbers. It is table driven (a few dozen words per language), not a
//! parser: it gets common code right and never fails, and an unknown file is simply left plain.
//! The only state between lines is whether a block comment is open.

use std::path::Path;

/// What a stretch of a row is, for the colour it gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    Keyword,
    String,
    Comment,
    Number,
    /// Markdown: a heading, `**strong**`, `*emphasis*`, `` `code` ``, a link, a quotation, a
    /// list bullet or a rule.
    Heading,
    Strong,
    Emphasis,
    Code,
    Link,
    Quote,
    Bullet,
    /// Hex editor: a byte that was changed, and the byte the cursor is on.
    Modified,
    Cursor,
}

/// A stretch of a row's text, in characters (`start..end`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub kind: SpanKind,
}

/// How a language writes its parts.
#[derive(Debug)]
pub struct Lang {
    pub name: &'static str,
    line_comments: &'static [&'static str],
    block: Option<(&'static str, &'static str)>,
    keywords: &'static [&'static str],
    quotes: &'static [char],
    /// Keywords ignore case (SQL).
    ignore_case: bool,
}

const C_LIKE_COMMENTS: &[&str] = &["//"];
const HASH: &[&str] = &["#"];
const QUOTES: &[char] = &['"', '\''];
const QUOTES_TICK: &[char] = &['"', '\'', '`'];

static RUST: Lang = Lang {
    name: "Rust",
    line_comments: C_LIKE_COMMENTS,
    block: Some(("/*", "*/")),
    keywords: &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
        "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
        "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait",
        "true", "type", "unsafe", "use", "where", "while",
    ],
    quotes: &['"'],
    ignore_case: false,
};

static PYTHON: Lang = Lang {
    name: "Python",
    line_comments: HASH,
    block: None,
    keywords: &[
        "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
        "elif", "else", "except", "False", "finally", "for", "from", "global", "if", "import",
        "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return", "True",
        "try", "while", "with", "yield",
    ],
    quotes: QUOTES,
    ignore_case: false,
};

static SCRIPT: Lang = Lang {
    name: "JavaScript / C / Java",
    line_comments: C_LIKE_COMMENTS,
    block: Some(("/*", "*/")),
    keywords: &[
        "abstract",
        "async",
        "await",
        "boolean",
        "break",
        "case",
        "catch",
        "char",
        "class",
        "const",
        "continue",
        "default",
        "delete",
        "do",
        "double",
        "else",
        "enum",
        "export",
        "extends",
        "false",
        "final",
        "finally",
        "float",
        "for",
        "function",
        "if",
        "implements",
        "import",
        "in",
        "instanceof",
        "int",
        "interface",
        "let",
        "long",
        "namespace",
        "new",
        "null",
        "package",
        "private",
        "protected",
        "public",
        "return",
        "short",
        "static",
        "struct",
        "super",
        "switch",
        "this",
        "throw",
        "throws",
        "true",
        "try",
        "typeof",
        "undefined",
        "using",
        "var",
        "void",
        "volatile",
        "while",
        "yield",
    ],
    quotes: QUOTES_TICK,
    ignore_case: false,
};

static GO: Lang = Lang {
    name: "Go",
    line_comments: C_LIKE_COMMENTS,
    block: Some(("/*", "*/")),
    keywords: &[
        "break",
        "case",
        "chan",
        "const",
        "continue",
        "default",
        "defer",
        "else",
        "fallthrough",
        "false",
        "for",
        "func",
        "go",
        "goto",
        "if",
        "import",
        "interface",
        "map",
        "nil",
        "package",
        "range",
        "return",
        "select",
        "struct",
        "switch",
        "true",
        "type",
        "var",
    ],
    quotes: QUOTES_TICK,
    ignore_case: false,
};

static JSON: Lang = Lang {
    name: "JSON",
    line_comments: &[],
    block: None,
    keywords: &["true", "false", "null"],
    quotes: &['"'],
    ignore_case: false,
};

static CONFIG: Lang = Lang {
    name: "TOML / INI / YAML",
    line_comments: &["#", ";"],
    block: None,
    keywords: &["true", "false", "null", "yes", "no", "on", "off"],
    quotes: QUOTES,
    ignore_case: false,
};

static SHELL: Lang = Lang {
    name: "Shell",
    line_comments: HASH,
    block: None,
    keywords: &[
        "case", "do", "done", "elif", "else", "esac", "export", "fi", "for", "function", "if",
        "in", "local", "return", "set", "then", "unset", "until", "while",
    ],
    quotes: QUOTES,
    ignore_case: false,
};

static SQL: Lang = Lang {
    name: "SQL",
    line_comments: &["--"],
    block: Some(("/*", "*/")),
    keywords: &[
        "add", "all", "alter", "and", "as", "asc", "between", "by", "case", "create", "delete",
        "desc", "distinct", "drop", "else", "end", "exists", "from", "group", "having", "in",
        "index", "inner", "insert", "into", "is", "join", "left", "like", "limit", "not", "null",
        "on", "or", "order", "outer", "primary", "right", "select", "set", "table", "then",
        "union", "unique", "update", "values", "view", "when", "where",
    ],
    quotes: &['\'', '"'],
    ignore_case: true,
};

static MARKUP: Lang = Lang {
    name: "HTML / XML",
    line_comments: &[],
    block: Some(("<!--", "-->")),
    keywords: &[],
    quotes: QUOTES,
    ignore_case: false,
};

static CSS: Lang = Lang {
    name: "CSS",
    line_comments: &[],
    block: Some(("/*", "*/")),
    keywords: &["important", "inherit", "initial", "none", "auto"],
    quotes: QUOTES,
    ignore_case: false,
};

impl Lang {
    /// Whether the language has `/* */`-style comments that run over lines.
    pub fn has_block_comments(&self) -> bool {
        self.block.is_some()
    }
}

/// The language of a file, by its extension (or a few well-known names).
pub fn lang_of(path: &Path) -> Option<&'static Lang> {
    let name = path.file_name()?.to_string_lossy().to_lowercase();
    if matches!(name.as_str(), "makefile" | "dockerfile" | "cargo.lock") {
        return Some(&CONFIG);
    }
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    Some(match ext.as_str() {
        "rs" => &RUST,
        "py" | "pyw" => &PYTHON,
        "js" | "mjs" | "ts" | "tsx" | "jsx" | "c" | "h" | "cpp" | "cc" | "hpp" | "cs" | "java"
        | "kt" | "swift" | "php" | "scala" | "dart" => &SCRIPT,
        "go" => &GO,
        "json" | "jsonc" | "geojson" => &JSON,
        "toml" | "ini" | "cfg" | "conf" | "yaml" | "yml" | "env" | "properties" | "bp" => &CONFIG,
        "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" | "cmd" => &SHELL,
        "sql" => &SQL,
        "html" | "htm" | "xml" | "svg" | "xaml" | "xhtml" | "plist" => &MARKUP,
        "css" | "scss" | "less" => &CSS,
        _ => return None,
    })
}

fn starts_with_at(chars: &[char], at: usize, pattern: &str) -> bool {
    pattern
        .chars()
        .enumerate()
        .all(|(k, p)| chars.get(at + k) == Some(&p))
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Colours one line. `in_block` is whether a block comment is open at its start; the result
/// says whether one is open at its end.
pub fn highlight_line(lang: &Lang, line: &str, in_block: bool) -> (Vec<Span>, bool) {
    let chars: Vec<char> = line.chars().collect();
    let mut spans: Vec<Span> = Vec::new();
    let mut i = 0;
    // A language without block comments never has one open.
    let mut in_block = in_block && lang.block.is_some();
    let n = chars.len();
    let find_end = |from: usize, close: &str| -> Option<usize> {
        (from..n).find(|&at| starts_with_at(&chars, at, close))
    };
    while i < n {
        if in_block {
            let (_, close) = lang.block.unwrap_or(("", ""));
            match find_end(i, close) {
                Some(at) => {
                    let end = at + close.chars().count();
                    spans.push(Span {
                        start: i,
                        end,
                        kind: SpanKind::Comment,
                    });
                    i = end;
                    in_block = false;
                }
                None => {
                    spans.push(Span {
                        start: i,
                        end: n,
                        kind: SpanKind::Comment,
                    });
                    i = n;
                }
            }
            continue;
        }
        let c = chars[i];
        if lang
            .line_comments
            .iter()
            .any(|p| starts_with_at(&chars, i, p))
        {
            spans.push(Span {
                start: i,
                end: n,
                kind: SpanKind::Comment,
            });
            break;
        }
        if let Some((open, close)) = lang.block {
            if starts_with_at(&chars, i, open) {
                let after = i + open.chars().count();
                match find_end(after, close) {
                    Some(at) => {
                        let end = at + close.chars().count();
                        spans.push(Span {
                            start: i,
                            end,
                            kind: SpanKind::Comment,
                        });
                        i = end;
                    }
                    None => {
                        spans.push(Span {
                            start: i,
                            end: n,
                            kind: SpanKind::Comment,
                        });
                        in_block = true;
                        i = n;
                    }
                }
                continue;
            }
        }
        if lang.quotes.contains(&c) {
            let mut j = i + 1;
            while j < n && chars[j] != c {
                // A backslash takes the next character with it.
                j += if chars[j] == '\\' { 2 } else { 1 };
            }
            let end = (j + 1).min(n);
            spans.push(Span {
                start: i,
                end,
                kind: SpanKind::String,
            });
            i = end;
            continue;
        }
        if c.is_ascii_digit() && (i == 0 || !is_word(chars[i - 1])) {
            let mut j = i + 1;
            while j < n && (chars[j].is_ascii_alphanumeric() || chars[j] == '_' || chars[j] == '.')
            {
                j += 1;
            }
            spans.push(Span {
                start: i,
                end: j,
                kind: SpanKind::Number,
            });
            i = j;
            continue;
        }
        if lang.name == MARKUP.name && c == '<' {
            let end = (i..n).find(|&at| chars[at] == '>').map_or(n, |at| at + 1);
            spans.push(Span {
                start: i,
                end,
                kind: SpanKind::Keyword,
            });
            i = end;
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let mut j = i + 1;
            while j < n && is_word(chars[j]) {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();
            let hit = if lang.ignore_case {
                lang.keywords.iter().any(|k| k.eq_ignore_ascii_case(&word))
            } else {
                lang.keywords.contains(&word.as_str())
            };
            if hit {
                spans.push(Span {
                    start: i,
                    end: j,
                    kind: SpanKind::Keyword,
                });
            }
            i = j;
            continue;
        }
        i += 1;
    }
    (spans, in_block)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(lang: &Lang, line: &str) -> Vec<(String, SpanKind)> {
        let chars: Vec<char> = line.chars().collect();
        highlight_line(lang, line, false)
            .0
            .into_iter()
            .map(|s| (chars[s.start..s.end].iter().collect(), s.kind))
            .collect()
    }

    #[test]
    fn languages_are_chosen_by_extension_and_unknown_files_stay_plain() {
        let name = |p: &str| lang_of(Path::new(p)).map(|l| l.name);
        assert_eq!(name("src/main.RS"), Some("Rust"));
        assert_eq!(name("a/b.py"), Some("Python"));
        assert_eq!(name("x.tsx"), Some("JavaScript / C / Java"));
        assert_eq!(name("Cargo.toml"), Some("TOML / INI / YAML"));
        assert_eq!(name("Makefile"), Some("TOML / INI / YAML"));
        assert_eq!(name("notes.txt"), None);
        assert_eq!(name("noextension"), None);
    }

    #[test]
    fn rust_keywords_strings_numbers_and_comments_are_marked() {
        let got = kinds(&RUST, "let x = 42; // zażółć \"gęślą\"");
        assert_eq!(
            got,
            [
                ("let".to_string(), SpanKind::Keyword),
                ("42".to_string(), SpanKind::Number),
                ("// zażółć \"gęślą\"".to_string(), SpanKind::Comment),
            ]
        );
        let got = kinds(&RUST, "fn nazwa() -> &'static str { \"a\\\"b\" }");
        assert!(got.contains(&("fn".to_string(), SpanKind::Keyword)));
        assert!(
            got.contains(&("\"a\\\"b\"".to_string(), SpanKind::String)),
            "an escaped quote stays inside the string: {got:?}"
        );
        // Words that merely contain a keyword are not keywords; digits inside names are not numbers.
        assert!(kinds(&RUST, "letter x1 forest").is_empty());
    }

    #[test]
    fn a_block_comment_carries_over_lines_and_ends_where_it_closes() {
        let (spans, open) = highlight_line(&RUST, "a /* start", false);
        assert!(open);
        assert_eq!(spans.last().unwrap().kind, SpanKind::Comment);
        let (spans, open) = highlight_line(&RUST, "still inside", true);
        assert!(open);
        assert_eq!((spans[0].start, spans[0].end), (0, 12));
        let (spans, open) = highlight_line(&RUST, "end */ let", true);
        assert!(!open);
        assert_eq!(
            spans[0],
            Span {
                start: 0,
                end: 6,
                kind: SpanKind::Comment
            }
        );
        assert_eq!(spans[1].kind, SpanKind::Keyword);
        let (_, open) = highlight_line(&RUST, "/* one */ code", false);
        assert!(!open, "closed on the same line");
    }

    #[test]
    fn other_languages_use_their_own_comments_and_case_rules() {
        assert_eq!(
            kinds(&PYTHON, "def f(): # komentarz")[0],
            ("def".to_string(), SpanKind::Keyword)
        );
        assert_eq!(
            kinds(&SQL, "SELECT * FROM t -- wszystko").last().unwrap().1,
            SpanKind::Comment
        );
        assert!(
            kinds(&SQL, "Select a From b").len() >= 2,
            "case does not matter in SQL"
        );
        assert_eq!(
            kinds(&JSON, "{\"klucz\": [1, 2.5, true, null]}"),
            [
                ("\"klucz\"".to_string(), SpanKind::String),
                ("1".to_string(), SpanKind::Number),
                ("2.5".to_string(), SpanKind::Number),
                ("true".to_string(), SpanKind::Keyword),
                ("null".to_string(), SpanKind::Keyword),
            ]
        );
        assert_eq!(
            kinds(&MARKUP, "<p class=\"a\">tekst</p>")
                .iter()
                .filter(|(_, k)| *k == SpanKind::Keyword)
                .count(),
            2
        );
        assert_eq!(
            kinds(&CONFIG, "klucz = 1 ; uwaga").last().unwrap().1,
            SpanKind::Comment
        );
    }

    #[test]
    fn odd_input_never_panics_and_spans_stay_inside_the_line() {
        for line in [
            "",
            "\"",
            "'\\",
            "/*",
            "*/",
            "0x",
            "ą\"ć",
            "<",
            "\\\\\\",
            "é\u{301}\"x",
        ] {
            for lang in [
                &RUST, &PYTHON, &SCRIPT, &JSON, &SQL, &MARKUP, &CSS, &SHELL, &GO,
            ] {
                for open in [false, true] {
                    let n = line.chars().count();
                    let (spans, _) = highlight_line(lang, line, open);
                    assert!(
                        spans.iter().all(|s| s.start < s.end && s.end <= n),
                        "{line:?} {spans:?}"
                    );
                }
            }
        }
    }
}
