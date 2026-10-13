//! End-to-end tests of the archive crate on real files in temporary directories.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use bluepane_archive::{
    create, detect, extract, list, verify, ArchiveError, CreateOptions, Format, Level, Selection,
};
use bluepane_ops::{Conflict, FailOnConflict, Host, OpError, Progress, Resolution};

/// Answers conflicts from a list; with no answers left it cancels.
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

fn opts(format: Format) -> CreateOptions {
    CreateOptions {
        format,
        level: Level::Normal,
        password: None,
        split: None,
        encrypt_names: false,
    }
}

fn write(path: &Path, content: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// `src/photos/{a.txt, sub/big.bin (3 MB), zażółć.txt, empty/}` and `src/note.txt`.
fn sample(root: &Path) -> Vec<PathBuf> {
    let photos = root.join("src/photos");
    write(&photos.join("a.txt"), "alpha");
    let big: Vec<u8> = (0..3_000_000u32).map(|i| (i % 253) as u8).collect();
    write(&photos.join("sub/big.bin"), &big);
    write(&photos.join("zażółć.txt"), "unicode name");
    fs::create_dir_all(photos.join("empty")).unwrap();
    write(&root.join("src/note.txt"), "a note");
    vec![photos, root.join("src/note.txt")]
}

fn make(
    root: &Path,
    name: &str,
    options: &CreateOptions,
) -> Result<bluepane_archive::CreateReport, ArchiveError> {
    create(
        &sample(root),
        &root.join(name),
        options,
        &Progress::new(),
        &mut FailOnConflict,
    )
}

fn unpack(archive: &Path, dest: &Path, password: Option<&str>) -> Result<(), ArchiveError> {
    extract(
        archive,
        &Selection {
            strip: "",
            entries: &[],
        },
        dest,
        password,
        &Progress::new(),
        &mut FailOnConflict,
    )
    .map(|_| ())
}

fn assert_sample_extracted(dest: &Path) {
    assert_eq!(
        fs::read_to_string(dest.join("photos/a.txt")).unwrap(),
        "alpha"
    );
    assert_eq!(
        fs::read_to_string(dest.join("photos/zażółć.txt")).unwrap(),
        "unicode name"
    );
    assert_eq!(fs::read_to_string(dest.join("note.txt")).unwrap(), "a note");
    let big = fs::read(dest.join("photos/sub/big.bin")).unwrap();
    assert_eq!(big.len(), 3_000_000);
    assert!(big.iter().enumerate().all(|(i, &b)| b == (i % 253) as u8));
    assert!(dest.join("photos/empty").is_dir());
}

#[test]
fn every_format_round_trips() {
    for (format, name) in [
        (Format::Zip, "out.zip"),
        (Format::Tar, "out.tar"),
        (Format::TarGz, "out.tar.gz"),
        (Format::SevenZ, "out.7z"),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let report = make(tmp.path(), name, &opts(format)).unwrap();
        assert_eq!(report.volumes, [tmp.path().join(name)]);
        assert_eq!(report.files, 4);
        assert!(!tmp.path().join(format!(".{name}.bluepane-tmp")).exists());

        let listing = list(&tmp.path().join(name), None, &Progress::new()).unwrap();
        assert_eq!(listing.format, format);
        let paths: Vec<_> = listing.entries.iter().map(|e| e.path.as_str()).collect();
        for expected in [
            "photos",
            "photos/a.txt",
            "photos/empty",
            "photos/sub",
            "photos/sub/big.bin",
            "photos/zażółć.txt",
            "note.txt",
        ] {
            assert!(
                paths.contains(&expected),
                "{format:?}: missing {expected} in {paths:?}"
            );
        }
        assert_eq!(listing.find("photos/sub/big.bin").unwrap().size, 3_000_000);
        assert!(listing.find("photos/empty").unwrap().is_dir);
        assert!(!listing.has_encrypted());

        let dest = tmp.path().join("out");
        unpack(&tmp.path().join(name), &dest, None).unwrap();
        assert_sample_extracted(&dest);
    }
}

#[test]
fn modification_times_survive_a_round_trip() {
    let stamp = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    for (format, name) in [
        (Format::Zip, "t.zip"),
        (Format::TarGz, "t.tar.gz"),
        (Format::SevenZ, "t.7z"),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("f.txt");
        write(&src, "x");
        fs::File::options()
            .write(true)
            .open(&src)
            .unwrap()
            .set_modified(stamp)
            .unwrap();
        create(
            &[src],
            &tmp.path().join(name),
            &opts(format),
            &Progress::new(),
            &mut FailOnConflict,
        )
        .unwrap();
        let dest = tmp.path().join("out");
        unpack(&tmp.path().join(name), &dest, None).unwrap();
        let got = fs::metadata(dest.join("f.txt"))
            .unwrap()
            .modified()
            .unwrap();
        let diff = got
            .duration_since(stamp)
            .or_else(|_| stamp.duration_since(got));
        assert!(diff.unwrap() <= Duration::from_secs(2), "{format:?}");
    }
}

#[test]
fn compression_levels_differ_and_store_is_largest() {
    let tmp = tempfile::tempdir().unwrap();
    let text: String = (0..20_000)
        .map(|i| format!("line number {i} of the file\n"))
        .collect();
    write(&tmp.path().join("d/log.txt"), &text);
    let size_at = |level: Level, name: &str| {
        let options = CreateOptions {
            level,
            ..opts(Format::TarGz)
        };
        create(
            &[tmp.path().join("d")],
            &tmp.path().join(name),
            &options,
            &Progress::new(),
            &mut FailOnConflict,
        )
        .unwrap();
        fs::metadata(tmp.path().join(name)).unwrap().len()
    };
    let store = size_at(Level::Store, "s.tar.gz");
    let fast = size_at(Level::Fast, "f.tar.gz");
    let max = size_at(Level::Maximum, "m.tar.gz");
    assert!(store > text.len() as u64, "store keeps the full size");
    assert!(fast < store / 2 && max <= fast, "{store} {fast} {max}");
}

#[test]
fn zip_passwords_protect_content_but_not_names() {
    let tmp = tempfile::tempdir().unwrap();
    let options = CreateOptions {
        password: Some("sekret".into()),
        ..opts(Format::Zip)
    };
    make(tmp.path(), "p.zip", &options).unwrap();
    let archive = tmp.path().join("p.zip");

    // The table of contents needs no password, and shows which files are encrypted.
    let listing = list(&archive, None, &Progress::new()).unwrap();
    assert!(listing.has_encrypted());
    assert!(listing.find("photos/a.txt").unwrap().encrypted);
    assert!(!listing.find("photos").unwrap().encrypted);

    let dest = tmp.path().join("out");
    assert!(matches!(
        unpack(&archive, &dest, None),
        Err(ArchiveError::NeedsPassword)
    ));
    assert!(matches!(
        unpack(&archive, &dest, Some("wrong")),
        Err(ArchiveError::WrongPassword)
    ));
    // A missing or wrong password is detected before anything is written.
    assert!(!dest.exists(), "nothing may be created by a failed attempt");

    unpack(&archive, &dest, Some("sekret")).unwrap();
    assert_sample_extracted(&dest);
}

#[test]
fn password_verification_reports_missing_and_wrong_passwords() {
    let tmp = tempfile::tempdir().unwrap();
    let options = CreateOptions {
        password: Some("pw".into()),
        ..opts(Format::Zip)
    };
    make(tmp.path(), "p.zip", &options).unwrap();
    let archive = tmp.path().join("p.zip");
    assert!(matches!(
        verify(&archive, None, &Progress::new()),
        Err(ArchiveError::NeedsPassword)
    ));
    assert!(matches!(
        verify(&archive, Some("nope"), &Progress::new()),
        Err(ArchiveError::WrongPassword)
    ));
    let ok = verify(&archive, Some("pw"), &Progress::new()).unwrap();
    assert!(ok.damaged.is_empty());
    assert_eq!(ok.files, 4);
    assert_eq!(ok.bytes, 5 + 3_000_000 + 12 + 6);
}

#[test]
fn an_empty_password_means_no_encryption() {
    let tmp = tempfile::tempdir().unwrap();
    let options = CreateOptions {
        password: Some(String::new()),
        ..opts(Format::Zip)
    };
    make(tmp.path(), "p.zip", &options).unwrap();
    assert!(!list(&tmp.path().join("p.zip"), None, &Progress::new())
        .unwrap()
        .has_encrypted());
}

#[test]
fn tar_cannot_be_password_protected() {
    let tmp = tempfile::tempdir().unwrap();
    for format in [Format::Tar, Format::TarGz] {
        let options = CreateOptions {
            password: Some("pw".into()),
            ..opts(format)
        };
        let err = make(tmp.path(), "x.tar", &options).unwrap_err();
        assert!(matches!(err, ArchiveError::Unsupported(_)), "{err}");
    }
    assert!(!tmp.path().join("x.tar").exists());
}

#[test]
fn split_archives_are_cut_into_volumes_and_read_back_from_the_first() {
    for (format, name) in [
        (Format::Zip, "s.zip"),
        (Format::TarGz, "s.tar.gz"),
        (Format::SevenZ, "s.7z"),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let options = CreateOptions {
            split: Some(100_000),
            level: Level::Store,
            ..opts(format)
        };
        let report = make(tmp.path(), name, &options).unwrap();
        assert!(
            report.volumes.len() > 20,
            "{} volumes",
            report.volumes.len()
        );
        for (i, volume) in report.volumes.iter().enumerate() {
            let expected = format!("{name}.{:03}", i + 1);
            assert_eq!(volume.file_name().unwrap().to_string_lossy(), expected);
            let size = fs::metadata(volume).unwrap().len();
            assert!(size <= 100_000);
            if i + 1 < report.volumes.len() {
                assert_eq!(size, 100_000, "all but the last volume are full");
            }
        }
        assert!(!tmp.path().join(name).exists(), "no unsplit copy is left");

        let first = tmp.path().join(format!("{name}.001"));
        assert_eq!(detect(&first), Some(format));
        assert_eq!(
            list(&first, None, &Progress::new()).unwrap().volumes,
            report.volumes.len()
        );
        let dest = tmp.path().join("out");
        unpack(&first, &dest, None).unwrap();
        assert_sample_extracted(&dest);
    }
}

#[test]
fn split_and_password_work_together() {
    let tmp = tempfile::tempdir().unwrap();
    let options = CreateOptions {
        split: Some(400_000),
        password: Some("both".into()),
        ..opts(Format::Zip)
    };
    make(tmp.path(), "b.zip", &options).unwrap();
    let first = tmp.path().join("b.zip.001");
    let dest = tmp.path().join("out");
    assert!(matches!(
        unpack(&first, &dest, None),
        Err(ArchiveError::NeedsPassword)
    ));
    unpack(&first, &dest, Some("both")).unwrap();
    assert_sample_extracted(&dest);
}

#[test]
fn a_missing_volume_makes_the_archive_unreadable() {
    let tmp = tempfile::tempdir().unwrap();
    let options = CreateOptions {
        split: Some(300_000),
        level: Level::Store,
        ..opts(Format::Zip)
    };
    make(tmp.path(), "m.zip", &options).unwrap();
    fs::remove_file(tmp.path().join("m.zip.003")).unwrap();
    // The numbering now breaks after volume 2, so the end of the archive is missing.
    let err = list(&tmp.path().join("m.zip.001"), None, &Progress::new()).unwrap_err();
    assert!(
        matches!(err, ArchiveError::Corrupt(_) | ArchiveError::Io(_)),
        "{err}"
    );
}

#[test]
fn an_existing_archive_is_handled_through_the_host() {
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("c.zip");
    fs::write(&out, "precious").unwrap();
    let sources = sample(tmp.path());
    let run = |answers: Vec<Resolution>| {
        create(
            &sources,
            &out,
            &opts(Format::Zip),
            &Progress::new(),
            &mut Answers(answers),
        )
    };

    let skipped = run(vec![Resolution::Skip]).unwrap();
    assert!(skipped.skipped && skipped.volumes.is_empty());
    assert_eq!(fs::read_to_string(&out).unwrap(), "precious");

    assert!(matches!(run(vec![]), Err(ArchiveError::Cancelled)));
    assert_eq!(fs::read_to_string(&out).unwrap(), "precious");

    let both = run(vec![Resolution::KeepBoth]).unwrap();
    assert_eq!(both.volumes, [tmp.path().join("c (kopia).zip")]);
    assert_eq!(fs::read_to_string(&out).unwrap(), "precious");

    let replaced = run(vec![Resolution::Overwrite]).unwrap();
    assert_eq!(replaced.volumes, std::slice::from_ref(&out));
    assert!(list(&out, None, &Progress::new()).is_ok());
}

#[test]
fn overwriting_a_split_archive_removes_stale_volumes() {
    let tmp = tempfile::tempdir().unwrap();
    let many = CreateOptions {
        split: Some(100_000),
        level: Level::Store,
        ..opts(Format::Zip)
    };
    let first = make(tmp.path(), "o.zip", &many).unwrap();
    assert!(first.volumes.len() > 20);
    // A much smaller archive over it: only the new volumes may remain.
    let small = CreateOptions {
        split: Some(100_000),
        ..opts(Format::Zip)
    };
    let report = create(
        &[tmp.path().join("src/note.txt")],
        &tmp.path().join("o.zip"),
        &small,
        &Progress::new(),
        &mut Answers(vec![Resolution::Overwrite]),
    )
    .unwrap();
    assert_eq!(report.volumes.len(), 1);
    let remaining: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("o.zip"))
        .collect();
    assert_eq!(remaining, ["o.zip.001"]);
}

