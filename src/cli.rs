//! Command-line parsing (pure, unit-tested).
//!
//! ```text
//! tibia-net-optimizer [status [--json] | apply | revert | startup [on|off|status]]
//!                     [--elevate] [--pause] [--minimized] [-h|--help]
//! ```

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupAction {
    On,
    Off,
    Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Status { json: bool },
    Apply,
    Revert,
    Startup(StartupAction),
    Help,
}

impl Command {
    /// Commands that modify system settings.
    pub fn requires_admin(&self) -> bool {
        matches!(
            self,
            Command::Apply | Command::Revert | Command::Startup(StartupAction::On | StartupAction::Off)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cli {
    pub command: Command,
    /// Relaunch elevated (UAC) if not already Administrator.
    pub elevate: bool,
    /// Wait for Enter before exiting (keeps an elevated console window open).
    pub pause: bool,
}

pub const USAGE: &str = "\
Usage: tibia-net-optimizer [COMMAND] [OPTIONS]

Commands:
  status [--json]          Show current optimization state (default, read-only)
  apply                    Disable auto-tuning and Nagle on active adapters (admin)
  revert                   Restore Windows defaults (admin)
  startup [on|off|status]  Run elevated at logon via Task Scheduler (admin for on/off)

Options:
  --elevate                Relaunch as Administrator (UAC prompt) if needed
  --pause                  Wait for Enter before exiting
  -h, --help               Show this help";

/// Parses arguments (without the program name).
pub fn parse_args<I, S>(args: I) -> Result<Cli, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let (mut elevate, mut pause, mut json, mut help) = (false, false, false, false);
    let mut positional = Vec::new();
    for arg in args {
        match arg.as_ref() {
            "--elevate" => elevate = true,
            "--pause" => pause = true,
            "--json" => json = true,
            "--minimized" => {}
            "-h" | "--help" => help = true,
            flag if flag.starts_with('-') => return Err(format!("unknown option `{flag}`")),
            word => positional.push(word.to_string()),
        }
    }

    let words: Vec<&str> = positional.iter().map(String::as_str).collect();
    let command = if help {
        Command::Help
    } else {
        match words.as_slice() {
            [] | ["status"] => Command::Status { json },
            ["apply"] => Command::Apply,
            ["revert"] => Command::Revert,
            ["help"] => Command::Help,
            ["startup"] | ["startup", "status"] => Command::Startup(StartupAction::Status),
            ["startup", "on"] => Command::Startup(StartupAction::On),
            ["startup", "off"] => Command::Startup(StartupAction::Off),
            ["startup", other] => {
                return Err(format!("unknown startup action `{other}` (use on|off|status)"))
            }
            [first, ..] if words.len() == 1 => return Err(format!("unknown command `{first}`")),
            _ => return Err(format!("unexpected arguments: {}", words.join(" "))),
        }
    };

    if json && !matches!(command, Command::Status { .. } | Command::Help) {
        return Err("--json is only valid with `status`".into());
    }
    Ok(Cli {
        command,
        elevate,
        pause,
    })
}

/// Arguments for the elevated child: same command, without `--elevate`
/// (no relaunch loop) and with `--pause` (so its console output stays visible).
pub fn relaunch_args(args: &[String]) -> Vec<String> {
    let mut out: Vec<String> = args.iter().filter(|a| *a != "--elevate").cloned().collect();
    if !out.iter().any(|a| a == "--pause") {
        out.push("--pause".into());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, String> {
        parse_args(args.iter().copied())
    }

    fn cmd(args: &[&str]) -> Command {
        parse(args).unwrap().command
    }

    #[test]
    fn defaults_to_status() {
        assert_eq!(
            parse(&[]).unwrap(),
            Cli {
                command: Command::Status { json: false },
                elevate: false,
                pause: false
            }
        );
    }

    #[test]
    fn parses_basic_commands() {
        assert_eq!(cmd(&["status"]), Command::Status { json: false });
        assert_eq!(cmd(&["status", "--json"]), Command::Status { json: true });
        assert_eq!(cmd(&["--json"]), Command::Status { json: true });
        assert_eq!(cmd(&["apply"]), Command::Apply);
        assert_eq!(cmd(&["revert"]), Command::Revert);
        assert_eq!(cmd(&["help"]), Command::Help);
        assert_eq!(cmd(&["-h"]), Command::Help);
        assert_eq!(cmd(&["apply", "--help"]), Command::Help);
    }

    #[test]
    fn parses_startup_subcommands() {
        assert_eq!(cmd(&["startup"]), Command::Startup(StartupAction::Status));
        assert_eq!(
            cmd(&["startup", "status"]),
            Command::Startup(StartupAction::Status)
        );
        assert_eq!(cmd(&["startup", "on"]), Command::Startup(StartupAction::On));
        assert_eq!(cmd(&["startup", "off"]), Command::Startup(StartupAction::Off));
    }

    #[test]
    fn flags_can_appear_anywhere() {
        let cli = parse(&["--elevate", "apply", "--pause"]).unwrap();
        assert_eq!(cli.command, Command::Apply);
        assert!(cli.elevate && cli.pause);
    }

    #[test]
    fn minimized_flag_is_accepted() {
        // Passed by the Task Scheduler entry; meaningful once the GUI exists.
        assert_eq!(cmd(&["--minimized"]), Command::Status { json: false });
    }

    #[test]
    fn rejects_unknown_input() {
        assert!(parse(&["frobnicate"]).is_err());
        assert!(parse(&["--frobnicate"]).is_err());
        assert!(parse(&["startup", "maybe"]).is_err());
        assert!(parse(&["apply", "extra"]).is_err());
        assert!(parse(&["status", "apply"]).is_err());
    }

    #[test]
    fn json_only_valid_for_status() {
        let err = parse(&["apply", "--json"]).unwrap_err();
        assert!(err.contains("--json"), "{err}");
    }

    #[test]
    fn admin_required_only_for_mutating_commands() {
        assert!(Command::Apply.requires_admin());
        assert!(Command::Revert.requires_admin());
        assert!(Command::Startup(StartupAction::On).requires_admin());
        assert!(Command::Startup(StartupAction::Off).requires_admin());
        assert!(!Command::Startup(StartupAction::Status).requires_admin());
        assert!(!Command::Status { json: false }.requires_admin());
        assert!(!Command::Help.requires_admin());
    }

    #[test]
    fn relaunch_args_drop_elevate_and_add_pause_once() {
        let args: Vec<String> = ["apply", "--elevate"].map(String::from).to_vec();
        assert_eq!(relaunch_args(&args), vec!["apply", "--pause"]);
        let args: Vec<String> = ["--pause", "revert", "--elevate"].map(String::from).to_vec();
        assert_eq!(relaunch_args(&args), vec!["--pause", "revert"]);
    }
}
