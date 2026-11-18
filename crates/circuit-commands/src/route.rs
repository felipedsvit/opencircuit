//! Wire routing on request (M9.2, issue #118): an orthogonal route between
//! the two ends of a wire that goes around symbols, in one step that one
//! undo takes back.
//!
//! The route is a shortest path on the grid where every bend costs as much
//! as [`BEND_COST`] grid units, so it prefers few bends. It does not enter
//! the area of a symbol, does not pass over a junction or a terminal other
//! than its own ends, and does not run along another wire (crossing one at a
//! right angle is fine: crossing wires are not connected). The search stays
//! within [`MARGIN`] grid units of the box around the two ends, and gives up
//! after [`MAX_STEPS`] steps.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use circuit_model::{ComponentCatalog, GridPoint, LayoutIndex, Schematic, Subject, WireId};

use crate::{CommandError, Edit, Transaction, layers};

/// What a bend costs, in grid units of length.
pub const BEND_COST: u32 = 4;
/// How far around the box of the two ends the route may go, in grid units.
pub const MARGIN: i32 = 20;
/// Most points the search looks at for one wire.
pub const MAX_STEPS: usize = 200_000;
/// Most wires routed at once.
pub const MAX_WIRES: usize = 64;

/// The four directions, as steps on the grid.
const DIRECTIONS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// What the route may not cross.
struct Obstacles {
    /// Points no route goes through.
    blocked: BTreeSet<GridPoint>,
    /// Points where another wire runs horizontally, or vertically: a route
    /// may cross them at a right angle but not run along them.
    along_x: BTreeSet<GridPoint>,
    along_y: BTreeSet<GridPoint>,
}

impl Obstacles {
    fn of(doc: &Schematic, layout: &LayoutIndex<'_>, wire: WireId) -> Self {
        let mut blocked = BTreeSet::new();
        for symbol in &doc.symbols {
            let Some(bounds) = layout.symbol_bounds(symbol.id) else {
                continue;
            };
            // A symbol covers at most a few dozen grid points.
            for x in bounds.min.x..=bounds.max.x {
                for y in bounds.min.y..=bounds.max.y {
                    blocked.insert(GridPoint::new(x, y));
                }
            }
        }
        blocked.extend(doc.junctions.iter().map(|junction| junction.position));
        let mut along_x = BTreeSet::new();
        let mut along_y = BTreeSet::new();
        for other in doc.wires.iter().filter(|other| other.id != wire) {
            let Some(path) = layout.wire_polyline(other) else {
                continue;
            };
            for pair in path.windows(2) {
                let [a, b] = pair else { continue };
                if a.y == b.y {
                    for x in a.x.min(b.x)..=a.x.max(b.x) {
                        along_x.insert(GridPoint::new(x, a.y));
                    }
                } else if a.x == b.x {
                    for y in a.y.min(b.y)..=a.y.max(b.y) {
                        along_y.insert(GridPoint::new(a.x, y));
                    }
                }
            }
        }
        Self {
            blocked,
            along_x,
            along_y,
        }
    }

    /// Whether a step in `direction` may arrive at `point`.
    fn allows(&self, point: GridPoint, direction: usize) -> bool {
        if self.blocked.contains(&point) {
            return false;
        }
        let horizontal = direction < 2;
        if horizontal {
            !self.along_x.contains(&point)
        } else {
            !self.along_y.contains(&point)
        }
    }
}

