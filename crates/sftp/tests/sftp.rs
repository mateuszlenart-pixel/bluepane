//! The SFTP client against an in-process server (see `support`).

mod support;

use std::fs;

use bluepane_ops::Progress;
use bluepane_sftp::{
    Auth, ConnectParams, Connection, HostKeyStatus, KnownHosts, SftpError, TransferOptions,
};
use support::{start, write_file, Settings, TestServer};

struct Fixture {
    _tmp: tempfile::TempDir,
    server: TestServer,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("server");
    fs::create_dir_all(&root).unwrap();
    write_file(root.join("hello.txt"), "hello world");
    write_file(root.join("docs/a.txt"), "alpha");
    write_file(root.join("docs/sub/b.txt"), "bravo");
    let server = start(Settings {
        root,
        user: "alice".into(),
        password: "secret".into(),
        authorized_key: None,
    });
    Fixture { _tmp: tmp, server }
}

fn params(fx: &Fixture, password: &str) -> ConnectParams {
    ConnectParams {
        port: fx.server.port,
        ..ConnectParams::new("127.0.0.1", "alice", Auth::Password(password.into()))
    }
}

fn connect(fx: &Fixture) -> Connection {
    Connection::connect(
        &params(fx, "secret"),
        &KnownHosts::in_memory(),
        Some(&fx.server.fingerprint),
    )
    .unwrap()
}

#[test]
fn an_unknown_host_is_not_trusted_silently() {
    let fx = fixture();
    let err =
        Connection::connect(&params(&fx, "secret"), &KnownHosts::in_memory(), None).unwrap_err();
    match err {
        SftpError::UnknownHost {
            fingerprint,
            algorithm,
            port,
            ..
        } => {
            assert_eq!(fingerprint, fx.server.fingerprint);
            assert_eq!(algorithm, fx.server.algorithm);
            assert_eq!(port, fx.server.port);
        }
        other => panic!("{other}"),
    }
}

#[test]
fn a_confirmed_key_is_remembered_and_a_changed_one_refused() {
    let fx = fixture();
    let mut known = KnownHosts::in_memory();
    assert!(Connection::connect(&params(&fx, "secret"), &known, Some("SHA256:wrong")).is_err());
    known
        .add(
            "127.0.0.1",
            fx.server.port,
            &fx.server.algorithm,
            &fx.server.fingerprint,
        )
        .unwrap();
    assert_eq!(
        known.status(
            "127.0.0.1",
            fx.server.port,
            &fx.server.algorithm,
            &fx.server.fingerprint
        ),
        HostKeyStatus::Known
    );
    // Remembered: no confirmation needed any more.
    Connection::connect(&params(&fx, "secret"), &known, None)
        .unwrap()
        .close();

    // The same host with another key on file: refused even when the caller says "trust".
    let mut other = KnownHosts::in_memory();
    other
        .add(
            "127.0.0.1",
            fx.server.port,
            &fx.server.algorithm,
            "SHA256:somethingelse",
        )
        .unwrap();
    let err = Connection::connect(&params(&fx, "secret"), &other, Some(&fx.server.fingerprint))
        .unwrap_err();
    assert!(matches!(err, SftpError::HostKeyChanged { .. }), "{err}");
    assert!(err.to_string().contains("się zmienił"));
}

#[test]
fn a_wrong_password_is_a_clear_error() {
    let fx = fixture();
    let err = Connection::connect(
        &params(&fx, "nope"),
        &KnownHosts::in_memory(),
        Some(&fx.server.fingerprint),
    )
    .unwrap_err();
    assert!(matches!(err, SftpError::Auth(_)), "{err}");
}

#[test]
fn an_unreachable_server_is_a_clear_error() {
    let tmp = tempfile::tempdir().unwrap();
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let params = ConnectParams {
        port,
        ..ConnectParams::new("127.0.0.1", "u", Auth::Password("p".into()))
    };
    let err = Connection::connect(
        &params,
        &KnownHosts::load(&tmp.path().join("k")).unwrap(),
        None,
    )
    .unwrap_err();
    assert!(matches!(err, SftpError::Connect(_)), "{err}");
}

#[test]
fn lists_and_stats() {
    let fx = fixture();
    let conn = connect(&fx);
    assert_eq!(conn.home().unwrap(), "/");
    let mut names: Vec<_> = conn
        .list("/")
        .unwrap()
        .into_iter()
        .map(|e| (e.name, e.is_dir, e.size))
        .collect();
    names.sort();
    assert_eq!(
        names,
        [("docs".into(), true, 0), ("hello.txt".into(), false, 11)]
    );
    assert!(conn.stat("/docs/sub/b.txt").unwrap().is_some());
    assert!(conn.stat("/nothing").unwrap().is_none());
    assert!(conn.stat("/docs").unwrap().unwrap().is_dir);
    assert!(conn.list("/missing").is_err());
}

