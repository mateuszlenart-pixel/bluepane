//! Changing existing archives: add, remove, rename, new directory, for every format.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use bluepane_archive::{
    create, extract, list, modify, ArchiveError, Change, CreateOptions, Format, Level, Listing,
    Selection,
};
use bluepane_ops::{Conflict, FailOnConflict, Host, OpError, Progress, Resolution};

const FORMATS: [(Format, &str); 4] = [
    (Format::Zip, "m.zip"),
    (Format::Tar, "m.tar"),
    (Format::TarGz, "m.tar.gz"),
    (Format::SevenZ, "m.7z"),
];

#[derive(Default)]
struct Answers(Vec<Resolution>);

impl Host for Answers {
    fn conflict(&mut self, _conflict: &Conflict) -> Result<Resolution, OpError> {
        if self.0.is_empty() {
            Err(OpError::Cancelled)
        } else {
            Ok(self.0.remove(0))
        }
    }
}

fn write(path: &Path, content: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn options(format: Format, password: Option<&str>, split: Option<u64>) -> CreateOptions {
    CreateOptions {
        format,
        level: Level::Normal,
        password: password.map(str::to_string),
        split,
        encrypt_names: false,
    }
}

/// `src/docs/{a.txt, sub/b.txt}`, `src/top.txt`, packed into `out`.
fn archive(root: &Path, out: &str, opts: &CreateOptions) -> PathBuf {
    let src = root.join("src");
    write(&src.join("docs/a.txt"), "alpha");
    write(&src.join("docs/sub/b.txt"), "bravo");
    write(&src.join("top.txt"), "top");
    let path = root.join(out);
    create(
        &[src.join("docs"), src.join("top.txt")],
        &path,
        opts,
        &Progress::new(),
        &mut FailOnConflict,
    )
    .unwrap();
    path
}

fn relist(path: &Path, password: Option<&str>) -> Listing {
    list(path, password, &Progress::new()).unwrap()
}

fn paths(listing: &Listing) -> Vec<String> {
    listing.entries.iter().map(|e| e.path.clone()).collect()
}

fn apply(
    path: &Path,
    change: &Change,
    password: Option<&str>,
    host: &mut dyn Host,
) -> Result<bluepane_archive::ModifyReport, ArchiveError> {
    let existing = relist(path, password);
    modify(path, &existing, change, password, &Progress::new(), host)
}

fn read_from(path: &Path, inner: &str, password: Option<&str>, dir: &Path) -> String {
    let _ = fs::remove_dir_all(dir);
    extract(
        path,
        &Selection {
            strip: "",
            entries: &[inner.to_string()],
        },
        dir,
        password,
        &Progress::new(),
        &mut FailOnConflict,
    )
    .unwrap();
    fs::read_to_string(dir.join(inner)).unwrap()
}

fn no_leftovers(root: &Path, keep: &[&str]) {
    let names: Vec<String> = fs::read_dir(root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !keep.contains(&n.as_str()))
        .collect();
    assert!(names.is_empty(), "unexpected files: {names:?}");
}

#[test]
fn removing_files_and_directories() {
    for (format, name) in FORMATS {
        let tmp = tempfile::tempdir().unwrap();
        let path = archive(tmp.path(), name, &options(format, None, None));
        let report = apply(
            &path,
            &Change::Remove {
                entries: vec!["top.txt".into(), "docs/sub".into()],
            },
            None,
            &mut FailOnConflict,
        )
        .unwrap();
        assert_eq!(
            report.removed, 3,
            "{format:?}: top.txt, docs/sub and its file"
        );
        let listing = relist(&path, None);
        assert_eq!(paths(&listing), ["docs", "docs/a.txt"], "{format:?}");
        assert_eq!(
            read_from(&path, "docs/a.txt", None, &tmp.path().join("o")),
            "alpha"
        );
        no_leftovers(tmp.path(), &["src", "o", name]);
    }
}

#[test]
fn renaming_a_file_and_a_directory_with_everything_below_it() {
    for (format, name) in FORMATS {
        let tmp = tempfile::tempdir().unwrap();
        let path = archive(tmp.path(), name, &options(format, None, None));
        let report = apply(
            &path,
            &Change::Rename {
                from: "docs".into(),
                to: "papers".into(),
            },
            None,
            &mut FailOnConflict,
        )
        .unwrap();
        assert_eq!(report.renamed, 4, "{format:?}: docs, a.txt, sub, b.txt");
        assert_eq!(
            paths(&relist(&path, None)),
            [
                "papers",
                "papers/a.txt",
                "papers/sub",
                "papers/sub/b.txt",
                "top.txt"
            ],
            "{format:?}"
        );
        assert_eq!(
            read_from(&path, "papers/sub/b.txt", None, &tmp.path().join("o")),
            "bravo"
        );
        apply(
            &path,
            &Change::Rename {
                from: "top.txt".into(),
                to: "papers/moved.txt".into(),
            },
            None,
            &mut FailOnConflict,
        )
        .unwrap();
        assert_eq!(
            read_from(&path, "papers/moved.txt", None, &tmp.path().join("o2")),
            "top"
        );
    }
}

#[test]
fn renames_that_make_no_sense_are_refused_and_change_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let path = archive(tmp.path(), "r.zip", &options(Format::Zip, None, None));
    let before = fs::read(&path).unwrap();
    for (from, to) in [
        ("docs", "top.txt"),     // already exists
        ("docs", "docs/inside"), // into itself
        ("nope", "x"),           // missing
        ("docs", "../escape"),   // unsafe
    ] {
        let err = apply(
            &path,
            &Change::Rename {
                from: from.into(),
                to: to.into(),
            },
            None,
            &mut FailOnConflict,
        )
        .unwrap_err();
        assert!(matches!(err, ArchiveError::Op(_)), "{from} -> {to}: {err}");
    }
    assert_eq!(fs::read(&path).unwrap(), before, "the archive is untouched");
    no_leftovers(tmp.path(), &["src", "r.zip"]);
}

