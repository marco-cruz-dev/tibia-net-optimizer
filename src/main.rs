//! Entry point: the dashboard/tray app by default, or a CLI command.
//! A thin shell over the tested library.

use std::process::ExitCode;
use std::time::SystemTime;

use tibia_net_optimizer::actions::{run_tweaks, Outcome};
use tibia_net_optimizer::cli::{self, Cli, Command, StartupAction};
use tibia_net_optimizer::config::Config;
use tibia_net_optimizer::platform::{self, Os, Platform};
use tibia_net_optimizer::report;
use tibia_net_optimizer::system::{boot, elevation};
use tibia_net_optimizer::tweak::TweakId;
use tibia_net_optimizer::{Error, Result};

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let cli = match cli::parse_args(&raw) {
        Ok(cli) => cli,
        Err(msg) => {
            eprintln!("Error: {msg}\n\n{}", cli::USAGE);
            return ExitCode::from(2);
        }
    };
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
    tibia_net_optimizer::system::console::detach_if_owned();
    match tibia_net_optimizer::ui::run(minimized) {
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
        return match elevation::relaunch_elevated(&cli::relaunch_args(raw, windows)) {
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

fn load_config() -> (Config, Option<std::path::PathBuf>) {
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
            program: "tibia-net-optimizer".into(),
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
            let exe = std::env::current_exe()?;
            platform.startup.register(&exe)?;
            println!("Run at startup: enabled");
        }
        StartupAction::Off => {
            platform.startup.unregister()?;
            println!("Run at startup: disabled");
        }
    }
    Ok(())
}
