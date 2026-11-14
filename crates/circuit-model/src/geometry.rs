//! Logical sheet geometry.
//!
//! Coordinates are integer document units, never screen pixels, and rotation
//! is restricted to quarter turns. The core uses no floating point.

use serde::{Deserialize, Serialize};

/// A point on the logical document grid.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(deny_unknown_fields)]
pub struct GridPoint {
    /// Horizontal coordinate, growing to the right.
    pub x: i32,
    /// Vertical coordinate, growing downwards.
    pub y: i32,
}

impl GridPoint {
    /// Shorthand constructor.
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// Rotation of a symbol in right-angle steps.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum QuarterTurn {
    /// No rotation.
    #[default]
    Deg0,
    /// One quarter turn.
    Deg90,
    /// Half a turn.
    Deg180,
    /// Three quarter turns.
    Deg270,
}
