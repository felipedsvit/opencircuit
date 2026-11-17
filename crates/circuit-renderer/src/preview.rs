//! The drawing of one catalog symbol on its own, for a palette.

use circuit_model::{ComponentModel, SymbolKey};

use crate::scene::Scene;
use crate::symbols::{self, Actuator, Look};

/// The drawing of the symbol `key` of `model`, at rest, around its origin,
/// unrotated: no designator, no terminal numbers and no wires, which belong
/// to a device placed in a document. `None` when the model has no such
/// symbol.
#[must_use]
pub fn symbol_preview(model: &ComponentModel, key: &SymbolKey) -> Option<Scene> {
    let definition = model.symbol(key)?;
    let look = Look::default();
    let drawing = match definition.artwork() {
        Some(artwork) => symbols::library_art(artwork, look),
        None => symbols::art(definition.kind(), Actuator::of(model.operator()), look),
    };
    let mut scene = Scene::default();
    for (shape, style) in drawing {
        scene.push(shape, style, None);
    }
    Some(scene)
}
