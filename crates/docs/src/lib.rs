//! Text of documents for the viewer: Word (DOCX), OpenDocument (ODT, ODS), Excel (XLSX) and
//! PDF. It is a reading aid, not a renderer: paragraphs, headings, lists and tables come
//! out as plain text; layout, pictures and fonts do not. Nothing is written and nothing
//! runs: the files are only read (the XML formats are zip archives).

mod docx;
mod odf;
mod office;
mod pdf;
mod render;
mod xlsx;
mod xml;

pub use office::{office_program, to_pdf};
pub use render::{PageImage, PdfDocument};

use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

/// Largest document file that is read (zip based formats).
pub const MAX_INPUT: u64 = 64 * 1024 * 1024;
/// Largest PDF that is read (extracting text from a big PDF takes a while).
pub const MAX_PDF: u64 = 32 * 1024 * 1024;
/// The text is cut off after this many bytes.
pub const MAX_TEXT: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Docx,
    Odt,
    Ods,
    Xlsx,
    Pdf,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Docx => "dokument Word",
            Kind::Odt => "dokument OpenDocument",
            Kind::Ods => "arkusz OpenDocument",
            Kind::Xlsx => "skoroszyt Excel",
            Kind::Pdf => "dokument PDF",
        }
    }
}

/// What a file is, by its extension.
pub fn kind_of(path: &Path) -> Option<Kind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "docx" | "docm" | "dotx" => Kind::Docx,
        "odt" | "ott" => Kind::Odt,
        "ods" | "ots" => Kind::Ods,
        "xlsx" | "xlsm" => Kind::Xlsx,
        "pdf" => Kind::Pdf,
        _ => return None,
    })
}

#[derive(Debug)]
pub enum DocError {
    Io(io::Error),
    TooBig {
        size: u64,
        limit: u64,
    },
    /// The file is damaged or not what its extension says.
    Corrupt(String),
    /// Protected with a password.
    Encrypted,
    /// A PDF without any text (a scan, drawings).
    NoText,
}

impl fmt::Display for DocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DocError::Io(e) => write!(f, "{e}"),
            DocError::TooBig { size, limit } => write!(
                f,
                "plik ma {} MB, za duży na podgląd tekstowy (limit {} MB): otwórz go w zewnętrznym programie (F4)",
                size / (1024 * 1024),
                limit / (1024 * 1024)
            ),
            DocError::Corrupt(what) => write!(f, "uszkodzony lub nieobsługiwany dokument: {what}"),
            DocError::Encrypted => write!(
                f,
                "dokument jest chroniony hasłem: otwórz go w zewnętrznym programie (F4)"
            ),
            DocError::NoText => write!(
                f,
                "w tym PDF nie ma tekstu (skan lub rysunek): otwórz go w zewnętrznym programie (F4)"
            ),
        }
    }
}

impl std::error::Error for DocError {}

impl From<io::Error> for DocError {
    fn from(e: io::Error) -> Self {
        DocError::Io(e)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    pub kind: Kind,
    pub text: String,
    /// The document was longer than [`MAX_TEXT`].
    pub truncated: bool,
}

/// The text of a document file.
pub fn extract(path: &Path) -> Result<Text, DocError> {
    let kind = kind_of(path).ok_or_else(|| DocError::Corrupt("nieznany typ dokumentu".into()))?;
    let size = fs::metadata(path)?.len();
    let limit = if kind == Kind::Pdf {
        MAX_PDF
    } else {
        MAX_INPUT
    };
    if size > limit {
        return Err(DocError::TooBig { size, limit });
    }
    let mut text = match kind {
        Kind::Docx => docx::extract(path)?,
        Kind::Odt | Kind::Ods => odf::extract(path)?,
        Kind::Xlsx => xlsx::extract(path)?,
        Kind::Pdf => pdf::extract(path)?,
    };
    let truncated = text.len() > MAX_TEXT;
    if truncated {
        let mut cut = MAX_TEXT;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str("\n… (dalsza część dokumentu została pominięta)\n");
    }
    Ok(Text {
        kind,
        text,
        truncated,
    })
}

/// A zip member as text, or `None` when it is not there.
pub(crate) fn member(
    archive: &mut zip::ZipArchive<fs::File>,
    name: &str,
) -> Result<Option<String>, DocError> {
    use std::io::Read;
    let mut file = match archive.by_name(name) {
        Ok(file) => file,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(DocError::Corrupt(e.to_string())),
    };
    // XML parts are small; a member claiming to be huge is not a document.
    if file.size() > MAX_INPUT {
        return Err(DocError::Corrupt(format!("część {name} jest zbyt duża")));
    }
    let mut text = String::new();
    file.read_to_string(&mut text)
        .map_err(|e| DocError::Corrupt(format!("{name}: {e}")))?;
    Ok(Some(text))
}

pub(crate) fn open_zip(path: &Path) -> Result<zip::ZipArchive<fs::File>, DocError> {
    let file = fs::File::open(path)?;
    zip::ZipArchive::new(file).map_err(|e| match e {
        zip::result::ZipError::InvalidArchive(why) => DocError::Corrupt(why.to_string()),
        zip::result::ZipError::UnsupportedArchive(_) => DocError::Encrypted,
        other => DocError::Corrupt(other.to_string()),
    })
}

#[cfg(test)]
pub(crate) mod testing {
    //! Small documents built in memory for the tests.
    use std::io::Write;
    use std::path::{Path, PathBuf};

    pub fn zip_of(dir: &Path, name: &str, parts: &[(&str, &str)]) -> PathBuf {
        let path = dir.join(name);
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (member, content) in parts {
            zip.start_file(*member, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_follow_the_extension() {
        for (name, kind) in [
            ("a.DOCX", Kind::Docx),
            ("a.docm", Kind::Docx),
            ("a.odt", Kind::Odt),
            ("a.ods", Kind::Ods),
            ("a.xlsx", Kind::Xlsx),
            ("a.pdf", Kind::Pdf),
        ] {
            assert_eq!(kind_of(Path::new(name)), Some(kind), "{name}");
        }
        assert_eq!(kind_of(Path::new("a.txt")), None);
        assert_eq!(kind_of(Path::new("noext")), None);
    }

    #[test]
    fn a_file_that_is_not_a_document_is_an_error_not_a_crash() {
        let tmp = tempfile::tempdir().unwrap();
        for name in ["x.docx", "x.odt", "x.xlsx", "x.pdf", "x.ods"] {
            let path = tmp.path().join(name);
            fs::write(&path, "this is not what the extension says").unwrap();
            assert!(extract(&path).is_err(), "{name}");
        }
        assert!(extract(&tmp.path().join("missing.docx")).is_err());
    }

    #[test]
    fn huge_files_are_refused_before_reading() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("big.pdf");
        let file = fs::File::create(&path).unwrap();
        file.set_len(MAX_PDF + 1).unwrap();
        let err = extract(&path).unwrap_err();
        assert!(matches!(err, DocError::TooBig { .. }), "{err}");
        assert!(err.to_string().contains("F4"));
    }
}
