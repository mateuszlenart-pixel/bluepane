//! Saved sites (the site manager's data) and where their secrets live.
//!
//! The list is a small text file in the config directory:
//!
//! ```text
//! [Proxmox]
//! protocol = sftp
//! host = 192.168.1.10
//! port = 22
//! user = root
//! key = /home/me/.ssh/id_ed25519
//! start = /root
//! secret = ask
//! ```
//!
//! `secret` says where the password (or the key's passphrase) comes from: `ask` (typed at
//! every connection, never stored), `local:<text>` (stored in this file, which is made
//! readable for the owner only) or `bitwarden:<item>` (fetched from an unlocked Bitwarden
//! CLI when connecting; nothing is stored here).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use std::sync::Arc;
use std::time::Duration;

use crate::ftp::FtpConnection;
use crate::{Auth, ConnectParams, KnownHosts, Protocol, RemoteError, RemoteFs, SftpConnection};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SecretStore {
    /// Asked at every connection.
    #[default]
    Ask,
    /// Kept in the sites file in plain text (the file is owner-only on Unix).
    Local(String),
    /// A Bitwarden item (name or id) read with the `bw` command.
    Bitwarden(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Site {
    pub name: String,
    pub protocol: Protocol,
    pub host: String,
    pub port: u16,
    pub user: String,
    /// A private key file; without it the secret is a password.
    pub key: Option<PathBuf>,
    pub secret: SecretStore,
    /// Directory to open after connecting; empty means the login directory.
    pub start_dir: String,
}

impl Site {
    pub fn new(name: impl Into<String>, host: impl Into<String>, user: impl Into<String>) -> Site {
        Site {
            name: name.into(),
            protocol: Protocol::Sftp,
            host: host.into(),
            port: 22,
            user: user.into(),
            key: None,
            secret: SecretStore::Ask,
            start_dir: String::new(),
        }
    }

    /// What is wrong with the site's data, if anything (Polish, ready to show).
    pub fn problem(&self) -> Option<String> {
        if self.name.trim().is_empty() || self.name.contains(['\n', ']', '[']) {
            return Some("nazwa witryny jest pusta albo zawiera niedozwolone znaki".into());
        }
        if self.host.trim().is_empty() || self.host.contains(char::is_whitespace) {
            return Some("podaj adres serwera (bez spacji)".into());
        }
        if self.user.trim().is_empty() {
            return Some("podaj nazwę użytkownika".into());
        }
        if self.port == 0 {
            return Some("port nie może być zerem".into());
        }
        None
    }

    /// Connects with this site's protocol. `secret` is the password (or the key's
    /// passphrase); `trust` is the fingerprint of an unknown key or certificate that the user
    /// has just confirmed.
    pub fn connect(
        &self,
        secret: Option<String>,
        known: &KnownHosts,
        trust: Option<&str>,
    ) -> Result<Arc<dyn RemoteFs>, RemoteError> {
        match self.protocol {
            Protocol::Sftp => {
                let conn = SftpConnection::connect(&self.params(secret), known, trust)?;
                Ok(Arc::new(conn))
            }
            protocol => {
                // Anonymous logins want some text as the "password".
                let password = secret.filter(|s| !s.is_empty()).unwrap_or_else(|| {
                    if self.user.eq_ignore_ascii_case("anonymous") || self.user == "ftp" {
                        "anonymous@".to_string()
                    } else {
                        String::new()
                    }
                });
                let conn = FtpConnection::connect(
                    protocol,
                    &self.host,
                    self.port,
                    &self.user,
                    &password,
                    Duration::from_secs(20),
                    known,
                    trust,
                )?;
                Ok(Arc::new(conn))
            }
        }
    }

    /// Connection parameters for SFTP; `secret` is the password or the key's passphrase.
    pub fn params(&self, secret: Option<String>) -> ConnectParams {
        let auth = match &self.key {
            Some(path) => Auth::Key {
                path: path.clone(),
                passphrase: secret.filter(|s| !s.is_empty()),
            },
            None => Auth::Password(secret.unwrap_or_default()),
        };
        ConnectParams {
            port: self.port,
            ..ConnectParams::new(self.host.clone(), self.user.clone(), auth)
        }
    }

    /// The stored secret, if it can be had without asking: `Ok(None)` means "ask".
    pub fn stored_secret(&self) -> Result<Option<String>, RemoteError> {
        match &self.secret {
            SecretStore::Ask => Ok(None),
            SecretStore::Local(secret) => Ok(Some(secret.clone())),
            SecretStore::Bitwarden(item) => bitwarden_secret(item).map(Some),
        }
    }

    /// The same site without anything secret in it (for lists and status lines).
    pub fn label(&self) -> String {
        format!("{} ({}@{}:{})", self.name, self.user, self.host, self.port)
    }
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn unescape(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sites {
    path: Option<PathBuf>,
    pub sites: Vec<Site>,
}

impl Sites {
    pub fn in_memory() -> Sites {
        Sites::default()
    }

    /// Reads the file; a missing one is an empty list.
    pub fn load(path: &Path) -> io::Result<Sites> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e),
        };
        let mut sites: Vec<Site> = Vec::new();
        for line in text.lines() {
            let line = line.trim_end_matches('\r');
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                sites.push(Site::new(unescape(name), "", ""));
                continue;
            }
            let (Some(site), Some((key, value))) = (sites.last_mut(), line.split_once('=')) else {
                continue;
            };
            let value = unescape(value.trim_start());
            match key.trim() {
                "protocol" => site.protocol = Protocol::from_word(&value).unwrap_or_default(),
                "host" => site.host = value,
                "port" => site.port = value.trim().parse().unwrap_or(22),
                "user" => site.user = value,
                "key" => {
                    site.key = Some(PathBuf::from(value)).filter(|p| !p.as_os_str().is_empty())
                }
                "start" => site.start_dir = value,
                "secret" => {
                    site.secret = if let Some(secret) = value.strip_prefix("local:") {
                        SecretStore::Local(secret.to_string())
                    } else if let Some(item) = value.strip_prefix("bitwarden:") {
                        SecretStore::Bitwarden(item.to_string())
                    } else {
                        SecretStore::Ask
                    }
                }
                _ => {}
            }
        }
        Ok(Sites {
            path: Some(path.to_path_buf()),
            sites,
        })
    }

    pub fn get(&self, name: &str) -> Option<&Site> {
        self.sites
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// Adds a site or replaces the one with the same name, then saves.
    pub fn upsert(&mut self, site: Site) -> io::Result<()> {
        match self
            .sites
            .iter_mut()
            .find(|s| s.name.eq_ignore_ascii_case(&site.name))
        {
            Some(existing) => *existing = site,
            None => self.sites.push(site),
        }
        self.save()
    }

    pub fn remove(&mut self, name: &str) -> io::Result<bool> {
        let before = self.sites.len();
        self.sites.retain(|s| !s.name.eq_ignore_ascii_case(name));
        let removed = self.sites.len() != before;
        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut text = String::new();
        for site in &self.sites {
            text.push_str(&format!("[{}]\n", escape(&site.name)));
            text.push_str(&format!("protocol = {}\n", site.protocol.word()));
            text.push_str(&format!("host = {}\n", escape(&site.host)));
            text.push_str(&format!("port = {}\n", site.port));
            text.push_str(&format!("user = {}\n", escape(&site.user)));
            if let Some(key) = &site.key {
                text.push_str(&format!("key = {}\n", escape(&key.to_string_lossy())));
            }
            if !site.start_dir.is_empty() {
                text.push_str(&format!("start = {}\n", escape(&site.start_dir)));
            }
            let secret = match &site.secret {
                SecretStore::Ask => "ask".to_string(),
                SecretStore::Local(secret) => format!("local:{secret}"),
                SecretStore::Bitwarden(item) => format!("bitwarden:{item}"),
            };
            text.push_str(&format!("secret = {}\n\n", escape(&secret)));
        }
        // Owner-only before anything secret is written into the file.
        let mut file = fs::File::create(path)?;
        bluepane_vfs::set_mode(path, 0o600)?;
        file.write_all(text.as_bytes())
    }
}

/// The Bitwarden command line tool; `BLUEPANE_BW` overrides its name (tests, odd installs).
fn bw_command() -> String {
    std::env::var("BLUEPANE_BW").unwrap_or_else(|_| "bw".into())
}

/// Whether the Bitwarden CLI is installed and answers.
pub fn bitwarden_available() -> bool {
    Command::new(bw_command())
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success())
}

