//! macOS: automatic receive buffer and delayed ACK.

use std::path::PathBuf;

use super::{Os, Platform};
use crate::system::command::SystemCommandRunner;
use crate::tweak::sysctl::{SysctlSpec, SysctlTweak};
use crate::tweak::TweakId;
use crate::unix::persist::{FileSettingsStore, PersistFormat};
use crate::unix::service::{LaunchdStartup, LAUNCHD_STARTUP_PATH};
use crate::unix::sysctl::CommandSysctl;

pub const SYSCTL_LABEL: &str = "com.tibianetoptimizer.sysctl";
pub const SYSCTL_PLIST: &str = "/Library/LaunchDaemons/com.tibianetoptimizer.sysctl.plist";

pub const AUTO_RCVBUF: SysctlSpec = SysctlSpec {
    id: TweakId::MacAutoRcvbuf,
    label: "Receive buffer auto-tuning",
    key: "net.inet.tcp.doautorcvbuf",
    optimized: "0",
    default: "1",
};

pub const DELAYED_ACK: SysctlSpec = SysctlSpec {
    id: TweakId::MacDelayedAck,
    label: "Delayed ACK",
    key: "net.inet.tcp.delayed_ack",
    optimized: "0",
    default: "3",
};

pub fn platform() -> Platform {
    let store = FileSettingsStore {
        path: PathBuf::from(SYSCTL_PLIST),
        format: PersistFormat::LaunchdPlist {
            label: SYSCTL_LABEL.into(),
        },
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
        os: Os::Macos,
        tweaks: vec![
            Box::new(sysctl_tweak(AUTO_RCVBUF)),
            Box::new(sysctl_tweak(DELAYED_ACK)),
        ],
        startup: Box::new(LaunchdStartup {
            plist_path: PathBuf::from(LAUNCHD_STARTUP_PATH),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_platform_has_expected_tweaks_in_order() {
        let p = platform();
        assert_eq!(p.os, Os::Macos);
        let ids: Vec<TweakId> = p.tweaks.iter().map(|t| t.id()).collect();
        assert_eq!(ids, vec![TweakId::MacAutoRcvbuf, TweakId::MacDelayedAck]);
    }

    #[test]
    fn delayed_ack_default_is_three() {
        assert_eq!((DELAYED_ACK.optimized, DELAYED_ACK.default), ("0", "3"));
    }
}
