//! Nagle's algorithm / delayed ACK tuning via per-interface registry values
//! under `HKLM\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces`.

use std::net::Ipv4Addr;

use super::adapters::{detect_active, AdapterSource};
use super::ipv4::parse_usable_ipv4;
use crate::Result;

pub const TCP_ACK_FREQUENCY: &str = "TcpAckFrequency";
pub const TCP_NO_DELAY: &str = "TCPNoDelay";

/// Raw registry data for one interface subkey.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InterfaceSnapshot {
    pub guid: String,
    /// `DhcpIPAddress` (REG_SZ).
    pub dhcp_ip_address: Option<String>,
    /// `IPAddress` (REG_MULTI_SZ) — static configuration.
    pub ip_addresses: Vec<String>,
    pub tcp_ack_frequency: Option<u32>,
    pub tcp_no_delay: Option<u32>,
}

impl InterfaceSnapshot {
    /// First usable IPv4: DHCP lease first, then static addresses.
    pub fn active_ipv4(&self) -> Option<Ipv4Addr> {
        self.dhcp_ip_address
            .iter()
            .chain(self.ip_addresses.iter())
            .find_map(|s| parse_usable_ipv4(s))
    }

    pub fn is_optimized(&self) -> bool {
        self.tcp_ack_frequency == Some(1) && self.tcp_no_delay == Some(1)
    }

    fn has_tuning_values(&self) -> bool {
        self.tcp_ack_frequency.is_some() || self.tcp_no_delay.is_some()
    }
}

/// Port over the registry so logic is testable without touching HKLM.
pub trait InterfaceRegistry {
    fn interfaces(&self) -> Result<Vec<InterfaceSnapshot>>;
    fn set_dword(&self, guid: &str, name: &str, value: u32) -> Result<()>;
    /// Deleting a value that doesn't exist must succeed.
    fn delete_value(&self, guid: &str, name: &str) -> Result<()>;
}

/// A connected interface with a usable IPv4 address (what the UI shows).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveAdapter {
    pub guid: String,
    /// OS friendly name ("Ethernet"), when OS adapter info is available.
    pub name: Option<String>,
    pub ipv4: Ipv4Addr,
    pub optimized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NagleStatus {
    /// Every active adapter has both values set to 1.
    Optimized,
    /// At least one active adapter is missing the tuning.
    NotOptimized,
    /// No interface with a usable IPv4 address was found.
    NoActiveAdapter,
}

/// Registry interfaces that are really connected. If the OS adapter list
/// can't be read, falls back to registry-only detection.
pub fn active_adapters(reg: &impl InterfaceRegistry, os: &impl AdapterSource) -> Result<Vec<ActiveAdapter>> {
    let snapshots = reg.interfaces()?;
    let live = os.adapters().ok();
    Ok(detect_active(snapshots, live.as_deref()))
}

pub fn status_of(adapters: &[ActiveAdapter]) -> NagleStatus {
    if adapters.is_empty() {
        NagleStatus::NoActiveAdapter
    } else if adapters.iter().all(|a| a.optimized) {
        NagleStatus::Optimized
    } else {
        NagleStatus::NotOptimized
    }
}

/// Sets `TcpAckFrequency=1` and `TCPNoDelay=1` on every active adapter that
/// isn't already optimized. Returns the GUIDs that were patched.
pub fn apply(reg: &impl InterfaceRegistry, os: &impl AdapterSource) -> Result<Vec<String>> {
    let mut patched = Vec::new();
    for adapter in active_adapters(reg, os)?.into_iter().filter(|a| !a.optimized) {
        reg.set_dword(&adapter.guid, TCP_ACK_FREQUENCY, 1)?;
        reg.set_dword(&adapter.guid, TCP_NO_DELAY, 1)?;
        patched.push(adapter.guid);
    }
    Ok(patched)
}

