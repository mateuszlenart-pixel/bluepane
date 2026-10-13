//! Compatibility with real tools (7-Zip, Info-ZIP, tar). Each test skips itself when the
//! tool it needs is not installed, so they run wherever the tools exist.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bluepane_archive::{
    create, extract, list, ArchiveError, CreateOptions, Format, Level, Selection,
};
use bluepane_ops::{FailOnConflict, Progress};

fn find_tool(candidates: &[&str]) -> Option<String> {
    candidates
        .iter()
        .find(|name| Command::new(name).output().is_ok())
        .map(|s| s.to_string())
}

fn seven_zip() -> Option<String> {
    find_tool(&["7z", "7zz", "7za"])
}

fn run(program: &str, args: &[&str], dir: &Path) -> std::process::Output {
    Command::new(program)
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("{program}: {e}"))
}

fn ok(output: &std::process::Output) -> bool {
    output.status.success()
}

fn tree(root: &Path) -> PathBuf {
    let dir = root.join("data");
    fs::create_dir_all(dir.join("sub")).unwrap();
    fs::write(dir.join("a.txt"), "alpha text, repeated. ".repeat(500)).unwrap();
    fs::write(
        dir.join("sub/b.bin"),
        (0..200_000u32)
            .map(|i| (i % 251) as u8)
            .collect::<Vec<u8>>(),
    )
    .unwrap();
    fs::write(dir.join("zażółć.txt"), "unicode").unwrap();
    dir
}

fn opts(format: Format) -> CreateOptions {
    CreateOptions {
        format,
        level: Level::Normal,
        password: None,
        split: None,
    }
}

fn mine(sources: &[PathBuf], out: &Path, options: &CreateOptions) {
    create(sources, out, options, &Progress::new(), &mut FailOnConflict).unwrap();
}

fn unpack_mine(archive: &Path, dest: &Path, password: Option<&str>) -> Result<(), ArchiveError> {
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

fn same_tree(a: &Path, b: &Path) {
    for rel in ["a.txt", "sub/b.bin", "zażółć.txt"] {
        assert_eq!(
            fs::read(a.join(rel)).unwrap(),
            fs::read(b.join(rel)).unwrap(),
            "{rel}"
        );
    }
}

#[test]
fn zip_made_here_passes_unzip_and_seven_zip() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tree(tmp.path());
    mine(
        std::slice::from_ref(&data),
        &tmp.path().join("m.zip"),
        &opts(Format::Zip),
    );
    if find_tool(&["unzip"]).is_some() {
        assert!(ok(&run("unzip", &["-t", "m.zip"], tmp.path())), "unzip -t");
    }
    if let Some(z) = seven_zip() {
        assert!(ok(&run(&z, &["t", "m.zip"], tmp.path())), "7z t");
        assert!(ok(&run(&z, &["x", "m.zip", "-oout", "-y"], tmp.path())));
        same_tree(&data, &tmp.path().join("out/data"));
    }
}

#[test]
fn aes_zip_made_here_opens_in_seven_zip_with_the_password_only() {
    let Some(z) = seven_zip() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let data = tree(tmp.path());
    let options = CreateOptions {
        password: Some("sekret".into()),
        ..opts(Format::Zip)
    };
    mine(
        std::slice::from_ref(&data),
        &tmp.path().join("p.zip"),
        &options,
    );
    assert!(
        ok(&run(&z, &["t", "p.zip", "-psekret"], tmp.path())),
        "right password"
    );
    assert!(
        !ok(&run(&z, &["t", "p.zip", "-pwrong"], tmp.path())),
        "wrong password must fail"
    );
    assert!(ok(&run(
        &z,
        &["x", "p.zip", "-psekret", "-oout", "-y"],
        tmp.path()
    )));
    same_tree(&data, &tmp.path().join("out/data"));
}

#[test]
fn split_archive_made_here_is_a_plain_concatenation_seven_zip_reads() {
    let Some(z) = seven_zip() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let data = tree(tmp.path());
    let options = CreateOptions {
        split: Some(20_000),
        level: Level::Store,
        ..opts(Format::Zip)
    };
    mine(
        std::slice::from_ref(&data),
        &tmp.path().join("s.zip"),
        &options,
    );
    assert!(tmp.path().join("s.zip.001").exists() && tmp.path().join("s.zip.002").exists());
    let out = run(&z, &["x", "s.zip.001", "-oout", "-y"], tmp.path());
    assert!(ok(&out), "{}", String::from_utf8_lossy(&out.stdout));
    same_tree(&data, &tmp.path().join("out/data"));
}

