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

impl TweakId {
    pub const ALL: [TweakId; 7] = [
        TweakId::WinAutoTuning,
        TweakId::WinNagle,
        TweakId::LinuxModerateRcvbuf,
        TweakId::LinuxAutocorking,
        TweakId::LinuxQuickAck,
        TweakId::MacAutoRcvbuf,
        TweakId::MacDelayedAck,
    ];

    /// Stable identifier (same as the JSON form), e.g. `win_nagle`.
    pub fn key(&self) -> &'static str {
        match self {
            TweakId::WinAutoTuning => "win_auto_tuning",
            TweakId::WinNagle => "win_nagle",
            TweakId::LinuxModerateRcvbuf => "linux_moderate_rcvbuf",
            TweakId::LinuxAutocorking => "linux_autocorking",
            TweakId::LinuxQuickAck => "linux_quick_ack",
            TweakId::MacAutoRcvbuf => "mac_auto_rcvbuf",
            TweakId::MacDelayedAck => "mac_delayed_ack",
        }
    }

    pub fn parse(key: &str) -> Option<TweakId> {
        TweakId::ALL.into_iter().find(|id| id.key() == key)
    }
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

/// A controllable in-memory tweak for tests (thread-safe so it can live in
/// the GUI worker thread).
#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    pub struct FakeTweak {
        pub id: TweakId,
        pub label: &'static str,
        pub state: Arc<Mutex<TweakState>>,
        pub fail: bool,
        pub reboot: bool,
    }

    impl FakeTweak {
        pub fn new(id: TweakId, label: &'static str, state: TweakState) -> Self {
            Self {
                id,
                label,
                state: Arc::new(Mutex::new(state)),
                fail: false,
                reboot: false,
            }
        }
        pub fn failing(mut self) -> Self {
            self.fail = true;
            self
        }
        pub fn needing_reboot(mut self) -> Self {
            self.reboot = true;
            self
        }
        pub fn current(&self) -> TweakState {
            *self.state.lock().unwrap()
        }
        fn set(&self, s: TweakState) -> Result<()> {
            if self.fail {
                return Err(crate::Error::AccessDenied);
            }
            *self.state.lock().unwrap() = s;
            Ok(())
        }
    }

    impl Tweak for FakeTweak {
        fn id(&self) -> TweakId {
            self.id
        }
        fn label(&self) -> &'static str {
            self.label
        }
        fn check(&self) -> TweakStatus {
            TweakStatus::new(self.id, self.label, self.current(), "fake")
        }
        fn apply(&self) -> Result<()> {
            self.set(TweakState::Optimized)
        }
        fn revert(&self) -> Result<()> {
            self.set(TweakState::NotOptimized)
        }
        fn reboot_recommended(&self) -> bool {
            self.reboot
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_roundtrip_and_match_json() {
        for id in TweakId::ALL {
            assert_eq!(TweakId::parse(id.key()), Some(id));
            assert_eq!(serde_json::to_value(id).unwrap(), id.key());
        }
        assert_eq!(TweakId::parse("nope"), None);
    }

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