#[test]
fn cancelling_leaves_no_archive_and_no_temporary_file() {
    let tmp = tempfile::tempdir().unwrap();
    let progress = Progress::new();
    progress.cancel();
    let err = create(
        &sample(tmp.path()),
        &tmp.path().join("x.zip"),
        &opts(Format::Zip),
        &progress,
        &mut FailOnConflict,
    )
    .unwrap_err();
    assert!(matches!(err, ArchiveError::Cancelled));
    let names: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["src"], "{names:?}");
}

#[test]
fn cancelling_in_the_middle_of_a_big_file_cleans_up() {
    let tmp = tempfile::tempdir().unwrap();
    // Incompressible-ish data so maximum compression takes a while.
    let mut state = 12345u64;
    let data: Vec<u8> = (0..32 * 1024 * 1024)
        .map(|_| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) as u8
        })
        .collect();
    write(&tmp.path().join("src/big.bin"), &data);
    let progress = Progress::new();
    let canceller = {
        let progress = progress.clone();
        std::thread::spawn(move || {
            while progress.bytes_done() == 0 {
                std::thread::yield_now();
            }
            progress.cancel();
        })
    };
    let options = CreateOptions {
        level: Level::Maximum,
        ..opts(Format::TarGz)
    };
    let result = create(
        &[tmp.path().join("src/big.bin")],
        &tmp.path().join("big.tar.gz"),
        &options,
        &progress,
        &mut FailOnConflict,
    );
    canceller.join().unwrap();
    assert!(matches!(result, Err(ArchiveError::Cancelled)), "{result:?}");
    let leftovers: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != "src")
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn the_archive_is_never_packed_into_itself() {
    let tmp = tempfile::tempdir().unwrap();
    write(&tmp.path().join("d/a.txt"), "a");
    let out = tmp.path().join("d/self.zip");
    create(
        &[tmp.path().join("d")],
        &out,
        &opts(Format::Zip),
        &Progress::new(),
        &mut FailOnConflict,
    )
    .unwrap();
    // Packing again must not pick up the first archive's bytes either way.
    let listing = list(&out, None, &Progress::new()).unwrap();
    let files: Vec<_> = listing
        .entries
        .iter()
        .filter(|e| !e.is_dir)
        .map(|e| e.path.as_str())
        .collect();
    assert_eq!(files, ["d/a.txt"]);
}

