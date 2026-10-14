//! What the usual ports are, and who made a device (from its MAC address).

/// Ports tried to find out whether a host is there at all, most likely first.
pub const PROBE_PORTS: &[u16] = &[
    443, 80, 445, 22, 3389, 135, 139, 8080, 53, 8006, 8123, 21, 23, 5900, 9100, 631,
];

/// Ports looked at on every host that is there.
pub const COMMON_PORTS: &[u16] = &[
    21, 22, 23, 25, 53, 80, 110, 111, 135, 139, 143, 389, 443, 445, 515, 548, 554, 631, 993, 995,
    1433, 1883, 3000, 3306, 3389, 5000, 5001, 5060, 5432, 5900, 6379, 8000, 8006, 8080, 8123, 8443,
    8888, 9000, 9100, 9200, 32400,
];

/// The service usually found on a port.
pub fn service_name(port: u16) -> Option<&'static str> {
    Some(match port {
        21 => "FTP",
        22 => "SSH / SFTP",
        23 => "Telnet",
        25 => "SMTP",
        53 => "DNS",
        80 => "HTTP",
        110 => "POP3",
        111 => "RPC (NFS)",
        135 => "RPC (Windows)",
        139 => "NetBIOS",
        143 => "IMAP",
        389 => "LDAP",
        443 => "HTTPS",
        445 => "SMB (udziały)",
        515 => "LPD (drukarka)",
        548 => "AFP",
        554 => "RTSP (kamera)",
        631 => "IPP (drukarka)",
        993 => "IMAPS",
        995 => "POP3S",
        1433 => "SQL Server",
        1883 => "MQTT",
        3000 => "HTTP (3000)",
        3306 => "MySQL",
        3389 => "RDP (pulpit zdalny)",
        5000 => "HTTP (5000, np. Synology)",
        5001 => "HTTPS (5001, np. Synology)",
        5060 => "SIP",
        5432 => "PostgreSQL",
        5900 => "VNC",
        6379 => "Redis",
        8000 => "HTTP (8000)",
        8006 => "Proxmox VE",
        8080 => "HTTP (8080)",
        8123 => "Home Assistant",
        8443 => "HTTPS (8443)",
        8888 => "HTTP (8888)",
        9000 => "HTTP (9000)",
        9100 => "JetDirect (drukarka)",
        9200 => "Elasticsearch",
        32400 => "Plex",
        _ => return None,
    })
}

/// The manufacturer behind a MAC address (IEEE registry, built into the program).
pub fn vendor_of(mac: &str) -> Option<String> {
    oui_data::lookup(mac).map(|record| record.organization().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_port_has_a_name() {
        for port in COMMON_PORTS.iter().chain(PROBE_PORTS) {
            assert!(service_name(*port).is_some(), "port {port}");
        }
        assert_eq!(service_name(8006), Some("Proxmox VE"));
        assert_eq!(service_name(8123), Some("Home Assistant"));
        assert_eq!(service_name(2), None);
    }

    #[test]
    fn probe_ports_are_a_subset_of_the_common_ones() {
        for port in PROBE_PORTS {
            assert!(COMMON_PORTS.contains(port), "{port}");
        }
    }

    #[test]
    fn vendors_come_from_the_registry() {
        let cisco = vendor_of("00:00:0C:11:22:33").unwrap();
        assert!(cisco.to_lowercase().contains("cisco"), "{cisco}");
        assert_eq!(vendor_of("not a mac"), None);
    }
}
