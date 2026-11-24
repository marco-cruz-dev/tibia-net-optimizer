//! Windows Task Scheduler integration: run elevated at logon without a UAC
//! prompt (`/rl highest`).

use std::path::Path;

use super::command::CommandRunner;
use crate::Result;

pub const TASK_NAME: &str = "TibiaNetOptimizer";
const SCHTASKS: &str = "schtasks";

/// "Re-apply with admin rights at boot/logon", per OS.
pub trait StartupManager {
    fn is_registered(&self) -> Result<bool>;
    fn register(&self, exe: &Path) -> Result<()>;
    fn unregister(&self) -> Result<()>;
}

/// Windows: Task Scheduler logon task with highest privileges.
pub struct TaskScheduler<R> {
    pub runner: R,
}

impl<R: CommandRunner> StartupManager for TaskScheduler<R> {
    fn is_registered(&self) -> Result<bool> {
        is_registered(&self.runner)
    }
    fn register(&self, exe: &Path) -> Result<()> {
        register(&self.runner, exe)
    }
    fn unregister(&self) -> Result<()> {
        unregister(&self.runner)
    }
}

/// The `/tr` value: quoted exe path followed by `--minimized`.
pub fn task_command(exe: &Path) -> String {
    format!("\"{}\" --minimized", exe.display())
}

/// `true` when the scheduled task exists.
pub fn is_registered(runner: &impl CommandRunner) -> Result<bool> {
    let out = runner.run(SCHTASKS, &["/query", "/tn", TASK_NAME])?;
    Ok(out.success)
}

/// Creates (or overwrites) the logon task. Requires Administrator rights.
pub fn register(runner: &impl CommandRunner, exe: &Path) -> Result<()> {
    let tr = task_command(exe);
    runner
        .run(
            SCHTASKS,
            &[
                "/create", "/tn", TASK_NAME, "/tr", &tr, "/sc", "onlogon", "/rl", "highest", "/f",
            ],
        )?
        .ensure_success(SCHTASKS)?;
    Ok(())
}

/// Removes the logon task. Requires Administrator rights.
pub fn unregister(runner: &impl CommandRunner) -> Result<()> {
    runner
        .run(SCHTASKS, &["/delete", "/tn", TASK_NAME, "/f"])?
        .ensure_success(SCHTASKS)?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    pub struct FakeStartup {
        pub registered: Arc<Mutex<bool>>,
    }

    impl StartupManager for FakeStartup {
        fn is_registered(&self) -> Result<bool> {
            Ok(*self.registered.lock().unwrap())
        }
        fn register(&self, _exe: &Path) -> Result<()> {
            *self.registered.lock().unwrap() = true;
            Ok(())
        }
        fn unregister(&self) -> Result<()> {
            *self.registered.lock().unwrap() = false;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::command::fake::FakeRunner;
    use crate::Error;

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn task_command_quotes_path_with_spaces() {
        let cmd = task_command(Path::new(r"C:\Program Files\TNO\tibia-net-optimizer.exe"));
        assert_eq!(
            cmd,
            r#""C:\Program Files\TNO\tibia-net-optimizer.exe" --minimized"#
        );
    }

    #[test]
    fn is_registered_true_when_query_succeeds() {
        let runner = FakeRunner::new().respond_ok("TaskName ...");
        assert!(is_registered(&runner).unwrap());
        assert_eq!(
            runner.calls(),
            vec![strs(&["schtasks", "/query", "/tn", "TibiaNetOptimizer"])]
        );
    }

    #[test]
    fn is_registered_false_when_query_fails() {
        let runner = FakeRunner::new().respond_fail("ERROR: The system cannot find the file specified.");
        assert!(!is_registered(&runner).unwrap());
    }

    #[test]
    fn register_creates_onlogon_task_with_highest_privileges() {
        let runner = FakeRunner::new();
        register(&runner, Path::new(r"C:\apps\tno.exe")).unwrap();
        assert_eq!(
            runner.calls(),
            vec![strs(&[
                "schtasks",
                "/create",
                "/tn",
                "TibiaNetOptimizer",
                "/tr",
                r#""C:\apps\tno.exe" --minimized"#,
                "/sc",
                "onlogon",
                "/rl",
                "highest",
                "/f",
            ])]
        );
    }

    #[test]
    fn register_surfaces_failure() {
        let runner = FakeRunner::new().respond_fail("ERROR: Access is denied.");
        let err = register(&runner, Path::new(r"C:\x.exe")).unwrap_err();
        assert!(matches!(err, Error::CommandFailed { .. }));
    }

    #[test]
    fn unregister_deletes_task_forcefully() {
        let runner = FakeRunner::new();
        unregister(&runner).unwrap();
        assert_eq!(
            runner.calls(),
            vec![strs(&["schtasks", "/delete", "/tn", "TibiaNetOptimizer", "/f"])]
        );
    }
}
