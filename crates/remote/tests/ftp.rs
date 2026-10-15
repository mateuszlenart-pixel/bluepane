//! FTP and FTPS against an in-process server (see `bluepane_remote::testing`).

use std::fs;
use std::sync::Arc;

use bluepane_ops::Progress;
use bluepane_remote::testing::{start_ftp, write_file, FtpServer, FtpSettings, Tls};
use bluepane_remote::{
    KnownHosts, Protocol, RemoteError, RemoteFs, SecretStore, Site, TransferOptions,
};

struct Fixture {
    _tmp: tempfile::TempDir,
    server: FtpServer,
    protocol: Protocol,
}

fn fixture_with(tls: Tls, no_mlsd: bool) -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("server");
    write_file(root.join("hello.txt"), "hello world");
    write_file(root.join("docs/a.txt"), "alpha");
    write_file(root.join("docs/sub/b.txt"), "bravo");
    let server = start_ftp(FtpSettings {
        root,
        user: "alice".into(),
        password: "secret".into(),
        tls,
        no_mlsd,
    });
    let protocol = match tls {
        Tls::None => Protocol::Ftp,
        Tls::Explicit => Protocol::FtpsExplicit,
        Tls::Implicit => Protocol::FtpsImplicit,
    };
    Fixture {
        _tmp: tmp,
        server,
        protocol,
    }
}

fn fixture() -> Fixture {
    fixture_with(Tls::None, false)
}

fn site(fx: &Fixture) -> Site {
    let mut site = Site::new("Test", "127.0.0.1", "alice");
    site.protocol = fx.protocol;
    site.port = fx.server.port;
    site.secret = SecretStore::Local("secret".into());
    site
}

fn connect_with(
    fx: &Fixture,
    password: &str,
    known: &KnownHosts,
    trust: Option<&str>,
) -> Result<Arc<dyn RemoteFs>, RemoteError> {
    site(fx).connect(Some(password.into()), known, trust)
}

fn connect(fx: &Fixture) -> Arc<dyn RemoteFs> {
    connect_with(
        fx,
        "secret",
        &KnownHosts::in_memory(),
        fx.server.fingerprint.as_deref(),
    )
    .unwrap()
}

#[test]
fn logs_in_and_reports_the_protocol() {
    let fx = fixture();
    let conn = connect(&fx);
    assert_eq!(conn.protocol(), Protocol::Ftp);
    assert_eq!(conn.home().unwrap(), "/");
    let err = connect_with(&fx, "wrong", &KnownHosts::in_memory(), None).unwrap_err();
    assert!(matches!(err, RemoteError::Auth(_)), "{err}");
}

#[test]
fn lists_and_stats_with_mlsd_and_with_the_list_fallback() {
    for no_mlsd in [false, true] {
        let fx = fixture_with(Tls::None, no_mlsd);
        let conn = connect(&fx);
        let mut names: Vec<_> = conn
            .list("/")
            .unwrap()
            .into_iter()
            .map(|e| (e.name, e.is_dir, e.size))
            .collect();
        names.sort();
        assert_eq!(
            names,
            [("docs".into(), true, 0), ("hello.txt".into(), false, 11)],
            "no_mlsd={no_mlsd}"
        );
        let file = conn.stat("/docs/sub/b.txt").unwrap().unwrap();
        assert!(!file.is_dir && file.size == 5);
        assert!(conn.stat("/docs").unwrap().unwrap().is_dir);
        assert!(conn.stat("/nothing").unwrap().is_none());
        assert!(conn.list("/missing").is_err());
        assert_eq!(conn.canonical("/docs/../hello.txt").unwrap(), "/hello.txt");
    }
}

#[test]
fn makes_renames_and_removes() {
    let fx = fixture();
    let conn = connect(&fx);
    conn.mkdir("/new").unwrap();
    assert!(fx.server.root.join("new").is_dir());
    conn.rename("/hello.txt", "/new/moved.txt").unwrap();
    assert!(fx.server.root.join("new/moved.txt").is_file());
    write_file(fx.server.root.join("x.txt"), "x");
    assert!(matches!(
        conn.rename("/x.txt", "/new/moved.txt"),
        Err(RemoteError::Exists(_))
    ));
    conn.remove_file("/new/moved.txt").unwrap();
    conn.remove_dir("/new").unwrap();
    assert!(!fx.server.root.join("new").exists());
    assert!(conn.remove_file("/missing").is_err());
}