#[test]
fn a_new_directory_appears_in_every_format() {
    for (format, name) in FORMATS {
        let tmp = tempfile::tempdir().unwrap();
        let path = archive(tmp.path(), name, &options(format, None, None));
        apply(
            &path,
            &Change::NewDir {
                path: "docs/new".into(),
            },
            None,
            &mut FailOnConflict,
        )
        .unwrap();
        let listing = relist(&path, None);
        assert!(listing.find("docs/new").unwrap().is_dir, "{format:?}");
        let err = apply(
            &path,
            &Change::NewDir {
                path: "docs".into(),
            },
            None,
            &mut FailOnConflict,
        )
        .unwrap_err();
        assert!(matches!(err, ArchiveError::Op(_)));
    }
}

#[test]
fn adding_files_and_a_directory_into_a_subdirectory() {
    for (format, name) in FORMATS {
        let tmp = tempfile::tempdir().unwrap();
        let path = archive(tmp.path(), name, &options(format, None, None));
        write(&tmp.path().join("new/one.txt"), "one");
        write(&tmp.path().join("new/tree/two.txt"), "two");
        let report = apply(
            &path,
            &Change::Add {
                sources: vec![tmp.path().join("new/one.txt"), tmp.path().join("new/tree")],
                into: "docs".into(),
            },
            None,
            &mut FailOnConflict,
        )
        .unwrap();
        assert_eq!(report.added, 3, "{format:?}: one.txt, tree, tree/two.txt");
        let listing = relist(&path, None);
        for expected in [
            "docs/one.txt",
            "docs/tree",
            "docs/tree/two.txt",
            "docs/a.txt",
            "top.txt",
        ] {
            assert!(listing.find(expected).is_some(), "{format:?}: {expected}");
        }
        assert_eq!(
            read_from(&path, "docs/tree/two.txt", None, &tmp.path().join("o")),
            "two"
        );
        assert_eq!(
            read_from(&path, "docs/a.txt", None, &tmp.path().join("o2")),
            "alpha"
        );
        no_leftovers(tmp.path(), &["src", "new", "o", "o2", name]);
    }
}

#[test]
fn adding_to_the_top_level_and_the_archive_is_never_packed_into_itself() {
    let tmp = tempfile::tempdir().unwrap();
    let path = archive(tmp.path(), "self.zip", &options(Format::Zip, None, None));
    // The archive lives in the directory being added.
    write(&tmp.path().join("box/inside.txt"), "x");
    let beside = tmp.path().join("box/self.zip");
    fs::copy(&path, &beside).unwrap();
    apply(
        &beside,
        &Change::Add {
            sources: vec![tmp.path().join("box")],
            into: String::new(),
        },
        None,
        &mut FailOnConflict,
    )
    .unwrap();
    let listing = relist(&beside, None);
    assert!(listing.find("box/inside.txt").is_some());
    assert!(
        listing.find("box/self.zip").is_none(),
        "the archive does not contain itself"
    );
}

