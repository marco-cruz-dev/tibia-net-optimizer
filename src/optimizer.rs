//! Combines individual checks into an overall status and decides what the
//! auto-heal loop should fix.

use crate::config::Config;
use crate::network::autotuning::AutoTuningLevel;
use crate::network::nagle::NagleStatus;

/// Snapshot of everything we check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemStatus {
    /// `None` when the level could not be read.
    pub autotuning: Option<AutoTuningLevel>,
    pub nagle: NagleStatus,
}

/// Drives the tray icon colour and the status banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overall {
    /// Green: both optimizations active.
    Optimized,
    /// Red: at least one setting is known to be suboptimal.
    NeedsFix,
    /// Yellow: couldn't determine (read error / no active adapter).
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fix {
    DisableAutoTuning,
    DisableNagle,
}

impl SystemStatus {
    pub fn needs_autotuning_fix(&self) -> bool {
        matches!(&self.autotuning, Some(level) if !level.is_optimized())
    }

    pub fn needs_nagle_fix(&self) -> bool {
        self.nagle == NagleStatus::NotOptimized
    }

    pub fn overall(&self) -> Overall {
        if self.needs_autotuning_fix() || self.needs_nagle_fix() {
            Overall::NeedsFix
        } else if self
            .autotuning
            .as_ref()
            .is_some_and(AutoTuningLevel::is_optimized)
            && self.nagle == NagleStatus::Optimized
        {
            Overall::Optimized
        } else {
            Overall::Unknown
        }
    }
}

/// Fixes the background auto-heal loop should apply right now.
/// Nothing happens unless auto-heal is enabled *and* we're elevated.
pub fn heal_plan(status: &SystemStatus, config: &Config, elevated: bool) -> Vec<Fix> {
    if !config.auto_heal || !elevated {
        return Vec::new();
    }
    let mut fixes = Vec::new();
    if status.needs_autotuning_fix() {
        fixes.push(Fix::DisableAutoTuning);
    }
    if status.needs_nagle_fix() {
        fixes.push(Fix::DisableNagle);
    }
    fixes
}

#[cfg(test)]
mod tests {
    use super::*;
    use AutoTuningLevel::*;

    fn status(at: Option<AutoTuningLevel>, nagle: NagleStatus) -> SystemStatus {
        SystemStatus {
            autotuning: at,
            nagle,
        }
    }

    fn heal_on() -> Config {
        Config {
            auto_heal: true,
            ..Config::default()
        }
    }

    #[test]
    fn overall_optimized_when_both_are() {
        assert_eq!(
            status(Some(Disabled), NagleStatus::Optimized).overall(),
            Overall::Optimized
        );
    }

    #[test]
    fn overall_needs_fix_when_any_is_suboptimal() {
        assert_eq!(
            status(Some(Normal), NagleStatus::Optimized).overall(),
            Overall::NeedsFix
        );
        assert_eq!(
            status(Some(Disabled), NagleStatus::NotOptimized).overall(),
            Overall::NeedsFix
        );
        // A known problem beats an unknown.
        assert_eq!(
            status(None, NagleStatus::NotOptimized).overall(),
            Overall::NeedsFix
        );
    }

    #[test]
    fn overall_unknown_when_information_missing() {
        assert_eq!(status(None, NagleStatus::Optimized).overall(), Overall::Unknown);
        assert_eq!(
            status(Some(Disabled), NagleStatus::NoActiveAdapter).overall(),
            Overall::Unknown
        );
    }

    #[test]
    fn heal_plan_fixes_each_drifted_setting() {
        let s = status(Some(Normal), NagleStatus::NotOptimized);
        assert_eq!(
            heal_plan(&s, &heal_on(), true),
            vec![Fix::DisableAutoTuning, Fix::DisableNagle]
        );
    }

    #[test]
    fn heal_plan_empty_when_already_optimized_or_unknown() {
        assert!(heal_plan(&status(Some(Disabled), NagleStatus::Optimized), &heal_on(), true).is_empty());
        assert!(heal_plan(&status(None, NagleStatus::NoActiveAdapter), &heal_on(), true).is_empty());
    }

    #[test]
    fn heal_plan_respects_config_and_elevation() {
        let s = status(Some(Normal), NagleStatus::NotOptimized);
        assert!(
            heal_plan(&s, &Config::default(), true).is_empty(),
            "auto-heal off"
        );
        assert!(heal_plan(&s, &heal_on(), false).is_empty(), "not elevated");
    }
}
