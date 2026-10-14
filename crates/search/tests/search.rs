//! Searching real directory trees and archives.

use std::fs;
use std::path::{Path, PathBuf};

use bluepane_archive::{create, CreateOptions, Format, Level};
use bluepane_ops::{FailOnConflict, Progress};
use bluepane_search::{search, Encoding, Hit, Location, Query, Summary, TextQuery};

fn write(path: impl AsRef<Path>, content: impl AsRef<[u8]>) {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// A tree with source files, notes, a Windows-1250 file, a binary and a nested folder.
fn tree() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let r = tmp.path();
    write(
        r.join("a/main.rs"),
        "fn main() {\n    println!(\"needle\");\n}\n",
    );
    write(r.join("a/lib.rs"), "pub fn other() {}\n");
    write(r.join("a/notes.md"), "# Notes\nNeedle in a haystack\n");
    write(
        r.join("a/deep/er/readme.txt"),
        "nothing\nthe NEEDLE is here\n",
    );
    write(r.join("b/stara.txt"), b"tu jest za\xBF\xF3\xB3\xE6\n");
    write(r.join("b/data.bin"), b"needle\0\0\0");
    write(r.join("c/target/build.rs"), "needle\n");
    tmp
}

fn run(query: &Query) -> (Vec<Hit>, Summary) {
    let mut hits = Vec::new();
    let summary = search(query, &Progress::new(), &mut |hit| hits.push(hit)).unwrap();
    (hits, summary)
}

fn names(hits: &[Hit], root: &Path) -> Vec<String> {
    let mut names: Vec<String> = hits
        .iter()
        .map(|h| match &h.location {
            Location::File(p) => p
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/"),
            Location::InArchive { archive, inner } => format!(
                "{} > {inner}",
                archive
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            ),
        })
        .collect();
    names.sort();
    names
}

fn query(root: &Path) -> Query {
    Query {
        roots: vec![root.to_path_buf()],
        ..Query::default()
    }
}

#[test]
fn finds_files_by_mask() {
    let tmp = tree();
    let (hits, summary) = run(&Query {
        name: "*.rs".into(),
        ..query(tmp.path())
    });
    assert_eq!(
        names(&hits, tmp.path()),
        ["a/lib.rs", "a/main.rs", "c/target/build.rs"]
    );
    assert!(summary.files >= 7 && summary.folders >= 6);
    assert_eq!(summary.hits, 3);
}

#[test]
fn several_masks_and_excluded_folders() {
    let tmp = tree();
    let (hits, _) = run(&Query {
        name: "*.rs;*.md".into(),
        exclude_dirs: "target".into(),
        ..query(tmp.path())
    });
    assert_eq!(
        names(&hits, tmp.path()),
        ["a/lib.rs", "a/main.rs", "a/notes.md"]
    );
}

#[test]
fn folders_are_found_by_name_too() {
    let tmp = tree();
    let (hits, _) = run(&Query {
        name: "deep".into(),
        ..query(tmp.path())
    });
    assert_eq!(names(&hits, tmp.path()), ["a/deep"]);
    assert!(hits[0].is_dir);
}

#[test]
fn full_text_search_with_line_numbers() {
    let tmp = tree();
    let (hits, summary) = run(&Query {
        text: Some(TextQuery::new("needle")),
        ..query(tmp.path())
    });
    assert_eq!(
        names(&hits, tmp.path()),
        [
            "a/deep/er/readme.txt",
            "a/main.rs",
            "a/notes.md",
            "c/target/build.rs"
        ]
    );
    let main = hits
        .iter()
        .find(|h| h.location.name() == "main.rs")
        .unwrap();
    assert_eq!(main.matching_lines, 1);
    assert_eq!(main.lines[0].line, 2);
    assert_eq!(summary.binary, 1, "data.bin looks binary");
}

#[test]
fn text_search_respects_case_words_and_regex() {
    let tmp = tree();
    let mut text = TextQuery::new("Needle");
    text.case_sensitive = true;
    let (hits, _) = run(&Query {
        text: Some(text),
        ..query(tmp.path())
    });
    assert_eq!(names(&hits, tmp.path()), ["a/notes.md"]);

    let mut text = TextQuery::new(r"^fn \w+\(\)");
    text.regex = true;
    let (hits, _) = run(&Query {
        text: Some(text),
        name: "*.rs".into(),
        ..query(tmp.path())
    });
    assert_eq!(names(&hits, tmp.path()), ["a/main.rs"]);
}

#[test]
fn name_and_text_together_narrow_the_search() {
    let tmp = tree();
    let (hits, _) = run(&Query {
        name: "*.md".into(),
        text: Some(TextQuery::new("needle")),
        ..query(tmp.path())
    });
    assert_eq!(names(&hits, tmp.path()), ["a/notes.md"]);
}

