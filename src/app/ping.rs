//! Live connection monitor: finds the Tibia client's game connection (a
//! Cloudflare edge) and pings it on a fixed cadence. Independent from the
//! admin worker so slow OS calls never delay the cadence.

use std::net::IpAddr;
use std::sync::mpsc::{self, RecvTimeoutError, Sender, TryRecvError};
use std::time::Duration;

use super::state::{PingOutcome, PingSample};

pub const PING_INTERVAL: Duration = Duration::from_secs(2);
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(1);
/// Re-detect the game connection every N ticks (≈10 s), to follow relogs
/// and world changes.
pub const REDETECT_EVERY: u32 = 5;
/// …and immediately after this many consecutive losses.
pub const REDETECT_AFTER_FAILURES: u32 = 3;

/// One monitor step: (target, outcome).
pub type Tick = Box<dyn FnMut() -> (Option<IpAddr>, PingOutcome) + Send>;

/// Detection + probing. Keeps measuring one target and re-detects
/// periodically or after repeated losses. On (re)detection it uses the
/// current target if still present, otherwise the first candidate that
/// answers ICMP (the client can hold several game-port connections, and some
/// hosts block ICMP).
pub struct GameProbe<D, P> {
    detect: D,
    probe: P,
    target: Option<IpAddr>,
    ticks_since_detect: u32,
    failures: u32,
}

impl<D, P> GameProbe<D, P>
where
    D: FnMut() -> Vec<IpAddr>,
    P: FnMut(IpAddr) -> Result<Duration, String>,
{
    pub fn new(detect: D, probe: P) -> Self {
        Self {
            detect,
            probe,
            target: None,
            ticks_since_detect: 0,
            failures: 0,
        }
    }

    pub fn tick(&mut self) -> (Option<IpAddr>, PingOutcome) {
        let redetect = self.target.is_none()
            || self.ticks_since_detect >= REDETECT_EVERY
            || self.failures >= REDETECT_AFTER_FAILURES;
        if redetect {
            return self.redetect();
        }
        self.ticks_since_detect += 1;
        let ip = self.target.expect("checked above");
        match (self.probe)(ip) {
            Ok(rtt) => {
                self.failures = 0;
                (Some(ip), PingOutcome::Reply(rtt))
            }
            Err(e) => {
                self.failures += 1;
                (Some(ip), PingOutcome::Lost(e))
            }
        }
    }

    /// Picks a target: the current one first (stability), then the others in
    /// table order; the first that answers wins. That probe is this tick's sample.
    fn redetect(&mut self) -> (Option<IpAddr>, PingOutcome) {
        self.ticks_since_detect = 1;
        self.failures = 0;
        let candidates = (self.detect)();
        if candidates.is_empty() {
            self.target = None;
            return (None, PingOutcome::NoGame);
        }
        let mut ordered: Vec<IpAddr> = self
            .target
            .into_iter()
            .filter(|t| candidates.contains(t))
            .collect();
        ordered.extend(candidates.iter().filter(|c| Some(**c) != self.target));
        for ip in ordered {
            if let Ok(rtt) = (self.probe)(ip) {
                self.target = Some(ip);
                return (Some(ip), PingOutcome::Reply(rtt));
            }
        }
        self.target = None;
        (None, PingOutcome::NotPingable(candidates))
    }
}

/// Stops when dropped. The thread is not joined, so stopping never blocks
/// the UI on an in-flight probe; any late sample carries an old generation
/// and is ignored by the reducer.
pub struct PingMonitor {
    _stop: Sender<()>,
}

