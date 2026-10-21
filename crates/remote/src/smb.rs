//! Windows / Samba shares (SMB) as a [`RemoteFs`], without mounting them in the system. bluepane
//! does not speak SMB: it asks the `smbclient` program (package `smbclient` on Linux). The
//! server is one site; its root lists the shares and a share is a folder, so `/dane/zdjęcia`
//! is the folder `zdjęcia` of the share `dane`.
//!
//! Every call is one run of `smbclient` with a `-c` script; how it is started is behind
//! [`SmbRunner`], so tests use a fake one and need neither smbclient nor a network. The
//! password goes in the `PASSWD` environment variable, never on the command line.
//!
//! Limits of the program that show here: `smbclient` splits its script at `;`, and a name with
//! `"` cannot be quoted, so such names are refused with a message; an interrupted transfer
//! starts over (there is no resuming).

use std::cell::Cell;
use std::fmt::Debug;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bluepane_ops::Progress;

use crate::{Protocol, RemoteEntry, RemoteError, RemoteFs, TransferOptions};

/// What one run of `smbclient` produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SmbOutput {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Starts `smbclient`.
pub trait SmbRunner: Send + Sync + Debug {
    /// Whether the program is installed.
    fn available(&self) -> bool;

    /// Runs it with these arguments and, when given, this password in `PASSWD`. `cancelled` is
    /// asked while it runs (about every 50 ms); when it says yes the run is stopped and the
    /// answer is `Ok(None)`.
    fn run(
        &self,
        args: &[String],
        password: Option<&str>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<SmbOutput>, RemoteError>;
}

/// What to say when the program is not there.
pub const SMB_NOT_INSTALLED: &str =
    "nie znaleziono programu smbclient: zainstaluj pakiet smbclient (Linux), żeby otwierać udziały bez montowania";

/// The `smbclient` program found through the search path.
#[derive(Debug, Clone, Default)]
pub struct SystemSmb;

impl SmbRunner for SystemSmb {
    fn available(&self) -> bool {
        bluepane_vfs::run_cancellable("smbclient", &["--version".to_string()], &|| false)
            .is_ok_and(|out| out.is_some_and(|o| o.status.success()))
    }

