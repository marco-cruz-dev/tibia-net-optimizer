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

/// The "Live Connection Monitor" section.
#[derive(Debug, Clone, PartialEq)]
pub struct PingView {
    /// What the monitor is doing right now.
    pub status: &'static str,
    /// The measured address (Tibia's Cloudflare entry point), when known.
    pub target: Option<String>,
    pub current: String,
    pub average: String,
    pub jitter: String,
    pub jitter_tone: Tone,
    pub loss: String,
    pub loss_tone: Tone,
    /// Oldest-first RTTs in ms for the sparkline (`None` = lost).
    pub points: Vec<Option<f64>>,
    pub error: Option<String>,
}

pub const PING_WAITING: &str = "Waiting for Tibia - log in to a game world to measure your connection.";
pub const PING_MEASURING: &str = "Measuring your connection to Tibia's entry point (Cloudflare).";
pub const PING_STARTING: &str = "Looking for the Tibia client...";
pub const PING_NOT_PINGABLE: &str =
    "Connected, but Tibia's server doesn't answer ping (ICMP is blocked), so latency can't be measured.";

/// A plain-text summary of everything on screen, for "Copy diagnostics".
pub fn diagnostics(s: &AppState) -> String {
    let on = |b: bool| if b { "on" } else { "off" };
    let mut out = vec![
        format!("Tibia Network Optimizer {}", env!("CARGO_PKG_VERSION")),
        format!(
            "Platform: {}   Privileges: {}",
            s.os.key(),
            if s.elevated {
                s.os.admin_name()
            } else {
                "standard user"
            }
        ),
        String::new(),
    ];
    match &s.report {
        Some(r) => out.push(r.to_text()),
        None => out.push("Status: still checking".into()),
    }
    out.push(String::new());
    out.push(format!(
        "Auto-heal: {}   Minimize to tray: {}   Check interval: {} s",
        on(s.config.auto_heal),
        on(s.config.minimize_to_tray),
        s.config.check_interval_secs
    ));

    let p = PingView::from_state(s);
    out.push(String::new());
    out.push(format!("Connection monitor: {}", p.status));
    if let Some(t) = &p.target {
        out.push(format!("  Address: {t}"));
    }
    out.push(format!(
        "  Ping: {}   Average: {}   Jitter: {}   Loss: {}   Samples: {}",
        p.current,
        p.average,
        p.jitter,
        p.loss,
        s.ping.stats.len()
    ));
    if let Some(e) = &p.error {
        out.push(format!("  Last probe error: {e}"));
    }
    if let Some(n) = &s.notice {
        out.push(String::new());
        out.push(format!("Notice: {}", n.text));
    }
    out.join("\n")
}
/// Shown as a tooltip: what is (and isn't) measured.
pub const PING_EXPLAINER: &str = "Tibia routes game traffic through Cloudflare. This pings the Cloudflare \
     server your client is connected to (ICMP only, never the game port). That is the part of the path your \
     ISP and Wi-Fi affect, where lag spikes and jitter come from. Cloudflare's link on to the game world \
     can't be measured from outside.";

/// Jitter is what makes steps stutter: under 5 ms is smooth, 5–15 ms is
/// noticeable, and above that it hurts.
pub fn jitter_tone(jitter_ms: Option<f64>) -> Tone {
    match jitter_ms {
        None => Tone::Neutral,
        Some(j) if j < 5.0 => Tone::Good,
        Some(j) if j < 15.0 => Tone::Warn,
        Some(_) => Tone::Bad,
    }
}

/// Any sustained loss drops inputs: 0% good, up to 2% warning, above bad.
pub fn loss_tone(loss_pct: f64, has_samples: bool) -> Tone {
    if !has_samples {
        Tone::Neutral
    } else if loss_pct <= 0.0 {
        Tone::Good
    } else if loss_pct <= 2.0 {
        Tone::Warn
    } else {
        Tone::Bad
    }
}

const DASH: &str = "—";

