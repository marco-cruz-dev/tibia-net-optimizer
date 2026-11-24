//! CLI entry point: a thin, platform-agnostic shell over the tested library.
//! The tray + egui GUI becomes the default mode in Sprint 4.

use std::process::ExitCode;
use std::time::SystemTime;

use tibia_net_optimizer::cli::{self, Cli, Command, StartupAction};
use tibia_net_optimizer::config::Config;
use tibia_net_optimizer::platform::{self, Os, Platform};
use tibia_net_optimizer::report::{StatusInputs, StatusReport};
use tibia_net_optimizer::system::{boot, elevation};
use tibia_net_optimizer::tweak::TweakState;
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
    let code = run(&cli, &raw);
    if cli.pause {
        println!("\nPress Enter to close...");
        let _ = std::io::stdin().read_line(&mut String::new());
    }
    code
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
        Command::Status { json } => status(&platform, json, elevated),
        Command::Apply => apply(&platform),
        Command::Revert => revert(&platform),
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
    let reboot_recommended = boot::boot_time().is_some_and(|booted| {
        boot::reboot_pending(config.autotuning_changed_at.map(boot::from_unix_secs), booted)
    });
    let report = StatusReport::build(StatusInputs {
        os: platform.os,
        tweaks: platform.tweaks.iter().map(|t| t.check()).collect(),
        startup: platform.startup.is_registered(),
        elevated,
        reboot_recommended,
    });
    println!("{}", if json { report.to_json() } else { report.to_text() });
    Ok(())
}

/// Remembers when a reboot-sensitive tweak changed.
fn record_reboot_sensitive_change() {
    let (mut config, path) = load_config();
    config.autotuning_changed_at = Some(boot::to_unix_secs(SystemTime::now()));
    if let Some(path) = path {
        if let Err(e) = config.save(&path) {
            eprintln!("Warning: could not save config: {e}");
        }
    }
}

/// Runs `op` on every tweak, continuing past failures, then reports the
/// first error (if any).
fn for_each_tweak(platform: &Platform, applying: bool) -> Result<()> {
    let mut first_error = None;
    let mut reboot = false;
    for tweak in &platform.tweaks {
        let before = tweak.check().state;
        let (skip, verb) = if applying {
            (before == TweakState::Optimized, "applied")
        } else {
            (false, "reverted")
        };
        if skip {
            println!("{:<26}: already optimized", tweak.label());
            continue;
        }
        let result = if applying { tweak.apply() } else { tweak.revert() };
        match result {
            Ok(()) => {
                println!("{:<26}: {verb}", tweak.label());
                let changed = if applying {
                    before != TweakState::Optimized
                } else {
                    before == TweakState::Optimized
                };
                reboot |= changed && tweak.reboot_recommended();
            }
            Err(e) => {
                println!("{:<26}: FAILED ({e})", tweak.label());
                first_error.get_or_insert(e);
            }
        }
    }
    if reboot {
        record_reboot_sensitive_change();
        println!("Reboot recommended for the change to take full effect.");
    }
    first_error.map_or(Ok(()), Err)
}

fn apply(platform: &Platform) -> Result<()> {
    for_each_tweak(platform, true)
}

fn revert(platform: &Platform) -> Result<()> {
    for_each_tweak(platform, false)
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
            println!("Run at startup: enabled ({} apply)", exe.display());
        }
        StartupAction::Off => {
            platform.startup.unregister()?;
            println!("Run at startup: disabled");
        }
    }
    Ok(())
}
