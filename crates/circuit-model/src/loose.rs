//! Drawing mistakes that look connected and are not (ADR-0014).
//!
//! In this model two things are connected only by a wire between them
//! (`Wire::from` and `Wire::to`), never because they touch. A person looking
//! at the paper cannot tell the difference, so the compiler warns about the
//! two ways a drawing lies:
//!
//! - **`OC-STR-018`**: a terminal or a junction sits on another terminal, on
//!   another junction or on the middle of a wire that is not in its net.
//! - **`OC-STR-019`**: two wires of different nets run over the same stretch
//!   of a line, so that they look like one. Wires of one net that overlap
//!   say the same thing twice and are left alone.
//!
//! Only horizontal and vertical segments are examined (a diagonal one has its
//! own warning, `OC-STR-009`). The checks are bounded: they stop after
//! [`MAX_COMPARISONS`] comparisons and report at most [`MAX_REPORTED`]
//! findings of each code, so a hostile document costs a fixed amount of work.

use std::collections::{BTreeMap, BTreeSet};

use crate::catalog::ComponentCatalog;
use crate::diagnostics::{Diagnostic, DiagnosticCode, Subject};
use crate::geometry::GridPoint;
use crate::ids::{JunctionId, SymbolId, WireId};
use crate::layout::LayoutIndex;
use crate::nets::StaticNets;
use crate::schematic::{Endpoint, PinRef, Schematic, TerminalKey};

/// Most comparisons between a point and a segment, or between two segments,
/// that one call makes.
pub const MAX_COMPARISONS: usize = 4_000_000;
/// Most findings of one code that one call reports.
pub const MAX_REPORTED: usize = 1_000;

/// What a point is: a terminal of a symbol, or a junction.
#[derive(Clone, Copy)]
enum Kind<'a> {
    Pin(SymbolId, &'a TerminalKey),
    Junction(JunctionId),
}

/// A terminal or a junction, with where it is.
#[derive(Clone, Copy)]
struct Point<'a> {
    kind: Kind<'a>,
    at: GridPoint,
}

impl Point<'_> {
    /// The order of points at one place: terminals before junctions, as
    /// `Subject` orders them, each by ID; it costs no allocation.
    fn order(&self) -> (u8, u64, &str) {
        match self.kind {
            Kind::Pin(symbol_id, terminal) => (0, symbol_id.0, terminal.0.as_str()),
            Kind::Junction(id) => (1, id.0, ""),
        }
    }

    fn subject(&self) -> Subject {
        match self.kind {
            Kind::Pin(symbol_id, terminal) => Subject::Pin(PinRef {
                symbol_id,
                terminal: terminal.clone(),
            }),
            Kind::Junction(id) => Subject::Junction(id),
        }
    }

    fn endpoint(&self) -> Endpoint {
        match self.kind {
            Kind::Pin(symbol_id, terminal) => Endpoint::Terminal(PinRef {
                symbol_id,
                terminal: terminal.clone(),
            }),
            Kind::Junction(id) => Endpoint::Junction(id),
        }
    }
}

/// A horizontal or vertical stretch of a wire, `from <= to` along its line.
#[derive(Clone, Copy)]
struct Stretch {
    from: i32,
    to: i32,
    wire: WireId,
}

/// Something that looks connected, before it is known whether it is: the
/// nets decide, and are built only when there is a candidate at all.
enum Candidate {
    /// Two points at one place, by index into the sorted points.
    Touch(usize, usize),
    /// A point on the middle of a wire.
    OnWire(usize, WireId),
    /// Two wires over one stretch of a line, the smaller ID first.
    Overlap(WireId, WireId),
}

