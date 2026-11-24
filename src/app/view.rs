//! View model: everything the window and tray display, derived from state.

use super::state::{AppState, Notice, Route};
use crate::tweak::{TweakId, TweakState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Good,
    Bad,
    Warn,
    Neutral,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Badge {
    pub text: &'static str,
    pub tone: Tone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Banner {
    pub text: &'static str,
    pub tone: Tone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadOnlyBanner {
    pub text: &'static str,
    /// Show a "Restart as Administrator" button.
    pub restart_button: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardView {
    pub id: TweakId,
    pub title: String,
    pub value: String,
    pub state_text: &'static str,
    pub tone: Tone,
    pub toggle_on: bool,
    pub tooltip: &'static str,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub badge: Badge,
    pub read_only: Option<ReadOnlyBanner>,
    pub banner: Banner,
    pub cards: Vec<CardView>,
    /// Buttons, toggles and the startup checkbox can be used.
    pub controls_enabled: bool,
    /// `None` while unknown.
    pub startup_checked: Option<bool>,
    pub auto_heal_checked: bool,
    /// Shown under the auto-heal checkbox when it can't run (no admin/root).
    pub auto_heal_hint: Option<&'static str>,
    pub minimize_checked: bool,
    pub reboot_notice: Option<&'static str>,
    pub notice: Option<Notice>,
    pub busy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayView {
    /// Text of the disabled first menu item, e.g. `Status: Optimized`.
    pub status_text: &'static str,
    pub tone: Tone,
    pub tooltip: String,
}

pub const BANNER_CHECKING: &str = "Checking current settings...";
pub const BANNER_OPTIMIZED: &str = "Optimized for Tibia — Zero Packet Bundling";
pub const BANNER_NEEDS_FIX: &str = "Suboptimal Settings Detected — Click 'Optimize Now'";
pub const BANNER_UNKNOWN: &str = "Status unknown — some settings couldn't be read";
pub const READ_ONLY_WINDOWS: &str = "Running in Read-Only Mode — Restart as Administrator to apply changes";
pub const READ_ONLY_UNIX: &str = "Running without root — changes will ask for your password";
pub const REBOOT_NOTICE: &str = "Reboot recommended for the auto-tuning change to take full effect.";
pub const AUTO_HEAL_HINT_WINDOWS: &str =
    "Auto-heal needs admin rights. Enable 'Run at startup' so the app starts elevated at login.";
pub const AUTO_HEAL_HINT_UNIX: &str =
    "Auto-heal needs root. 'Run at startup' re-applies the settings at every boot instead.";

fn auto_heal_hint(s: &AppState) -> Option<&'static str> {
    (s.config.auto_heal && !s.elevated).then_some(match s.os {
        crate::platform::Os::Windows => AUTO_HEAL_HINT_WINDOWS,
        _ => AUTO_HEAL_HINT_UNIX,
    })
}

/// Why each setting matters (card tooltips).
pub fn tooltip(id: TweakId) -> &'static str {
    match id {
        TweakId::WinAutoTuning => {
            "Windows resizes the TCP receive window on the fly. Disabling auto-tuning removes \
             buffer jitter that delays small game packets. Takes full effect after a reboot."
        }
        TweakId::WinNagle => {
            "Nagle's algorithm and delayed ACKs hold small packets (steps, spells) for up to \
             200 ms to bundle them. Disabling both sends every keypress immediately."
        }
        TweakId::LinuxModerateRcvbuf | TweakId::MacAutoRcvbuf => {
            "The kernel resizes the TCP receive buffer on the fly. Turning it off avoids \
             buffer-induced jitter on small, frequent game packets."
        }
        TweakId::LinuxAutocorking => {
            "Autocorking holds small writes while earlier data is still in flight. Turning it \
             off sends each keypress immediately."
        }
        TweakId::LinuxQuickAck => {
            "Quick ACK acknowledges packets immediately instead of waiting up to 40 ms, so the \
             server sends the next update sooner."
        }
        TweakId::MacDelayedAck => {
            "Delayed ACK waits before acknowledging packets. Turning it off makes the server's \
             updates arrive sooner."
        }
    }
}

pub fn tone_of(state: TweakState) -> Tone {
    match state {
        TweakState::Optimized => Tone::Good,
        TweakState::NotOptimized => Tone::Bad,
        TweakState::Unknown => Tone::Warn,
    }
}

impl View {
    pub fn from_state(s: &AppState) -> View {
        let route = s.route();
        let (badge, read_only) = if s.elevated {
            (
                Badge {
                    text: "Admin Active",
                    tone: Tone::Good,
                },
                None,
            )
        } else {
            let banner = match route {
                Route::NeedsRestart => ReadOnlyBanner {
                    text: READ_ONLY_WINDOWS,
                    restart_button: true,
                },
                _ => ReadOnlyBanner {
                    text: READ_ONLY_UNIX,
                    restart_button: false,
                },
            };
            (
                Badge {
                    text: "Read-Only",
                    tone: Tone::Warn,
                },
                Some(banner),
            )
        };

        let banner = match s.report.as_ref().map(|r| r.overall.as_str()) {
            None => Banner {
                text: BANNER_CHECKING,
                tone: Tone::Neutral,
            },
            Some("optimized") => Banner {
                text: BANNER_OPTIMIZED,
                tone: Tone::Good,
            },
            Some("needs_fix") => Banner {
                text: BANNER_NEEDS_FIX,
                tone: Tone::Bad,
            },
            Some(_) => Banner {
                text: BANNER_UNKNOWN,
                tone: Tone::Warn,
            },
        };

        let cards = s
            .report
            .iter()
            .flat_map(|r| &r.tweaks)
            .map(|t| CardView {
                id: t.id,
                title: t.label.clone(),
                value: t.summary.clone(),
                state_text: t.state.describe(),
                tone: tone_of(t.state),
                toggle_on: t.state == TweakState::Optimized,
                tooltip: tooltip(t.id),
                details: t
                    .details
                    .iter()
                    .map(|d| {
                        let tag = if d.optimized { "tuned" } else { "default" };
                        format!("{} · {} · {tag}", d.label, d.value)
                    })
                    .collect(),
            })
            .collect();

        View {
            badge,
            read_only,
            banner,
            cards,
            controls_enabled: !s.busy && route != Route::NeedsRestart,
            startup_checked: s.report.as_ref().and_then(|r| r.startup),
            auto_heal_checked: s.config.auto_heal,
            auto_heal_hint: auto_heal_hint(s),
            minimize_checked: s.config.minimize_to_tray,
            reboot_notice: s
                .report
                .as_ref()
                .is_some_and(|r| r.reboot_recommended)
                .then_some(REBOOT_NOTICE),
            notice: s.notice.clone(),
            busy: s.busy,
        }
    }
}

impl TrayView {
    pub fn from_state(s: &AppState) -> TrayView {
        let (status_text, tone, short) = match s.report.as_ref().map(|r| r.overall.as_str()) {
            None => ("Status: Checking...", Tone::Neutral, "Checking..."),
            Some("optimized") => ("Status: Optimized", Tone::Good, "Optimized"),
            Some("needs_fix") => ("Status: Needs fix", Tone::Bad, "Needs fix"),
            Some(_) => ("Status: Unknown", Tone::Warn, "Unknown"),
        };
        TrayView {
            status_text,
            tone,
            tooltip: format!("Tibia Network Optimizer - {short}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::platform::Os;
    use crate::report::{StatusInputs, StatusReport};
    use crate::tweak::{TweakDetail, TweakStatus};

    fn nagle(state: TweakState) -> TweakStatus {
        TweakStatus::new(
            TweakId::WinNagle,
            "Nagle / delayed ACK",
            state,
            "active on some adapters",
        )
        .with_details(vec![
            TweakDetail {
                label: "Ethernet".into(),
                value: "192.168.1.185  {g}".into(),
                optimized: true,
            },
            TweakDetail {
                label: "Wi-Fi".into(),
                value: "10.0.0.2  {w}".into(),
                optimized: false,
            },
        ])
    }

    fn with_report(os: Os, elevated: bool, tweaks: Vec<TweakStatus>, reboot: bool) -> AppState {
        let mut s = AppState::new(os, elevated, Config::default(), false).0;
        s.report = Some(StatusReport::build(StatusInputs {
            os,
            tweaks,
            startup: Ok(true),
            elevated,
            reboot_recommended: reboot,
        }));
        s
    }

    #[test]
    fn badge_reflects_privileges() {
        let v = View::from_state(&with_report(Os::Windows, true, vec![], false));
        assert_eq!(
            v.badge,
            Badge {
                text: "Admin Active",
                tone: Tone::Good
            }
        );
        assert_eq!(v.read_only, None);
        let v = View::from_state(&with_report(Os::Windows, false, vec![], false));
        assert_eq!(
            v.badge,
            Badge {
                text: "Read-Only",
                tone: Tone::Warn
            }
        );
        assert_eq!(
            v.read_only,
            Some(ReadOnlyBanner {
                text: READ_ONLY_WINDOWS,
                restart_button: true
            })
        );
        let v = View::from_state(&with_report(Os::Linux, false, vec![], false));
        assert_eq!(
            v.read_only,
            Some(ReadOnlyBanner {
                text: READ_ONLY_UNIX,
                restart_button: false
            })
        );
    }

    #[test]
    fn banner_follows_overall_status() {
        let s = AppState::new(Os::Windows, true, Config::default(), false).0;
        assert_eq!(
            View::from_state(&s).banner,
            Banner {
                text: BANNER_CHECKING,
                tone: Tone::Neutral
            }
        );

        let ok = TweakStatus::new(
            TweakId::WinAutoTuning,
            "TCP auto-tuning",
            TweakState::Optimized,
            "disabled",
        );
        let v = View::from_state(&with_report(Os::Windows, true, vec![ok.clone()], false));
        assert_eq!(
            v.banner,
            Banner {
                text: BANNER_OPTIMIZED,
                tone: Tone::Good
            }
        );

        let v = View::from_state(&with_report(
            Os::Windows,
            true,
            vec![ok, nagle(TweakState::NotOptimized)],
            false,
        ));
        assert_eq!(
            v.banner,
            Banner {
                text: BANNER_NEEDS_FIX,
                tone: Tone::Bad
            }
        );

        let v = View::from_state(&with_report(
            Os::Windows,
            true,
            vec![nagle(TweakState::Unknown)],
            false,
        ));
        assert_eq!(
            v.banner,
            Banner {
                text: BANNER_UNKNOWN,
                tone: Tone::Warn
            }
        );
    }

    #[test]
    fn cards_show_value_state_tooltip_and_details() {
        let v = View::from_state(&with_report(
            Os::Windows,
            true,
            vec![nagle(TweakState::NotOptimized)],
            false,
        ));
        assert_eq!(
            v.cards,
            vec![CardView {
                id: TweakId::WinNagle,
                title: "Nagle / delayed ACK".into(),
                value: "active on some adapters".into(),
                state_text: "not optimized",
                tone: Tone::Bad,
                toggle_on: false,
                tooltip: tooltip(TweakId::WinNagle),
                details: vec![
                    "Ethernet · 192.168.1.185  {g} · tuned".into(),
                    "Wi-Fi · 10.0.0.2  {w} · default".into()
                ],
            }]
        );
    }

    #[test]
    fn controls_enabled_only_when_actions_can_run() {
        assert!(View::from_state(&with_report(Os::Windows, true, vec![], false)).controls_enabled);
        assert!(
            View::from_state(&with_report(Os::Linux, false, vec![], false)).controls_enabled,
            "via pkexec"
        );
        assert!(!View::from_state(&with_report(Os::Windows, false, vec![], false)).controls_enabled);
        let mut busy = with_report(Os::Windows, true, vec![], false);
        busy.busy = true;
        let v = View::from_state(&busy);
        assert!(!v.controls_enabled && v.busy);
    }

    #[test]
    fn automation_and_notices() {
        let mut s = with_report(Os::Windows, true, vec![], true);
        s.config.auto_heal = true;
        s.config.minimize_to_tray = false;
        let v = View::from_state(&s);
        assert_eq!(v.startup_checked, Some(true));
        assert!(v.auto_heal_checked && !v.minimize_checked);
        assert_eq!(v.reboot_notice, Some(REBOOT_NOTICE));
        let fresh = AppState::new(Os::Windows, true, Config::default(), false).0;
        assert_eq!(View::from_state(&fresh).startup_checked, None);
    }

    #[test]
    fn auto_heal_hint_only_when_enabled_without_rights() {
        let hint = |os, elevated, on| {
            let mut s = with_report(os, elevated, vec![], false);
            s.config.auto_heal = on;
            View::from_state(&s).auto_heal_hint
        };
        assert_eq!(hint(Os::Windows, false, true), Some(AUTO_HEAL_HINT_WINDOWS));
        assert_eq!(hint(Os::Linux, false, true), Some(AUTO_HEAL_HINT_UNIX));
        assert_eq!(hint(Os::Macos, false, true), Some(AUTO_HEAL_HINT_UNIX));
        assert_eq!(hint(Os::Windows, true, true), None, "elevated: it works");
        assert_eq!(hint(Os::Windows, false, false), None, "off: nothing to explain");
    }

    #[test]
    fn every_tweak_has_a_tooltip() {
        for id in TweakId::ALL {
            assert!(tooltip(id).len() > 20, "{id:?}");
        }
    }

    #[test]
    fn tray_reflects_status() {
        let s = AppState::new(Os::Windows, true, Config::default(), false).0;
        assert_eq!(TrayView::from_state(&s).status_text, "Status: Checking...");
        let ok = TweakStatus::new(TweakId::WinAutoTuning, "x", TweakState::Optimized, "");
        let t = TrayView::from_state(&with_report(Os::Windows, true, vec![ok], false));
        assert_eq!((t.status_text, t.tone), ("Status: Optimized", Tone::Good));
        assert_eq!(t.tooltip, "Tibia Network Optimizer - Optimized");
        let t = TrayView::from_state(&with_report(
            Os::Windows,
            true,
            vec![nagle(TweakState::NotOptimized)],
            false,
        ));
        assert_eq!((t.status_text, t.tone), ("Status: Needs fix", Tone::Bad));
        let t = TrayView::from_state(&with_report(
            Os::Windows,
            true,
            vec![nagle(TweakState::Unknown)],
            false,
        ));
        assert_eq!((t.status_text, t.tone), ("Status: Unknown", Tone::Warn));
    }
}
