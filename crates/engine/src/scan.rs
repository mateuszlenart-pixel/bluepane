//! The network scanner window's state and commands (`scan`, `scanset`, `scanrun`,
//! `scanstop`, `scanclose`). The scanning is `bluepane-netscan`; it runs on its own thread
//! and fills a shared table the window reads every frame.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use bluepane_netscan::{Host, ScanOptions, ScanSummary, COMMON_PORTS};
use bluepane_ops::Progress;

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
                self.scan = Some(ScanView {
                    form: ScanForm::new(),
                    visible: true,
                    data: Arc::default(),
                    progress: None,
                    started: false,
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
