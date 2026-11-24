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

#[test]
#[ignore]
fn icmpv6_echo_to_loopback() {
    let rtt =
        tibia_net_optimizer::network::icmp::echo6(std::net::Ipv6Addr::LOCALHOST, Duration::from_secs(1))
            .expect("::1 should answer ICMPv6");
    assert!(rtt < Duration::from_secs(1));
}

/// Reads the real TCP table. Prints the Tibia game target if the client is
/// connected (it isn't on CI).
#[test]
#[ignore]
fn reads_the_tcp_table_and_detects_tibia_if_running() {
    use tibia_net_optimizer::network::conntable::WindowsConnections;
    use tibia_net_optimizer::network::gameconn::{detect_game_targets, ConnectionSource};
    let conns = WindowsConnections
        .established()
        .expect("TCP table should be readable");
    assert!(conns.iter().all(|c| c.remote.port() != 0));
    println!("{} established connections", conns.len());
    for c in conns.iter().filter(|c| !c.remote.ip().is_loopback()) {
        println!("  conn pid={} {:<28} {}", c.pid, c.process, c.remote);
    }
    println!(
        "Tibia game target: {:?}",
        detect_game_targets(&WindowsConnections)
    );
}

/// While Tibia is connected: list every game connection, ICMP each once,
/// then run the app's real `GameProbe` for a few ticks. Skips when Tibia
/// isn't running.
#[test]
#[ignore]
fn monitors_the_tibia_connection_if_connected() {
    use tibia_net_optimizer::app::ping::GameProbe;
    use tibia_net_optimizer::network::conntable::WindowsConnections;
    use tibia_net_optimizer::network::gameconn::detect_game_targets;
    use tibia_net_optimizer::network::probe::icmp_echo;
    let candidates = detect_game_targets(&WindowsConnections);
    if candidates.is_empty() {
        eprintln!("skipped: Tibia is not connected");
        return;
    }
    for ip in &candidates {
        println!("candidate {ip}: {:?}", icmp_echo(*ip, Duration::from_secs(1)));
    }
    let mut probe = GameProbe::new(
        || detect_game_targets(&WindowsConnections),
        |ip| icmp_echo(ip, Duration::from_secs(1)),
    );
    for _ in 0..4 {
        println!("tick: {:?}", probe.tick());
    }
}

/// ICMP to a given address with the app's prober (manual QA):
/// `$env:TNO_PING_IP="2606:4700:..."; cargo test --test system_smoke -- --ignored pings_ip_from_env --nocapture`
#[test]
#[ignore]
fn pings_ip_from_env() {
    let Ok(ip) = std::env::var("TNO_PING_IP") else {
        eprintln!("skipped: set TNO_PING_IP");
        return;
    };
    let ip: std::net::IpAddr = ip.parse().expect("TNO_PING_IP must be an IP address");
    let rtts: Vec<_> = (0..5)
        .map(|_| tibia_net_optimizer::network::probe::icmp_echo(ip, Duration::from_secs(1)))
        .collect();
    println!("{ip}: {rtts:?}");
    assert!(rtts.iter().any(|r| r.is_ok()));
}

#[test]
#[ignore]
fn elevation_and_boot_time_are_readable() {
    println!("elevated: {}", elevation::is_elevated());
    let booted = boot::boot_time().expect("boot time");
    assert!(booted < std::time::SystemTime::now());
}
