//! Snapping a pointer to the element a wire can end on.
//!
//! A front end knows the pointer more finely than the grid. Rounding it to
//! the nearest grid point means a pin is reached only from inside its own
//! grid cell, which is a few pixels wide when the view is zoomed out.
//! [`snap`] instead looks for the nearest pin, junction or wire within a
//! radius the front end chooses (a fixed distance on screen, converted to
//! scene units), and falls back to the nearest grid point.

use circuit_model::{
    ComponentCatalog, GridPoint, JunctionId, LayoutIndex, PinRef, Schematic, WireId, segment_at,
};

/// A pointer position, `per_grid` steps per grid unit on each axis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pointer {
    /// Horizontal position, in steps.
    pub x: i64,
    /// Vertical position, in steps; down is positive, like the grid.
    pub y: i64,
    /// Steps per grid unit; at least 1.
    pub per_grid: i64,
}

/// What the pointer snapped to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SnapTarget {
    /// A terminal of a symbol.
    Pin(PinRef),
    /// A junction.
    Junction(JunctionId),
    /// A grid point on a wire.
    Wire(WireId),
    /// An end or a bend of a wire ([`SnapOptions::wire_points`]).
    WireEnd(WireId),
    /// The middle of a straight segment of a wire, when it is a grid point
    /// ([`SnapOptions::wire_points`]).
    WireMiddle(WireId),
    /// Where two wires cross without being connected
    /// ([`SnapOptions::wire_points`]); the lower ID first.
    Crossing(WireId, WireId),
    /// Nothing within the radius: the nearest grid point.
    Grid,
}

/// Where a pointer snaps, and to what.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snap {
    /// The grid point to use for the pointer.
    pub point: GridPoint,
    /// The element found there.
    pub target: SnapTarget,
}

/// What [`snap_with`] looks for besides pins, junctions and wires.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SnapOptions {
    /// The ends and bends of wires, the middles of their straight segments
    /// and the points where two wires cross, tried after junctions and
    /// before the other points of wires.
    pub wire_points: bool,
}

/// Snap `pointer` to the nearest pin within `radius` steps, else the nearest
/// junction, else the nearest grid point on a wire, else the nearest grid
/// point.
///
/// Pins come first because a wire most often ends on one. Among candidates
/// of one kind the nearest wins, and on a tie the highest ID, as in
/// [`hit_test`](crate::hit_test), so the result does not depend on the order
/// of the document collections. Returns `None` for a pointer whose grid
/// point is outside the coordinate range or a `per_grid` below 1.
#[must_use]
pub fn snap(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    pointer: Pointer,
    radius: i64,
) -> Option<Snap> {
    snap_with(doc, catalog, pointer, radius, SnapOptions::default())
}

