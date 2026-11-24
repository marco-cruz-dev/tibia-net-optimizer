//! Status report shared by the CLI (text / `--json`) and, later, the GUI.

use serde::Serialize;

use crate::optimizer;
use crate::platform::Os;
use crate::tweak::TweakStatus;
use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusReport {
    pub platform: Os,
    /// `optimized` | `needs_fix` | `unknown`
    pub overall: String,
    pub elevated: bool,
    pub tweaks: Vec<TweakStatus>,
    /// Whether the app re-applies itself at boot/logon; `None` if unknown.
    pub startup: Option<bool>,
    pub reboot_recommended: bool,
}

/// Inputs gathered from the OS.
pub struct StatusInputs {
    pub os: Os,
    pub tweaks: Vec<TweakStatus>,
    pub startup: Result<bool>,
    pub elevated: bool,
    pub reboot_recommended: bool,
}

const LABEL_WIDTH: usize = 26;

impl StatusReport {
    pub fn build(inputs: StatusInputs) -> Self {
        StatusReport {
            platform: inputs.os,
            overall: optimizer::overall(&inputs.tweaks).as_str().to_string(),
            elevated: inputs.elevated,
            tweaks: inputs.tweaks,
            startup: inputs.startup.ok(),
            reboot_recommended: inputs.reboot_recommended,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("report is always serializable")
    }

    pub fn to_text(&self) -> String {
        let w = LABEL_WIDTH;
        let mut lines = Vec::new();

        for t in &self.tweaks {
            lines.push(format!("{:<w$}: {} ({})", t.label, t.summary, t.state.describe()));
            for d in &t.details {
                let tag = if d.optimized { "[tuned]" } else { "[default]" };
                lines.push(format!("  - {:<20} {tag:<9}  {}", d.label, d.value));
            }
        }

        let startup = match self.startup {
            Some(true) => "yes",
            Some(false) => "no",
            None => "unknown",
        };
        lines.push(format!("{:<w$}: {startup}", "Run at startup"));
        let privileges = if self.elevated {
            self.platform.admin_name()
        } else {
            "standard user (read-only)"
        };
        lines.push(format!("{:<w$}: {privileges}", "Privileges"));
        if self.reboot_recommended {
            lines.push(format!(
                "{:<w$}: Reboot recommended - auto-tuning changed since last boot",
                "Note"
            ));
        }

        let apply_cmd = if self.elevated { "apply" } else { "apply --elevate" };
        let overall = match self.overall.as_str() {
            "optimized" => "OPTIMIZED for Tibia".to_string(),
            "needs_fix" => format!("NEEDS FIX - run `tibia-net-optimizer {apply_cmd}`"),
            _ => "UNKNOWN".to_string(),
        };
        lines.push(String::new());
        lines.push(format!("{:<w$}: {overall}", "Overall"));
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tweak::{TweakDetail, TweakId, TweakState};
    use crate::Error;

    fn autotuning(state: TweakState, summary: &str) -> TweakStatus {
        TweakStatus::new(TweakId::WinAutoTuning, "TCP auto-tuning", state, summary)
    }

    fn nagle(optimized: bool) -> TweakStatus {
        TweakStatus::new(
            TweakId::WinNagle,
            "Nagle / delayed ACK",
            if optimized {
                TweakState::Optimized
            } else {
                TweakState::NotOptimized
            },
            "summary",
        )
        .with_details(vec![TweakDetail {
            label: "Ethernet".into(),
            value: "192.168.1.185  {guid}".into(),
            optimized,
        }])
    }

    fn report(
        os: Os,
        tweaks: Vec<TweakStatus>,
        startup: Result<bool>,
        elevated: bool,
        reboot: bool,
    ) -> StatusReport {
        StatusReport::build(StatusInputs {
            os,
            tweaks,
            startup,
            elevated,
            reboot_recommended: reboot,
        })
    }

    #[test]
    fn overall_is_computed_from_tweaks() {
        let r = report(
            Os::Windows,
            vec![autotuning(TweakState::Optimized, "disabled"), nagle(true)],
            Ok(true),
            true,
            false,
        );
        assert_eq!(r.overall, "optimized");
        assert_eq!(r.startup, Some(true));
        let r = report(Os::Windows, vec![nagle(false)], Ok(true), true, false);
        assert_eq!(r.overall, "needs_fix");
    }

    #[test]
    fn startup_error_becomes_unknown() {
        let r = report(Os::Linux, vec![], Err(Error::AccessDenied), false, false);
        assert_eq!(r.startup, None);
        assert_eq!(r.overall, "unknown");
    }

    #[test]
    fn json_has_stable_field_names() {
        let r = report(Os::Macos, vec![nagle(false)], Ok(false), false, true);
        let v: serde_json::Value = serde_json::from_str(&r.to_json()).unwrap();
        assert_eq!(v["platform"], "macos");
        assert_eq!(v["overall"], "needs_fix");
        assert_eq!(v["tweaks"][0]["id"], "win_nagle");
        assert_eq!(v["tweaks"][0]["state"], "not_optimized");
        assert_eq!(v["tweaks"][0]["details"][0]["label"], "Ethernet");
        assert_eq!(v["startup"], false);
        assert_eq!(v["elevated"], false);
        assert_eq!(v["reboot_recommended"], true);
    }

    #[test]
    fn text_lists_each_tweak_with_state_and_details() {
        let t = report(
            Os::Windows,
            vec![autotuning(TweakState::Optimized, "disabled"), nagle(false)],
            Ok(false),
            true,
            false,
        )
        .to_text();
        assert!(
            t.contains("TCP auto-tuning           : disabled (optimized)"),
            "{t}"
        );
        assert!(
            t.contains("Nagle / delayed ACK       : summary (not optimized)"),
            "{t}"
        );
        assert!(t.contains("  - Ethernet"), "{t}");
        assert!(t.contains("[default]  192.168.1.185  {guid}"), "{t}");
        assert!(t.contains("Run at startup            : no"), "{t}");
        assert!(t.contains("Privileges                : Administrator"), "{t}");
    }

    #[test]
    fn privileges_use_platform_wording() {
        let t = report(Os::Linux, vec![], Ok(false), true, false).to_text();
        assert!(t.contains("Privileges                : root"), "{t}");
        let t = report(Os::Macos, vec![], Ok(false), false, false).to_text();
        assert!(t.contains("standard user (read-only)"), "{t}");
    }

    #[test]
    fn needs_fix_hint_depends_on_elevation() {
        let user = report(Os::Windows, vec![nagle(false)], Ok(false), false, false).to_text();
        assert!(
            user.contains("run `tibia-net-optimizer apply --elevate`"),
            "{user}"
        );
        let admin = report(Os::Windows, vec![nagle(false)], Ok(false), true, false).to_text();
        assert!(admin.contains("run `tibia-net-optimizer apply`"), "{admin}");
    }

    #[test]
    fn text_mentions_pending_reboot_only_when_needed() {
        let t = report(Os::Windows, vec![], Ok(false), true, true).to_text();
        assert!(t.contains("Reboot recommended"), "{t}");
        let t = report(Os::Windows, vec![], Ok(false), true, false).to_text();
        assert!(!t.contains("Reboot recommended"), "{t}");
    }
}
