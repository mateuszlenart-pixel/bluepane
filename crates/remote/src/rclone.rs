//! Cloud storage (WebDAV, S3, Google Drive, OneDrive and the many others `rclone` knows) as a
//! [`RemoteFs`]. bluepane does not speak these protocols or keep their logins: it asks the
//! `rclone` program, which the user has installed and set up with `rclone config`. A site
//! of the `rclone` kind only names one of rclone's remotes.
//!
//! Every call is one run of `rclone` (`lsjson`, `mkdir`, `deletefile`, `rmdir`, `moveto`,
//! `copyto`); how the program is started is behind [`RcloneRunner`], so tests use a fake one
//! and need neither rclone nor a network.

use std::cell::Cell;
use std::fmt::Debug;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bluepane_ops::Progress;

use crate::{Protocol, RemoteEntry, RemoteError, RemoteFs, TransferOptions};

/// What one run of `rclone` produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RcloneOutput {
    /// The exit code (`3` is "directory not found", `4` "file not found").
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

/// Starts `rclone`.
pub trait RcloneRunner: Send + Sync + Debug {
    /// Runs it with these arguments. `cancelled` is asked while it runs (about every 50 ms);
    /// when it says yes the run is stopped and the answer is `Ok(None)`.
    fn run(
        &self,
        args: &[String],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<RcloneOutput>, RemoteError>;
}

/// The `rclone` program found through the search path.
#[derive(Debug, Clone)]
pub struct SystemRclone {
    program: String,
}

impl SystemRclone {
    pub fn new() -> SystemRclone {
        SystemRclone {
            program: "rclone".into(),
        }
    }
}

impl Default for SystemRclone {
    fn default() -> Self {
        SystemRclone::new()
    }
}

/// What to say when the program is not there.
pub const NOT_INSTALLED: &str =
    "nie znaleziono programu rclone: zainstaluj go (rclone.org) i skonfiguruj chmurę poleceniem `rclone config`";

impl RcloneRunner for SystemRclone {
    fn run(
        &self,
        args: &[String],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<RcloneOutput>, RemoteError> {
        match bluepane_vfs::run_cancellable(&self.program, args, cancelled) {
            Ok(None) => Ok(None),
            Ok(Some(output)) => Ok(Some(RcloneOutput {
                code: output.status.code().unwrap_or(-1),
                stdout: output.stdout,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(RemoteError::Connect(NOT_INSTALLED.into()))
            }
            Err(e) => Err(RemoteError::Connect(format!("rclone: {e}"))),
        }
    }
}

/// One remote configured in rclone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudRemote {
    pub name: String,
    /// The kind rclone reports: `drive`, `onedrive`, `s3`, `webdav`...
    pub kind: String,
}

/// The remotes rclone has been configured with (`rclone listremotes --long`).
pub fn list_remotes(runner: &dyn RcloneRunner) -> Result<Vec<CloudRemote>, RemoteError> {
    let args = ["listremotes".to_string(), "--long".to_string()];
    let output = runner
        .run(&args, &|| false)?
        .ok_or(RemoteError::Cancelled)?;
    if output.code != 0 {
        return Err(failure(&output));
    }
    Ok(parse_remotes(&String::from_utf8_lossy(&output.stdout)))
}

/// `name:   type` lines.
fn parse_remotes(text: &str) -> Vec<CloudRemote> {
    text.lines()
        .filter_map(|line| {
            let (name, kind) = line.split_once(':')?;
            let name = name.trim();
            (!name.is_empty()).then(|| CloudRemote {
                name: name.to_string(),
                kind: kind.trim().to_string(),
            })
        })
        .collect()
}

/// A message from what rclone said when it failed.
fn failure(output: &RcloneOutput) -> RemoteError {
    let last = output
        .stderr
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .unwrap_or("nieznany błąd");
    let text = match output.code {
        3 => format!("nie ma takiego katalogu ({last})"),
        4 => format!("nie ma takiego pliku ({last})"),
        _ => last.to_string(),
    };
    RemoteError::Remote(text)
}

/// A path with `.` and `..` resolved, always starting with `/`.
fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    format!("/{}", parts.join("/"))
}

fn to_time(text: &str) -> Option<SystemTime> {
    let parsed = chrono::DateTime::parse_from_rfc3339(text).ok()?;
    let secs = u64::try_from(parsed.timestamp()).ok()?;
    Some(UNIX_EPOCH + Duration::from_secs(secs))
}

/// The entries in `lsjson` output.
fn parse_listing(bytes: &[u8]) -> Result<Vec<RemoteEntry>, RemoteError> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| RemoteError::Remote(format!("niezrozumiała odpowiedź rclone: {e}")))?;
    let items = value
        .as_array()
        .ok_or_else(|| RemoteError::Remote("rclone nie zwrócił listy".into()))?;
    Ok(items
        .iter()
        .filter_map(|item| {
            let name = item.get("Name")?.as_str()?.to_string();
            Some(RemoteEntry {
                name,
                is_dir: item.get("IsDir").and_then(|v| v.as_bool()).unwrap_or(false),
                is_symlink: false,
                // Some clouds do not know a size (-1): it shows as 0.
                size: item
                    .get("Size")
                    .and_then(|v| v.as_i64())
                    .map_or(0, |n| u64::try_from(n).unwrap_or(0)),
                modified: item
                    .get("ModTime")
                    .and_then(|v| v.as_str())
                    .and_then(to_time),
                permissions: None,
            })
        })
        .collect())
}