    fn run(
        &self,
        args: &[String],
        password: Option<&str>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<SmbOutput>, RemoteError> {
        let env: Vec<(String, String)> = password
            .filter(|p| !p.is_empty())
            .map(|p| vec![("PASSWD".to_string(), p.to_string())])
            .unwrap_or_default();
        match bluepane_vfs::run_cancellable_env("smbclient", args, &env, cancelled) {
            Ok(None) => Ok(None),
            Ok(Some(output)) => Ok(Some(SmbOutput {
                code: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(RemoteError::Connect(SMB_NOT_INSTALLED.into()))
            }
            Err(e) => Err(RemoteError::Connect(format!("smbclient: {e}"))),
        }
    }
}

/// The `NT_STATUS_*` code `smbclient` complained with, if it did.
fn status_of(text: &str) -> Option<&str> {
    text.lines().find_map(|line| {
        let line = line.trim();
        let at = line.find("NT_STATUS_")?;
        let start_of_line = at == 0;
        let after_failure = line[..at].ends_with("failed: ") || line[..at].ends_with("(Error ");
        if !(start_of_line || after_failure) {
            return None;
        }
        let end = line[at..]
            .find(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'))
            .map_or(line.len(), |n| at + n);
        Some(&line[at..end])
    })
}

/// A message for a status.
fn explain(status: &str, text: &str) -> RemoteError {
    match status {
        "NT_STATUS_LOGON_FAILURE" | "NT_STATUS_WRONG_PASSWORD" | "NT_STATUS_ACCOUNT_DISABLED" => {
            RemoteError::Auth("błędny login lub hasło do udziału".into())
        }
        "NT_STATUS_ACCESS_DENIED" | "NT_STATUS_NETWORK_ACCESS_DENIED" => {
            RemoteError::Remote("brak dostępu (uprawnienia albo wymagane logowanie)".into())
        }
        "NT_STATUS_BAD_NETWORK_NAME" => {
            RemoteError::Remote("na serwerze nie ma takiego udziału".into())
        }
        "NT_STATUS_OBJECT_NAME_NOT_FOUND"
        | "NT_STATUS_OBJECT_PATH_NOT_FOUND"
        | "NT_STATUS_NO_SUCH_FILE" => {
            RemoteError::Remote("nie ma takiego pliku ani katalogu".into())
        }
        "NT_STATUS_OBJECT_NAME_COLLISION" => RemoteError::Exists(first_line(text)),
        "NT_STATUS_DIRECTORY_NOT_EMPTY" => RemoteError::Remote("katalog nie jest pusty".into()),
        "NT_STATUS_HOST_UNREACHABLE"
        | "NT_STATUS_UNSUCCESSFUL"
        | "NT_STATUS_CONNECTION_REFUSED"
        | "NT_STATUS_IO_TIMEOUT"
        | "NT_STATUS_NETWORK_UNREACHABLE"
        | "NT_STATUS_BAD_NETWORK_PATH" => RemoteError::Connect(
            "serwer nie odpowiada na SMB (adres, zapora, udziały wyłączone?)".into(),
        ),
        other => RemoteError::Remote(format!("serwer odpowiedział: {other}")),
    }
}

fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_string()
}

/// A path with `.` and `..` resolved, always starting with `/`.
fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    let path = path.replace('\\', "/");
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

/// `/share/a/b` as the share and the path inside it with backslashes (empty for the share's
/// root). `None` for the server's root.
fn split(path: &str) -> Option<(String, String)> {
    let path = normalize(path);
    let rest = path.trim_start_matches('/');
    if rest.is_empty() {
        return None;
    }
    let (share, inside) = rest.split_once('/').unwrap_or((rest, ""));
    Some((share.to_string(), inside.replace('/', "\\")))
}

/// The shares in `smbclient -L -g` output (`Disk|name|comment`), without the hidden ones
/// (`C$`, `ADMIN$`) and without printers and pipes.
fn parse_shares(text: &str) -> Vec<RemoteEntry> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split('|');
            let (kind, name) = (fields.next()?, fields.next()?);
            (kind == "Disk" && !name.is_empty() && !name.ends_with('$')).then(|| RemoteEntry {
                name: name.to_string(),
                is_dir: true,
                is_symlink: false,
                size: 0,
                modified: None,
                permissions: None,
            })
        })
        .collect()
}

/// Takes the last whitespace-separated word off `text`: (what is left, the word).
fn pop_word(text: &str) -> (&str, &str) {
    let text = text.trim_end();
    match text.rfind(char::is_whitespace) {
        Some(at) => (&text[..at], &text[at + 1..]),
        None => ("", text),
    }
}

fn to_time(text: &str) -> Option<SystemTime> {
    use chrono::TimeZone;
    let naive = chrono::NaiveDateTime::parse_from_str(text, "%a %b %e %H:%M:%S %Y").ok()?;
    // The server prints its own local time; ours is the best guess at it.
    let local = chrono::Local.from_local_datetime(&naive).earliest()?;
    let secs = u64::try_from(local.timestamp()).ok()?;
    Some(UNIX_EPOCH + Duration::from_secs(secs))
}

/// The entries in `ls` output:
/// `  name with spaces      D        0  Tue Oct 21 10:00:00 2026`.
fn parse_listing(text: &str) -> Vec<RemoteEntry> {
    text.lines()
        .filter_map(|line| {
            // Entries start with two spaces; the summary line starts with tabs.
            if !line.starts_with("  ") || line.starts_with("  \t") {
                return None;
            }
            let (rest, year) = pop_word(line);
            let (rest, time) = pop_word(rest);
            let (rest, day) = pop_word(rest);
            let (rest, month) = pop_word(rest);
            let (rest, weekday) = pop_word(rest);
            let (rest, size) = pop_word(rest);
            let (name, attrs) = pop_word(rest);
            let size: u64 = size.parse().ok()?;
            if attrs.is_empty() || !attrs.chars().all(|c| c.is_ascii_alphabetic()) {
                return None;
            }
            let name = name.trim();
            if name.is_empty() || name == "." || name == ".." {
                return None;
            }
            Some(RemoteEntry {
                name: name.to_string(),
                is_dir: attrs.contains('D'),
                is_symlink: false,
                size: if attrs.contains('D') { 0 } else { size },
                modified: to_time(&format!("{weekday} {month} {day} {time} {year}")),
                permissions: None,
            })
        })
        .collect()
}

/// The same text in quotes for a script (fails for what cannot be quoted).
fn quoted(text: &str) -> Result<String, RemoteError> {
    if text.contains(['"', ';']) || text.chars().any(char::is_control) {
        return Err(RemoteError::Remote(format!(
            "smbclient nie przyjmie nazwy z cudzysłowem, średnikiem ani znakiem sterującym: {text}"
        )));
    }
    Ok(format!("\"{text}\""))
}

/// A server with SMB shares as a file system.
#[derive(Debug)]
pub struct SmbFs {
    runner: Arc<dyn SmbRunner>,
    host: String,
    port: u16,
    /// Empty for a guest login.
    user: String,
    password: Option<String>,
}

impl SmbFs {
    /// Logs in (by listing the shares) so a wrong address or password is reported right away.
    pub fn connect(
        runner: Arc<dyn SmbRunner>,
        host: &str,
        port: u16,
        user: &str,
        password: Option<String>,
    ) -> Result<SmbFs, RemoteError> {
        if host.is_empty()
            || host.contains(|c: char| c.is_whitespace() || "\\/\"'`;&|<>$".contains(c))
        {
            return Err(RemoteError::Connect(format!(
                "„{host}” nie jest poprawnym adresem serwera"
            )));
        }
        let fs = SmbFs {
            runner,
            host: host.to_string(),
            port,
            user: user.to_string(),
            password: password.filter(|p| !p.is_empty()),
        };
        fs.shares()?;
        Ok(fs)
    }

