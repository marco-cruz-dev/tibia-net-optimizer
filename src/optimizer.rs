//! Combines tweak statuses into an overall status and decides what the
//! auto-heal loop should fix.

use crate::config::Config;
use crate::tweak::{TweakId, TweakState, TweakStatus};

/// Drives the tray icon colour and the status banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overall {
    /// Green: every tweak is optimized.
    Optimized,
    /// Red: at least one tweak is known to be suboptimal.
    NeedsFix,
    /// Yellow: nothing is known-bad, but something couldn't be determined.
    Unknown,
}

impl Overall {
    pub fn as_str(&self) -> &'static str {
        match self {
            Overall::Optimized => "optimized",
            Overall::NeedsFix => "needs_fix",
            Overall::Unknown => "unknown",
        }
    }
}

pub fn overall(statuses: &[TweakStatus]) -> Overall {
    if statuses.iter().any(|s| s.state == TweakState::NotOptimized) {
        Overall::NeedsFix
    } else if !statuses.is_empty() && statuses.iter().all(|s| s.state == TweakState::Optimized) {
        Overall::Optimized
    } else {
        Overall::Unknown
    }
}

/// Tweaks the background auto-heal loop should apply right now.
/// Nothing happens unless auto-heal is enabled *and* we're elevated.
pub fn heal_plan(statuses: &[TweakStatus], config: &Config, elevated: bool) -> Vec<TweakId> {
    if !config.auto_heal || !elevated {
        return Vec::new();
    }
    statuses
        .iter()
        .filter(|s| s.state == TweakState::NotOptimized)
        .map(|s| s.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use TweakState::*;

    fn st(id: TweakId, state: TweakState) -> TweakStatus {
        TweakStatus::new(id, "x", state, "")
    }

    fn heal_on() -> Config {
        Config {
            auto_heal: true,
            ..Config::default()
        }
    }

    #[test]
    fn overall_optimized_when_all_are() {
        let s = [
            st(TweakId::WinAutoTuning, Optimized),
            st(TweakId::WinNagle, Optimized),
        ];
        assert_eq!(overall(&s), Overall::Optimized);
    }

    #[test]
    fn overall_needs_fix_when_any_is_suboptimal_even_with_unknowns() {
        let s = [
            st(TweakId::WinAutoTuning, Unknown),
            st(TweakId::WinNagle, NotOptimized),
        ];
        assert_eq!(overall(&s), Overall::NeedsFix);
    }

    #[test]
    fn overall_unknown_when_information_missing_or_empty() {
        let s = [
            st(TweakId::WinAutoTuning, Optimized),
            st(TweakId::WinNagle, Unknown),
        ];
        assert_eq!(overall(&s), Overall::Unknown);
        assert_eq!(overall(&[]), Overall::Unknown);
    }

    #[test]
    fn heal_plan_targets_each_suboptimal_tweak() {
        let s = [
            st(TweakId::LinuxModerateRcvbuf, NotOptimized),
            st(TweakId::LinuxAutocorking, Optimized),
            st(TweakId::LinuxQuickAck, NotOptimized),
            st(TweakId::WinNagle, Unknown),
        ];
        assert_eq!(
            heal_plan(&s, &heal_on(), true),
            vec![TweakId::LinuxModerateRcvbuf, TweakId::LinuxQuickAck]
        );
    }

    #[test]
    fn heal_plan_respects_config_and_elevation() {
        let s = [st(TweakId::WinNagle, NotOptimized)];
        assert!(
            heal_plan(&s, &Config::default(), true).is_empty(),
            "auto-heal off"
        );
        assert!(heal_plan(&s, &heal_on(), false).is_empty(), "not elevated");
    }
}
