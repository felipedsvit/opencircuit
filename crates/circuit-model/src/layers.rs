//! Layers: named groups of drawing elements that can be hidden or locked
//! (ADR-0015).
//!
//! A layer is drawing organisation, not electrical meaning: the compiler and
//! the simulator never read it, so hiding a layer changes nothing in a
//! simulation. Symbols, wires and text notes may name a layer; one that names
//! none is on the default layer, which always shows and always edits.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::ids::{JunctionId, LayerId};
use crate::schematic::{Endpoint, Schematic};

/// Most layers a document may have.
pub const MAX_LAYERS: usize = 256;

/// Most characters in the name of a layer.
pub const MAX_LAYER_NAME_CHARS: usize = 32;

/// A named group of elements with two switches.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    /// Stable identifier, from the document counter.
    pub id: LayerId,
    /// The name, unique in the document.
    pub name: String,
    /// Whether the layer is drawn, exported and hit-tested.
    pub visible: bool,
    /// Whether the editor refuses to change the elements on the layer.
    pub locked: bool,
}

impl Layer {
    /// Whether `name` is acceptable: 1 to [`MAX_LAYER_NAME_CHARS`] characters,
    /// no control character, and no space at either end.
    #[must_use]
    pub fn is_valid_name(name: &str) -> bool {
        !name.is_empty()
            && name.chars().count() <= MAX_LAYER_NAME_CHARS
            && name.len() <= crate::limits::MAX_TEXT_BYTES
            && !name.chars().any(char::is_control)
            && name.trim() == name
    }
}

/// What the layers of a document say about its elements, indexed once.
#[derive(Clone, Debug, Default)]
pub struct LayerStates {
    states: BTreeMap<LayerId, (bool, bool)>,
}

impl LayerStates {
    /// Index the layers of a document.
    #[must_use]
    pub fn new(doc: &Schematic) -> Self {
        Self {
            states: doc
                .layers
                .iter()
                .map(|layer| (layer.id, (layer.visible, layer.locked)))
                .collect(),
        }
    }

    /// Whether an element on `layer` is drawn. The default layer, and a layer
    /// that does not exist (validation reports that), are visible.
    #[must_use]
    pub fn is_visible(&self, layer: Option<LayerId>) -> bool {
        layer
            .and_then(|id| self.states.get(&id))
            .is_none_or(|(visible, _)| *visible)
    }

    /// Whether an element on `layer` is locked. The default layer, and a
    /// layer that does not exist, are not.
    #[must_use]
    pub fn is_locked(&self, layer: Option<LayerId>) -> bool {
        layer
            .and_then(|id| self.states.get(&id))
            .is_some_and(|(_, locked)| *locked)
    }

    /// Whether no layer hides or locks anything.
    #[must_use]
    pub fn is_plain(&self) -> bool {
        self.states
            .values()
            .all(|(visible, locked)| *visible && !*locked)
    }
}

/// The junctions that have wires, every one of them on a hidden layer: they
/// would be dots with nothing to join, so they are hidden too.
#[must_use]
pub fn hidden_junctions(doc: &Schematic, states: &LayerStates) -> BTreeSet<JunctionId> {
    if states.is_plain() {
        return BTreeSet::new();
    }
    let mut all_hidden: BTreeMap<JunctionId, bool> = BTreeMap::new();
    for wire in &doc.wires {
        for end in [&wire.from, &wire.to] {
            if let Endpoint::Junction(id) = end {
                let entry = all_hidden.entry(*id).or_insert(true);
                *entry &= !states.is_visible(wire.layer);
            }
        }
    }
    all_hidden
        .into_iter()
        .filter_map(|(id, hidden)| hidden.then_some(id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(layers: &[(u64, bool, bool)]) -> Schematic {
        let mut doc = Schematic::new("layers");
        for (id, visible, locked) in layers {
            doc.layers.push(Layer {
                id: LayerId(*id),
                name: format!("L{id}"),
                visible: *visible,
                locked: *locked,
            });
        }
        doc
    }

    #[test]
    fn a_name_is_one_to_32_characters_without_controls_or_edge_spaces() {
        assert!(Layer::is_valid_name("Power"));
        assert!(Layer::is_valid_name("Notas de projeto"));
        assert!(Layer::is_valid_name(&"x".repeat(MAX_LAYER_NAME_CHARS)));
        assert!(!Layer::is_valid_name(""));
        assert!(!Layer::is_valid_name(&"x".repeat(MAX_LAYER_NAME_CHARS + 1)));
        assert!(!Layer::is_valid_name(" Power"));
        assert!(!Layer::is_valid_name("Power "));
        assert!(!Layer::is_valid_name("a\nb"));
    }

    #[test]
    fn the_default_layer_and_a_missing_one_always_show_and_edit() {
        let states = LayerStates::new(&doc(&[(1, false, true)]));

        assert!(states.is_visible(None) && !states.is_locked(None));
        assert!(states.is_visible(Some(LayerId(9))) && !states.is_locked(Some(LayerId(9))));
    }

    #[test]
    fn a_junction_is_hidden_only_when_every_wire_on_it_is() {
        use crate::schematic::{Junction, Wire};
        let wire = |id: u64, junction: u64, layer: Option<u64>| Wire {
            id: crate::ids::WireId(id),
            from: Endpoint::Junction(JunctionId(junction)),
            to: Endpoint::Junction(JunctionId(junction + 100)),
            route: Vec::new(),
            number: None,
            layer: layer.map(LayerId),
            spec: None,
        };
        let mut doc = doc(&[(1, false, false)]);
        for id in [1, 2, 101, 102] {
            doc.junctions.push(Junction {
                id: JunctionId(id),
                position: crate::geometry::GridPoint::default(),
            });
        }
        // Junction 1 has one hidden wire; junction 2 has a hidden and a plain one.
        doc.wires = vec![
            wire(10, 1, Some(1)),
            wire(11, 2, Some(1)),
            wire(12, 2, None),
        ];

        let hidden = hidden_junctions(&doc, &LayerStates::new(&doc));

        assert!(hidden.contains(&JunctionId(1)) && hidden.contains(&JunctionId(101)));
        assert!(!hidden.contains(&JunctionId(2)));
        // Everything is shown when nothing is hidden.
        let shown = Schematic::new("none");
        assert!(hidden_junctions(&shown, &LayerStates::new(&shown)).is_empty());
    }

    #[test]
    fn a_layer_hides_and_locks_what_it_says() {
        let states = LayerStates::new(&doc(&[
            (1, false, false),
            (2, true, true),
            (3, true, false),
        ]));

        assert!(!states.is_visible(Some(LayerId(1))));
        assert!(states.is_visible(Some(LayerId(2))) && states.is_locked(Some(LayerId(2))));
        assert!(states.is_visible(Some(LayerId(3))) && !states.is_locked(Some(LayerId(3))));
        assert!(!states.is_plain());
        assert!(LayerStates::new(&doc(&[(3, true, false)])).is_plain());
        assert!(LayerStates::new(&doc(&[])).is_plain());
    }
}
