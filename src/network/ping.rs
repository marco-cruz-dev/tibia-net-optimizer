//! Rolling latency / jitter / packet-loss statistics.
//!
//! Pure data structure; the probers (`network::probe`) and the monitor
//! thread (`app::ping`) feed samples into [`LatencyStats::record`].

use std::collections::VecDeque;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct LatencyStats {
    capacity: usize,
    /// `None` = lost probe (timeout).
    samples: VecDeque<Option<f64>>,
}

impl LatencyStats {
    /// Keeps the most recent `capacity` samples.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be > 0");
        Self {
            capacity,
            samples: VecDeque::with_capacity(capacity),
        }
    }

    /// Records a round-trip time, or `None` for a lost probe.
    pub fn record(&mut self, rtt: Option<Duration>) {
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(rtt.map(|d| d.as_secs_f64() * 1000.0));
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Oldest-first samples in ms (`None` = lost), for the sparkline.
    pub fn samples_ms(&self) -> Vec<Option<f64>> {
        self.samples.iter().copied().collect()
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    fn successes(&self) -> impl Iterator<Item = f64> + '_ {
        self.samples.iter().flatten().copied()
    }

    /// Latest sample; `None` if empty or the latest probe was lost.
    pub fn current_ms(&self) -> Option<f64> {
        self.samples.back().copied().flatten()
    }

    pub fn average_ms(&self) -> Option<f64> {
        let (sum, n) = self.successes().fold((0.0, 0usize), |(s, n), x| (s + x, n + 1));
        (n > 0).then(|| sum / n as f64)
    }

    /// Mean absolute difference between consecutive successful samples
    /// (lost probes are skipped). Needs at least two successes.
    pub fn jitter_ms(&self) -> Option<f64> {
        let ok: Vec<f64> = self.successes().collect();
        if ok.len() < 2 {
            return None;
        }
        let total: f64 = ok.windows(2).map(|w| (w[1] - w[0]).abs()).sum();
        Some(total / (ok.len() - 1) as f64)
    }

    /// Percentage of lost probes in the window (0.0 when empty).
    pub fn packet_loss_pct(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let lost = self.samples.iter().filter(|s| s.is_none()).count();
        lost as f64 * 100.0 / self.samples.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(v: u64) -> Option<Duration> {
        Some(Duration::from_millis(v))
    }

    fn approx(a: Option<f64>, b: f64) {
        let a = a.expect("expected a value");
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    #[test]
    fn empty_stats_have_no_values() {
        let s = LatencyStats::new(10);
        assert!(s.is_empty());
        assert_eq!(s.current_ms(), None);
        assert_eq!(s.average_ms(), None);
        assert_eq!(s.jitter_ms(), None);
        assert_eq!(s.packet_loss_pct(), 0.0);
    }

    #[test]
    fn current_and_average() {
        let mut s = LatencyStats::new(10);
        for v in [100, 110, 90] {
            s.record(ms(v));
        }
        approx(s.current_ms(), 90.0);
        approx(s.average_ms(), 100.0);
    }

    #[test]
    fn jitter_is_mean_abs_delta_between_consecutive_samples() {
        let mut s = LatencyStats::new(10);
        for v in [100, 110, 90, 95] {
            s.record(ms(v));
        }
        // |10| + |-20| + |5| = 35 / 3
        approx(s.jitter_ms(), 35.0 / 3.0);
    }

    #[test]
    fn jitter_needs_two_successful_samples() {
        let mut s = LatencyStats::new(10);
        s.record(ms(100));
        s.record(None);
        assert_eq!(s.jitter_ms(), None);
    }

    #[test]
    fn lost_probes_count_as_loss_and_are_skipped_in_latency() {
        let mut s = LatencyStats::new(10);
        s.record(ms(100));
        s.record(None);
        s.record(ms(120));
        s.record(None);
        assert_eq!(s.current_ms(), None);
        approx(s.average_ms(), 110.0);
        approx(s.jitter_ms(), 20.0);
        assert_eq!(s.packet_loss_pct(), 50.0);
    }

    #[test]
    fn window_drops_oldest_samples() {
        let mut s = LatencyStats::new(3);
        for v in [1000, 100, 100, 100] {
            s.record(ms(v));
        }
        assert_eq!(s.len(), 3);
        approx(s.average_ms(), 100.0);
        approx(s.jitter_ms(), 0.0);
    }

    #[test]
    fn exposes_samples_oldest_first_and_clears() {
        let mut s = LatencyStats::new(3);
        for v in [ms(10), None, ms(30), ms(40)] {
            s.record(v);
        }
        assert_eq!(s.samples_ms(), vec![None, Some(30.0), Some(40.0)]);
        s.clear();
        assert!(s.is_empty());
        assert_eq!(s.samples_ms(), vec![]);
    }

    #[test]
    #[should_panic]
    fn zero_capacity_is_rejected() {
        LatencyStats::new(0);
    }
}
