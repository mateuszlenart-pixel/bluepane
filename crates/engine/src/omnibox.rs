//! The omnibox: what the path bar does with the text typed into it. A plain path goes there;
//! `sftp://user@host:port/dir` (also `ftp://`, `ftps://`) connects, `smb://host/share` opens
//! a share, `?pattern` searches the folder, `>command` runs a command. `omnibox_hints` offers
//! completions while typing.

use std::path::{Path, PathBuf};

use bluepane_remote::{Protocol, SecretStore, Site};

use crate::palette::rank;
use crate::session::Session;
use crate::EngineError;

/// How many completions are offered.
const HINTS_MAX: usize = 8;

/// What the typed text asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A command line (`>sort size`).
    Command(String),
    /// A file name pattern to search for in the active folder (`?*.pdf`).
    Search(String),
    Server {
        protocol: Protocol,
        user: Option<String>,
        host: String,
        port: Option<u16>,
        dir: Option<String>,
    },
    /// `smb://[user@]host[/share[/folder]]`: `path` is everything after the host.
    Share {
        user: Option<String>,
        host: String,
        path: Option<String>,
    },
    /// Anything else: a local, archive or server folder for `cd`.
    Path(String),
}

/// One completion: what is shown and the text it puts in the path bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    pub label: String,
    pub completion: String,
}

/// Works out what `text` asks for.
pub fn classify(text: &str) -> Result<Target, EngineError> {
    let text = text.trim();
    if let Some(command) = text.strip_prefix('>') {
        let command = command.trim();
        return if command.is_empty() {
            Err(EngineError("po > wpisz komendę, np. >sort size".into()))
        } else {
            Ok(Target::Command(command.to_string()))
        };
    }
    if text.starts_with('!') {
        // `!program argumenty` runs a program in the folder of the panel (the `run` command).
        return Ok(Target::Command(text.to_string()));
    }
    if let Some(pattern) = text.strip_prefix('?') {
        let pattern = pattern.trim();
        return if pattern.is_empty() {
            Err(EngineError("po ? wpisz nazwę lub maskę, np. ?*.pdf".into()))
        } else {
            Ok(Target::Search(pattern.to_string()))
        };
    }
    if let Some((scheme, rest)) = text.split_once("://") {
        let scheme = scheme.to_ascii_lowercase();
        if scheme == "smb" {
            let (authority, path) = match rest.split_once('/') {
                Some((host, path)) => (host, Some(path.trim_matches('/').to_string())),
                None => (rest, None),
            };
            let (user, host) = match authority.rsplit_once('@') {
                Some((user, host)) => (Some(user.to_string()), host),
                None => (None, authority),
            };
            if host.is_empty() {
                return Err(EngineError(
                    "smb://: podaj komputer, np. smb://nas/zdjecia".into(),
                ));
            }
            return Ok(Target::Share {
                user: user.filter(|u| !u.is_empty()),
                host: host.to_string(),
                path: path.filter(|p| !p.is_empty()),
            });
        }
        if let Some(protocol) = Protocol::from_word(&scheme) {
            let (authority, dir) = match rest.find('/') {
                Some(at) => (&rest[..at], Some(rest[at..].to_string())),
                None => (rest, None),
            };
            let (user, hostport) = match authority.rsplit_once('@') {
                Some((user, hostport)) => (Some(user.to_string()), hostport),
                None => (None, authority),
            };
            let (host, port) =
                match hostport.rsplit_once(':') {
                    Some((host, port)) => (
                        host,
                        Some(port.parse::<u16>().map_err(|_| {
                            EngineError(format!("niepoprawny port w adresie: {port}"))
                        })?),
                    ),
                    None => (hostport, None),
                };
            if host.is_empty() {
                return Err(EngineError(format!(
                    "{scheme}://: podaj serwer, np. {scheme}://użytkownik@host/katalog"
                )));
            }
            return Ok(Target::Server {
                protocol,
                user: user.filter(|u| !u.is_empty()),
                host: host.to_string(),
                port,
                dir,
            });
        }
    }
    if text.is_empty() {
        return Err(EngineError(
            "wpisz ścieżkę, adres serwera, ?wzorzec albo >komendę".into(),
        ));
    }
    Ok(Target::Path(text.to_string()))
}

