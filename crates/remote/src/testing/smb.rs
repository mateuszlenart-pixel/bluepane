//! A stand-in for the `smbclient` program over a local folder: each share is a sub-folder of
//! `root`. It understands the few commands bluepane uses (`-L`, and `ls`, `mkdir`, `del`,
//! `rmdir`, `rename`, `get`, `put` in a `-c` script) and answers in the same shapes, with
//! `NT_STATUS_*` errors. Not for production use.

use std::path::{Path, PathBuf};

use crate::smb::{SmbOutput, SmbRunner};
use crate::RemoteError;

#[derive(Debug)]
pub struct FakeSmb {
    root: PathBuf,
    shares: Vec<String>,
    /// The only login that works (user, password); `None` lets guests in.
    login: Option<(String, String)>,
}

impl FakeSmb {
    pub fn new(root: &Path, shares: &[&str], login: Option<(&str, &str)>) -> FakeSmb {
        for share in shares {
            let _ = std::fs::create_dir_all(root.join(share));
        }
        FakeSmb {
            root: root.to_path_buf(),
            shares: shares.iter().map(|s| (*s).to_string()).collect(),
            login: login.map(|(u, p)| (u.to_string(), p.to_string())),
        }
    }

    fn done(code: i32, stdout: String, stderr: &str) -> Result<Option<SmbOutput>, RemoteError> {
        Ok(Some(SmbOutput {
            code,
            stdout,
            stderr: stderr.to_string(),
        }))
    }

    /// The words of a command, honouring double quotes.
    fn words(command: &str) -> Vec<String> {
        let mut words = Vec::new();
        let mut current = String::new();
        let mut quoted = false;
        for c in command.chars() {
            match c {
                '"' => quoted = !quoted,
                c if c.is_whitespace() && !quoted => {
                    if !current.is_empty() {
                        words.push(std::mem::take(&mut current));
                    }
                }
                c => current.push(c),
            }
        }
        if !current.is_empty() {
            words.push(current);
        }
        words
    }

    fn inside(base: &Path, path: &str) -> PathBuf {
        path.split('\\')
            .filter(|p| !p.is_empty())
            .fold(base.to_path_buf(), |acc, part| acc.join(part))
    }

    fn command(base: &Path, command: &str) -> String {
        let words = Self::words(command);
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        let io_status = |e: std::io::Error| match e.kind() {
            std::io::ErrorKind::NotFound => "NT_STATUS_OBJECT_NAME_NOT_FOUND",
            std::io::ErrorKind::AlreadyExists => "NT_STATUS_OBJECT_NAME_COLLISION",
            _ => "NT_STATUS_DIRECTORY_NOT_EMPTY",
        };
        match words.as_slice() {
            ["ls", pattern] => {
                let dir = Self::inside(base, pattern.trim_end_matches('*'));
                let Ok(read) = std::fs::read_dir(&dir) else {
                    return format!("NT_STATUS_OBJECT_NAME_NOT_FOUND listing \\{pattern}\n");
                };
                let mut out = String::from(
                    "  .                                   D        0  Tue Oct 21 10:00:00 2026\n  ..                                  D        0  Tue Oct 21 10:00:00 2026\n",
                );
                for entry in read.flatten() {
                    let meta = entry.metadata().unwrap();
                    let (attr, size) = if meta.is_dir() {
                        ("D", 0)
                    } else {
                        ("A", meta.len())
                    };
                    out.push_str(&format!(
                        "  {}                    {attr}  {size:>7}  Mon Oct  6 09:05:01 2025\n",
                        entry.file_name().to_string_lossy()
                    ));
                }
                out.push_str("\n\t\t62914560 blocks of size 1024. 40000 blocks available\n");
                out
            }
            ["mkdir", path] => match std::fs::create_dir(Self::inside(base, path)) {
                Ok(()) => String::new(),
                Err(e) => format!("{} making remote directory \\{path}\n", io_status(e)),
            },
            ["del", path] => {
                let file = Self::inside(base, path);
                if !file.is_file() {
                    return format!(
                        "NT_STATUS_OBJECT_NAME_NOT_FOUND deleting remote file \\{path}\n"
                    );
                }
                match std::fs::remove_file(file) {
                    Ok(()) => String::new(),
                    Err(e) => format!("{} deleting remote file \\{path}\n", io_status(e)),
                }
            }
            ["rmdir", path] => match std::fs::remove_dir(Self::inside(base, path)) {
                Ok(()) => String::new(),
                Err(e) => format!("{} removing remote directory \\{path}\n", io_status(e)),
            },
            ["rename", from, to] => {
                let target = Self::inside(base, to);
                if target.exists() {
                    return format!(
                        "NT_STATUS_OBJECT_NAME_COLLISION renaming files \\{from} -> \\{to}\n"
                    );
                }
                match std::fs::rename(Self::inside(base, from), target) {
                    Ok(()) => String::new(),
                    Err(e) => format!("{} renaming files \\{from} -> \\{to}\n", io_status(e)),
                }
            }
            ["get", remote, local] => {
                let source = Self::inside(base, remote);
                if !source.is_file() {
                    return format!(
                        "NT_STATUS_OBJECT_NAME_NOT_FOUND opening remote file \\{remote}\n"
                    );
                }
                match std::fs::copy(source, local) {
                    Ok(n) => format!("getting file \\{remote} of size {n} as {local}\n"),
                    Err(e) => format!("{} opening local file {local}\n", io_status(e)),
                }
            }
            ["put", local, remote] => match std::fs::copy(local, Self::inside(base, remote)) {
                Ok(n) => format!("putting file {local} as \\{remote} ({n} bytes)\n"),
                Err(e) => format!("{} opening remote file \\{remote}\n", io_status(e)),
            },
            other => format!("unsupported command {other:?}\n"),
        }
    }
}

impl SmbRunner for FakeSmb {
    fn available(&self) -> bool {
        true
    }

    fn run(
        &self,
        args: &[String],
        password: Option<&str>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<SmbOutput>, RemoteError> {
        if cancelled() {
            return Ok(None);
        }
        // `-U user` or `-N` (guest).
        let user = args
            .iter()
            .position(|a| a == "-U")
            .and_then(|at| args.get(at + 1))
            .map_or("", String::as_str);
        let allowed = match &self.login {
            Some((u, p)) => user == u && password == Some(p.as_str()),
            None => true,
        };
        if !allowed {
            return Self::done(
                1,
                String::new(),
                "session setup failed: NT_STATUS_LOGON_FAILURE\n",
            );
        }
        if args.first().map(String::as_str) == Some("-L") {
            let mut out: String = self
                .shares
                .iter()
                .map(|s| format!("Disk|{s}|komentarz\n"))
                .collect();
            out.push_str("Disk|C$|Default share\nIPC|IPC$|IPC Service\nPrinter|drukarka|\n");
            return Self::done(0, out, "");
        }
        let share = args
            .first()
            .and_then(|a| a.rsplit('/').next())
            .unwrap_or_default();
        if !self.shares.iter().any(|s| s == share) {
            return Self::done(
                1,
                String::new(),
                "tree connect failed: NT_STATUS_BAD_NETWORK_NAME\n",
            );
        }
        let script = args
            .iter()
            .position(|a| a == "-c")
            .and_then(|at| args.get(at + 1))
            .map_or("", String::as_str);
        let base = self.root.join(share);
        let out: String = script
            .split(';')
            .map(|command| Self::command(&base, command.trim()))
            .collect();
        Self::done(0, out, "")
    }
}