#[test]
fn makes_renames_and_removes() {
    let fx = fixture();
    let conn = connect(&fx);
    conn.mkdir("/new").unwrap();
    assert!(fx.server.root.join("new").is_dir());
    conn.rename("/hello.txt", "/new/moved.txt").unwrap();
    assert!(fx.server.root.join("new/moved.txt").is_file());
    // A rename never replaces.
    write_file(fx.server.root.join("x.txt"), "x");
    assert!(matches!(
        conn.rename("/x.txt", "/new/moved.txt"),
        Err(SftpError::Exists(_))
    ));
    conn.remove_file("/new/moved.txt").unwrap();
    conn.remove_dir("/new").unwrap();
    assert!(!fx.server.root.join("new").exists());
    assert!(conn.remove_file("/missing").is_err());
}

#[test]
fn downloads_keep_the_modification_time_and_never_overwrite() {
    let fx = fixture();
    let conn = connect(&fx);
    let local = tempfile::tempdir().unwrap();
    let target = local.path().join("hello.txt");
    let progress = Progress::new();
    conn.download("/hello.txt", &target, &progress, TransferOptions::default())
        .unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "hello world");
    assert_eq!(progress.bytes_done(), 11);
    assert!(!local.path().join("hello.txt.bpart").exists());
    let remote_time = fs::metadata(fx.server.root.join("hello.txt"))
        .unwrap()
        .modified()
        .unwrap();
    let local_time = fs::metadata(&target).unwrap().modified().unwrap();
    let diff = remote_time
        .duration_since(local_time)
        .unwrap_or_else(|e| e.duration());
    assert!(diff.as_secs() <= 1, "{diff:?}");

    let again = conn.download(
        "/hello.txt",
        &target,
        &Progress::new(),
        TransferOptions::default(),
    );
    assert!(matches!(again, Err(SftpError::Exists(_))));
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
}

#[test]
fn uploads_never_overwrite_unless_told() {
    let fx = fixture();
    let conn = connect(&fx);
    let local = tempfile::tempdir().unwrap();
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
        Err(SftpError::Exists(_))
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
fn a_cancelled_download_resumes_where_it_stopped() {
    let fx = fixture();
    let big: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
    write_file(fx.server.root.join("big.bin"), &big);
    let conn = connect(&fx);
    let local = tempfile::tempdir().unwrap();
    let target = local.path().join("big.bin");

    // Cancelled right away: nothing under the real name, no data yet.
    let progress = Progress::new();
    progress.cancel();
    let result = conn.download("/big.bin", &target, &progress, TransferOptions::default());
    assert!(matches!(result, Err(SftpError::Cancelled)));
    assert!(!target.exists());

    // A partial file from an earlier attempt: resume continues after its last byte.
    fs::write(local.path().join("big.bin.bpart"), &big[..1_000_000]).unwrap();
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
    assert_eq!(
        progress.bytes_done(),
        big.len() as u64,
        "resumed bytes count too"
    );
}

#[test]
fn a_cancelled_upload_resumes_where_it_stopped() {
    let fx = fixture();
    let big: Vec<u8> = (0..2_500_000u32).map(|i| (i % 241) as u8).collect();
    let local = tempfile::tempdir().unwrap();
    let source = local.path().join("big.bin");
    fs::write(&source, &big).unwrap();
    let conn = connect(&fx);

    let progress = Progress::new();
    progress.cancel();
    assert!(matches!(
        conn.upload(&source, "/big.bin", &progress, TransferOptions::default()),
        Err(SftpError::Cancelled)
    ));
    assert!(!fx.server.root.join("big.bin").exists());

    write_file(fx.server.root.join("big.bin.bpart"), &big[..900_000]);
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

#[test]
fn key_login_works() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("server");
    fs::create_dir_all(&root).unwrap();
    let key =
        russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519).unwrap();
    let server = start(Settings {
        root,
        user: "bob".into(),
        password: "unused".into(),
        authorized_key: Some(key.public_key().clone()),
    });
    let key_path = tmp.path().join("id_ed25519");
    fs::write(
        &key_path,
        key.to_openssh(russh::keys::ssh_key::LineEnding::LF)
            .unwrap()
            .as_str(),
    )
    .unwrap();
    let params = ConnectParams {
        port: server.port,
        ..ConnectParams::new(
            "127.0.0.1",
            "bob",
            Auth::Key {
                path: key_path.clone(),
                passphrase: None,
            },
        )
    };
    let conn =
        Connection::connect(&params, &KnownHosts::in_memory(), Some(&server.fingerprint)).unwrap();
    assert_eq!(conn.home().unwrap(), "/");

    let bad = ConnectParams {
        auth: Auth::Key {
            path: tmp.path().join("missing"),
            passphrase: None,
        },
        ..params
    };
    let err =
        Connection::connect(&bad, &KnownHosts::in_memory(), Some(&server.fingerprint)).unwrap_err();
    assert!(matches!(err, SftpError::Key(_)), "{err}");
}
