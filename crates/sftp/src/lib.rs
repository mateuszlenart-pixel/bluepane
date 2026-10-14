//! SFTP over SSH with a blocking API.
//!
//! The client is async inside (russh and russh-sftp on a tokio runtime that a
//! [`Connection`] owns), but nothing async leaks out: every call blocks the calling
//! thread, like the rest of bluepane's worker-thread operations, and honours
//! `bluepane_ops::Progress` for progress and cancellation.
//!
//! Host keys are checked against a [`KnownHosts`] list. An unknown host is *not* trusted
//! silently: [`Connection::connect`] fails with [`SftpError::UnknownHost`] carrying the
//! fingerprint, the caller shows it to the user and connects again with that fingerprint
//! as `trust`. A changed key is always refused.

mod connection;
mod error;
mod known_hosts;
mod transfer;

pub use connection::{Auth, ConnectParams, Connection, RemoteEntry};
pub use error::SftpError;
pub use known_hosts::{HostKeyStatus, KnownHosts};
pub use transfer::TransferOptions;
