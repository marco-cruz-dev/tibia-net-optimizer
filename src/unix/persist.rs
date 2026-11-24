//! Persisting sysctl values across reboots, remembering the originals so
//! `revert` can restore them.
//!
//! - Linux: `/etc/sysctl.d/99-tibia-net-optimizer.conf`
//! - macOS: a LaunchDaemon plist that runs `sysctl -w` at boot
//!
//! Both files embed machine-readable metadata comments
//! (`tno-setting <key> value=<v> original=<o|->`), so one parser reads both.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

use super::service::launchd_plist;
use super::sysctl::SYSCTL;
use crate::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub value: String,
    /// The value before we first changed it (`None` if unknown).
    pub original: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PersistedSettings {
    pub entries: BTreeMap<String, Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistFormat {
    SysctlConf,
    LaunchdPlist { label: String },
}

const MARKER: &str = "tno-setting";

fn metadata_line(key: &str, e: &Entry) -> String {
    format!(
        "{MARKER} {key} value={} original={}",
        e.value,
        e.original.as_deref().unwrap_or("-")
    )
}

pub fn render(settings: &PersistedSettings, format: &PersistFormat) -> String {
    let metadata: Vec<String> = settings
        .entries
        .iter()
        .map(|(k, e)| metadata_line(k, e))
        .collect();
    match format {
        PersistFormat::SysctlConf => {
            let mut out = String::from("# Managed by tibia-net-optimizer. Remove with `tno revert`.\n");
            for line in &metadata {
                out.push_str(&format!("# {line}\n"));
            }
            out.push('\n');
            for (key, e) in &settings.entries {
                out.push_str(&format!("{key} = {}\n", e.value));
            }
            out
        }
        PersistFormat::LaunchdPlist { label } => {
            let mut args = vec![SYSCTL.to_string(), "-w".to_string()];
            args.extend(settings.entries.iter().map(|(k, e)| format!("{k}={}", e.value)));
            let mut comments = vec!["Managed by tibia-net-optimizer. Remove with `tno revert`.".into()];
            comments.extend(metadata);
            launchd_plist(label, &args, &comments)
        }
    }
}

/// Reads the metadata comments back; anything else is ignored.
pub fn parse(text: &str) -> PersistedSettings {
    let mut settings = PersistedSettings::default();
    for line in text.lines() {
        let Some(rest) = line.split_once(MARKER).map(|(_, r)| r) else {
            continue;
        };
        let mut tokens = rest.trim().trim_end_matches("-->").split_whitespace();
        let Some(key) = tokens.next() else { continue };
        let (mut value, mut original) = (None, None);
        for t in tokens {
            if let Some(v) = t.strip_prefix("value=") {
                value = Some(v.to_string());
            } else if let Some(o) = t.strip_prefix("original=") {
                original = (o != "-").then(|| o.to_string());
            }
        }
        if let Some(value) = value {
            settings
                .entries
                .insert(key.to_string(), Entry { value, original });
        }
    }
    settings
}

/// Where persisted settings live.
pub trait SettingsStore {
    fn load(&self) -> Result<PersistedSettings>;
    /// Saving an empty set removes the file.
    fn save(&self, settings: &PersistedSettings) -> Result<()>;
}

impl<T: SettingsStore + ?Sized> SettingsStore for &T {
    fn load(&self) -> Result<PersistedSettings> {
        (**self).load()
    }
    fn save(&self, settings: &PersistedSettings) -> Result<()> {
        (**self).save(settings)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSettingsStore {
    pub path: PathBuf,
    pub format: PersistFormat,
}

impl SettingsStore for FileSettingsStore {
    fn load(&self) -> Result<PersistedSettings> {
        match fs::read_to_string(&self.path) {
            Ok(text) => Ok(parse(&text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(PersistedSettings::default()),
            Err(e) => Err(e.into()),
        }
    }

    fn save(&self, settings: &PersistedSettings) -> Result<()> {
        if settings.entries.is_empty() {
            return match fs::remove_file(&self.path) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                other => Ok(other?),
            };
        }
        fs::write(&self.path, render(settings, &self.format))?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct MemStore(pub RefCell<PersistedSettings>);

    impl SettingsStore for MemStore {
        fn load(&self) -> Result<PersistedSettings> {
            Ok(self.0.borrow().clone())
        }
        fn save(&self, settings: &PersistedSettings) -> Result<()> {
            *self.0.borrow_mut() = settings.clone();
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PersistedSettings {
        let mut s = PersistedSettings::default();
        s.entries.insert(
            "net.ipv4.tcp_autocorking".into(),
            Entry {
                value: "0".into(),
                original: Some("1".into()),
            },
        );
        s.entries.insert(
            "net.ipv4.tcp_moderate_rcvbuf".into(),
            Entry {
                value: "0".into(),
                original: None,
            },
        );
        s
    }

    fn plist() -> PersistFormat {
        PersistFormat::LaunchdPlist {
            label: "com.tibianetoptimizer.sysctl".into(),
        }
    }

    #[test]
    fn sysctl_conf_has_assignments_and_metadata() {
        let text = render(&sample(), &PersistFormat::SysctlConf);
        assert!(text.contains("\nnet.ipv4.tcp_autocorking = 0\n"), "{text}");
        assert!(
            text.contains("# tno-setting net.ipv4.tcp_autocorking value=0 original=1"),
            "{text}"
        );
        assert!(
            text.contains("# tno-setting net.ipv4.tcp_moderate_rcvbuf value=0 original=-"),
            "{text}"
        );
        assert!(text.starts_with("# Managed by tibia-net-optimizer"), "{text}");
    }

    #[test]
    fn launchd_plist_runs_sysctl_at_boot() {
        let text = render(&sample(), &plist());
        assert!(
            text.contains("<string>com.tibianetoptimizer.sysctl</string>"),
            "{text}"
        );
        assert!(text.contains("<string>/usr/sbin/sysctl</string>"), "{text}");
        assert!(text.contains("<string>-w</string>"), "{text}");
        assert!(
            text.contains("<string>net.ipv4.tcp_autocorking=0</string>"),
            "{text}"
        );
        assert!(text.contains("<key>RunAtLoad</key>"), "{text}");
        assert!(
            text.contains("<!-- tno-setting net.ipv4.tcp_autocorking value=0 original=1 -->"),
            "{text}"
        );
    }

    #[test]
    fn both_formats_roundtrip() {
        for f in [PersistFormat::SysctlConf, plist()] {
            assert_eq!(parse(&render(&sample(), &f)), sample(), "{f:?}");
        }
    }

    #[test]
    fn parse_ignores_unrelated_and_malformed_lines() {
        let text =
            "net.core.foo = 1\n# random comment\n# tno-setting broken\n# tno-setting k value=1 original=2\n";
        let s = parse(text);
        assert_eq!(s.entries.len(), 1);
        assert_eq!(
            s.entries["k"],
            Entry {
                value: "1".into(),
                original: Some("2".into())
            }
        );
    }

    #[test]
    fn file_store_roundtrip_and_removal() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileSettingsStore {
            path: dir.path().join("99-tibia-net-optimizer.conf"),
            format: PersistFormat::SysctlConf,
        };
        assert_eq!(
            store.load().unwrap(),
            PersistedSettings::default(),
            "missing file = empty"
        );
        store.save(&sample()).unwrap();
        assert_eq!(store.load().unwrap(), sample());
        store.save(&PersistedSettings::default()).unwrap();
        assert!(!store.path.exists(), "empty settings remove the file");
        store.save(&PersistedSettings::default()).unwrap(); // removing twice is fine
    }
}
