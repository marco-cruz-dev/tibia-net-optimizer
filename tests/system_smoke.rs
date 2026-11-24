//! Read-only smoke tests against the real machine (no elevation needed).
//! Ignored by default because results depend on the host:
//!
//!     cargo test -- --ignored

#![cfg(windows)]

use tibia_net_optimizer::network::autotuning;
use tibia_net_optimizer::network::nagle::{self, InterfaceRegistry};
use tibia_net_optimizer::network::registry::WindowsInterfaceRegistry;
use tibia_net_optimizer::system::command::SystemCommandRunner;
use tibia_net_optimizer::system::startup;

#[test]
#[ignore]
fn reads_real_autotuning_level() {
    let level = autotuning::current_level(&SystemCommandRunner).expect("netsh should be readable");
    println!("auto-tuning level: {level:?}");
}

#[test]
#[ignore]
fn reads_real_interfaces_from_registry() {
    let ifaces = WindowsInterfaceRegistry
        .interfaces()
        .expect("HKLM Interfaces should be readable");
    assert!(!ifaces.is_empty(), "expected at least one interface subkey");
    let active = nagle::active_adapters(&WindowsInterfaceRegistry).unwrap();
    println!("active adapters: {active:#?}");
}

#[test]
#[ignore]
fn queries_real_startup_task() {
    let registered = startup::is_registered(&SystemCommandRunner).expect("schtasks should run");
    println!("startup task registered: {registered}");
}