/// The password of a Bitwarden item. The vault has to be unlocked for the CLI
/// (`bw unlock`, then `BW_SESSION` set in the environment bluepane runs in).
pub fn bitwarden_secret(item: &str) -> Result<String, RemoteError> {
    let out = Command::new(bw_command())
        .args(["get", "password", item, "--raw", "--nointeraction"])
        .output()
        .map_err(|e| {
            RemoteError::Auth(format!(
                "nie da się uruchomić Bitwarden CLI ({e}); zainstaluj `bw` albo wybierz inny sposób zapisu hasła"
            ))
        })?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(RemoteError::Auth(format!(
            "Bitwarden: {}. Odblokuj sejf (`bw unlock`) i ustaw BW_SESSION przed uruchomieniem bluepane",
            if why.is_empty() { "brak hasła dla elementu".to_string() } else { why }
        )));
    }
    let secret = String::from_utf8_lossy(&out.stdout);
    Ok(secret.trim_end_matches(['\r', '\n']).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Site> {
        let mut proxmox = Site::new("Proxmox", "192.168.1.10", "root");
        proxmox.key = Some(PathBuf::from("/home/me/.ssh/id_ed25519"));
        proxmox.start_dir = "/root".into();
        proxmox.secret = SecretStore::Bitwarden("proxmox key".into());
        let mut ha = Site::new("Home Assistant", "ha.local", "hass");
        ha.port = 2222;
        ha.secret = SecretStore::Local("p@ss = word\\with\nnewline".into());
        vec![proxmox, ha, Site::new("Ask me", "example.com", "bob")]
    }

    #[test]
    fn sites_round_trip_through_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("conf/sites.conf");
        let mut sites = Sites::load(&path).unwrap();
        assert!(sites.sites.is_empty());
        for site in sample() {
            sites.upsert(site).unwrap();
        }
        let again = Sites::load(&path).unwrap();
        assert_eq!(again.sites, sample());
        assert_eq!(again.get("home assistant").unwrap().port, 2222);
    }

    #[test]
    fn upsert_replaces_and_remove_deletes() {
        let mut sites = Sites::in_memory();
        sites.upsert(Site::new("A", "h1", "u")).unwrap();
        sites.upsert(Site::new("a", "h2", "u")).unwrap();
        assert_eq!(sites.sites.len(), 1);
        assert_eq!(sites.sites[0].host, "h2");
        assert!(sites.remove("A").unwrap());
        assert!(!sites.remove("A").unwrap());
    }

    #[test]
    fn the_file_with_secrets_is_owner_only() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("sites.conf");
        let mut sites = Sites::load(&path).unwrap();
        let mut site = Site::new("S", "h", "u");
        site.secret = SecretStore::Local("hunter2".into());
        sites.upsert(site).unwrap();
        // `None` on Windows, where there are no permission bits.
        if let Some(mode) = bluepane_vfs::file_mode(&path) {
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn problems_are_described() {
        assert!(Site::new("ok", "h", "u").problem().is_none());
        assert!(Site::new("", "h", "u").problem().is_some());
        assert!(Site::new("a]b", "h", "u").problem().is_some());
        assert!(Site::new("n", "", "u").problem().is_some());
        assert!(Site::new("n", "a b", "u").problem().is_some());
        assert!(Site::new("n", "h", " ").problem().is_some());
        let mut zero = Site::new("n", "h", "u");
        zero.port = 0;
        assert!(zero.problem().is_some());
    }

    #[test]
    fn params_use_the_secret_as_password_or_passphrase() {
        let mut site = Site::new("n", "h", "u");
        site.port = 2200;
        let params = site.params(Some("pw".into()));
        assert_eq!(params.port, 2200);
        assert_eq!(params.auth, Auth::Password("pw".into()));
        site.key = Some(PathBuf::from("/k"));
        assert_eq!(
            site.params(Some(String::new())).auth,
            Auth::Key {
                path: PathBuf::from("/k"),
                passphrase: None
            }
        );
        assert_eq!(site.stored_secret().unwrap(), None, "ask");
        site.secret = SecretStore::Local("x".into());
        assert_eq!(site.stored_secret().unwrap(), Some("x".into()));
    }

    #[test]
    fn a_missing_bitwarden_cli_is_a_clear_error() {
        std::env::set_var("BLUEPANE_BW", "definitely-not-an-installed-program");
        assert!(!bitwarden_available());
        let err = bitwarden_secret("item").unwrap_err();
        assert!(err.to_string().contains("Bitwarden CLI"), "{err}");
        std::env::remove_var("BLUEPANE_BW");
    }
}
