//! Shared entry point for both binaries:
//!
//! - `tibia-net-optimizer` — the dashboard + tray. On Windows it uses the GUI
//!   subsystem (no console window flashes at login); CLI arguments still work
//!   by attaching to the parent terminal.
//! - `tno` — the command line (console subsystem). With no arguments it
//!   prints the status.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::SystemTime;

use crate::actions::{run_tweaks, Outcome};
use crate::cli::{self, Cli, Command, StartupAction};
use crate::config::Config;
use crate::platform::{self, Os, Platform};
use crate::report;
use crate::system::{boot, elevation};
use crate::tweak::TweakId;
use crate::{Error, Result};

pub const GUI_BIN: &str = "tibia-net-optimizer";
pub const CLI_BIN: &str = "tno";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Gui,
    Cli,
}

/// `tno` never opens the dashboard: no command means "status".
pub fn effective_command(mode: Mode, command: Command) -> Command {
    match (mode, command) {
        (Mode::Cli, Command::Gui { .. }) => Command::Status { json: false },
        (_, command) => command,
    }
}

/// `dir/<stem><same extension>` next to `current`.
pub fn sibling(current: &Path, stem: &str) -> PathBuf {
    let mut name = std::ffi::OsString::from(stem);
    if let Some(ext) = current.extension() {
        name.push(".");
        name.push(ext);
    }
    current.with_file_name(name)
}

fn stem_is(path: &Path, stem: &str) -> bool {
    path.file_stem().is_some_and(|s| s == stem)
}

/// `current`'s sibling `target_stem` when running as `from_stem` and the
/// sibling exists; otherwise `current`.
fn swap_binary(
    current: &Path,
    from_stem: &str,
    target_stem: &str,
    exists: impl Fn(&Path) -> bool,
) -> PathBuf {
    if stem_is(current, from_stem) {
        let other = sibling(current, target_stem);
        if exists(&other) {
            return other;
        }
    }
    current.to_path_buf()
}

/// What the startup entry launches: always the dashboard exe (the logon
/// task runs it with `--minimized`), even when registered from `tno`.
pub fn startup_target(current: &Path, exists: impl Fn(&Path) -> bool) -> PathBuf {
    swap_binary(current, CLI_BIN, GUI_BIN, exists)
}

/// What `--elevate` relaunches for a CLI command: `tno`, because an elevated
/// GUI-subsystem process would have no console to print to.
pub fn elevated_cli_target(current: &Path, exists: impl Fn(&Path) -> bool) -> PathBuf {
    swap_binary(current, GUI_BIN, CLI_BIN, exists)
}

/// Runs the app with the process arguments.
pub fn main(mode: Mode) -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let parsed = cli::parse_args(&raw);
    // The GUI exe has no console of its own: borrow the terminal for CLI use.
    let wants_terminal = !matches!(
        parsed,
        Ok(Cli {
            command: Command::Gui { .. },
            ..
        })
    );
    if mode == Mode::Gui && wants_terminal {
        crate::system::console::attach_parent_console();
    }
    let mut cli = match parsed {
        Ok(cli) => cli,
        Err(msg) => {
            eprintln!("Error: {msg}\n\n{}", cli::USAGE);
            return ExitCode::from(2);
        }
    };
    cli.command = effective_command(mode, cli.command);
    if let Command::Gui { minimized } = cli.command {
        return gui(minimized);
    }
    let code = run(&cli, &raw);
    if cli.pause {
        println!("\nPress Enter to close...");
        let _ = std::io::stdin().read_line(&mut String::new());
    }
    code
}

#[cfg(feature = "gui")]
fn gui(minimized: bool) -> ExitCode {
    match crate::ui::run(minimized) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: could not start the dashboard: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(feature = "gui"))]
fn gui(_minimized: bool) -> ExitCode {
    eprintln!("This build has no GUI (built without the `gui` feature). Showing status instead.\n");
    run(
        &Cli {
            command: Command::Status { json: false },
            elevate: false,
            pause: false,
        },
        &[],
    )
}

fn current_exe() -> Result<PathBuf> {
    Ok(std::env::current_exe()?)
}

fn run(cli: &Cli, raw: &[String]) -> ExitCode {
    let platform = platform::current();
    let elevated = elevation::is_elevated();

    if cli.command.requires_admin() && !elevated {
        if !cli.elevate {
            let alternative = match platform.os {
                Os::Windows => "from an Administrator terminal",
                Os::Linux | Os::Macos => "with sudo",
            };
            eprintln!(
                "This command changes system settings and needs {} rights.\n\
                 Re-run with --elevate, or {alternative}.",
                platform.os.admin_name()
            );
            return ExitCode::FAILURE;
        }
        let windows = platform.os == Os::Windows;
        let exe = match current_exe() {
            Ok(exe) => elevated_cli_target(&exe, |p| p.exists()),
            Err(e) => return report_error(e),
        };
        return match elevation::relaunch_elevated(&exe, &cli::relaunch_args(raw, windows)) {
            Ok(()) => {
                if windows {
                    println!("Started an elevated window to run the command.");
                }
                ExitCode::SUCCESS
            }
            Err(e) => report_error(e),
        };
    }

    let result = match cli.command {
        Command::Help => {
            println!("{}", cli::USAGE);
            Ok(())
        }
        Command::Gui { .. } => unreachable!("handled in main"),
        Command::Status { json } => status(&platform, json, elevated),
        Command::Apply { only } => change(&platform, true, only),
        Command::Revert { only } => change(&platform, false, only),
        Command::Startup(action) => startup_cmd(&platform, action),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => report_error(e),
    }
}

