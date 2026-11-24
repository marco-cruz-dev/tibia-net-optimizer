//! Read-only smoke tests against the real machine (no elevation needed).
//! Ignored by default because results depend on the host:
//!
//!     cargo test -- --ignored

#![cfg(windows)]

use tibia_net_optimizer::network::adapters::AdapterSource;
use tibia_net_optimizer::network::autotuning;
use tibia_net_optimizer::network::iphelper::WindowsAdapterSource;
use tibia_net_optimizer::network::nagle::{self, InterfaceRegistry};
use tibia_net_optimizer::network::registry::WindowsInterfaceRegistry;
use tibia_net_optimizer::system::command::SystemCommandRunner;
use tibia_net_optimizer::system::{boot, elevation, startup};

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
}

#[test]
#[ignore]
fn lists_real_os_adapters() {
    let adapters = WindowsAdapterSource
        .adapters()
        .expect("GetAdaptersAddresses should succeed");
    assert!(
        !adapters.is_empty(),
        "expected at least one adapter (loopback at minimum)"
    );
    for a in &adapters {
        assert!(
            a.guid.starts_with('{') && a.guid.ends_with('}'),
            "GUID format: {}",
            a.guid
        );
    }
    println!("OS adapters: {adapters:#?}");
}

#[test]
#[ignore]
fn every_active_adapter_is_up_according_to_the_os() {
    let os = WindowsAdapterSource.adapters().unwrap();
    let active = nagle::active_adapters(&WindowsInterfaceRegistry, &WindowsAdapterSource).unwrap();
    for a in &active {
        let live = os
            .iter()
            .find(|o| o.guid.eq_ignore_ascii_case(&a.guid))
            .expect("known to OS");
        assert!(live.is_up, "{} is reported active but is down", a.guid);
        assert!(a.name.is_some());
    }
    println!("active adapters: {active:#?}");
}

#[test]
#[ignore]
fn queries_real_startup_task() {
    let registered = startup::is_registered(&SystemCommandRunner).expect("schtasks should run");
    println!("startup task registered: {registered}");
}

#[test]
#[ignore]
fn elevation_and_boot_time_are_readable() {
    println!("elevated: {}", elevation::is_elevated());
    let booted = boot::boot_time().expect("boot time");
    assert!(booted < std::time::SystemTime::now());
}
