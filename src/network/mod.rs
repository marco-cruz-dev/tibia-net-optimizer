//! Network tuning: TCP auto-tuning (netsh), Nagle (registry) and latency stats.

pub mod adapters;
pub mod autotuning;
#[cfg(windows)]
pub mod iphelper;
pub mod ipv4;
pub mod nagle;
pub mod ping;
#[cfg(windows)]
pub mod registry;
