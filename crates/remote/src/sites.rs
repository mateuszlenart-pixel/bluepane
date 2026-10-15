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
//! every connection, never stored), `keyring` (the system password store), `local:<text>` (stored in this file, which is made
//! readable for the owner only) or `bitwarden:<item>` (fetched from an unlocked Bitwarden
//! CLI when connecting; nothing is stored here).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use std::sync::Arc;
use std::time::Duration;

use crate::ftp::FtpConnection;
use crate::secrets::{bitwarden_create, bitwarden_secret, SecretServices, KEYRING_SERVICE};
use crate::{Auth, ConnectParams, KnownHosts, Protocol, RemoteError, RemoteFs, SftpConnection};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SecretStore {
    /// Asked at every connection.
    #[default]
    Ask,
    /// Kept in the sites file in plain text (the file is owner-only on Unix).
    Local(String),
    /// In the system password store (Secret Service, Credential Manager).
    Keyring,
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
    /// FTP and FTPS only: active mode (the server connects back for data) instead of passive.
    pub active: bool,
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
            active: false,
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
                    self.active,
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

    /// What identifies this site's entry in the system password store.
    pub fn keyring_user(&self) -> String {
        format!("{}@{}:{}", self.user, self.host, self.port)
    }

    /// The stored secret, if it can be had without asking: `Ok(None)` means "ask".
    pub fn stored_secret(
        &self,
        services: &dyn SecretServices,
    ) -> Result<Option<String>, RemoteError> {
        match &self.secret {
            SecretStore::Ask => Ok(None),
            SecretStore::Local(secret) => Ok(Some(secret.clone())),
            SecretStore::Keyring => services
                .keys()
                .get(KEYRING_SERVICE, &self.keyring_user())
                .map_err(|e| RemoteError::Auth(format!("magazyn haseł systemu: {e}")))?
                .map(Some)
                .ok_or_else(|| {
                    RemoteError::Auth(
                        "w magazynie haseł systemu nie ma hasła do tej witryny: wpisz je w oknie serwerów".into(),
                    )
                }),
            SecretStore::Bitwarden(item) => {
                bitwarden_secret(services.bitwarden(), item).map(Some)
            }
        }
    }

    /// Saves `password` where this site's secret lives. `Local` and `Ask` are not handled
    /// here (the site itself holds or forgets the text).
    pub fn store_secret(
        &self,
        services: &dyn SecretServices,
        password: &str,
    ) -> Result<(), RemoteError> {
        match &self.secret {
            SecretStore::Keyring => services
                .keys()
                .set(KEYRING_SERVICE, &self.keyring_user(), password)
                .map_err(|e| RemoteError::Auth(format!("magazyn haseł systemu: {e}"))),
            SecretStore::Bitwarden(item) => bitwarden_create(
                services.bitwarden(),
                item,
                &self.user,
                &format!("{}://{}:{}", self.protocol.word(), self.host, self.port),
                password,
            ),
            SecretStore::Local(_) | SecretStore::Ask => Err(RemoteError::Auth(
                "ten sposób przechowywania nie zapisuje hasła poza witryną".into(),
            )),
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
                "mode" => site.active = value.trim().eq_ignore_ascii_case("active"),
                "secret" => {
                    site.secret = if let Some(secret) = value.strip_prefix("local:") {
                        SecretStore::Local(secret.to_string())
                    } else if value == "keyring" {
                        SecretStore::Keyring
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
            if site.active {
                text.push_str("mode = active\n");
            }
            let secret = match &site.secret {
                SecretStore::Ask => "ask".to_string(),
                SecretStore::Local(secret) => format!("local:{secret}"),
                SecretStore::Keyring => "keyring".to_string(),
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
        let mut nas = Site::new("NAS", "nas.local", "bob");
        nas.protocol = Protocol::Ftp;
        nas.port = 21;
        nas.active = true;
        vec![proxmox, ha, Site::new("Ask me", "example.com", "bob"), nas]
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
        let services = crate::testing::MemoryServices::default();
        assert_eq!(site.stored_secret(&services).unwrap(), None, "ask");
        site.secret = SecretStore::Local("x".into());
        assert_eq!(site.stored_secret(&services).unwrap(), Some("x".into()));
    }

    #[test]
    fn the_system_store_keeps_the_password_apart_from_the_file() {
        let services = crate::testing::MemoryServices::default();
        let mut site = Site::new("Proxmox", "10.0.0.2", "root");
        site.secret = SecretStore::Keyring;
        assert!(site.stored_secret(&services).is_err(), "nothing stored yet");
        site.store_secret(&services, "s3cret").unwrap();
        assert_eq!(
            site.stored_secret(&services).unwrap(),
            Some("s3cret".into())
        );
        // The entry is tied to who and where, so two servers never share a password.
        let mut other = site.clone();
        other.host = "10.0.0.3".into();
        assert!(other.stored_secret(&services).is_err());
        // A broken store is reported in words, not hidden.
        *services.keys.broken.lock().unwrap() = Some("sejf zablokowany".into());
        let err = site.stored_secret(&services).unwrap_err().to_string();
        assert!(err.contains("sejf zablokowany"), "{err}");
        // The sites file only says "keyring".
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("sites.conf");
        let mut sites = Sites::load(&path).unwrap();
        sites.upsert(site).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("secret = keyring") && !text.contains("s3cret"),
            "{text}"
        );
        assert_eq!(
            Sites::load(&path).unwrap().sites[0].secret,
            SecretStore::Keyring
        );
    }

    #[test]
    fn bitwarden_items_are_read_and_created() {
        let services = crate::testing::MemoryServices {
            bitwarden: crate::testing::FakeBitwarden::default().with_item("old", "from vault"),
            ..Default::default()
        };
        let mut site = Site::new("Files", "nas.local", "me");
        site.protocol = Protocol::Ftp;
        site.port = 21;
        site.secret = SecretStore::Bitwarden("old".into());
        assert_eq!(
            site.stored_secret(&services).unwrap(),
            Some("from vault".into())
        );

        // A new item is created with the login data and a quoted-name-safe JSON.
        site.secret = SecretStore::Bitwarden("bluepane: \"Files\"".into());
        site.store_secret(&services, "p\"w\\d").unwrap();
        assert_eq!(
            services
                .bitwarden
                .password_of("bluepane: \"Files\"")
                .as_deref(),
            Some("p\"w\\d")
        );
        let json = services.bitwarden.created.lock().unwrap()[0].clone();
        assert!(json.contains("\"username\":\"me\""), "{json}");
        assert!(json.contains("\"uri\":\"ftp://nas.local:21\""), "{json}");
        assert_eq!(
            site.stored_secret(&services).unwrap(),
            Some("p\"w\\d".into())
        );

        // An existing item is never replaced.
        site.secret = SecretStore::Bitwarden("old".into());
        let err = site
            .store_secret(&services, "other")
            .unwrap_err()
            .to_string();
        assert!(err.contains("już element"), "{err}");
        assert_eq!(
            services.bitwarden.password_of("old").as_deref(),
            Some("from vault")
        );
    }

    #[test]
    fn bitwarden_problems_are_explained() {
        let missing = crate::testing::MemoryServices {
            bitwarden: crate::testing::FakeBitwarden {
                installed: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut site = Site::new("S", "h", "u");
        site.secret = SecretStore::Bitwarden("x".into());
        let err = site.stored_secret(&missing).unwrap_err().to_string();
        assert!(err.contains("Bitwarden CLI"), "{err}");
        assert!(!crate::bitwarden_responds(&missing.bitwarden));

        let locked = crate::testing::MemoryServices {
            bitwarden: crate::testing::FakeBitwarden {
                locked: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let err = site.store_secret(&locked, "pw").unwrap_err().to_string();
        assert!(err.contains("bw unlock"), "{err}");
        // The other stores do not save through this call.
        site.secret = SecretStore::Ask;
        assert!(site.store_secret(&locked, "pw").is_err());
    }
}
