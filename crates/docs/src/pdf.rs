//! PDF files: the text of every page, when the file has any.

use std::path::Path;

use crate::DocError;

pub(crate) fn extract(path: &Path) -> Result<String, DocError> {
    let bytes = std::fs::read(path)?;
    if !bytes.starts_with(b"%PDF")
        && !bytes[..bytes.len().min(1024)]
            .windows(4)
            .any(|w| w == b"%PDF")
    {
        return Err(DocError::Corrupt("to nie jest plik PDF".into()));
    }
    // The PDF reader meets all kinds of damaged and exotic files; one that panics must not
    // take the program down.
    let pages = std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem_by_pages(&bytes))
        .map_err(|_| DocError::Corrupt("nie udało się odczytać tego PDF".into()))?
        .map_err(|e| {
            let text = e.to_string();
            if text.to_lowercase().contains("encrypt") || text.to_lowercase().contains("password") {
                DocError::Encrypted
            } else {
                DocError::Corrupt(text)
            }
        })?;
    if pages.iter().all(|page| page.trim().is_empty()) {
        return Err(DocError::NoText);
    }
    let mut out = String::new();
    for (number, page) in pages.iter().enumerate() {
        out.push_str(&format!("──── strona {} ────\n", number + 1));
        let text = page.trim();
        if text.is_empty() {
            out.push_str("(brak tekstu na tej stronie)\n");
        } else {
            out.push_str(text);
            out.push('\n');
        }
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    /// A one-page PDF with the text `Hello PDF world`, with a correct cross-reference table.
    fn tiny_pdf(text: &str) -> Vec<u8> {
        let stream = format!("BT /F1 18 Tf 72 720 Td ({text}) Tj ET");
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
            format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
        }
        let xref = pdf.len();
        pdf.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for offset in offsets {
            pdf.extend(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        pdf
    }

    #[test]
    fn the_text_of_a_simple_pdf_is_extracted() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a.pdf");
        std::fs::write(&path, tiny_pdf("Hello PDF world")).unwrap();
        let text = crate::extract(&path).unwrap();
        assert_eq!(text.kind, crate::Kind::Pdf);
        assert!(text.text.contains("strona 1"), "{}", text.text);
        assert!(text.text.contains("Hello PDF world"), "{}", text.text);
    }

    #[test]
    fn a_pdf_without_text_says_so() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("scan.pdf");
        std::fs::write(&path, tiny_pdf("")).unwrap();
        let err = crate::extract(&path).unwrap_err();
        assert!(matches!(err, crate::DocError::NoText), "{err}");
    }

    #[test]
    fn damaged_pdfs_are_errors_not_panics() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("bad.pdf");
        let mut bytes = tiny_pdf("x");
        bytes.truncate(bytes.len() / 2);
        std::fs::write(&path, bytes).unwrap();
        assert!(crate::extract(&path).is_err());
        std::fs::write(&path, b"%PDF-1.4 garbage garbage").unwrap();
        assert!(crate::extract(&path).is_err());
    }
}
