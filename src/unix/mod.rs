//! Building blocks for Linux and macOS.
//!
//! Everything here is plain Rust (text rendering/parsing and ports over
//! external commands/files) and compiles on every OS, so it is unit-tested
//! on Windows too. Only `elevate::is_root` touches a Unix syscall.

pub mod elevate;
pub mod persist;
pub mod route;
pub mod service;
pub mod sysctl;