#[test]
fn extracting_a_selection_roots_names_at_the_strip_directory() {
    let tmp = tempfile::tempdir().unwrap();
    make(tmp.path(), "r.zip", &opts(Format::Zip)).unwrap();
    let archive = tmp.path().join("r.zip");
    let pull = |strip: &str, entries: &[&str], dest: &str| {
        let entries: Vec<String> = entries.iter().map(|s| s.to_string()).collect();
        extract(
            &archive,
            &Selection {
                strip,
                entries: &entries,
            },
            &tmp.path().join(dest),
            None,
            &Progress::new(),
            &mut FailOnConflict,
        )
        .unwrap()
    };
    pull("", &["photos/a.txt"], "d1");
    assert!(
        tmp.path().join("d1/photos/a.txt").exists(),
        "a whole path below the root"
    );
    pull("photos", &["photos/a.txt", "photos/sub"], "d2");
    assert_eq!(
        fs::read_to_string(tmp.path().join("d2/a.txt")).unwrap(),
        "alpha"
    );
    assert!(tmp.path().join("d2/sub/big.bin").exists());
    assert!(
        !tmp.path().join("d2/zażółć.txt").exists(),
        "unselected files stay out"
    );
    assert!(!tmp.path().join("d2/photos").exists());
}

#[test]
fn extracting_over_existing_files_asks_the_host() {
    let tmp = tempfile::tempdir().unwrap();
    make(tmp.path(), "e.zip", &opts(Format::Zip)).unwrap();
    let dest = tmp.path().join("out");
    write(&dest.join("note.txt"), "mine");
    let report = extract(
        &tmp.path().join("e.zip"),
        &Selection {
            strip: "",
            entries: &["note.txt".to_string()],
        },
        &dest,
        None,
        &Progress::new(),
        &mut Answers(vec![Resolution::Skip]),
    )
    .unwrap();
    assert_eq!(report.outcome.skipped, 1);
    assert_eq!(fs::read_to_string(dest.join("note.txt")).unwrap(), "mine");
    let report = extract(
        &tmp.path().join("e.zip"),
        &Selection {
            strip: "",
            entries: &["note.txt".to_string()],
        },
        &dest,
        None,
        &Progress::new(),
        &mut Answers(vec![Resolution::Overwrite]),
    )
    .unwrap();
    assert_eq!(report.outcome.files, 1);
    assert_eq!(fs::read_to_string(dest.join("note.txt")).unwrap(), "a note");
}

