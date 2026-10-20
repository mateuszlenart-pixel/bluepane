//! FTP, FTPS (explicit and implicit) through suppaftp.
//!
//! An FTP session has one control connection that can do one thing at a time, so every
//! call takes a lock. Paths are always absolute (`/dir/file`): nothing depends on the
//! server's "current directory".

use std::fs;
use std::io::{Read, Write};
use std::net::ToSocketAddrs;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bluepane_ops::Progress;
use suppaftp::list::{File as ListedFile, ListParser};
use suppaftp::types::FileType;
use suppaftp::{FtpError, ImplFtpStream, RustlsConnector, TlsStream};
use suppaftp::{FtpStream, RustlsFtpStream};

use crate::fs::{Protocol, RemoteEntry, RemoteFs, TransferOptions};
use crate::known_hosts::{HostKeyStatus, KnownHosts};
use crate::tls::{pinned_config, Observed, CERT_KIND};
use crate::RemoteError;

const PART: &str = ".bpart";
const CHUNK: usize = 64 * 1024;

enum Session {
    Plain(FtpStream),
    Secure(RustlsFtpStream),
}

/// Runs the same code on either kind of stream.
macro_rules! on {
    ($session:expr, $s:ident => $body:expr) => {
        match $session {
            Session::Plain($s) => $body,
            Session::Secure($s) => $body,
        }
    };
}

pub struct FtpConnection {
    protocol: Protocol,
    host: String,
    port: u16,
    session: Mutex<Session>,
}

impl std::fmt::Debug for FtpConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "FtpConnection({:?} {}:{})",
            self.protocol, self.host, self.port
        )
    }
}

fn remote(error: FtpError) -> RemoteError {
    RemoteError::Remote(error.to_string())
}

