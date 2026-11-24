//! Status report shared by the CLI (text / `--json`) and, later, the GUI.

use serde::Serialize;

use crate::network::autotuning::AutoTuningLevel;
use crate::network::nagle::{self, ActiveAdapter, NagleStatus};
use crate::optimizer::{Overall, SystemStatus};
use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusReport {
    /// `optimized` | `needs_fix` | `unknown`
    pub overall: String,
    pub elevated: bool,
    pub autotuning: AutoTuningReport,
    pub nagle: NagleReport,
    /// `None` when the task state couldn't be queried.
    pub startup_task: Option<bool>,
    pub reboot_recommended: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AutoTuningReport {
    pub level: Option<String>,
    pub optimized: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NagleReport {
    /// `optimized` | `not_optimized` | `no_active_adapter`
    pub status: String,
    pub adapters: Vec<AdapterReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdapterReport {
    pub guid: String,
    pub name: Option<String>,
    pub ipv4: String,
    pub optimized: bool,
}

/// Inputs gathered from the OS.
pub struct StatusInputs<'a> {
    pub autotuning: &'a Result<AutoTuningLevel>,
    pub adapters: &'a [ActiveAdapter],
    pub startup_task: &'a Result<bool>,
    pub elevated: bool,
    pub reboot_recommended: bool,
}

impl StatusReport {
    pub fn build(inputs: StatusInputs<'_>) -> Self {
        let nagle_status = nagle::status_of(inputs.adapters);
        let overall = SystemStatus {
            autotuning: inputs.autotuning.as_ref().ok().cloned(),
            nagle: nagle_status,
        }
        .overall();

        let autotuning = match inputs.autotuning {
            Ok(level) => AutoTuningReport {
                level: Some(level.keyword().to_string()),
                optimized: level.is_optimized(),
                error: None,
            },
            Err(e) => AutoTuningReport {
                level: None,
                optimized: false,
                error: Some(e.to_string()),
            },
        };

        StatusReport {
            overall: match overall {
                Overall::Optimized => "optimized",
                Overall::NeedsFix => "needs_fix",
                Overall::Unknown => "unknown",
            }
            .into(),
            elevated: inputs.elevated,
            autotuning,
            nagle: NagleReport {
                status: match nagle_status {
                    NagleStatus::Optimized => "optimized",
                    NagleStatus::NotOptimized => "not_optimized",
                    NagleStatus::NoActiveAdapter => "no_active_adapter",
                }
                .into(),
                adapters: inputs
                    .adapters
                    .iter()
                    .map(|a| AdapterReport {
                        guid: a.guid.clone(),
                        name: a.name.clone(),
                        ipv4: a.ipv4.to_string(),
                        optimized: a.optimized,
                    })
                    .collect(),
            },
            startup_task: inputs.startup_task.as_ref().ok().copied(),
            reboot_recommended: inputs.reboot_recommended,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("report is always serializable")
    }

    pub fn to_text(&self) -> String {
        let mut lines = Vec::new();

        let at = &self.autotuning;
        let at_text = match (&at.level, &at.error) {
            (Some(level), _) if at.optimized => format!("{level} (optimized)"),
            (Some(level), _) => format!("{level} (not optimized)"),
            (None, Some(e)) => format!("unknown ({e})"),
            (None, None) => "unknown".into(),
        };
        lines.push(format!("TCP auto-tuning : {at_text}"));

        let nagle_text = match self.nagle.status.as_str() {
            "optimized" => "disabled on all active adapters (optimized)",
            "not_optimized" => "active on some adapters (not optimized)",
            _ => "no active IPv4 adapter found",
        };
        lines.push(format!("Nagle / ACK     : {nagle_text}"));
        for a in &self.nagle.adapters {
            let state = if a.optimized { "[tuned]" } else { "[default]" };
            let label = a.name.as_deref().unwrap_or(&a.guid);
            let suffix = if a.name.is_some() {
                format!("  {}", a.guid)
            } else {
                String::new()
            };
            lines.push(format!("  - {label:<20} {:<15} {state:<9}{suffix}", a.ipv4));
        }

        let startup = match self.startup_task {
            Some(true) => "registered",
            Some(false) => "not registered",
            None => "unknown",
        };
        lines.push(format!("Startup task    : {startup}"));
        lines.push(format!(
            "Privileges      : {}",
            if self.elevated {
                "Administrator"
            } else {
                "standard user (read-only)"
            }
        ));
        if self.reboot_recommended {
            lines.push("Note            : Reboot recommended - auto-tuning changed since last boot".into());
        }

        let apply_cmd = if self.elevated { "apply" } else { "apply --elevate" };
        let overall = match self.overall.as_str() {
            "optimized" => "OPTIMIZED for Tibia".to_string(),
            "needs_fix" => format!("NEEDS FIX - run `tibia-net-optimizer {apply_cmd}`"),
            _ => "UNKNOWN".to_string(),
        };
        lines.push(String::new());
        lines.push(format!("Overall         : {overall}"));
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use std::net::Ipv4Addr;

    fn adapter(name: Option<&str>, optimized: bool) -> ActiveAdapter {
        ActiveAdapter {
            guid: "{ac78b7b4}".into(),
            name: name.map(String::from),
            ipv4: Ipv4Addr::new(192, 168, 1, 153),
            optimized,
        }
    }

    fn report(
        at: Result<AutoTuningLevel>,
        adapters: &[ActiveAdapter],
        startup: Result<bool>,
        elevated: bool,
        reboot: bool,
    ) -> StatusReport {
        StatusReport::build(StatusInputs {
            autotuning: &at,
            adapters,
            startup_task: &startup,
            elevated,
            reboot_recommended: reboot,
        })
    }

    #[test]
    fn optimized_system() {
        let r = report(
            Ok(AutoTuningLevel::Disabled),
            &[adapter(Some("Ethernet"), true)],
            Ok(true),
            true,
            false,
        );
        assert_eq!(r.overall, "optimized");
        assert_eq!(
            r.autotuning,
            AutoTuningReport {
                level: Some("disabled".into()),
                optimized: true,
                error: None
            }
        );
        assert_eq!(r.nagle.status, "optimized");
        assert_eq!(
            r.nagle.adapters,
            vec![AdapterReport {
                guid: "{ac78b7b4}".into(),
                name: Some("Ethernet".into()),
                ipv4: "192.168.1.153".into(),
                optimized: true
            }]
        );
        assert_eq!(r.startup_task, Some(true));
    }

    #[test]
    fn errors_become_unknowns() {
        let r = report(
            Err(Error::Parse("nope".into())),
            &[],
            Err(Error::AccessDenied),
            false,
            false,
        );
        assert_eq!(r.overall, "unknown");
        assert_eq!(r.autotuning.level, None);
        assert!(r.autotuning.error.as_deref().unwrap().contains("nope"));
        assert_eq!(r.nagle.status, "no_active_adapter");
        assert_eq!(r.startup_task, None);
    }

    #[test]
    fn json_has_stable_field_names() {
        let r = report(
            Ok(AutoTuningLevel::Normal),
            &[adapter(None, false)],
            Ok(false),
            false,
            true,
        );
        let v: serde_json::Value = serde_json::from_str(&r.to_json()).unwrap();
        assert_eq!(v["overall"], "needs_fix");
        assert_eq!(v["autotuning"]["level"], "normal");
        assert_eq!(v["nagle"]["status"], "not_optimized");
        assert_eq!(v["nagle"]["adapters"][0]["ipv4"], "192.168.1.153");
        assert_eq!(v["startup_task"], false);
        assert_eq!(v["elevated"], false);
        assert_eq!(v["reboot_recommended"], true);
    }

    #[test]
    fn text_shows_friendly_name_and_tuning_state() {
        let r = report(
            Ok(AutoTuningLevel::Disabled),
            &[adapter(Some("Ethernet"), false)],
            Ok(false),
            true,
            false,
        );
        let t = r.to_text();
        assert!(t.contains("TCP auto-tuning : disabled (optimized)"), "{t}");
        assert!(t.contains("Ethernet"), "{t}");
        assert!(t.contains("[default]"), "{t}");
        assert!(t.contains("Startup task    : not registered"), "{t}");
        assert!(t.contains("Privileges      : Administrator"), "{t}");
    }

    #[test]
    fn text_falls_back_to_guid_without_name() {
        let t = report(
            Ok(AutoTuningLevel::Disabled),
            &[adapter(None, true)],
            Ok(true),
            true,
            false,
        )
        .to_text();
        assert!(t.contains("{ac78b7b4}") && t.contains("[tuned]"), "{t}");
    }

    #[test]
    fn needs_fix_hint_depends_on_elevation() {
        let at = || Ok(AutoTuningLevel::Normal);
        let user = report(at(), &[], Ok(false), false, false).to_text();
        assert!(user.contains("apply --elevate"), "{user}");
        assert!(user.contains("standard user (read-only)"), "{user}");
        let admin = report(at(), &[], Ok(false), true, false).to_text();
        assert!(admin.contains("run `tibia-net-optimizer apply`"), "{admin}");
    }

    #[test]
    fn text_mentions_pending_reboot() {
        let t = report(Ok(AutoTuningLevel::Disabled), &[], Ok(false), true, true).to_text();
        assert!(t.contains("Reboot recommended"), "{t}");
        let t = report(Ok(AutoTuningLevel::Disabled), &[], Ok(false), true, false).to_text();
        assert!(!t.contains("Reboot recommended"), "{t}");
    }
}
