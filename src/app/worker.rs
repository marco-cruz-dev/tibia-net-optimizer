//! Background thread that owns the [`Platform`] and performs slow OS calls,
//! so the UI never blocks.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::SystemTime;

use super::state::{WorkerCmd, WorkerEvent};
use crate::actions::{run_action, Action};
use crate::platform::Platform;
use crate::report::collect;
use crate::system::boot::to_unix_secs;

/// Runs CLI args elevated in a separate process (pkexec / osascript).
pub type Elevator = Box<dyn Fn(&[String]) -> Result<(), String> + Send>;

pub struct Worker {
    tx: Option<Sender<WorkerCmd>>,
    events: Receiver<WorkerEvent>,
    handle: Option<JoinHandle<()>>,
}

impl Worker {
    /// `make_platform` runs on the worker thread (platform objects needn't
    /// be `Send`). `notify` is called after every event (e.g. to wake the UI).
    pub fn spawn<F, N>(make_platform: F, elevated: bool, elevator: Elevator, notify: N) -> Worker
    where
        F: FnOnce() -> Platform + Send + 'static,
        N: Fn() + Send + 'static,
    {
        let (cmd_tx, cmd_rx) = mpsc::channel::<WorkerCmd>();
        let (event_tx, event_rx) = mpsc::channel::<WorkerEvent>();
        let handle = std::thread::Builder::new()
            .name("tno-worker".into())
            .spawn(move || {
                let platform = make_platform();
                // Ends when the Worker is dropped (sender closed).
                for cmd in cmd_rx {
                    let event = match cmd {
                        WorkerCmd::Refresh {
                            autotuning_changed_at,
                        } => WorkerEvent::Status(collect(&platform, elevated, autotuning_changed_at)),
                        WorkerCmd::Run(action) => run_in_process(&platform, action),
                        WorkerCmd::RunElevated(action) => WorkerEvent::ActionFinished {
                            action,
                            result: elevator(&action.cli_args()),
                            reboot_change_at: None,
                        },
                    };
                    if event_tx.send(event).is_err() {
                        break;
                    }
                    notify();
                }
            })
            .expect("failed to spawn worker thread");
        Worker {
            tx: Some(cmd_tx),
            events: event_rx,
            handle: Some(handle),
        }
    }

    pub fn send(&self, cmd: WorkerCmd) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(cmd);
        }
    }

    /// Events received so far (non-blocking).
    pub fn drain(&self) -> Vec<WorkerEvent> {
        self.events.try_iter().collect()
    }

    /// Blocks for the next event (tests).
    pub fn recv(&self) -> Option<WorkerEvent> {
        self.events.recv_timeout(std::time::Duration::from_secs(5)).ok()
    }
}

fn run_in_process(platform: &Platform, action: Action) -> WorkerEvent {
    let exe = std::env::current_exe().unwrap_or_default();
    let (result, reboot) = match run_action(platform, &action, &exe) {
        Ok(summary) => (Ok(()), summary.reboot_recommended),
        Err(e) => (Err(e), false),
    };
    WorkerEvent::ActionFinished {
        action,
        result,
        reboot_change_at: reboot.then(|| to_unix_secs(SystemTime::now())),
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        drop(self.tx.take()); // closes the channel → worker loop ends
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Os;
    use crate::system::startup::fake::FakeStartup;
    use crate::tweak::fake::FakeTweak;
    use crate::tweak::{Tweak, TweakId, TweakState};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    fn spawn(tweaks: Vec<FakeTweak>, elevator: Elevator) -> (Worker, Arc<AtomicUsize>) {
        let notified = Arc::new(AtomicUsize::new(0));
        let n = notified.clone();
        let worker = Worker::spawn(
            move || Platform {
                os: Os::Windows,
                tweaks: tweaks
                    .into_iter()
                    .map(|t| Box::new(t) as Box<dyn Tweak>)
                    .collect(),
                startup: Box::new(FakeStartup::default()),
            },
            true,
            elevator,
            move || {
                n.fetch_add(1, Ordering::SeqCst);
            },
        );
        (worker, notified)
    }

    fn no_elevator() -> Elevator {
        Box::new(|_| panic!("elevator must not be used"))
    }

    #[test]
    fn refresh_sends_status_and_notifies() {
        let t = FakeTweak::new(TweakId::WinNagle, "N", TweakState::Optimized);
        let (w, notified) = spawn(vec![t], no_elevator());
        w.send(WorkerCmd::Refresh {
            autotuning_changed_at: None,
        });
        match w.recv() {
            Some(WorkerEvent::Status(r)) => {
                assert_eq!(r.overall, "optimized");
                assert_eq!(r.startup, Some(false));
                assert!(r.elevated);
            }
            other => panic!("unexpected {other:?}"),
        }
        // notify() runs right after the event is sent, so give it a moment.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while notified.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert_eq!(notified.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn run_applies_and_reports_reboot() {
        let t = FakeTweak::new(TweakId::WinAutoTuning, "A", TweakState::NotOptimized).needing_reboot();
        let (w, _) = spawn(vec![t.clone()], no_elevator());
        w.send(WorkerCmd::Run(Action::ApplyAll));
        match w.recv() {
            Some(WorkerEvent::ActionFinished {
                action,
                result,
                reboot_change_at,
            }) => {
                assert_eq!(action, Action::ApplyAll);
                assert_eq!(result, Ok(()));
                assert!(reboot_change_at.is_some());
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(t.current(), TweakState::Optimized);
    }

    #[test]
    fn run_failure_is_reported() {
        let t = FakeTweak::new(TweakId::WinNagle, "N", TweakState::NotOptimized).failing();
        let (w, _) = spawn(vec![t], no_elevator());
        w.send(WorkerCmd::Run(Action::ApplyAll));
        match w.recv() {
            Some(WorkerEvent::ActionFinished {
                result: Err(e),
                reboot_change_at: None,
                ..
            }) => {
                assert!(e.starts_with("N: "), "{e}")
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn run_elevated_uses_cli_args() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        let elevator: Elevator = Box::new(move |args| {
            s.lock().unwrap().push(args.to_vec());
            Err("cancelled".into())
        });
        let (w, _) = spawn(vec![], elevator);
        let action = Action::SetTweak {
            id: TweakId::LinuxQuickAck,
            optimized: true,
        };
        w.send(WorkerCmd::RunElevated(action));
        match w.recv() {
            Some(WorkerEvent::ActionFinished { result, .. }) => assert_eq!(result, Err("cancelled".into())),
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(*seen.lock().unwrap(), vec![action.cli_args()]);
    }

    #[test]
    fn dropping_the_worker_stops_the_thread() {
        let (w, _) = spawn(vec![], no_elevator());
        drop(w); // must not hang
    }
}