/// [`snap`], with the points of wires `options` asks for tried after
/// junctions: among them the nearest wins, then an end, a crossing and a
/// middle in that order, then the highest ID.
#[must_use]
pub fn snap_with(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    pointer: Pointer,
    radius: i64,
    options: SnapOptions,
) -> Option<Snap> {
    if pointer.per_grid < 1 {
        return None;
    }
    let nearest_grid = GridPoint::new(
        round_to_grid(pointer.x, pointer.per_grid)?,
        round_to_grid(pointer.y, pointer.per_grid)?,
    );
    let limit = i128::from(radius.max(0)).checked_pow(2)?;
    let within =
        |point: GridPoint| distance_squared(pointer, point).filter(|distance| *distance <= limit);
    let layout = LayoutIndex::new(doc, catalog);

    let mut best_pin: Option<(i128, PinRef, GridPoint)> = None;
    for symbol in &doc.symbols {
        let Some(definition) = layout.definition(symbol.id) else {
            continue;
        };
        for terminal in definition.terminals() {
            let pin = PinRef {
                symbol_id: symbol.id,
                terminal: terminal.key.clone(),
            };
            let Some(at) = layout.pin_position(&pin) else {
                continue;
            };
            let Some(distance) = within(at) else {
                continue;
            };
            let better = best_pin.as_ref().is_none_or(|(best, best_pin, _)| {
                (distance, std::cmp::Reverse(&pin)) < (*best, std::cmp::Reverse(best_pin))
            });
            if better {
                best_pin = Some((distance, pin, at));
            }
        }
    }
    if let Some((_, pin, point)) = best_pin {
        return Some(Snap {
            point,
            target: SnapTarget::Pin(pin),
        });
    }

    let junction = doc
        .junctions
        .iter()
        .filter_map(|junction| {
            within(junction.position)
                .map(|distance| (distance, std::cmp::Reverse(junction.id), junction.position))
        })
        .min();
    if let Some((_, std::cmp::Reverse(id), point)) = junction {
        return Some(Snap {
            point,
            target: SnapTarget::Junction(id),
        });
    }

    // No `let` chain: the core builds on Rust 1.85.
    if options.wire_points {
        if let Some(found) = wire_point(doc, &layout, pointer, &within) {
            return Some(found);
        }
    }

    let wire = doc
        .wires
        .iter()
        .filter_map(|wire| {
            let path = layout.wire_polyline(wire)?;
            nearest_on_path(&path, pointer)
                .and_then(|point| within(point).map(|distance| (distance, point)))
                .map(|(distance, point)| (distance, std::cmp::Reverse(wire.id), point))
        })
        .min();
    if let Some((_, std::cmp::Reverse(id), point)) = wire {
        return Some(Snap {
            point,
            target: SnapTarget::Wire(id),
        });
    }

    Some(Snap {
        point: nearest_grid,
        target: SnapTarget::Grid,
    })
}

/// The nearest end, bend, crossing or segment middle of a wire for which
/// `within` gives a distance.
fn wire_point(
    doc: &Schematic,
    layout: &LayoutIndex,
    pointer: Pointer,
    within: &dyn Fn(GridPoint) -> Option<i128>,
) -> Option<Snap> {
    // (distance, kind: 0 end, 1 crossing, 2 middle, reversed IDs, point)
    type Candidate = (i128, u8, std::cmp::Reverse<(WireId, WireId)>, GridPoint);
    let mut best: Option<(Candidate, SnapTarget)> = None;
    let mut offer = |candidate: Candidate, target: SnapTarget| {
        if best.as_ref().is_none_or(|(old, _)| candidate < *old) {
            best = Some((candidate, target));
        }
    };
    // Only wires that come within reach: a box around each path.
    let near: Vec<(WireId, Vec<GridPoint>)> = doc
        .wires
        .iter()
        .filter_map(|wire| Some((wire.id, layout.wire_polyline(wire)?)))
        .filter(|(_, path)| {
            path.windows(2).any(|pair| match pair {
                [a, b] => nearest_on_path(&[*a, *b], pointer).is_some_and(|p| within(p).is_some()),
                _ => false,
            })
        })
        .collect();
    for (id, path) in &near {
        for point in path {
            if let Some(distance) = within(*point) {
                offer(
                    (distance, 0, std::cmp::Reverse((*id, *id)), *point),
                    SnapTarget::WireEnd(*id),
                );
            }
        }
        for pair in path.windows(2) {
            let [a, b] = pair else { continue };
            // i32 + i32 always fits an i64; an odd sum has no grid middle.
            let (Some(sx), Some(sy)) = (
                i64::from(a.x).checked_add(i64::from(b.x)),
                i64::from(a.y).checked_add(i64::from(b.y)),
            ) else {
                continue;
            };
            if sx.rem_euclid(2) != 0 || sy.rem_euclid(2) != 0 || a == b {
                continue;
            }
            let (Ok(x), Ok(y)) = (
                i32::try_from(sx.div_euclid(2)),
                i32::try_from(sy.div_euclid(2)),
            ) else {
                continue;
            };
            let middle = GridPoint::new(x, y);
            if let Some(distance) = within(middle) {
                offer(
                    (distance, 2, std::cmp::Reverse((*id, *id)), middle),
                    SnapTarget::WireMiddle(*id),
                );
            }
        }
    }
    for (index, (first, first_path)) in near.iter().enumerate() {
        for (second, second_path) in near.iter().skip(index.saturating_add(1)) {
            for one in first_path.windows(2) {
                for other in second_path.windows(2) {
                    let (Some(point), [a, b], [c, d]) = (crossing(one, other), one, other) else {
                        continue;
                    };
                    // Meeting at an end is a connection or a corner, not a
                    // crossing.
                    if [a, b, c, d].contains(&&point) {
                        continue;
                    }
                    if let Some(distance) = within(point) {
                        let (low, high) = ((*first).min(*second), (*first).max(*second));
                        offer(
                            (distance, 1, std::cmp::Reverse((low, high)), point),
                            SnapTarget::Crossing(low, high),
                        );
                    }
                }
            }
        }
    }
    best.map(|((_, _, _, point), target)| Snap { point, target })
}

