//! Where things are on the sheet.
//!
//! A document stores the position and rotation of each symbol and the
//! waypoints of each wire. Everything else is derived: a pin is at the symbol
//! position plus the rotated terminal offset of the catalog, and a wire runs
//! from the position of its first endpoint through its waypoints to the
//! position of its second endpoint. Renderer, editor and viewer derive these
//! positions here, so they always agree.
//!
//! Nothing here trusts the document. A reference that does not resolve, an
//! unknown model or a coordinate that would overflow gives `None`; it never
//! panics. Structural problems themselves are reported by validation and by
//! the compiler, not here.

use std::collections::BTreeMap;

use crate::catalog::{ComponentCatalog, SymbolDefinition};
use crate::geometry::{GridPoint, GridRect};
use crate::ids::{DeviceId, JunctionId, SymbolId};
use crate::schematic::{Device, Endpoint, PinRef, Schematic, SymbolInstance, Wire};

/// Lookup tables for the geometry of one document.
///
/// Building the index sorts the entities by ID once, so that each query is
/// logarithmic. When an ID appears twice (a structural error), the first
/// entity in document order is used, which keeps the result deterministic.
#[derive(Clone, Debug)]
pub struct LayoutIndex<'a> {
    catalog: &'a ComponentCatalog,
    devices: BTreeMap<DeviceId, &'a Device>,
    symbols: BTreeMap<SymbolId, &'a SymbolInstance>,
    junctions: BTreeMap<JunctionId, GridPoint>,
}

impl<'a> LayoutIndex<'a> {
    /// Index a document against a catalog.
    #[must_use]
    pub fn new(doc: &'a Schematic, catalog: &'a ComponentCatalog) -> Self {
        let mut devices = BTreeMap::new();
        for device in &doc.devices {
            devices.entry(device.id).or_insert(device);
        }
        let mut symbols = BTreeMap::new();
        for symbol in &doc.symbols {
            symbols.entry(symbol.id).or_insert(symbol);
        }
        let mut junctions = BTreeMap::new();
        for junction in &doc.junctions {
            junctions.entry(junction.id).or_insert(junction.position);
        }
        Self {
            catalog,
            devices,
            symbols,
            junctions,
        }
    }

    /// The symbol instance with this ID.
    #[must_use]
    pub fn symbol(&self, id: SymbolId) -> Option<&'a SymbolInstance> {
        self.symbols.get(&id).copied()
    }

    /// The device with this ID.
    #[must_use]
    pub fn device(&self, id: DeviceId) -> Option<&'a Device> {
        self.devices.get(&id).copied()
    }

    /// The catalog definition of a symbol instance: its device, the model of
    /// that device and the symbol key must all resolve.
    #[must_use]
    pub fn definition(&self, id: SymbolId) -> Option<&'a SymbolDefinition> {
        let instance = self.symbol(id)?;
        let device = self.device(instance.device_id)?;
        self.catalog
            .model(&device.model)?
            .symbol(&instance.symbol_key)
    }

    /// Position of one terminal of one symbol.
    #[must_use]
    pub fn pin_position(&self, pin: &PinRef) -> Option<GridPoint> {
        let instance = self.symbol(pin.symbol_id)?;
        let definition = self.definition(pin.symbol_id)?;
        let terminal = definition
            .terminals()
            .iter()
            .find(|terminal| terminal.key == pin.terminal)?;
        instance
            .rotation
            .rotate(terminal.offset)?
            .checked_add(instance.position)
    }

    /// Position of a wire endpoint.
    #[must_use]
    pub fn endpoint_position(&self, endpoint: &Endpoint) -> Option<GridPoint> {
        match endpoint {
            Endpoint::Terminal(pin) => self.pin_position(pin),
            Endpoint::Junction(id) => self.junctions.get(id).copied(),
        }
    }

    /// The full path of a wire: its first endpoint, its waypoints, then its
    /// second endpoint. `None` when an endpoint does not resolve.
    #[must_use]
    pub fn wire_polyline(&self, wire: &Wire) -> Option<Vec<GridPoint>> {
        let from = self.endpoint_position(&wire.from)?;
        let to = self.endpoint_position(&wire.to)?;
        let mut points = Vec::with_capacity(wire.route.len().saturating_add(2));
        points.push(from);
        points.extend_from_slice(&wire.route);
        points.push(to);
        Some(points)
    }

    /// The area a symbol instance covers on the sheet.
    #[must_use]
    pub fn symbol_bounds(&self, id: SymbolId) -> Option<GridRect> {
        let instance = self.symbol(id)?;
        let definition = self.definition(id)?;
        definition
            .kind()
            .bounds()
            .placed(instance.rotation, instance.position)
    }

    /// The symbol whose area contains the point. When several do, the one
    /// with the highest ID wins, so the answer does not depend on document
    /// order.
    #[must_use]
    pub fn symbol_at(&self, point: GridPoint) -> Option<SymbolId> {
        self.symbols
            .keys()
            .rev()
            .copied()
            .find(|id| self.symbol_bounds(*id).is_some_and(|b| b.contains(point)))
    }
}

