//! Privileged operations shared by the CLI and the GUI worker.

use std::path::Path;

use crate::platform::Platform;
use crate::tweak::{Tweak, TweakId, TweakState};

/// Something that changes system settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    ApplyAll,
    RevertAll,
    SetTweak { id: TweakId, optimized: bool },
    SetStartup(bool),
}

impl Action {
    pub fn success_message(&self) -> &'static str {
        match self {
            Action::ApplyAll => "Optimizations applied.",
            Action::RevertAll => "Defaults restored.",
            Action::SetTweak { .. } => "Setting updated.",
            Action::SetStartup(true) => "Run at startup enabled.",
            Action::SetStartup(false) => "Run at startup disabled.",
        }
    }

    /// CLI arguments that perform this action (for running it elevated in a
    /// separate process).
    pub fn cli_args(&self) -> Vec<String> {
        let v: Vec<&str> = match self {
            Action::ApplyAll => vec!["apply"],
            Action::RevertAll => vec!["revert"],
            Action::SetTweak { id, optimized: true } => vec!["apply", "--only", id.key()],
            Action::SetTweak { id, optimized: false } => vec!["revert", "--only", id.key()],
            Action::SetStartup(true) => vec!["startup", "on"],
            Action::SetStartup(false) => vec!["startup", "off"],
        };
        v.into_iter().map(String::from).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Applied,
    Reverted,
    AlreadyOptimized,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TweakOutcome {
    pub label: &'static str,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunSummary {
    pub outcomes: Vec<TweakOutcome>,
    /// A reboot-sensitive tweak actually changed.
    pub reboot_recommended: bool,
}

impl RunSummary {
    /// `"<label>: <error>"` for the first failure, if any.
    pub fn first_error(&self) -> Option<String> {
        self.outcomes.iter().find_map(|o| match &o.outcome {
            Outcome::Failed(e) => Some(format!("{}: {e}", o.label)),
            _ => None,
        })
    }
}

/// Applies (or reverts) every tweak, or only `only`. Continues past
/// failures. Applying skips tweaks that are already optimized.
pub fn run_tweaks(tweaks: &[Box<dyn Tweak>], applying: bool, only: Option<TweakId>) -> RunSummary {
    let mut summary = RunSummary::default();
    for tweak in tweaks.iter().filter(|t| only.is_none_or(|id| t.id() == id)) {
        let before = tweak.check().state;
        let outcome = if applying && before == TweakState::Optimized {
            Outcome::AlreadyOptimized
        } else {
            let result = if applying { tweak.apply() } else { tweak.revert() };
            match result {
                Ok(()) => {
                    // A change happened if we moved away from (or into) optimized.
                    let changed = applying != (before == TweakState::Optimized);
                    summary.reboot_recommended |= changed && tweak.reboot_recommended();
                    if applying {
                        Outcome::Applied
                    } else {
                        Outcome::Reverted
                    }
                }
                Err(e) => Outcome::Failed(e.to_string()),
            }
        };
        summary.outcomes.push(TweakOutcome {
            label: tweak.label(),
            outcome,
        });
    }
    summary
}

/// Runs `action` in-process. `exe` is what the startup entry launches.
pub fn run_action(platform: &Platform, action: &Action, exe: &Path) -> Result<RunSummary, String> {
    let summary = match *action {
        Action::ApplyAll => run_tweaks(&platform.tweaks, true, None),
        Action::RevertAll => run_tweaks(&platform.tweaks, false, None),
        Action::SetTweak { id, optimized } => {
            let s = run_tweaks(&platform.tweaks, optimized, Some(id));
            if s.outcomes.is_empty() {
                return Err(format!("setting `{}` is not available on this OS", id.key()));
            }
            s
        }
        Action::SetStartup(on) => {
            let result = if on {
                platform.startup.register(exe)
            } else {
                platform.startup.unregister()
            };
            result.map_err(|e| e.to_string())?;
            RunSummary::default()
        }
    };
    match summary.first_error() {
        Some(e) => Err(e),
        None => Ok(summary),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Os;
    use crate::system::startup::fake::FakeStartup;
    use crate::tweak::fake::FakeTweak;
    use TweakState::*;

    fn boxed(tweaks: &[FakeTweak]) -> Vec<Box<dyn Tweak>> {
        tweaks
            .iter()
            .cloned()
            .map(|t| Box::new(t) as Box<dyn Tweak>)
            .collect()
    }

    fn a() -> FakeTweak {
        FakeTweak::new(TweakId::WinAutoTuning, "A", NotOptimized)
    }
    fn b() -> FakeTweak {
        FakeTweak::new(TweakId::WinNagle, "B", Optimized)
    }

    #[test]
    fn apply_all_skips_already_optimized() {
        let (ta, tb) = (a(), b());
        let s = run_tweaks(&boxed(&[ta.clone(), tb.clone()]), true, None);
        assert_eq!(
            s.outcomes,
            vec![
                TweakOutcome {
                    label: "A",
                    outcome: Outcome::Applied
                },
                TweakOutcome {
                    label: "B",
                    outcome: Outcome::AlreadyOptimized
                },
            ]
        );
        assert_eq!(ta.current(), Optimized);
        assert!(!s.reboot_recommended);
        assert_eq!(s.first_error(), None);
    }

    #[test]
    fn revert_all_reverts_everything() {
        let (ta, tb) = (a(), b());
        let s = run_tweaks(&boxed(&[ta.clone(), tb.clone()]), false, None);
        assert!(s.outcomes.iter().all(|o| o.outcome == Outcome::Reverted));
        assert_eq!((ta.current(), tb.current()), (NotOptimized, NotOptimized));
    }

    #[test]
    fn only_targets_a_single_tweak() {
        let (ta, tb) = (a(), b());
        let s = run_tweaks(&boxed(&[ta.clone(), tb.clone()]), false, Some(TweakId::WinNagle));
        assert_eq!(
            s.outcomes,
            vec![TweakOutcome {
                label: "B",
                outcome: Outcome::Reverted
            }]
        );
        assert_eq!(ta.current(), NotOptimized, "untouched");
    }

    #[test]
    fn failures_continue_and_are_reported() {
        let bad = a().failing();
        let other = FakeTweak::new(TweakId::WinNagle, "B", NotOptimized);
        let s = run_tweaks(&boxed(&[bad, other.clone()]), true, None);
        assert_eq!(
            s.first_error().as_deref(),
            Some("A: access denied: run as Administrator")
        );
        assert_eq!(other.current(), Optimized, "later tweaks still run");
    }

    #[test]
    fn reboot_only_when_a_reboot_sensitive_tweak_changed() {
        let s = run_tweaks(&boxed(&[a().needing_reboot()]), true, None);
        assert!(s.reboot_recommended);
        let already = FakeTweak::new(TweakId::WinAutoTuning, "A", Optimized).needing_reboot();
        assert!(!run_tweaks(&boxed(std::slice::from_ref(&already)), true, None).reboot_recommended);
        assert!(
            run_tweaks(&boxed(&[already]), false, None).reboot_recommended,
            "revert of an optimized one"
        );
    }

    fn platform(tweaks: &[FakeTweak], startup: &FakeStartup) -> Platform {
        Platform {
            os: Os::Windows,
            tweaks: boxed(tweaks),
            startup: Box::new(startup.clone()),
        }
    }

    #[test]
    fn run_action_dispatches_each_action() {
        let (ta, startup) = (a(), FakeStartup::default());
        let p = platform(std::slice::from_ref(&ta), &startup);
        let exe = Path::new("tno");

        run_action(&p, &Action::ApplyAll, exe).unwrap();
        assert_eq!(ta.current(), Optimized);
        run_action(
            &p,
            &Action::SetTweak {
                id: TweakId::WinAutoTuning,
                optimized: false,
            },
            exe,
        )
        .unwrap();
        assert_eq!(ta.current(), NotOptimized);
        run_action(&p, &Action::SetStartup(true), exe).unwrap();
        assert!(*startup.registered.lock().unwrap());
        run_action(&p, &Action::SetStartup(false), exe).unwrap();
        assert!(!*startup.registered.lock().unwrap());
    }

    #[test]
    fn run_action_reports_errors() {
        let p = platform(&[a().failing()], &FakeStartup::default());
        let err = run_action(&p, &Action::ApplyAll, Path::new("tno")).unwrap_err();
        assert!(err.starts_with("A: "), "{err}");
        let err = run_action(
            &p,
            &Action::SetTweak {
                id: TweakId::MacDelayedAck,
                optimized: true,
            },
            Path::new("tno"),
        )
        .unwrap_err();
        assert!(err.contains("not available"), "{err}");
    }

    #[test]
    fn cli_args_for_elevated_helper() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(Action::ApplyAll.cli_args(), s(&["apply"]));
        assert_eq!(Action::RevertAll.cli_args(), s(&["revert"]));
        assert_eq!(
            Action::SetTweak {
                id: TweakId::LinuxQuickAck,
                optimized: true
            }
            .cli_args(),
            s(&["apply", "--only", "linux_quick_ack"])
        );
        assert_eq!(
            Action::SetTweak {
                id: TweakId::MacDelayedAck,
                optimized: false
            }
            .cli_args(),
            s(&["revert", "--only", "mac_delayed_ack"])
        );
        assert_eq!(Action::SetStartup(true).cli_args(), s(&["startup", "on"]));
        assert_eq!(Action::SetStartup(false).cli_args(), s(&["startup", "off"]));
    }

    #[test]
    fn success_messages() {
        assert_eq!(Action::ApplyAll.success_message(), "Optimizations applied.");
        assert_eq!(Action::RevertAll.success_message(), "Defaults restored.");
        assert_eq!(
            Action::SetStartup(true).success_message(),
            "Run at startup enabled."
        );
        assert_eq!(
            Action::SetStartup(false).success_message(),
            "Run at startup disabled."
        );
        assert_eq!(
            Action::SetTweak {
                id: TweakId::WinNagle,
                optimized: true
            }
            .success_message(),
            "Setting updated."
        );
    }
}
