//! The interface every protocol implements, and the types it speaks.

use std::fmt::Debug;
use std::path::Path;
use std::time::SystemTime;

use bluepane_ops::Progress;

use crate::RemoteError;

/// One file or directory on the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    /// Unix permission bits when the server reports them.
    pub permissions: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TransferOptions {
    /// Continue an interrupted transfer from the `.bpart` file instead of starting over.
    pub resume: bool,
    /// Replace an existing target. Without it an existing target is an error.
    pub overwrite: bool,
}

/// What is spoken to the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Protocol {
    #[default]
    Sftp,
    Ftp,
    /// FTP with TLS started on the normal port (`AUTH TLS`).
    FtpsExplicit,
    /// FTP inside TLS from the first byte (usually port 990).
    FtpsImplicit,
}

impl Protocol {
    pub fn default_port(self) -> u16 {
        match self {
            Protocol::Sftp => 22,
            Protocol::Ftp | Protocol::FtpsExplicit => 21,
            Protocol::FtpsImplicit => 990,
        }
    }

    /// The word used in the sites file and on the command line.
    pub fn word(self) -> &'static str {
        match self {
            Protocol::Sftp => "sftp",
            Protocol::Ftp => "ftp",
            Protocol::FtpsExplicit => "ftps",
            Protocol::FtpsImplicit => "ftps-implicit",
        }
    }

    pub fn from_word(word: &str) -> Option<Protocol> {
        match word.to_ascii_lowercase().as_str() {
            "sftp" | "ssh" => Some(Protocol::Sftp),
            "ftp" => Some(Protocol::Ftp),
            "ftps" | "ftps-explicit" => Some(Protocol::FtpsExplicit),
            "ftps-implicit" => Some(Protocol::FtpsImplicit),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Protocol::Sftp => "SFTP (SSH)",
            Protocol::Ftp => "FTP (bez szyfrowania)",
            Protocol::FtpsExplicit => "FTPS (jawny, AUTH TLS)",
            Protocol::FtpsImplicit => "FTPS (niejawny, port 990)",
        }
    }
}

/// A logged-in server. Every call blocks the calling thread and may be made from any thread
/// (implementations serialize what the protocol cannot do in parallel).
///
/// Transfers go through `name.bpart`, which is renamed when complete, so a broken transfer
/// never leaves a half file under the real name and can be resumed.
pub trait RemoteFs: Send + Sync + Debug {
    fn protocol(&self) -> Protocol;

    /// The login directory.
    fn home(&self) -> Result<String, RemoteError>;

    /// A path with `..` resolved.
    fn canonical(&self, path: &str) -> Result<String, RemoteError>;

    /// The entries of a directory, without `.` and `..`.
    fn list(&self, path: &str) -> Result<Vec<RemoteEntry>, RemoteError>;

    /// One path, or `None` when it does not exist.
    fn stat(&self, path: &str) -> Result<Option<RemoteEntry>, RemoteError>;

    fn mkdir(&self, path: &str) -> Result<(), RemoteError>;
    fn remove_file(&self, path: &str) -> Result<(), RemoteError>;
    fn remove_dir(&self, path: &str) -> Result<(), RemoteError>;

    /// Renames on the server. Never replaces an existing target.
    fn rename(&self, from: &str, to: &str) -> Result<(), RemoteError>;

    fn download(
        &self,
        remote: &str,
        local: &Path,
        progress: &Progress,
        options: TransferOptions,
    ) -> Result<(), RemoteError>;

    fn upload(
        &self,
        local: &Path,
        remote: &str,
        progress: &Progress,
        options: TransferOptions,
    ) -> Result<(), RemoteError>;
}