/// A cloud as a file system: the remote called `remote` in rclone's configuration.
#[derive(Debug)]
pub struct RcloneFs {
    runner: Arc<dyn RcloneRunner>,
    remote: String,
}

impl RcloneFs {
    /// Checks that rclone knows a remote of this name.
    pub fn connect(runner: Arc<dyn RcloneRunner>, remote: &str) -> Result<RcloneFs, RemoteError> {
        if remote.is_empty() || remote.contains([':', '/', '\\']) {
            return Err(RemoteError::Connect(format!(
                "„{remote}” nie jest nazwą chmury z rclone (np. gdrive)"
            )));
        }
        let known = list_remotes(&*runner)?;
        if !known.iter().any(|r| r.name == remote) {
            let names: Vec<&str> = known.iter().map(|r| r.name.as_str()).collect();
            return Err(RemoteError::Connect(format!(
                "rclone nie zna chmury „{remote}” (są: {}); dodaj ją poleceniem `rclone config`",
                if names.is_empty() {
                    "brak".to_string()
                } else {
                    names.join(", ")
                }
            )));
        }
        Ok(RcloneFs {
            runner,
            remote: remote.to_string(),
        })
    }

    /// `remote:path/in/the/cloud` for a path that starts at the cloud's root.
    fn spec(&self, path: &str) -> String {
        format!(
            "{}:{}",
            self.remote,
            normalize(path).trim_start_matches('/')
        )
    }

    fn run(&self, args: &[&str], progress: Option<&Progress>) -> Result<RcloneOutput, RemoteError> {
        self.run_watching(args, progress, &|| {})
    }

    /// Runs rclone; `watch` is called every time it is checked on, and a cancelled
    /// `progress` stops the run.
    fn run_watching(
        &self,
        args: &[&str],
        progress: Option<&Progress>,
        watch: &dyn Fn(),
    ) -> Result<RcloneOutput, RemoteError> {
        let args: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
        let output = self.runner.run(&args, &|| {
            watch();
            progress.is_some_and(Progress::is_cancelled)
        })?;
        output.ok_or(RemoteError::Cancelled)
    }

    fn ok(&self, args: &[&str]) -> Result<RcloneOutput, RemoteError> {
        let output = self.run(args, None)?;
        if output.code == 0 {
            Ok(output)
        } else {
            Err(failure(&output))
        }
    }
}

impl RemoteFs for RcloneFs {
    fn protocol(&self) -> Protocol {
        Protocol::Rclone
    }

    fn home(&self) -> Result<String, RemoteError> {
        Ok("/".into())
    }

