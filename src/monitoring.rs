use std::{collections::VecDeque, time::Duration};

#[derive(Clone, Copy)]
pub struct Sample {
    pub elapsed: f64,
    pub fps: f32,
    pub peak: f32,
}

#[derive(Default)]
pub struct Monitor {
    pub samples: VecDeque<Sample>,
}

impl Monitor {
    pub fn record(&mut self, elapsed: Duration, fps: f64, peak: f32) {
        let elapsed = elapsed.as_secs_f64();
        if self
            .samples
            .back()
            .is_some_and(|last| elapsed - last.elapsed < 0.5)
        {
            return;
        }
        self.samples.push_back(Sample {
            elapsed,
            fps: if fps.is_finite() {
                fps.max(0.0) as f32
            } else {
                0.0
            },
            peak: if peak.is_finite() {
                peak.clamp(0.0, 1.0)
            } else {
                0.0
            },
        });
        while self.samples.len() > 120 {
            self.samples.pop_front();
        }
    }
    pub fn reset(&mut self) {
        self.samples.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn histories_are_bounded_sampled_and_reset_for_each_session() {
        let mut monitor = Monitor::default();
        for millis in (0..120_000).step_by(100) {
            monitor.record(Duration::from_millis(millis), 60.0, 0.5);
        }
        assert_eq!(monitor.samples.len(), 120);
        assert_eq!(monitor.samples.back().unwrap().elapsed, 119.5);
        assert_eq!(monitor.samples.front().unwrap().elapsed, 60.0);
        monitor.reset();
        assert!(monitor.samples.is_empty());
        monitor.record(Duration::ZERO, f64::NAN, f32::INFINITY);
        assert_eq!(monitor.samples[0].fps, 0.0);
        assert_eq!(monitor.samples[0].peak, 0.0);
    }
}
