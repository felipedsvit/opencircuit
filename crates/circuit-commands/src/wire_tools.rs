//! Wire tools (M9.2, issue #118): trim a wire at a point, extend a free end
//! to the next connection, and copy wires alongside. Each is one transaction
//! that one undo takes back.
//!
//! A free end is a junction only one wire ends at and no other wire passes
//! through: the end of a wire drawn into empty space.

use circuit_model::{
    ComponentCatalog, Endpoint, GridPoint, IdAllocator, Junction, JunctionId, LayoutIndex, PinRef,
    Schematic, Subject, Wire, WireId, segment_at,
};

use crate::ops::{Anchor, resolve};
use crate::{CommandError, Edit, Transaction, layers};

/// Farthest an end is extended, in grid units.
pub const MAX_EXTEND: i32 = 500;

/// Most wires one parallel copy takes.
pub const MAX_COPIED_WIRES: usize = 256;

fn wire_of(doc: &Schematic, wire: WireId) -> Result<&Wire, CommandError> {
    doc.wires
        .iter()
        .find(|candidate| candidate.id == wire)
        .ok_or(CommandError::Missing(Subject::Wire(wire)))
}

/// Length of a polyline along its segments, in grid units.
fn length(path: &[GridPoint]) -> i64 {
    path.windows(2)
        .map(|pair| match pair {
            [a, b] => i64::from(a.x.abs_diff(b.x)).saturating_add(i64::from(a.y.abs_diff(b.y))),
            _ => 0,
        })
        .fold(0, i64::saturating_add)
}

/// The wire through `at` (between its ends or at one of them), the lowest
/// ID when several are.
#[must_use]
pub fn wire_at(doc: &Schematic, catalog: &ComponentCatalog, at: GridPoint) -> Option<WireId> {
    let layout = LayoutIndex::new(doc, catalog);
    doc.wires
        .iter()
        .filter(|wire| {
            layout
                .wire_polyline(wire)
                .is_some_and(|path| segment_at(&path, at).is_some())
        })
        .map(|wire| wire.id)
        .min()
}

/// Whether only `wire` ends at `junction`.
fn only_end(doc: &Schematic, junction: JunctionId, wire: WireId) -> bool {
    doc.wires.iter().all(|other| {
        other.id == wire
            || (other.from != Endpoint::Junction(junction)
                && other.to != Endpoint::Junction(junction))
    })
}

/// Cut `wire` at `at`, a point of it that is not one of its ends, and remove
/// the part from there to its nearer end (on a tie, its last end). The wire
/// then ends at a new junction at `at`; a junction left with no wire is
/// removed.
///
/// # Errors
///
/// Returns [`CommandError::Missing`] for a wire that does not exist,
/// [`CommandError::NotOnWire`] for a point that is not on it or is one of
/// its ends, [`CommandError::Locked`] for a wire on a locked layer, and
/// [`CommandError::Ids`] when the document has no ID left.
pub fn trim(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    wire: WireId,
    at: GridPoint,
) -> Result<Transaction, CommandError> {
    let found = wire_of(doc, wire)?;
    layers::ensure_wire_unlocked(doc, wire)?;
    let layout = LayoutIndex::new(doc, catalog);
    let path = layout
        .wire_polyline(found)
        .ok_or(CommandError::Missing(Subject::Wire(wire)))?;
    if path.first() == Some(&at) || path.last() == Some(&at) {
        return Err(CommandError::NotOnWire(wire));
    }
    let segment = segment_at(&path, at).ok_or(CommandError::NotOnWire(wire))?;
    let head: Vec<GridPoint> = path
        .iter()
        .take(segment.saturating_add(1))
        .copied()
        .chain(std::iter::once(at))
        .collect();
    let tail: Vec<GridPoint> = std::iter::once(at)
        .chain(path.iter().skip(segment.saturating_add(1)).copied())
        .collect();
    // Keep the longer part: the shorter one ends nearer the point.
    let keep_head = length(&head) >= length(&tail);
    let mut ids = IdAllocator::starting_at(doc.next_id);
    let junction = JunctionId(ids.alloc()?);
    let kept = WireId(ids.alloc()?);
    let inner = |points: &[GridPoint]| -> Vec<GridPoint> {
        let last = points.len().saturating_sub(1);
        points
            .iter()
            .enumerate()
            .filter(|(index, point)| *index != 0 && *index != last && **point != at)
            .map(|(_, point)| *point)
            .collect()
    };
    let (from, to, route, dropped) = if keep_head {
        (
            found.from.clone(),
            Endpoint::Junction(junction),
            inner(&head),
            &found.to,
        )
    } else {
        (
            Endpoint::Junction(junction),
            found.to.clone(),
            inner(&tail),
            &found.from,
        )
    };
    let mut tx = Transaction::new("Trim wire", doc.next_id);
    tx.edits.push(Edit::RemoveWire(wire));
    // No `let` chain: the core builds on Rust 1.85.
    if let Endpoint::Junction(old) = dropped {
        if only_end(doc, *old, wire) {
            tx.edits.push(Edit::RemoveJunction(*old));
        }
    }
    tx.edits.push(Edit::InsertJunction(Junction {
        id: junction,
        position: at,
    }));
    tx.edits.push(Edit::InsertWire(Wire {
        id: kept,
        from,
        to,
        route,
        number: found.number.clone(),
        layer: found.layer,
    }));
    tx.next_id = ids.next_id();
    Ok(tx)
}

