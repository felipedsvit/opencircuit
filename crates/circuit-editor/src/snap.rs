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