/// The two diagnostics, in no particular order.
#[must_use]
pub fn loose_geometry(doc: &Schematic, catalog: &ComponentCatalog) -> Vec<Diagnostic> {
    let layout = LayoutIndex::new(doc, catalog);

    // Every terminal that resolves, then every junction, sorted by place.
    let mut points: Vec<Point<'_>> = Vec::new();
    for symbol in &doc.symbols {
        let Some(definition) = layout.definition(symbol.id) else {
            continue;
        };
        for terminal in definition.terminals() {
            let at = symbol.place(terminal.offset);
            if let Some(at) = at {
                points.push(Point {
                    kind: Kind::Pin(symbol.id, &terminal.key),
                    at,
                });
            }
        }
    }
    for junction in &doc.junctions {
        points.push(Point {
            kind: Kind::Junction(junction.id),
            at: junction.position,
        });
    }
    points.sort_by(|a, b| (a.at, a.order()).cmp(&(b.at, b.order())));
    points.dedup_by(|a, b| (a.at, a.order()) == (b.at, b.order()));

    // Horizontal stretches by row and vertical ones by column; the ends of a
    // wire are kept apart so that a terminal at an end is not "on" the wire.
    let mut rows: BTreeMap<i32, Vec<Stretch>> = BTreeMap::new();
    let mut columns: BTreeMap<i32, Vec<Stretch>> = BTreeMap::new();
    let mut ends: BTreeMap<WireId, [GridPoint; 2]> = BTreeMap::new();
    let mut wires: Vec<_> = doc.wires.iter().collect();
    wires.sort_by_key(|wire| wire.id);
    for wire in wires {
        let Some(path) = layout.wire_polyline(wire) else {
            continue;
        };
        if let (Some(first), Some(last)) = (path.first(), path.last()) {
            ends.entry(wire.id).or_insert([*first, *last]);
        }
        for pair in path.windows(2) {
            let [a, b] = pair else {
                continue;
            };
            if a == b {
                continue;
            }
            if a.y == b.y {
                rows.entry(a.y).or_default().push(Stretch {
                    from: a.x.min(b.x),
                    to: a.x.max(b.x),
                    wire: wire.id,
                });
            } else if a.x == b.x {
                columns.entry(a.x).or_default().push(Stretch {
                    from: a.y.min(b.y),
                    to: a.y.max(b.y),
                    wire: wire.id,
                });
            }
        }
    }

    let mut budget = MAX_COMPARISONS;
    let mut candidates = Vec::new();
    touching_points(&points, &mut budget, &mut candidates);
    points_on_wires(
        &points,
        &rows,
        &columns,
        &ends,
        &mut budget,
        &mut candidates,
    );
    overlapping_wires(&rows, &columns, &mut budget, &mut candidates);
    if candidates.is_empty() {
        return Vec::new();
    }

    // Only now are the nets needed: a candidate is loose when its two sides
    // are in different nets.
    let nets = StaticNets::build(doc, catalog);
    let net_of_point = |point: &Point<'_>| nets.net_of_endpoint(&point.endpoint());
    let mut out = Vec::new();
    let (mut loose, mut overlapping) = (0_usize, 0_usize);
    for candidate in candidates {
        match candidate {
            Candidate::Touch(first, second) => {
                let (Some(first), Some(second)) = (points.get(first), points.get(second)) else {
                    continue;
                };
                let (a, b) = (net_of_point(first), net_of_point(second));
                if a.is_some() && b.is_some() && a != b && loose < MAX_REPORTED {
                    loose = loose.saturating_add(1);
                    let (one, other) = (first.subject(), second.subject());
                    out.push(
                        Diagnostic::new(
                            DiagnosticCode::LooseConnection,
                            one.clone(),
                            format!("{one} is on {other} but is not connected to it"),
                        )
                        .with_related(vec![other]),
                    );
                }
            }
            Candidate::OnWire(index, wire) => {
                let Some(point) = points.get(index) else {
                    continue;
                };
                let (a, b) = (net_of_point(point), nets.net_of_wire(wire));
                if a.is_some() && b.is_some() && a != b && loose < MAX_REPORTED {
                    loose = loose.saturating_add(1);
                    let one = point.subject();
                    out.push(
                        Diagnostic::new(
                            DiagnosticCode::LooseConnection,
                            one.clone(),
                            format!("{one} is on wire {wire} but is not connected to it"),
                        )
                        .with_related(vec![Subject::Wire(wire)]),
                    );
                }
            }
            Candidate::Overlap(first, second) => {
                let (a, b) = (nets.net_of_wire(first), nets.net_of_wire(second));
                if a.is_some() && b.is_some() && a != b && overlapping < MAX_REPORTED {
                    overlapping = overlapping.saturating_add(1);
                    out.push(
                        Diagnostic::new(
                            DiagnosticCode::OverlappingWires,
                            Subject::Wire(first),
                            format!("wires {first} and {second} run over the same line"),
                        )
                        .with_related(vec![Subject::Wire(second)]),
                    );
                }
            }
        }
    }
    out
}

