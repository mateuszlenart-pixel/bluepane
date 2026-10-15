//! OpenDocument text and spreadsheets: `content.xml` as plain text.

use std::path::Path;

use crate::xml::{attr, events, Event};
use crate::{member, open_zip, DocError};

pub(crate) fn extract(path: &Path) -> Result<String, DocError> {
    let mut zip = open_zip(path)?;
    let xml = member(&mut zip, "content.xml")?.ok_or_else(|| {
        DocError::Corrupt("brak content.xml (to nie jest plik OpenDocument)".into())
    })?;
    Ok(text_of(&xml))
}

/// Rows of spreadsheets can be repeated millions of times; only this many are read.
const MAX_ROWS: usize = 20_000;

pub(crate) fn text_of(xml: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut paragraph = String::new();
    let mut in_paragraph = false;
    let mut heading: Option<usize> = None;
    let mut list_depth = 0;
    let mut table_depth = 0;
    let mut row: Vec<String> = Vec::new();
    let mut cell: Vec<String> = Vec::new();
    let mut rows = 0;
    let mut slides = 0;

    for event in events(xml) {
        match event {
            Event::Start { name, attrs } => match name.as_str() {
                "text:p" | "text:h" => {
                    in_paragraph = true;
                    paragraph.clear();
                    heading = (name == "text:h").then(|| {
                        attr(&attrs, "text:outline-level")
                            .and_then(|l| l.parse().ok())
                            .unwrap_or(1)
                    });
                }
                "draw:page" => {
                    slides += 1;
                    out.push(format!("──── slajd {slides} ────"));
                }
                "text:s" => {
                    let count: usize = attr(&attrs, "text:c")
                        .and_then(|c| c.parse().ok())
                        .unwrap_or(1);
                    paragraph.push_str(&" ".repeat(count.min(200)));
                }
                "text:tab" => paragraph.push('\t'),
                "text:line-break" => paragraph.push('\n'),
                "text:list-item" => list_depth += 1,
                "table:table" => {
                    table_depth += 1;
                    if let Some(sheet) = attr(&attrs, "table:name") {
                        out.push(format!("## {sheet}"));
                    }
                }
                "table:table-row" => row.clear(),
                "table:table-cell" => cell.clear(),
                _ => {}
            },
            Event::End(name) => match name.as_str() {
                "text:p" | "text:h" => {
                    in_paragraph = false;
                    let mut line = paragraph.trim_end().to_string();
                    if !line.is_empty() {
                        if let Some(level) = heading {
                            line = format!("{} {line}", "#".repeat(level.clamp(1, 6)));
                        } else if list_depth > 0 {
                            line = format!("• {line}");
                        }
                    }
                    if table_depth > 0 {
                        if !line.is_empty() {
                            cell.push(line);
                        }
                    } else if !(line.is_empty() && out.last().is_none_or(String::is_empty)) {
                        out.push(line);
                    }
                }
                "text:list-item" => list_depth -= 1,
                "table:table-cell" | "table:covered-table-cell" => row.push(cell.join(" ")),
                "table:table-row" => {
                    while row.last().is_some_and(String::is_empty) {
                        row.pop();
                    }
                    if !row.is_empty() && rows < MAX_ROWS {
                        out.push(row.join(" | "));
                        rows += 1;
                    }
                }
                "table:table" => {
                    table_depth -= 1;
                    out.push(String::new());
                }
                _ => {}
            },
            Event::Text(text) => {
                if in_paragraph {
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

    const TEXT: &str = r#"<office:document-content><office:body><office:text>
<text:h text:outline-level="2">Chapter</text:h>
<text:p>Hello<text:s text:c="2"/>there<text:tab/>x<text:line-break/>next &amp; last</text:p>
<text:list><text:list-item><text:p>one</text:p></text:list-item><text:list-item><text:p>two</text:p></text:list-item></text:list>
<text:p/>
<text:p>end</text:p>
</office:text></office:body></office:document-content>"#;

    const SHEET: &str = r#"<office:document-content><office:body><office:spreadsheet>
<table:table table:name="Budget">
<table:table-row><table:table-cell><text:p>Item</text:p></table:table-cell><table:table-cell office:value-type="float" office:value="5"><text:p>5</text:p></table:table-cell><table:table-cell/></table:table-row>
<table:table-row table:number-rows-repeated="1000000"><table:table-cell/><table:table-cell/></table:table-row>
</table:table>
</office:spreadsheet></office:body></office:document-content>"#;

    #[test]
    fn text_documents_keep_headings_lists_and_breaks() {
        assert_eq!(
            text_of(TEXT),
            "## Chapter\nHello  there\tx\nnext & last\n• one\n• two\n\nend\n"
        );
    }

    #[test]
    fn spreadsheets_become_rows_without_the_empty_millions() {
        assert_eq!(text_of(SHEET), "## Budget\nItem | 5\n\n");
    }

    #[test]
    fn odt_and_ods_files_are_read_through_their_zip() {
        let tmp = tempfile::tempdir().unwrap();
        let odt = zip_of(tmp.path(), "a.odt", &[("content.xml", TEXT)]);
        assert!(crate::extract(&odt).unwrap().text.starts_with("## Chapter"));
        let ods = zip_of(tmp.path(), "a.ods", &[("content.xml", SHEET)]);
        let text = crate::extract(&ods).unwrap();
        assert_eq!(text.kind, crate::Kind::Ods);
        assert!(text.text.contains("Item | 5"));
        let wrong = zip_of(tmp.path(), "b.odt", &[("x", "y")]);
        assert!(crate::extract(&wrong).is_err());
    }
}
