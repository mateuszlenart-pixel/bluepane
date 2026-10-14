//! Reading the table of neighbours the operating system keeps (IP address -> MAC address).
//! Only the parsing lives here, so both formats can be tested on any system.

use std::net::Ipv4Addr;

/// `AA:BB:CC:DD:EE:FF` from `aa-bb-cc-dd-ee-ff`, `AA:BB:...` or `aabb.ccdd.eeff`; `None` when
/// the text is not a usable hardware address (wrong shape, all zeros, broadcast).
pub(crate) fn normalize_mac(text: &str) -> Option<String> {
    let hex: String = text
        .chars()
        .filter(|c| !matches!(c, ':' | '-' | '.'))
        .collect();
    if hex.len() != 12 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let hex = hex.to_ascii_uppercase();
    if hex == "000000000000" || hex == "FFFFFFFFFFFF" {
        return None;
    }
    let pairs: Vec<&str> = (0..6).map(|i| &hex[i * 2..i * 2 + 2]).collect();
    Some(pairs.join(":"))
}

/// Linux `/proc/net/arp`:
/// `IP address  HW type  Flags  HW address  Mask  Device`; flag `0x2` marks a complete entry.
#[cfg_attr(windows, allow(dead_code))] // only Linux reads /proc/net/arp
pub(crate) fn parse_proc_net_arp(text: &str) -> Vec<(Ipv4Addr, String)> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let [ip, _hw_type, flags, mac, ..] = fields.as_slice() else {
                return None;
            };
            let complete =
                u32::from_str_radix(flags.trim_start_matches("0x"), 16).is_ok_and(|f| f & 0x2 != 0);
            if !complete {
                return None;
            }
            Some((ip.parse().ok()?, normalize_mac(mac)?))
        })
        .collect()
}

/// The output of `arp -a` (Windows, also macOS/BSD in a different shape): any line with an
/// IPv4 address (possibly in parentheses) and a hardware address.
#[cfg_attr(not(windows), allow(dead_code))] // only Windows runs `arp -a`
pub(crate) fn parse_arp_a(text: &str) -> Vec<(Ipv4Addr, String)> {
    text.lines()
        .filter_map(|line| {
            let mut ip = None;
            let mut mac = None;
            for word in line.split_whitespace() {
                let word = word.trim_matches(|c| c == '(' || c == ')');
                if ip.is_none() {
                    if let Ok(parsed) = word.parse::<Ipv4Addr>() {
                        ip = Some(parsed);
                        continue;
                    }
                }
                if mac.is_none() {
                    mac = normalize_mac(word);
                }
            }
            let ip = ip?;
            // Multicast and broadcast addresses are not neighbours.
            if ip.is_multicast() || ip.is_broadcast() {
                return None;
            }
            Some((ip, mac?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macs_are_normalized_and_junk_is_refused() {
        assert_eq!(
            normalize_mac("aa-bb-cc-dd-ee-0f").as_deref(),
            Some("AA:BB:CC:DD:EE:0F")
        );
        assert_eq!(
            normalize_mac("AA:BB:CC:DD:EE:0F").as_deref(),
            Some("AA:BB:CC:DD:EE:0F")
        );
        assert_eq!(
            normalize_mac("aabb.ccdd.ee0f").as_deref(),
            Some("AA:BB:CC:DD:EE:0F")
        );
        assert_eq!(normalize_mac("00:00:00:00:00:00"), None);
        assert_eq!(normalize_mac("ff-ff-ff-ff-ff-ff"), None);
        assert_eq!(normalize_mac("not a mac"), None);
        assert_eq!(normalize_mac("aa:bb:cc"), None);
    }

    #[test]
    fn proc_net_arp_keeps_complete_entries_only() {
        let text = "\
IP address       HW type     Flags       HW address            Mask     Device
192.168.0.1      0x1         0x2         48:0f:cf:3c:bc:77     *        eth0
192.168.0.5      0x1         0x0         00:00:00:00:00:00     *        eth0
192.168.0.7      0x1         0x2         00:1b:78:1d:64:6f     *        eth0
";
        assert_eq!(
            parse_proc_net_arp(text),
            [
                (
                    "192.168.0.1".parse().unwrap(),
                    "48:0F:CF:3C:BC:77".to_string()
                ),
                (
                    "192.168.0.7".parse().unwrap(),
                    "00:1B:78:1D:64:6F".to_string()
                ),
            ]
        );
    }

    #[test]
    fn windows_arp_output_is_read_in_any_language() {
        let text = "
Interfejs: 192.168.0.20 --- 0x4
  Adres internetowy     Adres fizyczny        Typ
  192.168.0.1           48-0f-cf-3c-bc-77     dynamiczny
  192.168.0.255         ff-ff-ff-ff-ff-ff     statyczny
  224.0.0.22            01-00-5e-00-00-16     statyczny
";
        assert_eq!(
            parse_arp_a(text),
            [(
                "192.168.0.1".parse().unwrap(),
                "48:0F:CF:3C:BC:77".to_string()
            )]
        );
    }

    #[test]
    fn bsd_style_lines_work_too() {
        let text = "? (192.168.0.9) at aa:bb:cc:dd:ee:01 on en0 ifscope [ethernet]";
        assert_eq!(
            parse_arp_a(text),
            [(
                "192.168.0.9".parse().unwrap(),
                "AA:BB:CC:DD:EE:01".to_string()
            )]
        );
    }
}
