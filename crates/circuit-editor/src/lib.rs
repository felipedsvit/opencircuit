//! Editor logic of OpenCircuit, without a GUI.
//!
//! A front end converts pointer and key events to grid points and passes
//! them to an [`Editor`]. The editor tracks the tool, the selection and the
//! gesture in progress, answers what lies under a point ([`hit_test`]) and
//! where a pointer finer than the grid snaps ([`snap`]), and
//! returns the [`Transaction`](circuit_commands::Transaction) of each user
//! action for the caller to apply through a `History`. It never changes the
//! document itself, and [`Editor::preview`] shows what a drag or a placement
//! would do before it is committed.
//!
//! The crate depends on `circuit-model` and `circuit-commands` only: no
//! simulator, no renderer, no GUI and no I/O.

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

mod editor;
mod hit;
mod snap;

#[cfg(test)]
mod tests;

pub use editor::{Editor, Key, Modifiers, Outcome, Preview, Tool};
pub use hit::{Hit, hit_test};
pub use snap::{Pointer, Snap, SnapTarget, snap};
