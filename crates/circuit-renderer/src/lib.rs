//! Neutral scene of an OpenCircuit document.
//!
//! [`build_scene`] turns a schematic, the component catalog and optionally
//! the snapshot of one solved tick into a [`Scene`]: a list of drawing
//! primitives in integer logical coordinates, each with a semantic
//! [`StyleKey`] instead of a colour, and with the document element it
//! represents. A desktop front end paints it, an exporter can write it as
//! SVG, and tests can inspect it without any GUI.
//!
//! The crate depends on `circuit-model` only. It never sees the simulator:
//! the simulation state reaches it as a `SimSnapshot`, which uses document
//! IDs. It does no I/O and reads no clock.

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

mod annotations;
mod build;
mod frame;
mod preview;
pub mod scene;
mod symbols;

#[cfg(test)]
mod tests;

pub use build::{SceneOptions, build_scene, build_sheet_scene, sheet_scenes};
pub use preview::symbol_preview;
pub use scene::{
    Anchor, SUBUNITS_PER_GRID, Scene, SceneItem, ScenePoint, SceneRect, Shape, StyleKey,
};
