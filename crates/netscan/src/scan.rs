//! The scan itself: find the hosts that are there, then look at what they offer.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use bluepane_ops::Progress;

use crate::services::{service_name, vendor_of, COMMON_PORTS, PROBE_PORTS};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanOptions {
    pub addresses: Vec<Ipv4Addr>,
    /// Ports tried to find out whether a host is there.
    pub probe_ports: Vec<u16>,
    /// Ports listed for every host that is there.
    pub service_ports: Vec<u16>,
    /// How long one connection attempt may take.
    pub timeout: Duration,
    /// How many connections are tried at once.
    pub threads: usize,
    /// Ask the name service for each host's name (can be slow on some networks).
    pub resolve_names: bool,
    /// Count a host as present when the system's neighbour table has its MAC address.
    pub use_arp: bool,
}

impl ScanOptions {
    pub fn new(addresses: Vec<Ipv4Addr>) -> ScanOptions {
        ScanOptions {
            addresses,
            probe_ports: PROBE_PORTS.to_vec(),
            service_ports: COMMON_PORTS.to_vec(),
            timeout: Duration::from_millis(500),
            threads: 128,
            resolve_names: true,
            use_arp: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Service {
    pub port: u16,
    /// `HTTPS`, `SSH / SFTP`, or `port 1234` for a port with no well-known use.
    pub name: String,
}

/// What is known about one host. A host is reported several times as the scan learns more
/// (first that it is there, then its services, then its name); the last report is the full one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    pub ip: Ipv4Addr,
    pub name: Option<String>,
    pub mac: Option<String>,
    pub vendor: Option<String>,
    pub services: Vec<Service>,
    /// How long the first answering connection took.
    pub latency: Option<Duration>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanSummary {
    pub addresses: usize,
    pub hosts: usize,
    pub cancelled: bool,
}

/// What a connection attempt tells about a host.
enum Answer {
    Open,
    /// Refused: nothing listens there, but the host is.
    Refused,
    Silent,
}

fn try_connect(ip: Ipv4Addr, port: u16, timeout: Duration) -> (Answer, Duration) {
    let started = Instant::now();
    let answer = match TcpStream::connect_timeout(&SocketAddr::from((ip, port)), timeout) {
        Ok(_) => Answer::Open,
        Err(e)
            if matches!(
                e.kind(),
                ErrorKind::ConnectionRefused | ErrorKind::ConnectionReset
            ) =>
        {
            Answer::Refused
        }
        Err(_) => Answer::Silent,
    };
    (answer, started.elapsed())
}

/// Runs `work(index)` for every index below `count` on up to `threads` threads.
fn parallel(count: usize, threads: usize, progress: &Progress, work: &(dyn Fn(usize) + Sync)) {
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads.clamp(1, count.max(1)) {
            scope.spawn(|| loop {
                if progress.is_cancelled() {
                    return;
                }
                let index = next.fetch_add(1, Ordering::Relaxed);
                if index >= count {
                    return;
                }
                work(index);
            });
        }
    });
}

/// Scans. `each` is called from the calling thread with a snapshot of a host whenever
/// something new is known about it.
pub fn scan(options: &ScanOptions, progress: &Progress, each: &mut dyn FnMut(Host)) -> ScanSummary {
    let mut summary = ScanSummary {
        addresses: options.addresses.len(),
        ..ScanSummary::default()
    };

    // 1. Who is there? Port by port over all addresses, so a host that answers the first
    //    probe is not asked the others.
    let probes: Vec<(Ipv4Addr, u16)> = options
        .probe_ports
        .iter()
        .flat_map(|port| options.addresses.iter().map(move |ip| (*ip, *port)))
        .collect();
    progress.set_total(0, probes.len() as u64);
    let found: Mutex<HashMap<Ipv4Addr, (Duration, BTreeSet<u16>)>> = Mutex::new(HashMap::new());
    parallel(probes.len(), options.threads, progress, &|index| {
        let (ip, port) = probes[index];
        let known = found.lock().unwrap().contains_key(&ip);
        if !known {
            let (answer, took) = try_connect(ip, port, options.timeout);
            if !matches!(answer, Answer::Silent) {
                let mut found = found.lock().unwrap();
                let entry = found.entry(ip).or_insert((took, BTreeSet::new()));
                if matches!(answer, Answer::Open) {
                    entry.1.insert(port);
                }
            }
        }
        progress.finish_item();
    });
    let mut found = found.into_inner().unwrap();

    // Hosts that firewall every port still answer ARP on the local network.
    let neighbours: HashMap<Ipv4Addr, String> = if options.use_arp {
        bluepane_vfs::neighbours().into_iter().collect()
    } else {
        HashMap::new()
    };
    let wanted: BTreeSet<Ipv4Addr> = options.addresses.iter().copied().collect();
    for ip in neighbours.keys().filter(|ip| wanted.contains(ip)) {
        found
            .entry(*ip)
            .or_insert((Duration::ZERO, BTreeSet::new()));
    }
    if progress.is_cancelled() {
        summary.cancelled = true;
        return summary;
    }

    let mut hosts: BTreeMap<Ipv4Addr, Host> = BTreeMap::new();
    for (ip, (latency, _)) in &found {
        let mac = neighbours.get(ip).cloned();
        hosts.insert(
            *ip,
            Host {
                ip: *ip,
                name: None,
                vendor: mac.as_deref().and_then(vendor_of),
                mac,
                services: Vec::new(),
                latency: (!latency.is_zero()).then_some(*latency),
            },
        );
    }
    summary.hosts = hosts.len();
    for host in hosts.values() {
        each(host.clone());
    }

    // 2. What do they offer?
    let pairs: Vec<(Ipv4Addr, u16)> = hosts
        .keys()
        .flat_map(|ip| options.service_ports.iter().map(move |port| (*ip, *port)))
        .collect();
    progress.set_total(0, (probes.len() + pairs.len()) as u64);
    let open: Mutex<HashMap<Ipv4Addr, BTreeSet<u16>>> = Mutex::new(
        found
            .iter()
            .map(|(ip, (_, ports))| (*ip, ports.clone()))
            .collect(),
    );
    parallel(pairs.len(), options.threads, progress, &|index| {
        let (ip, port) = pairs[index];
        if let (Answer::Open, _) = try_connect(ip, port, options.timeout) {
            open.lock().unwrap().entry(ip).or_default().insert(port);
        }
        progress.finish_item();
    });
    let open = open.into_inner().unwrap();
    for (ip, host) in &mut hosts {
        host.services = open
            .get(ip)
            .into_iter()
            .flatten()
            .map(|port| Service {
                port: *port,
                name: service_name(*port).map_or_else(|| format!("port {port}"), String::from),
            })
            .collect();
        each(host.clone());
    }
    if progress.is_cancelled() {
        summary.cancelled = true;
        return summary;
    }

    // 3. Names (the slowest part, so it comes last and hosts appear before it).
    if options.resolve_names {
        let list: Vec<Ipv4Addr> = hosts.keys().copied().collect();
        let names: Mutex<HashMap<Ipv4Addr, String>> = Mutex::new(HashMap::new());
        parallel(list.len(), 16, progress, &|index| {
            let ip = list[index];
            if let Ok(name) = dns_lookup::lookup_addr(&IpAddr::V4(ip)) {
                // Some resolvers answer with the address itself when there is no name.
                if name != ip.to_string() && !name.is_empty() {
                    names.lock().unwrap().insert(ip, name);
                }
            }
        });
        let names = names.into_inner().unwrap();
        for (ip, host) in &mut hosts {
            if let Some(name) = names.get(ip) {
                host.name = Some(name.clone());
                each(host.clone());
            }
        }
    }
    summary.cancelled = progress.is_cancelled();
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn listener() -> (TcpListener, u16) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    /// A port nothing listens on (bound once, then released).
    fn closed_port() -> u16 {
        listener().1
    }

    fn options(ports: &[u16], services: &[u16]) -> ScanOptions {
        ScanOptions {
            probe_ports: ports.to_vec(),
            service_ports: services.to_vec(),
            resolve_names: false,
            use_arp: false,
            timeout: Duration::from_millis(300),
            ..ScanOptions::new(vec![Ipv4Addr::LOCALHOST])
        }
    }

    fn run(options: &ScanOptions) -> (Vec<Host>, ScanSummary) {
        let mut reports = Vec::new();
        let summary = scan(options, &Progress::new(), &mut |h| reports.push(h));
        (reports, summary)
    }

    #[test]
    fn a_listening_host_is_found_with_its_open_ports() {
        let (_open, port) = listener();
        let dead = closed_port();
        let (reports, summary) = run(&options(&[port], &[port, dead]));
        assert_eq!(summary.hosts, 1);
        assert!(!summary.cancelled);
        let last = reports.last().unwrap();
        assert_eq!(last.ip, Ipv4Addr::LOCALHOST);
        assert_eq!(
            last.services,
            [Service {
                port,
                name: format!("port {port}")
            }]
        );
        assert!(last.latency.is_some());
        // First "it is there", then the same host with its services.
        assert!(reports.len() >= 2 && reports[0].services.is_empty());
    }

    #[test]
    fn a_refusing_host_is_still_a_host() {
        let dead = closed_port();
        let (reports, summary) = run(&options(&[dead], &[dead]));
        assert_eq!(summary.hosts, 1, "a reset means somebody is there");
        assert!(reports.last().unwrap().services.is_empty());
    }

    #[test]
    fn well_known_ports_get_their_names() {
        // Port 22 or 80 may or may not be open here: only the name logic is checked.
        assert_eq!(service_name(22), Some("SSH / SFTP"));
        let (_open, port) = listener();
        let (reports, _) = run(&options(&[port], &[port]));
        assert!(reports.last().unwrap().services[0]
            .name
            .starts_with("port "));
    }

    #[test]
    fn nothing_is_found_where_nothing_answers() {
        // An address the system cannot route to anything: no answer, no host.
        let mut o = options(&[closed_port()], &[]);
        o.addresses = vec![Ipv4Addr::new(192, 0, 2, 1)];
        o.timeout = Duration::from_millis(100);
        let (reports, summary) = run(&o);
        assert!(reports.is_empty() && summary.hosts == 0);
    }

    #[test]
    fn a_cancelled_scan_reports_so() {
        let (_open, port) = listener();
        let progress = Progress::new();
        progress.cancel();
        let mut reports = Vec::new();
        let summary = scan(&options(&[port], &[port]), &progress, &mut |h| {
            reports.push(h)
        });
        assert!(summary.cancelled);
        assert!(reports.is_empty());
    }

    #[test]
    fn several_addresses_are_scanned_together() {
        let (_open, port) = listener();
        let mut o = options(&[port], &[port]);
        o.addresses = vec![Ipv4Addr::LOCALHOST, Ipv4Addr::new(127, 0, 0, 2)];
        let (reports, summary) = run(&o);
        // 127.0.0.2 is local too on Linux; elsewhere it may not answer on this port.
        assert!(summary.hosts >= 1 && summary.addresses == 2);
        assert!(reports.iter().any(|h| h.ip == Ipv4Addr::LOCALHOST));
    }

    #[test]
    fn progress_counts_the_work() {
        let (_open, port) = listener();
        let progress = Progress::new();
        scan(&options(&[port], &[port]), &progress, &mut |_| {});
        assert!(progress.items_total() >= 2);
        assert_eq!(progress.items_done(), progress.items_total());
    }
}
