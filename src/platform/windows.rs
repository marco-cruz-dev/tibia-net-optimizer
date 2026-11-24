//! Windows: netsh auto-tuning + registry Nagle tweaks, Task Scheduler startup.

use super::{Os, Platform};
use crate::network::iphelper::WindowsAdapterSource;
use crate::network::registry::WindowsInterfaceRegistry;
use crate::system::command::SystemCommandRunner;
use crate::system::startup::TaskScheduler;
use crate::tweak::windows::{AutoTuningTweak, NagleTweak};

pub fn platform() -> Platform {
    Platform {
        os: Os::Windows,
        tweaks: vec![
            Box::new(AutoTuningTweak {
                runner: SystemCommandRunner,
            }),
            Box::new(NagleTweak {
                registry: WindowsInterfaceRegistry,
                adapters: WindowsAdapterSource,
            }),
        ],
        startup: Box::new(TaskScheduler {
            runner: SystemCommandRunner,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tweak::TweakId;

    #[test]
    fn windows_platform_has_expected_tweaks_in_order() {
        let p = platform();
        assert_eq!(p.os, Os::Windows);
        let ids: Vec<TweakId> = p.tweaks.iter().map(|t| t.id()).collect();
        assert_eq!(ids, vec![TweakId::WinAutoTuning, TweakId::WinNagle]);
    }
}
