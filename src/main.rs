//! Sprint 1 walking skeleton: a CLI over the tested core.
//! The tray + egui GUI replaces the default command in Sprint 3.
//!
//! Usage: tibia-net-optimizer [status|apply|revert]

use std::process::ExitCode;

use tibia_net_optimizer::network::autotuning::{self, AutoTuningLevel};
use tibia_net_optimizer::network::nagle::{self, ActiveAdapter, NagleStatus};
use tibia_net_optimizer::optimizer::{Overall, SystemStatus};
use tibia_net_optimizer::system::command::SystemCommandRunner;
use tibia_net_optimizer::system::startup;
use tibia_net_optimizer::Result;

#[cfg(windows)]
use tibia_net_optimizer::network::registry::WindowsInterfaceRegistry;

fn main() -> ExitCode {
    let cmd = std::env::args().nth(1).unwrap_or_else(|| "status".into());
    let result = match cmd.as_str() {
        "status" => status(),
        "apply" => apply(),
        "revert" => revert(),
        "-h" | "--help" | "help" => {
            println!("Usage: tibia-net-optimizer [status|apply|revert]");
            Ok(())
        }
        other => {
            eprintln!("Unknown command `{other}`. Try --help.");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(windows)]
fn status() -> Result<()> {
    let runner = SystemCommandRunner;
    let level = autotuning::current_level(&runner);
    let adapters = nagle::active_adapters(&WindowsInterfaceRegistry)?;
    let nagle_status = nagle::status_of(&adapters);

    println!("TCP auto-tuning : {}", describe_level(&level));
    println!("Nagle / ACK     : {}", describe_nagle(nagle_status));
    print_adapters(&adapters);
    match startup::is_registered(&runner) {
        Ok(on) => println!(
            "Startup task    : {}",
            if on { "registered" } else { "not registered" }
        ),
        Err(e) => println!("Startup task    : unknown ({e})"),
    }

    let overall = SystemStatus {
        autotuning: level.ok(),
        nagle: nagle_status,
    }
    .overall();
    println!();
    println!(
        "Overall         : {}",
        match overall {
            Overall::Optimized => "OPTIMIZED for Tibia",
            Overall::NeedsFix => "NEEDS FIX — run `tibia-net-optimizer apply` as Administrator",
            Overall::Unknown => "UNKNOWN",
        }
    );
    Ok(())
}

#[cfg(windows)]
fn apply() -> Result<()> {
    autotuning::apply(&SystemCommandRunner)?;
    let patched = nagle::apply(&WindowsInterfaceRegistry)?;
    println!(
        "Auto-tuning disabled; Nagle tuned on {} adapter(s).",
        patched.len()
    );
    println!("A reboot is recommended for auto-tuning changes to fully take effect.");
    Ok(())
}

#[cfg(windows)]
fn revert() -> Result<()> {
    autotuning::revert(&SystemCommandRunner)?;
    let reverted = nagle::revert(&WindowsInterfaceRegistry)?;
    println!(
        "Auto-tuning restored to normal; Nagle values removed from {} interface(s).",
        reverted.len()
    );
    println!("A reboot is recommended for auto-tuning changes to fully take effect.");
    Ok(())
}

#[cfg(not(windows))]
fn status() -> Result<()> {
    unsupported()
}
#[cfg(not(windows))]
fn apply() -> Result<()> {
    unsupported()
}
#[cfg(not(windows))]
fn revert() -> Result<()> {
    unsupported()
}
#[cfg(not(windows))]
fn unsupported() -> Result<()> {
    eprintln!("This tool only modifies Windows network settings.");
    Ok(())
}

#[cfg_attr(not(windows), allow(dead_code))]
fn describe_level(level: &Result<AutoTuningLevel>) -> String {
    match level {
        Ok(l) if l.is_optimized() => "disabled (optimized)".into(),
        Ok(AutoTuningLevel::Other(s)) => format!("{s} (not optimized)"),
        Ok(l) => format!("{} (not optimized)", format!("{l:?}").to_lowercase()),
        Err(e) => format!("unknown ({e})"),
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn describe_nagle(s: NagleStatus) -> &'static str {
    match s {
        NagleStatus::Optimized => "disabled on all active adapters (optimized)",
        NagleStatus::NotOptimized => "active on some adapters (not optimized)",
        NagleStatus::NoActiveAdapter => "no active IPv4 adapter found",
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn print_adapters(adapters: &[ActiveAdapter]) {
    for a in adapters {
        println!(
            "  - {} {:<15} {}",
            a.guid,
            a.ipv4,
            if a.optimized { "[tuned]" } else { "[default]" }
        );
    }
}
