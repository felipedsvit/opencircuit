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
pub mod documentation;
pub mod geometry;
pub mod ids;
pub mod input;
pub mod labels;
pub mod layers;
pub mod layout;
pub mod library;
pub mod limits;
pub mod loose;
pub mod nets;
pub mod schematic;
pub mod sheet;
pub mod signals;
pub mod snapshot;
pub mod supply;
pub mod table;
pub mod terminals;
pub mod tree;
mod validate;
pub mod wire_numbers;
pub mod xref;

pub use builder::SchematicBuilder;
pub use catalog::{
    CatalogError, Category, ComponentCatalog, ComponentModel, Connection, DeviceClass, Operator,
    ParamDefinition, ParamType, Provenance, SymbolDefinition, SymbolKind, TerminalDefinition,
    TimerMode,
};
pub use diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticCode, Severity, Subject, has_errors, sort_diagnostics,
};
pub use documentation::{
    DeviceInfo, MAX_NOTE_CHARS, TextAnchor, TextNote, Waiver, WaiverIndex, WireNumbering,
};
pub use geometry::{GridPoint, GridRect, QuarterTurn};
pub use ids::{
    DeviceId, IdAllocator, IdExhausted, JunctionId, LayerId, MAX_ENTITY_ID, SheetId, SupplyId,
    SymbolId, TextId, WireId,
};
pub use input::{Control, InputAction, InputEvent, operator_controls};
pub use labels::{natural_key, pin_name, terminal_label};
pub use layers::{Layer, LayerStates, MAX_LAYER_NAME_CHARS, MAX_LAYERS, hidden_junctions};
pub use layout::{
    Bend, LayoutIndex, is_orthogonal, orthogonal_route, orthogonal_route_bent, segment_at,
    segment_contains, stretch_route, text_bounds,
};
pub use loose::loose_geometry;
pub use nets::{Net, NetId, StaticNets};
pub use schematic::{
    CURRENT_SCHEMA_VERSION, Device, Endpoint, Junction, ModelRef, ParamValue, PinRef, ProjectMeta,
    ReferenceDesignator, Schematic, SymbolInstance, SymbolKey, TerminalKey, Wire,
};
pub use sheet::{Orientation, ReadingKey, Sheet, SheetSize, Zone, Zones, reading_key, sheet_at};
pub use signals::{ArrowAt, lone_arrows, signal_arrows, signal_references};
pub use snapshot::{Live, MotorState, MotorView, SimSnapshot, TimerView, WindingConnection};
pub use supply::{
    Conductor, ConductorConflict, ConductorSet, PhaseSequence, SupplyKind, SupplyProfile,
};
pub use table::Table;
pub use terminals::{StripTerminal, duplicate_terminals, strip_terminals};
pub use tree::{TreeDevice, TreeGroup, project_tree};
pub use wire_numbers::WireNumbers;
pub use xref::{ContactRef, CrossReferences, Link, Location, cross_references, links};