fn report_error(e: Error) -> ExitCode {
    eprintln!("Error: {e}");
    ExitCode::FAILURE
}

fn load_config() -> (Config, Option<PathBuf>) {
    let path = Config::default_path();
    let config = path.as_deref().map(Config::load).unwrap_or_default();
    (config, path)
}

fn status(platform: &Platform, json: bool, elevated: bool) -> Result<()> {
    let (config, _) = load_config();
    let report = report::collect(platform, elevated, config.autotuning_changed_at);
    println!("{}", if json { report.to_json() } else { report.to_text() });
    Ok(())
}

/// Applies or reverts all tweaks (or one), printing a line per tweak.
fn change(platform: &Platform, applying: bool, only: Option<TweakId>) -> Result<()> {
    let summary = run_tweaks(&platform.tweaks, applying, only);
    if summary.outcomes.is_empty() {
        if let Some(id) = only {
            return Err(Error::Parse(format!(
                "setting `{}` is not available on this OS",
                id.key()
            )));
        }
    }
    for o in &summary.outcomes {
        let text = match &o.outcome {
            Outcome::Applied => "applied".to_string(),
            Outcome::Reverted => "reverted".to_string(),
            Outcome::AlreadyOptimized => "already optimized".to_string(),
            Outcome::Failed(e) => format!("FAILED ({e})"),
        };
        println!("{:<26}: {text}", o.label);
    }
    if summary.reboot_recommended {
        let (mut config, path) = load_config();
        config.autotuning_changed_at = Some(boot::to_unix_secs(SystemTime::now()));
        if let Some(path) = path {
            if let Err(e) = config.save(&path) {
                eprintln!("Warning: could not save config: {e}");
            }
        }
        println!("Reboot recommended for the change to take full effect.");
    }
    match summary.first_error() {
        Some(e) => Err(Error::CommandFailed {
            program: CLI_BIN.into(),
            message: e,
        }),
        None => Ok(()),
    }
}

fn startup_cmd(platform: &Platform, action: StartupAction) -> Result<()> {
    match action {
        StartupAction::Status => {
            let on = platform.startup.is_registered()?;
            println!("Run at startup: {}", if on { "yes" } else { "no" });
        }
        StartupAction::On => {
            let exe = startup_target(&current_exe()?, |p| p.exists());
            platform.startup.register(&exe)?;
            println!("Run at startup: enabled ({})", exe.display());
        }
        StartupAction::Off => {
            platform.startup.unregister()?;
            println!("Run at startup: disabled");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        PathBuf::from("app")
    }

    #[test]
    fn cli_binary_never_opens_the_dashboard() {
        assert_eq!(
            effective_command(Mode::Cli, Command::Gui { minimized: false }),
            Command::Status { json: false }
        );
        assert_eq!(
            effective_command(Mode::Cli, Command::Gui { minimized: true }),
            Command::Status { json: false }
        );
        assert_eq!(
            effective_command(Mode::Gui, Command::Gui { minimized: true }),
            Command::Gui { minimized: true }
        );
        assert_eq!(
            effective_command(Mode::Cli, Command::Apply { only: None }),
            Command::Apply { only: None }
        );
    }

    #[test]
    fn sibling_keeps_directory_and_extension() {
        assert_eq!(
            sibling(&dir().join("tno.exe"), GUI_BIN),
            dir().join("tibia-net-optimizer.exe")
        );
        assert_eq!(
            sibling(&dir().join("tibia-net-optimizer"), CLI_BIN),
            dir().join("tno")
        );
    }

    #[test]
    fn startup_always_targets_the_dashboard() {
        let gui = dir().join("tibia-net-optimizer.exe");
        let tno = dir().join("tno.exe");
        assert_eq!(startup_target(&tno, |p| p == gui.as_path()), gui, "from tno");
        assert_eq!(startup_target(&gui, |_| true), gui, "from the dashboard");
        assert_eq!(
            startup_target(&tno, |_| false),
            tno,
            "falls back when the dashboard is missing"
        );
    }

    #[test]
    fn elevated_cli_commands_use_tno() {
        let gui = dir().join("tibia-net-optimizer.exe");
        let tno = dir().join("tno.exe");
        assert_eq!(elevated_cli_target(&gui, |p| p == tno.as_path()), tno);
        assert_eq!(elevated_cli_target(&tno, |_| true), tno);
        assert_eq!(
            elevated_cli_target(&gui, |_| false),
            gui,
            "falls back when tno is missing"
        );
    }
}
