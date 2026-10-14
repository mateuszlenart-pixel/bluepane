//! Where the passwords of saved sites can live besides the sites file: the system password
//! store (Secret Service, Windows Credential Manager) and Bitwarden (through its `bw`
//! command line tool). Both sit behind small traits so tests need neither.

use std::fmt::Debug;
use std::io;
use std::process::Command;

use crate::RemoteError;

/// The name under which bluepane's entries appear in the system password store.
pub const KEYRING_SERVICE: &str = "bluepane";

/// A password store of the operating system.
pub trait KeyStore: Send + Sync + Debug {
    fn get(&self, service: &str, user: &str) -> Result<Option<String>, String>;
    fn set(&self, service: &str, user: &str, password: &str) -> Result<(), String>;
    fn delete(&self, service: &str, user: &str) -> Result<(), String>;
}

#[derive(Debug, Default)]
pub struct SystemKeyring;

fn entry(service: &str, user: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(service, user).map_err(|e| e.to_string())
}

impl KeyStore for SystemKeyring {
    fn get(&self, service: &str, user: &str) -> Result<Option<String>, String> {
        match entry(service, user)?.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn set(&self, service: &str, user: &str, password: &str) -> Result<(), String> {
        entry(service, user)?
            .set_password(password)
            .map_err(|e| e.to_string())
    }

    fn delete(&self, service: &str, user: &str) -> Result<(), String> {
        match entry(service, user)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// What a run of the Bitwarden command line tool left behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// The Bitwarden command line tool.
pub trait BitwardenCli: Send + Sync + Debug {
    /// Runs `bw <args>`; an `Err` means the program could not be started at all.
    fn run(&self, args: &[&str]) -> io::Result<CliOutput>;
}

/// The real `bw`; `BLUEPANE_BW` overrides its name (odd installs).
#[derive(Debug, Default)]
pub struct SystemBitwarden;

impl BitwardenCli for SystemBitwarden {
    fn run(&self, args: &[&str]) -> io::Result<CliOutput> {
        let program = std::env::var("BLUEPANE_BW").unwrap_or_else(|_| "bw".into());
        let out = Command::new(program).args(args).output()?;
        Ok(CliOutput {
            success: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

/// Everything a site needs to find or save its password.
pub trait SecretServices: Send + Sync + Debug {
    fn keys(&self) -> &dyn KeyStore;
    fn bitwarden(&self) -> &dyn BitwardenCli;
}

#[derive(Debug, Default)]
pub struct SystemServices {
    keys: SystemKeyring,
    bitwarden: SystemBitwarden,
}

impl SecretServices for SystemServices {
    fn keys(&self) -> &dyn KeyStore {
        &self.keys
    }

    fn bitwarden(&self) -> &dyn BitwardenCli {
        &self.bitwarden
    }
}

/// Whether this Bitwarden CLI is installed and answers.
pub fn bitwarden_responds(bw: &dyn BitwardenCli) -> bool {
    bw.run(&["--version"]).is_ok_and(|out| out.success)
}

/// Whether the system's Bitwarden CLI is installed and answers.
pub fn bitwarden_available() -> bool {
    bitwarden_responds(&SystemBitwarden)
}

fn bw_failure(error: io::Error) -> RemoteError {
    RemoteError::Auth(format!(
        "nie da się uruchomić Bitwarden CLI ({error}); zainstaluj `bw` albo wybierz inny sposób zapisu hasła"
    ))
}

fn bw_locked(stderr: &str) -> RemoteError {
    let why = stderr.trim();
    RemoteError::Auth(format!(
        "Bitwarden: {}. Odblokuj sejf (`bw unlock`) i ustaw BW_SESSION przed uruchomieniem bluepane",
        if why.is_empty() { "polecenie nie powiodło się" } else { why }
    ))
}

/// The password of a Bitwarden item. The vault has to be unlocked for the CLI.
pub fn bitwarden_secret(bw: &dyn BitwardenCli, item: &str) -> Result<String, RemoteError> {
    let out = bw
        .run(&["get", "password", item, "--raw", "--nointeraction"])
        .map_err(bw_failure)?;
    if !out.success {
        return Err(bw_locked(&out.stderr));
    }
    Ok(out.stdout.trim_end_matches(['\r', '\n']).to_string())
}

/// Creates a Bitwarden login item holding `password`. Never replaces an item that exists.
pub fn bitwarden_create(
    bw: &dyn BitwardenCli,
    item: &str,
    user: &str,
    uri: &str,
    password: &str,
) -> Result<(), RemoteError> {
    let existing = bw
        .run(&["get", "item", item, "--raw", "--nointeraction"])
        .map_err(bw_failure)?;
    if existing.success {
        return Err(RemoteError::Auth(format!(
            "w Bitwarden jest już element „{item}”: wybierz inną nazwę albo usuń go w Bitwarden"
        )));
    }
    let json = format!(
        "{{\"type\":1,\"name\":{},\"notes\":\"Utworzone przez bluepane\",\"favorite\":false,\
         \"login\":{{\"username\":{},\"password\":{},\"uris\":[{{\"match\":null,\"uri\":{}}}]}}}}",
        json_string(item),
        json_string(user),
        json_string(password),
        json_string(uri),
    );
    let encoded = base64(json.as_bytes());
    let out = bw
        .run(&["create", "item", &encoded, "--nointeraction"])
        .map_err(bw_failure)?;
    if out.success {
        Ok(())
    } else {
        Err(bw_locked(&out.stderr))
    }
}

/// A JSON string literal.
pub fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Standard base64 with padding (what `bw create` expects).
pub fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(char::from(TABLE[(n >> 18) as usize & 63]));
        out.push(char::from(TABLE[(n >> 12) as usize & 63]));
        out.push(if chunk.len() > 1 {
            char::from(TABLE[(n >> 6) as usize & 63])
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            char::from(TABLE[n as usize & 63])
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn json_strings_are_escaped() {
        assert_eq!(json_string("plain"), "\"plain\"");
        assert_eq!(json_string("a\"b\\c"), "\"a\\\"b\\\\c\"");
        assert_eq!(json_string("line\nbreak\ttab"), "\"line\\nbreak\\ttab\"");
        assert_eq!(json_string("\u{1}"), "\"\\u0001\"");
        assert_eq!(json_string("zażółć"), "\"zażółć\"");
    }
}
