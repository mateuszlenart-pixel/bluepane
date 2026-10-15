//! The network scanner window's state and commands (`scan`, `scanset`, `scanrun`,
//! `scanstop`, `scanclose`). The scanning is `bluepane-netscan`; it runs on its own thread
//! and fills a shared table the window reads every frame.

use std::collections::{BTreeMap, HashMap};
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use bluepane_netscan::{Host, ScanOptions, ScanSummary, COMMON_PORTS};
use bluepane_ops::Progress;
use bluepane_vfs::Share;

use crate::session::Session;
use crate::EngineError;

/// What the user typed in the scanner window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanForm {
    /// `192.168.0.1-254`, `10.0.0.0/24`, several separated by commas.
    pub range: String,
    /// Ports to look at on each host: `auto` (the usual ones) or `22,80,8000-8010`.
    pub ports: String,
    pub names: bool,
    pub arp: bool,
    /// Only rows containing this text are shown.
    pub filter: String,
    /// How long one connection may take, in milliseconds.
    pub timeout_ms: u64,
}

impl ScanForm {
    fn new() -> ScanForm {
        ScanForm {
            range: bluepane_netscan::local_range().unwrap_or_else(|| "192.168.0.1-254".into()),
            ports: "auto".into(),
            names: true,
            arp: true,
            filter: String::new(),
            timeout_ms: 500,
        }
    }
}

/// What a click on a service of a host does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launch {
    /// Open an address in the browser.
    Url(String),
    /// A remote desktop session to this host.
    RemoteDesktop(String),
    /// Open a share (`smb://host/name`, `\\host\name` on Windows) in the file manager.
    Share { host: String, name: String },
}

/// Starts something outside the program.
pub type LaunchFn = dyn Fn(&Launch) -> Result<(), String> + Send + Sync;

/// Lists the shares of a host (an address or a name).
pub type ListSharesFn = dyn Fn(&str) -> Result<Vec<Share>, String> + Send + Sync;

/// How things outside the program are started and asked; tests replace them.
#[derive(Clone)]
pub struct ScanServices {
    pub launch: Arc<LaunchFn>,
    pub list_shares: Arc<ListSharesFn>,
}

impl std::fmt::Debug for ScanServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScanServices")
    }
}

impl Default for ScanServices {
    fn default() -> Self {
        ScanServices {
            launch: Arc::new(|what| {
                match what {
                    Launch::Url(url) => {
                        bluepane_vfs::open_external(std::path::Path::new(url), None)
                    }
                    Launch::RemoteDesktop(host) => bluepane_vfs::open_remote_desktop(host),
                    Launch::Share { host, name } => {
                        let target = if cfg!(windows) {
                            format!("\\\\{host}\\{name}")
                        } else {
                            format!("smb://{host}/{name}")
                        };
                        bluepane_vfs::open_external(std::path::Path::new(&target), None)
                    }
                }
                .map_err(|e| e.to_string())
            }),
            list_shares: Arc::new(bluepane_vfs::list_shares),
        }
    }
}

/// The shares of one host, as they come in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShareList {
    Loading,
    Ready(Vec<Share>),
    Failed(String),
}

/// A host the user keeps an eye on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Favorite {
    pub ip: Ipv4Addr,
    pub name: String,
}

/// What the scanning thread produces.
#[derive(Debug, Default)]
pub struct ScanData {
    pub hosts: BTreeMap<Ipv4Addr, Host>,
    pub summary: Option<ScanSummary>,
    pub error: Option<String>,
}

#[derive(Debug)]
pub struct ScanView {
    pub form: ScanForm,
    pub visible: bool,
    data: Arc<Mutex<ScanData>>,
    progress: Option<Progress>,
    pub started: bool,
    /// The row the actions below the table work on.
    pub selected: Option<Ipv4Addr>,
    shares: Arc<Mutex<HashMap<Ipv4Addr, ShareList>>>,
    pub favorites: Vec<Favorite>,
    favorites_path: Option<PathBuf>,
    /// Show the favorites instead of the scan results.
    pub show_favorites: bool,
}

