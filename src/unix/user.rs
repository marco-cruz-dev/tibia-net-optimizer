//! The real user behind `sudo`/`pkexec`: home directory lookup and handing
//! file ownership back after root writes their settings (Unix only).
//! The env parsing is pure and tested in `config::invoking_user`.

use std::ffi::{CStr, CString};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use super::elevate::is_root;
use crate::config::{invoking_user, InvokingUser};

struct Passwd {
    home: String,
    uid: u32,
    gid: u32,
}

fn lookup(user: &InvokingUser) -> Option<Passwd> {
    // SAFETY: getpw*_r write into our zeroed struct and buffer; `result` is
    // only dereferenced (via `pwd`) when the call succeeded and is non-null.
    unsafe {
        let mut pwd: libc::passwd = std::mem::zeroed();
        let mut buf = vec![0 as libc::c_char; 16 * 1024];
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        let rc = match (user.uid, &user.name) {
            (Some(uid), _) => libc::getpwuid_r(uid, &mut pwd, buf.as_mut_ptr(), buf.len(), &mut result),
            (None, Some(name)) => {
                let name = CString::new(name.as_str()).ok()?;
                libc::getpwnam_r(name.as_ptr(), &mut pwd, buf.as_mut_ptr(), buf.len(), &mut result)
            }
            (None, None) => return None,
        };
        if rc != 0 || result.is_null() || pwd.pw_dir.is_null() {
            return None;
        }
        Some(Passwd {
            home: CStr::from_ptr(pwd.pw_dir).to_string_lossy().into_owned(),
            uid: pwd.pw_uid,
            gid: pwd.pw_gid,
        })
    }
}

/// The invoking user, only when we are actually root.
fn current() -> Option<InvokingUser> {
    if !is_root() {
        return None;
    }
    invoking_user(|k| std::env::var(k).ok())
}

/// Home directory of the user who ran us through sudo/pkexec.
pub fn invoking_home() -> Option<String> {
    lookup(&current()?).map(|p| p.home)
}

/// After root wrote `path` (and maybe created its directory) on behalf of
/// another user, make both owned by that user again. Best effort.
pub fn hand_back(path: &Path) {
    let Some(user) = current() else { return };
    let Some(pw) = lookup(&user) else { return };
    let gid = user.gid.unwrap_or(pw.gid);
    for p in [Some(path), path.parent()].into_iter().flatten() {
        if let Ok(c) = CString::new(p.as_os_str().as_bytes()) {
            // SAFETY: valid NUL-terminated path; failure is ignored (best effort).
            unsafe {
                libc::chown(c.as_ptr(), pw.uid, gid);
            }
        }
    }
}