/// The waypoints of a route from `from` to `to`: the bends, without the ends.
fn search(from: GridPoint, to: GridPoint, obstacles: &Obstacles) -> Option<Vec<GridPoint>> {
    let low = GridPoint::new(
        from.x.min(to.x).checked_sub(MARGIN)?,
        from.y.min(to.y).checked_sub(MARGIN)?,
    );
    let high = GridPoint::new(
        from.x.max(to.x).checked_add(MARGIN)?,
        from.y.max(to.y).checked_add(MARGIN)?,
    );
    let inside = |p: GridPoint| low.x <= p.x && p.x <= high.x && low.y <= p.y && p.y <= high.y;
    // State: a point and the direction it was reached in (4: the start).
    type State = (GridPoint, usize);
    let estimate = |p: GridPoint| p.x.abs_diff(to.x).saturating_add(p.y.abs_diff(to.y));
    let mut best: BTreeMap<State, u32> = BTreeMap::new();
    let mut came: BTreeMap<State, State> = BTreeMap::new();
    let mut open = BinaryHeap::new();
    let start = (from, 4);
    best.insert(start, 0);
    open.push(Reverse((estimate(from), 0_u32, from, 4_usize)));
    let mut steps = 0_usize;
    while let Some(Reverse((_, cost, point, direction))) = open.pop() {
        if point == to {
            return Some(waypoints(&came, (point, direction), from, to));
        }
        if best
            .get(&(point, direction))
            .is_some_and(|known| *known < cost)
        {
            continue;
        }
        steps = steps.saturating_add(1);
        if steps > MAX_STEPS {
            return None;
        }
        for (next_direction, (dx, dy)) in DIRECTIONS.iter().enumerate() {
            let (Some(x), Some(y)) = (point.x.checked_add(*dx), point.y.checked_add(*dy)) else {
                continue;
            };
            let next = GridPoint::new(x, y);
            if !inside(next) || (next != to && !obstacles.allows(next, next_direction)) {
                continue;
            }
            // No turning back on the path just taken.
            if direction < 4
                && next_direction ^ 1 == direction
                && direction / 2 == next_direction / 2
            {
                continue;
            }
            let bend = if direction < 4 && direction != next_direction {
                BEND_COST
            } else {
                0
            };
            let next_cost = cost.saturating_add(1).saturating_add(bend);
            let state = (next, next_direction);
            if best.get(&state).is_some_and(|known| *known <= next_cost) {
                continue;
            }
            best.insert(state, next_cost);
            came.insert(state, (point, direction));
            open.push(Reverse((
                next_cost.saturating_add(estimate(next)),
                next_cost,
                next,
                next_direction,
            )));
        }
    }
    None
}

/// The bends of the path that ends in `last`.
fn waypoints(
    came: &BTreeMap<(GridPoint, usize), (GridPoint, usize)>,
    last: (GridPoint, usize),
    from: GridPoint,
    to: GridPoint,
) -> Vec<GridPoint> {
    let mut points = vec![last.0];
    let mut state = last;
    while let Some(previous) = came.get(&state) {
        points.push(previous.0);
        state = *previous;
    }
    points.reverse();
    // Keep the points where the direction changes.
    let mut bends = Vec::new();
    for triple in points.windows(3) {
        let [a, b, c] = triple else { continue };
        let straight = (a.x == b.x && b.x == c.x) || (a.y == b.y && b.y == c.y);
        if !straight {
            bends.push(*b);
        }
    }
    bends.retain(|point| *point != from && *point != to);
    bends
}

/// Give each of `wires` an orthogonal route around symbols, junctions and
/// other wires, in one transaction. A wire that already has that route is
/// left alone.
///
/// # Errors
///
/// Returns [`CommandError::Missing`] for a wire that does not exist,
/// [`CommandError::Locked`] for one on a locked layer,
/// [`CommandError::InvalidParameter`] for more than [`MAX_WIRES`] wires, and
/// [`CommandError::NoRoute`] when no route is found for one of them.
pub fn route(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    wires: &[WireId],
) -> Result<Transaction, CommandError> {
    if wires.len() > MAX_WIRES {
        return Err(CommandError::InvalidParameter(format!(
            "at most {MAX_WIRES} wires are routed at once"
        )));
    }
    let layout = LayoutIndex::new(doc, catalog);
    let mut sorted = wires.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut tx = Transaction::new("Route wires", doc.next_id);
    for wire in sorted {
        let found = doc
            .wires
            .iter()
            .find(|candidate| candidate.id == wire)
            .ok_or(CommandError::Missing(Subject::Wire(wire)))?;
        layers::ensure_wire_unlocked(doc, wire)?;
        let (Some(from), Some(to)) = (
            layout.endpoint_position(&found.from),
            layout.endpoint_position(&found.to),
        ) else {
            return Err(CommandError::Missing(Subject::Wire(wire)));
        };
        let obstacles = Obstacles::of(doc, &layout, wire);
        let route = search(from, to, &obstacles).ok_or(CommandError::NoRoute(wire))?;
        if route != found.route {
            tx.edits.push(Edit::SetRoute { wire, route });
        }
    }
    Ok(tx)
}

#[cfg(test)]
mod tests {
    use circuit_model::builder::pin;
    use circuit_model::{QuarterTurn, SchematicBuilder};

    use super::*;

    fn p(x: i32, y: i32) -> GridPoint {
        GridPoint::new(x, y)
    }

