use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client;
use russh::keys::{load_secret_key, HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh_sftp::client::SftpSession;
use tokio::runtime::Runtime;

use crate::fs::{Protocol, RemoteEntry, RemoteFs, TransferOptions};
use crate::known_hosts::{HostKeyStatus, KnownHosts};
use crate::RemoteError;

/// How to log in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Auth {
    Password(String),
    /// A private key file (OpenSSH or PEM format), with its passphrase if it has one.
    Key {
        path: PathBuf,
        passphrase: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectParams {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: Auth,
    /// Longest wait for the connection and the login.
    pub timeout: Duration,
}

impl ConnectParams {
    pub fn new(host: impl Into<String>, user: impl Into<String>, auth: Auth) -> ConnectParams {
        ConnectParams {
            host: host.into(),
            port: 22,
            user: user.into(),
            auth,
            timeout: Duration::from_secs(20),
        }
    }
}

/// What the key check saw, so a refused connection can say why.
#[derive(Debug, Clone)]
struct Observed {
    algorithm: String,
    fingerprint: String,
    status: HostKeyStatus,
}

struct Handler {
    host: String,
    port: u16,
    known: KnownHosts,
    trust: Option<String>,
    observed: Arc<Mutex<Option<Observed>>>,
}

impl client::Handler for Handler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let data = match key {
            PublicKeyOrCertificate::PublicKey { key, .. } => key.key_data().clone(),
            PublicKeyOrCertificate::Certificate(cert) => cert.public_key().clone(),
        };
        let algorithm = data.algorithm().to_string();
        let fingerprint = data.fingerprint(HashAlg::Sha256).to_string();
        let status = self
            .known
            .status(&self.host, self.port, &algorithm, &fingerprint);
        let trusted = match &status {
            HostKeyStatus::Known => true,
            HostKeyStatus::Unknown => self.trust.as_deref() == Some(fingerprint.as_str()),
            HostKeyStatus::Changed { .. } => false,
        };
        *self.observed.lock().unwrap_or_else(|e| e.into_inner()) = Some(Observed {
            algorithm,
            fingerprint,
            status,
        });
        Ok(trusted)
    }
}

/// A logged-in SFTP session.
pub struct SftpConnection {
    pub(crate) rt: Runtime,
    pub(crate) sftp: SftpSession,
    // Keeps the SSH connection alive.
    _handle: client::Handle<Handler>,
    pub host: String,
    pub port: u16,
}

impl std::fmt::Debug for SftpConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SftpConnection({}:{})", self.host, self.port)
    }
}

fn connect_error(
    error: &russh::Error,
    observed: Option<Observed>,
    host: &str,
    port: u16,
) -> RemoteError {
    if let Some(seen) = observed {
        match seen.status {
            HostKeyStatus::Unknown => {
                return RemoteError::UnknownHost {
                    host: host.to_string(),
                    port,
                    algorithm: seen.algorithm,
                    fingerprint: seen.fingerprint,
                };
            }
            HostKeyStatus::Changed { expected } => {
                return RemoteError::HostKeyChanged {
                    host: host.to_string(),
                    port,
                    expected,
                    got: seen.fingerprint,
                };
            }
            HostKeyStatus::Known => {}
        }
    }
    RemoteError::Connect(error.to_string())
}