    fn canonical(&self, path: &str) -> Result<String, RemoteError> {
        Ok(normalize(path))
    }

    fn list(&self, path: &str) -> Result<Vec<RemoteEntry>, RemoteError> {
        let output = self.ok(&["lsjson", "--no-mimetype", &self.spec(path)])?;
        parse_listing(&output.stdout)
    }

    fn stat(&self, path: &str) -> Result<Option<RemoteEntry>, RemoteError> {
        let path = normalize(path);
        let Some((parent, name)) = path.rsplit_once('/') else {
            return Ok(None);
        };
        if name.is_empty() {
            // The root of the cloud always exists.
            return Ok(Some(RemoteEntry {
                name: String::new(),
                is_dir: true,
                is_symlink: false,
                size: 0,
                modified: None,
                permissions: None,
            }));
        }
        let output = self.run(&["lsjson", "--no-mimetype", &self.spec(parent)], None)?;
        match output.code {
            0 => Ok(parse_listing(&output.stdout)?
                .into_iter()
                .find(|e| e.name == name)),
            // The folder above is not there, so neither is the path.
            3 => Ok(None),
            _ => Err(failure(&output)),
        }
    }

    fn mkdir(&self, path: &str) -> Result<(), RemoteError> {
        self.ok(&["mkdir", &self.spec(path)]).map(|_| ())
    }

    fn remove_file(&self, path: &str) -> Result<(), RemoteError> {
        self.ok(&["deletefile", &self.spec(path)]).map(|_| ())
    }

    fn remove_dir(&self, path: &str) -> Result<(), RemoteError> {
        self.ok(&["rmdir", &self.spec(path)]).map(|_| ())
    }

    fn chmod(&self, _path: &str, _mode: u32) -> Result<(), RemoteError> {
        Err(RemoteError::Remote(
            "chmoda nie ma w chmurze (nie ma uprawnień unixowych)".into(),
        ))
    }

    fn rename(&self, from: &str, to: &str) -> Result<(), RemoteError> {
        if self.stat(to)?.is_some() {
            return Err(RemoteError::Exists(to.to_string()));
        }
        self.ok(&["moveto", &self.spec(from), &self.spec(to)])
            .map(|_| ())
    }

    fn download(
        &self,
        remote: &str,
        local: &Path,
        progress: &Progress,
        options: TransferOptions,
    ) -> Result<(), RemoteError> {
        if !options.overwrite && std::fs::symlink_metadata(local).is_ok() {
            return Err(RemoteError::Exists(local.display().to_string()));
        }
        let entry = self
            .stat(remote)?
            .ok_or_else(|| RemoteError::Remote(format!("nie ma pliku {remote}")))?;
        if entry.is_dir {
            return Err(RemoteError::Remote(format!("{remote} jest katalogiem")));
        }
        // Like the other protocols: the bytes land under a `.bpart` name first.
        let mut part_name = local.file_name().unwrap_or_default().to_os_string();
        part_name.push(".bpart");
        let part = local.with_file_name(part_name);
        let _ = std::fs::remove_file(&part);
        let counted = Cell::new(0u64);
        let target = part.display().to_string();
        let result = self.run_watching(
            &["copyto", &self.spec(remote), &target],
            Some(progress),
            // How far it got is how big the part file is by now.
            &|| {
                if let Ok(meta) = std::fs::metadata(&part) {
                    if meta.len() > counted.get() {
                        progress.add(meta.len() - counted.get());
                        counted.set(meta.len());
                    }
                }
            },
        );
        let output = match result {
            Ok(output) => output,
            Err(e) => {
                let _ = std::fs::remove_file(&part);
                return Err(e);
            }
        };
        if output.code != 0 {
            let _ = std::fs::remove_file(&part);
            return Err(failure(&output));
        }
        let written = std::fs::metadata(&part)?.len();
        if entry.size > 0 && written != entry.size {
            let _ = std::fs::remove_file(&part);
            return Err(RemoteError::Remote(format!(
                "pobrano {written} z {} bajtów",
                entry.size
            )));
        }
        progress.add(written.saturating_sub(counted.get()));
        if options.overwrite {
            let _ = std::fs::remove_file(local);
        }
        std::fs::rename(&part, local)?;
        Ok(())
    }

