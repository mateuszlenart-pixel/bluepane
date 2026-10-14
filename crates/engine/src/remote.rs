//! SFTP in the session: the site manager, connecting, browsing a server folder in a panel
//! and copying to and from it. All network work runs as [`JobKind::Remote`] jobs.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bluepane_sftp::{KnownHosts, RemoteEntry, SecretStore, Site, Sites};
use bluepane_vfs::{Entry, EntryKind};

use crate::job::{ArchiveJob, JobKind, JobSpec, RemoteJob, RemoteOp, Report};
use crate::panel::{is_parent_entry, RemoteView};
use crate::remote_job::{remote_join, remote_name, remote_parent};
use crate::session::{InputKind, Pending, Session};
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
            hidden: e.name.starts_with('.'),
        })
        .collect()
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

impl Session {
    /// Reads the saved sites and remembered server keys from the config directory (once).
    pub(crate) fn load_config(&mut self) {
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
        match field {
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
            "secret" => {
                site.secret = if let Some(secret) = value.strip_prefix("local:") {
                    SecretStore::Local(secret.to_string())
                } else if let Some(item) = value.strip_prefix("bitwarden:") {
                    SecretStore::Bitwarden(item.trim().to_string())
                } else if value.trim().eq_ignore_ascii_case("ask") || value.trim().is_empty() {
                    SecretStore::Ask
                } else {
                    return Err(EngineError(
                        "siteset secret: ask, local:<hasło> albo bitwarden:<element>".into(),
                    ));
                };
            }
            _ => return Err(EngineError(format!("siteset: nieznane pole {field}"))),
        }
        if let SecretStore::Bitwarden(_) = site.secret {
            if !bluepane_sftp::bitwarden_available() {
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
        if password.is_none() && site.secret == SecretStore::Ask {
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
        conn: Option<Arc<bluepane_sftp::Connection>>,
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
        let (src_remote, dst_remote) = (
            self.panels[src].remote.is_some(),
            self.panels[dst].remote.is_some(),
        );
        if src_remote && dst_remote {
            return Err(EngineError(
                "kopiowanie między dwoma serwerami nie jest dostępne: pobierz pliki na dysk, potem wyślij".into(),
            ));
        }
        if remove_source {
            return Err(EngineError(
                "przenoszenie przez SFTP nie jest dostępne: skopiuj (F5), sprawdź, a potem usuń (F8)".into(),
            ));
        }
        let targets: Vec<PathBuf> = self.panels[src]
            .target_entries()
            .into_iter()
            .filter(|e| !is_parent_entry(e))
            .map(|e| e.path.clone())
            .collect();
        if targets.is_empty() {
            return Err(EngineError("nic nie zaznaczono".into()));
        }
        if src_remote {
            let dest = self.other_directory()?;
            self.status = "Pobieranie…".into();
            self.remote_op(
                src,
                RemoteOp::Download {
                    sources: targets.iter().map(|p| path_text(p)).collect(),
                    dest,
                },
            )
        } else {
            let (dest, conn) = {
                let remote = self.remote_of(dst).expect("checked");
                (remote.path.clone(), remote.conn.clone())
            };
            if self.panels[src].archive.is_some() {
                return Err(EngineError(
                    "z archiwum nie wyślesz plików na serwer: najpierw rozpakuj".into(),
                ));
            }
            self.status = "Wysyłanie…".into();
            self.enqueue_quiet(Self::remote_spec(
                dst,
                Some(conn),
                RemoteOp::Upload {
                    sources: targets,
                    dest,
                },
            ));
            Ok(())
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
                        });
                        panel.show_remote(path.clone(), remote_entries(path, list));
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
            op => {
                let verb = match op {
                    RemoteOp::Mkdir { .. } => "Utworzono katalog",
                    RemoteOp::Rename { .. } => "Zmieniono nazwę",
                    RemoteOp::Delete { .. } => "Usunięto z serwera",
                    RemoteOp::Download { .. } => "Pobrano",
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
    use bluepane_sftp::testing::{start, write_file, Settings, TestServer};
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
        assert!(r.session.run_line("back").is_err());
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
        for line in [
            "move",
            "view",
            "edit",
            "pack",
            "multirename",
            "sync",
            "unpack",
        ] {
            assert!(r.session.run_line(line).is_err(), "{line}");
        }
        // Two servers at once cannot copy between each other.
        r.session.run_line("switch").unwrap();
        r.session.run_line("connect Test").unwrap();
        settle(&mut r.session);
        r.session.run_line("select docs").unwrap();
        assert!(r.session.run_line("copy").is_err());
        assert!(r.session.run_line("connect Missing").is_err());
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
