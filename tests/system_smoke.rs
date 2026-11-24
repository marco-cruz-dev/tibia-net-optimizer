//! Read-only smoke tests against the real machine (no elevation needed).
//! Ignored by default because results depend on the host:
//!
//!     cargo test -- --ignored

#![cfg(windows)]

use std::time::Duration;

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

/// Shows a real toast, so it only runs when asked explicitly:
/// `$env:TNO_TOAST_TEST=1; cargo test --test system_smoke -- --ignored shows_a_toast`
#[test]
#[ignore]
fn shows_a_toast() {
    if std::env::var("TNO_TOAST_TEST").is_err() {
        eprintln!("skipped: set TNO_TOAST_TEST=1");
        return;
    }
    tibia_net_optimizer::system::notify::send(
        "Tibia Network Optimizer",
        "Test notification from the smoke tests - you can dismiss it.",
    )
    .expect("toast should be shown");
}

#[test]
#[ignore]
fn icmp_echo_to_loopback() {
    let rtt = tibia_net_optimizer::network::icmp::echo(std::net::Ipv4Addr::LOCALHOST, Duration::from_secs(1))
        .expect("loopback should answer ICMP");
    assert!(rtt < Duration::from_secs(1));
}

/// Needs internet access: probes the default Frankfurt target.
#[test]
#[ignore]
fn probes_the_default_target() {
    use tibia_net_optimizer::network::probe::{parse_target, probe_once, DEFAULT_TARGET};
    let target = parse_target(DEFAULT_TARGET).unwrap();
    let rtt = probe_once(&target, Duration::from_secs(2)).expect("default target should answer");
    println!("{DEFAULT_TARGET}: {rtt:?}");
}

#[test]
#[ignore]
fn elevation_and_boot_time_are_readable() {
    println!("elevated: {}", elevation::is_elevated());
    let booted = boot::boot_time().expect("boot time");
    assert!(booted < std::time::SystemTime::now());
}
