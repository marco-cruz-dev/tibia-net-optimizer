//! TCP Receive Window Auto-Tuning via `netsh int tcp ... global`.

use crate::system::command::CommandRunner;
use crate::{Error, Result};

const NETSH: &str = "netsh";
const ENGLISH_LABEL: &str = "Receive Window Auto-Tuning Level";

/// Values accepted by `netsh int tcp set global autotuninglevel=`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoTuningLevel {
    Disabled,
    HighlyRestricted,
    Restricted,
    Normal,
    Experimental,
    /// Anything we don't recognise (kept verbatim for display).
    Other(String),
}

impl AutoTuningLevel {
    pub fn from_keyword(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "disabled" => Self::Disabled,
            "highlyrestricted" => Self::HighlyRestricted,
            "restricted" => Self::Restricted,
            "normal" => Self::Normal,
            "experimental" => Self::Experimental,
            _ => Self::Other(value.trim().to_string()),
        }
    }

    /// The netsh keyword (`disabled`, `normal`, ...), or the raw text for `Other`.
    pub fn keyword(&self) -> &str {
        match self {
            Self::Disabled => "disabled",
            Self::HighlyRestricted => "highlyrestricted",
            Self::Restricted => "restricted",
            Self::Normal => "normal",
            Self::Experimental => "experimental",
            Self::Other(s) => s,
        }
    }

    /// Tibia-optimal setting is `disabled`.
    pub fn is_optimized(&self) -> bool {
        matches!(self, Self::Disabled)
    }
}

/// Extracts the auto-tuning level from `netsh int tcp show global` output.
///
/// Prefers the English label; falls back to the first label containing
/// "auto" so localized Windows (e.g. PT "Ajuste Automático", DE "Autom.
/// Optimierungsgrad") still works — netsh keeps the *values* in English.
pub fn parse_level(netsh_output: &str) -> Option<AutoTuningLevel> {
    let entries: Vec<(&str, &str)> = netsh_output
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(k, v)| (k.trim(), v.trim()))
        .collect();

    entries
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(ENGLISH_LABEL))
        .or_else(|| entries.iter().find(|(k, _)| k.to_lowercase().contains("auto")))
        .filter(|(_, v)| !v.is_empty())
        .map(|(_, v)| AutoTuningLevel::from_keyword(v))
}

/// Reads the current level (works without elevation).
pub fn current_level(runner: &impl CommandRunner) -> Result<AutoTuningLevel> {
    let out = runner
        .run(NETSH, &["int", "tcp", "show", "global"])?
        .ensure_success(NETSH)?;
    parse_level(&out.stdout).ok_or_else(|| Error::Parse("auto-tuning level not found in netsh output".into()))
}

/// Sets `autotuninglevel=disabled`. Requires Administrator rights.
pub fn apply(runner: &impl CommandRunner) -> Result<()> {
    set_level(runner, "disabled")
}

/// Restores the Windows default `autotuninglevel=normal`.
pub fn revert(runner: &impl CommandRunner) -> Result<()> {
    set_level(runner, "normal")
}

