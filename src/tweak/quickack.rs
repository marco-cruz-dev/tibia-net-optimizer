//! Linux: TCP quick ACK on every default route (`ip route change ... quickack 1`).

use std::fs;
use std::io;
use std::path::PathBuf;

use super::{Tweak, TweakDetail, TweakId, TweakState, TweakStatus};
use crate::system::command::CommandRunner;
use crate::unix::route::{change_args, parse_default_routes, DefaultRoute, DISPATCHER_SCRIPT, IP};
use crate::Result;

pub const DISPATCHER_PATH: &str = "/etc/NetworkManager/dispatcher.d/99-tibia-net-optimizer";

pub struct QuickAckTweak<R> {
    pub runner: R,
    /// NetworkManager dispatcher hook that re-applies quickack after reconnects.
    /// Only written if its directory exists (i.e. NetworkManager is installed).
    pub dispatcher_path: PathBuf,
}

impl<R: CommandRunner> QuickAckTweak<R> {
    fn routes(&self) -> Result<Vec<DefaultRoute>> {
        let out = self
            .runner
            .run(IP, &["route", "show", "default"])?
            .ensure_success(IP)?;
        Ok(parse_default_routes(&out.stdout))
    }

    fn set(&self, route: &DefaultRoute, enable: bool) -> Result<()> {
        let args = change_args(route, enable);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        self.runner.run(IP, &args)?.ensure_success(IP)?;
        Ok(())
    }
}

impl<R: CommandRunner> Tweak for QuickAckTweak<R> {
    fn id(&self) -> TweakId {
        TweakId::LinuxQuickAck
    }
    fn label(&self) -> &'static str {
        "Quick ACK (default routes)"
    }

    fn check(&self) -> TweakStatus {
        let routes = match self.routes() {
            Ok(r) => r,
            Err(e) => return TweakStatus::unknown(self.id(), self.label(), e),
        };
        let (state, summary) = if routes.is_empty() {
            (TweakState::Unknown, "no default route")
        } else if routes.iter().all(|r| r.quickack) {
            (TweakState::Optimized, "enabled on all default routes")
        } else {
            (TweakState::NotOptimized, "disabled on some default routes")
        };
        let details = routes
            .iter()
            .map(|r| TweakDetail {
                label: r.dev().unwrap_or("?").to_string(),
                value: r
                    .via()
                    .map_or_else(|| "direct".to_string(), |v| format!("via {v}")),
                optimized: r.quickack,
            })
            .collect();
        TweakStatus::new(self.id(), self.label(), state, summary).with_details(details)
    }

    fn apply(&self) -> Result<()> {
        for route in self.routes()?.iter().filter(|r| !r.quickack) {
            self.set(route, true)?;
        }
        // Persist only where NetworkManager's dispatcher directory exists.
        if self.dispatcher_path.parent().is_some_and(|d| d.is_dir()) {
            fs::write(&self.dispatcher_path, DISPATCHER_SCRIPT)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&self.dispatcher_path, fs::Permissions::from_mode(0o755))?;
            }
        }
        Ok(())
    }

    fn revert(&self) -> Result<()> {
        for route in self.routes()?.iter().filter(|r| r.quickack) {
            self.set(route, false)?;
        }
        match fs::remove_file(&self.dispatcher_path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => Ok(other?),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::command::fake::FakeRunner;

    const NONE: &str = "default via 192.168.1.1 dev eth0 proto dhcp metric 100\n";
    const SOME: &str =
        "default via 192.168.1.1 dev eth0 quickack 1\ndefault via 10.0.0.1 dev wlan0 metric 600\n";
    const ALL: &str = "default via 192.168.1.1 dev eth0 quickack 1\n";

    fn tweak<'a>(runner: &'a FakeRunner, dir: &std::path::Path) -> QuickAckTweak<&'a FakeRunner> {
        QuickAckTweak {
            runner,
            dispatcher_path: dir.join("dispatcher.d").join("99-tibia-net-optimizer"),
        }
    }

    #[test]
    fn check_reports_state_and_routes() {
        let dir = tempfile::tempdir().unwrap();
        let runner = FakeRunner::new().respond_ok(SOME);
        let s = tweak(&runner, dir.path()).check();
        assert_eq!(s.state, TweakState::NotOptimized);
        assert_eq!(s.summary, "disabled on some default routes");
        assert_eq!(
            s.details,
            vec![
                TweakDetail {
                    label: "eth0".into(),
                    value: "via 192.168.1.1".into(),
                    optimized: true
                },
                TweakDetail {
                    label: "wlan0".into(),
                    value: "via 10.0.0.1".into(),
                    optimized: false
                },
            ]
        );
        assert_eq!(runner.calls(), vec![vec![IP, "route", "show", "default"]]);

        let runner = FakeRunner::new().respond_ok(ALL);
        let s = tweak(&runner, dir.path()).check();
        assert_eq!(
            (s.state, s.summary.as_str()),
            (TweakState::Optimized, "enabled on all default routes")
        );
    }

    #[test]
    fn check_without_default_route_is_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let runner = FakeRunner::new().respond_ok("");
        let s = tweak(&runner, dir.path()).check();
        assert_eq!(
            (s.state, s.summary.as_str()),
            (TweakState::Unknown, "no default route")
        );
    }

    #[test]
    fn apply_changes_only_routes_without_quickack_and_installs_hook() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("dispatcher.d")).unwrap();
        let runner = FakeRunner::new().respond_ok(SOME);
        let t = tweak(&runner, dir.path());
        t.apply().unwrap();
        let calls = runner.calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(
            calls[1],
            vec![
                IP, "route", "change", "default", "via", "10.0.0.1", "dev", "wlan0", "metric", "600",
                "quickack", "1"
            ]
        );
        assert_eq!(fs::read_to_string(&t.dispatcher_path).unwrap(), DISPATCHER_SCRIPT);
    }

    #[test]
    fn apply_skips_hook_without_networkmanager() {
        let dir = tempfile::tempdir().unwrap();
        let runner = FakeRunner::new().respond_ok(NONE);
        let t = tweak(&runner, dir.path());
        t.apply().unwrap();
        assert!(!t.dispatcher_path.exists());
    }

    #[test]
    fn revert_disables_quickack_and_removes_hook() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("dispatcher.d")).unwrap();
        let runner = FakeRunner::new().respond_ok(SOME);
        let t = tweak(&runner, dir.path());
        fs::write(&t.dispatcher_path, DISPATCHER_SCRIPT).unwrap();
        t.revert().unwrap();
        let calls = runner.calls();
        assert_eq!(calls.len(), 2, "only eth0 had quickack");
        assert_eq!(calls[1].last().unwrap(), "0");
        assert!(!t.dispatcher_path.exists());
    }
}