/// Which way the single bend of an orthogonal route turns.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Bend {
    /// Leave the start vertically, as wires run in a ladder diagram.
    #[default]
    VerticalFirst,
    /// Leave the start horizontally.
    HorizontalFirst,
}

impl Bend {
    /// The other way.
    #[must_use]
    pub const fn flipped(self) -> Self {
        match self {
            Self::VerticalFirst => Self::HorizontalFirst,
            Self::HorizontalFirst => Self::VerticalFirst,
        }
    }
}

/// Waypoints of an orthogonal path from `from` to `to`: none when the two
/// points share a row or a column, otherwise one bend that leaves `from`
/// vertically, as wires run in a ladder diagram. Deterministic.
#[must_use]
pub fn orthogonal_route(from: GridPoint, to: GridPoint) -> Vec<GridPoint> {
    orthogonal_route_bent(from, to, Bend::VerticalFirst)
}

/// [`orthogonal_route`] with the bend turning the given way.
#[must_use]
pub fn orthogonal_route_bent(from: GridPoint, to: GridPoint, bend: Bend) -> Vec<GridPoint> {
    if from.x == to.x || from.y == to.y {
        Vec::new()
    } else {
        match bend {
            Bend::VerticalFirst => vec![GridPoint::new(from.x, to.y)],
            Bend::HorizontalFirst => vec![GridPoint::new(to.x, from.y)],
        }
    }
}

/// The waypoints of an orthogonal wire after its ends moved from `old` to
/// `new` (first end, last end), keeping the bends the user drew.
///
/// For each end that moved, the waypoint next to it slides along the segment
/// that joined them: a vertical first segment moves the waypoint
/// horizontally, so that the segment stays vertical and the next one keeps
/// its row. A waypoint that lands on the point before it is dropped.
/// Returns `None` when the wire has no waypoint, when the old path was not
/// orthogonal, or when the result would not be orthogonal; the caller then
/// routes the wire again.
#[must_use]
pub fn stretch_route(
    route: &[GridPoint],
    old: (GridPoint, GridPoint),
    new: (GridPoint, GridPoint),
) -> Option<Vec<GridPoint>> {
    if route.is_empty() {
        return None;
    }
    let old_path: Vec<GridPoint> = std::iter::once(old.0)
        .chain(route.iter().copied())
        .chain(std::iter::once(old.1))
        .collect();
    if !is_orthogonal(&old_path) {
        return None;
    }
    let mut points = route.to_vec();
    let slide = |end_old: GridPoint, end_new: GridPoint, next: &mut GridPoint| {
        if end_old.x == next.x && end_old.y != next.y {
            next.x = end_new.x;
        } else if end_old.y == next.y && end_old.x != next.x {
            next.y = end_new.y;
        }
    };
    if old.0 != new.0 {
        if let Some(first) = points.first_mut() {
            slide(old.0, new.0, first);
        }
    }
    if old.1 != new.1 {
        if let Some(last) = points.last_mut() {
            slide(old.1, new.1, last);
        }
    }
    let mut path = vec![new.0];
    for point in points.into_iter().chain(std::iter::once(new.1)) {
        if path.last() != Some(&point) {
            path.push(point);
        }
    }
    if path.len() < 2 || !is_orthogonal(&path) {
        return None;
    }
    let inner = path.len().saturating_sub(2);
    Some(path.into_iter().skip(1).take(inner).collect())
}

/// The area a text note covers, in grid units: one line of the label font
/// (0.48 grid units a character, two units high) from, or around, its anchor.
/// `None` when it would leave the coordinate range.
#[must_use]
pub fn text_bounds(note: &crate::documentation::TextNote) -> Option<GridRect> {
    use crate::documentation::{MAX_NOTE_CHARS, TextAnchor};
    let chars = i32::try_from(note.text.chars().count().min(MAX_NOTE_CHARS)).ok()?;
    let width = chars.checked_mul(48)?.checked_add(99)?.checked_div(100)?;
    let left = match note.anchor {
        TextAnchor::Left => note.position.x,
        TextAnchor::Center => note.position.x.checked_sub(width.checked_div(2)?)?,
    };
    Some(GridRect::from_corners(
        GridPoint::new(left, note.position.y.checked_sub(1)?),
        GridPoint::new(left.checked_add(width)?, note.position.y.checked_add(1)?),
    ))
}