#[test]
fn polish_text_in_windows_1250_is_found() {
    let tmp = tree();
    let (hits, _) = run(&Query {
        text: Some(TextQuery::new("zażółć")),
        ..query(tmp.path())
    });
    assert_eq!(names(&hits, tmp.path()), ["b/stara.txt"]);
    assert_eq!(hits[0].lines[0].text, "tu jest zażółć");

    let mut forced = TextQuery::new("zażółć");
    forced.encoding = Encoding::Utf8;
    let (hits, _) = run(&Query {
        text: Some(forced),
        ..query(tmp.path())
    });
    assert!(hits.is_empty());
}

#[test]
fn binary_files_can_be_searched_on_request() {
    let tmp = tree();
    let mut text = TextQuery::new("needle");
    text.search_binary = true;
    let (hits, _) = run(&Query {
        text: Some(text),
        name: "*.bin".into(),
        ..query(tmp.path())
    });
    assert_eq!(names(&hits, tmp.path()), ["b/data.bin"]);
}

#[test]
fn several_roots_are_searched_and_nested_ones_only_once() {
    let tmp = tree();
    let r = tmp.path();
    let (hits, _) = run(&Query {
        roots: vec![r.join("a"), r.join("b"), r.join("a/deep")],
        name: "*.txt".into(),
        ..Query::default()
    });
    assert_eq!(names(&hits, r), ["a/deep/er/readme.txt", "b/stara.txt"]);
}

#[test]
fn a_single_file_can_be_a_root() {
    let tmp = tree();
    let (hits, _) = run(&Query {
        roots: vec![tmp.path().join("a/main.rs")],
        text: Some(TextQuery::new("needle")),
        ..Query::default()
    });
    assert_eq!(hits.len(), 1);
}

#[test]
fn size_and_date_filters() {
    let tmp = tree();
    let (hits, _) = run(&Query {
        name: "*".into(),
        min_size: Some(30),
        ..query(tmp.path())
    });
    assert_eq!(names(&hits, tmp.path()), ["a/main.rs"]);
    let (hits, _) = run(&Query {
        name: "*.rs".into(),
        max_size: Some(1),
        ..query(tmp.path())
    });
    assert!(hits.is_empty());
    let future = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
    let (hits, _) = run(&Query {
        name: "*.rs".into(),
        modified_after: Some(future),
        ..query(tmp.path())
    });
    assert!(hits.is_empty());
}

#[test]
fn results_can_be_capped() {
    let tmp = tree();
    let (hits, summary) = run(&Query {
        name: "*.rs;*.txt;*.md".into(),
        max_results: 2,
        ..query(tmp.path())
    });
    assert_eq!(hits.len(), 2);
    assert!(summary.truncated);
}

#[test]
fn cancelling_returns_what_was_found() {
    let tmp = tree();
    let progress = Progress::new();
    progress.cancel();
    let mut hits = Vec::new();
    let summary = search(
        &Query {
            name: "*".into(),
            ..query(tmp.path())
        },
        &progress,
        &mut |hit| hits.push(hit),
    )
    .unwrap();
    assert!(summary.cancelled);
    assert!(hits.is_empty());
}

#[test]
fn bad_queries_are_errors_not_panics() {
    let tmp = tree();
    let noop = &mut |_| {};
    let p = Progress::new();
    assert!(search(&Query::default(), &p, noop).is_err(), "no roots");
    assert!(
        search(&query(tmp.path()), &p, noop).is_err(),
        "nothing to look for"
    );
    let broken = Query {
        name: "(".into(),
        name_regex: true,
        ..query(tmp.path())
    };
    assert!(search(&broken, &p, noop).is_err());
    let mut text = TextQuery::new("(");
    text.regex = true;
    let broken = Query {
        text: Some(text),
        ..query(tmp.path())
    };
    assert!(search(&broken, &p, noop).is_err());
}

#[cfg(unix)]
#[test]
fn symlinks_to_folders_are_not_followed() {
    let tmp = tree();
    std::os::unix::fs::symlink(tmp.path(), tmp.path().join("a/loop")).unwrap();
    let (hits, summary) = run(&Query {
        name: "main.rs".into(),
        ..query(tmp.path())
    });
    assert_eq!(
        hits.len(),
        1,
        "no duplicates through the link, no endless loop"
    );
    assert!(!summary.cancelled);
}

#[test]
fn unreadable_roots_are_counted() {
    let (hits, summary) = run(&Query {
        roots: vec![PathBuf::from("/definitely/not/here")],
        name: "*".into(),
        ..Query::default()
    });
    assert!(hits.is_empty());
    assert_eq!(summary.unreadable, 1);
}

// ---- archives ----------------------------------------------------------------------