fn set_level(runner: &impl CommandRunner, level: &str) -> Result<()> {
    let arg = format!("autotuninglevel={level}");
    runner
        .run(NETSH, &["int", "tcp", "set", "global", &arg])?
        .ensure_success(NETSH)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::command::fake::FakeRunner;

    const EN_NORMAL: &str = "Querying active state...\r\n\r\n\
TCP Global Parameters\r\n\
----------------------------------------------\r\n\
Receive-Side Scaling State          : enabled\r\n\
Receive Window Auto-Tuning Level    : normal\r\n\
Add-On Congestion Control Provider  : default\r\n\
ECN Capability                      : disabled\r\n\
RFC 1323 Timestamps                 : allowed\r\n\
Initial RTO                         : 1000\r\n\
Non Sack Rtt Resiliency             : disabled\r\n";

    const EN_DISABLED: &str = "TCP Global Parameters\r\n\
Receive-Side Scaling State          : enabled\r\n\
Receive Window Auto-Tuning Level    : disabled\r\n";

    // Lossy-decoded OEM output: accented chars may become U+FFFD.
    const PT_DISABLED: &str = "Par\u{FFFD}metros Globais TCP\r\n\
Estado de Escala do Lado da Rece\u{FFFD}\u{FFFD}o       : enabled\r\n\
N\u{FFFD}vel de Ajuste Autom\u{FFFD}tico da Janela de Rece\u{FFFD}\u{FFFD}o : disabled\r\n\
Capacidade ECN                       : disabled\r\n";

    #[test]
    fn parses_normal_from_english_output() {
        assert_eq!(parse_level(EN_NORMAL), Some(AutoTuningLevel::Normal));
    }

    #[test]
    fn parses_disabled_from_english_output() {
        assert_eq!(parse_level(EN_DISABLED), Some(AutoTuningLevel::Disabled));
    }

    #[test]
    fn falls_back_to_auto_label_for_localized_output() {
        assert_eq!(parse_level(PT_DISABLED), Some(AutoTuningLevel::Disabled));
    }

    #[test]
    fn does_not_confuse_other_disabled_fields() {
        let only_ecn = "ECN Capability : disabled\r\nRFC 1323 Timestamps : allowed\r\n";
        assert_eq!(parse_level(only_ecn), None);
    }

    #[test]
    fn returns_none_for_empty_or_garbage_output() {
        assert_eq!(parse_level(""), None);
        assert_eq!(parse_level("The following command was not found"), None);
    }

    #[test]
    fn keyword_parsing_is_case_insensitive_and_keeps_unknowns() {
        assert_eq!(
            AutoTuningLevel::from_keyword(" HighlyRestricted "),
            AutoTuningLevel::HighlyRestricted
        );
        assert_eq!(
            AutoTuningLevel::from_keyword("weird"),
            AutoTuningLevel::Other("weird".into())
        );
    }

    #[test]
    fn keyword_roundtrips_through_from_keyword() {
        for kw in [
            "disabled",
            "highlyrestricted",
            "restricted",
            "normal",
            "experimental",
            "weird",
        ] {
            assert_eq!(AutoTuningLevel::from_keyword(kw).keyword(), kw);
        }
    }

    #[test]
    fn only_disabled_counts_as_optimized() {
        assert!(AutoTuningLevel::Disabled.is_optimized());
        assert!(!AutoTuningLevel::Normal.is_optimized());
        assert!(!AutoTuningLevel::Restricted.is_optimized());
    }

    #[test]
    fn current_level_runs_show_global() {
        let runner = FakeRunner::new().respond_ok(EN_NORMAL);
        assert_eq!(current_level(&runner).unwrap(), AutoTuningLevel::Normal);
        assert_eq!(
            runner.calls(),
            vec![vec!["netsh", "int", "tcp", "show", "global"]]
        );
    }

    #[test]
    fn current_level_reports_parse_error() {
        let runner = FakeRunner::new().respond_ok("nothing useful");
        assert!(matches!(current_level(&runner), Err(Error::Parse(_))));
    }

    #[test]
    fn apply_disables_autotuning() {
        let runner = FakeRunner::new().respond_ok("Ok.");
        apply(&runner).unwrap();
        assert_eq!(
            runner.calls(),
            vec![vec![
                "netsh",
                "int",
                "tcp",
                "set",
                "global",
                "autotuninglevel=disabled"
            ]]
        );
    }

    #[test]
    fn revert_restores_normal() {
        let runner = FakeRunner::new().respond_ok("Ok.");
        revert(&runner).unwrap();
        assert_eq!(
            runner.calls(),
            vec![vec![
                "netsh",
                "int",
                "tcp",
                "set",
                "global",
                "autotuninglevel=normal"
            ]]
        );
    }

    #[test]
    fn apply_without_elevation_fails() {
        let runner = FakeRunner::new()
            .respond_fail("The requested operation requires elevation (Run as administrator).");
        assert!(matches!(apply(&runner), Err(Error::CommandFailed { .. })));
    }
}