/// Where a horizontal and a vertical segment cross, if they do.
fn crossing(one: &[GridPoint], other: &[GridPoint]) -> Option<GridPoint> {
    let ([a, b], [c, d]) = (one, other) else {
        return None;
    };
    let (horizontal, vertical) = if a.y == b.y && c.x == d.x {
        ((a, b), (c, d))
    } else if c.y == d.y && a.x == b.x {
        ((c, d), (a, b))
    } else {
        return None;
    };
    let (x, y) = (vertical.0.x, horizontal.0.y);
    let inside_x =
        horizontal.0.x.min(horizontal.1.x) <= x && x <= horizontal.0.x.max(horizontal.1.x);
    let inside_y = vertical.0.y.min(vertical.1.y) <= y && y <= vertical.0.y.max(vertical.1.y);
    (inside_x && inside_y).then_some(GridPoint::new(x, y))
}

/// The grid coordinate nearest to `steps`, or `None` outside `i32`.
fn round_to_grid(steps: i64, per_grid: i64) -> Option<i32> {
    let half = per_grid.checked_div(2)?;
    let rounded = steps.checked_add(half)?.checked_div_euclid(per_grid)?;
    i32::try_from(rounded).ok()
}

/// Squared distance from the pointer to a grid point, in steps.
fn distance_squared(pointer: Pointer, point: GridPoint) -> Option<i128> {
    let per_grid = i128::from(pointer.per_grid);
    let dx = i128::from(point.x)
        .checked_mul(per_grid)?
        .checked_sub(i128::from(pointer.x))?;
    let dy = i128::from(point.y)
        .checked_mul(per_grid)?
        .checked_sub(i128::from(pointer.y))?;
    dx.checked_mul(dx)?.checked_add(dy.checked_mul(dy)?)
}

/// The grid point of a horizontal or vertical segment of `path` nearest to
/// the pointer. Diagonal segments are skipped: a wire is meant to be
/// orthogonal and only its grid points can be connected to.
fn nearest_on_path(path: &[GridPoint], pointer: Pointer) -> Option<GridPoint> {
    let x = round_to_grid(pointer.x, pointer.per_grid)?;
    let y = round_to_grid(pointer.y, pointer.per_grid)?;
    path.windows(2)
        .filter_map(|pair| match pair {
            [a, b] if a.x == b.x => Some(GridPoint::new(a.x, y.clamp(a.y.min(b.y), a.y.max(b.y)))),
            [a, b] if a.y == b.y => Some(GridPoint::new(x.clamp(a.x.min(b.x), a.x.max(b.x)), a.y)),
            _ => None,
        })
        .filter(|point| segment_at(path, *point).is_some())
        .filter_map(|point| distance_squared(pointer, point).map(|distance| (distance, point)))
        .min_by_key(|(distance, point)| (*distance, *point))
        .map(|(_, point)| point)
}