impl Session {
    /// Does what the typed text asks for (`go <text>`).
    pub(crate) fn omnibox(&mut self, text: &str) -> Result<(), EngineError> {
        match classify(text)? {
            Target::Command(line) => self.run_line(&line),
            Target::Path(path) => self.run_line(&format!("cd {path}")),
            Target::Search(pattern) => {
                let panel = self.active_panel();
                if panel.remote.is_some() || panel.archive.is_some() || panel.virtual_list.is_some()
                {
                    return Err(EngineError(
                        "szukanie ?wzorzec działa w katalogu na dysku".into(),
                    ));
                }
                let root = panel.cwd.display().to_string();
                self.run_line("find")?;
                self.run_line(&format!("findset roots \"{root}\""))?;
                self.run_line(&format!("findset name \"{pattern}\""))?;
                self.run_line("findrun")
            }
            Target::Share { user, host, path } => self.omnibox_share(user, &host, path),
            Target::Server {
                protocol,
                user,
                host,
                port,
                dir,
            } => self.omnibox_server(protocol, user, &host, port, dir),
        }
    }

    /// `smb://host/share/folder`: through `smbclient` as a server in the panel when it is
    /// installed; otherwise the share is opened the way the system does (a UNC path on
    /// Windows, a GVFS mount on Linux), which only reaches the share, not a folder in it.
    fn omnibox_share(
        &mut self,
        user: Option<String>,
        host: &str,
        path: Option<String>,
    ) -> Result<(), EngineError> {
        if self.smb.available() {
            return self.smb_connect(user, host, path);
        }
        match path.as_deref().map(|p| p.split('/').next().unwrap_or(p)) {
            Some(name) => self.run_line(&format!("scanshare {host} {name}")),
            None => self.run_line(&format!("scanshares {host}")),
        }
    }

    /// Opens an SMB server in the active panel, saving it as a site on first use.
    fn smb_connect(
        &mut self,
        user: Option<String>,
        host: &str,
        path: Option<String>,
    ) -> Result<(), EngineError> {
        self.load_config();
        let port = Protocol::Smb.default_port();
        let known = self.sites.sites.iter().find(|s| {
            s.protocol == Protocol::Smb
                && s.host.eq_ignore_ascii_case(host)
                && user.as_ref().is_none_or(|u| *u == s.user)
        });
        let name = match known {
            Some(site) => site.name.clone(),
            None => {
                let user = user.unwrap_or_default();
                let name = if user.is_empty() {
                    format!("{host} (smb)")
                } else {
                    format!("{user}@{host} (smb)")
                };
                let mut site = Site::new(name.clone(), host, user);
                site.protocol = Protocol::Smb;
                site.port = port;
                site.secret = SecretStore::Ask;
                if let Some(problem) = site.problem() {
                    return Err(EngineError(format!("smb://{host}: {problem}")));
                }
                self.sites
                    .upsert(site)
                    .map_err(|e| EngineError(format!("nie udało się zapisać witryny: {e}")))?;
                self.status = format!("Zapisano witrynę {name} (komenda sites)");
                name
            }
        };
        self.after_connect = path.map(|p| (name.clone(), format!("/{p}")));
        self.connect_site(&name, None)
    }

    fn omnibox_server(
        &mut self,
        protocol: Protocol,
        user: Option<String>,
        host: &str,
        port: Option<u16>,
        dir: Option<String>,
    ) -> Result<(), EngineError> {
        self.load_config();
        if protocol == Protocol::Rclone {
            // `rclone://chmura/katalog`: the cloud is the one `rclone config` named.
            return self.cloud_connect(host, dir);
        }
        let port = port.unwrap_or_else(|| protocol.default_port());
        let known = self.sites.sites.iter().find(|s| {
            s.host.eq_ignore_ascii_case(host)
                && s.protocol == protocol
                && s.port == port
                && user.as_ref().is_none_or(|u| *u == s.user)
        });
        let name = match known {
            Some(site) => site.name.clone(),
            None => {
                let user = match (&user, protocol) {
                    (Some(user), _) => user.clone(),
                    (None, Protocol::Sftp) => {
                        return Err(EngineError(format!(
                            "podaj użytkownika: sftp://użytkownik@{host}"
                        )))
                    }
                    (None, _) => "anonymous".to_string(),
                };
                let name = format!("{user}@{host}");
                let mut site = Site::new(name.clone(), host, user);
                site.protocol = protocol;
                site.port = port;
                site.secret = SecretStore::Ask;
                self.sites
                    .upsert(site)
                    .map_err(|e| EngineError(format!("nie udało się zapisać witryny: {e}")))?;
                self.status = format!("Zapisano witrynę {name} (komenda sites)");
                name
            }
        };
        self.after_connect = dir.map(|dir| (name.clone(), dir));
        self.connect_site(&name, None)
    }

