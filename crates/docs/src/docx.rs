//! Word documents: `word/document.xml` as plain text.

use std::path::Path;

use crate::xml::{attr, events, Event};
use crate::{member, open_zip, DocError};

pub(crate) fn extract(path: &Path) -> Result<String, DocError> {
    let mut zip = open_zip(path)?;
    let xml = member(&mut zip, "word/document.xml")?.ok_or_else(|| {
        DocError::Corrupt("brak word/document.xml (to nie jest plik Word)".into())
    })?;
    if member(&mut zip, "EncryptedPackage")?.is_some() {
        return Err(DocError::Encrypted);
    }
    Ok(text_of(&xml))
}

/// A heading style such as `Heading2` or `Nagwek1`: how deep the heading is.
fn heading_level(style: &str) -> Option<usize> {
    let lower = style.to_lowercase();
    let is_heading = ["heading", "nagwek", "nagłówek", "title", "tytu"]
        .iter()
        .any(|word| lower.starts_with(word));
    if !is_heading {
        return None;
    }
    let digits: String = lower.chars().filter(char::is_ascii_digit).collect();
    Some(digits.parse().unwrap_or(1).clamp(1, 6))
}

pub(crate) fn text_of(xml: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut paragraph = String::new();
    let mut in_text = false;
    let mut heading: Option<usize> = None;
    let mut bullet = false;
    // Tables: the cells of the row being read, and the text of the cell.
    let mut table_depth = 0;
    let mut row: Vec<String> = Vec::new();
    let mut cell: Vec<String> = Vec::new();

    for event in events(xml) {
        match event {
            Event::Start { name, attrs } => match name.as_str() {
                "w:p" => {
                    paragraph.clear();
                    heading = None;
                    bullet = false;
                }
                "w:pStyle" => heading = attr(&attrs, "w:val").and_then(heading_level),
                "w:numPr" => bullet = true,
                "w:t" => in_text = true,
                "w:tab" => paragraph.push('\t'),
                "w:br" | "w:cr" => paragraph.push('\n'),
                "w:tbl" => table_depth += 1,
                "w:tr" => row.clear(),
                "w:tc" => cell.clear(),
                _ => {}
            },
            Event::End(name) => match name.as_str() {
                "w:t" => in_text = false,
                "w:p" => {
                    let mut line = paragraph.trim_end().to_string();
                    if !line.is_empty() {
                        if let Some(level) = heading {
                            line = format!("{} {line}", "#".repeat(level));
                        } else if bullet {
                            line = format!("• {line}");
                        }
                    }
                    if table_depth > 0 {
                        if !line.is_empty() {
                            cell.push(line);
                        }
                    } else if !(line.is_empty() && out.last().is_none_or(String::is_empty)) {
                        // One blank line between blocks, never several.
                        out.push(line);
                    }
                }
                "w:tc" => row.push(cell.join(" ")),
                "w:tr" => out.push(row.join(" | ")),
                "w:tbl" => {
                    table_depth -= 1;
                    out.push(String::new());
                }
                _ => {}
            },
            Event::Text(text) => {
                if in_text {
                    paragraph.push_str(&text);
                }
            }
        }
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::zip_of;

    const BODY: &str = r#"<?xml version="1.0"?>
<w:document xmlns:w="x"><w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Title &amp; more</w:t></w:r></w:p>
<w:p><w:r><w:t>First </w:t></w:r><w:r><w:t xml:space="preserve">paragraph</w:t></w:r><w:r><w:tab/><w:t>tabbed</w:t></w:r></w:p>
<w:p/>
<w:p/>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/></w:numPr></w:pPr><w:r><w:t>item one</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="Nagwek2"/></w:pPr><w:r><w:t>Podtytuł</w:t></w:r></w:p>
<w:tbl>
<w:tr><w:tc><w:p><w:r><w:t>A1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B1</w:t></w:r></w:p><w:p><w:r><w:t>B1 second</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:p><w:r><w:t>A2</w:t></w:r></w:p></w:tc><w:tc><w:p/></w:tc></w:tr>
</w:tbl>
<w:p><w:r><w:t>after</w:t><w:br/><w:t>line break</w:t></w:r></w:p>
</w:body></w:document>"#;

    #[test]
    fn paragraphs_headings_lists_and_tables_come_out_as_text() {
        let text = text_of(BODY);
        assert_eq!(
            text,
            "# Title & more\nFirst paragraph\ttabbed\n\n• item one\n## Podtytuł\nA1 | B1 B1 second\nA2 | \n\nafter\nline break\n"
        );
    }

    #[test]
    fn a_docx_file_is_read_through_its_zip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = zip_of(tmp.path(), "a.docx", &[("word/document.xml", BODY)]);
        let text = crate::extract(&path).unwrap();
        assert_eq!(text.kind, crate::Kind::Docx);
        assert!(text.text.starts_with("# Title & more"));
        assert!(!text.truncated);
    }

    #[test]
    fn a_zip_that_is_not_a_word_file_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let path = zip_of(tmp.path(), "a.docx", &[("other.xml", "<a/>")]);
        let err = crate::extract(&path).unwrap_err();
        assert!(err.to_string().contains("word/document.xml"), "{err}");
    }

    #[test]
    fn password_protected_files_are_recognised() {
        let tmp = tempfile::tempdir().unwrap();
        let path = zip_of(
            tmp.path(),
            "a.docx",
            &[("word/document.xml", "<a/>"), ("EncryptedPackage", "x")],
        );
        assert!(matches!(
            crate::extract(&path),
            Err(crate::DocError::Encrypted)
        ));
    }
}
