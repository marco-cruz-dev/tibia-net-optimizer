//! Windows console handling for a binary that is both a CLI and a GUI.

/// When the app was double-clicked (Windows created a console just for us),
/// release it so only the GUI window remains. When started from a terminal,
/// the console is shared with the shell and is kept.
#[cfg(windows)]
pub fn detach_if_owned() {
    use windows_sys::Win32::System::Console::{FreeConsole, GetConsoleProcessList};

    let mut pids = [0u32; 2];
    // SAFETY: the buffer holds `pids.len()` entries as declared.
    let attached = unsafe { GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32) };
    if attached == 1 {
        // SAFETY: no preconditions; we only print nothing afterwards in GUI mode.
        unsafe {
            FreeConsole();
        }
    }
}

#[cfg(not(windows))]
pub fn detach_if_owned() {}