#[test]
fn hostile_zip_names_never_write_outside_the_destination() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("evil.zip");
    {
        let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        for name in ["../evil.txt", "/abs.txt", "sub/../../evil2.txt", "ok.txt"] {
            zip.start_file(name, options).unwrap();
            zip.write_all(b"data").unwrap();
        }
        zip.finish().unwrap();
    }
    let listing = list(&archive, None, &Progress::new()).unwrap();
    assert_eq!(listing.unsafe_skipped, 3);
    assert_eq!(listing.entries.len(), 1);

    let dest = tmp.path().join("nested/out");
    unpack(&archive, &dest, None).unwrap();
    assert!(dest.join("ok.txt").exists());
    assert!(!tmp.path().join("evil.txt").exists());
    assert!(!tmp.path().join("nested/evil.txt").exists());
    assert!(!tmp.path().join("evil2.txt").exists());
    assert!(!Path::new("/abs.txt").exists());
}

/// A tar entry whose name field is written raw, so `..` gets past the builder's checks.
fn raw_tar_entry(out: &mut Vec<u8>, name: &str, data: &[u8]) {
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_entry_type(tar::EntryType::Regular);
    header.set_mode(0o644);
    {
        let bytes = header.as_mut_bytes();
        bytes[..100].fill(0);
        bytes[..name.len()].copy_from_slice(name.as_bytes());
    }
    header.set_cksum();
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(data);
    out.resize(out.len().div_ceil(512) * 512, 0);
}

