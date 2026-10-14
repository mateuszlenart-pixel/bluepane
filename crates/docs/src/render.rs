//! PDF pages as pictures (pure Rust, on the CPU).

use std::sync::Arc;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{render, PixmapSettings, RenderCache, RenderSettings};

use crate::DocError;

/// The widest picture a page is rendered to.
pub const MAX_WIDTH: u32 = 4000;

/// A rendered page: 8-bit RGBA, rows top to bottom, opaque (white paper).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// An opened PDF. Pages are parsed and rendered one at a time, on demand.
#[derive(Clone)]
pub struct PdfDocument {
    bytes: Arc<Vec<u8>>,
    /// Page sizes in points (1/72 inch), after rotation.
    sizes: Vec<(f32, f32)>,
}

impl std::fmt::Debug for PdfDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PdfDocument({} pages)", self.sizes.len())
    }
}

fn parse(bytes: &Arc<Vec<u8>>) -> Result<Pdf, DocError> {
    Pdf::new(bytes.clone()).map_err(|e| {
        let text = format!("{e:?}");
        if text.to_lowercase().contains("encrypt") {
            DocError::Encrypted
        } else {
            DocError::Corrupt(format!("PDF: {text}"))
        }
    })
}

impl PdfDocument {
    pub fn open(bytes: Vec<u8>) -> Result<PdfDocument, DocError> {
        let bytes = Arc::new(bytes);
        // The renderer meets damaged and exotic files; a panic must not end the program.
        let sizes = std::panic::catch_unwind(|| {
            let pdf = parse(&bytes)?;
            Ok::<_, DocError>(
                pdf.pages()
                    .iter()
                    .map(|page| page.render_dimensions())
                    .collect::<Vec<_>>(),
            )
        })
        .map_err(|_| DocError::Corrupt("nie udało się odczytać tego PDF".into()))??;
        if sizes.is_empty() {
            return Err(DocError::Corrupt("PDF nie ma stron".into()));
        }
        Ok(PdfDocument { bytes, sizes })
    }

    pub fn pages(&self) -> usize {
        self.sizes.len()
    }

    /// Size of a page in points (1/72 inch).
    pub fn page_size(&self, page: usize) -> Option<(f32, f32)> {
        self.sizes.get(page).copied()
    }

    /// Renders a page `width` pixels wide (at most [`MAX_WIDTH`]).
    pub fn render(&self, page: usize, width: u32) -> Result<PageImage, DocError> {
        let (page_width, page_height) = self
            .page_size(page)
            .ok_or_else(|| DocError::Corrupt(format!("nie ma strony {}", page + 1)))?;
        let width = width.clamp(16, MAX_WIDTH);
        let scale = width as f32 / page_width.max(1.0);
        // The pixmap is at most 65535 pixels either way.
        let scale = scale.min(60_000.0 / page_height.max(1.0));
        let bytes = self.bytes.clone();
        std::panic::catch_unwind(move || {
            let pdf = parse(&bytes)?;
            let page = pdf
                .pages()
                .get(page)
                .ok_or_else(|| DocError::Corrupt("brak strony".into()))?;
            let cache = RenderCache::new();
            let pixmap = render(
                page,
                &cache,
                &InterpreterSettings::default(),
                &RenderSettings::default(),
                &PixmapSettings {
                    x_scale: scale,
                    y_scale: scale,
                    bg_color: WHITE,
                },
            );
            Ok::<_, DocError>(PageImage {
                width: u32::from(pixmap.width()),
                height: u32::from(pixmap.height()),
                rgba: pixmap.data_as_u8_slice().to_vec(),
            })
        })
        .map_err(|_| DocError::Corrupt("nie udało się narysować strony".into()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::tests::tiny_pdf;

    #[test]
    fn a_page_is_rendered_as_a_picture_with_ink_on_white_paper() {
        let doc = PdfDocument::open(tiny_pdf("Hello PDF world")).unwrap();
        assert_eq!(doc.pages(), 1);
        assert_eq!(doc.page_size(0), Some((612.0, 792.0)));
        assert_eq!(doc.page_size(1), None);
        let image = doc.render(0, 306).unwrap();
        assert_eq!(image.width, 306);
        assert_eq!(image.height, 396);
        assert_eq!(image.rgba.len(), (image.width * image.height * 4) as usize);
        let dark = image
            .rgba
            .chunks(4)
            .filter(|px| px[0] < 128 && px[1] < 128 && px[2] < 128)
            .count();
        let white = image
            .rgba
            .chunks(4)
            .filter(|px| px[..3] == [255, 255, 255])
            .count();
        assert!(dark > 20, "the text left ink: {dark}");
        assert!(white > image.rgba.len() / 8, "paper is white: {white}");
        assert!(
            image.rgba.chunks(4).all(|px| px[3] == 255),
            "no transparency"
        );
    }

    #[test]
    fn widths_are_limited_and_bad_pages_are_errors() {
        let doc = PdfDocument::open(tiny_pdf("x")).unwrap();
        assert_eq!(doc.render(0, 1).unwrap().width, 16);
        assert_eq!(doc.render(0, 100_000).unwrap().width, MAX_WIDTH);
        assert!(doc.render(5, 100).is_err());
    }

    #[test]
    fn files_that_are_not_pdfs_are_errors() {
        assert!(PdfDocument::open(b"hello".to_vec()).is_err());
        assert!(PdfDocument::open(Vec::new()).is_err());
    }
}