/// Whether `p` lies on the segment from `a` to `b`, ends included. Exact on
/// integers: the products of coordinate differences are taken in `i128`.
#[must_use]
pub fn segment_contains(a: GridPoint, b: GridPoint, p: GridPoint) -> bool {
    let within =
        |one: i32, other: i32, value: i32| one.min(other) <= value && value <= one.max(other);
    if !(within(a.x, b.x, p.x) && within(a.y, b.y, p.y)) {
        return false;
    }
    let delta = |from: i32, to: i32| i128::from(to).checked_sub(i128::from(from));
    match (
        delta(a.x, b.x),
        delta(a.y, b.y),
        delta(a.x, p.x),
        delta(a.y, p.y),
    ) {
        (Some(dx), Some(dy), Some(px), Some(py)) => {
            matches!((dx.checked_mul(py), dy.checked_mul(px)), (Some(l), Some(r)) if l == r)
        }
        _ => false,
    }
}

/// The index of the first segment of `path` that contains `p`: segment `i`
/// runs from `path[i]` to `path[i + 1]`.
#[must_use]
pub fn segment_at(path: &[GridPoint], p: GridPoint) -> Option<usize> {
    path.windows(2)
        .position(|pair| matches!(pair, [a, b] if segment_contains(*a, *b, p)))
}

