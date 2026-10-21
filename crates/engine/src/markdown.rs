//! Markdown for the viewer: the text of a `.md` file turned into rows of plain characters with
//! colour spans (headings, **strong**, *emphasis*, `code`, links, quotations, list bullets,
//! rules), wrapped to the width of the window. It is a reader's view, not a full CommonMark
//! implementation: headings, paragraphs, bullet and numbered lists, quotations, fenced code,
//! rules, inline code, emphasis, links and images; tables and HTML are shown as written.

use crate::highlight::{Span, SpanKind};

/// Whether a path is a Markdown file by its extension.
pub fn is_markdown(path: &std::path::Path) -> bool {
    path.extension().is_some_and(|e| {
        let e = e.to_string_lossy().to_lowercase();
        matches!(e.as_str(), "md" | "markdown" | "mdown" | "mkd")
    })
}

/// A rendered line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub spans: Vec<Span>,
}

/// Characters with the kind they are drawn in.
type Styled = Vec<(char, Option<SpanKind>)>;

fn plain(text: &str) -> Styled {
    text.chars().map(|c| (c, None)).collect()
}

fn with_kind(text: &str, kind: SpanKind) -> Styled {
    text.chars().map(|c| (c, Some(kind))).collect()
}

/// Inline formatting of one paragraph line.
fn inline(text: &str) -> Styled {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Styled = Vec::new();
    let mut i = 0;
    let n = chars.len();
    let find = |from: usize, pattern: &[char]| -> Option<usize> {
        (from..n.saturating_sub(pattern.len() - 1)).find(|&at| chars[at..].starts_with(pattern))
    };
    while i < n {
        let c = chars[i];
        // An escaped character is shown as itself.
        if c == '\\' && i + 1 < n && chars[i + 1].is_ascii_punctuation() {
            out.push((chars[i + 1], None));
            i += 2;
            continue;
        }
        if c == '`' {
            if let Some(end) = find(i + 1, &['`']) {
                let code: String = chars[i + 1..end].iter().collect();
                out.extend(with_kind(&code, SpanKind::Code));
                i = end + 1;
                continue;
            }
        }
        if (c == '*' || c == '_') && i + 1 < n && chars[i + 1] == c {
            let pattern = [c, c];
            if let Some(end) = find(i + 2, &pattern).filter(|&e| e > i + 2) {
                let inner: String = chars[i + 2..end].iter().collect();
                out.extend(inner_kind(&inner, SpanKind::Strong));
                i = end + 2;
                continue;
            }
        }
        // `*em*` and `_em_`; an underscore inside a word (snake_case) is left alone.
        let word_inside = c == '_' && i > 0 && chars[i - 1].is_alphanumeric();
        if (c == '*' || c == '_') && !word_inside && i + 1 < n && !chars[i + 1].is_whitespace() {
            if let Some(end) =
                find(i + 1, &[c]).filter(|&e| e > i + 1 && !chars[e - 1].is_whitespace())
            {
                let inner: String = chars[i + 1..end].iter().collect();
                out.extend(inner_kind(&inner, SpanKind::Emphasis));
                i = end + 1;
                continue;
            }
        }
        let image = c == '!' && i + 1 < n && chars[i + 1] == '[';
        if c == '[' || image {
            let open = if image { i + 1 } else { i };
            if let Some(close) = find(open + 1, &[']']) {
                if chars.get(close + 1) == Some(&'(') {
                    if let Some(end) = find(close + 2, &[')']) {
                        let label: String = chars[open + 1..close].iter().collect();
                        let url: String = chars[close + 2..end].iter().collect();
                        let shown = if image {
                            format!("[obraz: {label}]")
                        } else if label.is_empty() || label == url {
                            url
                        } else {
                            format!("{label} ({url})")
                        };
                        out.extend(with_kind(&shown, SpanKind::Link));
                        i = end + 1;
                        continue;
                    }
                }
            }
        }
        out.push((c, None));
        i += 1;
    }
    out
}

/// The inside of a strong or emphasised stretch (it may hold code or a link of its own).
fn inner_kind(text: &str, kind: SpanKind) -> Styled {
    inline(text)
        .into_iter()
        .map(|(c, k)| (c, k.or(Some(kind))))
        .collect()
}