#[test]
fn hostile_tar_names_never_write_outside_the_destination() {
    let tmp = tempfile::tempdir().unwrap();
    let mut bytes = Vec::new();
    raw_tar_entry(&mut bytes, "../evil.txt", b"data");
    raw_tar_entry(&mut bytes, "/abs.txt", b"data");
    raw_tar_entry(&mut bytes, "fine.txt", b"data");
    bytes.extend_from_slice(&[0u8; 1024]);
    let archive = tmp.path().join("evil.tar");
    fs::write(&archive, bytes).unwrap();
    let listing = list(&archive, None, &Progress::new()).unwrap();
    assert_eq!(listing.unsafe_skipped, 2);
    let dest = tmp.path().join("nested/out");
    unpack(&archive, &dest, None).unwrap();
    assert!(dest.join("fine.txt").exists());
    assert!(!tmp.path().join("evil.txt").exists());
    assert!(!tmp.path().join("nested/evil.txt").exists());
}

#[test]
fn links_in_a_tar_are_listed_but_not_extracted() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("links.tar");
    {
        let mut builder = tar::Builder::new(fs::File::create(&archive).unwrap());
        let mut header = tar::Header::new_gnu();
        header.set_size(2);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::Regular);
        builder
            .append_data(&mut header, "real.txt", &b"hi"[..])
            .unwrap();
        let mut link = tar::Header::new_gnu();
        link.set_entry_type(tar::EntryType::Symlink);
        link.set_size(0);
        builder
            .append_link(&mut link, "link", "/etc/passwd")
            .unwrap();
        builder.finish().unwrap();
    }
    let listing = list(&archive, None, &Progress::new()).unwrap();
    assert!(listing.find("link").unwrap().special);
    let dest = tmp.path().join("out");
    let report = extract(
        &archive,
        &Selection {
            strip: "",
            entries: &[],
        },
        &dest,
        None,
        &Progress::new(),
        &mut FailOnConflict,
    )
    .unwrap();
    assert_eq!(report.special_skipped, 1);
    assert!(dest.join("real.txt").exists());
    assert!(fs::symlink_metadata(dest.join("link")).is_err());
}

#[test]
fn long_names_round_trip_in_tar() {
    let tmp = tempfile::tempdir().unwrap();
    let long = "d".repeat(90);
    let rel = format!("{long}/{long}/file-with-a-rather-long-name.txt");
    write(
        &tmp.path()
            .join("src")
            .join(&long)
            .join(&long)
            .join("file-with-a-rather-long-name.txt"),
        "deep",
    );
    create(
        &[tmp.path().join("src").join(&long)],
        &tmp.path().join("l.tar"),
        &opts(Format::Tar),
        &Progress::new(),
        &mut FailOnConflict,
    )
    .unwrap();
    let dest = tmp.path().join("out");
    unpack(&tmp.path().join("l.tar"), &dest, None).unwrap();
    assert_eq!(fs::read_to_string(dest.join(rel)).unwrap(), "deep");
}

#[test]
fn damaged_archives_are_reported() {
    let tmp = tempfile::tempdir().unwrap();
    // A stored (uncompressed) zip: flipping a byte inside a file corrupts its checksum.
    let options = CreateOptions {
        level: Level::Store,
        ..opts(Format::Zip)
    };
    make(tmp.path(), "d.zip", &options).unwrap();
    let path = tmp.path().join("d.zip");
    let mut bytes = fs::read(&path).unwrap();
    let at = bytes.windows(5).position(|w| w == b"alpha").unwrap();
    bytes[at] ^= 0xff;
    fs::write(&path, bytes).unwrap();
    let report = verify(&path, None, &Progress::new()).unwrap();
    assert_eq!(report.damaged.len(), 1, "{:?}", report.damaged);
    assert!(report.damaged[0].contains("photos/a.txt"));
    assert_eq!(report.files, 3, "the other files still verify");

    // A truncated gzip is damaged too.
    make(tmp.path(), "d.tar.gz", &opts(Format::TarGz)).unwrap();
    let gz = tmp.path().join("d.tar.gz");
    let full = fs::read(&gz).unwrap();
    fs::write(&gz, &full[..full.len() / 2]).unwrap();
    let report = verify(&gz, None, &Progress::new()).unwrap();
    assert!(!report.damaged.is_empty());
}

