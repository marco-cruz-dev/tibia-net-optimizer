//! "Re-apply at boot" integration: systemd (Linux) and launchd (macOS).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::system::command::CommandRunner;
use crate::system::startup::StartupManager;
use crate::Result;

pub const SYSTEMD_UNIT_NAME: &str = "tibia-net-optimizer.service";
pub const SYSTEMD_UNIT_PATH: &str = "/etc/systemd/system/tibia-net-optimizer.service";
pub const SYSTEMCTL: &str = "systemctl";
pub const LAUNCHD_STARTUP_LABEL: &str = "com.tibianetoptimizer.startup";
pub const LAUNCHD_STARTUP_PATH: &str = "/Library/LaunchDaemons/com.tibianetoptimizer.startup.plist";

pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// Quotes one argument for a systemd `ExecStart=` line.
pub fn systemd_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// One-shot unit that runs `<exe> apply` once the network is up.
pub fn systemd_unit(exe: &Path) -> String {
    format!(
        "# Managed by tibia-net-optimizer. Remove with `tibia-net-optimizer startup off`.\n\
         [Unit]\n\
         Description=Tibia Network Optimizer - re-apply TCP tweaks at boot\n\
         Wants=network-online.target\n\
         After=network-online.target\n\
         \n\
         [Service]\n\
         Type=oneshot\n\
         ExecStart={} apply\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n",
        systemd_quote(&exe.to_string_lossy())
    )
}

/// A LaunchDaemon plist running `program_args` once at boot.
/// `comments` are emitted as XML comments before the `<plist>` element.
pub fn launchd_plist(label: &str, program_args: &[String], comments: &[String]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
    );
    for c in comments {
        // `--` is not allowed inside XML comments.
        out.push_str(&format!("<!-- {} -->\n", c.replace("--", "- -")));
    }
    out.push_str("<plist version=\"1.0\">\n<dict>\n");
    out.push_str(&format!(
        "  <key>Label</key>\n  <string>{}</string>\n",
        xml_escape(label)
    ));
    out.push_str("  <key>ProgramArguments</key>\n  <array>\n");
    for a in program_args {
        out.push_str(&format!("    <string>{}</string>\n", xml_escape(a)));
    }
    out.push_str("  </array>\n  <key>RunAtLoad</key>\n  <true/>\n</dict>\n</plist>\n");
    out
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Linux: systemd system unit.
pub struct SystemdStartup<R> {
    pub runner: R,
    pub unit_path: PathBuf,
}

impl<R: CommandRunner> SystemdStartup<R> {
    fn systemctl(&self, args: &[&str]) -> Result<bool> {
        Ok(self.runner.run(SYSTEMCTL, args)?.success)
    }
}

impl<R: CommandRunner> StartupManager for SystemdStartup<R> {
    fn is_registered(&self) -> Result<bool> {
        if !self.unit_path.exists() {
            return Ok(false);
        }
        self.systemctl(&["is-enabled", SYSTEMD_UNIT_NAME])
    }
    fn register(&self, exe: &Path) -> Result<()> {
        fs::write(&self.unit_path, systemd_unit(exe))?;
        self.runner
            .run(SYSTEMCTL, &["daemon-reload"])?
            .ensure_success(SYSTEMCTL)?;
        self.runner
            .run(SYSTEMCTL, &["enable", SYSTEMD_UNIT_NAME])?
            .ensure_success(SYSTEMCTL)?;
        Ok(())
    }
    fn unregister(&self) -> Result<()> {
        // Disabling an unknown unit fails harmlessly; removal is what matters.
        self.systemctl(&["disable", SYSTEMD_UNIT_NAME])?;
        remove_if_exists(&self.unit_path)?;
        self.systemctl(&["daemon-reload"])?;
        Ok(())
    }
}

/// macOS: LaunchDaemon plist (loaded by launchd at the next boot).
pub struct LaunchdStartup {
    pub plist_path: PathBuf,
}

