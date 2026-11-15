//! Exporters of an OpenCircuit drawing.
//!
//! An exporter receives the neutral [`Scene`] built by `circuit-renderer`
//! (integer logical coordinates and semantic styles), never GUI structures,
//! so a file written by the command line and one written by the desktop
//! application are the same. [`svg`] writes SVG text directly; no library is
//! involved.
//!
//! Output is deterministic: the same scene and theme give the same bytes on
//! every platform. Coordinates are written as integers in scene subunits,
//! widths as fixed decimals computed with integers, items in scene order,
//! attributes in a fixed order, LF line endings, no date and no random
//! identifier. The crate does no I/O and reads no clock.

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

mod svg;
mod theme;

pub use svg::{MARGIN, svg};
pub use theme::{Look, Rgb, Theme};

/// The scene type every exporter reads, re-exported for convenience.
pub use circuit_renderer::Scene;
