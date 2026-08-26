use std::time::{Duration, Instant};

/// Monotonic wall-clock timer. A zero limit is immediately complete.
pub struct Timer {
    start: Instant,
    limit: Duration,
}

impl Timer {
    pub fn new(limit_ms: u64) -> Self {
        Self {
            start: Instant::now(),
            limit: Duration::from_millis(limit_ms),
        }
    }

    #[inline]
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    #[inline]
    pub fn remaining(&self) -> Duration {
        self.limit.saturating_sub(self.start.elapsed())
    }

    #[inline]
    pub fn progress(&self) -> f64 {
        if self.limit.is_zero() {
            1.0
        } else {
            (self.start.elapsed().as_secs_f64() / self.limit.as_secs_f64()).min(1.0)
        }
    }

    #[inline]
    pub fn is_over(&self) -> bool {
        self.start.elapsed() >= self.limit
    }
}

#[cfg(test)]
mod tests {
    use super::Timer;

    #[test]
    fn progress_and_remaining_are_bounded() {
        let timer = Timer::new(10);
        assert!((0.0..=1.0).contains(&timer.progress()));
        assert!(timer.remaining().as_nanos() <= 10_000_000);
        let zero = Timer::new(0);
        assert_eq!(zero.progress(), 1.0);
        assert_eq!(zero.remaining().as_nanos(), 0);
    }
}
