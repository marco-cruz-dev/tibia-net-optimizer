//! CLI entry point: a thin shell over the tested library.
//! The tray + egui GUI becomes the default mode in Sprint 4.

use std::process::ExitCode;

use tibia_net_optimizer::cli;

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let cli = match cli::parse_args(&raw) {
        Ok(cli) => cli,
        Err(msg) => {
            eprintln!("Error: {msg}\n\n{}", cli::USAGE);
            return ExitCode::from(2);
        }
    };
    let code = app::run(&cli, &raw);
    if cli.pause {
        println!("\nPress Enter to close...");
        let _ = std::io::stdin().read_line(&mut String::new());
    }
    code
}

#[cfg(windows)]
mod app {
    use std::process::ExitCode;
    use std::time::SystemTime;

    use tibia_net_optimizer::cli::{self, Cli, Command, StartupAction};
    use tibia_net_optimizer::config::Config;
    use tibia_net_optimizer::network::autotuning;
    use tibia_net_optimizer::network::iphelper::WindowsAdapterSource;
    use tibia_net_optimizer::network::nagle;
    use tibia_net_optimizer::network::registry::WindowsInterfaceRegistry;
    use tibia_net_optimizer::report::{StatusInputs, StatusReport};
    use tibia_net_optimizer::system::command::SystemCommandRunner;
    use tibia_net_optimizer::system::{boot, elevation, startup};
    use tibia_net_optimizer::Result;

    pub fn run(cli: &Cli, raw: &[String]) -> ExitCode {
        let elevated = elevation::is_elevated();

        if cli.command.requires_admin() && !elevated {
            if !cli.elevate {
                eprintln!(
                    "This command changes system settings and needs Administrator rights.\n\
                     Re-run with --elevate, or from an elevated terminal."
                );
                return ExitCode::FAILURE;
            }
            return match elevation::relaunch_elevated(&cli::relaunch_args(raw)) {
                Ok(()) => {
                    println!("Started an elevated window to run the command.");
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
            Command::Status { json } => status(json, elevated),
            Command::Apply => apply(),
            Command::Revert => revert(),
            Command::Startup(action) => startup_cmd(action),
        };
        match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => report_error(e),
        }
    }

    fn report_error(e: tibia_net_optimizer::Error) -> ExitCode {
        eprintln!("Error: {e}");
        ExitCode::FAILURE
    }

    fn load_config() -> (Config, Option<std::path::PathBuf>) {
        let path = Config::default_path();
        let config = path.as_deref().map(Config::load).unwrap_or_default();
        (config, path)
    }

    fn status(json: bool, elevated: bool) -> Result<()> {
        let runner = SystemCommandRunner;
        let autotuning = autotuning::current_level(&runner);
        let adapters = nagle::active_adapters(&WindowsInterfaceRegistry, &WindowsAdapterSource)?;
        let startup_task = startup::is_registered(&runner);
        let (config, _) = load_config();
        let reboot_recommended = boot::boot_time().is_some_and(|booted| {
            boot::reboot_pending(config.autotuning_changed_at.map(boot::from_unix_secs), booted)
        });

        let report = StatusReport::build(StatusInputs {
            autotuning: &autotuning,
            adapters: &adapters,
            startup_task: &startup_task,
            elevated,
            reboot_recommended,
        });
        println!("{}", if json { report.to_json() } else { report.to_text() });
        Ok(())
    }

    /// Sets auto-tuning only if it differs, recording the change time so a
    /// reboot can be recommended.
    fn set_autotuning(want_disabled: bool) -> Result<bool> {
        let runner = SystemCommandRunner;
        let already = autotuning::current_level(&runner)
            .map(|l| l.is_optimized() == want_disabled)
            .unwrap_or(false);
        if already {
            return Ok(false);
        }
        if want_disabled {
            autotuning::apply(&runner)?;
        } else {
            autotuning::revert(&runner)?;
        }
        let (mut config, path) = load_config();
        config.autotuning_changed_at = Some(boot::to_unix_secs(SystemTime::now()));
        if let Some(path) = path {
            if let Err(e) = config.save(&path) {
                eprintln!("Warning: could not save config: {e}");
            }
        }
        Ok(true)
    }

    fn apply() -> Result<()> {
        let changed = set_autotuning(true)?;
        let patched = nagle::apply(&WindowsInterfaceRegistry, &WindowsAdapterSource)?;
        println!(
            "Auto-tuning: {}",
            if changed { "disabled" } else { "already disabled" }
        );
        println!("Nagle / ACK: tuned {} adapter(s)", patched.len());
        if changed {
            println!("Reboot recommended for the auto-tuning change to take full effect.");
        }
        Ok(())
    }

    fn revert() -> Result<()> {
        let changed = set_autotuning(false)?;
        let reverted = nagle::revert(&WindowsInterfaceRegistry)?;
        println!(
            "Auto-tuning: {}",
            if changed {
                "restored to normal"
            } else {
                "already normal"
            }
        );
        println!("Nagle / ACK: removed tuning from {} interface(s)", reverted.len());
        if changed {
            println!("Reboot recommended for the auto-tuning change to take full effect.");
        }
        Ok(())
    }

    fn startup_cmd(action: StartupAction) -> Result<()> {
        let runner = SystemCommandRunner;
        match action {
            StartupAction::Status => {
                let on = startup::is_registered(&runner)?;
                println!(
                    "Startup task: {}",
                    if on { "registered" } else { "not registered" }
                );
            }
            StartupAction::On => {
                let exe = std::env::current_exe()?;
                startup::register(&runner, &exe)?;
                println!("Startup task registered: {}", startup::task_command(&exe));
            }
            StartupAction::Off => {
                startup::unregister(&runner)?;
                println!("Startup task removed.");
            }
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod app {
    use std::process::ExitCode;
    use tibia_net_optimizer::cli::Cli;

    pub fn run(_cli: &Cli, _raw: &[String]) -> ExitCode {
        eprintln!("Linux/macOS support arrives in Sprint 3; this build only supports Windows.");
        ExitCode::FAILURE
    }
}