#[test]
fn intact_archives_verify_clean() {
    let tmp = tempfile::tempdir().unwrap();
    for (format, name) in [
        (Format::Zip, "v.zip"),
        (Format::Tar, "v.tar"),
        (Format::TarGz, "v.tar.gz"),
    ] {
        make(tmp.path(), name, &opts(format)).unwrap();
        let report = verify(&tmp.path().join(name), None, &Progress::new()).unwrap();
        assert!(
            report.damaged.is_empty(),
            "{format:?}: {:?}",
            report.damaged
        );
        assert_eq!(report.files, 4);
    }
}

#[test]
fn unknown_and_broken_files_are_clear_errors() {
    let tmp = tempfile::tempdir().unwrap();
    write(&tmp.path().join("notes.txt"), "x");
    assert!(matches!(
        list(&tmp.path().join("notes.txt"), None, &Progress::new()),
        Err(ArchiveError::Unsupported(_))
    ));
    write(
        &tmp.path().join("fake.zip"),
        "this is not a zip file at all",
    );
    assert!(matches!(
        list(&tmp.path().join("fake.zip"), None, &Progress::new()),
        Err(ArchiveError::Corrupt(_))
    ));
    write(&tmp.path().join("fake.tar.gz"), "this is not gzip");
    assert!(list(&tmp.path().join("fake.tar.gz"), None, &Progress::new()).is_err());
    assert!(list(&tmp.path().join("missing.zip"), None, &Progress::new()).is_err());
}

#[test]
fn listing_progress_counts_the_archive_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    make(tmp.path(), "p.tar.gz", &opts(Format::TarGz)).unwrap();
    let progress = Progress::new();
    let listing = list(&tmp.path().join("p.tar.gz"), None, &progress).unwrap();
    assert_eq!(progress.bytes_total(), listing.packed_size);
    assert!(progress.bytes_done() > 0 && progress.bytes_done() <= listing.packed_size);
    assert_eq!(progress.current(), "p.tar.gz");
}

#[test]
fn measure_matches_what_extraction_writes() {
    let tmp = tempfile::tempdir().unwrap();
    make(tmp.path(), "m.zip", &opts(Format::Zip)).unwrap();
    let listing = list(&tmp.path().join("m.zip"), None, &Progress::new()).unwrap();
    assert_eq!(listing.measure(&[]), (5 + 3_000_000 + 12 + 6, 4));
    assert_eq!(listing.measure(&["photos/sub".to_string()]), (3_000_000, 1));
    let progress = Progress::new();
    extract(
        &tmp.path().join("m.zip"),
        &Selection {
            strip: "",
            entries: &[],
        },
        &tmp.path().join("out"),
        None,
        &progress,
        &mut FailOnConflict,
    )
    .unwrap();
    assert_eq!(progress.bytes_done(), 5 + 3_000_000 + 12 + 6);
    assert_eq!(progress.items_done(), 4);
}

// ---- 7z -----------------------------------------------------------------------------

fn seven_with_password(names_too: bool) -> CreateOptions {
    CreateOptions {
        password: Some("sekret".into()),
        encrypt_names: names_too,
        ..opts(Format::SevenZ)
    }
}

#[test]
fn seven_z_passwords_protect_content_and_can_hide_the_names() {
    let tmp = tempfile::tempdir().unwrap();
    make(tmp.path(), "visible.7z", &seven_with_password(false)).unwrap();
    make(tmp.path(), "hidden.7z", &seven_with_password(true)).unwrap();

    // Names left visible: the table of contents needs no password, the content does.
    let visible = tmp.path().join("visible.7z");
    let listing = list(&visible, None, &Progress::new()).unwrap();
    assert!(listing.has_encrypted());
    assert!(listing.find("photos/a.txt").unwrap().encrypted);
    let dest = tmp.path().join("out1");
    assert!(matches!(
        unpack(&visible, &dest, None),
        Err(ArchiveError::NeedsPassword)
    ));
    assert!(matches!(
        unpack(&visible, &dest, Some("wrong")),
        Err(ArchiveError::WrongPassword)
    ));
    assert!(!dest.exists(), "a failed attempt creates nothing");
    unpack(&visible, &dest, Some("sekret")).unwrap();
    assert_sample_extracted(&dest);

    // Names hidden: even listing needs the password.
    let hidden = tmp.path().join("hidden.7z");
    assert!(matches!(
        list(&hidden, None, &Progress::new()),
        Err(ArchiveError::NeedsPassword)
    ));
    assert!(list(&hidden, Some("wrong"), &Progress::new()).is_err());
    let listing = list(&hidden, Some("sekret"), &Progress::new()).unwrap();
    assert!(listing.find("photos/sub/big.bin").is_some());
    let dest = tmp.path().join("out2");
    assert!(matches!(
        unpack(&hidden, &dest, None),
        Err(ArchiveError::NeedsPassword)
    ));
    unpack(&hidden, &dest, Some("sekret")).unwrap();
    assert_sample_extracted(&dest);
}