fn transfers(fx: &Fixture) {
    let conn = connect(fx);
    let local = tempfile::tempdir().unwrap();
    let target = local.path().join("hello.txt");
    let progress = Progress::new();
    conn.download("/hello.txt", &target, &progress, TransferOptions::default())
        .unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "hello world");
    assert_eq!(progress.bytes_done(), 11);
    assert!(!local.path().join("hello.txt.bpart").exists());
    assert!(matches!(
        conn.download(
            "/hello.txt",
            &target,
            &Progress::new(),
            TransferOptions::default()
        ),
        Err(RemoteError::Exists(_))
    ));
    fs::write(&target, "old").unwrap();
    conn.download(
        "/hello.txt",
        &target,
        &Progress::new(),
        TransferOptions {
            overwrite: true,
            ..TransferOptions::default()
        },
    )
    .unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "hello world");

    let source = local.path().join("up.txt");
    fs::write(&source, "uploaded data").unwrap();
    conn.upload(
        &source,
        "/up.txt",
        &Progress::new(),
        TransferOptions::default(),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(fx.server.root.join("up.txt")).unwrap(),
        "uploaded data"
    );
    assert!(!fx.server.root.join("up.txt.bpart").exists());
    assert!(matches!(
        conn.upload(
            &source,
            "/up.txt",
            &Progress::new(),
            TransferOptions::default()
        ),
        Err(RemoteError::Exists(_))
    ));
    fs::write(&source, "second").unwrap();
    conn.upload(
        &source,
        "/up.txt",
        &Progress::new(),
        TransferOptions {
            overwrite: true,
            ..TransferOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(fx.server.root.join("up.txt")).unwrap(),
        "second"
    );
}

#[test]
fn downloads_and_uploads() {
    transfers(&fixture());
}

#[test]
fn a_download_resumes_and_a_stale_partial_file_is_not_continued() {
    let fx = fixture();
    let big: Vec<u8> = (0..2_000_000u32).map(|i| (i % 251) as u8).collect();
    write_file(fx.server.root.join("big.bin"), &big);
    let conn = connect(&fx);
    let local = tempfile::tempdir().unwrap();
    let target = local.path().join("big.bin");

    let cancelled = Progress::new();
    cancelled.cancel();
    assert!(matches!(
        conn.download("/big.bin", &target, &cancelled, TransferOptions::default()),
        Err(RemoteError::Cancelled)
    ));
    assert!(!target.exists());

    fs::write(local.path().join("big.bin.bpart"), &big[..700_000]).unwrap();
    let progress = Progress::new();
    conn.download(
        "/big.bin",
        &target,
        &progress,
        TransferOptions {
            resume: true,
            ..TransferOptions::default()
        },
    )
    .unwrap();
    assert_eq!(fs::read(&target).unwrap(), big);
    assert_eq!(progress.bytes_done(), big.len() as u64);

    // A partial file from long ago is not the start of this version of the file.
    fs::remove_file(&target).unwrap();
    let part = local.path().join("big.bin.bpart");
    fs::write(&part, b"STALE").unwrap();
    let long_ago = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
    fs::OpenOptions::new()
        .write(true)
        .open(&part)
        .unwrap()
        .set_modified(long_ago)
        .unwrap();
    conn.download(
        "/big.bin",
        &target,
        &Progress::new(),
        TransferOptions {
            resume: true,
            ..TransferOptions::default()
        },
    )
    .unwrap();
    assert_eq!(fs::read(&target).unwrap(), big);
}

#[test]
fn an_upload_resumes() {
    let fx = fixture();
    let big: Vec<u8> = (0..1_500_000u32).map(|i| (i % 241) as u8).collect();
    let local = tempfile::tempdir().unwrap();
    let source = local.path().join("big.bin");
    fs::write(&source, &big).unwrap();
    let conn = connect(&fx);
    write_file(fx.server.root.join("big.bin.bpart"), &big[..600_000]);
    conn.upload(
        &source,
        "/big.bin",
        &Progress::new(),
        TransferOptions {
            resume: true,
            ..TransferOptions::default()
        },
    )
    .unwrap();
    assert_eq!(fs::read(fx.server.root.join("big.bin")).unwrap(), big);
    assert!(!fx.server.root.join("big.bin.bpart").exists());
}

