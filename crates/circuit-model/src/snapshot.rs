//! The observable state of one solved tick.
//!
//! A snapshot uses document IDs only, never the dense indices of a compiled
//! circuit, so a renderer or an editor can consume it without depending on the
//! simulator. Every list is sorted by ID.

use serde::Serialize;

use crate::diagnostics::Diagnostic;
use crate::ids::{DeviceId, JunctionId, SymbolId, WireId};
use crate::supply::ConductorSet;

/// An element together with the conductors present on it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Live<T> {
    /// The element.
    pub id: T,
    /// Conductors present; never empty in a snapshot.
    pub conductors: ConductorSet,
}

/// What a simplified motor is doing. No speed, torque or current.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MotorState {
    /// Not running.
    Stopped,
    /// Running in the direction of the supply phase sequence.
    Forward,
    /// Running against the supply phase sequence.
    Reverse,
}

/// State of one motor.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct MotorView {
    /// The motor device.
    pub device: DeviceId,
    /// What it is doing.
    pub state: MotorState,
}

/// State of one timer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct TimerView {
    /// The timer device.
    pub device: DeviceId,
    /// Ticks counted at the end of the tick.
    pub elapsed_ticks: u64,
    /// Preset in ticks.
    pub preset_ticks: u64,
    /// Output effective during the tick.
    pub out: bool,
}

/// State actually solved in one tick, before its results were committed to
/// the next tick.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SimSnapshot {
    /// The tick this snapshot describes.
    pub tick: u64,
    /// Virtual time at the start of the tick, in microseconds.
    pub t_us: u64,
    /// Devices with a load that is energized in this tick.
    pub energized_devices: Vec<DeviceId>,
    /// Contactors and relays that are picked up, and timers whose output is
    /// high. Differs from `energized_devices` during the actuation delay.
    pub active_devices: Vec<DeviceId>,
    /// Pushbuttons held and latching switches closed.
    pub actuated_devices: Vec<DeviceId>,
    /// Thermal overload relays that are tripped.
    pub tripped_devices: Vec<DeviceId>,
    /// Driven contacts that were closed when the network was solved.
    pub closed_symbols: Vec<SymbolId>,
    /// Wires carrying at least one conductor.
    pub live_wires: Vec<Live<WireId>>,
    /// Junctions carrying at least one conductor.
    pub live_junctions: Vec<Live<JunctionId>>,
    /// Every motor.
    pub motors: Vec<MotorView>,
    /// Every timer.
    pub timers: Vec<TimerView>,
    /// Whether the runtime is latched by a fault.
    pub faulted: bool,
    /// Faults active in this tick.
    pub faults: Vec<Diagnostic>,
}