#[test]
fn seven_z_options_that_make_no_sense_are_refused() {
    let tmp = tempfile::tempdir().unwrap();
    // Hiding names needs a password...
    let no_password = CreateOptions {
        encrypt_names: true,
        ..opts(Format::SevenZ)
    };
    assert!(matches!(
        make(tmp.path(), "a.7z", &no_password),
        Err(ArchiveError::Unsupported(_))
    ));
    // ...and a format that can do it.
    let zip = CreateOptions {
        password: Some("pw".into()),
        encrypt_names: true,
        ..opts(Format::Zip)
    };
    assert!(matches!(
        make(tmp.path(), "a.zip", &zip),
        Err(ArchiveError::Unsupported(_))
    ));
    assert!(!tmp.path().join("a.7z").exists() && !tmp.path().join("a.zip").exists());
}

#[test]
fn split_seven_z_with_hidden_names_round_trips() {
    let tmp = tempfile::tempdir().unwrap();
    let options = CreateOptions {
        split: Some(300_000),
        level: Level::Store,
        ..seven_with_password(true)
    };
    let report = make(tmp.path(), "p.7z", &options).unwrap();
    assert!(report.volumes.len() >= 8);
    let first = tmp.path().join("p.7z.001");
    assert_eq!(detect(&first), Some(Format::SevenZ));
    assert!(matches!(
        list(&first, None, &Progress::new()),
        Err(ArchiveError::NeedsPassword)
    ));
    let dest = tmp.path().join("out");
    extract(
        &first,
        &Selection {
            strip: "",
            entries: &[],
        },
        &dest,
        Some("sekret"),
        &Progress::new(),
        &mut FailOnConflict,
    )
    .unwrap();
    assert_sample_extracted(&dest);
}

#[test]
fn seven_z_verification_reports_passwords_and_damage() {
    let tmp = tempfile::tempdir().unwrap();
    make(tmp.path(), "v.7z", &seven_with_password(false)).unwrap();
    let archive = tmp.path().join("v.7z");
    assert!(matches!(
        verify(&archive, None, &Progress::new()),
        Err(ArchiveError::NeedsPassword)
    ));
    assert!(matches!(
        verify(&archive, Some("nope"), &Progress::new()),
        Err(ArchiveError::WrongPassword)
    ));
    let ok = verify(&archive, Some("sekret"), &Progress::new()).unwrap();
    assert!(ok.damaged.is_empty());
    assert_eq!(ok.files, 4);

    // A stored (uncompressed) archive: flipping a data byte breaks that file's checksum.
    let stored = CreateOptions {
        level: Level::Store,
        ..opts(Format::SevenZ)
    };
    make(tmp.path(), "d.7z", &stored).unwrap();
    let path = tmp.path().join("d.7z");
    let mut bytes = fs::read(&path).unwrap();
    let at = bytes.windows(5).position(|w| w == b"alpha").unwrap();
    bytes[at] ^= 0xff;
    fs::write(&path, bytes).unwrap();
    let report = verify(&path, None, &Progress::new()).unwrap();
    assert_eq!(report.damaged.len(), 1, "{:?}", report.damaged);
    assert!(report.files < 4);
}

#[test]
fn a_truncated_seven_z_is_corrupt() {
    let tmp = tempfile::tempdir().unwrap();
    make(tmp.path(), "t.7z", &opts(Format::SevenZ)).unwrap();
    let path = tmp.path().join("t.7z");
    let bytes = fs::read(&path).unwrap();
    fs::write(&path, &bytes[..bytes.len() / 2]).unwrap();
    assert!(list(&path, None, &Progress::new()).is_err());
    fs::write(&path, b"7z but not really").unwrap();
    assert!(matches!(
        list(&path, None, &Progress::new()),
        Err(ArchiveError::Corrupt(_)) | Err(ArchiveError::Io(_))
    ));
}

#[test]
fn hostile_seven_z_names_never_write_outside_the_destination() {
    use sevenz_rust2::{ArchiveEntry, ArchiveWriter};
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("evil.7z");
    {
        let mut writer = ArchiveWriter::new(fs::File::create(&archive).unwrap()).unwrap();
        for name in ["../evil.txt", "/abs.txt", "ok.txt"] {
            writer
                .push_archive_entry(ArchiveEntry::new_file(name), Some(&b"data"[..]))
                .unwrap();
        }
        writer.finish().unwrap();
    }
    let listing = list(&archive, None, &Progress::new()).unwrap();
    assert_eq!(listing.unsafe_skipped, 2);
    let dest = tmp.path().join("nested/out");
    unpack(&archive, &dest, None).unwrap();
    assert!(dest.join("ok.txt").exists());
    assert!(!tmp.path().join("evil.txt").exists());
    assert!(!tmp.path().join("nested/evil.txt").exists());
    assert!(!Path::new("/abs.txt").exists());
}

