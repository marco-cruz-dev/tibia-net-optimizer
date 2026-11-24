//! Network tuning: TCP auto-tuning (netsh), Nagle (registry) and latency stats.

pub mod autotuning;
pub mod ipv4;
pub mod nagle;
pub mod ping;
#[cfg(windows)]
pub mod registry;
