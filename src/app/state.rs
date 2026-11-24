//! Application state reducer: `update(msg) -> effects`.

use crate::actions::Action;
use crate::config::Config;
use crate::network::ping::LatencyStats;
use crate::network::probe::parse_target;
use crate::optimizer::heal_plan;
use crate::platform::Os;
use crate::report::StatusReport;
use crate::tweak::TweakId;

/// How a privileged action is carried out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// We have admin/root rights: run inside the worker.
    InProcess,
    /// Unix without root: run `<exe> <cli args>` through pkexec / osascript.
    ElevatedHelper,
    /// Windows without admin: the user must restart the app elevated.
    NeedsRestart,
}

pub fn route(os: Os, elevated: bool) -> Route {
    match (elevated, os) {
        (true, _) => Route::InProcess,
        (false, Os::Windows) => Route::NeedsRestart,
        (false, Os::Linux | Os::Macos) => Route::ElevatedHelper,
    }
}

/// Commands for the background worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerCmd {
    Refresh {
        autotuning_changed_at: Option<u64>,
    },
    Run(Action),
    RunElevated(Action),
    /// Re-apply tweaks that drifted back to their defaults.
    Heal(Vec<TweakId>),
}

/// Results from the background worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerEvent {
    /// After a `Refresh`, or on its own every check interval.
    Status(StatusReport),
    ActionFinished {
        action: Action,
        result: Result<(), String>,
        /// Set when a reboot-sensitive setting changed (unix seconds).
        reboot_change_at: Option<u64>,
    },
    Healed {
        /// Labels of the tweaks that were re-applied.
        labels: Vec<String>,
        result: Result<(), String>,
        reboot_change_at: Option<u64>,
    },
}

/// One latency probe result from the monitor thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PingSample {
    /// Which monitor run produced it (stale samples are ignored).
    pub generation: u64,
    /// `None` = lost / timed out.
    pub rtt: Option<std::time::Duration>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    Worker(WorkerEvent),
    /// Latency monitor: target text edited, Start, Stop, sample received.
    PingTargetEdited(String),
    PingStart,
    PingStop,
    Ping(PingSample),
    Refresh,
    /// A button, toggle, checkbox or tray item asking to change the system.
    Request(Action),
    RestartElevated,
    RestartFailed(String),
    SetAutoHeal(bool),
    SetMinimizeToTray(bool),
    /// Tray icon left-click or "Open Dashboard".
    ShowDashboard,
    /// Window close button.
    CloseRequested,
    Quit,
    DismissNotice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Worker(WorkerCmd),
    SaveConfig,
    ShowWindow,
    HideWindow,
    /// Relaunch the app elevated, then quit.
    RestartElevated,
    /// Desktop notification (toast).
    Notify {
        title: String,
        body: String,
    },
    /// Start (replacing any running) latency monitor for `target`.
    StartPing {
        target: String,
        generation: u64,
    },
    StopPing,
    Quit,
}

/// Samples kept for stats and the sparkline (≈2 minutes at one per 2 s).
pub const PING_WINDOW: usize = 60;

/// Live latency monitor state.
#[derive(Debug, Clone, PartialEq)]
pub struct PingState {
    /// Text in the target box (saved to config on Start).
    pub target_input: String,
    /// The user pressed Start (stays true while hidden in the tray).
    pub running: bool,
    /// A monitor thread is actually probing (running and window visible).
    pub active: bool,
    pub generation: u64,
    pub stats: LatencyStats,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Info,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub text: String,
}

impl Notice {
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            kind: NoticeKind::Info,
            text: text.into(),
        }
    }
    pub fn error(text: impl Into<String>) -> Self {
        Self {
            kind: NoticeKind::Error,
            text: text.into(),
        }
    }
}

pub const BUSY_NOTICE: &str = "Please wait - another change is in progress.";
pub const RESTART_NOTICE: &str = "Restart as Administrator to change settings.";
pub const APP_TITLE: &str = "Tibia Network Optimizer";
pub const AUTO_HEAL_OFF_NOTE: &str = "Auto-heal was turned off so it won't undo your change.";

