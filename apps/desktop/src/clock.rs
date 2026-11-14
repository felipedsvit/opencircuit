//! Real time to simulation ticks.
//!
//! The simulator has no clock: it runs whole ticks when asked. This module
//! turns the real time between two frames into a number of ticks at the
//! chosen speed, carrying the remainder to the next frame. All arithmetic is
//! on integer microseconds; the frame time from the GUI is converted once,
//! with a guard against NaN and absurd values.

#![cfg_attr(not(test), deny(clippy::arithmetic_side_effects))]

/// Most ticks run in one frame. Time beyond it is dropped, so a slow frame
/// or a stall never makes the next frame run an unbounded burst.
pub const MAX_TICKS_PER_FRAME: u32 = 10_000;
/// Longest real interval one frame accounts for, in microseconds.
pub const MAX_FRAME_US: u64 = 250_000;

/// Simulated time per real time, as a ratio.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Speed {
    /// Numerator.
    pub simulated: u64,
    /// Denominator; never zero.
    pub real: u64,
}

impl Speed {
    /// A short label such as `×0.5` or `×10`.
    #[must_use]
    pub fn label(self) -> String {
        match (self.simulated, self.real) {
            (simulated, 1) => format!("×{simulated}"),
            (1, 2) => "×0.5".to_owned(),
            (1, 4) => "×0.25".to_owned(),
            (1, 10) => "×0.1".to_owned(),
            (simulated, real) => format!("×{simulated}/{real}"),
        }
    }
}

/// The speeds offered, slowest first.
pub const SPEEDS: [Speed; 7] = [
    Speed {
        simulated: 1,
        real: 10,
    },
    Speed {
        simulated: 1,
        real: 4,
    },
    Speed {
        simulated: 1,
        real: 2,
    },
    Speed {
        simulated: 1,
        real: 1,
    },
    Speed {
        simulated: 2,
        real: 1,
    },
    Speed {
        simulated: 5,
        real: 1,
    },
    Speed {
        simulated: 10,
        real: 1,
    },
];

/// Index of real time in [`SPEEDS`].
const REAL_TIME: usize = 3;

/// Converts frame time to ticks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SimClock {
    speed: usize,
    /// Simulated microseconds not yet turned into a tick.
    carry_us: u64,
}

impl Default for SimClock {
    fn default() -> Self {
        Self {
            speed: REAL_TIME,
            carry_us: 0,
        }
    }
}

impl SimClock {
    /// The current speed.
    #[must_use]
    pub fn speed(&self) -> Speed {
        SPEEDS.get(self.speed).copied().unwrap_or(Speed {
            simulated: 1,
            real: 1,
        })
    }

    /// Index of the current speed in [`SPEEDS`].
    #[must_use]
    pub const fn speed_index(&self) -> usize {
        self.speed
    }

    /// Choose a speed by its index in [`SPEEDS`]; an index out of range is
    /// ignored.
    pub fn set_speed_index(&mut self, index: usize) {
        if index < SPEEDS.len() {
            self.speed = index;
        }
    }

    /// Forget the time not yet run.
    pub fn reset(&mut self) {
        self.carry_us = 0;
    }

    /// Microseconds of a frame time given in seconds. Not finite, negative
    /// or zero gives zero; more than [`MAX_FRAME_US`] is cut to it.
    #[must_use]
    pub fn frame_us(seconds: f32) -> u64 {
        if !seconds.is_finite() || seconds <= 0.0 {
            return 0;
        }
        let micros = (f64::from(seconds) * 1.0e6).round().min(1.0e9);
        // Finite, positive and at most 1e9: the cast is exact.
        let micros = micros as u64;
        micros.min(MAX_FRAME_US)
    }

    /// Account for `real_us` microseconds of real time and return how many
    /// ticks of `tick_us` microseconds to run now. At most
    /// [`MAX_TICKS_PER_FRAME`]; time beyond it is dropped.
    pub fn advance(&mut self, real_us: u64, tick_us: u32) -> u32 {
        let tick = u64::from(tick_us);
        if tick == 0 {
            return 0;
        }
        let speed = self.speed();
        let simulated = real_us
            .min(MAX_FRAME_US)
            .saturating_mul(speed.simulated)
            .checked_div(speed.real)
            .unwrap_or(0);
        let total = self.carry_us.saturating_add(simulated);
        let ticks = total.checked_div(tick).unwrap_or(0);
        match u32::try_from(ticks) {
            Ok(ticks) if ticks <= MAX_TICKS_PER_FRAME => {
                self.carry_us = total.checked_rem(tick).unwrap_or(0);
                ticks
            }
            _ => {
                self.carry_us = 0;
                MAX_TICKS_PER_FRAME
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_time_runs_one_tick_per_tick_duration() {
        let mut clock = SimClock::default();

        assert_eq!(clock.advance(16_000, 1_000), 16);
    }

    #[test]
    fn remainder_is_carried_to_the_next_frame() {
        let mut clock = SimClock::default();

        let first = clock.advance(1_500, 1_000);
        let second = clock.advance(1_500, 1_000);

        assert_eq!((first, second), (1, 2));
    }

    #[test]
    fn slow_speed_runs_fewer_ticks() {
        let mut clock = SimClock::default();
        clock.set_speed_index(0);

        let ticks: u32 = (0..10).map(|_| clock.advance(10_000, 1_000)).sum();

        assert_eq!(ticks, 10);
    }

    #[test]
    fn fast_speed_runs_more_ticks() {
        let mut clock = SimClock::default();
        clock.set_speed_index(SPEEDS.len() - 1);

        assert_eq!(clock.advance(16_000, 1_000), 160);
    }

    #[test]
    fn long_frame_is_cut_and_never_bursts() {
        let mut clock = SimClock::default();
        clock.set_speed_index(SPEEDS.len() - 1);

        let ticks = clock.advance(u64::MAX, 1);

        assert_eq!(ticks, MAX_TICKS_PER_FRAME);
        assert_eq!(clock.advance(0, 1), 0);
    }

    #[test]
    fn zero_tick_duration_runs_nothing() {
        let mut clock = SimClock::default();

        assert_eq!(clock.advance(16_000, 0), 0);
    }

    #[test]
    fn invalid_frame_times_count_as_no_time() {
        for seconds in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0, 0.0] {
            assert_eq!(SimClock::frame_us(seconds), 0, "{seconds}");
        }
    }

    #[test]
    fn frame_time_is_converted_to_microseconds_and_capped() {
        assert_eq!(SimClock::frame_us(0.016), 16_000);
        assert_eq!(SimClock::frame_us(1.0e30), MAX_FRAME_US);
    }

    #[test]
    fn speed_index_out_of_range_is_ignored() {
        let mut clock = SimClock::default();

        clock.set_speed_index(99);

        assert_eq!(clock.speed_index(), REAL_TIME);
    }

    #[test]
    fn labels_read_as_multipliers() {
        let labels: Vec<String> = SPEEDS.iter().map(|speed| speed.label()).collect();

        assert_eq!(labels, ["×0.1", "×0.25", "×0.5", "×1", "×2", "×5", "×10"]);
    }

    #[test]
    fn reset_forgets_the_carried_time() {
        let mut clock = SimClock::default();
        clock.advance(900, 1_000);

        clock.reset();

        assert_eq!(clock.advance(500, 1_000), 0);
    }
}