impl PingView {
    pub fn from_state(s: &AppState) -> PingView {
        let p = &s.ping;
        let stats = &p.stats;
        let ms = |v: Option<f64>| v.map_or_else(|| DASH.to_string(), |x| format!("{x:.0} ms"));
        let jitter = stats.jitter_ms();
        let status = if p.waiting_for_game {
            PING_WAITING
        } else if !p.not_pingable.is_empty() {
            PING_NOT_PINGABLE
        } else if p.target.is_some() {
            PING_MEASURING
        } else {
            PING_STARTING
        };
        let target = match p.target {
            Some(t) => Some(t.to_string()),
            None if !p.not_pingable.is_empty() => Some(
                p.not_pingable
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            None => None,
        };
        PingView {
            status,
            target,
            current: ms(stats.current_ms()),
            average: ms(stats.average_ms()),
            jitter: jitter.map_or_else(|| DASH.to_string(), |j| format!("{j:.1} ms")),
            jitter_tone: jitter_tone(jitter),
            loss: if stats.is_empty() {
                DASH.to_string()
            } else {
                format!("{:.0}%", stats.packet_loss_pct())
            },
            loss_tone: loss_tone(stats.packet_loss_pct(), !stats.is_empty()),
            points: stats.samples_ms(),
            error: p.last_error.clone(),
        }
    }
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

    // --- connection monitor ------------------------------------------------

    use crate::app::state::{Msg, PingOutcome, PingSample};

    const EDGE: &str = "2606:4700:90:0:4b6e:7f99:a2f2:fd80";

    fn feed(s: &mut AppState, samples: &[Option<u64>]) {
        for ms in samples {
            s.update(Msg::Ping(PingSample {
                generation: s.ping.generation,
                target: Some(EDGE.parse().unwrap()),
                outcome: match ms {
                    Some(ms) => PingOutcome::Reply(std::time::Duration::from_millis(*ms)),
                    None => PingOutcome::Lost("timed out".into()),
                },
            }));
        }
    }

    #[test]
    fn before_the_first_tick_it_is_looking_for_the_client() {
        let s = AppState::new(Os::Windows, false, Config::default(), false).0;
        let v = PingView::from_state(&s);
        assert_eq!(v.status, PING_STARTING);
        assert_eq!(v.target, None);
        assert_eq!(
            (
                v.current.as_str(),
                v.average.as_str(),
                v.jitter.as_str(),
                v.loss.as_str()
            ),
            ("—", "—", "—", "—")
        );
        assert_eq!((v.jitter_tone, v.loss_tone), (Tone::Neutral, Tone::Neutral));
        assert!(v.points.is_empty());
    }

    #[test]
    fn waiting_when_tibia_is_not_connected() {
        let mut s = AppState::new(Os::Windows, false, Config::default(), false).0;
        s.update(Msg::Ping(PingSample {
            generation: s.ping.generation,
            target: None,
            outcome: PingOutcome::NoGame,
        }));
        let v = PingView::from_state(&s);
        assert_eq!(v.status, PING_WAITING);
        assert_eq!(v.target, None);
    }

    #[test]
    fn measuring_formats_stats_and_shows_the_edge() {
        let mut s = AppState::new(Os::Windows, false, Config::default(), false).0;
        feed(&mut s, &[Some(10), Some(14), None, Some(12)]);
        let v = PingView::from_state(&s);
        assert_eq!(v.status, PING_MEASURING);
        assert_eq!(v.target.as_deref(), Some(EDGE));
        assert_eq!(v.current, "12 ms");
        assert_eq!(v.average, "12 ms");
        assert_eq!(v.jitter, "3.0 ms"); // |14-10| + |12-14| = 6 / 2
        assert_eq!(v.loss, "25%");
        assert_eq!(v.points, vec![Some(10.0), Some(14.0), None, Some(12.0)]);
        assert_eq!(v.error, None, "last probe succeeded");
    }

    #[test]
    fn not_pingable_status_lists_the_addresses() {
        let mut s = AppState::new(Os::Windows, false, Config::default(), false).0;
        s.update(Msg::Ping(PingSample {
            generation: s.ping.generation,
            target: None,
            outcome: PingOutcome::NotPingable(vec!["18.192.74.45".parse().unwrap()]),
        }));
        let v = PingView::from_state(&s);
        assert_eq!(v.status, PING_NOT_PINGABLE);
        assert_eq!(v.target.as_deref(), Some("18.192.74.45"));
    }

    #[test]
    fn diagnostics_include_status_settings_and_monitor() {
        let mut s = with_report(Os::Windows, false, vec![nagle(TweakState::NotOptimized)], false);
        s.config.auto_heal = true;
        feed(&mut s, &[Some(11), None]);
        s.notice = Some(Notice::error("Auto-heal failed: timed out"));
        let d = diagnostics(&s);
        for expected in [
            "Tibia Network Optimizer",
            env!("CARGO_PKG_VERSION"),
            "Platform: windows",
            "Privileges: standard user",
            "Nagle / delayed ACK",
            "Ethernet",
            "Auto-heal: on",
            "Minimize to tray: on",
            PING_MEASURING,
            EDGE,
            "Loss: 50%",
            "Last probe error: timed out",
            "Notice: Auto-heal failed: timed out",
        ] {
            assert!(d.contains(expected), "missing {expected:?} in:\n{d}");
        }
    }

    #[test]
    fn diagnostics_work_before_the_first_status() {
        let s = AppState::new(Os::Linux, true, Config::default(), false).0;
        let d = diagnostics(&s);
        assert!(d.contains("Status: still checking"), "{d}");
        assert!(d.contains("Privileges: root"), "{d}");
    }

    #[test]
    fn explainer_says_icmp_only_and_cloudflare() {
        assert!(PING_EXPLAINER.contains("ICMP only"));
        assert!(PING_EXPLAINER.contains("Cloudflare"));
    }

    #[test]
    fn ping_tones() {
        assert_eq!(jitter_tone(None), Tone::Neutral);
        assert_eq!(jitter_tone(Some(2.0)), Tone::Good);
        assert_eq!(jitter_tone(Some(5.0)), Tone::Warn);
        assert_eq!(jitter_tone(Some(15.0)), Tone::Bad);
        assert_eq!(loss_tone(0.0, false), Tone::Neutral);
        assert_eq!(loss_tone(0.0, true), Tone::Good);
        assert_eq!(loss_tone(2.0, true), Tone::Warn);
        assert_eq!(loss_tone(2.5, true), Tone::Bad);
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