/// Reports a fixed Unix mode for every file, as a platform that has modes would.
struct Modes;

impl Host for Modes {
    fn conflict(&mut self, _conflict: &Conflict) -> Result<Resolution, OpError> {
        Err(OpError::Cancelled)
    }

    fn file_mode(&self, _path: &Path) -> Option<u32> {
        Some(0o750)
    }
}

#[test]
fn unix_modes_are_stored_and_listed_for_every_format() {
    for (format, name) in [
        (Format::Zip, "m.zip"),
        (Format::Tar, "m.tar"),
        (Format::TarGz, "m.tar.gz"),
        (Format::SevenZ, "m.7z"),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        write(&tmp.path().join("src/f.sh"), "#!/bin/sh\n");
        create(
            &[tmp.path().join("src/f.sh")],
            &tmp.path().join(name),
            &opts(format),
            &Progress::new(),
            &mut Modes,
        )
        .unwrap();
        let listing = list(&tmp.path().join(name), None, &Progress::new()).unwrap();
        assert_eq!(
            listing.find("f.sh").unwrap().mode,
            Some(0o750),
            "{format:?}"
        );
    }
}

#[test]
fn cancelling_a_seven_z_in_the_middle_of_a_big_file_cleans_up() {
    let tmp = tempfile::tempdir().unwrap();
    let mut state = 99u64;
    let data: Vec<u8> = (0..16 * 1024 * 1024)
        .map(|_| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) as u8
        })
        .collect();
    write(&tmp.path().join("src/big.bin"), &data);
    let progress = Progress::new();
    let canceller = {
        let progress = progress.clone();
        std::thread::spawn(move || {
            while progress.bytes_done() == 0 {
                std::thread::yield_now();
            }
            progress.cancel();
        })
    };
    let options = CreateOptions {
        level: Level::Maximum,
        ..opts(Format::SevenZ)
    };
    let result = create(
        &[tmp.path().join("src/big.bin")],
        &tmp.path().join("big.7z"),
        &options,
        &progress,
        &mut FailOnConflict,
    );
    canceller.join().unwrap();
    assert!(matches!(result, Err(ArchiveError::Cancelled)), "{result:?}");
    let leftovers: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != "src")
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

/// The wrong-password check of 7z has no check value, so it relies on decoding noise failing.
/// Every archive has a fresh random salt and IV, so repeat on many, with several kinds of
/// content (tiny, patterned, pseudo-random, empty), to catch flaky detection. A short run
/// keeps CI quick; `BLUEPANE_ROUNDS=1500` gives a deep run (about 40 minutes in a debug build).
#[test]
fn a_wrong_7z_password_is_always_recognised() {
    let tmp = tempfile::tempdir().unwrap();
    let mut state = 7u64;
    let noise: Vec<u8> = (0..150_000)
        .map(|_| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) as u8
        })
        .collect();
    write(
        &tmp.path().join("src/small.txt"),
        "a short note ".repeat(30),
    );
    write(
        &tmp.path().join("src/pattern.bin"),
        (0..120_000u32)
            .map(|i| (i % 251) as u8)
            .collect::<Vec<u8>>(),
    );
    write(&tmp.path().join("src/noise.bin"), &noise);
    write(&tmp.path().join("src/empty.txt"), "");
    let sources = [
        tmp.path().join("src/small.txt"),
        tmp.path().join("src/pattern.bin"),
        tmp.path().join("src/noise.bin"),
        tmp.path().join("src/empty.txt"),
    ];
    let rounds: u32 = std::env::var("BLUEPANE_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8);
    for round in 0..rounds {
        for names_too in [false, true] {
            let name = format!("r{round}-{names_too}.7z");
            create(
                &sources,
                &tmp.path().join(&name),
                &seven_with_password(names_too),
                &Progress::new(),
                &mut FailOnConflict,
            )
            .unwrap();
            let archive = tmp.path().join(&name);
            let dest = tmp.path().join(format!("o{round}-{names_too}"));
            let wrong = if names_too {
                list(&archive, Some("nope"), &Progress::new()).map(|_| ())
            } else {
                unpack(&archive, &dest, Some("nope"))
            };
            assert!(
                matches!(wrong, Err(ArchiveError::WrongPassword)),
                "round {round}, names hidden {names_too}: {wrong:?}"
            );
            if dest.exists() {
                let wrote: Vec<String> = fs::read_dir(&dest)
                    .map(|d| {
                        d.filter_map(Result::ok)
                            .map(|e| e.path().display().to_string())
                            .collect()
                    })
                    .unwrap_or_default();
                panic!("a wrong password must write nothing, but {dest:?} exists: {wrote:?} (round {round})");
            }
            if !names_too {
                let checked = verify(&archive, Some("nope"), &Progress::new());
                assert!(
                    matches!(checked, Err(ArchiveError::WrongPassword)),
                    "verify, round {round}: {checked:?}"
                );
            }
        }
    }
}
