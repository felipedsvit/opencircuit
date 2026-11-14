//! Simulation core of OpenCircuit.
//!
//! A schematic is compiled once into an executable circuit: pins and
//! junctions joined by wires become static copper nets, contacts become
//! switches between nets, and coils, lamps and motors become loads. Each tick
//! the solver joins nets through the closed switches and decides which loads
//! are energized; the device logic then produces the signals of the next
//! tick.
//!
//! The model is discrete: continuity and energization only. Circuits it
//! cannot represent are rejected with a diagnostic, never approximated.
//!
//! The crate is headless and deterministic. It uses virtual time, integers
//! and booleans only, and depends on nothing but `circuit-model`.

#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]
#![cfg_attr(
    not(test),
    deny(clippy::indexing_slicing, clippy::arithmetic_side_effects)
)]

pub mod bitset;
pub mod compile;
pub mod compiled;
pub mod fingerprint;
pub mod logic;
pub mod runtime;
pub mod solve;
pub mod trace;
pub mod union_find;

#[cfg(test)]
mod testkit;

pub use compile::{CompileOutput, compile};
pub use compiled::{
    CompiledCircuit, CompiledDevice, CompiledLoad, CompiledPin, CompiledSource, CompiledSupply,
    CompiledSwitch, DeviceIx, Drive, Level, LoadIx, LoadRule, NetIx, PinIx, SigIx, SignalDef,
    SignalKind, SwitchIx,
};
pub use fingerprint::Fingerprint;
pub use runtime::{
    Checkpoint, ConfigError, DelayConfig, FaultPolicy, InvalidInput, MAX_PENDING_TRACE_EVENTS,
    RestoreError, SimConfig, SimulationError, Simulator, StepOutcome,
};
pub use solve::{Direction, ElectricalSolution, LoadState, Solver, SwitchMask};
pub use trace::{LoadActivity, TraceEvent, TraceHeader, TraceKind, format_event, format_trace};
