//! Reproducible baseline measurements (CR-BP-OPT, OPT-00). Nothing here runs in CI: every test
//! is `#[ignore]`, and the numbers depend on the machine, so they are never asserted on.
//!
//! ```text
//! cargo test -p bluepane-engine --release --test bench -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Data is generated only in a fresh temporary directory (under `BLUEPANE_BENCH_DIR` when set,
//! to measure a specific disk) and removed with it; no real data is touched or scanned.
//! Settings (environment): `BLUEPANE_BENCH_RUNS` (default 7 measured runs after one warm-up),
//! `BLUEPANE_BENCH_LARGE_MIB` (size of the large file, default 256), `BLUEPANE_BENCH_NOTE`
//! (free text about the machine/disk, printed in the header).
//!
//! The first run of every scenario is reported separately as `first`. It is the closest we get
//! to a cold cache without administrator rights; a truly cold cache needs the operator to drop
//! the system caches (see `docs/BENCHMARKS.md`), which these tests never do themselves.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bluepane_engine::{Panel, SortKey};
use bluepane_ops::Progress;
use bluepane_remote::testing::{start, Settings};
use bluepane_remote::{Auth, ConnectParams, KnownHosts, SftpConnection, TransferOptions};

fn runs() -> usize {
    std::env::var("BLUEPANE_BENCH_RUNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n >= 1)
        .unwrap_or(7)
}

fn large_mib() -> u64 {
    std::env::var("BLUEPANE_BENCH_LARGE_MIB")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n >= 1)
        .unwrap_or(256)
}

fn workdir() -> tempfile::TempDir {
    match std::env::var_os("BLUEPANE_BENCH_DIR") {
        Some(base) => tempfile::tempdir_in(base).expect("BLUEPANE_BENCH_DIR must be writable"),
        None => tempfile::tempdir().unwrap(),
    }
}

/// Peak and current resident memory in KiB, where the system exposes it (Linux); `None` on
/// Windows. Read at run time, so no platform-specific code is needed here.
fn memory_kib() -> Option<(u64, u64)> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let field = |name: &str| {
        status
            .lines()
            .find_map(|l| l.strip_prefix(name))
            .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
    };
    Some((field("VmHWM:")?, field("VmRSS:")?))
}

