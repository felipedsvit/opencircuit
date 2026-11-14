//! Device logic: actuation delay of coils and the TON/TOF timers.
//!
//! Every function here is pure. It reads the state that was effective during
//! a tick and returns the state for the next tick; nothing takes effect in
//! the tick that computed it. That one-tick minimum is structural: with no
//! delay a coil that closes its own seal-in contact would be a combinational
//! loop with no defined solution.

use circuit_model::TimerMode;

/// Consecutive-tick counters of one contactor or relay coil.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct ActuatorState {
    /// Consecutive ticks the coil was energized.
    pub on_run: u32,
    /// Consecutive ticks the coil was de-energized.
    pub off_run: u32,
}

/// Advance a coil by one tick.
///
/// `active` is whether the device was picked up during this tick and
/// `energized` whether its coil was energized in this tick's solution. The
/// device picks up after `pickup_ticks` consecutive energized ticks and drops
/// out after `dropout_ticks` consecutive de-energized ticks. Returns the new
/// counters and whether the device is picked up in the next tick.
#[must_use]
pub fn step_actuator(
    state: ActuatorState,
    active: bool,
    energized: bool,
    pickup_ticks: u32,
    dropout_ticks: u32,
) -> (ActuatorState, bool) {
    if energized {
        let on_run = state.on_run.saturating_add(1).min(pickup_ticks);
        let next = ActuatorState { on_run, off_run: 0 };
        (next, active || on_run >= pickup_ticks)
    } else {
        let off_run = state.off_run.saturating_add(1).min(dropout_ticks);
        let next = ActuatorState { on_run: 0, off_run };
        (next, active && off_run < dropout_ticks)
    }
}

/// Counter of one timer.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct TimerState {
    /// Ticks counted towards the preset.
    pub elapsed_ticks: u64,
}

/// Advance a timer by one tick.
///
/// `input` is whether the timer coil was energized in this tick's solution
/// and `out` the output that was effective during this tick. The input is
/// evaluated before the counter advances, so an input change in the tick that
/// would reach the preset wins. Returns the new counter and the output for
/// the next tick.
#[must_use]
pub fn step_timer(
    mode: TimerMode,
    preset_ticks: u64,
    state: TimerState,
    input: bool,
    out: bool,
) -> (TimerState, bool) {
    let advance = || state.elapsed_ticks.saturating_add(1).min(preset_ticks);
    match mode {
        TimerMode::OnDelay => {
            if input {
                let elapsed_ticks = advance();
                (TimerState { elapsed_ticks }, elapsed_ticks >= preset_ticks)
            } else {
                (TimerState::default(), false)
            }
        }
        TimerMode::OffDelay => {
            if input {
                (TimerState::default(), true)
            } else if out {
                let elapsed_ticks = advance();
                (TimerState { elapsed_ticks }, elapsed_ticks < preset_ticks)
            } else {
                (TimerState::default(), false)
            }
        }
    }
}

/// A timer preset converted to ticks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresetTicks {
    /// Preset in ticks, at least 1.
    pub ticks: u64,
    /// Whether the preset was not a multiple of the tick and was rounded up.
    pub quantized: bool,
}