/// Deletes both values from *every* interface that has them (including ones
/// that are no longer active), restoring Windows defaults.
pub fn revert(reg: &impl InterfaceRegistry) -> Result<Vec<String>> {
    let mut reverted = Vec::new();
    for iface in reg.interfaces()?.into_iter().filter(|i| i.has_tuning_values()) {
        reg.delete_value(&iface.guid, TCP_ACK_FREQUENCY)?;
        reg.delete_value(&iface.guid, TCP_NO_DELAY)?;
        reverted.push(iface.guid);
    }
    Ok(reverted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::adapters::OsAdapter;
    use std::cell::RefCell;

    /// In-memory registry fake.
    struct FakeRegistry(RefCell<Vec<InterfaceSnapshot>>);

    impl FakeRegistry {
        fn new(ifaces: Vec<InterfaceSnapshot>) -> Self {
            Self(RefCell::new(ifaces))
        }
        fn get(&self, guid: &str) -> InterfaceSnapshot {
            self.0.borrow().iter().find(|i| i.guid == guid).unwrap().clone()
        }
        fn with<F: FnOnce(&mut InterfaceSnapshot)>(&self, guid: &str, f: F) {
            f(self.0.borrow_mut().iter_mut().find(|i| i.guid == guid).unwrap())
        }
    }

    impl InterfaceRegistry for FakeRegistry {
        fn interfaces(&self) -> Result<Vec<InterfaceSnapshot>> {
            Ok(self.0.borrow().clone())
        }
        fn set_dword(&self, guid: &str, name: &str, value: u32) -> Result<()> {
            self.with(guid, |i| match name {
                TCP_ACK_FREQUENCY => i.tcp_ack_frequency = Some(value),
                TCP_NO_DELAY => i.tcp_no_delay = Some(value),
                other => panic!("unexpected value {other}"),
            });
            Ok(())
        }
        fn delete_value(&self, guid: &str, name: &str) -> Result<()> {
            self.with(guid, |i| match name {
                TCP_ACK_FREQUENCY => i.tcp_ack_frequency = None,
                TCP_NO_DELAY => i.tcp_no_delay = None,
                other => panic!("unexpected value {other}"),
            });
            Ok(())
        }
    }

    /// OS adapter info unavailable → registry-only fallback.
    struct NoOsInfo;
    impl AdapterSource for NoOsInfo {
        fn adapters(&self) -> Result<Vec<OsAdapter>> {
            Err(crate::Error::Parse("no OS info in this test".into()))
        }
    }

    struct LiveOs(Vec<OsAdapter>);
    impl AdapterSource for LiveOs {
        fn adapters(&self) -> Result<Vec<OsAdapter>> {
            Ok(self.0.clone())
        }
    }

    fn dhcp(guid: &str, ip: &str) -> InterfaceSnapshot {
        InterfaceSnapshot {
            guid: guid.into(),
            dhcp_ip_address: Some(ip.into()),
            ..Default::default()
        }
    }

    fn optimized(mut i: InterfaceSnapshot) -> InterfaceSnapshot {
        i.tcp_ack_frequency = Some(1);
        i.tcp_no_delay = Some(1);
        i
    }

    // --- InterfaceSnapshot -------------------------------------------------

    #[test]
    fn dhcp_address_makes_interface_active() {
        assert_eq!(
            dhcp("{A}", "192.168.1.5").active_ipv4(),
            Some(Ipv4Addr::new(192, 168, 1, 5))
        );
    }

    #[test]
    fn zero_dhcp_address_is_inactive() {
        assert_eq!(dhcp("{A}", "0.0.0.0").active_ipv4(), None);
    }

    #[test]
    fn falls_back_to_static_ip_list() {
        let i = InterfaceSnapshot {
            guid: "{S}".into(),
            dhcp_ip_address: Some("0.0.0.0".into()),
            ip_addresses: vec!["0.0.0.0".into(), "10.0.0.7".into()],
            ..Default::default()
        };
        assert_eq!(i.active_ipv4(), Some(Ipv4Addr::new(10, 0, 0, 7)));
    }

    #[test]
    fn optimized_requires_both_values_equal_to_one() {
        assert!(optimized(dhcp("{A}", "1.2.3.4")).is_optimized());
        let mut half = dhcp("{A}", "1.2.3.4");
        half.tcp_no_delay = Some(1);
        assert!(!half.is_optimized());
        half.tcp_ack_frequency = Some(2);
        assert!(!half.is_optimized());
    }

    // --- status ------------------------------------------------------------

    #[test]
    fn status_reports_no_active_adapter() {
        let reg = FakeRegistry::new(vec![InterfaceSnapshot {
            guid: "{X}".into(),
            ..Default::default()
        }]);
        let adapters = active_adapters(&reg, &NoOsInfo).unwrap();
        assert!(adapters.is_empty());
        assert_eq!(status_of(&adapters), NagleStatus::NoActiveAdapter);
    }

    #[test]
    fn status_optimized_only_when_all_active_adapters_are() {
        let reg = FakeRegistry::new(vec![
            optimized(dhcp("{A}", "192.168.1.5")),
            dhcp("{B}", "10.0.0.2"),
            dhcp("{Idle}", "0.0.0.0"), // inactive: ignored
        ]);
        assert_eq!(
            status_of(&active_adapters(&reg, &NoOsInfo).unwrap()),
            NagleStatus::NotOptimized
        );

        reg.with("{B}", |i| *i = optimized(i.clone()));
        assert_eq!(
            status_of(&active_adapters(&reg, &NoOsInfo).unwrap()),
            NagleStatus::Optimized
        );
    }

    // --- apply / revert ----------------------------------------------------

    #[test]
    fn apply_patches_only_active_unoptimized_adapters() {
        let reg = FakeRegistry::new(vec![
            optimized(dhcp("{Done}", "192.168.1.5")),
            dhcp("{Todo}", "10.0.0.2"),
            dhcp("{Idle}", "0.0.0.0"),
        ]);
        let patched = apply(&reg, &NoOsInfo).unwrap();
        assert_eq!(patched, vec!["{Todo}".to_string()]);
        assert!(reg.get("{Todo}").is_optimized());
        assert_eq!(reg.get("{Idle}").tcp_no_delay, None);
    }

    #[test]
    fn apply_skips_adapters_the_os_reports_as_down() {
        let reg = FakeRegistry::new(vec![dhcp("{Up}", "10.0.0.2"), dhcp("{Down}", "10.0.0.3")]);
        let os = LiveOs(vec![
            OsAdapter {
                guid: "{Up}".into(),
                friendly_name: "Ethernet".into(),
                is_up: true,
                if_type: 6,
                ipv4: vec![Ipv4Addr::new(10, 0, 0, 2)],
            },
            OsAdapter {
                guid: "{Down}".into(),
                friendly_name: "Wi-Fi".into(),
                is_up: false,
                if_type: 71,
                ipv4: vec![],
            },
        ]);
        assert_eq!(apply(&reg, &os).unwrap(), vec!["{Up}".to_string()]);
        assert_eq!(reg.get("{Down}").tcp_no_delay, None);
    }

    #[test]
    fn apply_is_idempotent() {
        let reg = FakeRegistry::new(vec![dhcp("{A}", "10.0.0.2")]);
        apply(&reg, &NoOsInfo).unwrap();
        assert!(apply(&reg, &NoOsInfo).unwrap().is_empty());
    }

    #[test]
    fn revert_removes_values_from_every_interface_that_has_them() {
        let reg = FakeRegistry::new(vec![
            optimized(dhcp("{Active}", "10.0.0.2")),
            optimized(dhcp("{NowIdle}", "0.0.0.0")),
            dhcp("{Clean}", "10.0.0.3"),
        ]);
        let mut reverted = revert(&reg).unwrap();
        reverted.sort();
        assert_eq!(reverted, vec!["{Active}".to_string(), "{NowIdle}".to_string()]);
        for guid in ["{Active}", "{NowIdle}"] {
            let i = reg.get(guid);
            assert_eq!((i.tcp_ack_frequency, i.tcp_no_delay), (None, None));
        }
    }
}
