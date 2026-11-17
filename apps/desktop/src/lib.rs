//! OpenCircuit desktop application.
//!
//! The window opens a project, draws it with `circuit-renderer`, edits it
//! with `circuit-editor` and `circuit-commands`, and runs it with
//! `circuit-sim`. It holds no electrical logic: it shows what the core
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
mod cmdline;
mod dialogs;
mod documentation;
pub mod files;
mod i18n;
mod icons;
mod inspector;
mod palette;
#[cfg(not(target_arch = "wasm32"))]
pub mod project_lock;
#[cfg(not(target_arch = "wasm32"))]
pub mod recent;
#[cfg(not(target_arch = "wasm32"))]
pub mod recovery;
pub mod session;
pub mod templates;
mod theme;
#[cfg(not(target_arch = "wasm32"))]
pub mod versions;
#[cfg(target_arch = "wasm32")]
pub mod web;

pub use app::{DesktopApp, Mode};
pub use i18n::Language;
pub use session::{LoadError, Session};