#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    pub os: Os,
    pub elevated: bool,
    pub config: Config,
    /// Latest status; `None` until the first check finishes.
    pub report: Option<StatusReport>,
    /// A privileged action (or heal) is in flight.
    pub busy: bool,
    pub notice: Option<Notice>,
    pub window_visible: bool,
    /// Last heal error, so the same failure isn't notified every interval.
    pub last_heal_error: Option<String>,
    /// Skip healing on the status that directly follows a heal (avoids a
    /// tight loop when a setting doesn't stick); retry on the next interval.
    pub skip_next_heal: bool,
    /// The current user action turned auto-heal off; mention it when done.
    pub auto_heal_turned_off: bool,
    pub ping: PingState,
}

impl AppState {
    /// Initial state plus the effects to run at startup.
    pub fn new(os: Os, elevated: bool, config: Config, start_minimized: bool) -> (Self, Vec<Effect>) {
        let ping = PingState {
            target_input: config.ping_target.clone(),
            running: false,
            active: false,
            generation: 0,
            stats: LatencyStats::new(PING_WINDOW),
            last_error: None,
        };
        let state = AppState {
            ping,
            os,
            elevated,
            config,
            report: None,
            busy: false,
            notice: None,
            window_visible: !start_minimized,
            last_heal_error: None,
            skip_next_heal: false,
            auto_heal_turned_off: false,
        };
        let effects = vec![state.refresh()];
        (state, effects)
    }

    pub fn route(&self) -> Route {
        route(self.os, self.elevated)
    }

    fn refresh(&self) -> Effect {
        Effect::Worker(WorkerCmd::Refresh {
            autotuning_changed_at: self.config.autotuning_changed_at,
        })
    }

    pub fn update(&mut self, msg: Msg) -> Vec<Effect> {
        match msg {
            Msg::Refresh => vec![self.refresh()],
            Msg::PingTargetEdited(text) => {
                self.ping.target_input = text;
                vec![]
            }
            Msg::PingStart => self.ping_start(),
            Msg::PingStop => {
                self.ping.running = false;
                self.sync_ping()
            }
            Msg::Ping(sample) => {
                self.on_ping(sample);
                vec![]
            }
            Msg::Worker(WorkerEvent::Status(report)) => {
                self.report = Some(report);
                self.heal_effects()
            }
            Msg::Worker(WorkerEvent::Healed {
                labels,
                result,
                reboot_change_at,
            }) => self.on_healed(labels, result, reboot_change_at),
            Msg::Request(action) => {
                if self.busy {
                    self.notice = Some(Notice::info(BUSY_NOTICE));
                    return vec![];
                }
                let cmd = match self.route() {
                    Route::InProcess => WorkerCmd::Run(action),
                    Route::ElevatedHelper => WorkerCmd::RunElevated(action),
                    Route::NeedsRestart => {
                        self.notice = Some(Notice::info(RESTART_NOTICE));
                        return vec![];
                    }
                };
                self.busy = true;
                self.notice = None;
                let mut effects = self.pause_auto_heal_for(&action);
                effects.push(Effect::Worker(cmd));
                effects
            }
            Msg::Worker(WorkerEvent::ActionFinished {
                action,
                result,
                reboot_change_at,
            }) => {
                self.busy = false;
                let note = std::mem::take(&mut self.auto_heal_turned_off);
                self.notice = Some(match result {
                    Ok(()) if note => {
                        Notice::info(format!("{} {AUTO_HEAL_OFF_NOTE}", action.success_message()))
                    }
                    Ok(()) => Notice::info(action.success_message()),
                    Err(e) => Notice::error(e),
                });
                let mut effects = Vec::new();
                if let Some(at) = reboot_change_at {
                    self.config.autotuning_changed_at = Some(at);
                    effects.push(Effect::SaveConfig);
                }
                effects.push(self.refresh());
                effects
            }
            Msg::RestartElevated => {
                if self.route() == Route::NeedsRestart {
                    vec![Effect::RestartElevated]
                } else {
                    vec![]
                }
            }
            Msg::RestartFailed(e) => {
                self.notice = Some(Notice::error(e));
                vec![]
            }
            Msg::SetAutoHeal(on) => self.set_flag(|c| &mut c.auto_heal, on),
            Msg::SetMinimizeToTray(on) => self.set_flag(|c| &mut c.minimize_to_tray, on),
            Msg::ShowDashboard => {
                self.window_visible = true;
                let mut effects = vec![Effect::ShowWindow];
                effects.extend(self.sync_ping());
                effects
            }
            Msg::CloseRequested => {
                if self.config.minimize_to_tray {
                    self.window_visible = false;
                    let mut effects = vec![Effect::HideWindow];
                    effects.extend(self.sync_ping());
                    effects
                } else {
                    vec![Effect::Quit]
                }
            }
            Msg::Quit => vec![Effect::Quit],
            Msg::DismissNotice => {
                self.notice = None;
                vec![]
            }
        }
    }