#[test]
fn tar_and_targz_made_here_pass_gnu_tar() {
    if find_tool(&["tar"]).is_none() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let data = tree(tmp.path());
    mine(
        std::slice::from_ref(&data),
        &tmp.path().join("m.tar"),
        &opts(Format::Tar),
    );
    mine(
        std::slice::from_ref(&data),
        &tmp.path().join("m.tar.gz"),
        &opts(Format::TarGz),
    );
    for (flag, name) in [("-xf", "m.tar"), ("-xzf", "m.tar.gz")] {
        let dir = tmp.path().join(format!("x-{name}"));
        fs::create_dir_all(&dir).unwrap();
        let archive = tmp.path().join(name);
        assert!(
            ok(&run("tar", &[flag, archive.to_str().unwrap()], &dir)),
            "{name}"
        );
        same_tree(&data, &dir.join("data"));
    }
}

#[test]
fn zip_made_by_info_zip_with_a_password_opens_here() {
    if find_tool(&["zip"]).is_none() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let data = tree(tmp.path());
    assert!(ok(&run(
        "zip",
        &["-r", "-q", "-P", "tajne", "sys.zip", "data"],
        tmp.path()
    )));
    let archive = tmp.path().join("sys.zip");
    assert!(list(&archive, &Progress::new()).unwrap().has_encrypted());
    let out = tmp.path().join("out");
    assert!(matches!(
        unpack_mine(&archive, &out, None),
        Err(ArchiveError::NeedsPassword)
    ));
    assert!(matches!(
        unpack_mine(&archive, &out, Some("zle")),
        Err(ArchiveError::WrongPassword)
    ));
    fs::remove_dir_all(&out).ok();
    unpack_mine(&archive, &out, Some("tajne")).unwrap();
    same_tree(&data, &out.join("data"));
}

#[test]
fn archives_made_by_seven_zip_open_here() {
    let Some(z) = seven_zip() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let data = tree(tmp.path());
    // Plain zip, AES-256 zip, zip split into volumes, tar and tar.gz.
    assert!(ok(&run(
        &z,
        &["a", "-tzip", "plain.zip", "data"],
        tmp.path()
    )));
    assert!(ok(&run(
        &z,
        &["a", "-tzip", "-ppw1", "-mem=AES256", "aes.zip", "data"],
        tmp.path()
    )));
    assert!(ok(&run(
        &z,
        &["a", "-tzip", "-v30k", "vol.zip", "data"],
        tmp.path()
    )));
    assert!(ok(&run(&z, &["a", "-ttar", "t.tar", "data"], tmp.path())));
    assert!(ok(&run(
        &z,
        &["a", "-tgzip", "t.tar.gz", "t.tar"],
        tmp.path()
    )));

    let cases: [(&str, Option<&str>); 5] = [
        ("plain.zip", None),
        ("aes.zip", Some("pw1")),
        ("vol.zip.001", None),
        ("t.tar", None),
        // 7-Zip makes a .tar.gz in two steps: tar, then gzip of that tar.
        ("t.tar.gz", None),
    ];
    for (name, password) in cases {
        let archive = tmp.path().join(name);
        let out = tmp.path().join(format!("out-{name}"));
        unpack_mine(&archive, &out, password).unwrap_or_else(|e| panic!("{name}: {e}"));
        same_tree(&data, &out.join("data"));
    }
}

#[test]
fn archives_made_by_gnu_tar_open_here() {
    if find_tool(&["tar"]).is_none() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let data = tree(tmp.path());
    assert!(ok(&run("tar", &["-czf", "g.tar.gz", "data"], tmp.path())));
    assert!(ok(&run("tar", &["-cf", "g.tar", "data"], tmp.path())));
    for name in ["g.tar.gz", "g.tar"] {
        let out = tmp.path().join(format!("out-{name}"));
        unpack_mine(&tmp.path().join(name), &out, None).unwrap();
        same_tree(&data, &out.join("data"));
    }
}

#[test]
fn zip_with_a_compression_method_this_build_lacks_is_a_clear_error() {
    let Some(z) = seven_zip() else { return };
    let tmp = tempfile::tempdir().unwrap();
    tree(tmp.path());
    let made = run(
        &z,
        &["a", "-tzip", "-mm=BZip2", "bz.zip", "data"],
        tmp.path(),
    );
    if !ok(&made) {
        return; // this 7-Zip cannot write BZip2 zips: nothing to check
    }
    let archive = tmp.path().join("bz.zip");
    // The table of contents is still readable...
    assert!(list(&archive, &Progress::new()).is_ok());
    // ...and extracting says what is wrong instead of failing obscurely.
    let err = unpack_mine(&archive, &tmp.path().join("out"), None).unwrap_err();
    assert!(matches!(err, ArchiveError::Unsupported(_)), "{err}");
}