impl SftpConnection {
    /// Connects and logs in. `known` is the list of remembered keys; `trust` is the
    /// fingerprint of an unknown key that the user has just confirmed.
    pub fn connect(
        params: &ConnectParams,
        known: &KnownHosts,
        trust: Option<&str>,
    ) -> Result<SftpConnection, RemoteError> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()?;
        let observed = Arc::new(Mutex::new(None));
        let handler = Handler {
            host: params.host.clone(),
            port: params.port,
            known: known.clone(),
            trust: trust.map(str::to_string),
            observed: observed.clone(),
        };
        let params_ = params.clone();
        let (handle, sftp) = rt.block_on(async move {
            let config = Arc::new(client::Config {
                inactivity_timeout: Some(Duration::from_secs(120)),
                ..client::Config::default()
            });
            let address = (params_.host.as_str(), params_.port);
            let mut handle =
                tokio::time::timeout(params_.timeout, client::connect(config, address, handler))
                    .await
                    .map_err(|_| RemoteError::Connect("przekroczono czas oczekiwania".into()))?
                    .map_err(|e| {
                        let seen = observed.lock().unwrap_or_else(|e| e.into_inner()).clone();
                        connect_error(&e, seen, &params_.host, params_.port)
                    })?;

            let result = match &params_.auth {
                Auth::Password(password) => handle
                    .authenticate_password(params_.user.clone(), password.clone())
                    .await
                    .map_err(|e| RemoteError::Auth(e.to_string()))?,
                Auth::Key { path, passphrase } => {
                    let key = load_secret_key(path, passphrase.as_deref())
                        .map_err(|e| RemoteError::Key(format!("{}: {e}", path.display())))?;
                    let hash = handle
                        .best_supported_rsa_hash()
                        .await
                        .map_err(|e| RemoteError::Auth(e.to_string()))?
                        .flatten();
                    handle
                        .authenticate_publickey(
                            params_.user.clone(),
                            PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                        )
                        .await
                        .map_err(|e| RemoteError::Auth(e.to_string()))?
                }
            };
            if !result.success() {
                return Err(RemoteError::Auth(
                    "błędne hasło, klucz albo nazwa użytkownika".into(),
                ));
            }
            let channel = handle
                .channel_open_session()
                .await
                .map_err(|e| RemoteError::Connect(e.to_string()))?;
            channel
                .request_subsystem(true, "sftp")
                .await
                .map_err(|e| RemoteError::Connect(format!("serwer nie udostępnia SFTP: {e}")))?;
            let sftp = SftpSession::new(channel.into_stream())
                .await
                .map_err(|e| RemoteError::Connect(format!("SFTP: {e}")))?;
            Ok::<_, RemoteError>((handle, sftp))
        })?;
        Ok(SftpConnection {
            rt,
            sftp,
            _handle: handle,
            host: params.host.clone(),
            port: params.port,
        })
    }

    pub(crate) fn run<T>(&self, future: impl Future<Output = T>) -> T {
        self.rt.block_on(future)
    }

    /// The server's idea of the login directory (`.` resolved).
    pub fn home(&self) -> Result<String, RemoteError> {
        Ok(self.run(self.sftp.canonicalize("."))?)
    }

    /// Resolves `..` and symlinks in a path.
    pub fn canonical(&self, path: &str) -> Result<String, RemoteError> {
        Ok(self.run(self.sftp.canonicalize(path))?)
    }

    /// The entries of a directory, without `.` and `..`, in the server's order.
    pub fn list(&self, path: &str) -> Result<Vec<RemoteEntry>, RemoteError> {
        self.run(async {
            let mut out = Vec::new();
            for item in self.sftp.read_dir(path).await? {
                let name = item.file_name();
                if name == "." || name == ".." {
                    continue;
                }
                let kind = item.file_type();
                let meta = item.metadata();
                let mut entry = RemoteEntry {
                    is_dir: kind.is_dir(),
                    is_symlink: kind.is_symlink(),
                    size: if kind.is_dir() { 0 } else { meta.len() },
                    modified: meta.modified().ok(),
                    permissions: meta.permissions,
                    name,
                };
                if entry.is_symlink {
                    // A link to a directory is browsed like a directory.
                    let target = format!("{}/{}", path.trim_end_matches('/'), entry.name);
                    if let Ok(real) = self.sftp.metadata(target).await {
                        entry.is_dir = real.file_type().is_dir();
                        if !entry.is_dir {
                            entry.size = real.len();
                        }
                    }
                }
                out.push(entry);
            }
            Ok(out)
        })
    }

    /// One path, or `None` when it does not exist.
    pub fn stat(&self, path: &str) -> Result<Option<RemoteEntry>, RemoteError> {
        self.run(self.stat_async(path))
    }

    pub(crate) async fn stat_async(&self, path: &str) -> Result<Option<RemoteEntry>, RemoteError> {
        if !self.sftp.try_exists(path).await? {
            return Ok(None);
        }
        let meta = self.sftp.metadata(path).await?;
        let is_dir = meta.file_type().is_dir();
        Ok(Some(RemoteEntry {
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            is_dir,
            is_symlink: false,
            size: if is_dir { 0 } else { meta.len() },
            modified: meta.modified().ok(),
            permissions: meta.permissions,
        }))
    }

    pub fn mkdir(&self, path: &str) -> Result<(), RemoteError> {
        Ok(self.run(self.sftp.create_dir(path))?)
    }

    pub fn remove_file(&self, path: &str) -> Result<(), RemoteError> {
        Ok(self.run(self.sftp.remove_file(path))?)
    }

    pub fn remove_dir(&self, path: &str) -> Result<(), RemoteError> {
        Ok(self.run(self.sftp.remove_dir(path))?)
    }

    /// Renames within the server. Never replaces an existing target.
    pub fn rename(&self, from: &str, to: &str) -> Result<(), RemoteError> {
        if self.stat(to)?.is_some() {
            return Err(RemoteError::Exists(to.to_string()));
        }
        Ok(self.run(self.sftp.rename(from, to))?)
    }

    /// Ends the session.
    pub fn close(self) {
        let _ = self.rt.block_on(self.sftp.close());
    }
}

impl RemoteFs for SftpConnection {
    fn protocol(&self) -> Protocol {
        Protocol::Sftp
    }

    fn home(&self) -> Result<String, RemoteError> {
        SftpConnection::home(self)
    }

    fn canonical(&self, path: &str) -> Result<String, RemoteError> {
        SftpConnection::canonical(self, path)
    }

    fn list(&self, path: &str) -> Result<Vec<RemoteEntry>, RemoteError> {
        SftpConnection::list(self, path)
    }

    fn stat(&self, path: &str) -> Result<Option<RemoteEntry>, RemoteError> {
        SftpConnection::stat(self, path)
    }

    fn mkdir(&self, path: &str) -> Result<(), RemoteError> {
        SftpConnection::mkdir(self, path)
    }

    fn remove_file(&self, path: &str) -> Result<(), RemoteError> {
        SftpConnection::remove_file(self, path)
    }

    fn remove_dir(&self, path: &str) -> Result<(), RemoteError> {
        SftpConnection::remove_dir(self, path)
    }

    fn rename(&self, from: &str, to: &str) -> Result<(), RemoteError> {
        SftpConnection::rename(self, from, to)
    }

    fn download(
        &self,
        remote: &str,
        local: &std::path::Path,
        progress: &bluepane_ops::Progress,
        options: TransferOptions,
    ) -> Result<(), RemoteError> {
        SftpConnection::download(self, remote, local, progress, options)
    }

    fn upload(
        &self,
        local: &std::path::Path,
        remote: &str,
        progress: &bluepane_ops::Progress,
        options: TransferOptions,
    ) -> Result<(), RemoteError> {
        SftpConnection::upload(self, local, remote, progress, options)
    }
}