    /// Validates the target, remembers it, resets the stats and starts.
    fn ping_start(&mut self) -> Vec<Effect> {
        let input = self.ping.target_input.trim().to_string();
        if let Err(e) = parse_target(&input) {
            self.notice = Some(Notice::error(format!("Invalid ping target: {e}")));
            return vec![];
        }
        let mut effects = Vec::new();
        if self.config.ping_target != input {
            self.config.ping_target = input.clone();
            effects.push(Effect::SaveConfig);
        }
        self.ping.target_input = input;
        self.ping.running = true;
        self.ping.stats.clear();
        self.ping.last_error = None;
        // A Start while already probing restarts with the (new) target.
        self.ping.active = false;
        effects.extend(self.sync_ping());
        effects
    }

    /// Probes run only while the user wants them *and* the window is visible.
    fn sync_ping(&mut self) -> Vec<Effect> {
        let wanted = self.ping.running && self.window_visible;
        match (wanted, self.ping.active) {
            (true, false) => {
                self.ping.generation += 1;
                self.ping.active = true;
                vec![Effect::StartPing {
                    target: self.config.ping_target.clone(),
                    generation: self.ping.generation,
                }]
            }
            (false, true) => {
                self.ping.active = false;
                vec![Effect::StopPing]
            }
            _ => vec![],
        }
    }

    fn on_ping(&mut self, sample: PingSample) {
        if !self.ping.active || sample.generation != self.ping.generation {
            return; // from a stopped or replaced monitor
        }
        self.ping.stats.record(sample.rtt);
        self.ping.last_error = sample.error;
    }

    /// After a status update: start a heal if auto-heal applies.
    fn heal_effects(&mut self) -> Vec<Effect> {
        if std::mem::take(&mut self.skip_next_heal) {
            return vec![];
        }
        if !self.config.auto_heal || self.busy || self.route() != Route::InProcess {
            return vec![];
        }
        let Some(report) = &self.report else {
            return vec![];
        };
        let plan = heal_plan(&report.tweaks, &self.config, self.elevated);
        if plan.is_empty() {
            return vec![];
        }
        self.busy = true;
        vec![Effect::Worker(WorkerCmd::Heal(plan))]
    }

    /// Reverting while auto-heal is on would be undone a minute later, so
    /// auto-heal is switched off (and persisted).
    fn pause_auto_heal_for(&mut self, action: &Action) -> Vec<Effect> {
        let reverting = matches!(
            action,
            Action::RevertAll | Action::SetTweak { optimized: false, .. }
        );
        if !(reverting && self.config.auto_heal) {
            return vec![];
        }
        self.config.auto_heal = false;
        self.auto_heal_turned_off = true;
        vec![Effect::SaveConfig]
    }

    fn on_healed(
        &mut self,
        labels: Vec<String>,
        result: Result<(), String>,
        reboot_change_at: Option<u64>,
    ) -> Vec<Effect> {
        self.busy = false;
        match result {
            Ok(()) => {
                self.last_heal_error = None;
                self.skip_next_heal = true;
                let list = labels.join(", ");
                self.notice = Some(Notice::info(format!("Auto-heal re-applied: {list}.")));
                let mut effects = vec![Effect::Notify {
                    title: APP_TITLE.into(),
                    body: format!("Settings were reset by the system and have been re-applied: {list}."),
                }];
                if let Some(at) = reboot_change_at {
                    self.config.autotuning_changed_at = Some(at);
                    effects.push(Effect::SaveConfig);
                }
                effects.push(self.refresh());
                effects
            }
            Err(error) => {
                let text = format!("Auto-heal failed: {error}");
                self.notice = Some(Notice::error(text.clone()));
                let is_new = self.last_heal_error.as_deref() != Some(error.as_str());
                self.last_heal_error = Some(error);
                if is_new {
                    vec![Effect::Notify {
                        title: APP_TITLE.into(),
                        body: text,
                    }]
                } else {
                    vec![]
                }
            }
        }
    }