impl StartupManager for LaunchdStartup {
    fn is_registered(&self) -> Result<bool> {
        Ok(self.plist_path.exists())
    }
    fn register(&self, exe: &Path) -> Result<()> {
        let plist = launchd_plist(
            LAUNCHD_STARTUP_LABEL,
            &[exe.to_string_lossy().into_owned(), "apply".into()],
            &["Managed by tibia-net-optimizer. Remove with `tibia-net-optimizer startup off`.".into()],
        );
        fs::write(&self.plist_path, plist)?;
        Ok(())
    }
    fn unregister(&self) -> Result<()> {
        remove_if_exists(&self.plist_path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::command::fake::FakeRunner;

    #[test]
    fn xml_escape_handles_special_chars() {
        assert_eq!(xml_escape(r#"a&b<c>"d'"#), "a&amp;b&lt;c&gt;&quot;d&apos;");
        assert_eq!(xml_escape("plain"), "plain");
    }

    #[test]
    fn systemd_quote_wraps_and_escapes() {
        assert_eq!(systemd_quote("/usr/bin/tno"), r#""/usr/bin/tno""#);
        assert_eq!(systemd_quote(r#"/opt/a "b"\c"#), r#""/opt/a \"b\"\\c""#);
    }

    #[test]
    fn systemd_unit_runs_apply_after_network() {
        let unit = systemd_unit(Path::new("/opt/tibia net/tno"));
        assert!(
            unit.contains("ExecStart=\"/opt/tibia net/tno\" apply\n"),
            "{unit}"
        );
        assert!(unit.contains("Type=oneshot"), "{unit}");
        assert!(unit.contains("After=network-online.target"), "{unit}");
        assert!(unit.contains("Wants=network-online.target"), "{unit}");
        assert!(unit.contains("WantedBy=multi-user.target"), "{unit}");
    }

    #[test]
    fn launchd_plist_is_escaped_and_runs_at_load() {
        let p = launchd_plist(
            "com.example",
            &["/Apps/A&B/tno".into(), "apply".into()],
            &["note".into()],
        );
        assert!(
            p.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"),
            "{p}"
        );
        assert!(p.contains("<!-- note -->"), "{p}");
        assert!(
            p.contains("<key>Label</key>\n  <string>com.example</string>"),
            "{p}"
        );
        assert!(
            p.contains("    <string>/Apps/A&amp;B/tno</string>\n    <string>apply</string>"),
            "{p}"
        );
        assert!(p.contains("<key>RunAtLoad</key>\n  <true/>"), "{p}");
        assert!(p.trim_end().ends_with("</plist>"), "{p}");
    }

    #[test]
    fn systemd_startup_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let runner = FakeRunner::new();
        let s = SystemdStartup {
            runner: &runner,
            unit_path: dir.path().join(SYSTEMD_UNIT_NAME),
        };
        s.register(Path::new("/usr/bin/tno")).unwrap();
        assert!(fs::read_to_string(&s.unit_path)
            .unwrap()
            .contains("ExecStart=\"/usr/bin/tno\" apply"));
        s.unregister().unwrap();
        assert!(!s.unit_path.exists());
        assert_eq!(
            runner.calls(),
            vec![
                vec![SYSTEMCTL, "daemon-reload"],
                vec![SYSTEMCTL, "enable", SYSTEMD_UNIT_NAME],
                vec![SYSTEMCTL, "disable", SYSTEMD_UNIT_NAME],
                vec![SYSTEMCTL, "daemon-reload"],
            ]
        );
    }

    #[test]
    fn systemd_is_registered_requires_unit_and_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let unit_path = dir.path().join(SYSTEMD_UNIT_NAME);
        let runner = FakeRunner::new();
        let s = SystemdStartup {
            runner: &runner,
            unit_path: unit_path.clone(),
        };
        assert!(!s.is_registered().unwrap(), "no unit file");
        assert!(runner.calls().is_empty(), "doesn't ask systemctl without a unit");

        fs::write(&unit_path, "x").unwrap();
        let runner = FakeRunner::new().respond_fail("disabled");
        let s = SystemdStartup {
            runner: &runner,
            unit_path: unit_path.clone(),
        };
        assert!(!s.is_registered().unwrap());
        assert_eq!(
            runner.calls(),
            vec![vec![SYSTEMCTL, "is-enabled", SYSTEMD_UNIT_NAME]]
        );

        let runner = FakeRunner::new().respond_ok("enabled");
        assert!(SystemdStartup {
            runner: &runner,
            unit_path
        }
        .is_registered()
        .unwrap());
    }

    #[test]
    fn launchd_startup_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let s = LaunchdStartup {
            plist_path: dir.path().join("com.tibianetoptimizer.startup.plist"),
        };
        assert!(!s.is_registered().unwrap());
        s.register(Path::new("/Applications/TNO/tno")).unwrap();
        assert!(s.is_registered().unwrap());
        let text = fs::read_to_string(&s.plist_path).unwrap();
        assert!(text.contains(LAUNCHD_STARTUP_LABEL) && text.contains("<string>apply</string>"));
        s.unregister().unwrap();
        assert!(!s.is_registered().unwrap());
        s.unregister().unwrap(); // idempotent
    }
}