    fn auth_args(&self, args: &mut Vec<String>) {
        args.push("-p".into());
        args.push(self.port.to_string());
        if self.user.is_empty() {
            args.push("-N".into());
        } else {
            args.push("-U".into());
            args.push(self.user.clone());
        }
    }

    fn shares(&self) -> Result<Vec<RemoteEntry>, RemoteError> {
        let mut args = vec![
            "-L".to_string(),
            format!("//{}", self.host),
            "-g".to_string(),
        ];
        self.auth_args(&mut args);
        let out = self.exec(args, None, &|| {})?;
        Ok(parse_shares(&out.stdout))
    }

    /// Runs a script in a share and returns the output of a run that went well.
    fn script(
        &self,
        share: &str,
        script: &str,
        progress: Option<&Progress>,
        watch: &dyn Fn(),
    ) -> Result<SmbOutput, RemoteError> {
        let mut args = vec![format!("//{}/{share}", self.host)];
        self.auth_args(&mut args);
        args.push("-c".into());
        args.push(script.to_string());
        self.exec(args, progress, watch)
    }

    fn exec(
        &self,
        args: Vec<String>,
        progress: Option<&Progress>,
        watch: &dyn Fn(),
    ) -> Result<SmbOutput, RemoteError> {
        let out = self
            .runner
            .run(&args, self.password.as_deref(), &|| {
                watch();
                progress.is_some_and(Progress::is_cancelled)
            })?
            .ok_or(RemoteError::Cancelled)?;
        let all = format!("{}\n{}", out.stdout, out.stderr);
        if let Some(status) = status_of(&all) {
            return Err(explain(status, &all));
        }
        if out.code != 0 {
            return Err(RemoteError::Remote(format!(
                "smbclient: {}",
                first_line(&out.stderr)
                    .chars()
                    .take(200)
                    .collect::<String>()
            )));
        }
        Ok(out)
    }

    /// The share and the path inside it (checked to be quotable), for a path that must name
    /// something in a share.
    fn target(&self, path: &str) -> Result<(String, String), RemoteError> {
        let (share, inside) = split(path).ok_or_else(|| {
            RemoteError::Remote("to jest lista udziałów serwera, nie katalog".into())
        })?;
        if inside.is_empty() {
            return Err(RemoteError::Remote(
                "udziału nie da się tak potraktować (to jego główny katalog)".into(),
            ));
        }
        quoted(&inside)?;
        Ok((share, inside))
    }
}

impl RemoteFs for SmbFs {
    fn protocol(&self) -> Protocol {
        Protocol::Smb
    }

    fn home(&self) -> Result<String, RemoteError> {
        Ok("/".into())
    }

    fn canonical(&self, path: &str) -> Result<String, RemoteError> {
        Ok(normalize(path))
    }

    fn list(&self, path: &str) -> Result<Vec<RemoteEntry>, RemoteError> {
        let Some((share, inside)) = split(path) else {
            return self.shares();
        };
        let pattern = if inside.is_empty() {
            "*".to_string()
        } else {
            format!("{inside}\\*")
        };
        let out = self.script(&share, &format!("ls {}", quoted(&pattern)?), None, &|| {})?;
        Ok(parse_listing(&out.stdout))
    }

