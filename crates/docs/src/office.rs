//! Word, Excel and OpenDocument files to PDF through LibreOffice, when it is installed, so
//! they can be shown as pages. Nothing is linked: LibreOffice runs as a separate program
//! in a private profile, without a window, and is stopped when it takes too long.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::DocError;

/// How long a conversion may take.
const TIMEOUT: Duration = Duration::from_secs(90);

/// The LibreOffice program: `BLUEPANE_OFFICE` if set (a path), else the usual places.
/// `BLUEPANE_NO_OFFICE=1` switches the use of LibreOffice off.
pub fn office_program() -> Option<PathBuf> {
    if std::env::var_os("BLUEPANE_NO_OFFICE").is_some_and(|v| v != "0" && !v.is_empty()) {
        return None;
    }
    if let Some(custom) = std::env::var_os("BLUEPANE_OFFICE") {
        let custom = PathBuf::from(custom);
        return custom.exists().then_some(custom);
    }
    let in_path = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default();
    let names = ["soffice", "libreoffice", "soffice.exe"];
    let fixed = [
        r"C:\Program Files\LibreOffice\program\soffice.exe",
        r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
        "/Applications/LibreOffice.app/Contents/MacOS/soffice",
        "/opt/libreoffice/program/soffice",
    ]
    .map(PathBuf::from);
    in_path
        .iter()
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .chain(fixed)
        .find(|candidate| candidate.is_file())
}

fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let text = text.trim_start_matches('/');
    format!("file:///{}", text.replace(' ', "%20"))
}

/// Converts `file` to a PDF inside `outdir` and returns its path. `cancelled` is polled
/// while LibreOffice works; the conversion is abandoned (and the program killed) when it
/// says yes or the time is up.
pub fn to_pdf(
    program: &Path,
    file: &Path,
    outdir: &Path,
    cancelled: &dyn Fn() -> bool,
) -> Result<PathBuf, DocError> {
    std::fs::create_dir_all(outdir)?;
    // A profile of its own: a LibreOffice the user has open would otherwise take the job
    // (and block), and this run must not touch the user's settings.
    let profile = outdir.join("profile");
    let mut child = Command::new(program)
        .arg("--headless")
        .arg("--norestore")
        .arg("--nolockcheck")
        .arg(format!("-env:UserInstallation={}", file_url(&profile)))
        .arg("--convert-to")
        .arg("pdf")
        .arg("--outdir")
        .arg(outdir)
        .arg(file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| DocError::Corrupt(format!("nie udało się uruchomić LibreOffice: {e}")))?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if cancelled() || started.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(DocError::Corrupt(if cancelled() {
                "przerwano konwersję".into()
            } else {
                "LibreOffice nie zdążył przekonwertować dokumentu".into()
            }));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let stem = file.file_stem().unwrap_or_default();
    let pdf = outdir.join(stem).with_extension("pdf");
    if !status.success() || !pdf.is_file() {
        return Err(DocError::Corrupt(
            "LibreOffice nie przekonwertował dokumentu (chroniony hasłem albo uszkodzony?)".into(),
        ));
    }
    Ok(pdf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_urls_are_built_for_both_kinds_of_paths() {
        assert_eq!(
            file_url(Path::new("/tmp/a b/profile")),
            "file:///tmp/a%20b/profile"
        );
        assert_eq!(
            file_url(Path::new(r"C:\Users\me\x")),
            "file:///C:/Users/me/x"
        );
    }

    #[test]
    fn the_program_can_be_switched_off_or_pointed_elsewhere() {
        std::env::set_var("BLUEPANE_NO_OFFICE", "1");
        assert_eq!(office_program(), None);
        std::env::remove_var("BLUEPANE_NO_OFFICE");
        std::env::set_var("BLUEPANE_OFFICE", "/definitely/not/here");
        assert_eq!(office_program(), None);
        std::env::remove_var("BLUEPANE_OFFICE");
    }

    #[test]
    fn a_missing_program_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let err = to_pdf(
            Path::new("/definitely/not/here"),
            &tmp.path().join("a.docx"),
            tmp.path(),
            &|| false,
        )
        .unwrap_err();
        assert!(err.to_string().contains("LibreOffice"), "{err}");
    }
}

#[cfg(test)]
mod real_office_tests {
    use super::*;

    /// Runs only where LibreOffice is installed (it is slow the first time).
    #[test]
    fn a_text_file_becomes_a_pdf_with_pages() {
        let Some(program) = office_program() else {
            return;
        };
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("note.txt");
        std::fs::write(&source, "Zażółć gęślą jaźń\nsecond line\n").unwrap();
        let out = tmp.path().join("out");
        let pdf = to_pdf(&program, &source, &out, &|| false).unwrap();
        assert!(pdf.is_file());
        let doc = crate::PdfDocument::open(std::fs::read(&pdf).unwrap()).unwrap();
        assert!(doc.pages() >= 1);
        // Cancelling stops it at once.
        let err = to_pdf(&program, &source, &tmp.path().join("out2"), &|| true).unwrap_err();
        assert!(err.to_string().contains("przerwano"), "{err}");
    }
}
