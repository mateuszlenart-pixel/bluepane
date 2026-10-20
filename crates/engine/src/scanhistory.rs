//! What the scanner remembers between scans: the devices it has seen (by hardware address,
//! or by IP address when there is none) and a short log of the scans. After a finished scan a
//! device that was never seen before is marked as new; the very first scan only sets the
//! baseline, so it does not mark everything. Kept in `scan_known.conf` and `scan_log.conf` in
//! the configuration folder (tab-separated text), written only when a scan finishes.

use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::path::Path;
use std::time::{Duration, SystemTime};

use bluepane_netscan::Host;

/// How many scans the log keeps.
const LOG_MAX: usize = 100;

/// A device seen at least once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownDevice {
    /// The hardware address in lower case, or the IP address when there is none.
    pub key: String,
    pub mac: Option<String>,
    pub ip: Ipv4Addr,
    pub name: String,
    pub vendor: String,
    /// Seconds since the Unix epoch.
    pub first_seen: u64,
    pub last_seen: u64,
}

impl KnownDevice {
    pub fn first_seen_at(&self) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(self.first_seen)
    }

    pub fn last_seen_at(&self) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(self.last_seen)
    }
}

/// One finished scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanLogEntry {
    pub time: u64,
    pub range: String,
    pub hosts: usize,
    pub new: usize,
}

impl ScanLogEntry {
    pub fn at(&self) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(self.time)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanHistory {
    pub devices: Vec<KnownDevice>,
    pub log: Vec<ScanLogEntry>,
}

/// The key a host is remembered by.
pub fn device_key(host: &Host) -> String {
    host.mac
        .as_deref()
        .map(|m| m.to_lowercase().replace('-', ":"))
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| host.ip.to_string())
}

fn clean(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ")
}