    fn stat(&self, path: &str) -> Result<Option<RemoteEntry>, RemoteError> {
        let path = normalize(path);
        let Some((parent, name)) = path.rsplit_once('/') else {
            return Ok(None);
        };
        if name.is_empty() {
            return Ok(Some(RemoteEntry {
                name: String::new(),
                is_dir: true,
                is_symlink: false,
                size: 0,
                modified: None,
                permissions: None,
            }));
        }
        match self.list(parent) {
            Ok(list) => Ok(list.into_iter().find(|e| e.name == name)),
            Err(RemoteError::Remote(text)) if text.starts_with("nie ma takiego") => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn mkdir(&self, path: &str) -> Result<(), RemoteError> {
        let (share, inside) = self.target(path)?;
        self.script(&share, &format!("mkdir {}", quoted(&inside)?), None, &|| {})
            .map(|_| ())
    }

    fn remove_file(&self, path: &str) -> Result<(), RemoteError> {
        let (share, inside) = self.target(path)?;
        self.script(&share, &format!("del {}", quoted(&inside)?), None, &|| {})
            .map(|_| ())
    }

    fn remove_dir(&self, path: &str) -> Result<(), RemoteError> {
        let (share, inside) = self.target(path)?;
        self.script(&share, &format!("rmdir {}", quoted(&inside)?), None, &|| {})
            .map(|_| ())
    }

    fn chmod(&self, _path: &str, _mode: u32) -> Result<(), RemoteError> {
        Err(RemoteError::Remote(
            "udziały SMB nie mają uprawnień unixowych (chmod)".into(),
        ))
    }

    fn rename(&self, from: &str, to: &str) -> Result<(), RemoteError> {
        let ((share, from_in), (to_share, to_in)) = (self.target(from)?, self.target(to)?);
        if share != to_share {
            return Err(RemoteError::Remote(
                "przenosić można tylko w obrębie jednego udziału (skopiuj i usuń)".into(),
            ));
        }
        if self.stat(to)?.is_some() {
            return Err(RemoteError::Exists(to.to_string()));
        }
        self.script(
            &share,
            &format!("rename {} {}", quoted(&from_in)?, quoted(&to_in)?),
            None,
            &|| {},
        )
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
        let (share, inside) = self.target(remote)?;
        let mut part_name = local.file_name().unwrap_or_default().to_os_string();
        part_name.push(".bpart");
        let part = local.with_file_name(part_name);
        let _ = std::fs::remove_file(&part);
        let counted = Cell::new(0u64);
        let result = self.script(
            &share,
            &format!(
                "get {} {}",
                quoted(&inside)?,
                quoted(&part.display().to_string())?
            ),
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
        if let Err(e) = result {
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
        let written = std::fs::metadata(&part)?.len();
        if written != entry.size {
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
        let (share, inside) = self.target(remote)?;
        let exists = self.stat(remote)?.is_some();
        if exists && !options.overwrite {
            return Err(RemoteError::Exists(remote.to_string()));
        }
        // Under a `.bpart` name first, renamed when whole.
        let part = format!("{inside}.bpart");
        let (inside_q, part_q) = (quoted(&inside)?, quoted(&part)?);
        let script = format!("put {} {part_q}", quoted(&local.display().to_string())?);
        if let Err(e) = self.script(&share, &script, Some(progress), &|| {}) {
            let _ = self.script(&share, &format!("del {part_q}"), None, &|| {});
            return Err(e);
        }
        if exists {
            self.script(&share, &format!("del {inside_q}"), None, &|| {})?;
        }
        self.script(&share, &format!("rename {part_q} {inside_q}"), None, &|| {})?;
        progress.add(size);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::FakeSmb;

    fn server() -> (tempfile::TempDir, SmbFs) {
        let dir = tempfile::tempdir().unwrap();
        let fake = FakeSmb::new(dir.path(), &["dane", "Zdjęcia"], Some(("ala", "tajne")));
        let fs = SmbFs::connect(Arc::new(fake), "nas", 445, "ala", Some("tajne".into())).unwrap();
        (dir, fs)
    }

    #[test]
    fn shares_are_read_without_the_hidden_and_the_printers() {
        let shares = parse_shares(
            "Disk|dane|Dokumenty\nDisk|C$|Default share\nIPC|IPC$|IPC Service\nPrinter|hp|drukarka\nDisk|Zdjęcia|\nDisk||x\nbroken\n",
        );
        let names: Vec<&str> = shares.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["dane", "Zdjęcia"]);
        assert!(shares.iter().all(|s| s.is_dir));
    }

    #[test]
    fn a_listing_is_read_from_the_right_so_names_may_hold_spaces_and_polish_letters() {
        let text = "  .                                   D        0  Tue Oct 21 10:00:00 2026\n\
                    \x20 ..                                  D        0  Tue Oct 21 10:00:00 2026\n\
                    \x20 zażółć gęślą  jaźń.txt              A     1234  Mon Oct  6 09:05:01 2025\n\
                    \x20 Zdjęcia                             D        0  Fri Jan  2 23:59:59 2026\n\
                    \x20 ukryty                              AH       7  Fri Jan  2 23:59:59 2026\n\
                    \n\
                    \t\t62914560 blocks of size 1024. 40000 blocks available\n";
        let list = parse_listing(text);
        let names: Vec<&str> = list.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["zażółć gęślą  jaźń.txt", "Zdjęcia", "ukryty"]);
        assert_eq!((list[0].is_dir, list[0].size), (false, 1234));
        assert!(list[0].modified.is_some());
        assert_eq!((list[1].is_dir, list[1].size), (true, 0));
    }

    #[test]
    fn statuses_become_messages_only_when_smbclient_complains() {
        assert_eq!(
            status_of("session setup failed: NT_STATUS_LOGON_FAILURE\n"),
            Some("NT_STATUS_LOGON_FAILURE")
        );
        assert_eq!(
            status_of("NT_STATUS_OBJECT_NAME_NOT_FOUND listing \\x\\*"),
            Some("NT_STATUS_OBJECT_NAME_NOT_FOUND")
        );
        assert_eq!(
            status_of("Connection to nas failed (Error NT_STATUS_HOST_UNREACHABLE)"),
            Some("NT_STATUS_HOST_UNREACHABLE")
        );
        // A file that merely has such a word in its name is not an error.
        assert_eq!(
            status_of("getting file \\NT_STATUS_notes.txt of size 3"),
            None
        );
        assert!(matches!(
            explain("NT_STATUS_LOGON_FAILURE", ""),
            RemoteError::Auth(_)
        ));
        assert!(matches!(
            explain(
                "NT_STATUS_OBJECT_NAME_COLLISION",
                "NT_STATUS_OBJECT_NAME_COLLISION making x"
            ),
            RemoteError::Exists(_)
        ));
    }

    #[test]
    fn paths_split_into_a_share_and_a_folder_inside_it() {
        assert_eq!(split("/"), None);
        assert_eq!(split("/dane"), Some(("dane".into(), String::new())));
        assert_eq!(
            split("/dane/a/./b/../Zdjęcia"),
            Some(("dane".into(), "a\\Zdjęcia".into()))
        );
        assert_eq!(normalize("/../.."), "/");
        assert!(quoted("a;b").is_err() && quoted("a\"b").is_err() && quoted("a\nb").is_err());
        assert_eq!(quoted("zażółć").unwrap(), "\"zażółć\"");
    }

    #[test]
    fn connecting_reports_a_bad_login_and_a_bad_address() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Arc::new(FakeSmb::new(dir.path(), &["dane"], Some(("ala", "tajne"))));
        let err = SmbFs::connect(fake.clone(), "nas", 445, "ala", Some("zle".into())).unwrap_err();
        assert!(matches!(err, RemoteError::Auth(_)), "{err}");
        let err = SmbFs::connect(fake.clone(), "nas", 445, "", None).unwrap_err();
        assert!(
            matches!(err, RemoteError::Auth(_)),
            "guests are refused: {err}"
        );
        for bad in ["", "a b", "a/b", "a;b"] {
            assert!(
                SmbFs::connect(fake.clone(), bad, 445, "ala", None).is_err(),
                "{bad}"
            );
        }
        assert!(SmbFs::connect(fake, "nas", 445, "ala", Some("tajne".into())).is_ok());
    }

    #[test]
    fn the_root_lists_shares_and_a_share_lists_its_files() {
        let (dir, fs) = server();
        std::fs::create_dir_all(dir.path().join("dane/Dokumenty")).unwrap();
        std::fs::write(dir.path().join("dane/zażółć gęślą.txt"), "treść").unwrap();
        let root: Vec<String> = fs.list("/").unwrap().into_iter().map(|e| e.name).collect();
        assert!(root.contains(&"dane".to_string()) && root.contains(&"Zdjęcia".to_string()));
        let mut inside = fs.list("/dane").unwrap();
        inside.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(inside.len(), 2);
        let file = inside.iter().find(|e| !e.is_dir).unwrap();
        assert_eq!((file.name.as_str(), file.size), ("zażółć gęślą.txt", 7));
        assert!(fs.list("/dane/Dokumenty").unwrap().is_empty());
        assert!(fs.list("/dane/nie-ma").is_err());
        assert_eq!(
            fs.stat("/dane/Dokumenty").unwrap().map(|e| e.is_dir),
            Some(true)
        );
        assert_eq!(fs.stat("/dane/nie-ma").unwrap(), None);
        assert!(fs.stat("/dane").unwrap().unwrap().is_dir);
    }

    #[test]
    fn folders_files_and_names_change_on_the_share() {
        let (dir, fs) = server();
        fs.mkdir("/dane/Nowy katalog").unwrap();
        assert!(dir.path().join("dane/Nowy katalog").is_dir());
        assert!(matches!(
            fs.mkdir("/dane/Nowy katalog"),
            Err(RemoteError::Exists(_))
        ));
        std::fs::write(dir.path().join("dane/a.txt"), "x").unwrap();
        fs.rename("/dane/a.txt", "/dane/Nowy katalog/łódź.txt")
            .unwrap();
        assert!(dir.path().join("dane/Nowy katalog/łódź.txt").exists());
        std::fs::write(dir.path().join("dane/b.txt"), "y").unwrap();
        assert!(matches!(
            fs.rename("/dane/b.txt", "/dane/Nowy katalog/łódź.txt"),
            Err(RemoteError::Exists(_))
        ));
        assert!(fs.rename("/dane/b.txt", "/Zdjęcia/b.txt").is_err());
        assert!(fs.remove_dir("/dane/Nowy katalog").is_err(), "not empty");
        fs.remove_file("/dane/Nowy katalog/łódź.txt").unwrap();
        fs.remove_dir("/dane/Nowy katalog").unwrap();
        assert!(!dir.path().join("dane/Nowy katalog").exists());
        assert!(fs.remove_file("/dane/nie-ma.txt").is_err());
        // The server's root and a share's root are not things to create or delete.
        assert!(fs.mkdir("/nowy-udzial").is_err());
        assert!(fs.remove_dir("/dane").is_err());
        assert!(fs.mkdir("/dane/a;b").is_err());
        assert!(fs.chmod("/dane/b.txt", 0o644).is_err());
    }

    #[test]
    fn files_go_both_ways_through_part_files_without_overwriting_unasked() {
        let (dir, fs) = server();
        let local = tempfile::tempdir().unwrap();
        let src = local.path().join("zażółć.txt");
        std::fs::write(&src, "treść pliku").unwrap();
        let progress = Progress::new();
        fs.upload(
            &src,
            "/dane/zażółć.txt",
            &progress,
            TransferOptions::default(),
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("dane/zażółć.txt")).unwrap(),
            "treść pliku"
        );
        assert!(!dir.path().join("dane/zażółć.txt.bpart").exists());
        assert!(matches!(
            fs.upload(
                &src,
                "/dane/zażółć.txt",
                &progress,
                TransferOptions::default()
            ),
            Err(RemoteError::Exists(_))
        ));
        std::fs::write(&src, "nowa").unwrap();
        fs.upload(
            &src,
            "/dane/zażółć.txt",
            &progress,
            TransferOptions {
                resume: false,
                overwrite: true,
            },
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("dane/zażółć.txt")).unwrap(),
            "nowa"
        );

        let back = local.path().join("pobrany.txt");
        fs.download(
            "/dane/zażółć.txt",
            &back,
            &progress,
            TransferOptions::default(),
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&back).unwrap(), "nowa");
        assert!(!local.path().join("pobrany.txt.bpart").exists());
        assert!(matches!(
            fs.download(
                "/dane/zażółć.txt",
                &back,
                &progress,
                TransferOptions::default()
            ),
            Err(RemoteError::Exists(_))
        ));
        assert!(fs
            .download(
                "/dane/nie-ma",
                &local.path().join("x"),
                &progress,
                TransferOptions::default()
            )
            .is_err());
        assert!(fs
            .download(
                "/dane",
                &local.path().join("y"),
                &progress,
                TransferOptions::default()
            )
            .is_err());
    }
}
