//! In-process servers for tests (here and in the engine): an SSH server with an SFTP
//! subsystem and an FTP server, both over a local directory, so tests need no real server.
//! Not for production use.

mod ftp_server;
mod sftp_server;

pub use ftp_server::{start_ftp, FtpServer, FtpSettings, Tls};
pub use sftp_server::{start, write_file, Settings, TestServer};
