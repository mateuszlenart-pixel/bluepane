//! Excel workbooks: every sheet as rows of cells.

use std::collections::HashMap;
use std::path::Path;

use crate::xml::{attr, events, Event};
use crate::{member, open_zip, DocError};

const MAX_ROWS: usize = 20_000;

pub(crate) fn extract(path: &Path) -> Result<String, DocError> {
    let mut zip = open_zip(path)?;
    let workbook = member(&mut zip, "xl/workbook.xml")?
        .ok_or_else(|| DocError::Corrupt("brak xl/workbook.xml (to nie jest plik Excel)".into()))?;
    let rels = member(&mut zip, "xl/_rels/workbook.xml.rels")?.unwrap_or_default();
    let shared = member(&mut zip, "xl/sharedStrings.xml")?
        .map(|xml| shared_strings(&xml))
        .unwrap_or_default();
    let targets = relationship_targets(&rels);

    let mut out = String::new();
    for (index, (name, rid)) in sheets(&workbook).into_iter().enumerate() {
        let target = targets
            .get(&rid)
            .map(|t| format!("xl/{}", t.trim_start_matches('/').trim_start_matches("xl/")))
            .unwrap_or_else(|| format!("xl/worksheets/sheet{}.xml", index + 1));
        let Some(xml) = member(&mut zip, &target)? else {
            continue;
        };
        out.push_str(&format!("## {name}\n"));
        out.push_str(&sheet_text(&xml, &shared));
        out.push_str("\n\n");
    }
    if out.is_empty() {
        return Err(DocError::Corrupt("skoroszyt nie ma arkuszy".into()));
    }
    Ok(out)
}

/// `(name, relationship id)` of every sheet, in workbook order.
fn sheets(xml: &str) -> Vec<(String, String)> {
    events(xml)
        .into_iter()
        .filter_map(|event| match event {
            Event::Start { name, attrs } if name == "sheet" => Some((
                attr(&attrs, "name")?.to_string(),
                attr(&attrs, "r:id").unwrap_or_default().to_string(),
            )),
            _ => None,
        })
        .collect()
}

fn relationship_targets(xml: &str) -> HashMap<String, String> {
    events(xml)
        .into_iter()
        .filter_map(|event| match event {
            Event::Start { name, attrs } if name == "Relationship" => Some((
                attr(&attrs, "Id")?.to_string(),
                attr(&attrs, "Target")?.to_string(),
            )),
            _ => None,
        })
        .collect()
}

/// The shared string table: one entry per `<si>`, its text runs joined.
fn shared_strings(xml: &str) -> Vec<String> {
    let mut strings = Vec::new();
    let mut current = String::new();
    let mut in_text = false;
    let mut skip = 0; // phonetic hints are not part of the text
    for event in events(xml) {
        match event {
            Event::Start { name, .. } => match name.as_str() {
                "si" => current.clear(),
                "t" => in_text = true,
                "rPh" => skip += 1,
                _ => {}
            },
            Event::End(name) => match name.as_str() {
                "t" => in_text = false,
                "rPh" => skip -= 1,
                "si" => strings.push(std::mem::take(&mut current)),
                _ => {}
            },
            Event::Text(text) => {
                if in_text && skip == 0 {
                    current.push_str(&text);
                }
            }
        }
    }
    strings
}

/// Column number (0-based) of a cell reference such as `AB12`.
fn column_of(reference: &str) -> usize {
    let letters = reference.chars().take_while(char::is_ascii_alphabetic);
    letters.fold(0, |n, c| {
        n * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1)
    }) - 1
}

fn sheet_text(xml: &str, shared: &[String]) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut kind = String::new();
    let mut column = 0;
    let mut value = String::new();
    let mut in_value = false;
    let mut in_inline = false;
    for event in events(xml) {
        match event {
            Event::Start { name, attrs } => match name.as_str() {
                "row" => row.clear(),
                "c" => {
                    kind = attr(&attrs, "t").unwrap_or("n").to_string();
                    column = attr(&attrs, "r").map_or(row.len(), column_of);
                    value.clear();
                }
                "v" => in_value = true,
                "is" => in_inline = true,
                _ => {}
            },
            Event::End(name) => match name.as_str() {
                "v" => in_value = false,
                "is" => in_inline = false,
                "c" => {
                    let shown = match kind.as_str() {
                        "s" => value
                            .trim()
                            .parse::<usize>()
                            .ok()
                            .and_then(|i| shared.get(i).cloned())
                            .unwrap_or_default(),
                        "b" => if value.trim() == "1" { "TRUE" } else { "FALSE" }.to_string(),
                        _ => value.clone(),
                    };
                    if column < 1000 {
                        if row.len() <= column {
                            row.resize(column + 1, String::new());
                        }
                        row[column] = shown;
                    }
                }
                "row" => {
                    while row.last().is_some_and(String::is_empty) {
                        row.pop();
                    }
                    if !row.is_empty() && lines.len() < MAX_ROWS {
                        lines.push(row.join(" | "));
                    }
                }
                _ => {}
            },
            Event::Text(text) => {
                if in_value || in_inline {
                    value.push_str(&text);
                }
            }
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::zip_of;

    const WORKBOOK: &str = r#"<workbook><sheets><sheet name="Sales" sheetId="1" r:id="rId1"/><sheet name="Notes" sheetId="2" r:id="rId2"/></sheets></workbook>"#;
    const RELS: &str = r#"<Relationships><Relationship Id="rId2" Target="worksheets/sheet7.xml"/><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#;
    const SHARED: &str = r#"<sst><si><t>Name</t></si><si><r><t>Zażółć </t></r><r><t>gęślą</t></r><rPh><t>ignored</t></rPh></si></sst>"#;
    const SHEET1: &str = r#"<worksheet><sheetData>
<row r="1"><c r="A1" t="s"><v>0</v></c><c r="C1"><v>42.5</v></c></row>
<row r="2"><c r="A2" t="s"><v>1</v></c><c r="B2" t="b"><v>1</v></c><c r="C2" t="inlineStr"><is><t>inline</t></is></c></row>
<row r="3"/>
</sheetData></worksheet>"#;
    const SHEET7: &str =
        r#"<worksheet><sheetData><row><c r="B1"><v>x</v></c></row></sheetData></worksheet>"#;

    #[test]
    fn column_letters_become_numbers() {
        assert_eq!(column_of("A1"), 0);
        assert_eq!(column_of("C12"), 2);
        assert_eq!(column_of("AA3"), 26);
    }

    #[test]
    fn a_workbook_becomes_sheets_of_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let path = zip_of(
            tmp.path(),
            "a.xlsx",
            &[
                ("xl/workbook.xml", WORKBOOK),
                ("xl/_rels/workbook.xml.rels", RELS),
                ("xl/sharedStrings.xml", SHARED),
                ("xl/worksheets/sheet1.xml", SHEET1),
                ("xl/worksheets/sheet7.xml", SHEET7),
            ],
        );
        let text = crate::extract(&path).unwrap();
        assert_eq!(
            text.text,
            "## Sales\nName |  | 42.5\nZażółć gęślą | TRUE | inline\n\n## Notes\n | x\n\n"
        );
    }

    #[test]
    fn something_that_is_not_a_workbook_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let path = zip_of(tmp.path(), "a.xlsx", &[("x", "y")]);
        assert!(crate::extract(&path).is_err());
    }
}
