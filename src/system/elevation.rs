//! Administrator detection and "Restart as Administrator".

/// Quotes one argument so `CommandLineToArgvW` / the MSVC runtime parse it
/// back verbatim.
pub fn quote_windows_arg(arg: &str) -> String {
    let needs_quotes = arg.is_empty() || arg.contains([' ', '\t', '\n', '\x0b', '"']);
    if !needs_quotes {
        return arg.to_string();
    }
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('"');
    let mut backslashes = 0usize;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                // Escape the run of backslashes and the quote itself.
                out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.extend(std::iter::repeat_n('\\', backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    // Trailing backslashes precede the closing quote, so they must be doubled.
    out.extend(std::iter::repeat_n('\\', backslashes * 2));
    out.push('"');
    out
}

/// Joins arguments into a single `lpParameters` string for `ShellExecuteW`.
pub fn build_parameters(args: &[String]) -> String {
    args.iter()
        .map(|a| quote_windows_arg(a))
        .collect::<Vec<_>>()
        .join(" ")
}

/// `true` when the current process token is elevated.
#[cfg(windows)]
pub fn is_elevated() -> bool {
    use std::ffi::c_void;
    use std::mem::size_of;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    // SAFETY: plain Win32 calls with correctly sized out-parameters; the token
    // handle is closed before returning.
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut c_void,
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

/// Non-Windows elevation lands in Sprint 3 (`geteuid() == 0`).
#[cfg(not(windows))]
pub fn is_elevated() -> bool {
    false
}

/// Relaunches the current executable elevated (UAC prompt) with `args`.
/// Returns once the elevated process has been started.
#[cfg(windows)]
pub fn relaunch_elevated(args: &[String]) -> crate::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    let exe = std::env::current_exe()?;
    let verb = wide("runas".as_ref());
    let file = wide(exe.as_os_str());
    let params = wide(build_parameters(args).as_ref());

    // SAFETY: all strings are NUL-terminated UTF-16 buffers that outlive the call.
    let code = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    // ShellExecuteW returns a value > 32 on success; 5 = access denied (UAC declined).
    match code {
        c if c > 32 => Ok(()),
        5 => Err(crate::Error::AccessDenied),
        c => Err(crate::Error::Io(std::io::Error::other(format!(
            "ShellExecuteW failed with code {c}"
        )))),
    }
}

#[cfg(not(windows))]
pub fn relaunch_elevated(_args: &[String]) -> crate::Result<()> {
    Err(crate::Error::Io(std::io::Error::other(
        "elevated relaunch is not supported on this platform yet",
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_args_are_left_alone() {
        assert_eq!(quote_windows_arg("apply"), "apply");
        assert_eq!(quote_windows_arg("--pause"), "--pause");
        assert_eq!(quote_windows_arg(r"a\\b"), r"a\\b");
    }

    #[test]
    fn empty_arg_becomes_empty_quotes() {
        assert_eq!(quote_windows_arg(""), r#""""#);
    }

    #[test]
    fn whitespace_triggers_quoting() {
        assert_eq!(quote_windows_arg("a b"), r#""a b""#);
        assert_eq!(quote_windows_arg("a\tb"), "\"a\tb\"");
    }

    #[test]
    fn embedded_quotes_are_escaped() {
        assert_eq!(quote_windows_arg(r#"say "hi""#), r#""say \"hi\"""#);
    }

    #[test]
    fn backslashes_before_quote_are_doubled() {
        // a\"b  →  "a\\\"b"
        assert_eq!(quote_windows_arg(r#"a\"b"#), r#""a\\\"b""#);
    }

    #[test]
    fn trailing_backslashes_are_doubled_before_closing_quote() {
        assert_eq!(quote_windows_arg(r"C:\with space\"), r#""C:\with space\\""#);
    }

    #[test]
    fn interior_backslashes_are_literal() {
        assert_eq!(quote_windows_arg(r"C:\a b\c"), r#""C:\a b\c""#);
    }

    #[test]
    fn parameters_are_space_joined() {
        let args = vec!["startup".to_string(), "on".into(), "a b".into()];
        assert_eq!(build_parameters(&args), r#"startup on "a b""#);
        assert_eq!(build_parameters(&[]), "");
    }
}
