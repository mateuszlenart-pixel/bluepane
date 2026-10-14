//! Address ranges as people write them.

use std::net::{Ipv4Addr, UdpSocket};

/// The most addresses one scan may cover (a /16).
pub const MAX_ADDRESSES: usize = 65_536;

fn address(text: &str) -> Result<Ipv4Addr, String> {
    text.trim()
        .parse()
        .map_err(|_| format!("niepoprawny adres IP: {}", text.trim()))
}

/// `192.168.0.5`, `192.168.0.90-110`, `192.168.0.1-192.168.1.20`, `10.0.0.0/24`, or several
/// of those separated by commas. A `/24` skips the network and broadcast addresses.
pub fn parse_range(text: &str) -> Result<Vec<Ipv4Addr>, String> {
    let mut out: Vec<Ipv4Addr> = Vec::new();
    for part in text
        .split([',', ';'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let (from, to) = if let Some((base, bits)) = part.split_once('/') {
            let base = address(base)?;
            let bits: u32 = bits
                .trim()
                .parse()
                .ok()
                .filter(|b| *b <= 32)
                .ok_or_else(|| format!("niepoprawna długość maski: /{}", bits.trim()))?;
            let mask = if bits == 0 {
                0
            } else {
                u32::MAX << (32 - bits)
            };
            let (first, last) = (u32::from(base) & mask, u32::from(base) | !mask);
            // A normal subnet has a network and a broadcast address that are not hosts.
            if bits <= 30 {
                (first + 1, last - 1)
            } else {
                (first, last)
            }
        } else if let Some((left, right)) = part.split_once('-') {
            let left = address(left)?;
            // `192.168.0.90-110` shortens the end to its last number.
            let right = if right.trim().contains('.') {
                address(right)?
            } else {
                let last: u8 = right
                    .trim()
                    .parse()
                    .map_err(|_| format!("niepoprawny koniec zakresu: {}", right.trim()))?;
                let [a, b, c, _] = left.octets();
                Ipv4Addr::new(a, b, c, last)
            };
            (u32::from(left), u32::from(right))
        } else {
            let single = u32::from(address(part)?);
            (single, single)
        };
        if to < from {
            return Err(format!("zakres jest odwrócony: {part}"));
        }
        if (to - from) as usize + 1 + out.len() > MAX_ADDRESSES {
            return Err(format!(
                "za dużo adresów (najwyżej {MAX_ADDRESSES}): zawęź zakres"
            ));
        }
        out.extend((from..=to).map(Ipv4Addr::from));
    }
    if out.is_empty() {
        return Err("podaj zakres adresów, np. 192.168.0.1-254".into());
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|ip| seen.insert(*ip));
    Ok(out)
}

/// The range of the local network, assuming a /24: `192.168.0.1-254` for the machine
/// `192.168.0.20`. Nothing is sent: connecting a UDP socket only asks the system which
/// address it would use.
pub fn local_range() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_unspecified() => {
            let [a, b, c, _] = ip.octets();
            Some(format!("{a}.{b}.{c}.1-254"))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ips(text: &str) -> Vec<String> {
        parse_range(text)
            .unwrap()
            .into_iter()
            .map(|ip| ip.to_string())
            .collect()
    }

    #[test]
    fn single_addresses_and_short_ranges() {
        assert_eq!(ips("192.168.0.5"), ["192.168.0.5"]);
        assert_eq!(
            ips("192.168.0.90-92"),
            ["192.168.0.90", "192.168.0.91", "192.168.0.92"]
        );
        assert_eq!(
            ips("192.168.0.254-192.168.1.1"),
            [
                "192.168.0.254",
                "192.168.0.255",
                "192.168.1.0",
                "192.168.1.1"
            ]
        );
    }

    #[test]
    fn cidr_skips_network_and_broadcast() {
        let all = ips("10.0.0.0/24");
        assert_eq!(all.len(), 254);
        assert_eq!(all[0], "10.0.0.1");
        assert_eq!(all[253], "10.0.0.254");
        assert_eq!(ips("10.0.0.4/30"), ["10.0.0.5", "10.0.0.6"]);
        assert_eq!(ips("10.0.0.4/31"), ["10.0.0.4", "10.0.0.5"]);
        assert_eq!(ips("10.0.0.7/32"), ["10.0.0.7"]);
    }

    #[test]
    fn lists_are_joined_without_repeats() {
        assert_eq!(
            ips("192.168.0.1, 192.168.0.1-2 ;192.168.0.9, 192.168.0.1"),
            ["192.168.0.1", "192.168.0.2", "192.168.0.9"]
        );
    }

    #[test]
    fn nonsense_and_huge_ranges_are_refused() {
        for bad in [
            "",
            "abc",
            "192.168.0.300",
            "192.168.0.9-5",
            "10.0.0.0/33",
            "10.0.0.0/x",
            "1.2.3.4-x",
        ] {
            assert!(parse_range(bad).is_err(), "{bad:?}");
        }
        assert!(parse_range("10.0.0.0/8").is_err(), "a /8 is too much");
        assert!(parse_range("10.0.0.0/16").is_ok());
    }

    #[test]
    fn the_local_range_looks_like_a_slash_24() {
        if let Some(range) = local_range() {
            assert!(range.ends_with(".1-254"), "{range}");
            assert!(parse_range(&range).is_ok());
        }
    }
}