/// Extend the free end of a wire at `end` along its last segment to the
/// first terminal, junction or wire it meets within [`MAX_EXTEND`] grid
/// units, and connect it there; meeting a wire between its ends splits it.
///
/// # Errors
///
/// Returns [`CommandError::Missing`] when no wire has a free end at `end`,
/// [`CommandError::NothingToReach`] when nothing is met, and the errors of
/// a locked layer or of the IDs.
pub fn extend(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    end: GridPoint,
) -> Result<Transaction, CommandError> {
    let layout = LayoutIndex::new(doc, catalog);
    let free = doc
        .junctions
        .iter()
        .filter(|junction| junction.position == end)
        .find_map(|junction| {
            let mut wires = doc.wires.iter().filter(|wire| {
                wire.from == Endpoint::Junction(junction.id)
                    || wire.to == Endpoint::Junction(junction.id)
            });
            match (wires.next(), wires.next()) {
                (Some(wire), None) => Some((junction.id, wire)),
                _ => None,
            }
        })
        .ok_or(CommandError::NothingToReach(end))?;
    let (junction, wire) = free;
    layers::ensure_wire_unlocked(doc, wire.id)?;
    let path = layout
        .wire_polyline(wire)
        .ok_or(CommandError::Missing(Subject::Wire(wire.id)))?;
    let at_end = wire.to == Endpoint::Junction(junction);
    let (last, before) = if at_end {
        (path.last(), path.iter().rev().nth(1))
    } else {
        (path.first(), path.get(1))
    };
    let (Some(last), Some(before)) = (last.copied(), before.copied()) else {
        return Err(CommandError::NothingToReach(end));
    };
    let step = (last.x.cmp(&before.x) as i32, last.y.cmp(&before.y) as i32);
    if step == (0, 0) || (step.0 != 0 && step.1 != 0) {
        return Err(CommandError::NothingToReach(end));
    }

    // What is at a point: a terminal, then a junction, then another wire.
    let pins: Vec<(GridPoint, PinRef)> = doc
        .symbols
        .iter()
        .filter_map(|symbol| Some((symbol, layout.definition(symbol.id)?)))
        .flat_map(|(symbol, definition)| {
            definition.terminals().iter().map(move |terminal| PinRef {
                symbol_id: symbol.id,
                terminal: terminal.key.clone(),
            })
        })
        .filter_map(|pin| Some((layout.pin_position(&pin)?, pin)))
        .collect();
    let paths: Vec<(WireId, Vec<GridPoint>)> = doc
        .wires
        .iter()
        .filter(|other| other.id != wire.id)
        .filter_map(|other| Some((other.id, layout.wire_polyline(other)?)))
        .collect();
    let mut target = None;
    for distance in 1..=MAX_EXTEND {
        let (Some(x), Some(y)) = (
            last.x.checked_add(step.0.saturating_mul(distance)),
            last.y.checked_add(step.1.saturating_mul(distance)),
        ) else {
            break;
        };
        let point = GridPoint::new(x, y);
        if let Some((_, pin)) = pins
            .iter()
            .filter(|(at, _)| *at == point)
            .min_by_key(|(_, pin)| pin.clone())
        {
            target = Some(Anchor::Endpoint(Endpoint::Terminal(pin.clone())));
        } else if let Some(other) = doc
            .junctions
            .iter()
            .filter(|other| other.position == point && other.id != junction)
            .map(|other| other.id)
            .min()
        {
            target = Some(Anchor::Endpoint(Endpoint::Junction(other)));
        } else if let Some((id, _)) = paths
            .iter()
            .filter(|(_, path)| segment_at(path, point).is_some())
            .min_by_key(|(id, _)| *id)
        {
            layers::ensure_wire_unlocked(doc, *id)?;
            target = Some(Anchor::OnWire {
                wire: *id,
                at: point,
            });
        }
        if target.is_some() {
            break;
        }
    }
    let target = target.ok_or(CommandError::NothingToReach(end))?;

    let mut ids = IdAllocator::starting_at(doc.next_id);
    let mut edits = Vec::new();
    let (reached, _) = resolve(doc, &layout, &target, &mut ids, &mut edits)?;
    let id = WireId(ids.alloc()?);
    let (from, to) = if at_end {
        (wire.from.clone(), reached)
    } else {
        (reached, wire.to.clone())
    };
    let mut tx = Transaction::new("Extend wire", doc.next_id);
    tx.edits.push(Edit::RemoveWire(wire.id));
    tx.edits.push(Edit::RemoveJunction(junction));
    tx.edits.extend(edits);
    tx.edits.push(Edit::InsertWire(Wire {
        id,
        from,
        to,
        route: wire.route.clone(),
        number: wire.number.clone(),
        layer: wire.layer,
    }));
    tx.next_id = ids.next_id();
    Ok(tx)
}

