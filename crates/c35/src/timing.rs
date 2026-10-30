//! Fixed-rate simulation clock, independent of render cadence.
//!
//! Time accumulates as integer nanoseconds multiplied by the tick rate.
//! This avoids rounding a 120 Hz tick to 8,333,333 ns and accumulating
//! drift. Catch-up work is bounded; discarded whole ticks are reported,
//! and the fractional remainder remains available for interpolation.

use std::ops::Range;
use std::time::Duration;

const SECOND: u128 = 1_000_000_000;

#[derive(Clone, Debug)]
pub struct StepBatch {
    /// Monotonic simulation tick indices to execute, in order.
    pub ticks: Range<u64>,
    /// Blend previous to current simulation state for presentation.
    pub alpha: f32,
    /// Wall time discarded to keep this frame's catch-up bounded.
    pub dropped: Duration,
}

#[derive(Clone, Debug)]
pub struct FixedClock {
    hz: u32,
    max_steps: u32,
    remainder: u128,
    tick: u64,
    dropped: Duration,
}

impl FixedClock {
    /// `hz` and `max_steps` must be nonzero. Typical setup: 120 Hz,
    /// at most eight ticks per rendered frame.
    pub fn new(hz: u32, max_steps: u32) -> Self {
        assert!(hz > 0 && max_steps > 0, "fixed clock requires nonzero rate and budget");
        Self { hz, max_steps, remainder: 0, tick: 0, dropped: Duration::ZERO }
    }

    pub fn step_seconds(&self) -> f32 { 1.0 / self.hz as f32 }
    pub fn tick(&self) -> u64 { self.tick }
    pub fn dropped_time(&self) -> Duration { self.dropped }

    pub fn advance(&mut self, elapsed: Duration) -> StepBatch {
        self.remainder += elapsed.as_nanos() * u128::from(self.hz);
        let available = self.remainder / SECOND;
        self.remainder %= SECOND;
        let steps = available.min(u128::from(self.max_steps)) as u64;
        let discarded = available - u128::from(steps);
        let dropped_nanos = discarded * SECOND / u128::from(self.hz);
        let dropped = Duration::new(
            (dropped_nanos / SECOND).min(u128::from(u64::MAX)) as u64,
            (dropped_nanos % SECOND) as u32,
        );
        self.dropped = self.dropped.saturating_add(dropped);
        let start = self.tick;
        self.tick = self.tick.saturating_add(steps);
        StepBatch { ticks: start..self.tick, alpha: self.remainder as f32 / SECOND as f32, dropped }
    }

    /// Discard the fractional backlog when resuming from a pause.
    /// Simulation tick identity and the accumulated cost ledger survive.
    pub fn reset_accumulator(&mut self) { self.remainder = 0; }
}

impl Default for FixedClock {
    fn default() -> Self { Self::new(120, 8) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cadence_does_not_change_tick_count() {
        for fps in [30u32, 60, 90, 120, 144, 240] {
            let mut clock = FixedClock::new(120, 8);
            // Partition exactly one second, including the division remainder.
            for i in 0..fps {
                let a = u64::from(i) * 1_000_000_000 / u64::from(fps);
                let b = u64::from(i + 1) * 1_000_000_000 / u64::from(fps);
                clock.advance(Duration::from_nanos(b - a));
            }
            assert_eq!(clock.tick(), 120, "render fps {fps}");
            assert_eq!(clock.dropped_time(), Duration::ZERO);
        }
    }

    #[test]
    fn stall_is_bounded_and_accounted() {
        let mut clock = FixedClock::new(100, 8);
        let batch = clock.advance(Duration::from_millis(1005));
        assert_eq!(batch.ticks, 0..8);
        assert_eq!(batch.dropped, Duration::from_millis(920));
        assert!((batch.alpha - 0.5).abs() < 1e-6);
        assert_eq!(clock.advance(Duration::from_millis(5)).ticks, 8..9);
    }

    #[test]
    fn pause_preserves_tick_identity() {
        let mut clock = FixedClock::new(100, 8);
        clock.advance(Duration::from_millis(15));
        clock.reset_accumulator();
        assert_eq!(clock.advance(Duration::from_millis(5)).ticks, 1..1);
    }
}