    fn upload(
        &self,
        local: &Path,
        remote: &str,
        progress: &Progress,
        options: TransferOptions,
    ) -> Result<(), RemoteError> {
        let size = std::fs::metadata(local)?.len();
        if !options.overwrite && self.stat(remote)?.is_some() {
            return Err(RemoteError::Exists(remote.to_string()));
        }
        let source = local.display().to_string();
        let output = self.run(&["copyto", &source, &self.spec(remote)], Some(progress))?;
        if output.code != 0 {
            return Err(failure(&output));
        }
        progress.add(size);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeRclone;

    fn cloud() -> (tempfile::TempDir, RcloneFs) {
        let dir = tempfile::tempdir().unwrap();
        let fake = FakeRclone::new(dir.path(), &[("chmura", "drive"), ("dysk", "webdav")]);
        let fs = RcloneFs::connect(Arc::new(fake), "chmura").unwrap();
        (dir, fs)
    }

    #[test]
    fn remotes_are_read_from_the_long_listing() {
        let remotes = parse_remotes("gdrive:      drive\nmoj nas:    webdav\n\nzepsuty\n:\n");
        assert_eq!(
            remotes,
            [
                CloudRemote {
                    name: "gdrive".into(),
                    kind: "drive".into()
                },
                CloudRemote {
                    name: "moj nas".into(),
                    kind: "webdav".into()
                }
            ]
        );
    }

    #[test]
    fn connecting_needs_a_remote_rclone_knows() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Arc::new(FakeRclone::new(dir.path(), &[("chmura", "drive")]));
        let err = RcloneFs::connect(fake.clone(), "inna")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("nie zna chmury") && err.contains("chmura"),
            "{err}"
        );
        for bad in ["", "a:b", "a/b"] {
            assert!(RcloneFs::connect(fake.clone(), bad).is_err(), "{bad}");
        }
        assert!(RcloneFs::connect(fake, "chmura").is_ok());
    }

    #[test]
    fn paths_are_normalized_and_mapped_to_remote_specs() {
        let (_dir, fs) = cloud();
        assert_eq!(fs.home().unwrap(), "/");
        assert_eq!(fs.canonical("/a/./b/../c/").unwrap(), "/a/c");
        assert_eq!(fs.canonical("/../..").unwrap(), "/");
        assert_eq!(fs.spec("/"), "chmura:");
        assert_eq!(
            fs.spec("/Dokumenty/zażółć.txt"),
            "chmura:Dokumenty/zażółć.txt"
        );
        assert_eq!(fs.protocol(), Protocol::Rclone);
    }

    #[test]
    fn folders_are_made_listed_renamed_and_removed() {
        let (_dir, fs) = cloud();
        fs.mkdir("/Zdjęcia").unwrap();
        fs.mkdir("/Zdjęcia/2026").unwrap();
        let root = fs.list("/").unwrap();
        assert_eq!(root.len(), 1);
        assert_eq!((root[0].name.as_str(), root[0].is_dir), ("Zdjęcia", true));
        assert!(fs.stat("/Zdjęcia/2026").unwrap().unwrap().is_dir);
        assert!(fs.stat("/Zdjęcia/brak").unwrap().is_none());
        assert!(
            fs.stat("/nie/ma/tego").unwrap().is_none(),
            "no folder above"
        );
        assert!(fs.stat("/").unwrap().unwrap().is_dir);
        assert!(fs.list("/brak").is_err());

        fs.rename("/Zdjęcia/2026", "/Zdjęcia/2027").unwrap();
        assert!(fs.stat("/Zdjęcia/2026").unwrap().is_none());
        fs.mkdir("/Zdjęcia/2028").unwrap();
        assert!(matches!(
            fs.rename("/Zdjęcia/2027", "/Zdjęcia/2028"),
            Err(RemoteError::Exists(_))
        ));
        fs.remove_dir("/Zdjęcia/2027").unwrap();
        assert!(fs.remove_dir("/Zdjęcia").is_err(), "not empty");
        assert!(fs.chmod("/Zdjęcia", 0o755).is_err());
    }

    #[test]
    fn files_go_up_and_down_without_overwriting_unless_told_to() {
        let (cloud_dir, fs) = cloud();
        let local = tempfile::tempdir().unwrap();
        let source = local.path().join("zażółć gęślą.txt");
        std::fs::write(&source, "treść pliku").unwrap();
        let progress = Progress::new();
        fs.upload(
            &source,
            "/zażółć gęślą.txt",
            &progress,
            TransferOptions::default(),
        )
        .unwrap();
        assert_eq!(progress.bytes_done(), "treść pliku".len() as u64);
        let listed = fs.list("/").unwrap();
        assert_eq!(listed[0].name, "zażółć gęślą.txt");
        assert_eq!(listed[0].size, "treść pliku".len() as u64);
        assert!(listed[0].modified.is_some());
        assert!(cloud_dir.path().join("chmura/zażółć gęślą.txt").is_file());

        // A second upload to the same name asks first.
        assert!(matches!(
            fs.upload(
                &source,
                "/zażółć gęślą.txt",
                &Progress::new(),
                TransferOptions::default()
            ),
            Err(RemoteError::Exists(_))
        ));
        std::fs::write(&source, "nowa treść").unwrap();
        fs.upload(
            &source,
            "/zażółć gęślą.txt",
            &Progress::new(),
            TransferOptions {
                overwrite: true,
                ..Default::default()
            },
        )
        .unwrap();

        let target = local.path().join("pobrany.txt");
        let progress = Progress::new();
        fs.download(
            "/zażółć gęślą.txt",
            &target,
            &progress,
            TransferOptions::default(),
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "nowa treść");
        assert_eq!(progress.bytes_done(), "nowa treść".len() as u64);
        assert!(!local.path().join("pobrany.txt.bpart").exists());
        assert!(matches!(
            fs.download(
                "/zażółć gęślą.txt",
                &target,
                &Progress::new(),
                TransferOptions::default()
            ),
            Err(RemoteError::Exists(_))
        ));
        assert!(fs
            .download(
                "/brak.txt",
                &local.path().join("x"),
                &Progress::new(),
                TransferOptions::default()
            )
            .is_err());
        assert!(!local.path().join("x.bpart").exists());

        fs.remove_file("/zażółć gęślą.txt").unwrap();
        assert!(fs.list("/").unwrap().is_empty());
        assert!(fs.remove_file("/zażółć gęślą.txt").is_err());
    }

    #[test]
    fn a_cancelled_download_leaves_nothing_behind() {
        let (_cloud_dir, fs) = cloud();
        let local = tempfile::tempdir().unwrap();
        let source = local.path().join("a.bin");
        std::fs::write(&source, vec![1u8; 1000]).unwrap();
        fs.upload(
            &source,
            "/a.bin",
            &Progress::new(),
            TransferOptions::default(),
        )
        .unwrap();
        let progress = Progress::new();
        progress.cancel();
        let target = local.path().join("b.bin");
        let result = fs.download("/a.bin", &target, &progress, TransferOptions::default());
        assert!(matches!(result, Err(RemoteError::Cancelled)), "{result:?}");
        assert!(!target.exists() && !local.path().join("b.bin.bpart").exists());
    }

    #[test]
    fn a_failure_is_explained_from_rclones_last_line() {
        let output = RcloneOutput {
            code: 1,
            stdout: Vec::new(),
            stderr: "2026/10/20 NOTICE: x\n2026/10/20 Failed to mkdir: quota exceeded\n".into(),
        };
        assert!(failure(&output).to_string().contains("quota exceeded"));
        let missing = RcloneOutput {
            code: 3,
            stderr: "directory not found".into(),
            ..Default::default()
        };
        assert!(failure(&missing)
            .to_string()
            .contains("nie ma takiego katalogu"));
    }
}