/// Copy `wires` moved by `offset` grid units: each copy runs alongside its
/// wire, between two new junctions, connected to nothing.
///
/// # Errors
///
/// Returns [`CommandError::Missing`] for a wire that does not exist,
/// [`CommandError::InvalidParameter`] for a zero offset, more than
/// [`MAX_COPIED_WIRES`] wires or a copy beyond the coordinate range, and
/// [`CommandError::Ids`] when the document has no ID left.
pub fn parallel_copy(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    wires: &[WireId],
    offset: (i32, i32),
) -> Result<Transaction, CommandError> {
    if offset == (0, 0) {
        return Err(CommandError::InvalidParameter(
            "a parallel copy needs an offset".to_owned(),
        ));
    }
    if wires.len() > MAX_COPIED_WIRES {
        return Err(CommandError::InvalidParameter(format!(
            "at most {MAX_COPIED_WIRES} wires are copied at once"
        )));
    }
    let layout = LayoutIndex::new(doc, catalog);
    let shift = |point: &GridPoint| -> Result<GridPoint, CommandError> {
        match (point.x.checked_add(offset.0), point.y.checked_add(offset.1)) {
            (Some(x), Some(y)) => Ok(GridPoint::new(x, y)),
            _ => Err(CommandError::InvalidParameter(
                "the copy would leave the drawing".to_owned(),
            )),
        }
    };
    let mut ids = IdAllocator::starting_at(doc.next_id);
    let mut tx = Transaction::new("Parallel copy", doc.next_id);
    let mut sorted = wires.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    for wire in sorted {
        let found = wire_of(doc, wire)?;
        let path = layout
            .wire_polyline(found)
            .ok_or(CommandError::Missing(Subject::Wire(wire)))?;
        let (Some(first), Some(last)) = (path.first(), path.last()) else {
            continue;
        };
        let start = JunctionId(ids.alloc()?);
        let end = JunctionId(ids.alloc()?);
        let id = WireId(ids.alloc()?);
        tx.edits.push(Edit::InsertJunction(Junction {
            id: start,
            position: shift(first)?,
        }));
        tx.edits.push(Edit::InsertJunction(Junction {
            id: end,
            position: shift(last)?,
        }));
        tx.edits.push(Edit::InsertWire(Wire {
            id,
            from: Endpoint::Junction(start),
            to: Endpoint::Junction(end),
            route: found.route.iter().map(shift).collect::<Result<_, _>>()?,
            number: None,
            layer: found.layer,
        }));
    }
    tx.next_id = ids.next_id();
    Ok(tx)
}

#[cfg(test)]
mod tests {
    use circuit_model::{SchematicBuilder, builder::pin};

    use super::*;

    fn p(x: i32, y: i32) -> GridPoint {
        GridPoint::new(x, y)
    }

    fn apply(doc: &Schematic, tx: &Transaction) -> Schematic {
        let (after, _) = tx.apply_to(doc).unwrap();
        assert!(
            after
                .validate()
                .iter()
                .all(|d| d.severity() != circuit_model::Severity::Error)
        );
        after
    }

