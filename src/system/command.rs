//! Port for running external programs (`netsh`, `schtasks`) silently.

use std::io;
use std::process::Command;

use super::encoding::decode_console_output;
use crate::{Error, Result};

/// Captured result of running an external program.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommandOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    /// Converts a failed run into [`Error::CommandFailed`].
    ///
    /// Windows tools often print their error text to stdout, so both streams
    /// are included in the message.
    pub fn ensure_success(self, program: &str) -> Result<CommandOutput> {
        if self.success {
            return Ok(self);
        }
        let message = [self.stdout.trim(), self.stderr.trim()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" | ");
        Err(Error::CommandFailed {
            program: program.to_string(),
            message,
        })
    }
}

/// Runs an external program and captures its output.
pub trait CommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> io::Result<CommandOutput>;
}

impl<T: CommandRunner + ?Sized> CommandRunner for &T {
    fn run(&self, program: &str, args: &[&str]) -> io::Result<CommandOutput> {
        (**self).run(program, args)
    }
}

/// Real implementation backed by `std::process::Command`.
/// On Windows it uses `CREATE_NO_WINDOW` so no console window flashes.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemCommandRunner;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> io::Result<CommandOutput> {
        let mut cmd = Command::new(program);
        cmd.args(args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let out = cmd.output()?;
        // NOTE: console tools emit the OEM code page; lossy UTF-8 keeps ASCII
        // keywords intact, which is all the parsers rely on.
        Ok(CommandOutput {
            success: out.status.success(),
            stdout: decode_console_output(&out.stdout),
            stderr: decode_console_output(&out.stderr),
        })
    }
}

/// Test double: records every invocation and replays queued responses.
#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    #[derive(Default)]
    pub struct FakeRunner {
        pub calls: RefCell<Vec<Vec<String>>>,
        responses: RefCell<VecDeque<io::Result<CommandOutput>>>,
    }

    impl FakeRunner {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn respond_ok(self, stdout: &str) -> Self {
            self.responses.borrow_mut().push_back(Ok(CommandOutput {
                success: true,
                stdout: stdout.into(),
                stderr: String::new(),
            }));
            self
        }

        pub fn respond_fail(self, stdout: &str) -> Self {
            self.responses.borrow_mut().push_back(Ok(CommandOutput {
                success: false,
                stdout: stdout.into(),
                stderr: String::new(),
            }));
            self
        }

        /// Each call as `[program, arg1, arg2, ...]`.
        pub fn calls(&self) -> Vec<Vec<String>> {
            self.calls.borrow().clone()
        }
    }

    impl CommandRunner for FakeRunner {
        fn run(&self, program: &str, args: &[&str]) -> io::Result<CommandOutput> {
            let mut call = vec![program.to_string()];
            call.extend(args.iter().map(|a| a.to_string()));
            self.calls.borrow_mut().push(call);
            self.responses.borrow_mut().pop_front().unwrap_or_else(|| {
                Ok(CommandOutput {
                    success: true,
                    ..Default::default()
                })
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_success_passes_through_successful_output() {
        let out = CommandOutput {
            success: true,
            stdout: "ok".into(),
            stderr: String::new(),
        };
        assert_eq!(out.clone().ensure_success("netsh").unwrap(), out);
    }

    #[test]
    fn ensure_success_reports_both_streams_on_failure() {
        let out = CommandOutput {
            success: false,
            stdout: "The requested operation requires elevation.\r\n".into(),
            stderr: "boom".into(),
        };
        match out.ensure_success("netsh") {
            Err(Error::CommandFailed { program, message }) => {
                assert_eq!(program, "netsh");
                assert_eq!(message, "The requested operation requires elevation. | boom");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }
}