    /// Two junctions with a lamp between them on the straight line.
    fn blocked_line() -> (Schematic, WireId) {
        let mut b = SchematicBuilder::new("route");
        let left = b.junction(p(0, 0)).unwrap();
        let right = b.junction(p(20, 0)).unwrap();
        let lamp = b.device("H1", "lamp.generic", &[]).unwrap();
        b.symbol_turned(lamp, "lamp", None, p(10, 0), QuarterTurn::Deg90)
            .unwrap();
        // A diagonal wire, straight through the lamp.
        let wire = b.wire(left, right).unwrap();
        (b.finish(), wire)
    }

    fn path(doc: &Schematic, wire: WireId) -> Vec<GridPoint> {
        let catalog = ComponentCatalog::builtin();
        let layout = LayoutIndex::new(doc, &catalog);
        layout
            .wire_polyline(doc.wires.iter().find(|w| w.id == wire).unwrap())
            .unwrap()
    }

    #[test]
    fn a_route_goes_around_a_symbol_orthogonally_and_undoes_in_one_step() {
        let (doc, wire) = blocked_line();
        let catalog = ComponentCatalog::builtin();
        let layout = LayoutIndex::new(&doc, &catalog);
        let lamp = layout.symbol_bounds(doc.symbols[0].id).unwrap();

        let tx = route(&doc, &catalog, &[wire]).unwrap();
        let (after, undo) = tx.apply_to(&doc).unwrap();

        let points = path(&after, wire);
        assert_eq!(
            (points.first(), points.last()),
            (Some(&p(0, 0)), Some(&p(20, 0)))
        );
        for pair in points.windows(2) {
            let [a, b] = pair else { continue };
            assert!(a.x == b.x || a.y == b.y, "orthogonal: {points:?}");
            // No point of a segment is inside the lamp.
            let (xs, ys) = (a.x.min(b.x)..=a.x.max(b.x), a.y.min(b.y)..=a.y.max(b.y));
            for x in xs {
                for y in ys.clone() {
                    assert!(
                        !lamp.contains(p(x, y)),
                        "{points:?} crosses the lamp at {x},{y}"
                    );
                }
            }
        }
        assert_eq!(undo.apply_to(&after).unwrap().0, doc);
    }

    #[test]
    fn a_free_straight_line_is_kept_and_routing_twice_changes_nothing() {
        let mut b = SchematicBuilder::new("route");
        let left = b.junction(p(0, 0)).unwrap();
        let right = b.junction(p(20, 0)).unwrap();
        let wire = b.wire(left, right).unwrap();
        let doc = b.finish();
        let catalog = ComponentCatalog::builtin();

        assert!(route(&doc, &catalog, &[wire]).unwrap().edits.is_empty());

        let (blocked, wire) = blocked_line();
        let once = route(&blocked, &catalog, &[wire])
            .unwrap()
            .apply_to(&blocked)
            .unwrap()
            .0;
        assert!(route(&once, &catalog, &[wire]).unwrap().edits.is_empty());
    }

    #[test]
    fn a_route_does_not_run_along_another_wire_or_over_a_terminal() {
        let mut b = SchematicBuilder::new("route");
        let s1 = b.device("S1", "pushbutton_no", &[]).unwrap();
        let contact = b.symbol(s1, "contact", None, p(0, 0)).unwrap();
        let far = b.junction(p(0, 20)).unwrap();
        // An existing vertical wire from p2 (0, 2) down to (0, 20).
        b.wire(pin(contact, "p2"), far).unwrap();
        let top = b.junction(p(0, -10)).unwrap();
        let bottom = b.junction(p(0, 30)).unwrap();
        let wire = b.wire(top, bottom).unwrap();
        let doc = b.finish();
        let catalog = ComponentCatalog::builtin();

        let after = route(&doc, &catalog, &[wire])
            .unwrap()
            .apply_to(&doc)
            .unwrap()
            .0;

        let points = path(&after, wire);
        assert!(
            points
                .iter()
                .all(|point| point.x != 0 || point.y < -2 || point.y > 20),
            "{points:?}"
        );
    }

    #[test]
    fn a_missing_wire_or_too_many_are_refused() {
        let (doc, _) = blocked_line();
        let catalog = ComponentCatalog::builtin();

        assert_eq!(
            route(&doc, &catalog, &[WireId(999)]),
            Err(CommandError::Missing(Subject::Wire(WireId(999))))
        );
        let many: Vec<WireId> = (1..=65).map(WireId).collect();
        assert!(matches!(
            route(&doc, &catalog, &many),
            Err(CommandError::InvalidParameter(_))
        ));
    }
}
