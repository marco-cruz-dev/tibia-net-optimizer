//! Read-only smoke tests against a real Linux/macOS machine (no root needed).
//! Ignored by default because results depend on the host:
//!
//!     cargo test -- --ignored

#![cfg(unix)]

use tibia_net_optimizer::platform;
use tibia_net_optimizer::system::command::SystemCommandRunner;
use tibia_net_optimizer::tweak::{TweakId, TweakState};
use tibia_net_optimizer::unix::sysctl::{CommandSysctl, Sysctl};

#[test]
#[ignore]
fn sysctl_tweaks_are_readable() {
    let p = platform::current();
    for t in &p.tweaks {
        let s = t.check();
        println!("{s:#?}");
        // Quick ACK can legitimately be Unknown on hosts without a default route.
        if s.id != TweakId::LinuxQuickAck {
            assert_ne!(s.state, TweakState::Unknown, "{}: {}", s.label, s.summary);
        }
    }
}

#[test]
#[ignore]
fn reads_a_well_known_sysctl_key() {
    let key = if cfg!(target_os = "macos") {
        "kern.ostype"
    } else {
        "kernel.ostype"
    };
    let value = CommandSysctl {
        runner: SystemCommandRunner,
    }
    .read(key)
    .expect("sysctl should be readable");
    assert!(!value.is_empty());
}

/// Needs internet access: TCP-connect probe to the default Frankfurt target.
#[test]
#[ignore]
fn probes_the_default_target() {
    use tibia_net_optimizer::network::probe::{parse_target, probe_once, DEFAULT_TARGET};
    let target = parse_target(DEFAULT_TARGET).unwrap();
    let rtt = probe_once(&target, std::time::Duration::from_secs(3)).expect("default target should answer");
    println!("{DEFAULT_TARGET}: {rtt:?}");
}

#[test]
#[ignore]
fn startup_state_is_queryable() {
    let registered = platform::current()
        .startup
        .is_registered()
        .expect("startup query");
    println!("run at startup: {registered}");
}
