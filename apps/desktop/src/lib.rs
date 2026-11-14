//! OpenCircuit desktop application.
//!
//! The window opens a project, draws it with `circuit-renderer` and runs it
//! with `circuit-sim`. It holds no electrical logic: it shows what the core
//! computes and forwards operator actions to it. egui and eframe are used
//! here and nowhere else in the workspace.

#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]
#![cfg_attr(not(test), deny(clippy::indexing_slicing))]

pub mod app;
pub mod camera;
mod canvas;
pub mod clock;
pub mod files;
pub mod session;
mod theme;

pub use app::ViewerApp;
pub use session::{LoadError, Session};