/// Cuts a styled paragraph into lines of at most `width` characters, at spaces when it can.
fn wrap(text: Styled, width: usize, indent: &str, hanging: &str) -> Vec<Styled> {
    let width = width.max(10);
    let mut lines: Vec<Styled> = Vec::new();
    let mut current: Styled = plain(indent);
    let mut first = true;
    let mut word: Styled = Vec::new();
    let indent_len = |first: bool| {
        if first {
            indent.chars().count()
        } else {
            hanging.chars().count()
        }
    };
    let flush_word =
        |current: &mut Styled, word: &mut Styled, first: &mut bool, lines: &mut Vec<Styled>| {
            if word.is_empty() {
                return;
            }
            let used = current.len();
            let starts = indent_len(*first);
            let needs_space = used > starts;
            if used + usize::from(needs_space) + word.len() > width && used > starts {
                lines.push(std::mem::replace(current, plain(hanging)));
                *first = false;
            } else if needs_space {
                // A space between two words of the same style belongs to that style.
                let kind = current
                    .last()
                    .and_then(|(_, k)| *k)
                    .filter(|k| word.first().is_some_and(|(_, wk)| *wk == Some(*k)));
                current.push((' ', kind));
            }
            current.append(word);
        };
    for item in text {
        if item.0 == ' ' {
            flush_word(&mut current, &mut word, &mut first, &mut lines);
        } else {
            word.push(item);
            // A word longer than the line is cut where it hits the edge.
            if word.len() >= width {
                flush_word(&mut current, &mut word, &mut first, &mut lines);
            }
        }
    }
    flush_word(&mut current, &mut word, &mut first, &mut lines);
    lines.push(current);
    lines
}

/// Runs of the same kind become spans.
fn to_line(styled: Styled) -> Line {
    let mut spans: Vec<Span> = Vec::new();
    let mut text = String::new();
    for (i, (c, kind)) in styled.iter().enumerate() {
        text.push(*c);
        if let Some(kind) = kind {
            match spans.last_mut() {
                Some(last) if last.end == i && last.kind == *kind => last.end = i + 1,
                _ => spans.push(Span {
                    start: i,
                    end: i + 1,
                    kind: *kind,
                }),
            }
        }
    }
    Line { text, spans }
}

fn heading_level(line: &str) -> Option<(usize, &str)> {
    let hashes = line.chars().take_while(|&c| c == '#').count();
    let rest = line.get(hashes..)?;
    ((1..=6).contains(&hashes) && (rest.is_empty() || rest.starts_with(' ')))
        .then(|| (hashes, rest.trim().trim_end_matches('#').trim_end()))
}

/// `- item`, `* item`, `+ item`, `1. item`: the marker, the indentation and the text.
fn list_item(line: &str) -> Option<(usize, String, &str)> {
    let indent = line.chars().take_while(|c| *c == ' ').count();
    let rest = &line[indent..];
    if let Some(text) = ["- ", "* ", "+ "].iter().find_map(|m| rest.strip_prefix(m)) {
        return Some((indent, "•".to_string(), text));
    }
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if (1..=9).contains(&digits) {
        let after = &rest[digits..];
        if let Some(text) = after
            .strip_prefix(". ")
            .or_else(|| after.strip_prefix(") "))
        {
            return Some((indent, format!("{}.", &rest[..digits]), text));
        }
    }
    None
}

fn is_rule(line: &str) -> bool {
    let t: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    t.len() >= 3
        && ["-", "*", "_"]
            .iter()
            .any(|m| t.chars().all(|c| c.to_string() == *m))
}