// ---- FTPS ---------------------------------------------------------------------------

#[test]
fn an_unknown_certificate_is_not_trusted_silently_in_both_ftps_modes() {
    for tls in [Tls::Explicit, Tls::Implicit] {
        let fx = fixture_with(tls, false);
        let err = connect_with(&fx, "secret", &KnownHosts::in_memory(), None).unwrap_err();
        match err {
            RemoteError::UnknownHost {
                fingerprint,
                algorithm,
                ..
            } => {
                assert_eq!(Some(fingerprint), fx.server.fingerprint);
                assert_eq!(algorithm, "tls-certificate");
            }
            other => panic!("{other}"),
        }
    }
}

#[test]
fn a_confirmed_certificate_works_for_listing_and_transfers_and_is_remembered() {
    for tls in [Tls::Explicit, Tls::Implicit] {
        let fx = fixture_with(tls, false);
        let fingerprint = fx.server.fingerprint.clone().unwrap();
        let conn =
            connect_with(&fx, "secret", &KnownHosts::in_memory(), Some(&fingerprint)).unwrap();
        assert_eq!(conn.protocol(), fx.protocol);
        assert_eq!(conn.list("/").unwrap().len(), 2);
        drop(conn);
        transfers(&fx);

        // Remembered: no confirmation needed any more.
        let mut known = KnownHosts::in_memory();
        known
            .add("127.0.0.1", fx.server.port, "tls-certificate", &fingerprint)
            .unwrap();
        connect_with(&fx, "secret", &known, None).unwrap();

        // A different certificate on file: always refused.
        let mut other = KnownHosts::in_memory();
        other
            .add(
                "127.0.0.1",
                fx.server.port,
                "tls-certificate",
                "SHA256:other",
            )
            .unwrap();
        let err = connect_with(&fx, "secret", &other, Some(&fingerprint)).unwrap_err();
        assert!(matches!(err, RemoteError::HostKeyChanged { .. }), "{err}");
    }
}

#[test]
fn a_wrong_ftps_password_is_an_auth_error() {
    let fx = fixture_with(Tls::Explicit, false);
    let fingerprint = fx.server.fingerprint.clone().unwrap();
    let err = connect_with(&fx, "nope", &KnownHosts::in_memory(), Some(&fingerprint)).unwrap_err();
    assert!(matches!(err, RemoteError::Auth(_)), "{err}");
}

#[test]
fn chmod_sends_site_chmod_and_reports_a_refusal() {
    let fx = fixture();
    let conn = connect(&fx);
    conn.chmod("/hello.txt", 0o640).unwrap();
    assert_eq!(
        bluepane_remote::testing::last_chmod(fx.server.root.join("hello.txt")),
        Some(0o640)
    );
    assert!(conn.chmod("/missing.txt", 0o644).is_err());
}

fn active_site(fx: &Fixture) -> Site {
    let mut site = site(fx);
    site.active = true;
    site
}

fn transfers_in_active_mode(tls: Tls) {
    let fx = fixture_with(tls, false);
    let conn = active_site(&fx)
        .connect(
            Some("secret".into()),
            &KnownHosts::in_memory(),
            fx.server.fingerprint.as_deref(),
        )
        .unwrap();
    let names: Vec<String> = conn
        .list("/")
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(names.contains(&"hello.txt".to_string()), "{names:?}");
    let local = fx._tmp.path().join("got.txt");
    conn.download(
        "/hello.txt",
        &local,
        &Progress::new(),
        TransferOptions::default(),
    )
    .unwrap();
    assert_eq!(fs::read_to_string(&local).unwrap(), "hello world");
    write_file(fx._tmp.path().join("up.txt"), "sent");
    conn.upload(
        &fx._tmp.path().join("up.txt"),
        "/up.txt",
        &Progress::new(),
        TransferOptions::default(),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(fx.server.root.join("up.txt")).unwrap(),
        "sent"
    );
    // The server really connected back to us for each of the four data transfers.
    assert!(bluepane_remote::testing::active_transfers(&fx.server.root) >= 3);
}

#[test]
fn active_mode_lists_downloads_and_uploads() {
    transfers_in_active_mode(Tls::None);
}

#[test]
fn active_mode_works_over_ftps_too() {
    transfers_in_active_mode(Tls::Explicit);
}
