//! In-process servers for tests (here and in the engine): an SSH server with an SFTP
//! subsystem and an FTP server, both over a local directory, so tests need no real server.
//! Not for production use.

mod ftp_server;
mod memory;
mod rclone;
mod sftp_server;
mod smb;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Every permission change a test server was asked for, by local path. The servers only
/// record it (a real `chmod` would behave differently on Windows), so tests are portable.
static CHMODS: Mutex<Vec<(PathBuf, u32)>> = Mutex::new(Vec::new());

pub(crate) fn record_chmod(path: PathBuf, mode: u32) {
    CHMODS.lock().unwrap().push((path, mode));
}

/// The last mode a test server was told to set on `path`, if any.
pub fn last_chmod(path: impl AsRef<Path>) -> Option<u32> {
    let path = path.as_ref();
    let list = CHMODS.lock().unwrap();
    list.iter().rev().find(|(p, _)| p == path).map(|(_, m)| *m)
}

static ACTIVE_TRANSFERS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

pub(crate) fn record_active_transfer(root: PathBuf) {
    ACTIVE_TRANSFERS.lock().unwrap().push(root);
}

/// How many data connections the FTP test server over `root` made to the client (active mode).
pub fn active_transfers(root: impl AsRef<Path>) -> usize {
    let root = root.as_ref();
    ACTIVE_TRANSFERS
        .lock()
        .unwrap()
        .iter()
        .filter(|r| *r == root)
        .count()
}

pub use ftp_server::{start_ftp, FtpServer, FtpSettings, Tls};
pub use memory::{FakeBitwarden, MemoryKeys, MemoryServices};
pub use rclone::FakeRclone;
pub use sftp_server::{start, write_file, Settings, TestServer};
pub use smb::FakeSmb;