impl ScanView {
    pub fn data(&self) -> MutexGuard<'_, ScanData> {
        self.data.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn is_running(&self) -> bool {
        self.started && {
            let data = self.data();
            data.summary.is_none() && data.error.is_none()
        }
    }

    /// Done and total steps of the running scan, for a progress bar.
    pub fn progress(&self) -> Option<(u64, u64)> {
        let progress = self.progress.as_ref()?;
        Some((progress.items_done(), progress.items_total()))
    }

    /// The shares found (or being looked for) on a host.
    pub fn shares_of(&self, ip: Ipv4Addr) -> Option<ShareList> {
        self.shares
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&ip)
            .cloned()
    }

    pub fn is_favorite(&self, ip: Ipv4Addr) -> bool {
        self.favorites.iter().any(|f| f.ip == ip)
    }

    /// The host as last scanned, if it is in the results.
    pub fn host(&self, ip: Ipv4Addr) -> Option<Host> {
        self.data().hosts.get(&ip).cloned()
    }

    /// The hosts that match the filter text, in address order.
    pub fn visible_hosts(&self) -> Vec<Host> {
        let needle = self.form.filter.trim().to_lowercase();
        self.data()
            .hosts
            .values()
            .filter(|host| needle.is_empty() || matches_filter(host, &needle))
            .cloned()
            .collect()
    }
}

fn matches_filter(host: &Host, needle: &str) -> bool {
    let mut haystack = vec![host.ip.to_string()];
    haystack.extend(host.name.clone());
    haystack.extend(host.mac.clone());
    haystack.extend(host.vendor.clone());
    haystack.extend(
        host.services
            .iter()
            .map(|s| format!("{} {}", s.port, s.name)),
    );
    haystack
        .iter()
        .any(|text| text.to_lowercase().contains(needle))
}

/// `22,80,8000-8010` as a list of ports; `auto` (or nothing) is the usual list.
pub fn parse_ports(text: &str) -> Result<Vec<u16>, EngineError> {
    let text = text.trim();
    if text.is_empty() || text.eq_ignore_ascii_case("auto") {
        return Ok(COMMON_PORTS.to_vec());
    }
    let bad = |what: &str| EngineError(format!("niepoprawny port: {what} (np. 22,80,8000-8010)"));
    let mut ports = Vec::new();
    for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (from, to) = match part.split_once('-') {
            Some((a, b)) => (a.trim(), b.trim()),
            None => (part, part),
        };
        let from: u16 = from
            .parse()
            .ok()
            .filter(|p| *p > 0)
            .ok_or_else(|| bad(part))?;
        let to: u16 = to
            .parse()
            .ok()
            .filter(|p| *p >= from)
            .ok_or_else(|| bad(part))?;
        ports.extend(from..=to);
    }
    ports.sort_unstable();
    ports.dedup();
    if ports.len() > 1000 {
        return Err(EngineError("za dużo portów (najwyżej 1000)".into()));
    }
    Ok(ports)
}

fn parse_bool(field: &str, value: &str) -> Result<bool, EngineError> {
    match value.to_lowercase().as_str() {
        "" | "on" | "1" | "true" | "yes" | "tak" => Ok(true),
        "off" | "0" | "false" | "no" | "nie" => Ok(false),
        _ => Err(EngineError(format!(
            "scanset {field}: oczekiwano on albo off"
        ))),
    }
}

