//! Archives: ZIP, TAR and TAR.GZ.
//!
//! Listing, extracting, testing and creating archives, optionally split into volumes
//! (`name.zip.001`, `name.zip.002`, ...) and, for ZIP, protected by a password (reading
//! ZipCrypto and AES, writing AES-256). TAR has no encryption of its own, so a password is
//! refused for it instead of being silently ignored.
//!
//! Writing files and conflict questions go through `bluepane_ops` (`Writer`, `Host`,
//! `Progress`), so extracting behaves like copying: chunked, cancellable, never overwriting
//! without an answer.

mod create;
mod format;
mod listing;
mod modify;
mod read;
mod seven;
mod sink;
mod split;
mod time;

use std::fmt;
use std::io;

use bluepane_ops::OpError;

pub use create::{create, CreateOptions, CreateReport, Level};
pub use format::{detect, volumes_of, Format};
pub use listing::{ArchiveEntry, Listing};
pub use modify::{modify, Change, ModifyReport};
pub use read::{extract, list, verify, ExtractReport, Selection, VerifyReport};

#[derive(Debug)]
pub enum ArchiveError {
    Io(io::Error),
    /// The operation needs a password and none was given.
    NeedsPassword,
    /// The given password does not open the archive.
    WrongPassword,
    /// The archive is damaged or not what its extension says.
    Corrupt(String),
    /// A feature this build does not handle (an unusual compression method, a password for
    /// TAR, ...).
    Unsupported(String),
    Cancelled,
    /// A failure reported by the file operations layer (existing target, bad name, ...).
    Op(String),
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArchiveError::Io(e) => write!(f, "{e}"),
            ArchiveError::NeedsPassword => write!(f, "archiwum jest zaszyfrowane: podaj hasło"),
            ArchiveError::WrongPassword => write!(f, "błędne hasło"),
            ArchiveError::Corrupt(what) => write!(f, "uszkodzone archiwum: {what}"),
            ArchiveError::Unsupported(what) => write!(f, "nieobsługiwane: {what}"),
            ArchiveError::Cancelled => write!(f, "anulowano"),
            ArchiveError::Op(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ArchiveError {}

impl From<io::Error> for ArchiveError {
    fn from(error: io::Error) -> Self {
        ArchiveError::Io(error)
    }
}

impl From<OpError> for ArchiveError {
    fn from(error: OpError) -> Self {
        match error {
            OpError::Cancelled => ArchiveError::Cancelled,
            OpError::Io(e) => ArchiveError::Io(e),
            other => ArchiveError::Op(other.to_string()),
        }
    }
}
