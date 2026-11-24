//! Application state reducer: `update(msg) -> effects`.

use crate::actions::Action;
use crate::config::Config;
use crate::platform::Os;
use crate::report::StatusReport;

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
    Refresh { autotuning_changed_at: Option<u64> },
    Run(Action),
    RunElevated(Action),
}

/// Results from the background worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerEvent {
    Status(StatusReport),
    ActionFinished {
        action: Action,
        result: Result<(), String>,
        /// Set when a reboot-sensitive setting changed (unix seconds).
        reboot_change_at: Option<u64>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    Worker(WorkerEvent),
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
    Quit,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppState {
    pub os: Os,
    pub elevated: bool,
    pub config: Config,
    /// Latest status; `None` until the first check finishes.
    pub report: Option<StatusReport>,
    /// A privileged action is in flight.
    pub busy: bool,
    pub notice: Option<Notice>,
    pub window_visible: bool,
}

impl AppState {
    /// Initial state plus the effects to run at startup.
    pub fn new(os: Os, elevated: bool, config: Config, start_minimized: bool) -> (Self, Vec<Effect>) {
        let state = AppState {
            os,
            elevated,
            config,
            report: None,
            busy: false,
            notice: None,
            window_visible: !start_minimized,
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
            Msg::Worker(WorkerEvent::Status(report)) => {
                self.report = Some(report);
                vec![]
            }
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
                vec![Effect::Worker(cmd)]
            }
            Msg::Worker(WorkerEvent::ActionFinished {
                action,
                result,
                reboot_change_at,
            }) => {
                self.busy = false;
                self.notice = Some(match result {
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
                vec![Effect::ShowWindow]
            }
            Msg::CloseRequested => {
                if self.config.minimize_to_tray {
                    self.window_visible = false;
                    vec![Effect::HideWindow]
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

    #[test]
    fn notices_can_be_dismissed() {
        let mut s = state(Os::Windows, false);
        s.update(Msg::Request(Action::ApplyAll));
        assert!(s.update(Msg::DismissNotice).is_empty());
        assert_eq!(s.notice, None);
    }
}