impl Session {
    /// `scan`: open the window (the last one again if there is one).
    pub(crate) fn open_scan(&mut self) {
        match &mut self.scan {
            Some(view) => view.visible = true,
            None => {
                self.load_config();
                let favorites_path = self.config_dir().map(|dir| dir.join("scan_favorites.conf"));
                let favorites = favorites_path
                    .as_deref()
                    .map(load_favorites)
                    .unwrap_or_default();
                self.scan = Some(ScanView {
                    form: ScanForm::new(),
                    visible: true,
                    data: Arc::default(),
                    progress: None,
                    started: false,
                    selected: None,
                    shares: Arc::default(),
                    favorites,
                    favorites_path,
                    show_favorites: false,
                });
            }
        }
        self.status = "Skaner sieci: ustaw zakres adresów i naciśnij Skanuj".into();
    }

    pub(crate) fn scan_set(&mut self, field: &str, value: &str) -> Result<(), EngineError> {
        let view = self
            .scan
            .as_mut()
            .ok_or_else(|| EngineError("najpierw otwórz skaner sieci (scan)".into()))?;
        let f = &mut view.form;
        match field {
            "range" => f.range = value.to_string(),
            "ports" => f.ports = value.to_string(),
            "names" => f.names = parse_bool(field, value)?,
            "arp" => f.arp = parse_bool(field, value)?,
            "filter" => f.filter = value.to_string(),
            "timeout" => {
                f.timeout_ms = value
                    .trim()
                    .parse()
                    .ok()
                    .filter(|ms| (50..=10_000).contains(ms))
                    .ok_or_else(|| EngineError("scanset timeout: 50-10000 (milisekundy)".into()))?;
            }
            "reset" => *f = ScanForm::new(),
            _ => return Err(EngineError(format!("scanset: nieznane pole {field}"))),
        }
        Ok(())
    }

    pub(crate) fn scan_run(&mut self) -> Result<(), EngineError> {
        let view = self
            .scan
            .as_mut()
            .ok_or_else(|| EngineError("najpierw otwórz skaner sieci (scan)".into()))?;
        let addresses = bluepane_netscan::parse_range(&view.form.range).map_err(EngineError)?;
        let mut options = ScanOptions::new(addresses);
        options.service_ports = parse_ports(&view.form.ports)?;
        options.resolve_names = view.form.names;
        options.use_arp = view.form.arp;
        options.timeout = Duration::from_millis(view.form.timeout_ms);
        // An older scan still running is told to stop; it keeps its own, orphaned, table.
        if let Some(old) = &view.progress {
            old.cancel();
        }
        let data: Arc<Mutex<ScanData>> = Arc::default();
        let progress = Progress::new();
        view.data = data.clone();
        view.progress = Some(progress.clone());
        view.started = true;
        let count = options.addresses.len();
        std::thread::spawn(move || {
            let summary = bluepane_netscan::scan(&options, &progress, &mut |host| {
                data.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .hosts
                    .insert(host.ip, host);
            });
            data.lock().unwrap_or_else(|e| e.into_inner()).summary = Some(summary);
        });
        self.status = format!("Skanowanie {count} adresów…");
        Ok(())
    }

    pub(crate) fn scan_stop(&mut self) {
        if let Some(progress) = self.scan.as_ref().and_then(|v| v.progress.as_ref()) {
            progress.cancel();
        }
    }

    pub(crate) fn scan_close(&mut self) {
        if let Some(view) = &mut self.scan {
            view.visible = false;
        }
    }
}

fn load_favorites(path: &std::path::Path) -> Vec<Favorite> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let (ip, name) = line.split_once('\t').unwrap_or((line, ""));
            Some(Favorite {
                ip: ip.trim().parse().ok()?,
                name: name.trim().to_string(),
            })
        })
        .collect()
}

fn save_favorites(path: &std::path::Path, favorites: &[Favorite]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text: String = favorites
        .iter()
        .map(|f| format!("{}\t{}\n", f.ip, f.name.replace(['\t', '\n'], " ")))
        .collect();
    std::fs::write(path, text)
}

