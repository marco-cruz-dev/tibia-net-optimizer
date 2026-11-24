//! Elevation on Linux/macOS: `geteuid`, and how to re-run as root.

use crate::platform::Os;

/// Quotes `s` for a POSIX shell.
pub fn sh_quote(s: &str) -> String {
    let safe = !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '=' | ':' | ',' | '+'));
    if safe {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

/// An AppleScript string literal.
pub fn applescript_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Program + args that re-run `exe args...` as root.
///
/// - In a terminal: `sudo` (prompts inline, output stays in the terminal).
/// - No terminal (GUI): `pkexec` on Linux, an AppleScript admin prompt on macOS.
pub fn elevation_command(os: Os, exe: &str, args: &[String], interactive: bool) -> (String, Vec<String>) {
    let argv = || {
        std::iter::once(exe.to_string())
            .chain(args.iter().cloned())
            .collect::<Vec<_>>()
    };
    match (interactive, os) {
        (true, _) => ("sudo".into(), argv()),
        (false, Os::Macos) => {
            let command = argv().iter().map(|a| sh_quote(a)).collect::<Vec<_>>().join(" ");
            let script = format!(
                "do shell script {} with administrator privileges",
                applescript_string(&command)
            );
            ("osascript".into(), vec!["-e".into(), script])
        }
        (false, _) => ("pkexec".into(), argv()),
    }
}

#[cfg(unix)]
pub fn is_root() -> bool {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn sh_quote_leaves_safe_words_bare() {
        assert_eq!(sh_quote("apply"), "apply");
        assert_eq!(sh_quote("/usr/local/bin/tno"), "/usr/local/bin/tno");
        assert_eq!(sh_quote("--json"), "--json");
    }

    #[test]
    fn sh_quote_single_quotes_everything_else() {
        assert_eq!(sh_quote(""), "''");
        assert_eq!(sh_quote("a b"), "'a b'");
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
        assert_eq!(sh_quote("$HOME"), "'$HOME'");
    }

    #[test]
    fn applescript_string_escapes_quotes_and_backslashes() {
        assert_eq!(applescript_string(r#"say "hi" \o/"#), r#""say \"hi\" \\o/""#);
    }

    #[test]
    fn terminal_uses_sudo_on_both_unixes() {
        for os in [Os::Linux, Os::Macos] {
            assert_eq!(
                elevation_command(os, "/usr/bin/tno", &args(&["apply"]), true),
                ("sudo".to_string(), args(&["/usr/bin/tno", "apply"]))
            );
        }
    }

    #[test]
    fn linux_gui_uses_pkexec() {
        assert_eq!(
            elevation_command(Os::Linux, "/usr/bin/tno", &args(&["apply"]), false),
            ("pkexec".to_string(), args(&["/usr/bin/tno", "apply"]))
        );
    }

    #[test]
    fn macos_gui_uses_applescript_admin_prompt() {
        let (prog, a) = elevation_command(
            Os::Macos,
            "/Applications/T N O/tno",
            &args(&["startup", "on"]),
            false,
        );
        assert_eq!(prog, "osascript");
        assert_eq!(
            a,
            args(&[
                "-e",
                r#"do shell script "'/Applications/T N O/tno' startup on" with administrator privileges"#
            ])
        );
    }
}
