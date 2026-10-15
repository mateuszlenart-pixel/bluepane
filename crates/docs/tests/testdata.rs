//! The sample files in `testdata/` (used for manual testing) must work with this crate.

use std::path::PathBuf;

use bluepane_docs::{extract, DocError, PdfDocument};

fn sample(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/dokumenty")
        .join(name)
}

#[test]
fn every_sample_document_gives_text() {
    for name in [
        "raport.docx",
        "raport.odt",
        "raport.rtf",
        "raport.pdf",
        "arkusz.xlsx",
        "arkusz.ods",
        "prezentacja.pptx",
        "prezentacja.odp",
    ] {
        let doc = extract(&sample(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            doc.text.trim().len() > 20,
            "{name} has text: {:?}",
            doc.text
        );
    }
    let text = extract(&sample("raport.docx")).unwrap().text;
    assert!(text.contains("Zażółć gęślą jaźń"), "{text}");
    let slides = extract(&sample("prezentacja.pptx")).unwrap().text;
    assert!(slides.contains("slajd 3"), "{slides}");
}

#[test]
fn the_sample_pdfs_render_and_the_protected_one_wants_its_password() {
    let plain = std::fs::read(sample("raport.pdf")).unwrap();
    let doc = PdfDocument::open(plain).unwrap();
    assert!(doc.pages() >= 2, "a few pages: {}", doc.pages());
    let locked = std::fs::read(sample("raport-haslo-sekret.pdf")).unwrap();
    assert!(matches!(
        PdfDocument::open(locked.clone()),
        Err(DocError::Encrypted)
    ));
    let opened = PdfDocument::open_with_password(locked, "sekret").unwrap();
    assert_eq!(opened.pages(), doc.pages());
}