fn header(title: &str) {
    println!("\n=== {title} ===");
    println!(
        "env: os={} arch={} profile={} runs={}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        if cfg!(debug_assertions) {
            "DEBUG (numbers are meaningless, use --release)"
        } else {
            "release"
        },
        runs()
    );
    if let Ok(cpu) = fs::read_to_string("/proc/cpuinfo") {
        if let Some(line) = cpu.lines().find(|l| l.starts_with("model name")) {
            println!(
                "cpu:{}",
                line.trim_start_matches("model name")
                    .trim_start_matches([' ', '\t', ':'])
            );
        }
    }
    if let Ok(note) = std::env::var("BLUEPANE_BENCH_NOTE") {
        println!("note: {note}");
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Nearest-rank percentile of an already sorted, non-empty list.
fn percentile(sorted: &[Duration], p: f64) -> Duration {
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

/// Runs `work` once unmeasured-for-statistics (`first`), once more as warm-up, then `runs()`
/// measured times. `setup` builds fresh input for every run and is not timed.
fn measure<S, T>(name: &str, mut setup: impl FnMut() -> S, mut work: impl FnMut(S) -> T) {
    let timed = |setup: &mut dyn FnMut() -> S, work: &mut dyn FnMut(S) -> T| {
        let input = setup();
        let started = Instant::now();
        let out = work(input);
        let elapsed = started.elapsed();
        drop(out);
        elapsed
    };
    let first = timed(&mut setup, &mut work);
    let _warm_up = timed(&mut setup, &mut work);
    let mut samples: Vec<Duration> = (0..runs()).map(|_| timed(&mut setup, &mut work)).collect();
    samples.sort();
    let (hwm, rss) = memory_kib().unzip();
    println!(
        "BENCH {name}: first={:.1} ms median={:.1} ms p95={:.1} ms min={:.1} ms max={:.1} ms \
         (n={}) peak_rss={} KiB rss={} KiB",
        ms(first),
        ms(percentile(&samples, 50.0)),
        ms(percentile(&samples, 95.0)),
        ms(samples[0]),
        ms(samples[samples.len() - 1]),
        samples.len(),
        hwm.map_or("n/a".into(), |v| v.to_string()),
        rss.map_or("n/a".into(), |v| v.to_string()),
    );
}

/// `count` small files with Polish names, spread over a few subdirectories when `spread`.
fn make_files(dir: &Path, count: usize, size: usize) {
    let body = vec![b'x'; size];
    for i in 0..count {
        fs::write(dir.join(format!("zażółć_{i:06}.txt")), &body).unwrap();
    }
}

#[test]
#[ignore]
fn listing_and_sorting() {
    header("directory listing, sorting and filtering");
    for count in [10_000usize, 100_000] {
        let tmp = workdir();
        make_files(tmp.path(), count, 0);
        measure(
            &format!("vfs::list_dir {count} files"),
            || (),
            |()| bluepane_vfs::list_dir(tmp.path()).unwrap().len(),
        );
        measure(
            &format!("Panel::new {count} files"),
            || (),
            |()| Panel::new(tmp.path()),
        );
        measure(
            &format!("Panel::reload {count} files"),
            || Panel::new(tmp.path()),
            |mut p| {
                p.reload();
                p
            },
        );
        for (key, label) in [
            (SortKey::Name, "name"),
            (SortKey::Size, "size"),
            (SortKey::Date, "date"),
        ] {
            measure(
                &format!("Panel::set_sort({label}) {count} files"),
                || Panel::new(tmp.path()),
                |mut p| {
                    p.set_sort(key, Some(true));
                    p
                },
            );
        }
        measure(
            &format!("Panel::set_filter {count} files"),
            || Panel::new(tmp.path()),
            |mut p| {
                p.set_filter("zażółć_0999");
                p
            },
        );
        // What `Session::finish_job` does after every job: reload both panels in turn.
        let other = workdir();
        make_files(other.path(), count, 0);
        measure(
            &format!("reload of two panels {count} files each"),
            || (Panel::new(tmp.path()), Panel::new(other.path())),
            |(mut a, mut b)| {
                a.reload();
                b.reload();
                (a, b)
            },
        );
    }
}

#[test]
#[ignore]
fn local_copy() {
    header("local copy");
    let tmp = workdir();
    let small = tmp.path().join("small");
    fs::create_dir_all(&small).unwrap();
    make_files(&small, 10_000, 1024);
    let dest = tmp.path().join("dest");
    fs::create_dir_all(&dest).unwrap();
    let mut n = 0;
    measure(
        "ops::copy 10000 files x 1 KiB",
        || {
            n += 1;
            let d = dest.join(format!("run{n}"));
            fs::create_dir_all(&d).unwrap();
            d
        },
        |d| bluepane_ops::copy(&small, &d, &Progress::new()).unwrap(),
    );

    let large = tmp.path().join("large.bin");
    let mib = large_mib();
    let chunk = vec![7u8; 1 << 20];
    {
        use std::io::Write;
        let mut f = fs::File::create(&large).unwrap();
        for _ in 0..mib {
            f.write_all(&chunk).unwrap();
        }
    }
    measure(
        &format!("ops::copy 1 file x {mib} MiB"),
        || {
            n += 1;
            let d = dest.join(format!("big{n}"));
            fs::create_dir_all(&d).unwrap();
            d
        },
        |d| bluepane_ops::copy(&large, &d, &Progress::new()).unwrap(),
    );
}

fn connect(root: &Path) -> (bluepane_remote::testing::TestServer, SftpConnection) {
    let server = start(Settings {
        root: root.to_path_buf(),
        user: "bench".into(),
        password: "bench".into(),
        authorized_key: None,
    });
    let params = ConnectParams {
        port: server.port,
        ..ConnectParams::new("127.0.0.1", "bench", Auth::Password("bench".into()))
    };
    let conn =
        SftpConnection::connect(&params, &KnownHosts::in_memory(), Some(&server.fingerprint))
            .unwrap();
    (server, conn)
}

/// Loopback SFTP against the in-process test server: it measures the client's own overhead
/// (round trips per file, chunking), not a network. Latency and packet loss are not simulated;
/// real SFTP/SMB runs are manual (see `docs/BENCHMARKS.md`).
#[test]
#[ignore]
fn sftp_loopback() {
    header("SFTP loopback (in-process server)");
    let tmp = workdir();
    let root = tmp.path().join("server");
    fs::create_dir_all(&root).unwrap();
    let (_server, conn) = connect(&root);
    let local = tmp.path().join("local");
    fs::create_dir_all(&local).unwrap();
    let mib = large_mib().min(64);
    let big: PathBuf = local.join("big.bin");
    fs::write(&big, vec![3u8; (mib as usize) << 20]).unwrap();
    let mut n = 0;
    measure(
        &format!("sftp upload 1 file x {mib} MiB"),
        || {
            n += 1;
            format!("/up{n}.bin")
        },
        |remote| {
            conn.upload(&big, &remote, &Progress::new(), TransferOptions::default())
                .unwrap()
        },
    );
    measure(
        &format!("sftp download 1 file x {mib} MiB"),
        || {
            n += 1;
            local.join(format!("down{n}.bin"))
        },
        |target| {
            conn.download(
                "/up1.bin",
                &target,
                &Progress::new(),
                TransferOptions::default(),
            )
            .unwrap()
        },
    );
    let small = local.join("small");
    fs::create_dir_all(&small).unwrap();
    make_files(&small, 500, 1024);
    let names: Vec<PathBuf> = fs::read_dir(&small)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    measure(
        "sftp upload 500 files x 1 KiB",
        || {
            n += 1;
            n
        },
        |run| {
            for (i, path) in names.iter().enumerate() {
                conn.upload(
                    path,
                    &format!("/s{run}_{i}.txt"),
                    &Progress::new(),
                    TransferOptions::default(),
                )
                .unwrap();
            }
        },
    );
}
