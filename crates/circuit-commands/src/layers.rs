//! Edits of the layers, and the check that a locked layer is respected
//! (ADR-0015).
//!
//! A locked layer protects what a person does to its elements directly: move,
//! rotate, delete, rename, set a parameter, change the catalog data, the words
//! or the wire number. It does not stop a wire from following a symbol that
//! moved, and it does not stop a new wire from ending at a pin of one of its
//! symbols: those leave the element where it is.

use std::collections::BTreeSet;

use circuit_model::{
    Layer, LayerId, LayerStates, MAX_LAYERS, Schematic, Subject, SymbolId, WireId,
};

use crate::edit::Edit;
use crate::error::CommandError;
use crate::transaction::Transaction;

/// The first of `subjects` that is on a locked layer, if any. A device is
/// locked when any of its symbols is; a junction, a pin, a supply and the
/// document have no layer.
#[must_use]
pub fn locked_subject<'a>(
    doc: &Schematic,
    subjects: impl IntoIterator<Item = &'a Subject>,
) -> Option<Subject> {
    let states = LayerStates::new(doc);
    if states.is_plain() {
        return None;
    }
    subjects
        .into_iter()
        .find(|subject| is_locked(doc, &states, subject))
        .cloned()
}

fn is_locked(doc: &Schematic, states: &LayerStates, subject: &Subject) -> bool {
    match subject {
        Subject::Symbol(id) => doc
            .symbols
            .iter()
            .any(|s| s.id == *id && states.is_locked(s.layer)),
        Subject::Pin(pin) => doc
            .symbols
            .iter()
            .any(|s| s.id == pin.symbol_id && states.is_locked(s.layer)),
        Subject::Device(id) => doc
            .symbols
            .iter()
            .any(|s| s.device_id == *id && states.is_locked(s.layer)),
        Subject::Wire(id) => doc
            .wires
            .iter()
            .any(|w| w.id == *id && states.is_locked(w.layer)),
        Subject::Text(id) => doc
            .texts
            .iter()
            .any(|t| t.id == *id && states.is_locked(t.layer)),
        Subject::Shape(id) => doc
            .shapes
            .iter()
            .any(|s| s.id == *id && states.is_locked(s.layer)),
        Subject::View(id) => doc
            .views
            .iter()
            .any(|v| v.id == *id && states.is_locked(v.layer)),
        Subject::Junction(_)
        | Subject::Supply(_)
        | Subject::Sheet(_)
        | Subject::Layer(_)
        | Subject::Comment(_)
        | Subject::Group(_)
        | Subject::Plc(_)
        | Subject::Document => false,
    }
}

/// Refuse a change to elements on a locked layer.
///
/// # Errors
///
/// Returns [`CommandError::Locked`] naming the first locked element.
pub fn ensure_unlocked<'a>(
    doc: &Schematic,
    subjects: impl IntoIterator<Item = &'a Subject>,
) -> Result<(), CommandError> {
    match locked_subject(doc, subjects) {
        Some(subject) => Err(CommandError::Locked(subject)),
        None => Ok(()),
    }
}

/// Refuse a change to these symbols when one is on a locked layer.
pub(crate) fn ensure_symbols_unlocked(
    doc: &Schematic,
    symbols: &BTreeSet<SymbolId>,
) -> Result<(), CommandError> {
    ensure_unlocked(
        doc,
        symbols
            .iter()
            .map(|id| Subject::Symbol(*id))
            .collect::<Vec<_>>()
            .iter(),
    )
}

/// Refuse moving what a selection moves (symbols, the symbols of devices and
/// text notes) when one is on a locked layer. Wires and junctions in the
/// selection are not moved by their own, so they are not checked.
pub(crate) fn ensure_movable_unlocked(
    doc: &Schematic,
    subjects: &BTreeSet<Subject>,
) -> Result<(), CommandError> {
    ensure_unlocked(
        doc,
        subjects.iter().filter(|subject| {
            matches!(
                subject,
                Subject::Symbol(_) | Subject::Device(_) | Subject::Text(_)
            )
        }),
    )
}

/// Refuse a change to this wire when it is on a locked layer.
pub(crate) fn ensure_wire_unlocked(doc: &Schematic, wire: WireId) -> Result<(), CommandError> {
    ensure_unlocked(doc, [&Subject::Wire(wire)])
}

fn layer_named<'a>(doc: &'a Schematic, name: &str) -> Option<&'a Layer> {
    doc.layers.iter().find(|layer| layer.name == name)
}

fn check_name(doc: &Schematic, name: &str, own: Option<LayerId>) -> Result<(), CommandError> {
    if !Layer::is_valid_name(name) {
        return Err(CommandError::InvalidParameter(format!(
            "a layer name is 1 to {} characters, with no control character or space at either end",
            circuit_model::MAX_LAYER_NAME_CHARS
        )));
    }
    if layer_named(doc, name).is_some_and(|other| Some(other.id) != own) {
        return Err(CommandError::InvalidParameter(format!(
            "a layer named `{name}` already exists"
        )));
    }
    Ok(())
}

/// Add a layer, visible and unlocked unless said otherwise.
///
/// # Errors
///
/// Returns [`CommandError::InvalidParameter`] for a name that is empty, too
/// long, has a control character or an edge space, or is another layer's, or
/// when the document has [`MAX_LAYERS`] layers; [`CommandError::Ids`] when it
/// has no ID left.
pub fn add_layer(
    doc: &Schematic,
    name: &str,
    visible: bool,
    locked: bool,
) -> Result<Transaction, CommandError> {
    check_name(doc, name, None)?;
    if doc.layers.len() >= MAX_LAYERS {
        return Err(CommandError::InvalidParameter(format!(
            "a drawing has at most {MAX_LAYERS} layers"
        )));
    }
    let mut ids = circuit_model::IdAllocator::starting_at(doc.next_id);
    let id = LayerId(ids.alloc()?);
    let mut tx = Transaction::new("Add layer", ids.next_id());
    tx.edits.push(Edit::InsertLayer(Layer {
        id,
        name: name.to_owned(),
        visible,
        locked,
    }));
    Ok(tx)
}

