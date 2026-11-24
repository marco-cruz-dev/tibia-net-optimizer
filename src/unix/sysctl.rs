//! Port over kernel tunables (`sysctl`), on Linux and macOS alike.

use crate::system::command::CommandRunner;
use crate::{Error, Result};

/// Absolute path so it works even when `/usr/sbin` isn't on a user's PATH
/// (Debian). On merged-/usr distros and macOS this path exists too.
pub const SYSCTL: &str = "/usr/sbin/sysctl";

pub trait Sysctl {
    fn read(&self, key: &str) -> Result<String>;
    /// Requires root.
    fn write(&self, key: &str, value: &str) -> Result<()>;
}

impl<T: Sysctl + ?Sized> Sysctl for &T {
    fn read(&self, key: &str) -> Result<String> {
        (**self).read(key)
    }
    fn write(&self, key: &str, value: &str) -> Result<()> {
        (**self).write(key, value)
    }
}

/// `sysctl -n <key>` / `sysctl -w <key>=<value>`.
#[derive(Debug, Clone, Copy, Default)]
pub struct CommandSysctl<R> {
    pub runner: R,
}

impl<R: CommandRunner> Sysctl for CommandSysctl<R> {
    fn read(&self, key: &str) -> Result<String> {
        let out = self.runner.run(SYSCTL, &["-n", key])?.ensure_success(SYSCTL)?;
        parse_value(&out.stdout)
    }

    fn write(&self, key: &str, value: &str) -> Result<()> {
        let assignment = format!("{key}={value}");
        self.runner
            .run(SYSCTL, &["-w", &assignment])?
            .ensure_success(SYSCTL)?;
        Ok(())
    }
}

/// Trims `sysctl -n` output; empty output is an error.
pub fn parse_value(output: &str) -> Result<String> {
    let value = output.trim();
    if value.is_empty() {
        Err(Error::Parse("empty sysctl output".into()))
    } else {
        Ok(value.to_string())
    }
}

#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    /// In-memory kernel tunables; missing keys fail to read.
    #[derive(Default)]
    pub struct FakeSysctl(pub RefCell<BTreeMap<String, String>>);

    impl FakeSysctl {
        pub fn with(pairs: &[(&str, &str)]) -> Self {
            Self(RefCell::new(
                pairs
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            ))
        }
        pub fn get(&self, key: &str) -> Option<String> {
            self.0.borrow().get(key).cloned()
        }
    }

    impl Sysctl for FakeSysctl {
        fn read(&self, key: &str) -> Result<String> {
            self.get(key)
                .ok_or_else(|| Error::Parse(format!("unknown oid '{key}'")))
        }
        fn write(&self, key: &str, value: &str) -> Result<()> {
            self.0.borrow_mut().insert(key.into(), value.into());
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::command::fake::FakeRunner;

    #[test]
    fn parse_value_trims_whitespace() {
        assert_eq!(parse_value(" 1\n").unwrap(), "1");
    }

    #[test]
    fn parse_value_rejects_empty_output() {
        assert!(matches!(parse_value("  \n"), Err(Error::Parse(_))));
    }

    #[test]
    fn read_runs_sysctl_n() {
        let runner = FakeRunner::new().respond_ok("3\n");
        let s = CommandSysctl { runner: &runner };
        assert_eq!(s.read("net.inet.tcp.delayed_ack").unwrap(), "3");
        assert_eq!(
            runner.calls(),
            vec![vec![SYSCTL, "-n", "net.inet.tcp.delayed_ack"]]
        );
    }

    #[test]
    fn write_runs_sysctl_w() {
        let runner = FakeRunner::new();
        CommandSysctl { runner: &runner }
            .write("net.ipv4.tcp_autocorking", "0")
            .unwrap();
        assert_eq!(
            runner.calls(),
            vec![vec![SYSCTL, "-w", "net.ipv4.tcp_autocorking=0"]]
        );
    }

    #[test]
    fn failures_are_reported() {
        let runner = FakeRunner::new().respond_fail("sysctl: permission denied on key");
        let err = CommandSysctl { runner: &runner }.write("k", "0").unwrap_err();
        assert!(matches!(err, Error::CommandFailed { .. }));
    }
}
