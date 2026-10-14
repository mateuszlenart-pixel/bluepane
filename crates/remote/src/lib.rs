//! Remote file systems behind one blocking interface ([`RemoteFs`]): SFTP over SSH and
//! FTP / FTPS. Saved sites ([`Sites`]) say which one and how to log in.
//!
//! SFTP over SSH with a blocking API.
//!
//! The client is async inside (russh and russh-sftp on a tokio runtime that a
//! [`Connection`] owns), but nothing async leaks out: every call blocks the calling
//! thread, like the rest of bluepane's worker-thread operations, and honours
//! `bluepane_ops::Progress` for progress and cancellation.
//!
//! Host keys are checked against a [`KnownHosts`] list. An unknown host is *not* trusted
//! silently: [`Connection::connect`] fails with [`RemoteError::UnknownHost`] carrying the
//! fingerprint, the caller shows it to the user and connects again with that fingerprint
//! as `trust`. A changed key is always refused.

mod error;
mod fs;
mod ftp;
mod known_hosts;
mod secrets;
mod sftp;
mod sftp_transfer;
mod sites;
mod tls;

#[doc(hidden)]
pub mod testing;

pub use error::RemoteError;
pub use fs::{Protocol, RemoteEntry, RemoteFs, TransferOptions};
pub use known_hosts::{HostKeyStatus, KnownHosts};
pub use secrets::{
    base64, bitwarden_available, bitwarden_create, bitwarden_responds, bitwarden_secret,
    json_string, BitwardenCli, CliOutput, KeyStore, SecretServices, SystemBitwarden, SystemKeyring,
    SystemServices, KEYRING_SERVICE,
};
pub use sftp::{Auth, ConnectParams, SftpConnection};
pub use sites::{SecretStore, Site, Sites};
