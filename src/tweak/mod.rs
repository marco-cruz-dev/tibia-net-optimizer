//! Generic optimization model shared by every platform.
//!
//! A [`Tweak`] is one OS setting we can check, apply and revert. Platforms
//! (see `crate::platform`) assemble a list of tweaks; status, auto-heal and
//! reporting work on that list without knowing which OS they're on.

use std::fmt::Display;

use serde::Serialize;

use crate::Result;

pub mod quickack;
pub mod sysctl;
pub mod windows;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TweakId {
    WinAutoTuning,
    WinNagle,
    LinuxModerateRcvbuf,
    LinuxAutocorking,
    LinuxQuickAck,
    MacAutoRcvbuf,
    MacDelayedAck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TweakState {
    Optimized,
    NotOptimized,
    /// Couldn't be determined (read error, nothing to tune, ...).
    Unknown,
}

impl TweakState {
    pub fn describe(&self) -> &'static str {
        match self {
            TweakState::Optimized => "optimized",
            TweakState::NotOptimized => "not optimized",
            TweakState::Unknown => "unknown",
        }
    }
}

/// A sub-item of a tweak (an adapter, a route, ...).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TweakDetail {
    pub label: String,
    pub value: String,
    pub optimized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TweakStatus {
    pub id: TweakId,
    pub label: String,
    pub state: TweakState,
    /// Short human description of the current value.
    pub summary: String,
    pub details: Vec<TweakDetail>,
}

impl TweakStatus {
    pub fn new(id: TweakId, label: &str, state: TweakState, summary: impl Into<String>) -> Self {
        Self {
            id,
            label: label.to_string(),
            state,
            summary: summary.into(),
            details: Vec::new(),
        }
    }

    /// Status for a tweak whose check failed.
    pub fn unknown(id: TweakId, label: &str, reason: impl Display) -> Self {
        Self::new(id, label, TweakState::Unknown, format!("error: {reason}"))
    }

    pub fn with_details(mut self, details: Vec<TweakDetail>) -> Self {
        self.details = details;
        self
    }
}

pub trait Tweak {
    fn id(&self) -> TweakId;
    fn label(&self) -> &'static str;
    /// Never fails: problems are reported as [`TweakState::Unknown`].
    fn check(&self) -> TweakStatus;
    fn apply(&self) -> Result<()>;
    fn revert(&self) -> Result<()>;
    /// Whether changing this tweak needs a reboot to take full effect.
    fn reboot_recommended(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_describe_themselves() {
        assert_eq!(TweakState::Optimized.describe(), "optimized");
        assert_eq!(TweakState::NotOptimized.describe(), "not optimized");
        assert_eq!(TweakState::Unknown.describe(), "unknown");
    }

    #[test]
    fn unknown_status_carries_reason() {
        let s = TweakStatus::unknown(TweakId::WinNagle, "Nagle", "access denied");
        assert_eq!(s.state, TweakState::Unknown);
        assert_eq!(s.summary, "error: access denied");
        assert_eq!(s.label, "Nagle");
        assert!(s.details.is_empty());
    }

    #[test]
    fn ids_and_states_serialize_as_snake_case() {
        assert_eq!(
            serde_json::to_value(TweakId::LinuxQuickAck).unwrap(),
            "linux_quick_ack"
        );
        assert_eq!(
            serde_json::to_value(TweakState::NotOptimized).unwrap(),
            "not_optimized"
        );
    }
}