fn secs(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// `/a/b/../c` -> `/a/c`
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

fn entry_of(file: &ListedFile) -> RemoteEntry {
    RemoteEntry {
        name: file.name().to_string(),
        is_dir: file.is_directory(),
        is_symlink: file.is_symlink(),
        size: if file.is_directory() {
            0
        } else {
            file.size() as u64
        },
        modified: Some(file.modified()),
        permissions: None,
    }
}

impl FtpConnection {
    /// Connects and logs in. For FTPS the server certificate is pinned: an unknown one
    /// fails with [`RemoteError::UnknownHost`] until confirmed (`trust` = its fingerprint).
    #[allow(clippy::too_many_arguments)] // one call site, all of it is login data
    pub fn connect(
        protocol: Protocol,
        host: &str,
        port: u16,
        user: &str,
        password: &str,
        timeout: Duration,
        known: &KnownHosts,
        trust: Option<&str>,
        active: bool,
    ) -> Result<FtpConnection, RemoteError> {
        let address = (host, port)
            .to_socket_addrs()
            .map_err(|e| RemoteError::Connect(format!("{host}: {e}")))?
            .next()
            .ok_or_else(|| RemoteError::Connect(format!("{host}: nie znaleziono adresu")))?;
        let connect_plain = || {
            FtpStream::connect_timeout(address, timeout)
                .map_err(|e| RemoteError::Connect(e.to_string()))
        };
        let mut session = match protocol {
            Protocol::Ftp => Session::Plain(with_mode(connect_plain()?, active)),
            Protocol::FtpsExplicit | Protocol::FtpsImplicit => {
                let (config, observed) = pinned_config(host, port, known, trust)
                    .map_err(|e| RemoteError::Connect(e.to_string()))?;
                if protocol == Protocol::FtpsImplicit {
                    // Look at the certificate first, on a connection of our own: a refused
                    // certificate must not leave the FTP library waiting on a dead handshake.
                    preflight(address, host, &config, &observed, timeout)
                        .map_err(|error| tls_failure(&error.to_string(), &observed, host, port))?;
                }
                let connector = RustlsConnector::from(config);
                let secured = if protocol == Protocol::FtpsExplicit {
                    RustlsFtpStream::connect_timeout(address, timeout)
                        .map_err(|e| RemoteError::Connect(e.to_string()))?
                        .into_secure(connector, host)
                } else {
                    RustlsFtpStream::connect_secure_implicit((host, port), connector, host)
                };
                match secured {
                    Ok(stream) => Session::Secure(with_mode(stream, active)),
                    Err(error) => {
                        return Err(tls_failure(&error.to_string(), &observed, host, port))
                    }
                }
            }
            Protocol::Sftp | Protocol::Rclone => {
                return Err(RemoteError::Connect("to nie jest protokół FTP".into()));
            }
        };
        on!(&mut session, s => {
            s.login(user, password)
                .map_err(|e| RemoteError::Auth(e.to_string()))?;
            s.transfer_type(FileType::Binary).map_err(remote)?;
        });
        Ok(FtpConnection {
            protocol,
            host: host.to_string(),
            port,
            session: Mutex::new(session),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Session> {
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// How long the server gets to connect back to us in active mode.
const ACTIVE_ACCEPT_TIMEOUT: Duration = Duration::from_secs(15);

/// Passive mode (the client connects for data) is the default; active mode makes the server
/// connect to a port we open, which needs this machine to be reachable from the server.
fn with_mode<T: suppaftp::TlsStream>(
    stream: suppaftp::ImplFtpStream<T>,
    active: bool,
) -> suppaftp::ImplFtpStream<T> {
    if active {
        stream.active_mode(ACTIVE_ACCEPT_TIMEOUT)
    } else {
        stream
    }
}

/// A failed TLS start: when the certificate was refused the user needs to see its fingerprint.
fn tls_failure(
    error: &str,
    observed: &std::sync::Arc<Mutex<Option<Observed>>>,
    host: &str,
    port: u16,
) -> RemoteError {
    let seen = observed.lock().unwrap_or_else(|e| e.into_inner()).clone();
    match seen {
        Some(Observed {
            fingerprint,
            status: HostKeyStatus::Unknown,
        }) => RemoteError::UnknownHost {
            host: host.to_string(),
            port,
            algorithm: CERT_KIND.to_string(),
            fingerprint,
        },
        Some(Observed {
            fingerprint,
            status: HostKeyStatus::Changed { expected },
        }) => RemoteError::HostKeyChanged {
            host: host.to_string(),
            port,
            expected,
            got: fingerprint,
        },
        _ => RemoteError::Connect(error.to_string()),
    }
}

/// Starts TLS on a throwaway connection and reads the greeting, so that the certificate is
/// checked (and its fingerprint recorded) before the real connection is made.
fn preflight(
    address: std::net::SocketAddr,
    host: &str,
    config: &std::sync::Arc<rustls::ClientConfig>,
    _observed: &std::sync::Arc<Mutex<Option<Observed>>>,
    timeout: Duration,
) -> std::io::Result<()> {
    let tcp = std::net::TcpStream::connect_timeout(&address, timeout)?;
    tcp.set_read_timeout(Some(timeout))?;
    tcp.set_write_timeout(Some(timeout))?;
    let name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()))?;
    let connection = rustls::ClientConnection::new(config.clone(), name)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    let mut stream = rustls::StreamOwned::new(connection, tcp);
    let mut first = [0u8; 1];
    stream.read_exact(&mut first)
}

/// The entries of a directory: MLSD when the server has it, LIST otherwise.
fn list_dir<T: TlsStream>(
    s: &mut ImplFtpStream<T>,
    path: &str,
) -> Result<Vec<RemoteEntry>, RemoteError> {
    let mut entries = Vec::new();
    match s.mlsd(Some(path)) {
        Ok(lines) => {
            for line in lines {
                if let Ok(file) = ListParser::parse_mlsd(&line) {
                    entries.push(entry_of(&file));
                }
            }
        }
        Err(_) => {
            for line in s.list(Some(path)).map_err(remote)? {
                if let Ok(file) =
                    ListParser::parse_posix(&line).or_else(|_| ListParser::parse_dos(&line))
                {
                    entries.push(entry_of(&file));
                }
            }
        }
    }
    entries.retain(|e| e.name != "." && e.name != ".." && !e.name.is_empty());
    Ok(entries)
}

fn stat_path<T: TlsStream>(
    s: &mut ImplFtpStream<T>,
    path: &str,
) -> Result<Option<RemoteEntry>, RemoteError> {
    let name = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_string();
    if path == "/" {
        return Ok(Some(RemoteEntry {
            name: "/".into(),
            is_dir: true,
            is_symlink: false,
            size: 0,
            modified: None,
            permissions: None,
        }));
    }
    if let Ok(size) = s.size(path) {
        let modified = s
            .mdtm(path)
            .ok()
            .map(|t| UNIX_EPOCH + Duration::from_secs(t.and_utc().timestamp().max(0) as u64));
        return Ok(Some(RemoteEntry {
            name,
            is_dir: false,
            is_symlink: false,
            size: size as u64,
            modified,
            permissions: None,
        }));
    }
    // Not a file we can measure: a directory (it can be entered) or nothing.
    if s.cwd(path).is_ok() {
        return Ok(Some(RemoteEntry {
            name,
            is_dir: true,
            is_symlink: false,
            size: 0,
            modified: None,
            permissions: None,
        }));
    }
    Ok(None)
}

fn download_file<T: TlsStream>(
    s: &mut ImplFtpStream<T>,
    remote_path: &str,
    local: &Path,
    progress: &Progress,
    options: TransferOptions,
) -> Result<(), RemoteError> {
    if !options.overwrite && fs::symlink_metadata(local).is_ok() {
        return Err(RemoteError::Exists(local.display().to_string()));
    }
    let size = s.size(remote_path).map_err(remote)? as u64;
    let modified = s
        .mdtm(remote_path)
        .ok()
        .map(|t| UNIX_EPOCH + Duration::from_secs(t.and_utc().timestamp().max(0) as u64));
    let mut part_name = local.file_name().unwrap_or_default().to_os_string();
    part_name.push(PART);
    let part = local.with_file_name(part_name);
    let mut offset = 0;
    if options.resume {
        if let Ok(existing) = fs::metadata(&part) {
            // Only a partial file newer than the source can be continued.
            let fresh = match (existing.modified(), modified) {
                (Ok(part_time), Some(source_time)) => secs(part_time) >= secs(source_time),
                _ => false,
            };
            if existing.len() < size && fresh {
                offset = existing.len();
            }
        }
    }
    if offset > 0 {
        s.resume_transfer(offset as usize).map_err(remote)?;
    }
    let mut stream = s.retr_as_stream(remote_path).map_err(remote)?;
    let mut target = if offset > 0 {
        fs::OpenOptions::new().append(true).open(&part)?
    } else {
        fs::File::create(&part)?
    };
    progress.add(offset);
    let mut buffer = vec![0u8; CHUNK];
    loop {
        if progress.is_cancelled() {
            drop(stream);
            return Err(RemoteError::Cancelled);
        }
        let n = stream.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        target.write_all(&buffer[..n])?;
        progress.add(n as u64);
    }
    stream.finish().map_err(remote)?;
    drop(target);
    let written = fs::metadata(&part)?.len();
    if written != size {
        return Err(RemoteError::Remote(format!(
            "pobrano {written} z {size} bajtów (połączenie przerwane?)"
        )));
    }
    if options.overwrite && fs::symlink_metadata(local).is_ok() {
        fs::remove_file(local)?;
    }
    fs::rename(&part, local)?;
    if let Some(time) = modified {
        if let Ok(file) = fs::OpenOptions::new().write(true).open(local) {
            let _ = file.set_modified(time);
        }
    }
    Ok(())
}

fn upload_file<T: TlsStream>(
    s: &mut ImplFtpStream<T>,
    local: &Path,
    remote_path: &str,
    progress: &Progress,
    options: TransferOptions,
) -> Result<(), RemoteError> {
    let meta = fs::metadata(local)?;
    let size = meta.len();
    if !options.overwrite && stat_path(s, remote_path)?.is_some() {
        return Err(RemoteError::Exists(remote_path.to_string()));
    }
    let part = format!("{remote_path}{PART}");
    let mut offset = 0;
    if options.resume {
        if let Some(existing) = stat_path(s, &part)? {
            let fresh = match (existing.modified, meta.modified()) {
                (Some(part_time), Ok(source_time)) => secs(part_time) >= secs(source_time),
                _ => false,
            };
            if existing.size < size && fresh {
                offset = existing.size;
            }
        }
    }
    let mut source = fs::File::open(local)?;
    let mut stream = if offset > 0 {
        std::io::Seek::seek(&mut source, std::io::SeekFrom::Start(offset))?;
        s.append_with_stream(&part).map_err(remote)?
    } else {
        s.put_with_stream(&part).map_err(remote)?
    };
    progress.add(offset);
    let mut buffer = vec![0u8; CHUNK];
    loop {
        if progress.is_cancelled() {
            drop(stream);
            return Err(RemoteError::Cancelled);
        }
        let n = source.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        stream.write_all(&buffer[..n]).map_err(RemoteError::Io)?;
        progress.add(n as u64);
    }
    stream.finish().map_err(remote)?;
    let written = s.size(&part).map_err(remote)? as u64;
    if written != size {
        return Err(RemoteError::Remote(format!(
            "wysłano {written} z {size} bajtów"
        )));
    }
    if options.overwrite && stat_path(s, remote_path)?.is_some() {
        s.rm(remote_path).map_err(remote)?;
    }
    s.rename(part.as_str(), remote_path).map_err(remote)?;
    Ok(())
}

impl RemoteFs for FtpConnection {
    fn protocol(&self) -> Protocol {
        self.protocol
    }

    fn home(&self) -> Result<String, RemoteError> {
        on!(&mut *self.lock(), s => s.pwd().map_err(remote))
    }

    fn canonical(&self, path: &str) -> Result<String, RemoteError> {
        Ok(normalize(path))
    }

    fn list(&self, path: &str) -> Result<Vec<RemoteEntry>, RemoteError> {
        on!(&mut *self.lock(), s => list_dir(s, path))
    }

    fn stat(&self, path: &str) -> Result<Option<RemoteEntry>, RemoteError> {
        on!(&mut *self.lock(), s => stat_path(s, path))
    }

    fn mkdir(&self, path: &str) -> Result<(), RemoteError> {
        on!(&mut *self.lock(), s => s.mkdir(path).map_err(remote))
    }

    fn remove_file(&self, path: &str) -> Result<(), RemoteError> {
        on!(&mut *self.lock(), s => s.rm(path).map_err(remote))
    }

    fn remove_dir(&self, path: &str) -> Result<(), RemoteError> {
        on!(&mut *self.lock(), s => s.rmdir(path).map_err(remote))
    }

    fn chmod(&self, path: &str, mode: u32) -> Result<(), RemoteError> {
        // `SITE CHMOD` is not part of the FTP standard, but nearly every Unix server has it.
        on!(&mut *self.lock(), s => s
            .custom_command(
                format!("SITE CHMOD {:o} {path}", mode & 0o7777),
                &[suppaftp::Status::CommandOk],
            )
            .map(|_| ())
            .map_err(remote))
    }

    fn rename(&self, from: &str, to: &str) -> Result<(), RemoteError> {
        on!(&mut *self.lock(), s => {
            if stat_path(s, to)?.is_some() {
                return Err(RemoteError::Exists(to.to_string()));
            }
            s.rename(from, to).map_err(remote)
        })
    }

    fn download(
        &self,
        remote_path: &str,
        local: &Path,
        progress: &Progress,
        options: TransferOptions,
    ) -> Result<(), RemoteError> {
        on!(&mut *self.lock(), s => download_file(s, remote_path, local, progress, options))
    }

    fn upload(
        &self,
        local: &Path,
        remote_path: &str,
        progress: &Progress,
        options: TransferOptions,
    ) -> Result<(), RemoteError> {
        on!(&mut *self.lock(), s => upload_file(s, local, remote_path, progress, options))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_normalized_without_asking_the_server() {
        assert_eq!(normalize("/a/b/../c"), "/a/c");
        assert_eq!(normalize("/a/./b/"), "/a/b");
        assert_eq!(normalize("/../.."), "/");
        assert_eq!(normalize("/"), "/");
    }
}