    /// Completions for the text being typed into the path bar of the active panel.
    pub fn omnibox_hints(&self, text: &str) -> Vec<Hint> {
        let text = text.trim_start();
        if let Some(rest) = text.strip_prefix('>') {
            return rank(rest.trim(), &self.recent_commands)
                .into_iter()
                .take(HINTS_MAX)
                .map(|entry| Hint {
                    label: format!("{}  {}", entry.help.usage, short(entry.help.description)),
                    completion: format!(">{}", entry.help.name),
                })
                .collect();
        }
        if text.starts_with('?') || text.is_empty() {
            return Vec::new();
        }
        if let Some((scheme, _)) = text.split_once("://") {
            return self.server_hints(scheme, text);
        }
        if text.len() < 3 && !text.contains(['/', '\\']) {
            return Vec::new();
        }
        self.folder_hints(text)
    }

    fn server_hints(&self, scheme: &str, text: &str) -> Vec<Hint> {
        let Some(protocol) = Protocol::from_word(scheme) else {
            return Vec::new();
        };
        let typed = text.to_ascii_lowercase();
        self.sites
            .sites
            .iter()
            .filter(|s| s.protocol == protocol)
            .map(|s| {
                let user = if s.user.is_empty() {
                    String::new()
                } else {
                    format!("{}@", s.user)
                };
                format!("{scheme}://{user}{}", s.host)
            })
            .filter(|url| {
                url.to_ascii_lowercase().starts_with(&typed) && !url.eq_ignore_ascii_case(text)
            })
            .take(HINTS_MAX)
            .map(|url| Hint {
                label: url.clone(),
                completion: url,
            })
            .collect()
    }

    /// Sub-folders of the typed folder that start with the typed last component.
    fn folder_hints(&self, text: &str) -> Vec<Hint> {
        let panel = self.active_panel();
        if panel.remote.is_some() || panel.archive.is_some() || panel.virtual_list.is_some() {
            return Vec::new();
        }
        let typed = Path::new(text);
        let ends_with_separator = text.ends_with(['/', '\\']);
        let (dir, prefix): (PathBuf, String) = if ends_with_separator {
            (typed.to_path_buf(), String::new())
        } else {
            (
                typed.parent().map(Path::to_path_buf).unwrap_or_default(),
                typed
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            )
        };
        let dir_to_read = if dir.as_os_str().is_empty() {
            panel.cwd.clone()
        } else if dir.is_absolute() {
            dir.clone()
        } else {
            panel.cwd.join(&dir)
        };
        let prefix = crate::palette::fold(&prefix);
        let Ok(read) = std::fs::read_dir(&dir_to_read) else {
            return Vec::new();
        };
        let mut names: Vec<String> = read
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| crate::palette::fold(n).starts_with(&prefix))
            .collect();
        names.sort_by_key(|n| crate::palette::fold(n));
        names
            .into_iter()
            .take(HINTS_MAX)
            .map(|name| {
                let full = if ends_with_separator {
                    format!("{text}{name}")
                } else {
                    dir.join(&name).display().to_string()
                };
                Hint {
                    label: name,
                    completion: format!("{full}{}", std::path::MAIN_SEPARATOR),
                }
            })
            .collect()
    }
}