#[test]
fn name_clashes_when_adding_go_through_the_host() {
    for (format, name) in FORMATS {
        let tmp = tempfile::tempdir().unwrap();
        let path = archive(tmp.path(), name, &options(format, None, None));
        write(&tmp.path().join("new/top.txt"), "REPLACED");
        let add = Change::Add {
            sources: vec![tmp.path().join("new/top.txt")],
            into: String::new(),
        };
        // Skip: nothing changes.
        let report = apply(&path, &add, None, &mut Answers(vec![Resolution::Skip])).unwrap();
        assert_eq!((report.added, report.skipped), (0, 1), "{format:?}");
        assert_eq!(
            read_from(&path, "top.txt", None, &tmp.path().join("o1")),
            "top"
        );
        // Keep both.
        apply(&path, &add, None, &mut Answers(vec![Resolution::KeepBoth])).unwrap();
        assert_eq!(
            read_from(&path, "top.txt", None, &tmp.path().join("o2")),
            "top"
        );
        assert_eq!(
            read_from(&path, "top (kopia).txt", None, &tmp.path().join("o3")),
            "REPLACED"
        );
        // Overwrite: the old one goes, the new one stays.
        apply(&path, &add, None, &mut Answers(vec![Resolution::Overwrite])).unwrap();
        assert_eq!(
            read_from(&path, "top.txt", None, &tmp.path().join("o4")),
            "REPLACED"
        );
        let listing = relist(&path, None);
        assert_eq!(
            listing
                .entries
                .iter()
                .filter(|e| e.path == "top.txt")
                .count(),
            1,
            "{format:?}: no duplicate entry"
        );
        // No answer means cancelled: the archive stays as it is.
        let before = fs::read(&path).unwrap();
        assert!(matches!(
            apply(&path, &add, None, &mut Answers(vec![])),
            Err(ArchiveError::Cancelled)
        ));
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[test]
fn overwrite_older_replaces_only_a_newer_file() {
    let tmp = tempfile::tempdir().unwrap();
    let path = archive(tmp.path(), "n.zip", &options(Format::Zip, None, None));
    write(&tmp.path().join("new/top.txt"), "NEWER");
    let set = |p: &Path, secs: u64| {
        fs::File::options()
            .write(true)
            .open(p)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(secs))
            .unwrap();
    };
    let add = Change::Add {
        sources: vec![tmp.path().join("new/top.txt")],
        into: String::new(),
    };
    set(&tmp.path().join("new/top.txt"), 1_000_000_000); // older than the archived file
    apply(
        &path,
        &add,
        None,
        &mut Answers(vec![Resolution::OverwriteOlder]),
    )
    .unwrap();
    assert_eq!(
        read_from(&path, "top.txt", None, &tmp.path().join("o1")),
        "top"
    );
    set(&tmp.path().join("new/top.txt"), 2_000_000_000); // newer
    apply(
        &path,
        &add,
        None,
        &mut Answers(vec![Resolution::OverwriteOlder]),
    )
    .unwrap();
    assert_eq!(
        read_from(&path, "top.txt", None, &tmp.path().join("o2")),
        "NEWER"
    );
}

#[test]
fn a_changed_archive_is_refused_with_a_stale_listing() {
    let tmp = tempfile::tempdir().unwrap();
    let path = archive(tmp.path(), "s.zip", &options(Format::Zip, None, None));
    let stale = relist(&path, None);
    apply(
        &path,
        &Change::NewDir { path: "x".into() },
        None,
        &mut FailOnConflict,
    )
    .unwrap();
    let err = modify(
        &path,
        &stale,
        &Change::NewDir { path: "y".into() },
        None,
        &Progress::new(),
        &mut FailOnConflict,
    )
    .unwrap_err();
    assert!(err.to_string().contains("zmieniło się"), "{err}");
}

#[test]
fn zip_removal_and_rename_need_no_password_but_adding_does() {
    let tmp = tempfile::tempdir().unwrap();
    let path = archive(tmp.path(), "p.zip", &options(Format::Zip, Some("pw"), None));
    // Raw copy: encrypted entries move without being decrypted.
    apply(
        &path,
        &Change::Rename {
            from: "top.txt".into(),
            to: "renamed.txt".into(),
        },
        None,
        &mut FailOnConflict,
    )
    .unwrap();
    apply(
        &path,
        &Change::Remove {
            entries: vec!["docs/sub".into()],
        },
        None,
        &mut FailOnConflict,
    )
    .unwrap();
    assert_eq!(
        read_from(&path, "renamed.txt", Some("pw"), &tmp.path().join("o")),
        "top"
    );

    write(&tmp.path().join("new/n.txt"), "fresh");
    let add = Change::Add {
        sources: vec![tmp.path().join("new/n.txt")],
        into: String::new(),
    };
    assert!(matches!(
        apply(&path, &add, None, &mut FailOnConflict),
        Err(ArchiveError::NeedsPassword)
    ));
    assert!(matches!(
        apply(&path, &add, Some("wrong"), &mut FailOnConflict),
        Err(ArchiveError::WrongPassword)
    ));
    apply(&path, &add, Some("pw"), &mut FailOnConflict).unwrap();
    let listing = relist(&path, None);
    assert!(
        listing.find("n.txt").unwrap().encrypted,
        "new files are encrypted too"
    );
    assert_eq!(
        read_from(&path, "n.txt", Some("pw"), &tmp.path().join("o2")),
        "fresh"
    );
    assert_eq!(
        read_from(&path, "docs/a.txt", Some("pw"), &tmp.path().join("o3")),
        "alpha"
    );
}

#[test]
fn seven_z_modification_keeps_the_password_and_hidden_names() {
    let tmp = tempfile::tempdir().unwrap();
    let hidden = CreateOptions {
        encrypt_names: true,
        ..options(Format::SevenZ, Some("pw"), None)
    };
    let path = archive(tmp.path(), "h.7z", &hidden);
    write(&tmp.path().join("new/n.txt"), "fresh");
    assert!(matches!(
        apply_with_listing(&path, "pw", None),
        Err(ArchiveError::NeedsPassword)
    ));
    assert!(matches!(
        apply_with_listing(&path, "pw", Some("wrong")),
        Err(ArchiveError::WrongPassword)
    ));
    apply_with_listing(&path, "pw", Some("pw")).unwrap();
    // The names are still hidden after the rewrite...
    assert!(matches!(
        list(&path, None, &Progress::new()),
        Err(ArchiveError::NeedsPassword)
    ));
    // ...and the content is readable with the password.
    let listing = relist(&path, Some("pw"));
    assert!(listing.names_encrypted);
    assert!(listing.find("n.txt").is_some());
    assert_eq!(
        read_from(&path, "docs/a.txt", Some("pw"), &tmp.path().join("o")),
        "alpha"
    );
    assert_eq!(
        read_from(&path, "n.txt", Some("pw"), &tmp.path().join("o2")),
        "fresh"
    );

    fn apply_with_listing(
        path: &Path,
        list_password: &str,
        password: Option<&str>,
    ) -> Result<bluepane_archive::ModifyReport, ArchiveError> {
        let existing = list(path, Some(list_password), &Progress::new()).unwrap();
        let root = path.parent().unwrap();
        modify(
            path,
            &existing,
            &Change::Add {
                sources: vec![root.join("new/n.txt")],
                into: String::new(),
            },
            password,
            &Progress::new(),
            &mut FailOnConflict,
        )
    }
}

#[test]
fn split_archives_stay_split_with_volumes_of_the_same_size() {
    for (format, name) in [
        (Format::Zip, "v.zip"),
        (Format::TarGz, "v.tar.gz"),
        (Format::SevenZ, "v.7z"),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        // Incompressible bytes: the rewrite uses normal compression, and the volume
        // count must not shrink because the data happens to compress well.
        let mut x = 0x2545_f491_u32;
        let big: Vec<u8> = (0..200_000)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 11) as u8
            })
            .collect();
        write(&tmp.path().join("src/big.bin"), &big);
        let opts = CreateOptions {
            level: Level::Store,
            ..options(format, None, Some(60_000))
        };
        let first = tmp.path().join(format!("{name}.001"));
        create(
            &[tmp.path().join("src/big.bin")],
            &tmp.path().join(name),
            &opts,
            &Progress::new(),
            &mut FailOnConflict,
        )
        .unwrap();
        let volumes_before = fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{name}.0"))
            })
            .count();
        assert!(volumes_before >= 3, "{format:?}");

        write(&tmp.path().join("new/small.txt"), "small");
        apply(
            &first,
            &Change::Add {
                sources: vec![tmp.path().join("new/small.txt")],
                into: String::new(),
            },
            None,
            &mut FailOnConflict,
        )
        .unwrap();
        let listing = relist(&first, None);
        assert!(listing.find("small.txt").is_some() && listing.find("big.bin").is_some());
        assert!(listing.volumes >= 3, "{format:?}");
        let sizes: Vec<u64> = (1..listing.volumes)
            .map(|n| {
                fs::metadata(tmp.path().join(format!("{name}.{n:03}")))
                    .unwrap()
                    .len()
            })
            .collect();
        assert!(sizes.iter().all(|&s| s == 60_000), "{format:?}: {sizes:?}");
        let out = tmp.path().join("o");
        let _ = fs::remove_dir_all(&out);
        extract(
            &first,
            &Selection {
                strip: "",
                entries: &["big.bin".to_string()],
            },
            &out,
            None,
            &Progress::new(),
            &mut FailOnConflict,
        )
        .unwrap();
        assert_eq!(
            fs::read(out.join("big.bin")).unwrap(),
            big,
            "{format:?}: content intact"
        );
        // Removing everything but one entry may shrink the volume count; the name stays.
        apply(
            &first,
            &Change::Remove {
                entries: vec!["big.bin".into()],
            },
            None,
            &mut FailOnConflict,
        )
        .unwrap();
        assert!(first.exists(), "{format:?}: still named .001");
        assert!(relist(&first, None).find("big.bin").is_none());
    }
}

