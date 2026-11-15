//! Document model for OpenCircuit.
//!
//! This crate holds the data every other layer agrees on: stable identifiers,
//! the schematic document, the component catalog, structured diagnostics, the
//! external inputs of a simulation and the snapshot a simulation produces.
//!
//! It contains data and structural rules only. It does not simulate, does not
//! touch the filesystem, sockets or the clock, and depends on no other
//! workspace crate.

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

pub mod builder;
pub mod catalog;
pub mod diagnostics;
pub mod geometry;
pub mod ids;
pub mod input;
pub mod layout;
pub mod limits;
pub mod schematic;
pub mod snapshot;
pub mod supply;
mod validate;

pub use builder::SchematicBuilder;
pub use catalog::{
    CatalogError, ComponentCatalog, ComponentModel, Connection, DeviceClass, Operator,
    ParamDefinition, ParamType, Provenance, SymbolDefinition, SymbolKind, TerminalDefinition,
    TimerMode,
};
pub use diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticCode, Severity, Subject, has_errors, sort_diagnostics,
};
pub use geometry::{GridPoint, GridRect, QuarterTurn};
pub use ids::{
    DeviceId, IdAllocator, IdExhausted, JunctionId, MAX_ENTITY_ID, SupplyId, SymbolId, WireId,
};
pub use input::{Control, InputAction, InputEvent, operator_controls};
pub use layout::{LayoutIndex, is_orthogonal, orthogonal_route, segment_at, segment_contains};
pub use schematic::{
    CURRENT_SCHEMA_VERSION, Device, Endpoint, Junction, ModelRef, ParamValue, PinRef, ProjectMeta,
    ReferenceDesignator, Schematic, SymbolInstance, SymbolKey, TerminalKey, Wire,
};
pub use snapshot::{Live, MotorState, MotorView, SimSnapshot, TimerView, WindingConnection};
pub use supply::{
    Conductor, ConductorConflict, ConductorSet, PhaseSequence, SupplyKind, SupplyProfile,
};
