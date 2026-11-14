//! The schematic document.
//!
//! A device is the equipment; a symbol is one graphical instance of it; a
//! terminal is a stable connection point of a symbol. Wires connect terminals
//! and explicit junctions. Two wires that merely cross are not connected.
//!
//! These types are exactly what a `.ocircuit` file stores. Unknown fields are
//! rejected, and the document persists component model references, never the
//! behaviour class of a device.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::diagnostics::Diagnostic;
use crate::geometry::{GridPoint, QuarterTurn};
use crate::ids::{DeviceId, JunctionId, SymbolId, WireId};
use crate::supply::SupplyProfile;
use crate::validate;

/// Schema version written by this build.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// User-visible label of a device, such as `K1` or `S2`.
///
/// It is only a label: symbols are linked to their device by [`DeviceId`], so
/// renaming a device never breaks a connection.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ReferenceDesignator(pub String);

impl ReferenceDesignator {
    /// Longest accepted designator, in bytes.
    pub const MAX_LEN: usize = 16;

    /// Whether the text is an uppercase ASCII letter followed by up to 15
    /// uppercase ASCII letters, digits or underscores.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let mut bytes = self.0.bytes();
        let Some(first) = bytes.next() else {
            return false;
        };
        self.0.len() <= Self::MAX_LEN
            && first.is_ascii_uppercase()
            && bytes.all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    }
}

/// Structural key of a terminal inside its symbol, such as `a1`, `p2` or
/// `pole1.in`. Displayed numbers like 13/14 are labels, not keys.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct TerminalKey(pub String);

/// Key of a symbol inside its component model, such as `coil` or `aux_no`.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct SymbolKey(pub String);

/// One terminal of one symbol instance.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PinRef {
    /// The symbol instance.
    pub symbol_id: SymbolId,
    /// The terminal inside that symbol.
    pub terminal: TerminalKey,
}

/// Versioned reference to a component model of the catalog.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRef {
    /// Model identifier, such as `contactor.generic`.
    pub id: String,
    /// Model version; a published version never changes.
    pub version: u32,
}

/// Value of a device parameter. There is no floating point: voltages are in
/// millivolts and times in milliseconds.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(untagged)]
pub enum ParamValue {
    /// A flag.
    Bool(bool),
    /// An integer quantity or an entity reference.
    Int(i64),
    /// Free text or one of the choices of an enumerated parameter.
    Text(String),
}

/// Physical or logical equipment.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Device {
    /// Stable identifier.
    pub id: DeviceId,
    /// User-visible label.
    pub reference: ReferenceDesignator,
    /// Component model that defines symbols, terminals and behaviour.
    pub model: ModelRef,
    /// Parameter values, validated against the model.
    pub params: BTreeMap<String, ParamValue>,
}

/// One graphical instance of a symbol of a device.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolInstance {
    /// Stable identifier.
    pub id: SymbolId,
    /// The device this symbol represents.
    pub device_id: DeviceId,
    /// Which symbol of the device model this is.
    pub symbol_key: SymbolKey,
    /// Position among repeatable symbols (auxiliary contacts); required
    /// exactly when the model marks the symbol as slotted.
    pub slot: Option<u8>,
    /// Position of the symbol origin.
    pub position: GridPoint,
    /// Rotation of the symbol.
    pub rotation: QuarterTurn,
}

/// Explicit connection point between wires.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Junction {
    /// Stable identifier.
    pub id: JunctionId,
    /// Position on the sheet.
    pub position: GridPoint,
}

/// Where a wire ends.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Endpoint {
    /// A terminal of a symbol.
    Terminal(PinRef),
    /// An explicit junction.
    Junction(JunctionId),
}

impl From<JunctionId> for Endpoint {
    fn from(junction: JunctionId) -> Self {
        Self::Junction(junction)
    }
}

impl From<PinRef> for Endpoint {
    fn from(pin: PinRef) -> Self {
        Self::Terminal(pin)
    }
}

/// A wire between two endpoints.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Wire {
    /// Stable identifier.
    pub id: WireId,
    /// First endpoint.
    pub from: Endpoint,
    /// Second endpoint.
    pub to: Endpoint,
    /// Intermediate waypoints only; the two ends are derived from the
    /// endpoints, so moving a symbol never invalidates stored geometry.
    pub route: Vec<GridPoint>,
}

/// Document metadata without electrical meaning.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectMeta {
    /// Project title.
    pub name: String,
    /// Editor hint for the grid pitch.
    pub grid_unit: u32,
}

/// The schematic document.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Schematic {
    /// Schema version of the document.
    pub schema_version: u32,
    /// Next identifier to hand out; greater than every ID in the document.
    pub next_id: u64,
    /// Metadata.
    pub project: ProjectMeta,
    /// Supply profiles.
    pub supplies: Vec<SupplyProfile>,
    /// Devices.
    pub devices: Vec<Device>,
    /// Symbol instances.
    pub symbols: Vec<SymbolInstance>,
    /// Junctions.
    pub junctions: Vec<Junction>,
    /// Wires.
    pub wires: Vec<Wire>,
}

impl Schematic {
    /// An empty document at the current schema version.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            next_id: 1,
            project: ProjectMeta {
                name: name.into(),
                grid_unit: 10,
            },
            supplies: Vec::new(),
            devices: Vec::new(),
            symbols: Vec::new(),
            junctions: Vec::new(),
            wires: Vec::new(),
        }
    }

    /// Sort every collection by ID, the canonical order of a saved file.
    pub fn sort_canonical(&mut self) {
        self.supplies.sort_by_key(|supply| supply.id);
        self.devices.sort_by_key(|device| device.id);
        self.symbols.sort_by_key(|symbol| symbol.id);
        self.junctions.sort_by_key(|junction| junction.id);
        self.wires.sort_by_key(|wire| wire.id);
    }

    /// Check every structural rule that needs no component catalog.
    ///
    /// The result is in canonical order and does not depend on the order of
    /// the collections. An empty result means the document is structurally
    /// sound; rules that need the catalog (models, terminals, parameters) are
    /// checked by the compiler.
    #[must_use]
    pub fn validate(&self) -> Vec<Diagnostic> {
        validate::validate(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_designator_accepts_the_documented_pattern() {
        for text in ["K1", "S2", "KT1", "Q", "A_B_9", "ABCDEFGHIJKLMNOP"] {
            assert!(
                ReferenceDesignator(text.to_owned()).is_valid(),
                "{text} should be valid"
            );
        }
    }

    #[test]
    fn reference_designator_rejects_everything_else() {
        for text in [
            "",
            "k1",
            "1K",
            "_K",
            "K 1",
            "K-1",
            "Ké",
            "ABCDEFGHIJKLMNOPQ",
        ] {
            assert!(
                !ReferenceDesignator(text.to_owned()).is_valid(),
                "{text:?} should be invalid"
            );
        }
    }

    #[test]
    fn sort_canonical_orders_every_collection_by_id() {
        let mut doc = Schematic::new("test");
        for id in [3, 1, 2] {
            doc.junctions.push(Junction {
                id: JunctionId(id),
                position: GridPoint::default(),
            });
        }

        doc.sort_canonical();

        let ids: Vec<u64> = doc.junctions.iter().map(|j| j.id.0).collect();
        assert_eq!(ids, [1, 2, 3]);
    }
}
