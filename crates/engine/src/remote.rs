//! SFTP in the session: the site manager, connecting, browsing a server folder in a panel
//! and copying to and from it. All network work runs as [`JobKind::Remote`] jobs.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bluepane_remote::{KnownHosts, RemoteEntry, SecretStore, Site, Sites};
use bluepane_vfs::{Entry, EntryKind};

use crate::job::{ArchiveJob, JobKind, JobSpec, RemoteJob, RemoteOp, Report};
use crate::panel::{is_parent_entry, RemoteView};
use crate::remote_job::{remote_join, remote_name, remote_parent};
use crate::session::{InputKind, Pending, RemoteEdit, Session};
use crate::EngineError;

/// Rows of a server listing as panel entries.
pub(crate) fn remote_entries(dir: &str, list: &[RemoteEntry]) -> Vec<Entry> {
    list.iter()
        .map(|e| Entry {
            name: e.name.clone(),
            path: PathBuf::from(remote_join(dir, &e.name)),
            kind: if e.is_dir {
                EntryKind::Dir
            } else {
                EntryKind::File
            },
            size: e.size,
            modified: e.modified,
            created: None,
            mode: None,
            owner: None,
            hidden: e.name.starts_with('.'),
        })
        .collect()
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

impl Session {
    /// Reads the saved sites and remembered server keys from the config directory (once).
    pub fn load_config(&mut self) {
        if self.config_loaded {
            return;
        }
        if let Some(dir) = bluepane_vfs::config_dir() {
            self.use_config_dir(&dir);
        } else {
            self.config_loaded = true;
        }
    }

    /// Takes the sites and server keys from `dir` (`sites.conf`, `known_hosts`).
    pub fn use_config_dir(&mut self, dir: &Path) {
        self.config_loaded = true;
        self.config_dir = Some(dir.to_path_buf());
        self.load_recent_commands(dir);
        self.load_visits(dir);
        self.load_favorites(dir);
        self.load_buttons(dir);
        let (settings, warnings) = crate::settings::load(&dir.join("settings.conf"));
        self.use_settings(settings);
        if !warnings.is_empty() {
            self.status = format!("settings.conf: {}", warnings.join("; "));
            self.status_is_error = true;
        }
        match Sites::load(&dir.join("sites.conf")) {
            Ok(sites) => self.sites = sites,
            Err(e) => {
                self.status = format!("Nie udało się wczytać witryn: {e}");
                self.status_is_error = true;
            }
        }
        match KnownHosts::load(&dir.join("known_hosts")) {
            Ok(known) => self.known_hosts = known,
            Err(e) => {
                self.status = format!("Nie udało się wczytać kluczy serwerów: {e}");
                self.status_is_error = true;
            }
        }
    }

    pub(crate) fn open_sites(&mut self) {
        self.load_config();
        self.sites_open = true;
        self.status = "Witryny SFTP: wybierz, edytuj albo połącz".into();
    }

    pub(crate) fn site_set(
        &mut self,
        name: &str,
        field: &str,
        value: &str,
    ) -> Result<(), EngineError> {
        self.load_config();
        let mut site = self
            .sites
            .get(name)
            .cloned()
            .unwrap_or_else(|| Site::new(name, "", ""));
        if field == "key" && site.protocol != bluepane_remote::Protocol::Sftp {
            return Err(EngineError(
                "klucz SSH działa tylko z SFTP (FTP loguje się hasłem)".into(),
            ));
        }
        match field {
            "protocol" => {
                let protocol =
                    bluepane_remote::Protocol::from_word(value.trim()).ok_or_else(|| {
                        EngineError(
                            "siteset protocol: sftp, ftp, ftps, ftps-implicit, rclone albo smb"
                                .into(),
                        )
                    })?;
                // A port that was the old protocol's default follows the change.
                if site.port == site.protocol.default_port() {
                    site.port = protocol.default_port();
                }
                site.protocol = protocol;
            }
            "host" => site.host = value.trim().to_string(),
            "port" => {
                site.port = value
                    .trim()
                    .parse()
                    .map_err(|_| EngineError("siteset port: oczekiwano liczby 1-65535".into()))?;
            }
            "user" => site.user = value.trim().to_string(),
            "key" => {
                site.key = Some(PathBuf::from(value.trim())).filter(|p| !p.as_os_str().is_empty())
            }
            "start" => site.start_dir = value.trim().to_string(),
            "mode" => {
                site.active = match value.trim().to_ascii_lowercase().as_str() {
                    "active" | "aktywny" => true,
                    "passive" | "pasywny" => false,
                    _ => return Err(EngineError("siteset mode: passive albo active".into())),
                };
            }
            "secret" => {
                site.secret = if let Some(secret) = value.strip_prefix("local:") {
                    SecretStore::Local(secret.to_string())
                } else if value.trim().eq_ignore_ascii_case("keyring") {
                    SecretStore::Keyring
                } else if let Some(item) = value.strip_prefix("bitwarden:") {
                    SecretStore::Bitwarden(item.trim().to_string())
                } else if value.trim().eq_ignore_ascii_case("ask") || value.trim().is_empty() {
                    SecretStore::Ask
                } else {
                    return Err(EngineError(
                        "siteset secret: ask, local:<hasło>, keyring albo bitwarden:<element>"
                            .into(),
                    ));
                };
            }
            _ => return Err(EngineError(format!("siteset: nieznane pole {field}"))),
        }
        if let SecretStore::Bitwarden(_) = site.secret {
            if !bluepane_remote::bitwarden_responds(self.secrets.bitwarden()) {
                self.status = "Uwaga: nie znaleziono Bitwarden CLI (`bw`); połączenie się nie uda, dopóki go nie zainstalujesz".into();
            }
        }
        // A half-filled site is kept as it is: the problem is reported when connecting.
        if site.name.trim().is_empty() || site.name.contains(['\n', '[', ']']) {
            return Err(EngineError(
                "nazwa witryny jest pusta albo zawiera niedozwolone znaki".into(),
            ));
        }
        self.sites
            .upsert(site)
            .map_err(|e| EngineError(format!("nie udało się zapisać witryn: {e}")))
    }

    /// `sitepassword <site> <password>`: saves the password where the site keeps its secret.
    pub(crate) fn site_password(
        &mut self,
        name: &str,
        password: String,
    ) -> Result<(), EngineError> {
        self.load_config();
        let site = self
            .sites
            .get(name)
            .cloned()
            .ok_or_else(|| EngineError(format!("nie ma witryny {name} (komenda sites)")))?;
        match &site.secret {
            SecretStore::Local(_) => {
                let mut site = site;
                site.secret = SecretStore::Local(password);
                self.sites
                    .upsert(site)
                    .map_err(|e| EngineError(format!("nie udało się zapisać witryn: {e}")))?;
                self.status = format!("Zapisano hasło w pliku witryn ({name})");
                Ok(())
            }
            SecretStore::Ask => Err(EngineError(
                "ta witryna pyta o hasło przy łączeniu: wybierz najpierw, gdzie je zapisać (siteset <nazwa> secret keyring|bitwarden:<element>|local:...)".into(),
            )),
            SecretStore::Keyring | SecretStore::Bitwarden(_) => {
                self.status = format!("Zapisywanie hasła ({name})…");
                self.enqueue_quiet(Self::remote_spec(
                    self.active,
                    None,
                    RemoteOp::SaveSecret {
                        site,
                        password,
                        services: self.secrets.clone(),
                    },
                ));
                Ok(())
            }
        }
    }

    pub(crate) fn site_delete(&mut self, name: &str) -> Result<(), EngineError> {
        self.load_config();
        match self.sites.remove(name) {
            Ok(true) => {
                self.status = format!("Usunięto witrynę {name}");
                Ok(())
            }
            Ok(false) => Err(EngineError(format!("nie ma witryny {name}"))),
            Err(e) => Err(EngineError(format!("nie udało się zapisać witryn: {e}"))),
        }
    }

    /// `connect <site> [password]`
    pub(crate) fn connect_site(
        &mut self,
        name: &str,
        password: Option<String>,
    ) -> Result<(), EngineError> {
        self.load_config();
        let site = self
            .sites
            .get(name)
            .cloned()
            .ok_or_else(|| EngineError(format!("nie ma witryny {name} (komenda sites)")))?;
        if let Some(problem) = site.problem() {
            return Err(EngineError(format!("witryna {}: {problem}", site.name)));
        }
        if self.active_panel().archive.is_some() {
            return Err(EngineError(
                "panel pokazuje archiwum: wyjdź z niego przed połączeniem".into(),
            ));
        }
        // A cloud has no login here: rclone keeps its own.
        // An SMB share opened as a guest (no user) has no password either.
        let no_login = match site.protocol {
            bluepane_remote::Protocol::Rclone => true,
            bluepane_remote::Protocol::Smb => site.user.is_empty(),
            _ => false,
        };
        if password.is_none() && site.secret == SecretStore::Ask && !no_login {
            self.status = format!(
                "Hasło do {} (Enter = połącz, Esc = anuluj; puste = bez hasła)",
                site.label()
            );
            self.pending = Some(Pending::Input {
                kind: InputKind::SitePassword(site.name),
                text: String::new(),
            });
            return Ok(());
        }
        self.start_connect(site, password, None);
        Ok(())
    }

    pub(crate) fn start_connect(
        &mut self,
        site: Site,
        secret: Option<String>,
        trust: Option<String>,
    ) {
        let side = self.active;
        self.status = format!("Łączenie z {}…", site.label());
        self.enqueue_quiet(Self::remote_spec(
            side,
            None,
            RemoteOp::Connect {
                site,
                secret,
                trust,
                known: self.known_hosts.clone(),
                services: self.secrets.clone(),
                rclone: self.rclone.clone(),
                smb: self.smb.clone(),
            },
        ));
    }

    /// The user confirmed the key of an unknown server: remember it and connect again.
    pub(crate) fn trust_host(&mut self, prompt: crate::job::HostPrompt) -> Result<(), EngineError> {
        self.known_hosts
            .add(
                &prompt.host,
                prompt.port,
                &prompt.algorithm,
                &prompt.fingerprint,
            )
            .map_err(|e| EngineError(format!("nie udało się zapisać klucza serwera: {e}")))?;
        let fingerprint = prompt.fingerprint.clone();
        self.start_connect(prompt.site, prompt.secret, Some(fingerprint));
        Ok(())
    }

    fn remote_spec(
        side: usize,
        conn: Option<Arc<dyn bluepane_remote::RemoteFs>>,
        op: RemoteOp,
    ) -> JobSpec {
        JobSpec {
            kind: JobKind::Remote,
            sources: Vec::new(),
            dest: None,
            archive: Some(ArchiveJob {
                side,
                remote: Some(RemoteJob { conn, op, side }),
                ..ArchiveJob::default()
            }),
        }
    }

    fn remote_of(&self, side: usize) -> Option<&RemoteView> {
        self.panels[side].remote.as_ref()
    }

    fn remote_op(&mut self, side: usize, op: RemoteOp) -> Result<(), EngineError> {
        let conn = self
            .remote_of(side)
            .map(|r| r.conn.clone())
            .ok_or_else(|| EngineError("panel nie jest połączony z serwerem".into()))?;
        self.enqueue_quiet(Self::remote_spec(side, Some(conn), op));
        Ok(())
    }

    /// Lists a server folder in the panel (in the background).
    pub(crate) fn remote_goto(&mut self, side: usize, path: String) -> Result<(), EngineError> {
        self.remote_op(side, RemoteOp::List { path })
    }

    /// Re-reads the folder every remote panel shows.
    pub(crate) fn remote_refresh_all(&mut self) {
        for side in 0..2 {
            if let Some(path) = self.remote_of(side).map(|r| r.path.clone()) {
                let _ = self.remote_goto(side, path);
            }
        }
    }

    /// Enter in a server panel: a folder is opened, a file explains how to get it.
    pub(crate) fn remote_enter(&mut self) {
        let side = self.active;
        let panel = &self.panels[side];
        let Some(entry) = panel.current().cloned() else {
            return;
        };
        let Some(remote) = &panel.remote else { return };
        let target = if is_parent_entry(&entry) {
            Some(path_text(&entry.path))
        } else if entry.is_dir() {
            Some(remote_join(&remote.path, &entry.name))
        } else {
            None
        };
        match target {
            Some(path) => {
                let _ = self.remote_goto(side, path);
            }
            None => {
                self.status = format!("{}: plik na serwerze, pobierz go klawiszem F5", entry.name);
            }
        }
    }

    /// Alt+Left / Alt+Right on a server: one step back or forward through visited folders.
    pub(crate) fn remote_history(&mut self, step: isize) -> Result<(), EngineError> {
        let side = self.active;
        let remote = self.panels[side]
            .remote
            .as_mut()
            .ok_or_else(|| EngineError("panel nie jest połączony z serwerem".into()))?;
        let target = remote
            .history_index
            .checked_add_signed(step)
            .filter(|i| *i < remote.history.len())
            .ok_or_else(|| {
                EngineError(if step < 0 {
                    "to początek historii katalogów serwera".into()
                } else {
                    "to koniec historii katalogów serwera".into()
                })
            })?;
        remote.history_index = target;
        let path = remote.history[target].clone();
        self.remote_goto(side, path)
    }

    pub(crate) fn remote_up(&mut self) {
        let side = self.active;
        let Some(path) = self.remote_of(side).map(|r| r.path.clone()) else {
            return;
        };
        if path == "/" {
            self.status = "To korzeń serwera (disconnect rozłącza)".into();
            return;
        }
        let _ = self.remote_goto(side, remote_parent(&path));
    }

    pub(crate) fn remote_cd(&mut self, text: &str) -> Result<(), EngineError> {
        let side = self.active;
        let base = self
            .remote_of(side)
            .map(|r| r.path.clone())
            .ok_or_else(|| EngineError("panel nie jest połączony z serwerem".into()))?;
        // The path bar shows `sftp://user@host:22/dir`: take the part after the host.
        let text = match text.strip_prefix("sftp://") {
            Some(rest) => rest.find('/').map_or("/", |at| &rest[at..]),
            None => text,
        };
        let path = if text.starts_with('/') {
            text.to_string()
        } else {
            remote_join(&base, text)
        };
        self.remote_goto(side, path)
    }

    pub(crate) fn disconnect(&mut self) -> Result<(), EngineError> {
        let side = self.active;
        let Some(remote) = self.panels[side].remote.take() else {
            return Err(EngineError("panel nie jest połączony z serwerem".into()));
        };
        self.panels[side].reload();
        self.status = format!("Rozłączono z {}", remote.label);
        Ok(())
    }

    pub(crate) fn deny_remote(&self, what: &str) -> Result<(), EngineError> {
        if self.panels.iter().any(|p| p.remote.is_some()) {
            return Err(EngineError(format!(
                "{what} nie działa, gdy panel pokazuje serwer SFTP: najpierw pobierz pliki (F5)"
            )));
        }
        Ok(())
    }

    /// F5 / F6 when a panel shows a server.
    pub(crate) fn remote_transfer(&mut self, remove_source: bool) -> Result<(), EngineError> {
        let (src, dst) = (self.active, 1 - self.active);
        let targets: Vec<PathBuf> = self.panels[src]
            .target_entries()
            .into_iter()
            .filter(|e| !is_parent_entry(e))
            .map(|e| e.path.clone())
            .collect();
        if targets.is_empty() {
            return Err(EngineError("nic nie zaznaczono".into()));
        }
        // Like every other transfer, this one uses up the selection.
        self.panels[src].selected.clear();
        let verb = if remove_source {
            "Przenoszenie"
        } else {
            "Kopiowanie"
        };
        let src_conn = self.remote_of(src).map(|r| r.conn.clone());
        let dst_conn = self.remote_of(dst).map(|r| r.conn.clone());
        match (src_conn, dst_conn) {
            (Some(from), Some(to)) => {
                let dest = self.remote_of(dst).expect("checked").path.clone();
                let sources: Vec<String> = targets.iter().map(|p| path_text(p)).collect();
                // The same saved site on both sides is one server: moving is a rename there.
                let same_server =
                    self.remote_of(src).map(|r| &r.site) == self.remote_of(dst).map(|r| &r.site);
                let op = if same_server && remove_source {
                    RemoteOp::MoveWithin { sources, dest }
                } else {
                    RemoteOp::Relay {
                        target: to,
                        sources,
                        dest,
                        remove_source,
                    }
                };
                self.status = format!("{verb} na serwerze…");
                self.enqueue_quiet(Self::remote_spec(src, Some(from), op));
                Ok(())
            }
            (Some(_), None) => {
                let dest = self.other_directory()?;
                self.status = format!("{verb}: pobieranie…");
                self.remote_op(
                    src,
                    RemoteOp::Download {
                        sources: targets.iter().map(|p| path_text(p)).collect(),
                        dest,
                        remove_source,
                        view: false,
                        edit: false,
                    },
                )
            }
            (None, Some(conn)) => {
                if self.panels[src].archive.is_some() {
                    return Err(EngineError(
                        "z archiwum nie wyślesz plików na serwer: najpierw rozpakuj".into(),
                    ));
                }
                let dest = self.remote_of(dst).expect("checked").path.clone();
                self.status = format!("{verb}: wysyłanie…");
                self.enqueue_quiet(Self::remote_spec(
                    dst,
                    Some(conn),
                    RemoteOp::Upload {
                        sources: targets,
                        dest,
                        remove_source,
                        replace: false,
                    },
                ));
                Ok(())
            }
            (None, None) => unreachable!("one panel shows a server"),
        }
    }

    /// F3 on a server: the file is fetched into a temporary folder and shown in the viewer.
    pub(crate) fn remote_view(&mut self) -> Result<(), EngineError> {
        let side = self.active;
        let entry = self.panels[side]
            .current()
            .filter(|e| !is_parent_entry(e))
            .cloned()
            .ok_or_else(|| EngineError("nic nie wskazano".into()))?;
        if entry.is_dir() {
            return Err(EngineError(format!("to katalog, nie plik: {}", entry.name)));
        }
        if entry.size > self.view_from_archive_limit {
            return Err(EngineError(format!(
                "plik na serwerze ma {}: za duży na podgląd (limit {}), pobierz go (F5)",
                crate::human_size(entry.size),
                crate::human_size(self.view_from_archive_limit)
            )));
        }
        let dir = std::env::temp_dir().join(format!(
            "bluepane-view-{}-{}",
            std::process::id(),
            crate::session::next_preview_number()
        ));
        self.status = format!("Pobieranie do podglądu: {}…", entry.name);
        self.remote_op(
            side,
            RemoteOp::Download {
                sources: vec![path_text(&entry.path)],
                dest: dir,
                remove_source: false,
                view: true,
                edit: false,
            },
        )
    }

    /// F4 on a server: fetch the file into a temporary folder and open it in the editor.
    pub(crate) fn remote_edit_start(&mut self) -> Result<(), EngineError> {
        let side = self.active;
        let entry = self.panels[side]
            .current()
            .filter(|e| !is_parent_entry(e))
            .cloned()
            .ok_or_else(|| EngineError("nic nie wskazano".into()))?;
        if entry.is_dir() {
            return Err(EngineError(format!("to katalog, nie plik: {}", entry.name)));
        }
        if entry.size > self.view_from_archive_limit {
            return Err(EngineError(format!(
                "plik na serwerze ma {}: za duży do edycji (limit {}), pobierz go (F5)",
                crate::human_size(entry.size),
                crate::human_size(self.view_from_archive_limit)
            )));
        }
        let dir = std::env::temp_dir().join(format!(
            "bluepane-edit-{}-{}",
            std::process::id(),
            crate::session::next_preview_number()
        ));
        self.status = format!("Pobieranie do edycji: {}…", entry.name);
        self.remote_op(
            side,
            RemoteOp::Download {
                sources: vec![path_text(&entry.path)],
                dest: dir,
                remove_source: false,
                view: false,
                edit: true,
            },
        )
    }

    /// `editsend`: send the edited copy back to the server (replacing the original).
    pub(crate) fn remote_edit_send(&mut self) -> Result<(), EngineError> {
        let Some(edit) = self.remote_edit.clone() else {
            return Err(EngineError(
                "nic nie jest w edycji: F4 na pliku z serwera pobiera go do edytora".into(),
            ));
        };
        let meta = std::fs::metadata(&edit.local)
            .map_err(|e| EngineError(format!("nie ma edytowanej kopii: {e}")))?;
        if meta.len() == edit.size && meta.modified().ok() == edit.modified {
            return Err(EngineError(
                "kopia się nie zmieniła: zapisz plik w edytorze".into(),
            ));
        }
        let side = (0..2)
            .find(|s| {
                self.panels[*s]
                    .remote
                    .as_ref()
                    .is_some_and(|r| Arc::ptr_eq(&r.conn, &edit.conn))
            })
            .ok_or_else(|| {
                EngineError("połączenie z serwerem zostało zamknięte: połącz się ponownie".into())
            })?;
        self.status = "Wysyłanie zmian na serwer…".into();
        self.enqueue_quiet(Self::remote_spec(
            side,
            Some(edit.conn.clone()),
            RemoteOp::Upload {
                sources: vec![edit.local.clone()],
                dest: remote_parent(&edit.remote_path),
                remove_source: false,
                replace: true,
            },
        ));
        Ok(())
    }

    pub(crate) fn remove_edit_dir(&mut self) {
        if let Some(edit) = self.remote_edit.take() {
            let _ = std::fs::remove_dir_all(&edit.dir);
        }
    }

    pub(crate) fn remote_mkdir(&mut self, name: &str) -> Result<(), EngineError> {
        let side = self.active;
        let name = name.trim();
        if !bluepane_ops::is_valid_name(name) {
            return Err(EngineError(format!("niepoprawna nazwa katalogu: {name}")));
        }
        let path = self
            .remote_of(side)
            .map(|r| remote_join(&r.path, name))
            .ok_or_else(|| EngineError("panel nie jest połączony z serwerem".into()))?;
        self.remote_op(side, RemoteOp::Mkdir { path })
    }

    /// `chmod` on a server panel: the selection (or the item under the cursor).
    pub(crate) fn remote_chmod(&mut self, mode: u32) -> Result<(), EngineError> {
        let side = self.active;
        if self.remote_of(side).is_none() {
            return Err(EngineError(
                "chmod działa tylko w panelu połączonym z serwerem".into(),
            ));
        }
        let paths: Vec<String> = self.panels[side]
            .target_entries()
            .into_iter()
            .filter(|e| !is_parent_entry(e))
            .map(|e| path_text(&e.path))
            .collect();
        if paths.is_empty() {
            return Err(EngineError("nic nie zaznaczono".into()));
        }
        self.remote_op(side, RemoteOp::Chmod { paths, mode })
    }

    pub(crate) fn remote_rename(&mut self, from: &Path, new_name: &str) -> Result<(), EngineError> {
        let side = self.active;
        let new_name = new_name.trim();
        if !bluepane_ops::is_valid_name(new_name) {
            return Err(EngineError(format!("niepoprawna nazwa: {new_name}")));
        }
        let from = path_text(from);
        let to = remote_join(&remote_parent(&from), new_name);
        self.remote_op(side, RemoteOp::Rename { from, to })
    }

    /// F8 on a server: nothing goes to a trash there, so the question says so plainly.
    pub(crate) fn remote_propose_delete(&mut self) -> Result<(), EngineError> {
        let side = self.active;
        let paths: Vec<String> = self.panels[side]
            .target_entries()
            .into_iter()
            .filter(|e| !is_parent_entry(e))
            .map(|e| path_text(&e.path))
            .collect();
        if paths.is_empty() {
            return Err(EngineError("nic nie zaznaczono".into()));
        }
        self.status = format!(
            "Usunąć z serwera: {}? TRWALE, serwer nie ma kosza (Enter = tak, Esc = nie)",
            if paths.len() == 1 {
                remote_name(&paths[0]).to_string()
            } else {
                format!("{} elementów", paths.len())
            }
        );
        self.pending = Some(Pending::ConfirmRemoteDelete { side, paths });
        Ok(())
    }

    pub(crate) fn remote_delete(
        &mut self,
        side: usize,
        paths: Vec<String>,
    ) -> Result<(), EngineError> {
        self.panels[side].selected.clear();
        self.remote_op(side, RemoteOp::Delete { paths })
    }

    /// The status line for a finished remote job, and the follow-up work (showing a new
    /// listing, refreshing after a change).
    pub(crate) fn remote_result(&mut self, spec: &JobSpec, report: &Report) -> (String, bool) {
        let Some(job) = spec.archive.as_ref().and_then(|a| a.remote.as_ref()) else {
            return ("Zadanie SFTP bez ustawień".into(), true);
        };
        let side = job.side;
        let failure = |report: &Report| report.failed.first().cloned();
        match &job.op {
            RemoteOp::Connect { site, .. } => {
                if let Some(prompt) = report.host_prompt.clone() {
                    let text = format!(
                        "Pierwsze połączenie z {}:{} ({}). Odcisk klucza: {}. Zaufać i zapamiętać? (Enter = tak, Esc = nie)",
                        prompt.host, prompt.port, prompt.algorithm, prompt.fingerprint
                    );
                    self.pending = Some(Pending::TrustHost(prompt));
                    return (text, false);
                }
                match (&report.connected, &report.remote_listing) {
                    (Some(conn), Some((path, list))) => {
                        let panel = &mut self.panels[side];
                        panel.archive = None;
                        panel.virtual_list = None;
                        panel.remote = Some(RemoteView {
                            site: site.name.clone(),
                            label: format!("{}@{}:{}", site.user, site.host, site.port),
                            path: path.clone(),
                            conn: conn.clone(),
                            history: vec![path.clone()],
                            history_index: 0,
                        });
                        panel.show_remote(path.clone(), remote_entries(path, list));
                        if let Some((name, dir)) = self.after_connect.take() {
                            if name == site.name {
                                let _ = self.remote_goto(side, dir);
                            }
                        }
                        (format!("Połączono z {}", site.label()), false)
                    }
                    _ => (
                        format!(
                            "Nie połączono: {}",
                            failure(report).unwrap_or_else(|| "nieznany błąd".into())
                        ),
                        true,
                    ),
                }
            }
            RemoteOp::SaveSecret { site, .. } => match failure(report) {
                None => (
                    match &site.secret {
                        SecretStore::Bitwarden(item) => {
                            format!("Zapisano hasło w Bitwarden (element „{item}”)")
                        }
                        _ => "Zapisano hasło w magazynie haseł systemu".to_string(),
                    },
                    false,
                ),
                Some(error) => (format!("Nie zapisano hasła: {error}"), true),
            },
            RemoteOp::List { .. } => match &report.remote_listing {
                Some((path, list)) => {
                    let same = job.conn.as_ref().is_some_and(|c| {
                        self.remote_of(side)
                            .is_some_and(|r| Arc::ptr_eq(&r.conn, c))
                    });
                    let moved = self.remote_of(side).is_none_or(|r| r.path != *path);
                    if same {
                        let panel = &mut self.panels[side];
                        let keep = panel.current().map(|e| e.path.clone());
                        if let Some(remote) = &mut panel.remote {
                            // Going to a folder the history is not already pointing at forgets
                            // what lay ahead (like a browser).
                            if remote.history.get(remote.history_index) != Some(path) {
                                remote.history.truncate(remote.history_index + 1);
                                remote.history.push(path.clone());
                                remote.history_index = remote.history.len() - 1;
                            }
                        }
                        panel.show_remote(path.clone(), remote_entries(path, list));
                        if let Some(keep) = keep {
                            panel.put_cursor_on(&keep);
                        }
                    }
                    if moved {
                        (format!("Serwer: {path}"), false)
                    } else {
                        // A refresh after a transfer or a change: keep its result on screen.
                        (self.status.clone(), self.status_is_error)
                    }
                }
                None => (
                    format!(
                        "Nie udało się wczytać katalogu: {}",
                        failure(report).unwrap_or_else(|| "nieznany błąd".into())
                    ),
                    true,
                ),
            },
            RemoteOp::Download {
                edit: true,
                dest,
                sources,
                ..
            } => {
                let name = remote_name(&sources[0]).to_string();
                let local = dest.join(&name);
                let conn = job.conn.clone();
                let meta = std::fs::metadata(&local);
                let (Some(conn), Ok(meta), true) = (conn, meta, report.failed.is_empty()) else {
                    let _ = std::fs::remove_dir_all(dest);
                    return (
                        format!(
                            "Nie udało się pobrać do edycji: {}",
                            failure(report).unwrap_or_else(|| "przerwano".into())
                        ),
                        true,
                    );
                };
                self.remove_edit_dir();
                // The state to compare with, taken before the editor can touch the file.
                self.remote_edit = Some(RemoteEdit {
                    conn,
                    remote_path: sources[0].clone(),
                    local: local.clone(),
                    dir: dest.clone(),
                    size: meta.len(),
                    modified: meta.modified().ok(),
                });
                match (self.editor.0)(&local) {
                    Ok(()) => (
                        format!("Edytujesz kopię {name}: po zapisaniu w edytorze wyślij ją na serwer (Ctrl+Shift+F4, komenda editsend)"),
                        false,
                    ),
                    Err(error) => (format!("Nie udało się otworzyć edytora: {error}"), true),
                }
            }
            RemoteOp::Download {
                view: true,
                dest,
                sources,
                ..
            } => {
                let file = dest.join(sources.first().map_or("", |s| remote_name(s)));
                if report.done == 1 && report.failed.is_empty() {
                    return self.show_fetched_document(&file, dest.clone(), "z serwera");
                }
                let _ = std::fs::remove_dir_all(dest);
                (
                    format!(
                        "Nie udało się pobrać do podglądu: {}",
                        failure(report).unwrap_or_else(|| "przerwano".into())
                    ),
                    true,
                )
            }
            op => {
                let verb = match op {
                    RemoteOp::Download {
                        remove_source: true,
                        ..
                    }
                    | RemoteOp::Upload {
                        remove_source: true,
                        ..
                    }
                    | RemoteOp::Relay {
                        remove_source: true,
                        ..
                    }
                    | RemoteOp::MoveWithin { .. } => "Przeniesiono",
                    RemoteOp::Relay { .. } => "Skopiowano między serwerami",
                    RemoteOp::Mkdir { .. } => "Utworzono katalog",
                    RemoteOp::Rename { .. } => "Zmieniono nazwę",
                    RemoteOp::Chmod { .. } => "Zmieniono uprawnienia",
                    RemoteOp::Delete { .. } => "Usunięto z serwera",
                    RemoteOp::Download { .. } => "Pobrano",
                    RemoteOp::Upload { replace: true, .. } => "Wysłano zmiany",
                    RemoteOp::Upload { .. } => "Wysłano",
                    _ => "Wykonano",
                };
                let mut text = match failure(report) {
                    Some(first) => format!("{verb}: {}, błąd: {first}", report.done),
                    None => format!("{verb}: {}", report.done),
                };
                if report.skipped > 0 {
                    text.push_str(&format!(", pominięto: {}", report.skipped));
                }
                if report.cancelled {
                    text = format!("Anulowano. {text}");
                }
                // The server changed (or a new file arrived locally): show it.
                self.remote_refresh_all();
                (text, !report.failed.is_empty())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bluepane_remote::testing::{start, write_file, Settings, TestServer};
    use std::fs;
    use std::time::Duration;

    struct Rig {
        tmp: tempfile::TempDir,
        server: TestServer,
        session: Session,
    }

    /// A server with `hello.txt`, `docs/a.txt`, `docs/sub/b.txt`; panel 0 is a local folder,
    /// panel 1 another; the site "Test" is saved (password stored locally).
    fn rig(secret: &str) -> Rig {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("server");
        write_file(root.join("hello.txt"), "hello world");
        write_file(root.join("docs/a.txt"), "alpha");
        write_file(root.join("docs/sub/b.txt"), "bravo");
        let server = start(Settings {
            root,
            user: "alice".into(),
            password: "secret".into(),
            authorized_key: None,
        });
        let (left, right) = (tmp.path().join("left"), tmp.path().join("right"));
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        write_file(left.join("up.txt"), "uploaded");
        let mut session = Session::new(&left, &right);
        session.use_config_dir(&tmp.path().join("config"));
        for (field, value) in [
            ("host", "127.0.0.1".to_string()),
            ("port", server.port.to_string()),
            ("user", "alice".to_string()),
            ("secret", secret.to_string()),
        ] {
            session
                .run_line(&format!("siteset Test {field} {value}"))
                .unwrap();
        }
        Rig {
            tmp,
            server,
            session,
        }
    }

    /// Runs the background jobs until they are done (or wait for an answer).
    fn settle(s: &mut Session) {
        for _ in 0..1000 {
            s.poll();
            if !s.is_busy() || matches!(s.pending, Some(Pending::Conflict(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("jobs did not finish: {}", s.status);
    }

    fn connect(r: &mut Rig) {
        r.session.run_line("connect Test").unwrap();
        settle(&mut r.session);
        assert!(
            matches!(r.session.pending, Some(Pending::TrustHost(_))),
            "{}",
            r.session.status
        );
        assert!(r.session.status.contains(&r.server.fingerprint));
        r.session.run_line("confirm").unwrap();
        settle(&mut r.session);
    }

    fn names(s: &Session, side: usize) -> Vec<String> {
        s.panels[side]
            .entries
            .iter()
            .map(|e| e.name.clone())
            .collect()
    }

    #[test]
    fn connecting_asks_to_trust_the_server_once_and_shows_its_files() {
        let mut r = rig("local:secret");
        connect(&mut r);
        let panel = &r.session.panels[0];
        assert_eq!(panel.remote.as_ref().unwrap().path, "/");
        assert_eq!(names(&r.session, 0), ["docs", "hello.txt"]);
        assert!(
            r.session.status.contains("Połączono"),
            "{}",
            r.session.status
        );
        assert!(r.tmp.path().join("config/known_hosts").exists());
        assert!(panel
            .location()
            .to_string_lossy()
            .starts_with("sftp://alice@127.0.0.1"));

        // The key is remembered: no question the second time.
        r.session.run_line("disconnect").unwrap();
        assert!(r.session.panels[0].remote.is_none());
        r.session.run_line("connect Test").unwrap();
        settle(&mut r.session);
        assert!(r.session.pending.is_none());
        assert!(r.session.panels[0].remote.is_some());
    }

    #[test]
    fn declining_the_key_does_not_connect() {
        let mut r = rig("local:secret");
        r.session.run_line("connect Test").unwrap();
        settle(&mut r.session);
        r.session.run_line("cancel").unwrap();
        assert!(r.session.panels[0].remote.is_none());
        assert!(!r.tmp.path().join("config/known_hosts").exists());
    }

    #[test]
    fn a_wrong_password_is_reported() {
        let mut r = rig("local:wrong");
        r.session.run_line("connect Test").unwrap();
        settle(&mut r.session);
        r.session.run_line("confirm").unwrap();
        settle(&mut r.session);
        assert!(r.session.panels[0].remote.is_none());
        assert!(r.session.status_is_error);
        assert!(
            r.session.status.contains("logowanie odrzucone"),
            "{}",
            r.session.status
        );
    }

    #[test]
    fn the_password_is_asked_for_when_it_is_not_stored() {
        let mut r = rig("ask");
        r.session.run_line("connect Test").unwrap();
        match &mut r.session.pending {
            Some(Pending::Input {
                kind: InputKind::SitePassword(site),
                text,
            }) => {
                assert_eq!(site, "Test");
                *text = "secret".into();
            }
            other => panic!("{other:?}"),
        }
        r.session.run_line("confirm").unwrap();
        settle(&mut r.session);
        r.session.run_line("confirm").unwrap(); // trust the key
        settle(&mut r.session);
        assert!(r.session.panels[0].remote.is_some(), "{}", r.session.status);
        // A password given on the command line skips the question.
        r.session.run_line("disconnect").unwrap();
        r.session.run_line("connect Test secret").unwrap();
        settle(&mut r.session);
        assert!(r.session.panels[0].remote.is_some());
    }

    #[test]
    fn navigating_the_server() {
        let mut r = rig("local:secret");
        connect(&mut r);
        r.session.run_line("select docs").unwrap();
        r.session.run_line("enter").unwrap();
        settle(&mut r.session);
        assert_eq!(r.session.panels[0].remote.as_ref().unwrap().path, "/docs");
        assert_eq!(names(&r.session, 0), ["..", "sub", "a.txt"]);
        r.session.run_line("cd sub").unwrap();
        settle(&mut r.session);
        assert_eq!(
            r.session.panels[0].remote.as_ref().unwrap().path,
            "/docs/sub"
        );
        r.session.run_line("up").unwrap();
        settle(&mut r.session);
        assert_eq!(r.session.panels[0].remote.as_ref().unwrap().path, "/docs");
        r.session.run_line("cd /").unwrap();
        settle(&mut r.session);
        assert_eq!(r.session.panels[0].remote.as_ref().unwrap().path, "/");
        r.session.run_line("up").unwrap();
        assert!(r.session.status.contains("korzeń"));
        // A folder that does not exist leaves the panel where it was.
        r.session.run_line("cd /nothing").unwrap();
        settle(&mut r.session);
        assert!(r.session.status_is_error);
        assert_eq!(r.session.panels[0].remote.as_ref().unwrap().path, "/");
    }

    #[test]
    fn downloading_files_and_folders_with_conflicts() {
        let mut r = rig("local:secret");
        connect(&mut r);
        r.session.run_line("select hello.txt").unwrap();
        r.session.run_line("select docs").unwrap();
        r.session.run_line("copy").unwrap();
        settle(&mut r.session);
        let right = r.tmp.path().join("right");
        assert_eq!(
            fs::read_to_string(right.join("hello.txt")).unwrap(),
            "hello world"
        );
        assert_eq!(
            fs::read_to_string(right.join("docs/sub/b.txt")).unwrap(),
            "bravo"
        );
        assert!(
            r.session.status.contains("Pobrano: 2"),
            "{}",
            r.session.status
        );

        // Again: the files exist, so the job asks; skip leaves them alone.
        fs::write(right.join("hello.txt"), "local edit").unwrap();
        r.session.run_line("select hello.txt").unwrap();
        r.session.run_line("copy").unwrap();
        settle(&mut r.session);
        assert!(matches!(r.session.pending, Some(Pending::Conflict(_))));
        r.session.run_line("skip").unwrap();
        settle(&mut r.session);
        assert_eq!(
            fs::read_to_string(right.join("hello.txt")).unwrap(),
            "local edit"
        );

        r.session.run_line("select hello.txt").unwrap();
        r.session.run_line("copy").unwrap();
        settle(&mut r.session);
        r.session.run_line("keepboth").unwrap();
        settle(&mut r.session);
        assert_eq!(
            fs::read_to_string(right.join("hello (kopia).txt")).unwrap(),
            "hello world"
        );

        r.session.run_line("select hello.txt").unwrap();
        r.session.run_line("copy").unwrap();
        settle(&mut r.session);
        r.session.run_line("overwrite").unwrap();
        settle(&mut r.session);
        assert_eq!(
            fs::read_to_string(right.join("hello.txt")).unwrap(),
            "hello world"
        );
    }

    #[test]
    fn uploading_files_and_folders() {
        let mut r = rig("local:secret");
        // The server in panel 1, the local folder in panel 0.
        r.session.run_line("switch").unwrap();
        connect(&mut r);
        r.session.run_line("switch").unwrap();
        write_file(r.tmp.path().join("left/dir/inner.txt"), "inner");
        r.session.run_line("refresh").unwrap();
        r.session.run_line("select up.txt").unwrap();
        r.session.run_line("select dir").unwrap();
        r.session.run_line("copy").unwrap();
        settle(&mut r.session);
        let root = &r.server.root;
        assert_eq!(fs::read_to_string(root.join("up.txt")).unwrap(), "uploaded");
        assert_eq!(
            fs::read_to_string(root.join("dir/inner.txt")).unwrap(),
            "inner"
        );
        assert!(
            r.session.status.contains("Wysłano: 2"),
            "{}",
            r.session.status
        );
        // The server listing in the other panel was refreshed.
        let listed = names(&r.session, 1);
        assert!(listed.contains(&"up.txt".to_string()) && listed.contains(&"dir".to_string()));

        // An existing remote file asks first and is not replaced by default.
        write_file(r.tmp.path().join("left/up.txt"), "changed");
        r.session.run_line("select up.txt").unwrap();
        r.session.run_line("copy").unwrap();
        settle(&mut r.session);
        assert!(matches!(r.session.pending, Some(Pending::Conflict(_))));
        r.session.run_line("skip").unwrap();
        settle(&mut r.session);
        assert_eq!(fs::read_to_string(root.join("up.txt")).unwrap(), "uploaded");
    }

    #[test]
    fn chmod_changes_permissions_of_the_selection_on_the_server_only() {
        let mut r = rig("local:secret");
        // A local panel has no such command.
        assert!(r.session.run_line("chmod 755").is_err());
        connect(&mut r);
        assert!(r.session.run_line("chmod 99").is_err());
        r.session.run_line("select hello.txt").unwrap();
        r.session.run_line("select docs").unwrap();
        r.session.run_line("chmod 640").unwrap();
        settle(&mut r.session);
        for name in ["hello.txt", "docs"] {
            assert_eq!(
                bluepane_remote::testing::last_chmod(r.server.root.join(name)),
                Some(0o640),
                "{name}"
            );
        }
        assert!(
            r.session.status.contains("Zmieniono uprawnienia: 2"),
            "{}",
            r.session.status
        );
    }

    #[test]
    fn making_renaming_and_deleting_on_the_server() {
        let mut r = rig("local:secret");
        connect(&mut r);
        r.session.run_line("mkdir fresh").unwrap();
        settle(&mut r.session);
        assert!(r.server.root.join("fresh").is_dir());
        assert!(names(&r.session, 0).contains(&"fresh".to_string()));
        assert!(r.session.run_line("mkdir a/b").is_err());

        r.session.run_line("select hello.txt").unwrap();
        r.session.run_line("rename renamed.txt").unwrap();
        settle(&mut r.session);
        assert!(r.server.root.join("renamed.txt").is_file());
        assert!(!r.server.root.join("hello.txt").exists());

        // Deleting asks, and says it is permanent.
        r.session.run_line("select renamed.txt").unwrap();
        r.session.run_line("select docs").unwrap();
        r.session.run_line("delete").unwrap();
        assert!(r.session.status.contains("TRWALE"), "{}", r.session.status);
        r.session.run_line("cancel").unwrap();
        assert!(
            r.server.root.join("docs").exists(),
            "declined: nothing happened"
        );
        r.session.run_line("select renamed.txt").unwrap();
        r.session.run_line("select docs").unwrap();
        r.session.run_line("delete").unwrap();
        r.session.run_line("confirm").unwrap();
        settle(&mut r.session);
        assert!(
            !r.server.root.join("docs").exists(),
            "folders go with everything inside"
        );
        assert!(!r.server.root.join("renamed.txt").exists());
        assert!(r.server.root.join("fresh").exists());
    }

    #[test]
    fn what_cannot_work_on_a_server_says_so() {
        let mut r = rig("local:secret");
        connect(&mut r);
        r.session.run_line("select hello.txt").unwrap();
        for line in ["pack", "multirename", "sync", "unpack"] {
            assert!(r.session.run_line(line).is_err(), "{line}");
        }
        assert!(r.session.run_line("connect Missing").is_err());
    }

    #[test]
    fn moving_from_the_server_deletes_only_what_arrived() {
        let mut r = rig("local:secret");
        connect(&mut r);
        let right = r.tmp.path().join("right");
        fs::write(right.join("hello.txt"), "already here").unwrap();
        r.session.run_line("select hello.txt").unwrap();
        r.session.run_line("select docs").unwrap();
        r.session.run_line("move").unwrap();
        settle(&mut r.session);
        // hello.txt conflicts: skipped, so it stays on the server; docs arrived in full.
        r.session.run_line("skip").unwrap();
        settle(&mut r.session);
        assert_eq!(
            fs::read_to_string(right.join("hello.txt")).unwrap(),
            "already here"
        );
        assert!(
            r.server.root.join("hello.txt").exists(),
            "a skipped file is not deleted"
        );
        assert_eq!(
            fs::read_to_string(right.join("docs/sub/b.txt")).unwrap(),
            "bravo"
        );
        assert!(
            !r.server.root.join("docs").exists(),
            "what arrived in full is gone"
        );
        assert!(
            r.session.status.contains("Przeniesiono"),
            "{}",
            r.session.status
        );
    }

    #[test]
    fn moving_to_the_server_keeps_the_local_file_when_skipped() {
        let mut r = rig("local:secret");
        r.session.run_line("switch").unwrap();
        connect(&mut r);
        r.session.run_line("switch").unwrap();
        write_file(r.server.root.join("up.txt"), "on the server");
        r.session.run_line("refresh").unwrap();
        settle(&mut r.session);
        r.session.run_line("select up.txt").unwrap();
        r.session.run_line("move").unwrap();
        settle(&mut r.session);
        r.session.run_line("skip").unwrap();
        settle(&mut r.session);
        assert_eq!(
            fs::read_to_string(r.tmp.path().join("left/up.txt")).unwrap(),
            "uploaded"
        );
        assert_eq!(
            fs::read_to_string(r.server.root.join("up.txt")).unwrap(),
            "on the server"
        );
    }

    #[test]
    fn copying_and_moving_between_two_connections() {
        let mut r = rig("local:secret");
        connect(&mut r);
        r.session.run_line("switch").unwrap();
        r.session.run_line("connect Test").unwrap();
        settle(&mut r.session);
        r.session.run_line("mkdir target").unwrap();
        settle(&mut r.session);
        r.session.run_line("select target").unwrap();
        r.session.run_line("enter").unwrap();
        settle(&mut r.session);
        assert_eq!(r.session.panels[1].remote.as_ref().unwrap().path, "/target");

        r.session.run_line("switch").unwrap();
        r.session.run_line("select docs").unwrap();
        r.session.run_line("copy").unwrap();
        settle(&mut r.session);
        assert_eq!(
            fs::read_to_string(r.server.root.join("target/docs/sub/b.txt")).unwrap(),
            "bravo"
        );
        assert!(
            r.server.root.join("docs/a.txt").exists(),
            "a copy leaves the source"
        );
        assert!(
            r.session.status.contains("między serwerami"),
            "{}",
            r.session.status
        );

        // Moving inside one server (the same saved site) is a rename.
        r.session.run_line("select hello.txt").unwrap();
        r.session.run_line("move").unwrap();
        settle(&mut r.session);
        assert!(
            r.server.root.join("target/hello.txt").exists(),
            "{}",
            r.session.status
        );
        assert!(!r.server.root.join("hello.txt").exists());
        // A move never replaces: the same name again is reported.
        write_file(r.server.root.join("hello.txt"), "second");
        r.session.run_line("refresh").unwrap();
        settle(&mut r.session);
        r.session.run_line("select hello.txt").unwrap();
        r.session.run_line("move").unwrap();
        settle(&mut r.session);
        assert!(r.session.status_is_error, "{}", r.session.status);
        assert_eq!(
            fs::read_to_string(r.server.root.join("hello.txt")).unwrap(),
            "second"
        );
        assert_eq!(
            fs::read_to_string(r.server.root.join("target/hello.txt")).unwrap(),
            "hello world"
        );
    }

    #[test]
    fn a_file_on_the_server_opens_in_the_viewer() {
        let mut r = rig("local:secret");
        connect(&mut r);
        r.session.run_line("select hello.txt").unwrap();
        r.session.run_line("view").unwrap();
        settle(&mut r.session);
        assert!(r.session.viewer.is_some(), "{}", r.session.status);
        assert!(r.session.preview_dir.is_some());
        let dir = r.session.preview_dir.clone().unwrap();
        r.session.remove_preview_dir();
        assert!(!dir.exists(), "the temporary copy is removed");
        // A folder, and a file too big for a preview, are refused.
        r.session.viewer = None;
        r.session.run_line("select docs").unwrap();
        assert!(r.session.run_line("view").is_err());
        r.session.view_from_archive_limit = 3;
        r.session.run_line("select hello.txt").unwrap();
        assert!(r.session.run_line("view").is_err());
    }

    /// An FTP (or FTPS) server in place of the SFTP one; panel 0 is local, panel 1 too.
    fn rig_ftp(
        tls: bluepane_remote::testing::Tls,
    ) -> (
        tempfile::TempDir,
        bluepane_remote::testing::FtpServer,
        Session,
    ) {
        use bluepane_remote::testing::{start_ftp, FtpSettings};
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("ftp");
        write_file(root.join("hello.txt"), "hello world");
        write_file(root.join("docs/a.txt"), "alpha");
        let server = start_ftp(FtpSettings {
            root,
            user: "alice".into(),
            password: "secret".into(),
            tls,
            no_mlsd: false,
        });
        let (left, right) = (tmp.path().join("left"), tmp.path().join("right"));
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        write_file(left.join("up.txt"), "uploaded");
        let mut session = Session::new(&left, &right);
        session.use_config_dir(&tmp.path().join("config"));
        let protocol = match tls {
            bluepane_remote::testing::Tls::None => "ftp",
            bluepane_remote::testing::Tls::Explicit => "ftps",
            bluepane_remote::testing::Tls::Implicit => "ftps-implicit",
        };
        for (field, value) in [
            ("protocol", protocol.to_string()),
            ("host", "127.0.0.1".to_string()),
            ("port", server.port.to_string()),
            ("user", "alice".to_string()),
            ("secret", "local:secret".to_string()),
        ] {
            session
                .run_line(&format!("siteset Files {field} {value}"))
                .unwrap();
        }
        (tmp, server, session)
    }

    #[test]
    fn plain_ftp_works_like_the_other_servers() {
        let (tmp, server, mut s) = rig_ftp(bluepane_remote::testing::Tls::None);
        s.run_line("connect Files").unwrap();
        settle(&mut s);
        assert!(s.pending.is_none(), "plain FTP has no key to confirm");
        assert_eq!(names(&s, 0), ["docs", "hello.txt"]);
        assert!(s.panels[0]
            .location()
            .to_string_lossy()
            .starts_with("sftp://"));
        s.run_line("select hello.txt").unwrap();
        s.run_line("copy").unwrap();
        settle(&mut s);
        assert_eq!(
            fs::read_to_string(tmp.path().join("right/hello.txt")).unwrap(),
            "hello world"
        );
        // Upload from a local panel, with a conflict question on the second go.
        s.run_line("disconnect").unwrap();
        s.run_line("switch").unwrap();
        s.run_line("connect Files").unwrap();
        settle(&mut s);
        s.run_line("switch").unwrap();
        s.run_line("select up.txt").unwrap();
        s.run_line("copy").unwrap();
        settle(&mut s);
        assert_eq!(
            fs::read_to_string(server.root.join("up.txt")).unwrap(),
            "uploaded"
        );
        s.run_line("select up.txt").unwrap();
        s.run_line("copy").unwrap();
        settle(&mut s);
        assert!(matches!(s.pending, Some(Pending::Conflict(_))));
        s.run_line("skip").unwrap();
        settle(&mut s);
        s.run_line("switch").unwrap();
        s.run_line("mkdir fresh").unwrap();
        settle(&mut s);
        assert!(server.root.join("fresh").is_dir());
        s.run_line("select hello.txt").unwrap();
        s.run_line("delete").unwrap();
        s.run_line("confirm").unwrap();
        settle(&mut s);
        assert!(!server.root.join("hello.txt").exists());
    }

    #[test]
    fn ftps_asks_to_trust_the_certificate_in_both_modes() {
        use bluepane_remote::testing::Tls;
        for tls in [Tls::Explicit, Tls::Implicit] {
            let (tmp, server, mut s) = rig_ftp(tls);
            s.run_line("connect Files").unwrap();
            settle(&mut s);
            assert!(
                matches!(s.pending, Some(Pending::TrustHost(_))),
                "{}",
                s.status
            );
            assert!(s.status.contains(server.fingerprint.as_ref().unwrap()));
            s.run_line("confirm").unwrap();
            settle(&mut s);
            assert!(s.panels[0].remote.is_some(), "{}", s.status);
            assert_eq!(names(&s, 0), ["docs", "hello.txt"]);
            s.run_line("select hello.txt").unwrap();
            s.run_line("copy").unwrap();
            settle(&mut s);
            assert_eq!(
                fs::read_to_string(tmp.path().join("right/hello.txt")).unwrap(),
                "hello world"
            );
            // Remembered: the second connection asks nothing.
            s.run_line("disconnect").unwrap();
            s.run_line("connect Files").unwrap();
            settle(&mut s);
            assert!(s.pending.is_none());
            assert!(s.panels[0].remote.is_some());
        }
    }

    #[test]
    fn the_protocol_of_a_site_can_be_changed_and_moves_the_default_port() {
        let mut s = Session::new("/tmp", "/tmp");
        s.use_config_dir(&tempfile::tempdir().unwrap().path().join("c"));
        s.run_line("siteset A host h").unwrap();
        assert_eq!(s.sites.get("A").unwrap().port, 22);
        s.run_line("siteset A protocol ftp").unwrap();
        assert_eq!(s.sites.get("A").unwrap().port, 21);
        s.run_line("siteset A protocol ftps-implicit").unwrap();
        assert_eq!(s.sites.get("A").unwrap().port, 990);
        s.run_line("siteset A port 2121").unwrap();
        s.run_line("siteset A protocol sftp").unwrap();
        assert_eq!(s.sites.get("A").unwrap().port, 2121, "a chosen port stays");
        assert!(s.run_line("siteset A protocol gopher").is_err());
        s.run_line("siteset A protocol ftp").unwrap();
        assert!(s.run_line("siteset A key /k").is_err(), "keys are SSH only");
        assert!(!s.sites.get("A").unwrap().active);
        s.run_line("siteset A mode active").unwrap();
        assert!(s.sites.get("A").unwrap().active);
        s.run_line("siteset A mode passive").unwrap();
        assert!(!s.sites.get("A").unwrap().active);
        assert!(s.run_line("siteset A mode sideways").is_err());
    }

    #[test]
    fn a_password_can_live_in_the_system_store_and_is_used_to_connect() {
        use bluepane_remote::testing::MemoryServices;
        use bluepane_remote::KeyStore;
        let mut r = rig("ask");
        let services = Arc::new(MemoryServices::default());
        r.session.secrets = services.clone();
        r.session.run_line("siteset Test secret keyring").unwrap();
        // Nothing saved yet: the connection explains what is missing.
        r.session.run_line("connect Test").unwrap();
        settle(&mut r.session);
        assert!(r.session.status_is_error);
        assert!(
            r.session.status.contains("magazynie haseł"),
            "{}",
            r.session.status
        );

        r.session.run_line("sitepassword Test secret").unwrap();
        settle(&mut r.session);
        assert!(
            r.session.status.contains("magazynie haseł systemu"),
            "{}",
            r.session.status
        );
        let user = r.session.sites.get("Test").unwrap().keyring_user();
        assert_eq!(
            services.keys.get("bluepane", &user).unwrap().as_deref(),
            Some("secret")
        );
        connect(&mut r);
        assert!(r.session.panels[0].remote.is_some(), "{}", r.session.status);
        // The sites file does not hold the password.
        let text = fs::read_to_string(r.tmp.path().join("config/sites.conf")).unwrap();
        assert!(
            text.contains("secret = keyring") && !text.contains("secret\n\n"),
            "{text}"
        );
    }

    #[test]
    fn a_password_can_be_saved_to_bitwarden_and_read_back_from_it() {
        use bluepane_remote::testing::MemoryServices;
        let mut r = rig("ask");
        let services = Arc::new(MemoryServices::default());
        r.session.secrets = services.clone();
        r.session
            .run_line("siteset Test secret bitwarden:bluepane: Test")
            .unwrap();
        r.session.run_line("sitepassword Test secret").unwrap();
        settle(&mut r.session);
        assert!(
            r.session.status.contains("Bitwarden"),
            "{}",
            r.session.status
        );
        assert_eq!(
            services.bitwarden.password_of("bluepane: Test").as_deref(),
            Some("secret")
        );
        let json = services.bitwarden.created.lock().unwrap()[0].clone();
        assert!(json.contains("\"username\":\"alice\""), "{json}");
        assert!(json.contains("sftp://127.0.0.1:"), "{json}");
        connect(&mut r);
        assert!(r.session.panels[0].remote.is_some(), "{}", r.session.status);
        // A second save does not overwrite the item.
        r.session.run_line("sitepassword Test other").unwrap();
        settle(&mut r.session);
        assert!(r.session.status_is_error);
        assert_eq!(
            services.bitwarden.password_of("bluepane: Test").as_deref(),
            Some("secret")
        );
    }

    #[test]
    fn saving_a_password_reports_a_locked_vault_without_leaking_it() {
        use bluepane_remote::testing::{FakeBitwarden, MemoryServices};
        let mut r = rig("ask");
        r.session.secrets = Arc::new(MemoryServices {
            bitwarden: FakeBitwarden {
                locked: true,
                ..Default::default()
            },
            ..Default::default()
        });
        r.session
            .run_line("siteset Test secret bitwarden:item")
            .unwrap();
        r.session.run_line("sitepassword Test hunter2").unwrap();
        settle(&mut r.session);
        assert!(r.session.status_is_error);
        assert!(
            r.session.status.contains("bw unlock"),
            "{}",
            r.session.status
        );
        assert!(!r.session.status.contains("hunter2"));
    }

    #[test]
    fn sitepassword_follows_where_the_site_keeps_its_secret() {
        let mut r = rig("ask");
        assert!(
            r.session.run_line("sitepassword Test pw").is_err(),
            "asked every time"
        );
        assert!(
            r.session.run_line("sitepassword Test").is_err(),
            "no password given"
        );
        assert!(r.session.run_line("sitepassword Missing pw").is_err());
        r.session.run_line("siteset Test secret local:old").unwrap();
        r.session
            .run_line("sitepassword Test new password")
            .unwrap();
        assert_eq!(
            r.session.sites.get("Test").unwrap().secret,
            SecretStore::Local("new password".into())
        );
    }

    fn path_of(s: &Session, side: usize) -> String {
        s.panels[side].remote.as_ref().unwrap().path.clone()
    }

    #[test]
    fn the_server_has_a_history_like_a_browser() {
        let mut r = rig("local:secret");
        connect(&mut r);
        r.session.run_line("cd docs").unwrap();
        settle(&mut r.session);
        r.session.run_line("cd sub").unwrap();
        settle(&mut r.session);
        assert_eq!(path_of(&r.session, 0), "/docs/sub");
        r.session.run_line("back").unwrap();
        settle(&mut r.session);
        assert_eq!(path_of(&r.session, 0), "/docs");
        r.session.run_line("back").unwrap();
        settle(&mut r.session);
        assert_eq!(path_of(&r.session, 0), "/");
        assert!(r.session.run_line("back").is_err(), "start of the history");
        r.session.run_line("forward").unwrap();
        settle(&mut r.session);
        assert_eq!(path_of(&r.session, 0), "/docs");
        // A new folder after going back forgets what lay ahead.
        r.session.run_line("back").unwrap();
        settle(&mut r.session);
        r.session.run_line("cd /docs/sub").unwrap();
        settle(&mut r.session);
        assert!(
            r.session.run_line("forward").is_err(),
            "nothing ahead any more"
        );
        r.session.run_line("back").unwrap();
        settle(&mut r.session);
        assert_eq!(path_of(&r.session, 0), "/");
        // A refresh is not a step in the history.
        r.session.run_line("refresh").unwrap();
        settle(&mut r.session);
        assert_eq!(
            r.session.panels[0].remote.as_ref().unwrap().history.len(),
            2
        );
    }

    #[test]
    fn tabs_on_a_server_share_the_connection() {
        let mut r = rig("local:secret");
        connect(&mut r);
        r.session.run_line("tab new").unwrap();
        assert_eq!(r.session.tab_strip(0).titles.len(), 2);
        assert_eq!(
            path_of(&r.session, 0),
            "/",
            "the new tab shows the same folder"
        );
        r.session.run_line("select docs").unwrap();
        r.session.run_line("enter").unwrap();
        settle(&mut r.session);
        assert_eq!(path_of(&r.session, 0), "/docs");
        r.session.run_line("tab 1").unwrap();
        settle(&mut r.session);
        assert_eq!(
            path_of(&r.session, 0),
            "/",
            "the first tab stayed where it was"
        );
        assert_eq!(names(&r.session, 0), ["docs", "hello.txt"]);
        // The folder under the cursor in a tab of its own.
        r.session.run_line("select docs").unwrap();
        r.session.run_line("tab open").unwrap();
        settle(&mut r.session);
        assert_eq!(r.session.tab_strip(0).titles.len(), 3);
        assert_eq!(path_of(&r.session, 0), "/docs");
        r.session.run_line("tab close").unwrap();
        r.session.run_line("tab close").unwrap();
        assert_eq!(r.session.tab_strip(0).titles.len(), 1);
        assert!(r.session.panels[0].remote.is_some());
    }

    #[test]
    fn a_file_on_the_server_can_be_edited_and_sent_back() {
        let mut r = rig("local:secret");
        // The "editor" appends a line to the local copy, like a save would.
        r.session.editor = crate::Editor(Arc::new(|path: &std::path::Path| {
            let mut text = fs::read_to_string(path).map_err(|e| e.to_string())?;
            text.push_str("\nedited");
            fs::write(path, text).map_err(|e| e.to_string())
        }));
        connect(&mut r);
        assert!(
            r.session.run_line("editsend").is_err(),
            "nothing is being edited"
        );
        r.session.run_line("select hello.txt").unwrap();
        r.session.run_line("edit").unwrap();
        settle(&mut r.session);
        assert!(
            r.session.status.contains("editsend"),
            "{}",
            r.session.status
        );
        let local = r.session.remote_edit.as_ref().unwrap().local.clone();
        assert_eq!(fs::read_to_string(&local).unwrap(), "hello world\nedited");
        // The server still has the original until the copy is sent back.
        assert_eq!(
            fs::read_to_string(r.server.root.join("hello.txt")).unwrap(),
            "hello world"
        );

        r.session.run_line("editsend").unwrap();
        settle(&mut r.session);
        assert!(
            r.session.pending.is_none(),
            "sending back an edit asks no question"
        );
        assert_eq!(
            fs::read_to_string(r.server.root.join("hello.txt")).unwrap(),
            "hello world\nedited"
        );
        assert!(
            r.session.status.contains("Wysłano zmiany"),
            "{}",
            r.session.status
        );
        assert!(!r.server.root.join("hello.txt.bpart").exists());

        // Only the original file was replaced.
        assert_eq!(names(&r.session, 0), ["docs", "hello.txt"]);
        // The temporary copy goes away with the next edit and with the session.
        let dir = r.session.remote_edit.as_ref().unwrap().dir.clone();
        r.session.remove_edit_dir();
        assert!(!dir.exists());
        assert!(r.session.run_line("editsend").is_err());
    }

    #[test]
    fn a_failing_editor_and_big_or_missing_files_are_reported() {
        let mut r = rig("local:secret");
        r.session.editor =
            crate::Editor(Arc::new(|_: &std::path::Path| Err("brak edytora".into())));
        connect(&mut r);
        r.session.run_line("select docs").unwrap();
        assert!(r.session.run_line("edit").is_err(), "a folder");
        r.session.run_line("select hello.txt").unwrap();
        r.session.view_from_archive_limit = 3;
        assert!(r.session.run_line("edit").is_err(), "too big");
        r.session.view_from_archive_limit = 1000;
        r.session.run_line("edit").unwrap();
        settle(&mut r.session);
        assert!(r.session.status_is_error);
        assert!(
            r.session.status.contains("brak edytora"),
            "{}",
            r.session.status
        );
    }

    #[test]
    fn a_site_needs_valid_data_and_can_be_deleted() {
        let mut s = Session::new("/tmp", "/tmp");
        s.use_config_dir(&tempfile::tempdir().unwrap().path().join("c"));
        s.run_line("siteset \"Home Assistant\" host ha.local")
            .unwrap();
        assert!(
            s.run_line("connect \"Home Assistant\"").is_err(),
            "no user yet"
        );
        s.run_line("siteset \"Home Assistant\" user hass").unwrap();
        s.run_line("siteset \"Home Assistant\" port 2222").unwrap();
        s.run_line("siteset \"Home Assistant\" secret bitwarden:ha")
            .unwrap();
        assert_eq!(s.sites.get("home assistant").unwrap().port, 2222);
        assert!(s.run_line("siteset X port abc").is_err());
        assert!(s.run_line("siteset X secret nonsense").is_err());
        assert!(s.run_line("siteset X bogus 1").is_err());
        s.run_line("sitedel \"Home Assistant\"").unwrap();
        assert!(s.run_line("sitedel \"Home Assistant\"").is_err());
        s.run_line("sites").unwrap();
        assert!(s.sites_open);
        s.run_line("siteclose").unwrap();
        assert!(!s.sites_open);
    }
}
