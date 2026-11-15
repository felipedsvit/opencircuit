//! Editing of OpenCircuit documents.
//!
//! A user action becomes a [`Transaction`]: a list of primitive [`Edit`]s,
//! each of which knows its inverse. A transaction is applied to a copy of
//! the document and replaces it only when every edit succeeded, the result
//! stays within the limits of the file format, and no integrity error
//! (duplicate or out-of-range ID, dangling reference, wire to itself) was
//! added. Nothing is ever applied halfway.
//!
//! [`History`] records applied transactions for undo and redo. Undo never
//! lowers the document counter, so an ID is never handed out twice, while
//! redo applies the same transaction again with the same IDs.
//!
//! The functions of [`ops`] turn the actions of an editor (place, connect,
//! delete, move, rotate, rename, set a parameter, edit supplies) into
//! transactions, [`copy`] copies, pastes and duplicates part of a
//! document, and [`arrange`] edits several elements at once. The crate depends on `circuit-model` only and does no I/O.

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

pub mod arrange;
pub mod copy;
mod edit;
mod error;
mod history;
pub mod ops;
mod sorted;
mod transaction;

#[cfg(test)]
mod tests;

pub use edit::Edit;
pub use error::CommandError;
pub use history::{Applied, History};
pub use ops::{Anchor, SupplyPreset};
pub use transaction::Transaction;