impl ScanHistory {
    pub fn load(dir: &Path) -> ScanHistory {
        let devices = std::fs::read_to_string(dir.join("scan_known.conf"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| {
                let f: Vec<&str> = line.split('\t').collect();
                if f.len() < 7 {
                    return None;
                }
                Some(KnownDevice {
                    key: f[0].to_string(),
                    mac: Some(f[1].to_string()).filter(|m| !m.is_empty()),
                    ip: f[2].parse().ok()?,
                    name: f[3].to_string(),
                    vendor: f[4].to_string(),
                    first_seen: f[5].parse().ok()?,
                    last_seen: f[6].parse().ok()?,
                })
            })
            .collect();
        let log = std::fs::read_to_string(dir.join("scan_log.conf"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| {
                let f: Vec<&str> = line.split('\t').collect();
                if f.len() < 4 {
                    return None;
                }
                Some(ScanLogEntry {
                    time: f[0].parse().ok()?,
                    range: f[1].to_string(),
                    hosts: f[2].parse().ok()?,
                    new: f[3].parse().ok()?,
                })
            })
            .collect();
        ScanHistory { devices, log }
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let known: String = self
            .devices
            .iter()
            .map(|d| {
                format!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                    clean(&d.key),
                    d.mac.as_deref().unwrap_or(""),
                    d.ip,
                    clean(&d.name),
                    clean(&d.vendor),
                    d.first_seen,
                    d.last_seen
                )
            })
            .collect();
        std::fs::write(dir.join("scan_known.conf"), known)?;
        let log: String = self
            .log
            .iter()
            .map(|e| format!("{}\t{}\t{}\t{}\n", e.time, clean(&e.range), e.hosts, e.new))
            .collect();
        std::fs::write(dir.join("scan_log.conf"), log)
    }

    /// Notes a finished scan; returns the keys of the devices that were never seen before.
    /// With no devices remembered yet this is the baseline and nothing counts as new.
    pub fn record(&mut self, hosts: &[Host], range: &str, now: u64) -> HashSet<String> {
        let baseline = self.devices.is_empty();
        let mut new = HashSet::new();
        for host in hosts {
            let key = device_key(host);
            let name = host.name.clone().unwrap_or_default();
            let vendor = host.vendor.clone().unwrap_or_default();
            match self.devices.iter_mut().find(|d| d.key == key) {
                Some(device) => {
                    device.ip = host.ip;
                    device.last_seen = now;
                    // A name or maker learned later is kept; an empty one never wipes it.
                    if !name.is_empty() {
                        device.name = name;
                    }
                    if !vendor.is_empty() {
                        device.vendor = vendor;
                    }
                    if device.mac.is_none() {
                        device.mac = host.mac.clone().map(|m| m.to_lowercase());
                    }
                }
                None => {
                    if !baseline {
                        new.insert(key.clone());
                    }
                    self.devices.push(KnownDevice {
                        key,
                        mac: host.mac.clone().map(|m| m.to_lowercase()),
                        ip: host.ip,
                        name,
                        vendor,
                        first_seen: now,
                        last_seen: now,
                    });
                }
            }
        }
        self.devices.sort_by_key(|d| d.ip);
        self.log.push(ScanLogEntry {
            time: now,
            range: range.to_string(),
            hosts: hosts.len(),
            new: new.len(),
        });
        if self.log.len() > LOG_MAX {
            let extra = self.log.len() - LOG_MAX;
            self.log.drain(..extra);
        }
        new
    }

    /// The remembered hardware address of a device with this IP address.
    pub fn mac_of(&self, ip: Ipv4Addr) -> Option<&str> {
        self.devices
            .iter()
            .find(|d| d.ip == ip)
            .and_then(|d| d.mac.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(last: u8, mac: Option<&str>, name: Option<&str>) -> Host {
        Host {
            ip: Ipv4Addr::new(192, 168, 0, last),
            name: name.map(str::to_string),
            mac: mac.map(str::to_string),
            vendor: None,
            services: Vec::new(),
            latency: None,
        }
    }

    #[test]
    fn the_first_scan_is_a_baseline_and_later_ones_mark_only_strangers() {
        let mut history = ScanHistory::default();
        let first = [
            host(10, Some("AA:BB:CC:00:00:01"), Some("drukarka")),
            host(11, None, None),
        ];
        assert!(history.record(&first, "192.168.0.1-254", 100).is_empty());
        assert_eq!(history.devices.len(), 2);

        // Same devices (the first one under another address), plus a stranger.
        let second = [
            host(20, Some("aa:bb:cc:00:00:01"), None),
            host(11, None, None),
            host(30, Some("AA:BB:CC:00:00:02"), Some("zażółć")),
        ];
        let new = history.record(&second, "192.168.0.1-254", 200);
        assert_eq!(new, HashSet::from(["aa:bb:cc:00:00:02".to_string()]));
        let moved = history
            .devices
            .iter()
            .find(|d| d.key == "aa:bb:cc:00:00:01")
            .unwrap();
        assert_eq!(moved.ip, Ipv4Addr::new(192, 168, 0, 20));
        assert_eq!((moved.first_seen, moved.last_seen), (100, 200));
        assert_eq!(
            moved.name, "drukarka",
            "an unknown name does not wipe a known one"
        );
        assert_eq!(
            history.mac_of(Ipv4Addr::new(192, 168, 0, 30)),
            Some("aa:bb:cc:00:00:02")
        );
        assert_eq!(history.mac_of(Ipv4Addr::new(192, 168, 0, 11)), None);
        assert_eq!(history.log.len(), 2);
        assert_eq!((history.log[1].hosts, history.log[1].new), (3, 1));
    }

    #[test]
    fn history_survives_a_round_trip_and_the_log_is_capped() {
        let dir = tempfile::tempdir().unwrap();
        let mut history = ScanHistory::default();
        history.record(
            &[host(5, Some("aa:aa:aa:aa:aa:aa"), Some("serwer\tNAS"))],
            "r\tx",
            7,
        );
        for i in 0..(LOG_MAX as u64 + 20) {
            history.record(&[], "r", 10 + i);
        }
        assert_eq!(history.log.len(), LOG_MAX);
        history.save(dir.path()).unwrap();
        let loaded = ScanHistory::load(dir.path());
        assert_eq!(loaded.devices.len(), 1);
        assert_eq!(loaded.devices[0].name, "serwer NAS");
        assert_eq!(loaded.devices[0].mac.as_deref(), Some("aa:aa:aa:aa:aa:aa"));
        assert_eq!(loaded.log, history.log);
        // Nothing saved yet: empty, not an error.
        assert_eq!(
            ScanHistory::load(&dir.path().join("none")),
            ScanHistory::default()
        );
    }
}