fn short(description: &str) -> &str {
    description.split(';').next().unwrap_or(description)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prefix_or_scheme_decides_what_the_text_means() {
        assert_eq!(
            classify(">sort size").unwrap(),
            Target::Command("sort size".into())
        );
        assert_eq!(classify("?*.pdf").unwrap(), Target::Search("*.pdf".into()));
        assert_eq!(
            classify("!git status").unwrap(),
            Target::Command("!git status".into())
        );
        assert_eq!(
            classify("/home/ala").unwrap(),
            Target::Path("/home/ala".into())
        );
        assert_eq!(
            classify("C:\\Users").unwrap(),
            Target::Path("C:\\Users".into())
        );
        assert!(classify(">").is_err() && classify("?").is_err() && classify("  ").is_err());
    }

    #[test]
    fn server_addresses_are_taken_apart() {
        assert_eq!(
            classify("sftp://ala@nas.local:2222/data/x").unwrap(),
            Target::Server {
                protocol: Protocol::Sftp,
                user: Some("ala".into()),
                host: "nas.local".into(),
                port: Some(2222),
                dir: Some("/data/x".into()),
            }
        );
        assert_eq!(
            classify("FTP://nas").unwrap(),
            Target::Server {
                protocol: Protocol::Ftp,
                user: None,
                host: "nas".into(),
                port: None,
                dir: None,
            }
        );
        assert!(classify("sftp://ala@:22").is_err());
        assert!(classify("sftp://ala@host:abc/").is_err());
    }

    #[test]
    fn share_addresses_are_taken_apart() {
        assert_eq!(
            classify("smb://nas/zdjecia").unwrap(),
            Target::Share {
                user: None,
                host: "nas".into(),
                path: Some("zdjecia".into())
            }
        );
        assert_eq!(
            classify("smb://192.168.1.5").unwrap(),
            Target::Share {
                user: None,
                host: "192.168.1.5".into(),
                path: None
            }
        );
        assert_eq!(
            classify("smb://ala@nas/dane/Zdjęcia/2025/").unwrap(),
            Target::Share {
                user: Some("ala".into()),
                host: "nas".into(),
                path: Some("dane/Zdjęcia/2025".into())
            }
        );
        assert!(classify("smb://").is_err());
    }

    fn session(tmp: &Path) -> Session {
        std::fs::create_dir_all(tmp.join("left/Dokumenty")).unwrap();
        std::fs::create_dir_all(tmp.join("left/Domowe")).unwrap();
        std::fs::create_dir_all(tmp.join("left/zdjęcia")).unwrap();
        std::fs::write(tmp.join("left/plik.txt"), "x").unwrap();
        std::fs::create_dir_all(tmp.join("right")).unwrap();
        let mut s = Session::new(tmp.join("left"), tmp.join("right"));
        s.use_config_dir(&tmp.join("config"));
        s
    }

    #[test]
    fn go_runs_commands_paths_and_searches() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("go >sort size").unwrap();
        assert_eq!(s.active_panel().sort.key, crate::SortKey::Size);
        s.run_line("go Dokumenty").unwrap();
        assert!(s.active_panel().cwd.ends_with("Dokumenty"));
        s.run_line("go ..").unwrap();
        s.run_line("go ?plik*").unwrap();
        assert!(
            s.find.as_ref().is_some_and(|f| f.visible),
            "the search window opened"
        );
        assert!(s.run_line("go sftp://nas").is_err(), "SFTP needs a user");
    }

    fn smb_session(tmp: &Path) -> Session {
        let mut s = session(tmp);
        let server = tmp.join("server");
        std::fs::create_dir_all(server.join("dane/Zdjęcia/2025")).unwrap();
        std::fs::write(server.join("dane/zażółć.txt"), "treść").unwrap();
        std::fs::write(server.join("dane/Zdjęcia/2025/a.jpg"), "jpg").unwrap();
        s.smb = std::sync::Arc::new(bluepane_remote::testing::FakeSmb::new(
            &server,
            &["dane", "Wideo"],
            Some(("ala", "tajne")),
        ));
        s
    }

    /// Types the password into the question the connection asked.
    fn answer_password(s: &mut Session, password: &str) {
        match s.pending.as_mut() {
            Some(crate::session::Pending::Input { text, .. }) => *text = password.to_string(),
            other => panic!("expected the password question, got {other:?}"),
        }
        s.run_line("confirm").unwrap();
    }

    #[test]
    fn an_smb_server_lists_its_shares_and_a_share_opens_at_a_folder_asking_for_the_password() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = smb_session(tmp.path());
        s.run_line("go smb://ala@nas/dane/Zdjęcia").unwrap();
        assert!(s.pending.is_some(), "a user means a password question");
        answer_password(&mut s, "tajne");
        s.wait_idle();
        s.wait_idle();
        let panel = s.active_panel();
        assert!(panel.remote.is_some(), "{}", s.status);
        let names: Vec<&str> = panel.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"2025"), "{names:?}");
        let site = s.sites.get("ala@nas (smb)").unwrap();
        assert_eq!(
            (
                site.protocol,
                site.host.as_str(),
                site.port,
                site.user.as_str()
            ),
            (Protocol::Smb, "nas", 445, "ala")
        );
        // The server's root is the list of shares, without the hidden ones.
        s.run_line("cd /").unwrap();
        s.wait_idle();
        let names: Vec<&str> = s
            .active_panel()
            .entries
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert!(
            names.contains(&"dane") && names.contains(&"Wideo"),
            "{names:?}"
        );
        assert!(!names.iter().any(|n| n.ends_with('$')), "{names:?}");
        // The same server again reuses the site.
        s.run_line("disconnect").unwrap();
        s.run_line("go smb://ala@nas").unwrap();
        assert_eq!(s.sites.sites.len(), 1);
    }

    #[test]
    fn a_wrong_smb_password_is_reported_and_a_missing_smbclient_falls_back_to_the_system() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = smb_session(tmp.path());
        s.run_line("go smb://ala@nas/dane").unwrap();
        answer_password(&mut s, "zle");
        s.wait_idle();
        assert!(s.active_panel().remote.is_none());
        assert!(
            s.status.contains("login") || s.status.contains("hasł"),
            "{}",
            s.status
        );

        // Without smbclient the share goes the old way (scanshare), which names the problem.
        #[derive(Debug)]
        struct Missing;
        impl bluepane_remote::SmbRunner for Missing {
            fn available(&self) -> bool {
                false
            }
            fn run(
                &self,
                _: &[String],
                _: Option<&str>,
                _: &dyn Fn() -> bool,
            ) -> Result<Option<bluepane_remote::SmbOutput>, bluepane_remote::RemoteError>
            {
                unreachable!("not asked when it is not there")
            }
        }
        s.smb = std::sync::Arc::new(Missing);
        s.sites.sites.clear();
        let _ = s.run_line("go smb://nas/dane/Zdjęcia");
        assert!(
            s.sites.sites.is_empty(),
            "no site is saved for the fallback"
        );
    }

    #[test]
    fn an_unknown_server_is_saved_and_a_known_one_is_reused() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        s.run_line("go ftp://serwer.test/pub").unwrap();
        assert_eq!(s.sites.sites.len(), 1);
        assert_eq!(s.sites.sites[0].name, "anonymous@serwer.test");
        assert_eq!(s.sites.sites[0].protocol, Protocol::Ftp);
        assert_eq!(s.after_connect.as_ref().unwrap().1, "/pub");
        // The same address again does not add a second site.
        s.run_line("go ftp://serwer.test").unwrap();
        assert_eq!(s.sites.sites.len(), 1);
        assert!(s.after_connect.is_none(), "no folder, nothing to go to");
    }

    #[test]
    fn hints_complete_folders_commands_and_servers() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = session(tmp.path());
        let labels = |h: Vec<Hint>| h.into_iter().map(|h| h.label).collect::<Vec<_>>();
        assert_eq!(
            labels(s.omnibox_hints("Do")),
            Vec::<String>::new(),
            "too short to guess"
        );
        assert_eq!(labels(s.omnibox_hints("Dok")), ["Dokumenty"]);
        assert_eq!(labels(s.omnibox_hints("Dom")), ["Domowe"]);
        assert_eq!(labels(s.omnibox_hints("zdj")), ["zdjęcia"]);
        let abs = format!("{}/", tmp.path().join("left").display());
        let all = labels(s.omnibox_hints(&abs));
        assert_eq!(
            all,
            ["Dokumenty", "Domowe", "zdjęcia"],
            "files are not offered"
        );
        let first = s.omnibox_hints(&format!("{abs}Dok"));
        assert!(first[0]
            .completion
            .ends_with(&format!("Dokumenty{}", std::path::MAIN_SEPARATOR)));
        let commands = s.omnibox_hints(">kolejka");
        assert!(commands.iter().any(|h| h.completion == ">queue"));
        assert!(s.omnibox_hints("?abc").is_empty());
        s.run_line("siteset nas host nas.local").unwrap();
        s.run_line("siteset nas user ala").unwrap();
        assert_eq!(
            labels(s.omnibox_hints("sftp://a")),
            ["sftp://ala@nas.local"]
        );
    }
}
