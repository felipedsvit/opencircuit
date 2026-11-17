//! Operations on several elements at once: one parameter set on many
//! devices, and symbols and junctions aligned or spaced evenly.
//!
//! Each returns one transaction, undone in one step.

use std::collections::BTreeSet;

use circuit_model::{
    ComponentCatalog, DeviceId, GridPoint, JunctionId, LayoutIndex, ParamValue, Schematic, Subject,
    SymbolId,
};

use crate::edit::Edit;
use crate::error::CommandError;
use crate::ops::{movable, reroute, set_param, still_fits};
use crate::transaction::Transaction;

/// Which edge of the selection the elements line up on. Positions are the
/// symbol origins and the junction points, the points the grid snaps.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Align {
    /// The smallest x.
    Left,
    /// The largest x.
    Right,
    /// The smallest y.
    Top,
    /// The largest y.
    Bottom,
}

/// The direction along which [`distribute`] spaces elements.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Axis {
    /// Along x.
    Horizontal,
    /// Along y.
    Vertical,
}

/// Set (`Some`) or clear (`None`) one parameter of every device in
/// `devices`, as one transaction. Each change is checked as by
/// [`set_param`].
///
/// # Errors
///
/// The first error [`set_param`] gives for one of
/// the devices; then nothing is changed.
pub fn set_param_many(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    devices: &BTreeSet<DeviceId>,
    key: &str,
    value: Option<ParamValue>,
) -> Result<Transaction, CommandError> {
    let mut tx = Transaction::new(
        format!("Set {key} of {} devices", devices.len()),
        doc.next_id,
    );
    for device in devices {
        let one = set_param(doc, catalog, *device, key, value.clone())?;
        tx.edits.extend(one.edits);
    }
    Ok(tx)
}

/// Line the symbols and junctions of a selection up on one of its edges.
/// The wires of the moved elements follow, keeping their bends where they
/// can.
///
/// # Errors
///
/// Returns [`CommandError`] for an element that does not exist or a symbol
/// that would leave the coordinate range.
pub fn align(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    subjects: &BTreeSet<Subject>,
    edge: Align,
) -> Result<Transaction, CommandError> {
    crate::layers::ensure_movable_unlocked(doc, subjects)?;
    let elements = positions(doc, subjects)?;
    let coordinate = |point: GridPoint| match edge {
        Align::Left | Align::Right => point.x,
        Align::Top | Align::Bottom => point.y,
    };
    let values = elements.iter().map(|(_, point)| coordinate(*point));
    let target = match edge {
        Align::Left | Align::Top => values.min(),
        Align::Right | Align::Bottom => values.max(),
    };
    let Some(target) = target else {
        return Ok(Transaction::new("Align", doc.next_id));
    };
    let moved = elements
        .into_iter()
        .map(|(element, point)| {
            let to = match edge {
                Align::Left | Align::Right => GridPoint::new(target, point.y),
                Align::Top | Align::Bottom => GridPoint::new(point.x, target),
            };
            (element, to)
        })
        .collect();
    place_each(doc, catalog, moved, "Align")
}

/// Space the symbols and junctions of a selection evenly along an axis,
/// keeping the first and the last where they are. The gaps are whole grid
/// units, so they may differ by one. With fewer than three elements nothing
/// moves.
///
/// # Errors
///
/// Returns [`CommandError`] for an element that does not exist or a symbol
/// that would leave the coordinate range.
pub fn distribute(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    subjects: &BTreeSet<Subject>,
    axis: Axis,
) -> Result<Transaction, CommandError> {
    crate::layers::ensure_movable_unlocked(doc, subjects)?;
    let mut elements = positions(doc, subjects)?;
    let coordinate = |point: GridPoint| match axis {
        Axis::Horizontal => i64::from(point.x),
        Axis::Vertical => i64::from(point.y),
    };
    elements.sort_by_key(|(element, point)| (coordinate(*point), *element));
    let (Some((_, first)), Some((_, last)), true) =
        (elements.first(), elements.last(), elements.len() >= 3)
    else {
        return Ok(Transaction::new("Distribute", doc.next_id));
    };
    let (start, end) = (coordinate(*first), coordinate(*last));
    let gaps = i64::try_from(elements.len().saturating_sub(1)).unwrap_or(i64::MAX);
    let span = end.saturating_sub(start);
    let mut moved = Vec::with_capacity(elements.len());
    for (index, (element, point)) in (0_i64..).zip(elements) {
        let offset = span
            .checked_mul(index)
            .and_then(|scaled| scaled.checked_div(gaps))
            .ok_or(CommandError::OutOfRange)?;
        let value =
            i32::try_from(start.saturating_add(offset)).map_err(|_| CommandError::OutOfRange)?;
        let to = match axis {
            Axis::Horizontal => GridPoint::new(value, point.y),
            Axis::Vertical => GridPoint::new(point.x, value),
        };
        moved.push((element, to));
    }
    place_each(doc, catalog, moved, "Distribute")
}

/// A symbol or a junction of a selection.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Element {
    Symbol(SymbolId),
    Junction(JunctionId),
}

/// The symbols (named, or of a named device) and junctions of a selection,
/// with their positions, in ID order.
fn positions(
    doc: &Schematic,
    subjects: &BTreeSet<Subject>,
) -> Result<Vec<(Element, GridPoint)>, CommandError> {
    let (symbols, junctions) = movable(doc, subjects);
    let mut out = Vec::with_capacity(symbols.len().saturating_add(junctions.len()));
    for id in symbols {
        let symbol = doc
            .symbols
            .iter()
            .find(|s| s.id == id)
            .ok_or(CommandError::Missing(Subject::Symbol(id)))?;
        out.push((Element::Symbol(id), symbol.position));
    }
    for id in junctions {
        let junction = doc
            .junctions
            .iter()
            .find(|j| j.id == id)
            .ok_or(CommandError::Missing(Subject::Junction(id)))?;
        out.push((Element::Junction(id), junction.position));
    }
    Ok(out)
}

/// Move each element to its own point, then route the wires of the moved
/// elements, in one transaction.
fn place_each(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    moved: Vec<(Element, GridPoint)>,
    label: &str,
) -> Result<Transaction, CommandError> {
    let layout = LayoutIndex::new(doc, catalog);
    let mut tx = Transaction::new(label, doc.next_id);
    let mut symbols = BTreeSet::new();
    let mut junctions = BTreeSet::new();
    for (element, to) in moved {
        match element {
            Element::Symbol(id) => {
                let Some(symbol) = doc.symbols.iter().find(|s| s.id == id) else {
                    return Err(CommandError::Missing(Subject::Symbol(id)));
                };
                if symbol.position == to {
                    continue;
                }
                still_fits(&layout, id, to, symbol.rotation)?;
                symbols.insert(id);
                tx.edits.push(Edit::Place {
                    symbol: id,
                    position: to,
                    rotation: symbol.rotation,
                });
            }
            Element::Junction(id) => {
                let Some(junction) = doc.junctions.iter().find(|j| j.id == id) else {
                    return Err(CommandError::Missing(Subject::Junction(id)));
                };
                if junction.position == to {
                    continue;
                }
                junctions.insert(id);
                tx.edits.push(Edit::MoveJunction {
                    junction: id,
                    position: to,
                });
            }
        }
    }
    if tx.is_empty() {
        return Ok(tx);
    }
    let (placed, _) = tx.apply_edits(doc)?;
    let routes = reroute(doc, &placed, catalog, &symbols, &junctions, None)?;
    tx.edits.extend(routes);
    Ok(tx)
}