    fn set_flag(&mut self, field: impl Fn(&mut Config) -> &mut bool, on: bool) -> Vec<Effect> {
        let flag = field(&mut self.config);
        if *flag == on {
            return vec![];
        }
        *flag = on;
        vec![Effect::SaveConfig]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::StatusInputs;

    fn state(os: Os, elevated: bool) -> AppState {
        AppState::new(os, elevated, Config::default(), false).0
    }

    fn refresh_effect(changed_at: Option<u64>) -> Effect {
        Effect::Worker(WorkerCmd::Refresh {
            autotuning_changed_at: changed_at,
        })
    }

    fn report() -> StatusReport {
        StatusReport::build(StatusInputs {
            os: Os::Windows,
            tweaks: vec![],
            startup: Ok(false),
            elevated: true,
            reboot_recommended: false,
        })
    }

    fn finished(action: Action, result: Result<(), String>, reboot: Option<u64>) -> Msg {
        Msg::Worker(WorkerEvent::ActionFinished {
            action,
            result,
            reboot_change_at: reboot,
        })
    }

    #[test]
    fn routing_table() {
        assert_eq!(route(Os::Windows, true), Route::InProcess);
        assert_eq!(route(Os::Linux, true), Route::InProcess);
        assert_eq!(route(Os::Macos, true), Route::InProcess);
        assert_eq!(route(Os::Windows, false), Route::NeedsRestart);
        assert_eq!(route(Os::Linux, false), Route::ElevatedHelper);
        assert_eq!(route(Os::Macos, false), Route::ElevatedHelper);
    }

    #[test]
    fn startup_refreshes_and_respects_minimized() {
        let config = Config {
            autotuning_changed_at: Some(7),
            ..Config::default()
        };
        let (s, effects) = AppState::new(Os::Windows, true, config.clone(), true);
        assert_eq!(effects, vec![refresh_effect(Some(7))]);
        assert!(!s.window_visible);
        assert!(s.report.is_none() && !s.busy && s.notice.is_none());
        assert!(AppState::new(Os::Windows, true, config, false).0.window_visible);
    }

    #[test]
    fn refresh_and_status() {
        let mut s = state(Os::Windows, true);
        assert_eq!(s.update(Msg::Refresh), vec![refresh_effect(None)]);
        assert!(s.update(Msg::Worker(WorkerEvent::Status(report()))).is_empty());
        assert_eq!(s.report, Some(report()));
    }

    #[test]
    fn request_runs_in_process_when_elevated() {
        let mut s = state(Os::Windows, true);
        s.notice = Some(Notice::info("old"));
        assert_eq!(
            s.update(Msg::Request(Action::ApplyAll)),
            vec![Effect::Worker(WorkerCmd::Run(Action::ApplyAll))]
        );
        assert!(s.busy);
        assert_eq!(s.notice, None, "stale notice cleared");
    }

    #[test]
    fn request_uses_elevated_helper_on_unix_without_root() {
        let mut s = state(Os::Linux, false);
        assert_eq!(
            s.update(Msg::Request(Action::RevertAll)),
            vec![Effect::Worker(WorkerCmd::RunElevated(Action::RevertAll))]
        );
        assert!(s.busy);
    }

    #[test]
    fn request_on_windows_without_admin_asks_for_restart() {
        let mut s = state(Os::Windows, false);
        assert!(s.update(Msg::Request(Action::ApplyAll)).is_empty());
        assert!(!s.busy);
        assert_eq!(s.notice, Some(Notice::info(RESTART_NOTICE)));
    }

    #[test]
    fn requests_while_busy_are_rejected() {
        let mut s = state(Os::Windows, true);
        s.update(Msg::Request(Action::ApplyAll));
        assert!(s.update(Msg::Request(Action::RevertAll)).is_empty());
        assert_eq!(s.notice, Some(Notice::info(BUSY_NOTICE)));
    }

    #[test]
    fn finished_action_reports_and_refreshes() {
        let mut s = state(Os::Windows, true);
        s.update(Msg::Request(Action::ApplyAll));
        assert_eq!(
            s.update(finished(Action::ApplyAll, Ok(()), None)),
            vec![refresh_effect(None)]
        );
        assert!(!s.busy);
        assert_eq!(s.notice, Some(Notice::info("Optimizations applied.")));

        s.update(Msg::Request(Action::ApplyAll));
        s.update(finished(Action::ApplyAll, Err("A: denied".into()), None));
        assert_eq!(s.notice, Some(Notice::error("A: denied")));
        assert!(!s.busy);
    }

    #[test]
    fn reboot_sensitive_change_is_persisted() {
        let mut s = state(Os::Windows, true);
        s.update(Msg::Request(Action::ApplyAll));
        assert_eq!(
            s.update(finished(Action::ApplyAll, Ok(()), Some(1_764_000_000))),
            vec![Effect::SaveConfig, refresh_effect(Some(1_764_000_000))]
        );
        assert_eq!(s.config.autotuning_changed_at, Some(1_764_000_000));
    }

    #[test]
    fn restart_elevated_only_offered_on_windows_without_admin() {
        assert_eq!(
            state(Os::Windows, false).update(Msg::RestartElevated),
            vec![Effect::RestartElevated]
        );
        assert!(state(Os::Windows, true).update(Msg::RestartElevated).is_empty());
        assert!(state(Os::Linux, false).update(Msg::RestartElevated).is_empty());
        let mut s = state(Os::Windows, false);
        s.update(Msg::RestartFailed("UAC declined".into()));
        assert_eq!(s.notice, Some(Notice::error("UAC declined")));
    }

    #[test]
    fn checkboxes_update_config_and_save_only_on_change() {
        let mut s = state(Os::Windows, true);
        assert_eq!(s.update(Msg::SetAutoHeal(true)), vec![Effect::SaveConfig]);
        assert!(s.config.auto_heal);
        assert!(s.update(Msg::SetAutoHeal(true)).is_empty());
        assert_eq!(s.update(Msg::SetMinimizeToTray(false)), vec![Effect::SaveConfig]);
        assert!(!s.config.minimize_to_tray);
    }

    #[test]
    fn close_hides_to_tray_or_quits() {
        let mut s = state(Os::Windows, true);
        assert_eq!(s.update(Msg::CloseRequested), vec![Effect::HideWindow]);
        assert!(!s.window_visible);
        assert_eq!(s.update(Msg::ShowDashboard), vec![Effect::ShowWindow]);
        assert!(s.window_visible);

        s.update(Msg::SetMinimizeToTray(false));
        assert_eq!(s.update(Msg::CloseRequested), vec![Effect::Quit]);
        assert_eq!(s.update(Msg::Quit), vec![Effect::Quit]);
    }

    // --- auto-heal ---------------------------------------------------------

    use crate::tweak::{TweakState, TweakStatus};

    fn drifted_report() -> StatusReport {
        StatusReport::build(StatusInputs {
            os: Os::Windows,
            tweaks: vec![
                TweakStatus::new(
                    TweakId::WinAutoTuning,
                    "TCP auto-tuning",
                    TweakState::NotOptimized,
                    "normal",
                ),
                TweakStatus::new(TweakId::WinNagle, "Nagle", TweakState::Optimized, "ok"),
            ],
            startup: Ok(true),
            elevated: true,
            reboot_recommended: false,
        })
    }

    fn healing(os: Os, elevated: bool) -> AppState {
        let config = Config {
            auto_heal: true,
            ..Config::default()
        };
        AppState::new(os, elevated, config, true).0
    }

    fn status(r: StatusReport) -> Msg {
        Msg::Worker(WorkerEvent::Status(r))
    }

    fn healed(labels: &[&str], result: Result<(), String>) -> Msg {
        Msg::Worker(WorkerEvent::Healed {
            labels: labels.iter().map(|s| s.to_string()).collect(),
            result,
            reboot_change_at: None,
        })
    }

    #[test]
    fn drift_triggers_heal_when_elevated_and_enabled() {
        let mut s = healing(Os::Windows, true);
        assert_eq!(
            s.update(status(drifted_report())),
            vec![Effect::Worker(WorkerCmd::Heal(vec![TweakId::WinAutoTuning]))]
        );
        assert!(s.busy);
    }

    #[test]
    fn no_heal_when_disabled_not_elevated_busy_or_optimized() {
        let mut off = state(Os::Windows, true);
        assert!(off.update(status(drifted_report())).is_empty(), "auto-heal off");

        for (os, elevated) in [(Os::Windows, false), (Os::Linux, false)] {
            let mut s = healing(os, elevated);
            assert!(
                s.update(status(drifted_report())).is_empty(),
                "{os:?} not elevated"
            );
        }

        let mut busy = healing(Os::Windows, true);
        busy.busy = true;
        assert!(busy.update(status(drifted_report())).is_empty(), "busy");

        let mut fine = healing(Os::Windows, true);
        assert!(fine.update(status(report())).is_empty(), "nothing drifted");
    }

    #[test]
    fn successful_heal_notifies_and_refreshes() {
        let mut s = healing(Os::Windows, true);
        s.update(status(drifted_report()));
        let effects = s.update(healed(&["TCP auto-tuning"], Ok(())));
        assert_eq!(
            effects,
            vec![
                Effect::Notify {
                    title: APP_TITLE.into(),
                    body: "Settings were reset by the system and have been re-applied: TCP auto-tuning."
                        .into()
                },
                refresh_effect(None),
            ]
        );
        assert!(!s.busy);
        assert_eq!(
            s.notice,
            Some(Notice::info("Auto-heal re-applied: TCP auto-tuning."))
        );
    }

    #[test]
    fn heal_that_does_not_stick_waits_for_the_next_interval() {
        let mut s = healing(Os::Windows, true);
        s.update(status(drifted_report()));
        s.update(healed(&["TCP auto-tuning"], Ok(())));
        assert!(
            s.update(status(drifted_report())).is_empty(),
            "no immediate retry"
        );
        assert_eq!(
            s.update(status(drifted_report())),
            vec![Effect::Worker(WorkerCmd::Heal(vec![TweakId::WinAutoTuning]))],
            "next periodic check retries"
        );
    }

    #[test]
    fn heal_errors_notify_once_per_distinct_error() {
        let mut s = healing(Os::Windows, true);
        s.update(status(drifted_report()));
        let first = s.update(healed(&[], Err("TCP auto-tuning: access denied".into())));
        assert_eq!(
            first,
            vec![Effect::Notify {
                title: APP_TITLE.into(),
                body: "Auto-heal failed: TCP auto-tuning: access denied".into()
            }]
        );
        assert_eq!(
            s.notice,
            Some(Notice::error("Auto-heal failed: TCP auto-tuning: access denied"))
        );
        assert!(!s.busy);

        s.update(status(drifted_report()));
        assert!(
            s.update(healed(&[], Err("TCP auto-tuning: access denied".into())))
                .is_empty(),
            "same error is not re-notified"
        );
        s.update(status(drifted_report()));
        assert_eq!(
            s.update(healed(&[], Err("other".into()))).len(),
            1,
            "new error is notified"
        );
    }

    #[test]
    fn heal_reboot_change_is_persisted() {
        let mut s = healing(Os::Windows, true);
        s.update(status(drifted_report()));
        let effects = s.update(Msg::Worker(WorkerEvent::Healed {
            labels: vec!["TCP auto-tuning".into()],
            result: Ok(()),
            reboot_change_at: Some(99),
        }));
        assert!(effects.contains(&Effect::SaveConfig));
        assert!(effects.contains(&refresh_effect(Some(99))));
        assert_eq!(s.config.autotuning_changed_at, Some(99));
    }

    #[test]
    fn reverting_turns_auto_heal_off_and_says_so() {
        for action in [
            Action::RevertAll,
            Action::SetTweak {
                id: TweakId::WinNagle,
                optimized: false,
            },
        ] {
            let mut s = healing(Os::Windows, true);
            assert_eq!(
                s.update(Msg::Request(action)),
                vec![Effect::SaveConfig, Effect::Worker(WorkerCmd::Run(action))]
            );
            assert!(!s.config.auto_heal);
            s.update(finished(action, Ok(()), None));
            let text = s.notice.clone().unwrap().text;
            assert!(text.ends_with(AUTO_HEAL_OFF_NOTE), "{text}");
        }
    }

    #[test]
    fn applying_keeps_auto_heal_on() {
        let mut s = healing(Os::Windows, true);
        assert_eq!(
            s.update(Msg::Request(Action::ApplyAll)),
            vec![Effect::Worker(WorkerCmd::Run(Action::ApplyAll))]
        );
        assert!(s.config.auto_heal);
        s.update(finished(Action::ApplyAll, Ok(()), None));
        assert_eq!(s.notice, Some(Notice::info("Optimizations applied.")));
    }

    // --- latency monitor ---------------------------------------------------

    fn sample(generation: u64, ms: Option<u64>) -> Msg {
        Msg::Ping(PingSample {
            generation,
            rtt: ms.map(std::time::Duration::from_millis),
            error: ms.is_none().then(|| "timed out".to_string()),
        })
    }

    #[test]
    fn ping_starts_with_saved_target_and_new_generation() {
        let mut s = state(Os::Windows, false);
        assert_eq!(s.ping.target_input, crate::config::DEFAULT_PING_TARGET);
        s.update(Msg::PingTargetEdited(" 1.2.3.4:7172 ".into()));
        assert_eq!(
            s.update(Msg::PingStart),
            vec![
                Effect::SaveConfig,
                Effect::StartPing {
                    target: "1.2.3.4:7172".into(),
                    generation: 1
                }
            ]
        );
        assert_eq!(s.config.ping_target, "1.2.3.4:7172");
        assert!(s.ping.running && s.ping.active);
    }

    #[test]
    fn ping_start_with_unchanged_target_does_not_save() {
        let mut s = state(Os::Windows, false);
        assert_eq!(
            s.update(Msg::PingStart),
            vec![Effect::StartPing {
                target: crate::config::DEFAULT_PING_TARGET.into(),
                generation: 1
            }]
        );
    }

    #[test]
    fn invalid_target_is_rejected_with_a_notice() {
        let mut s = state(Os::Windows, false);
        s.update(Msg::PingTargetEdited("host:abc".into()));
        assert!(s.update(Msg::PingStart).is_empty());
        assert!(!s.ping.running);
        let notice = s.notice.clone().unwrap();
        assert_eq!(notice.kind, NoticeKind::Error);
        assert!(notice.text.starts_with("Invalid ping target"), "{}", notice.text);
    }

    #[test]
    fn samples_feed_stats_and_stale_ones_are_ignored() {
        let mut s = state(Os::Windows, false);
        s.update(Msg::PingStart);
        s.update(sample(1, Some(50)));
        s.update(sample(1, None));
        s.update(sample(0, Some(999))); // from an older run
        assert_eq!(s.ping.stats.samples_ms(), vec![Some(50.0), None]);
        assert_eq!(s.ping.last_error.as_deref(), Some("timed out"));
        s.update(sample(1, Some(52)));
        assert_eq!(s.ping.last_error, None, "cleared by a success");
    }

    #[test]
    fn restart_resets_stats() {
        let mut s = state(Os::Windows, false);
        s.update(Msg::PingStart);
        s.update(sample(1, Some(50)));
        s.update(Msg::PingStop);
        s.update(Msg::PingStart);
        assert!(s.ping.stats.is_empty());
        assert_eq!(s.ping.generation, 2);
    }

    #[test]
    fn stop_stops_the_monitor() {
        let mut s = state(Os::Windows, false);
        s.update(Msg::PingStart);
        assert_eq!(s.update(Msg::PingStop), vec![Effect::StopPing]);
        assert!(!s.ping.running && !s.ping.active);
        assert!(s.update(Msg::PingStop).is_empty(), "already stopped");
        s.update(sample(1, Some(50)));
        assert!(s.ping.stats.is_empty(), "samples after stop are ignored");
    }

    #[test]
    fn hiding_to_tray_pauses_and_showing_resumes() {
        let mut s = state(Os::Windows, false);
        s.update(Msg::PingStart);
        assert_eq!(
            s.update(Msg::CloseRequested),
            vec![Effect::HideWindow, Effect::StopPing]
        );
        assert!(s.ping.running && !s.ping.active, "still wanted, just paused");
        assert_eq!(
            s.update(Msg::ShowDashboard),
            vec![
                Effect::ShowWindow,
                Effect::StartPing {
                    target: crate::config::DEFAULT_PING_TARGET.into(),
                    generation: 2
                }
            ]
        );
    }

    #[test]
    fn starting_while_hidden_waits_for_the_window() {
        let mut s = AppState::new(Os::Windows, false, Config::default(), true).0;
        assert!(s.update(Msg::PingStart).is_empty());
        assert!(s.ping.running && !s.ping.active);
    }

    #[test]
    fn notices_can_be_dismissed() {
        let mut s = state(Os::Windows, false);
        s.update(Msg::Request(Action::ApplyAll));
        assert!(s.update(Msg::DismissNotice).is_empty());
        assert_eq!(s.notice, None);
    }
}
