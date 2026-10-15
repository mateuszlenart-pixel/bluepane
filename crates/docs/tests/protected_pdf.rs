//! A PDF made with Ghostscript (RC4, 128 bit) whose user password is `sekret`.

use bluepane_docs::{DocError, PdfDocument};

fn bytes() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/protected.pdf"
    ))
    .unwrap()
}

#[test]
fn a_protected_pdf_asks_for_a_password_and_refuses_a_wrong_one() {
    assert!(matches!(
        PdfDocument::open(bytes()),
        Err(DocError::Encrypted)
    ));
    assert!(matches!(
        PdfDocument::open_with_password(bytes(), "nope"),
        Err(DocError::WrongPassword)
    ));
}

#[test]
fn the_right_password_opens_and_renders_the_pages() {
    let doc = PdfDocument::open_with_password(bytes(), "sekret").unwrap();
    assert_eq!(doc.pages(), 1);
    let image = doc.render(0, 300).unwrap();
    let ink = image
        .rgba
        .chunks(4)
        .filter(|px| px[0] < 128 && px[1] < 128 && px[2] < 128)
        .count();
    assert!(ink > 20, "the page was drawn, not blank: {ink}");
}
