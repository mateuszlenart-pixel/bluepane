//! A stand-in for the `rclone` program over a local folder: each configured remote is a
//! sub-folder of `root`. It understands the few commands bluepane uses and answers with the
//! same output shapes and exit codes as rclone (3: directory not found, 4: file not found).
//! Not for production use.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::rclone::{RcloneOutput, RcloneRunner};
use crate::RemoteError;

#[derive(Debug)]
pub struct FakeRclone {
    root: PathBuf,
    remotes: Vec<(String, String)>,
}

impl FakeRclone {
    pub fn new(root: &Path, remotes: &[(&str, &str)]) -> FakeRclone {
        for (name, _) in remotes {
            let _ = std::fs::create_dir_all(root.join(name));
        }
        FakeRclone {
            root: root.to_path_buf(),
            remotes: remotes
                .iter()
                .map(|(n, k)| ((*n).to_string(), (*k).to_string()))
                .collect(),
        }
    }

    /// `name:path` as a local path when `name` is one of the remotes.
    fn local(&self, spec: &str) -> Option<PathBuf> {
        let (name, path) = spec.split_once(':')?;
        self.remotes
            .iter()
            .any(|(n, _)| n == name)
            .then(|| self.root.join(name).join(path))
    }

    fn done(code: i32, stdout: String, stderr: &str) -> Result<Option<RcloneOutput>, RemoteError> {
        Ok(Some(RcloneOutput {
            code,
            stdout: stdout.into_bytes(),
            stderr: stderr.to_string(),
        }))
    }
}

impl RcloneRunner for FakeRclone {
    fn run(
        &self,
        args: &[String],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<RcloneOutput>, RemoteError> {
        // Like the real program, a run that is being cancelled ends without an answer.
        if cancelled() {
            return Ok(None);
        }
        let words: Vec<&str> = args.iter().map(String::as_str).collect();
        let args: Vec<&str> = words
            .iter()
            .copied()
            .filter(|a| !a.starts_with("--"))
            .collect();
        match args.as_slice() {
            ["listremotes"] if !words.contains(&"--long") => Self::done(
                0,
                self.remotes
                    .iter()
                    .map(|(n, _)| format!("{n}:\n"))
                    .collect(),
                "",
            ),
            ["listremotes", ..] => Self::done(
                0,
                self.remotes
                    .iter()
                    .map(|(n, k)| format!("{n}:{}{k}\n", " ".repeat(12)))
                    .collect(),
                "",
            ),
            ["lsjson", spec] => {
                let Some(dir) = self.local(spec) else {
                    return Self::done(1, String::new(), "Failed: unknown remote");
                };
                let Ok(read) = std::fs::read_dir(&dir) else {
                    return Self::done(3, String::new(), "directory not found");
                };
                let mut items = Vec::new();
                for entry in read.flatten() {
                    let meta = entry.metadata().unwrap();
                    let secs = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_secs());
                    let modified = chrono::DateTime::from_timestamp(secs as i64, 0)
                        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
                        .unwrap_or_default();
                    let name = entry.file_name().to_string_lossy().into_owned();
                    items.push(serde_json::json!({
                        "Path": name,
                        "Name": name,
                        "Size": if meta.is_dir() { -1 } else { meta.len() as i64 },
                        "ModTime": modified,
                        "IsDir": meta.is_dir(),
                    }));
                }
                Self::done(0, serde_json::Value::Array(items).to_string(), "")
            }
            ["mkdir", spec] => match self.local(spec) {
                Some(dir) => match std::fs::create_dir_all(dir) {
                    Ok(()) => Self::done(0, String::new(), ""),
                    Err(e) => Self::done(1, String::new(), &e.to_string()),
                },
                None => Self::done(1, String::new(), "Failed: unknown remote"),
            },
            ["deletefile", spec] => match self.local(spec) {
                Some(file) if file.is_file() => match std::fs::remove_file(file) {
                    Ok(()) => Self::done(0, String::new(), ""),
                    Err(e) => Self::done(1, String::new(), &e.to_string()),
                },
                _ => Self::done(4, String::new(), "object not found"),
            },
            ["rmdir", spec] => match self.local(spec) {
                Some(dir) if dir.is_dir() => match std::fs::remove_dir(dir) {
                    Ok(()) => Self::done(0, String::new(), ""),
                    Err(e) => Self::done(1, String::new(), &format!("Failed to rmdir: {e}")),
                },
                _ => Self::done(3, String::new(), "directory not found"),
            },
            ["moveto", from, to] => match (self.local(from), self.local(to)) {
                (Some(from), Some(to)) if from.exists() => match std::fs::rename(from, to) {
                    Ok(()) => Self::done(0, String::new(), ""),
                    Err(e) => Self::done(1, String::new(), &e.to_string()),
                },
                _ => Self::done(4, String::new(), "object not found"),
            },
            ["copyto", from, to] => {
                // One side is a remote, the other a local path.
                let (source, target) = match (self.local(from), self.local(to)) {
                    (Some(remote), None) => (remote, PathBuf::from(to)),
                    (None, Some(remote)) => (PathBuf::from(from), remote),
                    _ => return Self::done(1, String::new(), "Failed: need one remote"),
                };
                if !source.is_file() {
                    return Self::done(4, String::new(), "object not found");
                }
                if let Some(parent) = target.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match std::fs::copy(&source, &target) {
                    Ok(_) => Self::done(0, String::new(), ""),
                    Err(e) => Self::done(1, String::new(), &e.to_string()),
                }
            }
            other => Self::done(1, String::new(), &format!("Failed: unsupported {other:?}")),
        }
    }
}
