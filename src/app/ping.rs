//! Live latency monitor thread: probes on a fixed cadence and reports each
//! sample. Independent from the admin worker so slow OS calls never delay
//! the ping cadence.

use std::sync::mpsc::{self, RecvTimeoutError, Sender, TryRecvError};
use std::time::Duration;

use super::state::PingSample;

/// One probe: round-trip time or an error (counted as loss).
pub type ProbeFn = Box<dyn Fn() -> Result<Duration, String> + Send>;

pub const PING_INTERVAL: Duration = Duration::from_secs(2);
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(1);

/// Stops when dropped. The thread is not joined, so stopping never blocks
/// the UI on an in-flight probe; any late sample carries an old generation
/// and is ignored by the reducer.
pub struct PingMonitor {
    _stop: Sender<()>,
}

impl PingMonitor {
    pub fn start<S>(generation: u64, probe: ProbeFn, interval: Duration, on_sample: S) -> PingMonitor
    where
        S: Fn(PingSample) + Send + 'static,
    {
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        std::thread::Builder::new()
            .name("tno-ping".into())
            .spawn(move || loop {
                let sample = match probe() {
                    Ok(rtt) => PingSample {
                        generation,
                        rtt: Some(rtt),
                        error: None,
                    },
                    Err(e) => PingSample {
                        generation,
                        rtt: None,
                        error: Some(e),
                    },
                };
                // Stopped while probing: drop the sample.
                if let Err(TryRecvError::Disconnected) = stop_rx.try_recv() {
                    break;
                }
                on_sample(sample);
                match stop_rx.recv_timeout(interval) {
                    Err(RecvTimeoutError::Timeout) => continue,
                    _ => break,
                }
            })
            .expect("failed to spawn ping thread");
        PingMonitor { _stop: stop_tx }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn collect(monitor_gen: u64, probe: ProbeFn) -> (PingMonitor, mpsc::Receiver<PingSample>) {
        let (tx, rx) = mpsc::channel();
        let m = PingMonitor::start(monitor_gen, probe, Duration::from_millis(5), move |s| {
            let _ = tx.send(s);
        });
        (m, rx)
    }

    fn next(rx: &mpsc::Receiver<PingSample>) -> PingSample {
        rx.recv_timeout(Duration::from_secs(5)).expect("sample")
    }

    #[test]
    fn reports_successful_probes_with_generation() {
        let (_m, rx) = collect(7, Box::new(|| Ok(Duration::from_millis(42))));
        for _ in 0..3 {
            assert_eq!(
                next(&rx),
                PingSample {
                    generation: 7,
                    rtt: Some(Duration::from_millis(42)),
                    error: None
                }
            );
        }
    }

    #[test]
    fn failed_probes_are_losses_with_reason() {
        let (_m, rx) = collect(1, Box::new(|| Err("timed out".into())));
        assert_eq!(
            next(&rx),
            PingSample {
                generation: 1,
                rtt: None,
                error: Some("timed out".into())
            }
        );
    }

    #[test]
    fn dropping_stops_probing() {
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let (m, rx) = collect(
            1,
            Box::new(move || {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(Duration::from_millis(1))
            }),
        );
        next(&rx);
        drop(m);
        std::thread::sleep(Duration::from_millis(50));
        let after_stop = count.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(count.load(Ordering::SeqCst), after_stop, "no probes after stop");
    }
}
