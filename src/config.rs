//! User settings persisted as JSON in `%APPDATA%\TibiaNetOptimizer\config.json`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::platform::Os;
use crate::Result;

pub const DEFAULT_PING_TARGET: &str = "secura.tibia.com";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Re-apply optimizations automatically if Windows resets them.
    /// Off by default: modifying system settings should be opt-in.
    pub auto_heal: bool,
    /// Clicking the window's X hides it to the tray instead of quitting.
    pub minimize_to_tray: bool,
    /// Host used by the latency monitor.
    pub ping_target: String,
    /// Background drift-check interval.
    pub check_interval_secs: u64,
    /// Unix seconds of the last auto-tuning change made by this app; used to
    /// recommend a reboot until the machine has restarted.
    pub autotuning_changed_at: Option<u64>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            auto_heal: false,
            minimize_to_tray: true,
            ping_target: DEFAULT_PING_TARGET.to_string(),
            check_interval_secs: 60,
            autotuning_changed_at: None,
        }
    }
}

impl Config {
    /// `config.json` inside [`config_dir_for`] for the running OS.
    pub fn default_path() -> Option<PathBuf> {
        config_dir_for(Os::current(), |k| std::env::var(k).ok()).map(|d| d.join("config.json"))
    }

    /// Loads settings; a missing or corrupt file yields defaults so the app
    /// always starts. Unknown/missing fields fall back to defaults too.
    pub fn load(path: &Path) -> Config {
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Writes settings, creating the parent directory if needed.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self).expect("Config is always serializable");
        fs::write(path, json)?;
        Ok(())
    }
}

/// Per-OS settings directory:
/// - Windows: `%APPDATA%\TibiaNetOptimizer`
/// - Linux: `$XDG_CONFIG_HOME/tibia-net-optimizer` (absolute only), else `~/.config/tibia-net-optimizer`
/// - macOS: `~/Library/Application Support/TibiaNetOptimizer`
pub fn config_dir_for(os: Os, env: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let non_empty = |k: &str| env(k).filter(|v| !v.is_empty());
    match os {
        Os::Windows => non_empty("APPDATA").map(|d| PathBuf::from(d).join("TibiaNetOptimizer")),
        Os::Linux => non_empty("XDG_CONFIG_HOME")
            .filter(|d| d.starts_with('/'))
            .map(PathBuf::from)
            .or_else(|| non_empty("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|d| d.join("tibia-net-optimizer")),
        Os::Macos => non_empty("HOME").map(|h| {
            PathBuf::from(h)
                .join("Library")
                .join("Application Support")
                .join("TibiaNetOptimizer")
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn windows_config_dir_uses_appdata() {
        // Built with join() so the test also holds on Unix, where `\` isn't a separator.
        let appdata = r"C:\Users\me\AppData\Roaming";
        assert_eq!(
            config_dir_for(Os::Windows, env(&[("APPDATA", appdata)])),
            Some(PathBuf::from(appdata).join("TibiaNetOptimizer"))
        );
        assert_eq!(config_dir_for(Os::Windows, env(&[])), None);
    }

    #[test]
    fn linux_config_dir_prefers_absolute_xdg() {
        assert_eq!(
            config_dir_for(
                Os::Linux,
                env(&[("XDG_CONFIG_HOME", "/xdg"), ("HOME", "/home/me")])
            ),
            Some(PathBuf::from("/xdg/tibia-net-optimizer"))
        );
        assert_eq!(
            config_dir_for(
                Os::Linux,
                env(&[("XDG_CONFIG_HOME", "relative"), ("HOME", "/home/me")])
            ),
            Some(PathBuf::from("/home/me/.config/tibia-net-optimizer"))
        );
        assert_eq!(
            config_dir_for(Os::Linux, env(&[("XDG_CONFIG_HOME", ""), ("HOME", "/home/me")])),
            Some(PathBuf::from("/home/me/.config/tibia-net-optimizer"))
        );
        assert_eq!(config_dir_for(Os::Linux, env(&[])), None);
    }

    #[test]
    fn macos_config_dir_uses_application_support() {
        assert_eq!(
            config_dir_for(Os::Macos, env(&[("HOME", "/Users/me")])),
            Some(PathBuf::from(
                "/Users/me/Library/Application Support/TibiaNetOptimizer"
            ))
        );
    }

    #[test]
    fn defaults_are_safe() {
        let c = Config::default();
        assert!(!c.auto_heal);
        assert!(c.minimize_to_tray);
        assert_eq!(c.ping_target, DEFAULT_PING_TARGET);
        assert_eq!(c.check_interval_secs, 60);
        assert_eq!(c.autotuning_changed_at, None);
    }

    #[test]
    fn save_then_load_roundtrips_and_creates_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.json");
        let c = Config {
            auto_heal: true,
            minimize_to_tray: false,
            ping_target: "1.1.1.1".into(),
            check_interval_secs: 30,
            autotuning_changed_at: Some(1_764_000_000),
        };
        c.save(&path).unwrap();
        assert_eq!(Config::load(&path), c);
    }

    #[test]
    fn missing_file_loads_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Config::load(&dir.path().join("nope.json")), Config::default());
    }

    #[test]
    fn corrupt_file_loads_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, "{ not json").unwrap();
        assert_eq!(Config::load(&path), Config::default());
    }

    #[test]
    fn partial_file_fills_missing_fields_with_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, r#"{ "auto_heal": true }"#).unwrap();
        let c = Config::load(&path);
        assert!(c.auto_heal);
        assert_eq!(c.ping_target, DEFAULT_PING_TARGET);
    }
}