    fn paths(doc: &Schematic) -> Vec<Vec<GridPoint>> {
        let catalog = ComponentCatalog::builtin();
        let layout = LayoutIndex::new(doc, &catalog);
        let mut all: Vec<Vec<GridPoint>> = doc
            .wires
            .iter()
            .filter_map(|wire| layout.wire_polyline(wire))
            .collect();
        all.sort();
        all
    }

    /// A from (0, 0) to (10, 0); B from (20, -5) to (20, 5).
    fn two_wires() -> (Schematic, WireId, WireId) {
        let mut b = SchematicBuilder::new("wires");
        let ends = [p(0, 0), p(10, 0), p(20, -5), p(20, 5)].map(|at| b.junction(at).unwrap());
        let a = b.wire(ends[0], ends[1]).unwrap();
        let second = b.wire(ends[2], ends[3]).unwrap();
        (b.finish(), a, second)
    }

    #[test]
    fn trim_removes_the_part_toward_the_nearer_end() {
        let (doc, a, _) = two_wires();
        let catalog = ComponentCatalog::builtin();

        let near_end = apply(&doc, &trim(&doc, &catalog, a, p(8, 0)).unwrap());
        assert!(paths(&near_end).contains(&vec![p(0, 0), p(8, 0)]));
        assert!(!near_end.junctions.iter().any(|j| j.position == p(10, 0)));

        let near_start = apply(&doc, &trim(&doc, &catalog, a, p(2, 0)).unwrap());
        assert!(paths(&near_start).contains(&vec![p(2, 0), p(10, 0)]));
        assert!(!near_start.junctions.iter().any(|j| j.position == p(0, 0)));

        assert_eq!(
            trim(&doc, &catalog, a, p(10, 0)),
            Err(CommandError::NotOnWire(a))
        );
        assert_eq!(
            trim(&doc, &catalog, a, p(5, 1)),
            Err(CommandError::NotOnWire(a))
        );
    }

    #[test]
    fn extend_reaches_a_wire_and_connects_by_splitting_it() {
        let (doc, _, second) = two_wires();
        let catalog = ComponentCatalog::builtin();

        let after = apply(&doc, &extend(&doc, &catalog, p(10, 0)).unwrap());

        assert!(paths(&after).contains(&vec![p(0, 0), p(20, 0)]));
        assert!(!after.wires.iter().any(|wire| wire.id == second), "split");
        let meeting = after
            .junctions
            .iter()
            .find(|j| j.position == p(20, 0))
            .expect("a junction where they meet")
            .id;
        let ends = after
            .wires
            .iter()
            .filter(|w| [&w.from, &w.to].contains(&&Endpoint::Junction(meeting)))
            .count();
        assert_eq!(ends, 3);
        assert!(!after.junctions.iter().any(|j| j.position == p(10, 0)));
    }

    #[test]
    fn extend_reaches_a_terminal_and_refuses_when_nothing_is_met() {
        let mut b = SchematicBuilder::new("pin");
        let s1 = b.device("S1", "pushbutton_no", &[]).unwrap();
        let contact = b.symbol(s1, "contact", None, p(0, 0)).unwrap();
        let top = b.junction(p(0, -8)).unwrap();
        let free = b.junction(p(0, -5)).unwrap();
        b.wire(top, free).unwrap();
        let doc = b.finish();
        let catalog = ComponentCatalog::builtin();

        let after = apply(&doc, &extend(&doc, &catalog, p(0, -5)).unwrap());
        assert!(after.wires.iter().any(|w| w.to == pin(contact, "p1")));

        // The other free end points up, at nothing.
        assert_eq!(
            extend(&doc, &catalog, p(0, -8)),
            Err(CommandError::NothingToReach(p(0, -8)))
        );
        assert_eq!(
            extend(&doc, &catalog, p(3, 3)),
            Err(CommandError::NothingToReach(p(3, 3)))
        );
    }

    #[test]
    fn a_parallel_copy_runs_alongside_connected_to_nothing() {
        let (doc, a, second) = two_wires();
        let catalog = ComponentCatalog::builtin();

        let after = apply(
            &doc,
            &parallel_copy(&doc, &catalog, &[a, second, a], (0, 3)).unwrap(),
        );

        let all = paths(&after);
        assert!(all.contains(&vec![p(0, 3), p(10, 3)]));
        assert!(all.contains(&vec![p(20, -2), p(20, 8)]));
        assert_eq!(after.wires.len(), 4);
        assert!(matches!(
            parallel_copy(&doc, &catalog, &[a], (0, 0)),
            Err(CommandError::InvalidParameter(_))
        ));
    }
}
