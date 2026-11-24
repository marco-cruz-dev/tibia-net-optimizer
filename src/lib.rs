//! Tibia Network Optimizer — core library.
//!
//! Architecture (ports & adapters):
//! - Pure domain logic (parsing, status evaluation, statistics, healing
//!   decisions) is fully unit-tested and has no OS side effects.
//! - Side effects go through small traits ("ports"):
//!   [`system::command::CommandRunner`] for `netsh`/`schtasks`, and
//!   [`network::nagle::InterfaceRegistry`] for the HKLM registry.
//! - Real Windows implementations ("adapters") are thin and covered by
//!   `#[ignore]`d smoke tests in `tests/`.

pub mod cli;
pub mod config;
pub mod error;
pub mod network;
pub mod optimizer;
pub mod report;
pub mod system;

pub use error::{Error, Result};
