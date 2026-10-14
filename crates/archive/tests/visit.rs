//! Visiting archive files in turn, for every format.

use std::fs;
use std::path::Path;

use bluepane_archive::{create, visit, ArchiveError, CreateOptions, Format, Level, VisitedEntry};
use bluepane_ops::{FailOnConflict, Progress};

fn pack(root: &Path, out: &str, format: Format, password: Option<&str>) -> std::path::PathBuf {
    let src = root.join("src");
    fs::create_dir_all(src.join("docs/sub")).unwrap();
    fs::write(src.join("docs/a.txt"), "alpha").unwrap();
    fs::write(src.join("docs/sub/b.txt"), "bravo").unwrap();
    fs::write(src.join("top.txt"), "top").unwrap();
    let path = root.join(out);
    create(
        &[src.join("docs"), src.join("top.txt")],
        &path,
        &CreateOptions {
            format,
            level: Level::Normal,
            password: password.map(str::to_string),
            split: None,
            encrypt_names: false,
        },
        &Progress::new(),
        &mut FailOnConflict,
    )
    .unwrap();
    path
}

/// (path, content, readable) of every visited file, and how many were unreadable.
type Visited = (Vec<(String, String, bool)>, u64);

fn collect(path: &Path, password: Option<&str>) -> Result<Visited, ArchiveError> {
    let mut seen = Vec::new();
    let report = visit(
        path,
        password,
        &Progress::new(),
        &mut |entry: &VisitedEntry, data| {
            let mut text = String::new();
            data.read_to_string(&mut text).unwrap();
            seen.push((entry.path.clone(), text, entry.readable));
            Ok(())
        },
    )?;
    seen.sort();
    Ok((seen, report.unreadable))
}

#[test]
fn every_format_yields_each_file_with_its_content() {
    for (format, name) in [
        (Format::Zip, "v.zip"),
        (Format::Tar, "v.tar"),
        (Format::TarGz, "v.tar.gz"),
        (Format::SevenZ, "v.7z"),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let archive = pack(tmp.path(), name, format, None);
        let (seen, unreadable) = collect(&archive, None).unwrap();
        let names: Vec<_> = seen.iter().map(|(p, _, _)| p.as_str()).collect();
        assert_eq!(names, ["docs/a.txt", "docs/sub/b.txt", "top.txt"], "{name}");
        assert_eq!(seen[0].1, "alpha", "{name}");
        assert_eq!(seen[1].1, "bravo", "{name}");
        assert_eq!(unreadable, 0);
    }
}

#[test]
fn encrypted_zip_without_a_password_still_shows_names() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = pack(tmp.path(), "p.zip", Format::Zip, Some("secret"));
    let (seen, unreadable) = collect(&archive, None).unwrap();
    assert_eq!(seen.len(), 3);
    assert!(seen
        .iter()
        .all(|(_, text, readable)| text.is_empty() && !readable));
    assert_eq!(unreadable, 3);

    let (seen, unreadable) = collect(&archive, Some("secret")).unwrap();
    assert_eq!(seen[0], ("docs/a.txt".into(), "alpha".into(), true));
    assert_eq!(unreadable, 0);

    let (_, unreadable) = collect(&archive, Some("wrong")).unwrap();
    assert_eq!(unreadable, 3, "a wrong password reads nothing");
}

#[test]
fn encrypted_7z_needs_the_password() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = pack(tmp.path(), "p.7z", Format::SevenZ, Some("secret"));
    assert!(matches!(
        collect(&archive, None),
        Err(ArchiveError::NeedsPassword | ArchiveError::WrongPassword)
    ));
    let (seen, _) = collect(&archive, Some("secret")).unwrap();
    assert_eq!(seen[0].1, "alpha");
}

#[test]
fn cancelling_stops_the_visit() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = pack(tmp.path(), "c.zip", Format::Zip, None);
    let progress = Progress::new();
    progress.cancel();
    let result = visit(&archive, None, &progress, &mut |_, _| Ok(()));
    assert!(matches!(result, Err(ArchiveError::Cancelled)));
}
