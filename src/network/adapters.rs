//! Live OS view of network adapters (Windows: `GetAdaptersAddresses`).
//!
//! The registry keeps stale `DhcpIPAddress` values for disconnected
//! adapters, so "active" is decided by joining registry interface keys with
//! the adapters the OS reports as *up*.

use std::net::Ipv4Addr;

use super::ipv4::is_usable_ipv4;
use super::nagle::{ActiveAdapter, InterfaceSnapshot};
use crate::Result;

/// `IF_TYPE_SOFTWARE_LOOPBACK`
pub const IF_TYPE_LOOPBACK: u32 = 24;
/// `IF_TYPE_TUNNEL`
pub const IF_TYPE_TUNNEL: u32 = 131;

/// One adapter as reported by the OS.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OsAdapter {
    /// Interface GUID including braces, e.g. `{AC78B7B4-...}`.
    pub guid: String,
    /// Human-readable name, e.g. "Ethernet" or "Wi-Fi".
    pub friendly_name: String,
    /// `OperStatus == IfOperStatusUp`.
    pub is_up: bool,
    /// IANA ifType (6 = Ethernet, 71 = Wi-Fi, 24 = loopback, 131 = tunnel).
    pub if_type: u32,
    pub ipv4: Vec<Ipv4Addr>,
}

impl OsAdapter {
    /// Up, physical-ish (not loopback/tunnel).
    pub fn is_candidate(&self) -> bool {
        self.is_up && !matches!(self.if_type, IF_TYPE_LOOPBACK | IF_TYPE_TUNNEL)
    }
}

/// Port over the OS adapter list.
pub trait AdapterSource {
    fn adapters(&self) -> Result<Vec<OsAdapter>>;
}

impl<T: AdapterSource + ?Sized> AdapterSource for &T {
    fn adapters(&self) -> Result<Vec<OsAdapter>> {
        (**self).adapters()
    }
}

/// Decides which registry interfaces are active.
///
/// - `os = Some(..)`: an interface is active only if the OS reports a matching
///   (case-insensitive GUID) candidate adapter that has a usable IPv4. The
///   address comes from the OS and falls back to the registry. The OS
///   friendly name is attached.
/// - `os = None` (OS info unavailable): fall back to registry-only detection.
pub fn detect_active(snapshots: Vec<InterfaceSnapshot>, os: Option<&[OsAdapter]>) -> Vec<ActiveAdapter> {
    snapshots
        .into_iter()
        .filter_map(|snap| {
            let (name, ipv4) = match os {
                None => (None, snap.active_ipv4()?),
                Some(live) => {
                    let adapter = live
                        .iter()
                        .find(|a| a.is_candidate() && a.guid.eq_ignore_ascii_case(&snap.guid))?;
                    let ipv4 = adapter
                        .ipv4
                        .iter()
                        .copied()
                        .find(|ip| is_usable_ipv4(*ip))
                        .or_else(|| snap.active_ipv4())?;
                    (Some(adapter.friendly_name.clone()), ipv4)
                }
            };
            Some(ActiveAdapter {
                optimized: snap.is_optimized(),
                guid: snap.guid,
                name,
                ipv4,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(guid: &str, dhcp: &str) -> InterfaceSnapshot {
        InterfaceSnapshot {
            guid: guid.into(),
            dhcp_ip_address: Some(dhcp.into()),
            ..Default::default()
        }
    }

    fn os(guid: &str, name: &str, up: bool, ips: &[[u8; 4]]) -> OsAdapter {
        OsAdapter {
            guid: guid.into(),
            friendly_name: name.into(),
            is_up: up,
            if_type: 6,
            ipv4: ips.iter().map(|o| Ipv4Addr::from(*o)).collect(),
        }
    }

    #[test]
    fn candidate_requires_up_and_non_virtual_type() {
        let mut a = os("{A}", "Ethernet", true, &[]);
        assert!(a.is_candidate());
        a.is_up = false;
        assert!(!a.is_candidate());
        a.is_up = true;
        for t in [IF_TYPE_LOOPBACK, IF_TYPE_TUNNEL] {
            a.if_type = t;
            assert!(!a.is_candidate(), "ifType {t}");
        }
    }

    #[test]
    fn stale_dhcp_lease_on_disconnected_adapter_is_ignored() {
        let snaps = vec![snap("{eth}", "192.168.1.153"), snap("{wifi}", "192.168.1.185")];
        let live = [
            os("{ETH}", "Ethernet", true, &[[192, 168, 1, 153]]),
            os("{WIFI}", "Wi-Fi", false, &[]),
        ];
        let active = detect_active(snaps, Some(&live));
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].guid, "{eth}", "keeps the registry GUID spelling");
        assert_eq!(active[0].name.as_deref(), Some("Ethernet"));
        assert_eq!(active[0].ipv4, Ipv4Addr::new(192, 168, 1, 153));
    }

    #[test]
    fn os_address_wins_over_registry_address() {
        let snaps = vec![snap("{a}", "10.0.0.1")];
        let live = [os("{a}", "Ethernet", true, &[[10, 0, 0, 99]])];
        assert_eq!(
            detect_active(snaps, Some(&live))[0].ipv4,
            Ipv4Addr::new(10, 0, 0, 99)
        );
    }

    #[test]
    fn registry_address_used_when_os_reports_none_usable() {
        let snaps = vec![snap("{a}", "10.0.0.1")];
        let live = [os("{a}", "Ethernet", true, &[[169, 254, 1, 1]])];
        assert_eq!(
            detect_active(snaps, Some(&live))[0].ipv4,
            Ipv4Addr::new(10, 0, 0, 1)
        );
    }

    #[test]
    fn up_adapter_without_any_usable_ipv4_is_not_active() {
        let snaps = vec![snap("{a}", "0.0.0.0")];
        let live = [os("{a}", "Ethernet", true, &[])];
        assert!(detect_active(snaps, Some(&live)).is_empty());
    }

    #[test]
    fn registry_key_unknown_to_os_is_not_active() {
        let snaps = vec![snap("{ghost}", "10.0.0.1")];
        assert!(detect_active(snaps, Some(&[])).is_empty());
    }

    #[test]
    fn falls_back_to_registry_only_without_os_info() {
        let snaps = vec![snap("{a}", "10.0.0.1"), snap("{b}", "0.0.0.0")];
        let active = detect_active(snaps, None);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].guid, "{a}");
        assert_eq!(active[0].name, None);
    }

    #[test]
    fn optimized_flag_comes_from_registry_values() {
        let mut s = snap("{a}", "10.0.0.1");
        s.tcp_ack_frequency = Some(1);
        s.tcp_no_delay = Some(1);
        let live = [os("{a}", "Ethernet", true, &[[10, 0, 0, 1]])];
        assert!(detect_active(vec![s], Some(&live))[0].optimized);
    }
}
