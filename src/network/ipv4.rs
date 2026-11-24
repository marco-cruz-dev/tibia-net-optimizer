//! IPv4 validation for detecting "active" network interfaces.

use std::net::Ipv4Addr;

/// Parses `s` as an IPv4 address that indicates a real, connected adapter.
///
/// Rejects `0.0.0.0`, loopback (`127.x`), APIPA link-local (`169.254.x`,
/// meaning DHCP failed) and broadcast.
pub fn parse_usable_ipv4(s: &str) -> Option<Ipv4Addr> {
    let ip: Ipv4Addr = s.trim().parse().ok()?;
    is_usable_ipv4(ip).then_some(ip)
}

/// See [`parse_usable_ipv4`] for what counts as usable.
pub fn is_usable_ipv4(ip: Ipv4Addr) -> bool {
    !(ip.is_unspecified() || ip.is_loopback() || ip.is_link_local() || ip.is_broadcast())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_private_and_public_addresses() {
        assert_eq!(
            parse_usable_ipv4("192.168.1.10"),
            Some(Ipv4Addr::new(192, 168, 1, 10))
        );
        assert_eq!(parse_usable_ipv4("10.0.0.5"), Some(Ipv4Addr::new(10, 0, 0, 5)));
        assert_eq!(
            parse_usable_ipv4("85.240.1.2"),
            Some(Ipv4Addr::new(85, 240, 1, 2))
        );
    }

    #[test]
    fn trims_whitespace() {
        assert!(parse_usable_ipv4("  192.168.0.2 \r\n").is_some());
    }

    #[test]
    fn rejects_unspecified_loopback_linklocal_broadcast() {
        for s in ["0.0.0.0", "127.0.0.1", "169.254.10.20", "255.255.255.255"] {
            assert_eq!(parse_usable_ipv4(s), None, "{s} should be rejected");
        }
    }

    #[test]
    fn rejects_malformed_and_ipv6() {
        for s in ["", "abc", "192.168.1", "256.1.1.1", "fe80::1", "192.168.1.1/24"] {
            assert_eq!(parse_usable_ipv4(s), None, "{s:?} should be rejected");
        }
    }
}
