//! What lies under a point of the sheet.

use circuit_commands::Anchor;
use circuit_model::{
    ComponentCatalog, Endpoint, GridPoint, JunctionId, LayoutIndex, PinRef, Schematic, Subject,
    SymbolId, WireId, segment_at,
};

/// The element under a grid point.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Hit {
    /// A terminal of a symbol.
    Pin(PinRef),
    /// A junction.
    Junction(JunctionId),
    /// A point on a wire.
    Wire {
        /// The wire.
        wire: WireId,
        /// The point.
        at: GridPoint,
    },
    /// The body of a symbol.
    Symbol(SymbolId),
}

impl Hit {
    /// The element a click on this hit selects; a pin selects its symbol.
    #[must_use]
    pub fn subject(&self) -> Subject {
        match self {
            Self::Pin(pin) => Subject::Symbol(pin.symbol_id),
            Self::Junction(id) => Subject::Junction(*id),
            Self::Wire { wire, .. } => Subject::Wire(*wire),
            Self::Symbol(id) => Subject::Symbol(*id),
        }
    }

    /// Where a wire can end at this hit: a pin, a junction or a point on a
    /// wire. The body of a symbol is not one.
    #[must_use]
    pub fn anchor(&self) -> Option<Anchor> {
        match self {
            Self::Pin(pin) => Some(Anchor::Endpoint(Endpoint::Terminal(pin.clone()))),
            Self::Junction(id) => Some(Anchor::Endpoint(Endpoint::Junction(*id))),
            Self::Wire { wire, at } => Some(Anchor::OnWire {
                wire: *wire,
                at: *at,
            }),
            Self::Symbol(_) => None,
        }
    }
}

/// The element under `point`, by priority: a pin, then a junction, then a
/// wire, then a symbol body. Among elements of one kind the one with the
/// highest ID wins, as it is drawn last.
#[must_use]
pub fn hit_test(doc: &Schematic, catalog: &ComponentCatalog, point: GridPoint) -> Option<Hit> {
    let layout = LayoutIndex::new(doc, catalog);

    let mut symbols: Vec<SymbolId> = doc.symbols.iter().map(|s| s.id).collect();
    symbols.sort_unstable_by(|a, b| b.cmp(a));
    for id in &symbols {
        let Some(definition) = layout.definition(*id) else {
            continue;
        };
        for terminal in definition.terminals() {
            let pin = PinRef {
                symbol_id: *id,
                terminal: terminal.key.clone(),
            };
            if layout.pin_position(&pin) == Some(point) {
                return Some(Hit::Pin(pin));
            }
        }
    }

    let junction = doc
        .junctions
        .iter()
        .filter(|j| j.position == point)
        .map(|j| j.id)
        .max();
    if let Some(id) = junction {
        return Some(Hit::Junction(id));
    }

    let wire = doc
        .wires
        .iter()
        .filter(|w| {
            layout
                .wire_polyline(w)
                .is_some_and(|path| segment_at(&path, point).is_some())
        })
        .map(|w| w.id)
        .max();
    if let Some(id) = wire {
        return Some(Hit::Wire {
            wire: id,
            at: point,
        });
    }

    layout.symbol_at(point).map(Hit::Symbol)
}