/// The address to put in a URL: a name when the host has one is nicer, but the address always works.
fn url_for(scheme: &str, ip: Ipv4Addr, port: u16) -> String {
    let default = matches!((scheme, port), ("http", 80) | ("https", 443));
    if default {
        format!("{scheme}://{ip}/")
    } else {
        format!("{scheme}://{ip}:{port}/")
    }
}

impl Session {
    fn scan_view(&mut self) -> Result<&mut ScanView, EngineError> {
        self.scan
            .as_mut()
            .ok_or_else(|| EngineError("najpierw otwórz skaner sieci (scan)".into()))
    }

    pub(crate) fn scan_select(&mut self, ip: Ipv4Addr) -> Result<(), EngineError> {
        self.scan_view()?.selected = Some(ip);
        Ok(())
    }

    pub(crate) fn scan_favorite(&mut self, ip: Ipv4Addr) -> Result<(), EngineError> {
        let view = self.scan_view()?;
        let name = view.host(ip).and_then(|h| h.name).unwrap_or_default();
        let message = if let Some(at) = view.favorites.iter().position(|f| f.ip == ip) {
            view.favorites.remove(at);
            format!("Usunięto z ulubionych: {ip}")
        } else {
            view.favorites.push(Favorite { ip, name });
            view.favorites.sort_by_key(|f| f.ip);
            format!("Dodano do ulubionych: {ip}")
        };
        if let Some(path) = &view.favorites_path {
            save_favorites(path, &view.favorites)
                .map_err(|e| EngineError(format!("nie udało się zapisać ulubionych: {e}")))?;
        }
        self.status = message;
        Ok(())
    }

    pub(crate) fn scan_copy(&mut self, ip: Ipv4Addr) {
        self.status = format!("Skopiowano do schowka: {ip}");
        self.put_on_clipboard(ip.to_string());
    }

    /// `scanshares <ip>`: look for the shares in the background.
    pub(crate) fn scan_shares(&mut self, ip: Ipv4Addr) -> Result<(), EngineError> {
        let lister = self.scan_services.list_shares.clone();
        let view = self.scan_view()?;
        let table = view.shares.clone();
        table
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(ip, ShareList::Loading);
        std::thread::spawn(move || {
            let result = match lister(&ip.to_string()) {
                Ok(shares) => ShareList::Ready(shares),
                Err(why) => ShareList::Failed(why),
            };
            table
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(ip, result);
        });
        self.status = format!("Szukanie udziałów na {ip}…");
        Ok(())
    }

    /// `scanshare <ip> <name>`: open a share in the file manager.
    pub(crate) fn scan_share(&mut self, ip: Ipv4Addr, name: &str) -> Result<(), EngineError> {
        (self.scan_services.launch)(&Launch::Share {
            host: ip.to_string(),
            name: name.to_string(),
        })
        .map_err(|e| EngineError(format!("nie udało się otworzyć udziału: {e}")))?;
        self.status = format!("Otwarto udział {name} na {ip}");
        Ok(())
    }

    /// `scanopen <ip> <port>`: the usual thing to do with the service on that port.
    pub(crate) fn scan_open(&mut self, ip: Ipv4Addr, port: u16) -> Result<(), EngineError> {
        let url = |scheme: &str| Some(Launch::Url(url_for(scheme, ip, port)));
        let launch = match port {
            80 | 3000 | 5000 | 8000 | 8080 | 8123 | 8888 | 9000 => url("http"),
            443 | 5001 | 8006 | 8443 => url("https"),
            3389 => Some(Launch::RemoteDesktop(ip.to_string())),
            22 => return self.scan_site(ip, bluepane_remote::Protocol::Sftp),
            21 => return self.scan_site(ip, bluepane_remote::Protocol::Ftp),
            139 | 445 => return self.scan_shares(ip),
            _ => None,
        };
        let launch = launch.ok_or_else(|| {
            EngineError(format!(
                "nie ma domyślnej akcji dla portu {port} (skopiuj adres)"
            ))
        })?;
        (self.scan_services.launch)(&launch)
            .map_err(|e| EngineError(format!("nie udało się uruchomić: {e}")))?;
        self.status = match launch {
            Launch::Url(url) => format!("Otwarto w przeglądarce: {url}"),
            Launch::RemoteDesktop(host) => format!("Uruchomiono pulpit zdalny: {host}"),
            Launch::Share { .. } => String::new(),
        };
        Ok(())
    }