impl PingMonitor {
    pub fn start<S>(generation: u64, mut tick: Tick, interval: Duration, on_sample: S) -> PingMonitor
    where
        S: Fn(PingSample) + Send + 'static,
    {
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        std::thread::Builder::new()
            .name("tno-ping".into())
            .spawn(move || loop {
                let (target, outcome) = tick();
                // Stopped while probing: drop the sample.
                if let Err(TryRecvError::Disconnected) = stop_rx.try_recv() {
                    break;
                }
                on_sample(PingSample {
                    generation,
                    target,
                    outcome,
                });
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
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    const A: &str = "2606:4700:90:0:4b6e:7f99:a2f2:fd80";
    const B: &str = "2606:4700:90::1";

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    const AWS: &str = "18.192.74.45"; // blocks ICMP (measured)

    struct Script {
        detect_calls: Rc<RefCell<usize>>,
        probes: Rc<RefCell<Vec<IpAddr>>>,
        /// Addresses that currently don't answer ICMP.
        blocked: Rc<RefCell<Vec<IpAddr>>>,
    }

    /// Detection results are scripted (the last one repeats); probes answer
    /// 11 ms unless the address is in `blocked`.
    #[allow(clippy::type_complexity)]
    fn scripted(
        detections: Vec<Vec<&'static str>>,
        blocked: &[&str],
    ) -> (
        GameProbe<impl FnMut() -> Vec<IpAddr>, impl FnMut(IpAddr) -> Result<Duration, String>>,
        Script,
    ) {
        let script = Script {
            detect_calls: Rc::new(RefCell::new(0)),
            probes: Rc::new(RefCell::new(Vec::new())),
            blocked: Rc::new(RefCell::new(blocked.iter().map(|s| ip(s)).collect())),
        };
        let (calls, probes, blocked) = (
            script.detect_calls.clone(),
            script.probes.clone(),
            script.blocked.clone(),
        );
        let mut detections = detections.into_iter();
        let mut last: Vec<IpAddr> = Vec::new();
        let probe = GameProbe::new(
            move || {
                *calls.borrow_mut() += 1;
                if let Some(d) = detections.next() {
                    last = d.into_iter().map(ip).collect();
                }
                last.clone()
            },
            move |addr| {
                probes.borrow_mut().push(addr);
                if blocked.borrow().contains(&addr) {
                    Err("timed out".into())
                } else {
                    Ok(Duration::from_millis(11))
                }
            },
        );
        (probe, script)
    }

    fn reply() -> PingOutcome {
        PingOutcome::Reply(Duration::from_millis(11))
    }

    #[test]
    fn no_client_means_no_game_and_keeps_looking() {
        let (mut p, s) = scripted(vec![vec![]], &[]);
        assert_eq!(p.tick(), (None, PingOutcome::NoGame));
        assert_eq!(p.tick(), (None, PingOutcome::NoGame));
        assert_eq!(*s.detect_calls.borrow(), 2, "detects every tick until found");
    }

    #[test]
    fn measures_the_first_candidate_that_answers_icmp() {
        // The case that broke: AWS (blocked) is listed before the Cloudflare edge.
        let (mut p, s) = scripted(vec![vec![AWS, A]], &[AWS]);
        assert_eq!(p.tick(), (Some(ip(A)), reply()));
        assert_eq!(p.tick(), (Some(ip(A)), reply()));
        assert_eq!(*s.detect_calls.borrow(), 1, "cached between re-detections");
        assert_eq!(*s.probes.borrow(), vec![ip(AWS), ip(A), ip(A)]);
    }

    #[test]
    fn no_answering_candidate_is_not_pingable_rather_than_loss() {
        let (mut p, _) = scripted(vec![vec![AWS]], &[AWS]);
        assert_eq!(p.tick(), (None, PingOutcome::NotPingable(vec![ip(AWS)])));
    }

    #[test]
    fn redetection_keeps_the_current_target_when_still_present() {
        let (mut p, s) = scripted(vec![vec![A, B], vec![B, A]], &[]);
        assert_eq!(p.tick().0, Some(ip(A)));
        for _ in 1..REDETECT_EVERY {
            p.tick();
        }
        assert_eq!(p.tick().0, Some(ip(A)), "stable across re-detection");
        assert_eq!(*s.detect_calls.borrow(), 2);
    }

    #[test]
    fn redetects_periodically_to_follow_world_changes() {
        let (mut p, s) = scripted(vec![vec![A], vec![B]], &[]);
        for _ in 0..REDETECT_EVERY {
            assert_eq!(p.tick().0, Some(ip(A)));
        }
        assert_eq!(
            p.tick().0,
            Some(ip(B)),
            "re-detected after {REDETECT_EVERY} ticks"
        );
        assert_eq!(*s.detect_calls.borrow(), 2);
    }

    #[test]
    fn repeated_losses_switch_to_another_answering_candidate() {
        let (mut p, s) = scripted(vec![vec![A, B]], &[]);
        assert_eq!(p.tick(), (Some(ip(A)), reply()));
        s.blocked.borrow_mut().push(ip(A)); // A stops answering
        for _ in 0..REDETECT_AFTER_FAILURES {
            assert_eq!(p.tick(), (Some(ip(A)), PingOutcome::Lost("timed out".into())));
        }
        assert_eq!(
            p.tick(),
            (Some(ip(B)), reply()),
            "switched after {REDETECT_AFTER_FAILURES} losses"
        );
    }

    #[test]
    fn monitor_reports_ticks_with_generation_and_stops_on_drop() {
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let (tx, rx) = mpsc::channel();
        let m = PingMonitor::start(
            7,
            Box::new(move || {
                c.fetch_add(1, Ordering::SeqCst);
                (Some(ip(A)), PingOutcome::Reply(Duration::from_millis(11)))
            }),
            Duration::from_millis(5),
            move |s| {
                let _ = tx.send(s);
            },
        );
        let s = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            s,
            PingSample {
                generation: 7,
                target: Some(ip(A)),
                outcome: PingOutcome::Reply(Duration::from_millis(11))
            }
        );
        drop(m);
        std::thread::sleep(Duration::from_millis(50));
        let after_stop = count.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(count.load(Ordering::SeqCst), after_stop, "no probes after stop");
    }
}
