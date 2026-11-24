//! Linux: receive-buffer auto-tuning, autocorking, per-route quick ACK.

use std::path::PathBuf;

use super::{Os, Platform};
use crate::system::command::SystemCommandRunner;
use crate::tweak::quickack::{QuickAckTweak, DISPATCHER_PATH};
use crate::tweak::sysctl::{SysctlSpec, SysctlTweak};
use crate::tweak::TweakId;
use crate::unix::persist::{FileSettingsStore, PersistFormat};
use crate::unix::service::{SystemdStartup, SYSTEMD_UNIT_PATH};
use crate::unix::sysctl::CommandSysctl;

pub const SYSCTL_DROPIN: &str = "/etc/sysctl.d/99-tibia-net-optimizer.conf";

pub const MODERATE_RCVBUF: SysctlSpec = SysctlSpec {
    id: TweakId::LinuxModerateRcvbuf,
    label: "Receive buffer auto-tuning",
    key: "net.ipv4.tcp_moderate_rcvbuf",
    optimized: "0",
    default: "1",
};

pub const AUTOCORKING: SysctlSpec = SysctlSpec {
    id: TweakId::LinuxAutocorking,
    label: "TCP autocorking",
    key: "net.ipv4.tcp_autocorking",
    optimized: "0",
    default: "1",
};

pub fn platform() -> Platform {
    let store = FileSettingsStore {
        path: PathBuf::from(SYSCTL_DROPIN),
        format: PersistFormat::SysctlConf,
    };
    let sysctl = CommandSysctl {
        runner: SystemCommandRunner,
    };
    let sysctl_tweak = |spec| SysctlTweak {
        spec,
        sysctl,
        store: store.clone(),
    };
    Platform {
        os: Os::Linux,
        tweaks: vec![
            Box::new(sysctl_tweak(MODERATE_RCVBUF)),
            Box::new(sysctl_tweak(AUTOCORKING)),
            Box::new(QuickAckTweak {
                runner: SystemCommandRunner,
                dispatcher_path: PathBuf::from(DISPATCHER_PATH),
            }),
        ],
        startup: Box::new(SystemdStartup {
            runner: SystemCommandRunner,
            unit_path: PathBuf::from(SYSTEMD_UNIT_PATH),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_platform_has_expected_tweaks_in_order() {
        let p = platform();
        assert_eq!(p.os, Os::Linux);
        let ids: Vec<TweakId> = p.tweaks.iter().map(|t| t.id()).collect();
        assert_eq!(
            ids,
            vec![
                TweakId::LinuxModerateRcvbuf,
                TweakId::LinuxAutocorking,
                TweakId::LinuxQuickAck
            ]
        );
        assert!(p.tweaks.iter().all(|t| !t.reboot_recommended()));
    }

    #[test]
    fn specs_disable_the_kernel_features() {
        for s in [MODERATE_RCVBUF, AUTOCORKING] {
            assert_eq!((s.optimized, s.default), ("0", "1"), "{}", s.key);
        }
    }
}