#[test]
fn cancelling_leaves_the_original_and_no_temporary_file() {
    for (format, name) in [(Format::Zip, "c.zip"), (Format::SevenZ, "c.7z")] {
        let tmp = tempfile::tempdir().unwrap();
        let path = archive(tmp.path(), name, &options(format, None, None));
        let before = fs::read(&path).unwrap();
        let existing = relist(&path, None);
        let progress = Progress::new();
        progress.cancel();
        let err = modify(
            &path,
            &existing,
            &Change::Remove {
                entries: vec!["top.txt".into()],
            },
            None,
            &progress,
            &mut FailOnConflict,
        )
        .unwrap_err();
        assert!(matches!(err, ArchiveError::Cancelled), "{format:?}: {err}");
        assert_eq!(fs::read(&path).unwrap(), before);
        no_leftovers(tmp.path(), &["src", name]);
    }
}

#[test]
fn rewriting_tar_and_7z_refuses_archives_with_links() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("l.tar");
    {
        let mut builder = tar::Builder::new(fs::File::create(&path).unwrap());
        let mut header = tar::Header::new_gnu();
        header.set_size(2);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::Regular);
        builder
            .append_data(&mut header, "f.txt", &b"hi"[..])
            .unwrap();
        let mut link = tar::Header::new_gnu();
        link.set_entry_type(tar::EntryType::Symlink);
        link.set_size(0);
        builder.append_link(&mut link, "ln", "f.txt").unwrap();
        builder.finish().unwrap();
    }
    let before = fs::read(&path).unwrap();
    let err = apply(
        &path,
        &Change::NewDir { path: "d".into() },
        None,
        &mut FailOnConflict,
    )
    .unwrap_err();
    assert!(matches!(err, ArchiveError::Unsupported(_)), "{err}");
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn modification_progress_counts_the_data_it_moves() {
    let tmp = tempfile::tempdir().unwrap();
    let path = archive(tmp.path(), "g.tar.gz", &options(Format::TarGz, None, None));
    let existing = relist(&path, None);
    let progress = Progress::new();
    modify(
        &path,
        &existing,
        &Change::Remove {
            entries: vec!["top.txt".into()],
        },
        None,
        &progress,
        &mut FailOnConflict,
    )
    .unwrap();
    // docs/a.txt (5) + docs/sub/b.txt (5) were carried over.
    assert_eq!(progress.bytes_total(), 10);
    assert_eq!(progress.bytes_done(), 10);
}
