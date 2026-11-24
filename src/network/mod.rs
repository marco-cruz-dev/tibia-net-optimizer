//! Network tuning: TCP auto-tuning (netsh), Nagle (registry) and latency stats.

pub mod adapters;
pub mod autotuning;
#[cfg(windows)]
pub mod conntable;
pub mod gameconn;
#[cfg(windows)]
pub mod icmp;
#[cfg(windows)]
pub mod iphelper;
pub mod ipv4;
pub mod nagle;
pub mod ping;
pub mod probe;
#[cfg(windows)]
pub mod registry;