/// Whether every segment of a path is horizontal or vertical. A path with
/// fewer than two points has no segment and is orthogonal.
#[must_use]
pub fn is_orthogonal(points: &[GridPoint]) -> bool {
    points
        .windows(2)
        .all(|pair| matches!(pair, [a, b] if a.x == b.x || a.y == b.y))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_text_covers_a_line_from_its_anchor_or_around_it() {
        use crate::documentation::{TextAnchor, TextNote};
        use crate::ids::TextId;
        let note = |anchor, text: &str| TextNote {
            id: TextId(1),
            position: GridPoint::new(10, 5),
            text: text.to_owned(),
            anchor,
        };

        // Ten characters are about five grid units wide and two high.
        let left = text_bounds(&note(TextAnchor::Left, "0123456789")).unwrap();
        assert_eq!(
            (left.min, left.max),
            (GridPoint::new(10, 4), GridPoint::new(15, 6))
        );
        let centre = text_bounds(&note(TextAnchor::Center, "0123456789")).unwrap();
        assert_eq!((centre.min.x, centre.max.x), (8, 13));
        assert!(centre.contains(GridPoint::new(10, 5)));
        // The edge of the coordinate range gives nothing instead of overflowing.
        let edge = TextNote {
            position: GridPoint::new(i32::MAX, i32::MAX),
            ..note(TextAnchor::Left, "x")
        };
        assert_eq!(text_bounds(&edge), None);
    }

    use super::*;
    use crate::builder::{SchematicBuilder, pin};
    use crate::geometry::QuarterTurn;
    use crate::schematic::{ModelRef, TerminalKey};

    fn point(x: i32, y: i32) -> GridPoint {
        GridPoint::new(x, y)
    }

    /// A pushbutton at (10, 20) and a junction at (10, 30), wired with one
    /// waypoint.
    fn small() -> Schematic {
        let mut builder = SchematicBuilder::new("layout");
        let device = builder.device("S1", "pushbutton_no", &[]).unwrap();
        let contact = builder
            .symbol(device, "contact", None, point(10, 20))
            .unwrap();
        let junction = builder.junction(point(14, 30)).unwrap();
        builder
            .wire_routed(pin(contact, "p2"), junction, &[point(10, 30)])
            .unwrap();
        builder.finish()
    }

    #[test]
    fn pin_is_the_symbol_position_plus_the_terminal_offset() {
        let doc = small();
        let catalog = ComponentCatalog::builtin();
        let layout = LayoutIndex::new(&doc, &catalog);
        let contact = doc.symbols[0].id;

        assert_eq!(
            layout.pin_position(&PinRef {
                symbol_id: contact,
                terminal: TerminalKey("p1".to_owned()),
            }),
            Some(point(10, 18))
        );
    }

    #[test]
    fn pin_of_a_rotated_symbol_uses_the_rotated_offset() {
        let mut doc = small();
        doc.symbols[0].rotation = QuarterTurn::Deg90;
        let catalog = ComponentCatalog::builtin();
        let layout = LayoutIndex::new(&doc, &catalog);

        assert_eq!(
            layout.pin_position(&PinRef {
                symbol_id: doc.symbols[0].id,
                terminal: TerminalKey("p1".to_owned()),
            }),
            Some(point(12, 20))
        );
    }

    #[test]
    fn wire_polyline_runs_from_endpoint_through_waypoints_to_endpoint() {
        let doc = small();
        let catalog = ComponentCatalog::builtin();
        let layout = LayoutIndex::new(&doc, &catalog);

        assert_eq!(
            layout.wire_polyline(&doc.wires[0]),
            Some(vec![point(10, 22), point(10, 30), point(14, 30)])
        );
    }

    #[test]
    fn wire_to_a_missing_junction_has_no_polyline() {
        let mut doc = small();
        doc.junctions.clear();
        let catalog = ComponentCatalog::builtin();
        let layout = LayoutIndex::new(&doc, &catalog);

        assert_eq!(layout.wire_polyline(&doc.wires[0]), None);
    }

    #[test]
    fn pin_of_an_unknown_model_has_no_position() {
        let mut doc = small();
        doc.devices[0].model = ModelRef {
            id: "does.not.exist".to_owned(),
            version: 1,
        };
        let catalog = ComponentCatalog::builtin();
        let layout = LayoutIndex::new(&doc, &catalog);

        assert_eq!(layout.endpoint_position(&doc.wires[0].from), None);
    }

    #[test]
    fn unknown_terminal_has_no_position() {
        let doc = small();
        let catalog = ComponentCatalog::builtin();
        let layout = LayoutIndex::new(&doc, &catalog);

        assert_eq!(
            layout.pin_position(&PinRef {
                symbol_id: doc.symbols[0].id,
                terminal: TerminalKey("a1".to_owned()),
            }),
            None
        );
    }

    #[test]
    fn pin_beyond_the_coordinate_range_has_no_position() {
        let mut doc = small();
        doc.symbols[0].position = point(0, i32::MAX);
        let catalog = ComponentCatalog::builtin();
        let layout = LayoutIndex::new(&doc, &catalog);

        assert_eq!(layout.endpoint_position(&doc.wires[0].from), None);
    }

    #[test]
    fn symbol_at_finds_the_symbol_covering_the_point() {
        let doc = small();
        let catalog = ComponentCatalog::builtin();
        let layout = LayoutIndex::new(&doc, &catalog);

        assert_eq!(layout.symbol_at(point(11, 21)), Some(doc.symbols[0].id));
    }

    #[test]
    fn symbol_at_an_empty_point_finds_nothing() {
        let doc = small();
        let catalog = ComponentCatalog::builtin();
        let layout = LayoutIndex::new(&doc, &catalog);

        assert_eq!(layout.symbol_at(point(40, 40)), None);
    }

    #[test]
    fn symbol_at_prefers_the_highest_id_among_overlapping_symbols() {
        let mut builder = SchematicBuilder::new("overlap");
        let device = builder.device("K1", "contactor.generic", &[]).unwrap();
        builder.symbol(device, "coil", None, point(0, 0)).unwrap();
        let upper = builder
            .symbol(device, "aux_no", Some(1), point(1, 0))
            .unwrap();
        let doc = builder.finish();
        let catalog = ComponentCatalog::builtin();
        let layout = LayoutIndex::new(&doc, &catalog);

        assert_eq!(layout.symbol_at(point(0, 0)), Some(upper));
    }

    #[test]
    fn duplicate_ids_resolve_to_the_first_entity() {
        let mut doc = small();
        let mut copy = doc.junctions[0].clone();
        copy.position = point(99, 99);
        doc.junctions.push(copy);
        let catalog = ComponentCatalog::builtin();
        let layout = LayoutIndex::new(&doc, &catalog);

        assert_eq!(
            layout.endpoint_position(&doc.wires[0].to),
            Some(point(14, 30))
        );
    }

    #[test]
    fn horizontal_and_vertical_segments_are_orthogonal() {
        assert!(is_orthogonal(&[point(0, 0), point(0, 5), point(3, 5)]));
    }

    #[test]
    fn diagonal_segment_is_not_orthogonal() {
        assert!(!is_orthogonal(&[point(0, 0), point(0, 5), point(3, 6)]));
    }

    #[test]
    fn the_bend_turns_the_way_asked() {
        let (from, to) = (point(0, 0), point(4, 6));
        assert_eq!(
            orthogonal_route_bent(from, to, Bend::VerticalFirst),
            [point(0, 6)]
        );
        assert_eq!(
            orthogonal_route_bent(from, to, Bend::HorizontalFirst),
            [point(4, 0)]
        );
        assert_eq!(orthogonal_route(from, to), [point(0, 6)]);
        assert_eq!(Bend::default().flipped(), Bend::HorizontalFirst);
    }

    #[test]
    fn stretching_keeps_the_bends_and_slides_the_one_next_to_the_moved_end() {
        // (0,0) down to (0,5), across to (8,5), down to (8,10).
        let route = [point(0, 5), point(8, 5)];
        let old = (point(0, 0), point(8, 10));

        // The first end moves right by 2: its vertical segment follows.
        assert_eq!(
            stretch_route(&route, old, (point(2, 0), point(8, 10))),
            Some(vec![point(2, 5), point(8, 5)])
        );
        // The last end moves down by 3: nothing to slide, the run gets longer.
        assert_eq!(
            stretch_route(&route, old, (point(0, 0), point(8, 13))),
            Some(vec![point(0, 5), point(8, 5)])
        );
        // Both ends moved, differently.
        assert_eq!(
            stretch_route(&route, old, (point(-1, 0), point(9, 10))),
            Some(vec![point(-1, 5), point(9, 5)])
        );
    }

    #[test]
    fn stretching_drops_a_bend_that_collapses_and_gives_up_on_bad_input() {
        let route = [point(0, 5), point(8, 5)];
        let old = (point(0, 0), point(8, 10));

        // The last end moves up onto the horizontal run: its bend vanishes.
        assert_eq!(
            stretch_route(&route, old, (point(0, 0), point(8, 5))),
            Some(vec![point(0, 5)])
        );
        assert_eq!(stretch_route(&[], old, old), None);
        // A diagonal old path is not stretched.
        assert_eq!(
            stretch_route(
                &[point(3, 3)],
                (point(0, 0), point(8, 10)),
                (point(1, 0), point(8, 10))
            ),
            None
        );
        // The two ends meeting is a self-loop, not a route.
        assert_eq!(
            stretch_route(
                &[point(0, 5)],
                (point(0, 0), point(4, 5)),
                (point(0, 5), point(0, 5))
            ),
            None
        );
    }

    #[test]
    fn aligned_points_need_no_waypoint() {
        assert_eq!(orthogonal_route(point(2, 1), point(2, 9)), []);
        assert_eq!(orthogonal_route(point(-3, 4), point(5, 4)), []);
    }

    #[test]
    fn route_between_unaligned_points_bends_once_after_going_vertically() {
        let (from, to) = (point(0, 0), point(4, 6));

        let route = orthogonal_route(from, to);

        assert_eq!(route, [point(0, 6)]);
        let mut path = vec![from];
        path.extend(route);
        path.push(to);
        assert!(is_orthogonal(&path));
    }

    #[test]
    fn point_inside_or_at_the_end_of_a_segment_is_on_it() {
        assert!(segment_contains(point(0, 0), point(0, 6), point(0, 3)));
        assert!(segment_contains(point(0, 0), point(0, 6), point(0, 6)));
        assert!(segment_contains(point(0, 0), point(4, 4), point(2, 2)));
    }

    #[test]
    fn point_beside_or_beyond_a_segment_is_not_on_it() {
        assert!(!segment_contains(point(0, 0), point(0, 6), point(1, 3)));
        assert!(!segment_contains(point(0, 0), point(0, 6), point(0, 7)));
        assert!(!segment_contains(point(0, 0), point(4, 4), point(2, 3)));
    }

    #[test]
    fn extreme_coordinates_do_not_overflow() {
        let (a, b) = (point(i32::MIN, i32::MIN), point(i32::MAX, i32::MAX));

        assert!(segment_contains(a, b, point(0, 0)));
        assert!(!segment_contains(a, b, point(0, 1)));
    }

    #[test]
    fn segment_at_finds_the_segment_of_a_point() {
        let path = [point(0, 0), point(0, 4), point(6, 4)];

        assert_eq!(segment_at(&path, point(3, 4)), Some(1));
        assert_eq!(segment_at(&path, point(3, 3)), None);
    }

    #[test]
    fn path_without_segments_is_orthogonal() {
        assert!(is_orthogonal(&[point(1, 1)]));
    }
}