/// The rows of a Markdown text for a window `width` characters wide.
pub fn render(text: &str, width: usize) -> Vec<Line> {
    let width = width.max(20);
    let mut out: Vec<Line> = Vec::new();
    let mut in_code = false;
    let mut blank_before = true;
    let mut paragraph: Vec<&str> = Vec::new();
    let flush = |paragraph: &mut Vec<&str>, out: &mut Vec<Line>| {
        if paragraph.is_empty() {
            return;
        }
        let joined = paragraph.join(" ");
        paragraph.clear();
        out.extend(
            wrap(inline(&joined), width, "", "")
                .into_iter()
                .map(to_line),
        );
    };
    for raw in text.lines() {
        let line = raw.trim_end();
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            flush(&mut paragraph, &mut out);
            in_code = !in_code;
            if in_code && !blank_before {
                out.push(Line {
                    text: String::new(),
                    spans: Vec::new(),
                });
            }
            blank_before = false;
            continue;
        }
        if in_code {
            let shown = format!("    {}", line.replace('\t', "    "));
            let chars = shown.chars().count();
            out.push(Line {
                text: shown,
                spans: vec![Span {
                    start: 0,
                    end: chars.max(1),
                    kind: SpanKind::Code,
                }],
            });
            continue;
        }
        if line.trim().is_empty() {
            flush(&mut paragraph, &mut out);
            if !blank_before {
                out.push(Line {
                    text: String::new(),
                    spans: Vec::new(),
                });
            }
            blank_before = true;
            continue;
        }
        if let Some((level, title)) = heading_level(line) {
            flush(&mut paragraph, &mut out);
            if !blank_before {
                out.push(Line {
                    text: String::new(),
                    spans: Vec::new(),
                });
            }
            let styled: Styled = inline(title)
                .into_iter()
                .map(|(c, _)| (c, Some(SpanKind::Heading)))
                .collect();
            for l in wrap(styled, width, "", "") {
                out.push(to_line(l));
            }
            if level <= 2 {
                let bar = if level == 1 { '═' } else { '─' };
                let len = out.last().map_or(0, |l| l.text.chars().count()).min(width);
                out.push(Line {
                    text: bar.to_string().repeat(len),
                    spans: vec![Span {
                        start: 0,
                        end: len.max(1),
                        kind: SpanKind::Heading,
                    }],
                });
            }
            blank_before = false;
            continue;
        }
        if is_rule(line) && paragraph.is_empty() {
            let len = width.min(60);
            out.push(Line {
                text: "─".repeat(len),
                spans: vec![Span {
                    start: 0,
                    end: len,
                    kind: SpanKind::Quote,
                }],
            });
            blank_before = false;
            continue;
        }
        if let Some(quoted) = line.trim_start().strip_prefix('>') {
            flush(&mut paragraph, &mut out);
            let styled: Styled = inline(quoted.trim_start())
                .into_iter()
                .map(|(c, k)| (c, k.or(Some(SpanKind::Quote))))
                .collect();
            for l in wrap(styled, width, "│ ", "│ ") {
                let mut line = to_line(l);
                // The bar is part of the quotation colour.
                line.spans.insert(
                    0,
                    Span {
                        start: 0,
                        end: 1,
                        kind: SpanKind::Quote,
                    },
                );
                out.push(line);
            }
            blank_before = false;
            continue;
        }
        if let Some((indent, marker, item)) = list_item(line) {
            flush(&mut paragraph, &mut out);
            let lead = format!("{}{marker} ", " ".repeat(indent));
            let hanging = " ".repeat(lead.chars().count());
            let marker_end = lead.chars().count() - 1;
            for (n, l) in wrap(inline(item), width, &lead, &hanging)
                .into_iter()
                .enumerate()
            {
                let mut line = to_line(l);
                if n == 0 {
                    line.spans.insert(
                        0,
                        Span {
                            start: indent,
                            end: marker_end,
                            kind: SpanKind::Bullet,
                        },
                    );
                }
                out.push(line);
            }
            blank_before = false;
            continue;
        }
        // A table row or an indented block is shown as written.
        if line.trim_start().starts_with('|') || line.starts_with("    ") {
            flush(&mut paragraph, &mut out);
            out.push(Line {
                text: line.to_string(),
                spans: Vec::new(),
            });
            blank_before = false;
            continue;
        }
        paragraph.push(line.trim());
        blank_before = false;
    }
    flush(&mut paragraph, &mut out);
    while out.last().is_some_and(|l| l.text.is_empty()) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lines: &[Line]) -> Vec<&str> {
        lines.iter().map(|l| l.text.as_str()).collect()
    }

    fn kind_at(line: &Line, ch: usize) -> Option<SpanKind> {
        line.spans
            .iter()
            .find(|s| s.start <= ch && ch < s.end)
            .map(|s| s.kind)
    }

    #[test]
    fn markdown_files_are_recognised_by_extension() {
        for name in ["a.md", "B.MARKDOWN", "x/y.mdown"] {
            assert!(is_markdown(std::path::Path::new(name)), "{name}");
        }
        assert!(!is_markdown(std::path::Path::new("a.txt")));
        assert!(!is_markdown(std::path::Path::new("md")));
    }

    #[test]
    fn headings_get_a_colour_and_the_first_two_levels_an_underline() {
        let lines = render("# Zażółć gęślą\n\ntekst\n\n### Mały\n", 40);
        assert_eq!(
            texts(&lines),
            ["Zażółć gęślą", "════════════", "", "tekst", "", "Mały"]
        );
        assert_eq!(kind_at(&lines[0], 0), Some(SpanKind::Heading));
        assert_eq!(kind_at(&lines[1], 3), Some(SpanKind::Heading));
        assert_eq!(kind_at(&lines[3], 0), None);
        let h2 = render("## Drugi\n", 40);
        assert_eq!(texts(&h2), ["Drugi", "─────"]);
        // `#hashtag` without a space is a paragraph.
        assert_eq!(texts(&render("#hashtag", 40)), ["#hashtag"]);
    }

    #[test]
    fn inline_formatting_marks_strong_emphasis_code_and_links() {
        let lines = render(
            "To jest **mocne**, *kursywa*, `kod` i [link](http://a.pl) oraz snake_case_word.",
            80,
        );
        assert_eq!(
            lines[0].text,
            "To jest mocne, kursywa, kod i link (http://a.pl) oraz snake_case_word."
        );
        let at = |word: &str| {
            lines[0]
                .text
                .find(word)
                .map(|b| lines[0].text[..b].chars().count())
                .unwrap()
        };
        assert_eq!(kind_at(&lines[0], at("mocne")), Some(SpanKind::Strong));
        assert_eq!(kind_at(&lines[0], at("kursywa")), Some(SpanKind::Emphasis));
        assert_eq!(kind_at(&lines[0], at("kod")), Some(SpanKind::Code));
        assert_eq!(kind_at(&lines[0], at("link")), Some(SpanKind::Link));
        assert_eq!(
            kind_at(&lines[0], at("snake")),
            None,
            "underscores inside a word stay"
        );
        assert_eq!(
            texts(&render("a \\*nie kursywa\\* b", 40)),
            ["a *nie kursywa* b"]
        );
        assert_eq!(
            texts(&render("![zdjęcie](a.png)", 40)),
            ["[obraz: zdjęcie]"]
        );
        assert_eq!(
            texts(&render("**niezamknięte i *tak", 40)),
            ["**niezamknięte i *tak"]
        );
    }

    #[test]
    fn lists_quotes_rules_and_code_blocks() {
        let text = "- jeden\n- dwa **grube**\n  - zagnieżdżony\n\n1. pierwszy\n2. drugi\n\n> cytat o\n> dwóch liniach\n\n---\n\n```rust\nlet x = 1;\n```\n";
        let lines = render(text, 40);
        assert_eq!(
            texts(&lines),
            [
                "• jeden",
                "• dwa grube",
                "  • zagnieżdżony",
                "",
                "1. pierwszy",
                "2. drugi",
                "",
                "│ cytat o",
                "│ dwóch liniach",
                "",
                "────────────────────────────────────────",
                "",
                "    let x = 1;",
            ]
        );
        assert_eq!(kind_at(&lines[0], 0), Some(SpanKind::Bullet));
        assert_eq!(kind_at(&lines[0], 2), None);
        assert_eq!(kind_at(&lines[1], 6), Some(SpanKind::Strong));
        assert_eq!(kind_at(&lines[4], 0), Some(SpanKind::Bullet));
        assert_eq!(kind_at(&lines[7], 0), Some(SpanKind::Quote));
        assert_eq!(kind_at(&lines[7], 5), Some(SpanKind::Quote));
        assert_eq!(kind_at(&lines[12], 6), Some(SpanKind::Code));
    }

    #[test]
    fn paragraphs_wrap_at_the_width_and_keep_their_colours() {
        let text =
            "Ala ma kota **i bardzo długiego psa o imieniu Burek**, który lubi zażółć gęślą jaźń.";
        let lines = render(text, 30);
        assert!(lines.len() >= 3, "{:?}", texts(&lines));
        assert!(
            lines.iter().all(|l| l.text.chars().count() <= 30),
            "{:?}",
            texts(&lines)
        );
        let joined = texts(&lines).join(" ");
        assert_eq!(
            joined,
            "Ala ma kota i bardzo długiego psa o imieniu Burek, który lubi zażółć gęślą jaźń."
        );
        // Every letter of the strong phrase is strong (the spaces where a line broke are gone).
        let strong: usize = lines
            .iter()
            .map(|l| {
                let chars: Vec<char> = l.text.chars().collect();
                l.spans
                    .iter()
                    .filter(|s| s.kind == SpanKind::Strong)
                    .map(|s| {
                        chars[s.start..s.end]
                            .iter()
                            .filter(|c| !c.is_whitespace())
                            .count()
                    })
                    .sum::<usize>()
            })
            .sum();
        let phrase = "i bardzo długiego psa o imieniu Burek";
        assert_eq!(
            strong,
            phrase.chars().filter(|c| !c.is_whitespace()).count()
        );
        // A list item wraps with a hanging indent.
        let item = render(
            "- bardzo długi punkt listy który nie zmieści się w jednej linii",
            24,
        );
        assert!(item[1].text.starts_with("  "), "{:?}", texts(&item));
        // A word longer than the width is cut.
        let long = render(&"x".repeat(50), 20);
        assert!(long.iter().all(|l| l.text.chars().count() <= 20) && long.len() >= 3);
    }

    #[test]
    fn tables_stay_as_written_and_odd_input_does_not_panic() {
        let lines = render("| a | b |\n|---|---|\n| 1 | 2 |\n", 40);
        assert_eq!(texts(&lines), ["| a | b |", "|---|---|", "| 1 | 2 |"]);
        assert!(render("", 40).is_empty());
        for odd in [
            "[", "[]()", "![", "**", "* ", "> ", "#", "`", "\\", "1.", "- ", "```",
        ] {
            for width in [1, 10, 80] {
                let _ = render(odd, width);
            }
        }
        // The width never makes the spans point past the text.
        for line in render("**a** [b](c) `d` ęąść", 11) {
            let n = line.text.chars().count();
            assert!(
                line.spans.iter().all(|s| s.start < s.end && s.end <= n),
                "{line:?}"
            );
        }
    }
}
