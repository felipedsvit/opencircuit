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
use std::fmt;

use serde::de::{Deserializer, Error as _, MapAccess, Visitor};
use serde::{Deserialize, Serialize};

use crate::diagnostics::Diagnostic;
use crate::documentation::{DeviceInfo, TextNote, Waiver, WireNumbering};
use crate::geometry::{GridPoint, QuarterTurn};
use crate::ids::{DeviceId, JunctionId, LayerId, SymbolId, WireId};
use crate::layers::Layer;
use crate::sheet::Sheet;
use crate::supply::SupplyProfile;
use crate::validate;

/// Schema version written by this build.
pub const CURRENT_SCHEMA_VERSION: u32 = 4;

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
    #[serde(deserialize_with = "unique_params")]
    pub params: BTreeMap<String, ParamValue>,
    /// Catalog data of the physical device (ADR-0014); absent when none is
    /// set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info: Option<DeviceInfo>,
}

/// Deserialize device parameters, refusing a key that appears twice. A plain
/// map would silently keep the last value, and a file must not mean two
/// things.
fn unique_params<'de, D>(deserializer: D) -> Result<BTreeMap<String, ParamValue>, D::Error>
where
    D: Deserializer<'de>,
{
    struct UniqueParams;

    impl<'de> Visitor<'de> for UniqueParams {
        type Value = BTreeMap<String, ParamValue>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a map of device parameters with unique keys")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
            let mut params = BTreeMap::new();
            while let Some((key, value)) = access.next_entry::<String, ParamValue>()? {
                if params.contains_key(&key) {
                    return Err(A::Error::custom(format_args!(
                        "duplicate parameter `{key}`"
                    )));
                }
                params.insert(key, value);
            }
            Ok(params)
        }
    }

    deserializer.deserialize_map(UniqueParams)
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
    /// The layer the symbol is on (ADR-0015); absent: the default layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<LayerId>,
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
    /// Override of the generated wire number (ADR-0014); absent when the
    /// number is generated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    /// The layer the wire is on (ADR-0015); absent: the default layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<LayerId>,
}

/// Document metadata without electrical meaning.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectMeta {
    /// Project title.
    pub name: String,
    /// Editor hint for the grid pitch.
    pub grid_unit: u32,
    /// Who drew the project, for the title block.
    pub author: Option<String>,
    /// Company or organization, for the title block.
    pub company: Option<String>,
    /// Revision mark, such as `B` or `1.2`, for the title block.
    pub revision: Option<String>,
    /// Date as the author writes it, for the title block; the core has no
    /// clock and never fills it in.
    pub date: Option<String>,
    /// The rule that numbers the wires (ADR-0014); absent: wires are not
    /// numbered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wire_numbering: Option<WireNumbering>,
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
    /// Drawing sheets, frames over the canvas (ADR-0011). Empty: the
    /// drawing is one page sized to its content.
    pub sheets: Vec<Sheet>,
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
    /// Warnings and informational diagnostics a person accepted, with the
    /// reason (ADR-0014), sorted by code and subject.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub waivers: Vec<Waiver>,
    /// Text notes on the drawing (ADR-0015); absent when there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<TextNote>,
    /// Layers (ADR-0015), sorted by ID; absent when there are none. Elements
    /// with no layer are on the default layer, which always shows and edits.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<Layer>,
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
                author: None,
                company: None,
                revision: None,
                date: None,
                wire_numbering: None,
            },
            sheets: Vec::new(),
            supplies: Vec::new(),
            devices: Vec::new(),
            symbols: Vec::new(),
            junctions: Vec::new(),
            wires: Vec::new(),
            waivers: Vec::new(),
            texts: Vec::new(),
            layers: Vec::new(),
        }
    }

    /// Whether every collection is already sorted by ID, so that
    /// [`sort_canonical`](Self::sort_canonical) would change nothing.
    #[must_use]
    pub fn is_canonical(&self) -> bool {
        self.sheets.is_sorted_by_key(|sheet| sheet.id)
            && self.supplies.is_sorted_by_key(|supply| supply.id)
            && self.devices.is_sorted_by_key(|device| device.id)
            && self.symbols.is_sorted_by_key(|symbol| symbol.id)
            && self.junctions.is_sorted_by_key(|junction| junction.id)
            && self.wires.is_sorted_by_key(|wire| wire.id)
            && self.texts.is_sorted_by_key(|text| text.id)
            && self.layers.is_sorted_by_key(|layer| layer.id)
            && self
                .waivers
                .is_sorted_by(|a, b| a.sort_key() <= b.sort_key())
    }

    /// Sort every collection by ID, the canonical order of a saved file.
    pub fn sort_canonical(&mut self) {
        self.sheets.sort_by_key(|sheet| sheet.id);
        self.supplies.sort_by_key(|supply| supply.id);
        self.devices.sort_by_key(|device| device.id);
        self.symbols.sort_by_key(|symbol| symbol.id);
        self.junctions.sort_by_key(|junction| junction.id);
        self.wires.sort_by_key(|wire| wire.id);
        self.texts.sort_by_key(|text| text.id);
        self.layers.sort_by_key(|layer| layer.id);
        self.waivers.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
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

    /// The diagnostics of [`validate`](Self::validate) that make a document
    /// unsafe to edit: an unsupported schema version, IDs out of range or
    /// used twice, references to something that does not exist, and a wire
    /// that starts and ends at the same point.
    ///
    /// It returns exactly those diagnostics, in the same order, without the
    /// work of the other checks (junction degrees, designators and supply
    /// profiles), so an editor can ask it for every transaction.
    #[must_use]
    pub fn integrity_errors(&self) -> Vec<Diagnostic> {
        validate::integrity(self)
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

        assert!(!doc.is_canonical());
        doc.sort_canonical();
        assert!(doc.is_canonical());

        let ids: Vec<u64> = doc.junctions.iter().map(|j| j.id.0).collect();
        assert_eq!(ids, [1, 2, 3]);
    }

    #[test]
    fn is_canonical_looks_at_every_collection() {
        assert!(Schematic::new("empty").is_canonical());

        let mut doc = Schematic::new("test");
        doc.wires = [2, 1]
            .map(|id| Wire {
                id: WireId(id),
                from: Endpoint::Junction(JunctionId(1)),
                to: Endpoint::Junction(JunctionId(2)),
                route: Vec::new(),
                number: None,
                layer: None,
            })
            .to_vec();

        assert!(!doc.is_canonical());
    }
}