    /// `scansite <ip> <protocol>`: a saved server for this host, ready for the user's login.
    pub(crate) fn scan_site(
        &mut self,
        ip: Ipv4Addr,
        protocol: bluepane_remote::Protocol,
    ) -> Result<(), EngineError> {
        self.load_config();
        let host_name = self
            .scan
            .as_ref()
            .and_then(|v| v.host(ip))
            .and_then(|h| h.name);
        let label = host_name.unwrap_or_else(|| ip.to_string());
        let name = format!("{label} ({})", protocol.word());
        if self.sites.get(&name).is_none() {
            let mut site = bluepane_remote::Site::new(name.clone(), ip.to_string(), "");
            site.protocol = protocol;
            site.port = protocol.default_port();
            self.sites
                .upsert(site)
                .map_err(|e| EngineError(format!("nie udało się zapisać witryn: {e}")))?;
        }
        self.sites_open = true;
        self.scan_close();
        self.status = format!("Witryna „{name}” czeka: wpisz użytkownika i hasło, potem połącz");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn settle(s: &Session) {
        for _ in 0..1000 {
            if !s.scan.as_ref().unwrap().is_running() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the scan did not finish");
    }

    #[test]
    fn ports_are_parsed() {
        assert_eq!(parse_ports("auto").unwrap(), COMMON_PORTS);
        assert_eq!(parse_ports("").unwrap(), COMMON_PORTS);
        assert_eq!(
            parse_ports("80, 22,80,8000-8002").unwrap(),
            [22, 80, 8000, 8001, 8002]
        );
        for bad in ["0", "x", "70000", "10-5", "1-2000"] {
            assert!(parse_ports(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_scan_fills_the_table_and_the_filter_narrows_it() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut s = Session::new("/tmp", "/tmp");
        s.run_line("scan").unwrap();
        assert!(s.scan.as_ref().unwrap().visible);
        s.run_line("scanset range 127.0.0.1").unwrap();
        s.run_line(&format!("scanset ports {port}")).unwrap();
        s.run_line("scanset names off").unwrap();
        s.run_line("scanset arp off").unwrap();
        s.run_line("scanset timeout 300").unwrap();
        s.run_line("scanrun").unwrap();
        settle(&s);
        let view = s.scan.as_ref().unwrap();
        assert_eq!(view.data().summary.as_ref().unwrap().hosts, 1);
        let hosts = view.visible_hosts();
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].services[0].port, port);
        s.run_line("scanset filter 127.0.0").unwrap();
        assert_eq!(s.scan.as_ref().unwrap().visible_hosts().len(), 1);
        s.run_line("scanset filter nothing-like-this").unwrap();
        assert!(s.scan.as_ref().unwrap().visible_hosts().is_empty());
        s.run_line("scanset filter ").unwrap();
        s.run_line("scanclose").unwrap();
        assert!(!s.scan.as_ref().unwrap().visible);
        s.run_line("scan").unwrap();
        assert_eq!(
            s.scan.as_ref().unwrap().visible_hosts().len(),
            1,
            "results are kept"
        );
    }

    fn services(launches: Arc<Mutex<Vec<Launch>>>) -> ScanServices {
        ScanServices {
            launch: Arc::new(move |what| {
                if matches!(what, Launch::Url(u) if u.contains("fail")) {
                    return Err("brak przeglądarki".into());
                }
                launches.lock().unwrap().push(what.clone());
                Ok(())
            }),
            list_shares: Arc::new(|host| match host {
                "10.0.0.5" => Ok(vec![
                    Share {
                        name: "soft".into(),
                        kind: bluepane_vfs::ShareKind::Disk,
                        comment: "programy".into(),
                    },
                    Share {
                        name: "IPC$".into(),
                        kind: bluepane_vfs::ShareKind::Service,
                        comment: String::new(),
                    },
                ]),
                _ => Err("serwer wymaga logowania".into()),
            }),
        }
    }

    fn with_scanner() -> (tempfile::TempDir, Session, Arc<Mutex<Vec<Launch>>>) {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = Session::new(tmp.path(), tmp.path());
        s.use_config_dir(&tmp.path().join("config"));
        let launches = Arc::new(Mutex::new(Vec::new()));
        s.scan_services = services(launches.clone());
        s.run_line("scan").unwrap();
        (tmp, s, launches)
    }

    #[test]
    fn services_open_with_the_usual_program() {
        let (_tmp, mut s, launches) = with_scanner();
        for line in [
            "scanopen 10.0.0.5 80",
            "scanopen 10.0.0.5 443",
            "scanopen 10.0.0.5 8006",
            "scanopen 10.0.0.5 8123",
            "scanopen 10.0.0.5 3389",
        ] {
            s.run_line(line).unwrap();
        }
        assert_eq!(
            *launches.lock().unwrap(),
            [
                Launch::Url("http://10.0.0.5/".into()),
                Launch::Url("https://10.0.0.5/".into()),
                Launch::Url("https://10.0.0.5:8006/".into()),
                Launch::Url("http://10.0.0.5:8123/".into()),
                Launch::RemoteDesktop("10.0.0.5".into()),
            ]
        );
        assert!(
            s.run_line("scanopen 10.0.0.5 9999").is_err(),
            "no default action"
        );
        assert!(s.run_line("scanopen 10.0.0.5").is_err());
        assert!(s.run_line("scanopen nope 80").is_err());
    }

    #[test]
    fn ssh_and_ftp_become_saved_servers_waiting_for_a_login() {
        let (_tmp, mut s, _) = with_scanner();
        s.run_line("scanopen 10.0.0.5 22").unwrap();
        let site = s.sites.get("10.0.0.5 (sftp)").unwrap();
        assert_eq!(site.host, "10.0.0.5");
        assert_eq!(site.port, 22);
        assert_eq!(site.protocol, bluepane_remote::Protocol::Sftp);
        assert!(
            site.problem().is_some(),
            "no user yet, so no connecting by accident"
        );
        assert!(s.sites_open);
        assert!(
            !s.scan.as_ref().unwrap().visible,
            "the scanner makes room for the manager"
        );
        s.run_line("scan").unwrap();
        s.run_line("scansite 10.0.0.5 ftps").unwrap();
        assert_eq!(s.sites.get("10.0.0.5 (ftps)").unwrap().port, 21);
        assert!(s.run_line("scansite 10.0.0.5 gopher").is_err());
        assert!(s.run_line("scansite 10.0.0.5").is_err());
    }

    #[test]
    fn a_failing_program_is_reported() {
        let (_tmp, mut s, _) = with_scanner();
        // `fail` in the address makes the test launcher refuse; build such a url via port 80.
        s.scan_services.launch = Arc::new(|_| Err("brak przeglądarki".into()));
        let err = s.run_line("scanopen 10.0.0.5 80").unwrap_err();
        assert!(err.0.contains("brak przeglądarki"), "{err}");
    }

    #[test]
    fn shares_are_listed_in_the_background_and_can_be_opened() {
        let (_tmp, mut s, launches) = with_scanner();
        let ip: Ipv4Addr = "10.0.0.5".parse().unwrap();
        s.run_line("scanopen 10.0.0.5 445").unwrap();
        for _ in 0..500 {
            if matches!(
                s.scan.as_ref().unwrap().shares_of(ip),
                Some(ShareList::Ready(_))
            ) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let Some(ShareList::Ready(shares)) = s.scan.as_ref().unwrap().shares_of(ip) else {
            panic!("shares not listed");
        };
        assert_eq!(shares[0].name, "soft");
        s.run_line("scanshare 10.0.0.5 soft").unwrap();
        assert_eq!(
            launches.lock().unwrap().last(),
            Some(&Launch::Share {
                host: "10.0.0.5".into(),
                name: "soft".into()
            })
        );
        // A host that wants a login says so instead of showing an empty list.
        s.run_line("scanshares 10.0.0.9").unwrap();
        let other: Ipv4Addr = "10.0.0.9".parse().unwrap();
        for _ in 0..500 {
            if matches!(
                s.scan.as_ref().unwrap().shares_of(other),
                Some(ShareList::Failed(_))
            ) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(matches!(
            s.scan.as_ref().unwrap().shares_of(other),
            Some(ShareList::Failed(why)) if why.contains("logowania")
        ));
        assert!(s.run_line("scanshare 10.0.0.5").is_err());
    }

    #[test]
    fn favorites_are_kept_between_sessions() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("config");
        {
            let mut s = Session::new(tmp.path(), tmp.path());
            s.use_config_dir(&config);
            s.run_line("scan").unwrap();
            s.run_line("scanfav 10.0.0.7").unwrap();
            s.run_line("scanfav 10.0.0.2").unwrap();
            s.run_line("scanfav 10.0.0.7").unwrap();
            s.run_line("scanfav 10.0.0.7").unwrap();
            let view = s.scan.as_ref().unwrap();
            assert!(view.is_favorite("10.0.0.2".parse().unwrap()));
            assert_eq!(view.favorites.len(), 2);
            assert_eq!(
                view.favorites[0].ip,
                "10.0.0.2".parse::<Ipv4Addr>().unwrap(),
                "sorted"
            );
        }
        let mut s = Session::new(tmp.path(), tmp.path());
        s.use_config_dir(&config);
        s.run_line("scan").unwrap();
        assert_eq!(s.scan.as_ref().unwrap().favorites.len(), 2);
        s.run_line("scanfavorites").unwrap();
        assert!(s.scan.as_ref().unwrap().show_favorites);
        s.run_line("scanfavorites off").unwrap();
        assert!(!s.scan.as_ref().unwrap().show_favorites);
    }

    #[test]
    fn an_address_goes_to_the_clipboard_and_a_row_can_be_selected() {
        let (_tmp, mut s, _) = with_scanner();
        s.run_line("scancopy 192.168.0.20").unwrap();
        assert_eq!(s.take_clipboard().as_deref(), Some("192.168.0.20"));
        s.run_line("scanselect 192.168.0.20").unwrap();
        assert_eq!(
            s.scan.as_ref().unwrap().selected,
            Some("192.168.0.20".parse().unwrap())
        );
        assert!(s.run_line("scancopy 1.2.3").is_err());
    }

    #[test]
    fn bad_input_is_reported() {
        let mut s = Session::new("/tmp", "/tmp");
        assert!(s.run_line("scanrun").is_err(), "window not open");
        assert!(s.run_line("scanset range 1.2.3.4").is_err());
        s.run_line("scan").unwrap();
        s.run_line("scanset range nonsense").unwrap();
        assert!(s.run_line("scanrun").is_err());
        s.run_line("scanset range 10.0.0.0/8").unwrap();
        assert!(s.run_line("scanrun").is_err(), "too many addresses");
        s.run_line("scanset range 127.0.0.1").unwrap();
        s.run_line("scanset ports 99999").unwrap();
        assert!(s.run_line("scanrun").is_err());
        assert!(s.run_line("scanset timeout 5").is_err());
        assert!(s.run_line("scanset bogus 1").is_err());
        assert!(s.run_line("scanset names maybe").is_err());
        s.run_line("scanstop").unwrap();
    }
}