/// Spend `n` comparisons; `false` when the budget is gone.
fn spend(budget: &mut usize, n: usize) -> bool {
    match budget.checked_sub(n) {
        Some(left) => {
            *budget = left;
            true
        }
        None => {
            *budget = 0;
            false
        }
    }
}

/// Points at one place.
fn touching_points(points: &[Point<'_>], budget: &mut usize, out: &mut Vec<Candidate>) {
    let mut start = 0_usize;
    for group in points.chunk_by(|a, b| a.at == b.at) {
        for first in 0..group.len() {
            for second in first.saturating_add(1)..group.len() {
                if !spend(budget, 1) {
                    return;
                }
                out.push(Candidate::Touch(
                    start.saturating_add(first),
                    start.saturating_add(second),
                ));
            }
        }
        start = start.saturating_add(group.len());
    }
}

/// Points on the middle of a wire (a bend counts as the middle).
fn points_on_wires(
    points: &[Point<'_>],
    rows: &BTreeMap<i32, Vec<Stretch>>,
    columns: &BTreeMap<i32, Vec<Stretch>>,
    ends: &BTreeMap<WireId, [GridPoint; 2]>,
    budget: &mut usize,
    out: &mut Vec<Candidate>,
) {
    let mut seen: BTreeSet<(usize, WireId)> = BTreeSet::new();
    for (index, point) in points.iter().enumerate() {
        let lines = [
            rows.get(&point.at.y).map(|row| (row, point.at.x)),
            columns.get(&point.at.x).map(|column| (column, point.at.y)),
        ];
        for (stretches, along) in lines.into_iter().flatten() {
            if !spend(budget, stretches.len()) {
                return;
            }
            for stretch in stretches {
                let inside = stretch.from <= along && along <= stretch.to;
                let at_end = ends
                    .get(&stretch.wire)
                    .is_some_and(|ends| ends.contains(&point.at));
                if inside && !at_end && seen.insert((index, stretch.wire)) {
                    out.push(Candidate::OnWire(index, stretch.wire));
                }
            }
        }
    }
}

/// Wires that share a stretch of a line.
fn overlapping_wires(
    rows: &BTreeMap<i32, Vec<Stretch>>,
    columns: &BTreeMap<i32, Vec<Stretch>>,
    budget: &mut usize,
    out: &mut Vec<Candidate>,
) {
    let mut seen: BTreeSet<(WireId, WireId)> = BTreeSet::new();
    for stretches in rows.values().chain(columns.values()) {
        if !spend(budget, stretches.len()) {
            return;
        }
        let mut sorted = stretches.clone();
        sorted.sort_by_key(|s| (s.from, s.to, s.wire));
        // The stretch that reaches furthest so far.
        let mut reach: Option<Stretch> = None;
        for stretch in sorted {
            if let Some(far) = reach.filter(|far| stretch.from < far.to && stretch.wire != far.wire)
            {
                let pair = (stretch.wire.min(far.wire), stretch.wire.max(far.wire));
                if seen.insert(pair) {
                    out.push(Candidate::Overlap(pair.0, pair.1));
                }
            }
            if reach.is_none_or(|far| stretch.to > far.to) {
                reach = Some(stretch);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::{SchematicBuilder, pin};
    use crate::ids::SymbolId;

    fn point(x: i32, y: i32) -> GridPoint {
        GridPoint::new(x, y)
    }

    fn codes(doc: &Schematic) -> Vec<(DiagnosticCode, Subject, Vec<Subject>)> {
        let mut found: Vec<_> = loose_geometry(doc, &ComponentCatalog::builtin())
            .into_iter()
            .map(|d| (d.code(), d.subject().clone(), d.related().to_vec()))
            .collect();
        found.sort();
        found
    }

    /// Two pushbuttons. S1's contact has p1 at (0, -2) and p2 at (0, 2) when
    /// it is at the origin.
    fn two(first: GridPoint, second: GridPoint) -> (SchematicBuilder, SymbolId, SymbolId) {
        let mut b = SchematicBuilder::new("loose");
        let a = b.device("S1", "pushbutton_no", &[]).unwrap();
        let c = b.device("S2", "pushbutton_no", &[]).unwrap();
        let first = b.symbol(a, "contact", None, first).unwrap();
        let second = b.symbol(c, "contact", None, second).unwrap();
        (b, first, second)
    }

    fn pin_subject(symbol: SymbolId, key: &str) -> Subject {
        Subject::Pin(crate::schematic::PinRef {
            symbol_id: symbol,
            terminal: crate::schematic::TerminalKey(key.to_owned()),
        })
    }

    #[test]
    fn two_pins_at_one_place_that_no_wire_joins_are_reported_once() {
        // S1.p2 is at (0, 2) and S2.p1 at (0, 2) too.
        let (b, s1, s2) = two(point(0, 0), point(0, 4));
        let doc = b.finish();

        assert_eq!(
            codes(&doc),
            [(
                DiagnosticCode::LooseConnection,
                pin_subject(s1, "p2"),
                vec![pin_subject(s2, "p1")]
            )]
        );
    }

    #[test]
    fn a_wire_between_two_pins_at_one_place_makes_them_one_net_and_no_finding() {
        let (mut b, s1, s2) = two(point(0, 0), point(0, 4));
        b.wire(pin(s1, "p2"), pin(s2, "p1")).unwrap();

        assert_eq!(codes(&b.finish()), []);
    }

    #[test]
    fn a_pin_on_the_middle_of_a_wire_of_another_net_is_reported() {
        // A wire from a junction at (-5, 2) to one at (5, 2) passes through
        // S2's p1 at (0, 2).
        let (mut b, s1, s2) = two(point(20, 0), point(0, 4));
        let left = b.junction(point(-5, 2)).unwrap();
        let right = b.junction(point(5, 2)).unwrap();
        b.wire(left, right).unwrap();
        let _ = s1;
        let doc = b.finish();

        let found = codes(&doc);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, DiagnosticCode::LooseConnection);
        assert_eq!(found[0].1, pin_subject(s2, "p1"));
        assert_eq!(found[0].2, vec![Subject::Wire(doc.wires[0].id)]);
    }

    #[test]
    fn a_wire_that_ends_at_the_pin_is_not_loose() {
        let (mut b, s1, s2) = two(point(20, 0), point(0, 4));
        let left = b.junction(point(-5, 2)).unwrap();
        b.wire(left, pin(s2, "p1")).unwrap();
        let _ = s1;

        assert_eq!(codes(&b.finish()), []);
    }

    #[test]
    fn a_junction_drawn_on_a_wire_it_does_not_join_is_reported() {
        let mut b = SchematicBuilder::new("t");
        let a = b.junction(point(0, 0)).unwrap();
        let c = b.junction(point(10, 0)).unwrap();
        b.wire(a, c).unwrap();
        let tee = b.junction(point(5, 0)).unwrap();
        let below = b.junction(point(5, 5)).unwrap();
        b.wire(tee, below).unwrap();
        let doc = b.finish();

        let found = codes(&doc);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].1, Subject::Junction(tee));
        assert_eq!(found[0].2, vec![Subject::Wire(doc.wires[0].id)]);
    }

    #[test]
    fn a_wire_split_by_a_junction_is_not_loose() {
        let mut b = SchematicBuilder::new("t");
        let a = b.junction(point(0, 0)).unwrap();
        let tee = b.junction(point(5, 0)).unwrap();
        let c = b.junction(point(10, 0)).unwrap();
        b.wire(a, tee).unwrap();
        b.wire(tee, c).unwrap();

        assert_eq!(codes(&b.finish()), []);
    }

    #[test]
    fn a_waypoint_of_another_net_counts_as_the_middle_of_a_wire() {
        let mut b = SchematicBuilder::new("t");
        let a = b.junction(point(0, 0)).unwrap();
        let c = b.junction(point(10, 10)).unwrap();
        // The wire bends at (10, 0).
        b.wire_routed(a, c, &[point(10, 0)]).unwrap();
        let on_bend = b.junction(point(10, 0)).unwrap();
        let other = b.junction(point(20, 0)).unwrap();
        b.wire(on_bend, other).unwrap();

        let found = codes(&b.finish());

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].1, Subject::Junction(on_bend));
    }

    #[test]
    fn two_wires_over_the_same_line_are_reported_once_and_ones_that_only_meet_are_not() {
        let mut b = SchematicBuilder::new("t");
        let a = b.junction(point(0, 0)).unwrap();
        let c = b.junction(point(10, 0)).unwrap();
        let d = b.junction(point(4, 0)).unwrap();
        let e = b.junction(point(14, 0)).unwrap();
        b.wire(a, c).unwrap();
        b.wire(d, e).unwrap();
        // Meets the first at its end only.
        let f = b.junction(point(10, 0)).unwrap();
        let g = b.junction(point(10, 8)).unwrap();
        b.wire(f, g).unwrap();
        let doc = b.finish();

        let overlaps: Vec<_> = codes(&doc)
            .into_iter()
            .filter(|(code, _, _)| *code == DiagnosticCode::OverlappingWires)
            .collect();

        assert_eq!(
            overlaps,
            [(
                DiagnosticCode::OverlappingWires,
                Subject::Wire(doc.wires[0].id),
                vec![Subject::Wire(doc.wires[1].id)]
            )]
        );
    }

    #[test]
    fn wires_of_one_net_that_overlap_say_the_same_thing_twice_and_are_left_alone() {
        let mut b = SchematicBuilder::new("t");
        let hub = b.junction(point(0, 0)).unwrap();
        let near = b.junction(point(5, 0)).unwrap();
        let far = b.junction(point(10, 0)).unwrap();
        // Both leave the same junction along the same line.
        b.wire(hub, near).unwrap();
        b.wire(hub, far).unwrap();

        assert_eq!(codes(&b.finish()), []);
    }

    #[test]
    fn a_vertical_overlap_is_found_too_and_a_wire_does_not_overlap_itself() {
        let mut b = SchematicBuilder::new("t");
        let a = b.junction(point(0, 0)).unwrap();
        let c = b.junction(point(0, 10)).unwrap();
        let d = b.junction(point(0, 5)).unwrap();
        let e = b.junction(point(0, 20)).unwrap();
        b.wire(a, c).unwrap();
        b.wire(d, e).unwrap();
        // Doubles back on itself: not an overlap of two wires.
        let f = b.junction(point(30, 0)).unwrap();
        let g = b.junction(point(30, 10)).unwrap();
        b.wire_routed(f, g, &[point(30, 20), point(30, 5)]).unwrap();

        let overlaps = codes(&b.finish())
            .into_iter()
            .filter(|(code, _, _)| *code == DiagnosticCode::OverlappingWires)
            .count();

        assert_eq!(overlaps, 1);
    }

    #[test]
    fn the_findings_do_not_depend_on_the_order_of_the_collections() {
        let (mut b, s1, s2) = two(point(0, 0), point(0, 4));
        let a = b.junction(point(-5, 2)).unwrap();
        let c = b.junction(point(5, 2)).unwrap();
        b.wire(a, c).unwrap();
        let _ = (s1, s2);
        let mut doc = b.finish();
        let expected = codes(&doc);

        doc.devices.reverse();
        doc.symbols.reverse();
        doc.junctions.reverse();
        doc.wires.reverse();

        assert_eq!(codes(&doc), expected);
    }

    #[test]
    fn a_document_with_nothing_wrong_has_no_finding() {
        assert_eq!(codes(&Schematic::new("empty")), []);
    }
}
