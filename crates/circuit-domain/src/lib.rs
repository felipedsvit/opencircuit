//! Domain model for OpenCircuit.
//!
//! The crate contains only data and structural rules. It does not depend on
//! GUI, filesystem or simulation crates.

mod device;
mod ids;
mod schematic;
mod supply;
mod symbol;
mod terminal;

pub use device::{Device, DeviceKind, ReferenceDesignator};
pub use ids::{DeviceId, JunctionId, SupplyId, SymbolId, WireId};
pub use schematic::{
    CURRENT_SCHEMA_VERSION, ConnectionEndpoint, Junction, Schematic, ValidationIssue, Wire,
};
pub use supply::{Conductor, PhaseSequence, SupplyKind, SupplyProfile};
pub use symbol::{GridPoint, QuarterTurn, SymbolInstance, SymbolKind};
pub use terminal::{PinRef, TerminalKey};
