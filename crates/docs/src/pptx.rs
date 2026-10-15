//! PowerPoint presentations: the text of every slide, in slide order.

use std::path::Path;

use crate::xml::{events, Event};
use crate::{member, open_zip, DocError};

/// A presentation can have thousands of slides; only this many are read.
const MAX_SLIDES: usize = 1000;

pub(crate) fn extract(path: &Path) -> Result<String, DocError> {
    let mut zip = open_zip(path)?;
    let mut names: Vec<(usize, String)> = zip
        .file_names()
        .filter_map(Result::ok)
        .map(std::borrow::Cow::into_owned)
        .filter_map(|name| {
            let number = name
                .strip_prefix("ppt/slides/slide")?
                .strip_suffix(".xml")?
                .parse::<usize>()
                .ok()?;
            Some((number, name))
        })
        .collect();
    if names.is_empty() {
        return Err(DocError::Corrupt(
            "brak slajdów (to nie jest plik PowerPoint)".into(),
        ));
    }
    names.sort();
    let mut out = String::new();
    for (index, (_, name)) in names.iter().take(MAX_SLIDES).enumerate() {
        let xml = member(&mut zip, name)?.unwrap_or_default();
        out.push_str(&format!("──── slajd {} ────\n", index + 1));
        let text = text_of(&xml);
        if text.is_empty() {
            out.push_str("(brak tekstu na tym slajdzie)\n");
        } else {
            out.push_str(&text);
        }
        out.push('\n');
    }
    Ok(out)
}

pub(crate) fn text_of(xml: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut paragraph = String::new();
    let mut in_text = false;
    for event in events(xml) {
        match event {
            Event::Start { name, .. } => match name.as_str() {
                "a:p" => paragraph.clear(),
                "a:t" => in_text = true,
                "a:br" => paragraph.push('\n'),
                "a:tab" => paragraph.push('\t'),
                _ => {}
            },
            Event::End(name) => match name.as_str() {
                "a:t" => in_text = false,
                "a:p" => {
                    let line = paragraph.trim_end();
                    if !line.is_empty() {
                        lines.push(line.to_string());
                    }
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
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn pptx(slides: &[(&str, &str)]) -> tempfile::NamedTempFile {
        let file = tempfile::Builder::new().suffix(".pptx").tempfile().unwrap();
        let mut zip = zip::ZipWriter::new(file.reopen().unwrap());
        let options = zip::write::SimpleFileOptions::default();
        for (name, xml) in slides {
            zip.start_file(*name, options).unwrap();
            zip.write_all(xml.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        file
    }

    fn slide(paragraphs: &[&str]) -> String {
        let body: String = paragraphs
            .iter()
            .map(|p| format!("<a:p><a:r><a:t>{p}</a:t></a:r></a:p>"))
            .collect();
        format!("<p:sld><p:cSld><p:spTree><p:sp><p:txBody>{body}</p:txBody></p:sp></p:spTree></p:cSld></p:sld>")
    }

    #[test]
    fn slides_come_out_in_numeric_order_with_their_text() {
        let (one, two, ten) = (
            slide(&["Tytuł", "Punkt &amp; drugi"]),
            slide(&["Drugi slajd"]),
            slide(&[]),
        );
        let file = pptx(&[
            ("ppt/slides/slide10.xml", &ten),
            ("ppt/slides/slide2.xml", &two),
            ("ppt/slides/slide1.xml", &one),
        ]);
        let text = extract(file.path()).unwrap();
        let first = text.find("slajd 1 ").unwrap();
        let second = text.find("slajd 2 ").unwrap();
        let third = text.find("slajd 3 ").unwrap();
        assert!(first < second && second < third, "{text}");
        assert!(text.contains("Tytuł\nPunkt & drugi"), "{text}");
        assert!(text.contains("Drugi slajd"), "{text}");
        assert!(text.contains("(brak tekstu na tym slajdzie)"), "{text}");
    }

    #[test]
    fn a_zip_without_slides_is_not_a_presentation() {
        let file = pptx(&[("readme.txt", "x")]);
        assert!(matches!(extract(file.path()), Err(DocError::Corrupt(_))));
    }
}