fn pack(tmp: &Path, name: &str, format: Format, password: Option<&str>) -> PathBuf {
    let src = tmp.join("pack_src");
    write(
        src.join("docs/guide.txt"),
        "see the needle here\nline two\n",
    );
    write(src.join("docs/other.md"), "nothing\n");
    write(src.join("code.rs"), "// NEEDLE\n");
    let out = tmp.join(name);
    create(
        &[src.join("docs"), src.join("code.rs")],
        &out,
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
    out
}

fn archives_root(
    format: Format,
    name: &str,
    password: Option<&str>,
) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let archive = pack(tmp.path(), name, format, password);
    let root = tmp.path().join("root");
    fs::create_dir_all(&root).unwrap();
    fs::rename(&archive, root.join(name)).unwrap();
    (tmp, root)
}

#[test]
fn archives_are_ignored_unless_asked() {
    let (_tmp, root) = archives_root(Format::Zip, "a.zip", None);
    let (hits, _) = run(&Query {
        name: "*.txt".into(),
        ..query(&root)
    });
    assert!(hits.is_empty());
}

#[test]
fn names_are_found_inside_every_archive_format() {
    for (format, name) in [
        (Format::Zip, "a.zip"),
        (Format::Tar, "a.tar"),
        (Format::TarGz, "a.tar.gz"),
        (Format::SevenZ, "a.7z"),
    ] {
        let (_tmp, root) = archives_root(format, name, None);
        let (hits, summary) = run(&Query {
            name: "*.txt".into(),
            archives: true,
            ..query(&root)
        });
        assert_eq!(
            names(&hits, &root),
            [format!("{name} > docs/guide.txt")],
            "{name}"
        );
        assert_eq!(summary.archives, 1);
    }
}

#[test]
fn text_is_found_inside_every_archive_format() {
    for (format, name) in [
        (Format::Zip, "a.zip"),
        (Format::Tar, "a.tar"),
        (Format::TarGz, "a.tar.gz"),
        (Format::SevenZ, "a.7z"),
    ] {
        let (_tmp, root) = archives_root(format, name, None);
        let (hits, _) = run(&Query {
            text: Some(TextQuery::new("needle")),
            archives: true,
            ..query(&root)
        });
        assert_eq!(
            names(&hits, &root),
            [
                format!("{name} > code.rs"),
                format!("{name} > docs/guide.txt")
            ],
            "{name}"
        );
        let guide = hits
            .iter()
            .find(|h| h.location.name() == "guide.txt")
            .unwrap();
        assert_eq!(guide.lines[0].line, 1);
        assert!(guide.location.display().contains(" > docs/guide.txt"));
    }
}

#[test]
fn archives_next_to_loose_files_are_searched_together() {
    let (_tmp, root) = archives_root(Format::Zip, "a.zip", None);
    write(root.join("loose.txt"), "a needle outside\n");
    let (hits, _) = run(&Query {
        text: Some(TextQuery::new("needle")),
        archives: true,
        ..query(&root)
    });
    assert_eq!(
        names(&hits, &root),
        ["a.zip > code.rs", "a.zip > docs/guide.txt", "loose.txt"]
    );
}

#[test]
fn encrypted_zip_needs_the_password() {
    let (_tmp, root) = archives_root(Format::Zip, "p.zip", Some("secret"));
    let q = Query {
        text: Some(TextQuery::new("needle")),
        archives: true,
        ..query(&root)
    };
    let (hits, summary) = run(&q);
    assert!(hits.is_empty());
    assert_eq!(summary.encrypted, 3, "reported, not silently skipped");

    let (hits, summary) = run(&Query {
        archive_password: Some("secret".into()),
        ..q
    });
    assert_eq!(hits.len(), 2);
    assert_eq!(summary.encrypted, 0);
}

#[test]
fn names_in_an_encrypted_zip_are_searchable_without_the_password() {
    let (_tmp, root) = archives_root(Format::Zip, "p.zip", Some("secret"));
    let (hits, _) = run(&Query {
        name: "*.txt".into(),
        archives: true,
        ..query(&root)
    });
    assert_eq!(names(&hits, &root), ["p.zip > docs/guide.txt"]);
}

#[test]
fn encrypted_7z_is_reported() {
    let (_tmp, root) = archives_root(Format::SevenZ, "p.7z", Some("secret"));
    let q = Query {
        text: Some(TextQuery::new("needle")),
        archives: true,
        ..query(&root)
    };
    let (hits, summary) = run(&q);
    assert!(hits.is_empty());
    assert_eq!(summary.encrypted, 1);
    let (hits, _) = run(&Query {
        archive_password: Some("secret".into()),
        ..q
    });
    assert_eq!(hits.len(), 2);
}

#[test]
fn a_broken_archive_is_counted_and_the_rest_is_still_searched() {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path().join("bad.zip"), "this is not a zip");
    write(tmp.path().join("ok.txt"), "needle\n");
    let (hits, summary) = run(&Query {
        text: Some(TextQuery::new("needle")),
        archives: true,
        ..query(tmp.path())
    });
    assert_eq!(names(&hits, tmp.path()), ["ok.txt"]);
    assert_eq!(summary.unreadable, 1);
}
