//! Industrial I/O of OpenCircuit (ADR-0009).
//!
//! This crate is the pure half of the Modbus TCP server: it decodes and
//! encodes frames ([`modbus`]) and maps a project's inputs and outputs to
//! Modbus addresses ([`IoMap`]). It opens no socket, starts no thread and
//! reads no clock, so it builds for WebAssembly and is tested with arbitrary
//! bytes. The command line owns the socket and the real-time loop.
//!
//! OpenCircuit is a simulator for design, teaching and virtual
//! commissioning. It is not a controller and must not be connected to a
//! real plant.

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

mod map;
pub mod modbus;

pub use map::{IoMap, MapEntry, Observable, Table};
