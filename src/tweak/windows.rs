//! Windows tweaks, generic over their ports so they're testable everywhere.

use super::{Tweak, TweakDetail, TweakId, TweakState, TweakStatus};
use crate::network::adapters::AdapterSource;
use crate::network::autotuning;
use crate::network::nagle::{self, InterfaceRegistry, NagleStatus};
use crate::system::command::CommandRunner;
use crate::Result;

/// `netsh int tcp set global autotuninglevel=disabled|normal`.
pub struct AutoTuningTweak<R> {
    pub runner: R,
}

impl<R: CommandRunner> Tweak for AutoTuningTweak<R> {
    fn id(&self) -> TweakId {
        TweakId::WinAutoTuning
    }
    fn label(&self) -> &'static str {
        "TCP auto-tuning"
    }
    fn check(&self) -> TweakStatus {
        match autotuning::current_level(&self.runner) {
            Ok(level) => {
                let state = if level.is_optimized() {
                    TweakState::Optimized
                } else {
                    TweakState::NotOptimized
                };
                TweakStatus::new(self.id(), self.label(), state, level.keyword())
            }
            Err(e) => TweakStatus::unknown(self.id(), self.label(), e),
        }
    }
    fn apply(&self) -> Result<()> {
        autotuning::apply(&self.runner)
    }
    fn revert(&self) -> Result<()> {
        autotuning::revert(&self.runner)
    }
    fn reboot_recommended(&self) -> bool {
        true
    }
}

/// `TcpAckFrequency` / `TCPNoDelay` on every connected adapter.
pub struct NagleTweak<Reg, Os> {
    pub registry: Reg,
    pub adapters: Os,
}

impl<Reg: InterfaceRegistry, Os: AdapterSource> Tweak for NagleTweak<Reg, Os> {
    fn id(&self) -> TweakId {
        TweakId::WinNagle
    }
    fn label(&self) -> &'static str {
        "Nagle / delayed ACK"
    }
    fn check(&self) -> TweakStatus {
        let adapters = match nagle::active_adapters(&self.registry, &self.adapters) {
            Ok(a) => a,
            Err(e) => return TweakStatus::unknown(self.id(), self.label(), e),
        };
        let (state, summary) = match nagle::status_of(&adapters) {
            NagleStatus::Optimized => (TweakState::Optimized, "disabled on all active adapters"),
            NagleStatus::NotOptimized => (TweakState::NotOptimized, "active on some adapters"),
            NagleStatus::NoActiveAdapter => (TweakState::Unknown, "no active IPv4 adapter found"),
        };
        let details = adapters
            .into_iter()
            .map(|a| match a.name {
                Some(name) => TweakDetail {
                    label: name,
                    value: format!("{}  {}", a.ipv4, a.guid),
                    optimized: a.optimized,
                },
                None => TweakDetail {
                    label: a.guid,
                    value: a.ipv4.to_string(),
                    optimized: a.optimized,
                },
            })
            .collect();
        TweakStatus::new(self.id(), self.label(), state, summary).with_details(details)
    }
    fn apply(&self) -> Result<()> {
        nagle::apply(&self.registry, &self.adapters).map(drop)
    }
    fn revert(&self) -> Result<()> {
        nagle::revert(&self.registry).map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::adapters::OsAdapter;
    use crate::network::nagle::fake::{dhcp, optimized, FakeRegistry, LiveOs, NoOsInfo};
    use crate::system::command::fake::FakeRunner;
    use std::net::Ipv4Addr;

    const SHOW_NORMAL: &str = "Receive Window Auto-Tuning Level    : normal\r\n";
    const SHOW_DISABLED: &str = "Receive Window Auto-Tuning Level    : disabled\r\n";

    #[test]
    fn autotuning_check_maps_levels_to_states() {
        let t = AutoTuningTweak {
            runner: FakeRunner::new().respond_ok(SHOW_NORMAL),
        };
        let s = t.check();
        assert_eq!(
            (s.id, s.state, s.summary.as_str()),
            (TweakId::WinAutoTuning, TweakState::NotOptimized, "normal")
        );

        let t = AutoTuningTweak {
            runner: FakeRunner::new().respond_ok(SHOW_DISABLED),
        };
        assert_eq!(t.check().state, TweakState::Optimized);
    }

    #[test]
    fn autotuning_check_failure_is_unknown() {
        let t = AutoTuningTweak {
            runner: FakeRunner::new().respond_ok("garbage"),
        };
        let s = t.check();
        assert_eq!(s.state, TweakState::Unknown);
        assert!(s.summary.starts_with("error:"), "{}", s.summary);
    }

    #[test]
    fn autotuning_apply_and_revert_call_netsh() {
        let runner = FakeRunner::new();
        let t = AutoTuningTweak { runner: &runner };
        t.apply().unwrap();
        t.revert().unwrap();
        let calls = runner.calls();
        assert_eq!(calls[0].last().unwrap(), "autotuninglevel=disabled");
        assert_eq!(calls[1].last().unwrap(), "autotuninglevel=normal");
        assert!(t.reboot_recommended());
    }

    fn ethernet(guid: &str, ip: [u8; 4]) -> OsAdapter {
        OsAdapter {
            guid: guid.into(),
            friendly_name: "Ethernet".into(),
            is_up: true,
            if_type: 6,
            ipv4: vec![Ipv4Addr::from(ip)],
        }
    }

    #[test]
    fn nagle_check_lists_adapters_as_details() {
        let reg = FakeRegistry::new(vec![optimized(dhcp("{e}", "10.0.0.2"))]);
        let t = NagleTweak {
            registry: &reg,
            adapters: LiveOs(vec![ethernet("{e}", [10, 0, 0, 2])]),
        };
        let s = t.check();
        assert_eq!(s.state, TweakState::Optimized);
        assert_eq!(s.summary, "disabled on all active adapters");
        assert_eq!(
            s.details,
            vec![TweakDetail {
                label: "Ethernet".into(),
                value: "10.0.0.2  {e}".into(),
                optimized: true
            }]
        );
    }

    #[test]
    fn nagle_check_without_names_uses_guid_label() {
        let reg = FakeRegistry::new(vec![dhcp("{e}", "10.0.0.2")]);
        let s = NagleTweak {
            registry: &reg,
            adapters: NoOsInfo,
        }
        .check();
        assert_eq!(s.state, TweakState::NotOptimized);
        assert_eq!(s.summary, "active on some adapters");
        assert_eq!(s.details[0].label, "{e}");
        assert_eq!(s.details[0].value, "10.0.0.2");
    }

    #[test]
    fn nagle_check_without_active_adapter_is_unknown() {
        let reg = FakeRegistry::new(vec![dhcp("{idle}", "0.0.0.0")]);
        let s = NagleTweak {
            registry: &reg,
            adapters: NoOsInfo,
        }
        .check();
        assert_eq!(s.state, TweakState::Unknown);
        assert_eq!(s.summary, "no active IPv4 adapter found");
    }

    #[test]
    fn nagle_apply_and_revert_modify_registry() {
        let reg = FakeRegistry::new(vec![dhcp("{e}", "10.0.0.2")]);
        let t = NagleTweak {
            registry: &reg,
            adapters: NoOsInfo,
        };
        t.apply().unwrap();
        assert!(reg.get("{e}").is_optimized());
        t.revert().unwrap();
        assert_eq!(reg.get("{e}").tcp_no_delay, None);
    }
}
