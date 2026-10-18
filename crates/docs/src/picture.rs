//! Pictures (PNG, JPEG, GIF, BMP, WebP, ICO) for the viewer and Quick View: decoded once
//! into RGBA, then scaled to the width the window asks for.

use std::path::Path;

use crate::{DocError, PageImage};

/// Pictures bigger than this many pixels are not decoded (a 16k × 16k image would need a
/// gigabyte of memory).
const MAX_PIXELS: u64 = 100_000_000;

/// True for the picture formats the viewer shows (by extension).
pub fn is_picture(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| {
            matches!(
                e.as_str(),
                "png" | "jpg" | "jpeg" | "jfif" | "gif" | "bmp" | "webp" | "ico"
            )
        })
}

/// Decodes a picture file (its content decides the format, not the name).
pub fn decode_picture(bytes: &[u8]) -> Result<PageImage, DocError> {
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| DocError::Corrupt(e.to_string()))?;
    let (width, height) = reader
        .into_dimensions()
        .map_err(|e| DocError::Corrupt(format!("nieznany albo uszkodzony obraz: {e}")))?;
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(DocError::Corrupt(format!(
            "obraz jest za duży do podglądu ({width} × {height})"
        )));
    }
    let picture = image::load_from_memory(bytes)
        .map_err(|e| DocError::Corrupt(format!("nieznany albo uszkodzony obraz: {e}")))?
        .to_rgba8();
    Ok(PageImage {
        width: picture.width(),
        height: picture.height(),
        rgba: picture.into_raw(),
    })
}

/// The picture at `width` pixels, keeping its proportions (never wider than four times the
/// original, so a tiny icon does not become a blur the size of the window).
pub fn scale_picture(picture: &PageImage, width: u32) -> PageImage {
    let width = width.clamp(1, picture.width.saturating_mul(4).max(1));
    if width == picture.width {
        return picture.clone();
    }
    let height = ((u64::from(picture.height) * u64::from(width)) / u64::from(picture.width.max(1)))
        .max(1) as u32;
    let Some(source) =
        image::RgbaImage::from_raw(picture.width, picture.height, picture.rgba.clone())
    else {
        return picture.clone();
    };
    let scaled = image::imageops::resize(
        &source,
        width,
        height,
        image::imageops::FilterType::Triangle,
    );
    PageImage {
        width,
        height,
        rgba: scaled.into_raw(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4 × 2 PNG: red on the left half, blue on the right.
    fn png() -> Vec<u8> {
        let mut picture = image::RgbaImage::new(4, 2);
        for (x, _, pixel) in picture.enumerate_pixels_mut() {
            *pixel = if x < 2 {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([0, 0, 255, 255])
            };
        }
        let mut bytes = Vec::new();
        picture
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    #[test]
    fn pictures_are_known_by_extension() {
        assert!(is_picture(Path::new("a/Zdjęcie.JPG")));
        assert!(is_picture(Path::new("x.webp")));
        assert!(!is_picture(Path::new("x.pdf")));
        assert!(!is_picture(Path::new("noextension")));
    }

    #[test]
    fn a_picture_is_decoded_and_scaled_keeping_its_proportions() {
        let picture = decode_picture(&png()).unwrap();
        assert_eq!((picture.width, picture.height), (4, 2));
        assert_eq!(&picture.rgba[..4], &[255, 0, 0, 255]);
        assert_eq!(&picture.rgba[12..16], &[0, 0, 255, 255]);
        let bigger = scale_picture(&picture, 8);
        assert_eq!((bigger.width, bigger.height), (8, 4));
        assert_eq!(bigger.rgba.len(), 8 * 4 * 4);
        // Never more than four times the original.
        assert_eq!(scale_picture(&picture, 1000).width, 16);
        assert_eq!(scale_picture(&picture, 0).width, 1);
    }

    #[test]
    fn damaged_or_wrong_content_is_an_error_not_a_panic() {
        assert!(decode_picture(b"not a picture").is_err());
        let mut cut = png();
        cut.truncate(cut.len() / 2);
        assert!(decode_picture(&cut).is_err());
    }
}
