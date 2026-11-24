//! Desktop notifications (auto-heal toasts).
//!
//! - Windows: native WinRT toast (GUI builds only)
//! - Linux: `notify-send` (libnotify)
//! - macOS: `osascript -e 'display notification ...'`

use crate::platform::Os;
use crate::unix::elevate::applescript_string;

pub const APP_NAME: &str = "Tibia Network Optimizer";

/// The external command that shows a notification, or `None` where a native
/// API is used instead (Windows).
pub fn notification_command(os: Os, title: &str, body: &str) -> Option<(String, Vec<String>)> {
    match os {
        Os::Windows => None,
        Os::Linux => Some((
            "notify-send".into(),
            vec![format!("--app-name={APP_NAME}"), title.into(), body.into()],
        )),
        Os::Macos => Some((
            "osascript".into(),
            vec![
                "-e".into(),
                format!(
                    "display notification {} with title {}",
                    applescript_string(body),
                    applescript_string(title)
                ),
            ],
        )),
    }
}

/// Shows a notification without blocking (best effort).
pub fn send(title: &str, body: &str) -> Result<(), String> {
    if let Some((program, args)) = notification_command(Os::current(), title, body) {
        return std::process::Command::new(&program)
            .args(&args)
            .spawn()
            .map(drop)
            .map_err(|e| format!("{program}: {e}"));
    }
    native_toast(title, body)
}

#[cfg(all(windows, feature = "gui"))]
fn native_toast(title: &str, body: &str) -> Result<(), String> {
    use tauri_winrt_notification::Toast;
    // Unpackaged apps have no AppUserModelID of their own; PowerShell's is the
    // documented fallback for showing toasts.
    Toast::new(Toast::POWERSHELL_APP_ID)
        .title(title)
        .text1(body)
        .show()
        .map_err(|e| e.to_string())
}

#[cfg(not(all(windows, feature = "gui")))]
fn native_toast(_title: &str, _body: &str) -> Result<(), String> {
    Err("notifications are not available in this build".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_uses_notify_send() {
        assert_eq!(
            notification_command(Os::Linux, "Title", "Body text"),
            Some((
                "notify-send".to_string(),
                vec![
                    "--app-name=Tibia Network Optimizer".to_string(),
                    "Title".to_string(),
                    "Body text".to_string()
                ]
            ))
        );
    }

    #[test]
    fn macos_uses_osascript_with_escaping() {
        let (program, args) = notification_command(Os::Macos, "T \"1\"", r"a\b").unwrap();
        assert_eq!(program, "osascript");
        assert_eq!(
            args,
            vec![
                "-e".to_string(),
                r#"display notification "a\\b" with title "T \"1\"""#.to_string()
            ]
        );
    }

    #[test]
    fn windows_uses_native_toast() {
        assert_eq!(notification_command(Os::Windows, "t", "b"), None);
    }
}