/// Rename a layer, show or hide it, lock or unlock it. What is `None` stays;
/// a change that changes nothing is an empty transaction. Always allowed on a
/// locked layer, which is how it is unlocked.
///
/// # Errors
///
/// Returns [`CommandError::Missing`] for a layer that does not exist and
/// [`CommandError::InvalidParameter`] for a name [`add_layer`] refuses.
pub fn set_layer(
    doc: &Schematic,
    id: LayerId,
    name: Option<&str>,
    visible: Option<bool>,
    locked: Option<bool>,
) -> Result<Transaction, CommandError> {
    let found = doc
        .layers
        .iter()
        .find(|layer| layer.id == id)
        .ok_or(CommandError::Missing(Subject::Layer(id)))?;
    let mut next = found.clone();
    if let Some(name) = name {
        check_name(doc, name, Some(id))?;
        name.clone_into(&mut next.name);
    }
    next.visible = visible.unwrap_or(next.visible);
    next.locked = locked.unwrap_or(next.locked);
    let mut tx = Transaction::new("Set layer", doc.next_id);
    if next != *found {
        tx.edits.push(Edit::SetLayer(next));
    }
    Ok(tx)
}

/// Remove a layer that no element is on.
///
/// # Errors
///
/// Returns [`CommandError::Missing`] for a layer that does not exist and
/// [`CommandError::InUse`] naming an element that is still on it.
pub fn remove_layer(doc: &Schematic, id: LayerId) -> Result<Transaction, CommandError> {
    if !doc.layers.iter().any(|layer| layer.id == id) {
        return Err(CommandError::Missing(Subject::Layer(id)));
    }
    let mut tx = Transaction::new("Remove layer", doc.next_id);
    tx.edits.push(Edit::RemoveLayer(id));
    // Refused here, with the element, rather than when the edit is applied.
    let _checked = tx.apply_to(doc)?;
    Ok(tx)
}

/// Put symbols, wires and text notes on a layer (`None`: the default layer).
/// A device stands for its symbols; junctions and the rest have no layer and
/// are left out. Elements already there are left alone.
///
/// # Errors
///
/// Returns [`CommandError::Missing`] for a layer or an element that does not
/// exist and [`CommandError::Locked`] for an element that is on a locked
/// layer now (the layer it goes to may be locked).
pub fn assign_layer(
    doc: &Schematic,
    subjects: &BTreeSet<Subject>,
    layer: Option<LayerId>,
) -> Result<Transaction, CommandError> {
    if let Some(id) = layer {
        if !doc.layers.iter().any(|candidate| candidate.id == id) {
            return Err(CommandError::Missing(Subject::Layer(id)));
        }
    }
    let mut targets: Vec<(Subject, Option<LayerId>)> = Vec::new();
    for subject in subjects {
        match subject {
            Subject::Symbol(id) => targets.push((
                subject.clone(),
                doc.symbols
                    .iter()
                    .find(|s| s.id == *id)
                    .ok_or_else(|| CommandError::Missing(subject.clone()))?
                    .layer,
            )),
            Subject::Wire(id) => targets.push((
                subject.clone(),
                doc.wires
                    .iter()
                    .find(|w| w.id == *id)
                    .ok_or_else(|| CommandError::Missing(subject.clone()))?
                    .layer,
            )),
            Subject::Text(id) => targets.push((
                subject.clone(),
                doc.texts
                    .iter()
                    .find(|t| t.id == *id)
                    .ok_or_else(|| CommandError::Missing(subject.clone()))?
                    .layer,
            )),
            Subject::Device(id) => {
                if !doc.devices.iter().any(|d| d.id == *id) {
                    return Err(CommandError::Missing(subject.clone()));
                }
                targets.extend(
                    doc.symbols
                        .iter()
                        .filter(|s| s.device_id == *id)
                        .map(|s| (Subject::Symbol(s.id), s.layer)),
                );
            }
            Subject::Shape(id) => targets.push((
                subject.clone(),
                doc.shapes
                    .iter()
                    .find(|s| s.id == *id)
                    .ok_or_else(|| CommandError::Missing(subject.clone()))?
                    .layer,
            )),
            Subject::View(id) => targets.push((
                subject.clone(),
                doc.views
                    .iter()
                    .find(|v| v.id == *id)
                    .ok_or_else(|| CommandError::Missing(subject.clone()))?
                    .layer,
            )),
            Subject::Junction(_)
            | Subject::Pin(_)
            | Subject::Supply(_)
            | Subject::Sheet(_)
            | Subject::Layer(_)
            | Subject::Comment(_)
            | Subject::Group(_)
            | Subject::Plc(_)
            | Subject::Document => {}
        }
    }
    targets.sort();
    targets.dedup();
    let states = LayerStates::new(doc);
    let mut tx = Transaction::new("Move to layer", doc.next_id);
    for (subject, current) in targets {
        if current == layer {
            continue;
        }
        if states.is_locked(current) {
            return Err(CommandError::Locked(subject));
        }
        tx.edits.push(Edit::AssignLayer { subject, layer });
    }
    Ok(tx)
}
