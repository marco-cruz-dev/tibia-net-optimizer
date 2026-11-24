//! Per-OS assembly of tweaks and startup integration.
//!
//! `linux` and `macos` are compiled on every OS (they only wire concrete
//! adapters together), so their composition is unit-tested even on Windows.
//! `windows` needs the Win32 adapters and is Windows-only.

use serde::Serialize;

use crate::system::startup::StartupManager;
use crate::tweak::Tweak;

pub mod linux;
pub mod macos;
#[cfg(windows)]
pub mod windows;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Windows,
    Linux,
    Macos,
}

impl Os {
    /// The OS this binary was compiled for (other Unixes are treated as Linux).
    pub fn current() -> Os {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::Macos
        } else {
            Os::Linux
        }
    }

    /// What the elevated account is called on this OS.
    /// Lowercase name, same as the JSON form: `windows`, `linux`, `macos`.
    pub fn key(&self) -> &'static str {
        match self {
            Os::Windows => "windows",
            Os::Linux => "linux",
            Os::Macos => "macos",
        }
    }

    pub fn admin_name(&self) -> &'static str {
        match self {
            Os::Windows => "Administrator",
            Os::Linux | Os::Macos => "root",
        }
    }
}

pub struct Platform {
    pub os: Os,
    pub tweaks: Vec<Box<dyn Tweak>>,
    pub startup: Box<dyn StartupManager>,
}

/// The platform for the running OS.
pub fn current() -> Platform {
    #[cfg(windows)]
    return windows::platform();
    #[cfg(target_os = "macos")]
    return macos::platform();
    #[cfg(all(unix, not(target_os = "macos")))]
    return linux::platform();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_matches_compile_target() {
        #[cfg(windows)]
        assert_eq!(Os::current(), Os::Windows);
        #[cfg(target_os = "macos")]
        assert_eq!(Os::current(), Os::Macos);
        #[cfg(target_os = "linux")]
        assert_eq!(Os::current(), Os::Linux);
    }

    #[test]
    fn admin_names() {
        assert_eq!(Os::Windows.admin_name(), "Administrator");
        assert_eq!(Os::Linux.admin_name(), "root");
        assert_eq!(Os::Macos.admin_name(), "root");
    }
}