/// Convert a preset in milliseconds to ticks, rounding up. Returns `None`
/// for a zero tick duration.
#[must_use]
pub fn preset_ticks(preset_ms: u32, tick_us: u32) -> Option<PresetTicks> {
    if tick_us == 0 {
        return None;
    }
    let preset_us = u64::from(preset_ms).saturating_mul(1_000);
    let tick_us = u64::from(tick_us);
    Some(PresetTicks {
        ticks: preset_us.div_ceil(tick_us).max(1),
        quantized: preset_us.checked_rem(tick_us).is_some_and(|rest| rest != 0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run a timer over an input sequence. Returns, per tick, the counter at
    /// the end of the tick and the output effective during the tick.
    fn run_timer(mode: TimerMode, preset: u64, inputs: &[u8]) -> (Vec<u64>, Vec<u8>) {
        let mut state = TimerState::default();
        let mut out = false;
        let mut elapsed = Vec::new();
        let mut effective = Vec::new();
        for input in inputs {
            effective.push(u8::from(out));
            let (next_state, next_out) = step_timer(mode, preset, state, *input == 1, out);
            state = next_state;
            out = next_out;
            elapsed.push(state.elapsed_ticks);
        }
        (elapsed, effective)
    }

    /// Run a coil over an energization sequence. Returns, per tick, whether
    /// the device was picked up during the tick.
    fn run_actuator(pickup: u32, dropout: u32, energized: &[u8]) -> Vec<u8> {
        let mut state = ActuatorState::default();
        let mut active = false;
        let mut effective = Vec::new();
        for input in energized {
            effective.push(u8::from(active));
            let (next_state, next_active) =
                step_actuator(state, active, *input == 1, pickup, dropout);
            state = next_state;
            active = next_active;
        }
        effective
    }

    #[test]
    fn ton_matches_the_specification_tick_table() {
        let (elapsed, out) = run_timer(TimerMode::OnDelay, 3, &[0, 0, 1, 1, 1, 1, 0, 0, 0]);

        assert_eq!(elapsed, [0, 0, 1, 2, 3, 3, 0, 0, 0]);
        assert_eq!(out, [0, 0, 0, 0, 0, 1, 1, 0, 0]);
    }

    #[test]
    fn tof_matches_the_specification_tick_table() {
        let (elapsed, out) = run_timer(TimerMode::OffDelay, 3, &[0, 0, 1, 1, 1, 0, 0, 0, 0]);

        assert_eq!(elapsed, [0, 0, 0, 0, 0, 1, 2, 3, 0]);
        assert_eq!(out, [0, 0, 0, 1, 1, 1, 1, 1, 0]);
    }

    #[test]
    fn ton_input_dropping_before_the_preset_cancels_the_timing() {
        let (elapsed, out) = run_timer(TimerMode::OnDelay, 3, &[1, 1, 0, 1, 1, 1, 1]);

        assert_eq!(elapsed, [1, 2, 0, 1, 2, 3, 3]);
        assert_eq!(out, [0, 0, 0, 0, 0, 0, 1]);
    }

    #[test]
    fn tof_input_returning_during_the_delay_cancels_the_timing() {
        let (elapsed, out) = run_timer(TimerMode::OffDelay, 3, &[1, 0, 0, 1, 0, 0, 0, 0]);

        assert_eq!(elapsed, [0, 1, 2, 0, 1, 2, 3, 0]);
        assert_eq!(out, [0, 1, 1, 1, 1, 1, 1, 0]);
    }

    #[test]
    fn tof_that_was_never_on_stays_off() {
        let (elapsed, out) = run_timer(TimerMode::OffDelay, 2, &[0, 0, 0]);

        assert_eq!(elapsed, [0, 0, 0]);
        assert_eq!(out, [0, 0, 0]);
    }

    #[test]
    fn coil_with_unit_delays_follows_one_tick_behind() {
        assert_eq!(
            run_actuator(1, 1, &[0, 1, 1, 0, 0, 1, 0]),
            [0, 0, 1, 1, 0, 0, 1]
        );
    }

    #[test]
    fn coil_needs_consecutive_energized_ticks_to_pick_up() {
        // Two energized ticks, a gap, then three: only the run of three
        // reaches a pickup delay of three.
        assert_eq!(
            run_actuator(3, 1, &[1, 1, 0, 1, 1, 1, 1, 1]),
            [0, 0, 0, 0, 0, 0, 1, 1]
        );
    }

    #[test]
    fn coil_rides_through_a_dip_shorter_than_the_dropout_delay() {
        assert_eq!(
            run_actuator(1, 3, &[1, 1, 0, 0, 1, 0, 0, 0, 0]),
            [0, 1, 1, 1, 1, 1, 1, 1, 0]
        );
    }

    #[test]
    fn preset_is_rounded_up_to_whole_ticks_and_flagged() {
        assert_eq!(
            preset_ticks(5, 1_000),
            Some(PresetTicks {
                ticks: 5,
                quantized: false
            })
        );
        assert_eq!(
            preset_ticks(5, 2_000),
            Some(PresetTicks {
                ticks: 3,
                quantized: true
            })
        );
    }

    #[test]
    fn preset_shorter_than_one_tick_is_one_tick() {
        assert_eq!(
            preset_ticks(1, 10_000),
            Some(PresetTicks {
                ticks: 1,
                quantized: true
            })
        );
    }

    #[test]
    fn preset_conversion_does_not_overflow_or_divide_by_zero() {
        assert_eq!(preset_ticks(u32::MAX, 0), None);
        assert_eq!(
            preset_ticks(u32::MAX, 1),
            Some(PresetTicks {
                ticks: u64::from(u32::MAX) * 1_000,
                quantized: false
            })
        );
    }
}
