//! Boot time and "reboot recommended" tracking for auto-tuning changes.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A reboot is recommended when auto-tuning was changed after the last boot.
pub fn reboot_pending(changed_at: Option<SystemTime>, booted_at: SystemTime) -> bool {
    changed_at.is_some_and(|c| c > booted_at)
}

pub fn to_unix_secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn from_unix_secs(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

/// When the machine last booted (now − uptime).
#[cfg(windows)]
pub fn boot_time() -> Option<SystemTime> {
    // SAFETY: GetTickCount64 has no preconditions.
    let uptime_ms = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() };
    SystemTime::now().checked_sub(Duration::from_millis(uptime_ms))
}

#[cfg(not(windows))]
pub fn boot_time() -> Option<SystemTime> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(secs: u64) -> SystemTime {
        from_unix_secs(secs)
    }

    #[test]
    fn no_change_recorded_means_no_reboot() {
        assert!(!reboot_pending(None, t(1_000)));
    }

    #[test]
    fn change_before_boot_is_already_applied() {
        assert!(!reboot_pending(Some(t(900)), t(1_000)));
    }

    #[test]
    fn change_after_boot_needs_reboot() {
        assert!(reboot_pending(Some(t(1_100)), t(1_000)));
    }

    #[test]
    fn unix_secs_roundtrip() {
        assert_eq!(to_unix_secs(from_unix_secs(1_764_000_000)), 1_764_000_000);
    }
}
