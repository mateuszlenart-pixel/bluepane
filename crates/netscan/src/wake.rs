//! Wake-on-LAN: a "magic packet" (six bytes of `0xFF`, then the hardware address sixteen
//! times) sent as a UDP broadcast. A computer whose network card is set to listen for it
//! switches on. Nothing comes back, so all we can say is that the packet was sent.

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};

/// The port conventionally used for the packet.
pub const WAKE_PORT: u16 = 9;

/// Reads `aa:bb:cc:dd:ee:ff`, `aa-bb-cc-dd-ee-ff` or `aabbccddeeff`.
pub fn parse_mac(text: &str) -> Result<[u8; 6], String> {
    let bad = || {
        format!(
            "„{}” nie jest adresem MAC (np. aa:bb:cc:dd:ee:ff)",
            text.trim()
        )
    };
    let digits: String = text
        .trim()
        .chars()
        .filter(|c| !matches!(c, ':' | '-' | '.'))
        .collect();
    if digits.len() != 12 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(bad());
    }
    let mut mac = [0u8; 6];
    for (i, byte) in mac.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&digits[i * 2..i * 2 + 2], 16).map_err(|_| bad())?;
    }
    Ok(mac)
}

/// The 102 bytes that wake the card with this address.
pub fn magic_packet(mac: [u8; 6]) -> Vec<u8> {
    let mut packet = vec![0xFF; 6];
    for _ in 0..16 {
        packet.extend_from_slice(&mac);
    }
    packet
}

/// Where to send the packet for a host: the limited broadcast, and the broadcast of its
/// `/24` network when the address is known (some systems do not route the first one out of
/// the right interface).
pub fn wake_targets(host: Option<Ipv4Addr>) -> Vec<SocketAddr> {
    let mut targets = vec![SocketAddr::from((Ipv4Addr::BROADCAST, WAKE_PORT))];
    if let Some(ip) = host {
        let [a, b, c, _] = ip.octets();
        targets.push(SocketAddr::from((Ipv4Addr::new(a, b, c, 255), WAKE_PORT)));
    }
    targets
}

/// Sends the packet for `mac` to every target; succeeds when at least one send worked.
pub fn send_wake(mac: [u8; 6], targets: &[SocketAddr]) -> Result<(), String> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .map_err(|e| format!("nie udało się otworzyć gniazda: {e}"))?;
    socket
        .set_broadcast(true)
        .map_err(|e| format!("nie udało się włączyć rozgłaszania: {e}"))?;
    let packet = magic_packet(mac);
    let mut last_error = None;
    let mut sent = false;
    for target in targets {
        match socket.send_to(&packet, target) {
            Ok(_) => sent = true,
            Err(e) => last_error = Some(e),
        }
    }
    if sent {
        Ok(())
    } else {
        Err(format!(
            "nie udało się wysłać pakietu: {}",
            last_error.map_or_else(|| "brak adresów".to_string(), |e| e.to_string())
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_addresses_are_read_in_the_usual_spellings() {
        let want = [0xaa, 0xbb, 0xcc, 0x01, 0x02, 0x03];
        for text in [
            "aa:bb:cc:01:02:03",
            "AA-BB-CC-01-02-03",
            " aabbcc010203 ",
            "aabb.cc01.0203",
        ] {
            assert_eq!(parse_mac(text).unwrap(), want, "{text}");
        }
        for bad in ["", "aa:bb:cc", "zz:bb:cc:01:02:03", "aa:bb:cc:01:02:03:04"] {
            assert!(parse_mac(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_magic_packet_is_six_ff_and_the_address_sixteen_times() {
        let mac = [1, 2, 3, 4, 5, 6];
        let packet = magic_packet(mac);
        assert_eq!(packet.len(), 102);
        assert_eq!(&packet[..6], &[0xFF; 6]);
        assert!(packet[6..].chunks(6).all(|c| c == mac));
    }

    #[test]
    fn targets_include_the_network_broadcast_of_a_known_host() {
        let targets = wake_targets(Some(Ipv4Addr::new(192, 168, 1, 40)));
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].ip(), Ipv4Addr::BROADCAST);
        assert_eq!(targets[1].ip(), Ipv4Addr::new(192, 168, 1, 255));
        assert_eq!(wake_targets(None).len(), 1);
    }

    #[test]
    fn a_packet_reaches_the_listener_unchanged() {
        let listener = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let target = listener.local_addr().unwrap();
        let mac = [0x10, 0x20, 0x30, 0x40, 0x50, 0x60];
        send_wake(mac, &[target]).unwrap();
        let mut buffer = [0u8; 200];
        let (n, _) = listener.recv_from(&mut buffer).unwrap();
        assert_eq!(&buffer[..n], magic_packet(mac).as_slice());
        // No target accepted the packet: an error, not silence.
        assert!(send_wake(mac, &[]).is_err());
    }
}
