use std::fmt;
use std::io;

/// Everything that can go wrong; the `Display` text is ready to show (Polish).
#[derive(Debug)]
pub enum RemoteError {
    /// The server could not be reached or the SSH handshake failed.
    Connect(String),
    /// The server refused the login.
    Auth(String),
    /// A key file could not be read or decrypted.
    Key(String),
    /// First contact with this host: show the fingerprint and ask.
    UnknownHost {
        host: String,
        port: u16,
        algorithm: String,
        fingerprint: String,
    },
    /// The host presents a different key than the one remembered: refuse.
    HostKeyChanged {
        host: String,
        port: u16,
        expected: String,
        got: String,
    },
    /// The server answered with an error (no such file, permission denied, ...).
    Remote(String),
    /// A local file problem.
    Io(io::Error),
    /// The target exists and may not be replaced.
    Exists(String),
    Cancelled,
}

impl fmt::Display for RemoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RemoteError::Connect(what) => write!(f, "nie udało się połączyć: {what}"),
            RemoteError::Auth(what) => write!(f, "logowanie odrzucone: {what}"),
            RemoteError::Key(what) => write!(f, "klucz SSH: {what}"),
            RemoteError::UnknownHost {
                host,
                port,
                algorithm,
                fingerprint,
            } => write!(
                f,
                "nieznany serwer {host}:{port} ({algorithm}), odcisk klucza {fingerprint}"
            ),
            RemoteError::HostKeyChanged {
                host,
                port,
                expected,
                got,
            } => write!(
                f,
                "UWAGA: klucz serwera {host}:{port} się zmienił (był {expected}, jest {got}); \
                 ktoś może podszywać się pod serwer, połączenie odrzucone"
            ),
            RemoteError::Remote(what) => write!(f, "serwer: {what}"),
            RemoteError::Io(e) => write!(f, "{e}"),
            RemoteError::Exists(path) => write!(f, "{path} już istnieje"),
            RemoteError::Cancelled => write!(f, "anulowano"),
        }
    }
}

impl std::error::Error for RemoteError {}

impl From<io::Error> for RemoteError {
    fn from(e: io::Error) -> Self {
        RemoteError::Io(e)
    }
}

impl From<russh_sftp::client::error::Error> for RemoteError {
    fn from(e: russh_sftp::client::error::Error) -> Self {
        RemoteError::Remote(e.to_string())
    }
}
