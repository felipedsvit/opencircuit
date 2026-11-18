//! What lies under a point of the sheet.

use circuit_commands::Anchor;
use circuit_model::{
    ComponentCatalog, Endpoint, GridPoint, JunctionId, LayerStates, LayoutIndex, PinRef, Schematic,
    ShapeId, Subject, SymbolId, TextId, ViewId, WireId, hidden_junctions, segment_at, text_bounds,
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
    /// A text note.
    Text(TextId),
    /// The body of a symbol.
    Symbol(SymbolId),
    /// A line of a shape, or a QR code (ADR-0018).
    Shape(ShapeId),
    /// A device view (ADR-0018).
    View(ViewId),
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
            Self::Text(id) => Subject::Text(*id),
            Self::Shape(id) => Subject::Shape(*id),
            Self::View(id) => Subject::View(*id),
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
            Self::Symbol(_) | Self::Text(_) | Self::Shape(_) | Self::View(_) => None,
        }
    }
}

/// The element under `point`, by priority: a pin, then a junction, then a
/// wire, then a symbol body. Among elements of one kind the one with the
/// highest ID wins, as it is drawn last.
#[must_use]
pub fn hit_test(doc: &Schematic, catalog: &ComponentCatalog, point: GridPoint) -> Option<Hit> {
    let layout = LayoutIndex::new(doc, catalog);
    // What a hidden layer hides cannot be hit (ADR-0015).
    let layers = LayerStates::new(doc);
    let hidden = hidden_junctions(doc, &layers);

    let mut symbols: Vec<SymbolId> = doc
        .symbols
        .iter()
        .filter(|s| layers.is_visible(s.layer))
        .map(|s| s.id)
        .collect();
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
        .filter(|j| j.position == point && !hidden.contains(&j.id))
        .map(|j| j.id)
        .max();
    if let Some(id) = junction {
        return Some(Hit::Junction(id));
    }

    // A text note is above wires and symbols, below pins and junctions.
    let text = doc
        .texts
        .iter()
        .filter(|note| {
            layers.is_visible(note.layer)
                && text_bounds(note).is_some_and(|area| area.contains(point))
        })
        .map(|note| note.id)
        .max();
    if let Some(id) = text {
        return Some(Hit::Text(id));
    }

    let wire = doc
        .wires
        .iter()
        .filter(|w| {
            layers.is_visible(w.layer)
                && layout
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

    // A shape is hit on its lines (ADR-0018), above symbol bodies, so that a
    // frame drawn around symbols can be picked on its edge; a view in its
    // whole rectangle.
    let shape = doc
        .shapes
        .iter()
        .filter(|shape| layers.is_visible(shape.layer) && shape.kind.touches(point))
        .map(|shape| shape.id)
        .max();
    if let Some(id) = shape {
        return Some(Hit::Shape(id));
    }
    let view = doc
        .views
        .iter()
        .filter(|view| {
            layers.is_visible(view.layer) && view.bounds().is_some_and(|area| area.contains(point))
        })
        .map(|view| view.id)
        .max();
    if let Some(id) = view {
        return Some(Hit::View(id));
    }

    symbols
        .iter()
        .find(|id| {
            layout
                .symbol_bounds(**id)
                .is_some_and(|bounds| bounds.contains(point))
        })
        .map(|id| Hit::Symbol(*id))
}
