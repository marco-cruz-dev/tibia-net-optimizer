//! Windows console handling for the GUI-subsystem dashboard binary.

/// The dashboard exe has no console of its own. When it's run with CLI
/// arguments from a terminal, attach to that terminal so output is visible.
/// (When output is redirected, the inherited handles already work.)
#[cfg(windows)]
pub fn attach_parent_console() {
    use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    // SAFETY: no preconditions; failure (no parent console) is harmless.
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(windows))]
pub fn attach_parent_console() {}
