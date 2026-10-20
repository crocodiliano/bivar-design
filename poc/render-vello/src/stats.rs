//! Frame time statistics for the smoothness test.

/// Frame budget at 60 frames per second, in milliseconds.
pub const BUDGET_60_HZ: f64 = 1000.0 / 60.0;
/// Frame budget at 144 frames per second, in milliseconds.
pub const BUDGET_144_HZ: f64 = 1000.0 / 144.0;

#[derive(Clone, Debug, Default)]
pub struct FrameTimes {
    millis: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Summary {
    pub frames: usize,
    pub mean: f64,
    pub p50: f64,
    pub p99: f64,
    pub worst: f64,
    /// Frames slower than 16.7 ms (60 per second).
    pub over_60_hz: usize,
    /// Frames slower than 6.9 ms (144 per second).
    pub over_144_hz: usize,
}

impl FrameTimes {
    pub fn push(&mut self, millis: f64) {
        if millis.is_finite() && millis >= 0.0 {
            self.millis.push(millis);
        }
    }

    pub fn len(&self) -> usize {
        self.millis.len()
    }

    pub fn is_empty(&self) -> bool {
        self.millis.is_empty()
    }

    pub fn summary(&self) -> Option<Summary> {
        if self.millis.is_empty() {
            return None;
        }
        let mut sorted = self.millis.clone();
        sorted.sort_by(f64::total_cmp);
        let over = |budget: f64| sorted.iter().filter(|&&m| m > budget).count();
        Some(Summary {
            frames: sorted.len(),
            mean: sorted.iter().sum::<f64>() / sorted.len() as f64,
            p50: percentile(&sorted, 50.0),
            p99: percentile(&sorted, 99.0),
            worst: sorted[sorted.len() - 1],
            over_60_hz: over(BUDGET_60_HZ),
            over_144_hz: over(BUDGET_144_HZ),
        })
    }
}

/// Nearest-rank percentile of an ascending, non-empty slice.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_has_no_summary() {
        assert_eq!(FrameTimes::default().summary(), None);
    }

    #[test]
    fn summary_of_known_values() {
        let mut times = FrameTimes::default();
        for i in 1..=100 {
            times.push(f64::from(i));
        }
        let s = times.summary().unwrap();
        assert_eq!(s.frames, 100);
        assert_eq!(s.p50, 50.0);
        assert_eq!(s.p99, 99.0);
        assert_eq!(s.worst, 100.0);
        assert!((s.mean - 50.5).abs() < 1e-9);
        assert_eq!(s.over_60_hz, 100 - 16);
        assert_eq!(s.over_144_hz, 100 - 6);
    }

    #[test]
    fn invalid_samples_are_ignored() {
        let mut times = FrameTimes::default();
        times.push(f64::NAN);
        times.push(-1.0);
        times.push(f64::INFINITY);
        assert!(times.is_empty());
        times.push(5.0);
        assert_eq!(times.len(), 1);
    }

    #[test]
    fn single_sample_is_every_percentile() {
        let mut times = FrameTimes::default();
        times.push(7.0);
        let s = times.summary().unwrap();
        assert_eq!((s.p50, s.p99, s.worst), (7.0, 7.0, 7.0));
    }
}
