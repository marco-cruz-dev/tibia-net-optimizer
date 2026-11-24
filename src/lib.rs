//! Tibia Network Optimizer — core library.
//!
//! Architecture (ports & adapters):
//! - [`tweak::Tweak`] is the generic unit of optimization (check / apply /
//!   revert). [`platform`] assembles the tweaks for Windows, Linux or macOS.
//! - Pure logic (parsing, rendering, status, statistics, healing decisions)
//!   is unit-tested with no OS side effects, including the Linux/macOS code,
//!   which compiles and is tested on every OS.
//! - Side effects go through small traits ("ports"): `CommandRunner`,
//!   `InterfaceRegistry`, `AdapterSource`, `Sysctl`, `SettingsStore`,
//!   `StartupManager`. Real adapters are thin and covered by `#[ignore]`d
//!   read-only smoke tests in `tests/`.

pub mod cli;
pub mod config;
pub mod error;
pub mod network;
pub mod optimizer;
pub mod platform;
pub mod report;
pub mod system;
pub mod tweak;
pub mod unix;

pub use error::{Error, Result};
