//! Building the scene of a document, optionally in the state of one solved
//! tick.

use std::collections::{BTreeMap, BTreeSet};

use circuit_model::catalog::PARAM_INITIAL_CLOSED;
use circuit_model::{
    ComponentCatalog, ComponentModel, ConductorSet, CrossReferences, DeviceClass, DeviceId,
    JunctionId, LayoutIndex, Location, MotorState, PinRef, Schematic, SheetId, SimSnapshot,
    StaticNets, Subject, SymbolId, SymbolInstance, SymbolKind, TimerView, WindingConnection, Wire,
    WireId, WireNumbers, cross_references, terminal_label,
};

use crate::frame::{draw_sheets, frame_rect};
use crate::scene::{Anchor, SUBUNITS_PER_GRID, Scene, ScenePoint, SceneRect, Shape, StyleKey};
use crate::symbols::{self, Actuator, Look};

/// Radius of a junction dot, in subunits.
const JUNCTION_RADIUS: i64 = 3;
/// Half size of the placeholder drawn for an unresolvable symbol.
const PLACEHOLDER: i64 = 10;
/// Gap between a symbol and its designator.
const LABEL_GAP: i64 = 4;
/// Vertical distance between two lines of label text.
const LABEL_LINE: i64 = 12;
/// Radius of the ring outlining a pin or a junction.
const RING_RADIUS: i64 = 5;
/// Gap between a terminal and its number, to the right of the lead.
const TERMINAL_LABEL_GAP: i64 = 3;
/// Width of one glyph of the label font, in subunits (rounded up).
const GLYPH_ADVANCE: i64 = 5;
/// Distance between a wire and its number.
const WIRE_NUMBER_GAP: i64 = 6;
/// Free length a segment keeps beyond its number.
const WIRE_NUMBER_MARGIN: i64 = 10;

/// What to show besides the document and the simulation state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SceneOptions {
    /// Elements to highlight as selected.
    pub selected: BTreeSet<Subject>,
}

/// The simulation state the scene shows, indexed by ID.
#[derive(Default)]
struct State<'a> {
    running: bool,
    energized: BTreeSet<DeviceId>,
    active: BTreeSet<DeviceId>,
    actuated: BTreeSet<DeviceId>,
    tripped: BTreeSet<DeviceId>,
    closed: BTreeSet<SymbolId>,
    wires: BTreeMap<WireId, ConductorSet>,
    junctions: BTreeMap<JunctionId, ConductorSet>,
    motors: BTreeMap<DeviceId, (MotorState, Option<WindingConnection>)>,
    timers: BTreeMap<DeviceId, &'a TimerView>,
}

impl<'a> State<'a> {
    fn new(snapshot: Option<&'a SimSnapshot>) -> Self {
        let Some(snapshot) = snapshot else {
            return Self::default();
        };
        Self {
            running: true,
            energized: snapshot.energized_devices.iter().copied().collect(),
            active: snapshot.active_devices.iter().copied().collect(),
            actuated: snapshot.actuated_devices.iter().copied().collect(),
            tripped: snapshot.tripped_devices.iter().copied().collect(),
            closed: snapshot.closed_symbols.iter().copied().collect(),
            wires: snapshot
                .live_wires
                .iter()
                .map(|live| (live.id, live.conductors))
                .collect(),
            junctions: snapshot
                .live_junctions
                .iter()
                .map(|live| (live.id, live.conductors))
                .collect(),
            motors: snapshot
                .motors
                .iter()
                .map(|view| (view.device, (view.state, view.winding)))
                .collect(),
            timers: snapshot
                .timers
                .iter()
                .map(|view| (view.device, view))
                .collect(),
        }
    }
}

/// Build the scene of one printed page: sheet `sheet` of the document.
///
/// It holds the frame of that sheet, and every element of the drawing that
/// lies inside the frame (an element that crosses the edge is kept, and the
/// page cuts it), in the same styles and states as [`build_scene`]. The
/// bounds and the page are the frame, so an exporter writes a page of the
/// paper size. `None` when the document has no such sheet or its frame
/// leaves the coordinate range.
#[must_use]
pub fn build_sheet_scene(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    snapshot: Option<&SimSnapshot>,
    options: &SceneOptions,
    sheet: SheetId,
) -> Option<Scene> {
    let wanted = doc.sheets.iter().find(|candidate| candidate.id == sheet)?;
    let frame = frame_rect(wanted)?;
    let whole = build_scene(doc, catalog, snapshot, options);
    let own = Some(Subject::Sheet(sheet));
    let items = whole
        .items
        .into_iter()
        .filter(|item| match item.style {
            StyleKey::Frame => item.subject == own,
            _ => item
                .shape
                .bounds()
                .is_some_and(|bounds| frame.intersects(&bounds)),
        })
        .collect();
    Some(Scene {
        items,
        bounds: Some(frame),
        page: Some(frame),
    })
}

/// The scene of every sheet of the document, in sheet-number order: the
/// pages of a printed set. Empty when the document has no sheet (or none whose
/// frame is inside the coordinate range).
#[must_use]
pub fn sheet_scenes(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    snapshot: Option<&SimSnapshot>,
    options: &SceneOptions,
) -> Vec<Scene> {
    let mut sheets: Vec<&circuit_model::Sheet> = doc.sheets.iter().collect();
    sheets.sort_by_key(|sheet| (sheet.number, sheet.id));
    sheets
        .into_iter()
        .filter_map(|sheet| build_sheet_scene(doc, catalog, snapshot, options, sheet.id))
        .collect()
}

/// Build the scene of a document.
///
/// Without a snapshot every symbol is drawn at rest: normally open contacts
/// open, normally closed contacts closed, a maintained switch in its initial
/// position, and no wire live. With a snapshot the scene shows that solved
/// tick and outlines the elements of its active faults.
///
/// The scene is a pure function of its inputs: the order of the collections
/// in the document does not matter. Elements that do not resolve against
/// the catalog are drawn as placeholders or left out; nothing here panics.
#[must_use]
pub fn build_scene(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    snapshot: Option<&SimSnapshot>,
    options: &SceneOptions,
) -> Scene {
    let layout = LayoutIndex::new(doc, catalog);
    let state = State::new(snapshot);
    let wires = by_id(&doc.wires, |wire| wire.id);
    let symbols = by_id(&doc.symbols, |symbol| symbol.id);
    let mut scene = Scene::default();

    // Frames first, so that the drawing lies over them.
    draw_sheets(&mut scene, doc);

    for wire in wires.values() {
        let Some(path) = layout.wire_polyline(wire) else {
            continue;
        };
        let conductors = state.wires.get(&wire.id).copied().unwrap_or_default();
        scene.push(
            Shape::Polyline(path.into_iter().map(ScenePoint::from_grid).collect()),
            StyleKey::Wire { conductors },
            Some(Subject::Wire(wire.id)),
        );
    }

    for junction in by_id(&doc.junctions, |junction| junction.id).values() {
        let conductors = state
            .junctions
            .get(&junction.id)
            .copied()
            .unwrap_or_default();
        scene.push(
            Shape::Dot {
                center: ScenePoint::from_grid(junction.position),
                radius: JUNCTION_RADIUS,
            },
            StyleKey::Junction { conductors },
            Some(Subject::Junction(junction.id)),
        );
    }

    for symbol in symbols.values() {
        draw_symbol(&mut scene, &layout, catalog, &state, symbol);
    }

    // Where the contacts of each coil are, and where the coil of each contact
    // is: known only on sheets, so the lookup is empty without them.
    let xrefs = cross_references(doc, catalog);
    for symbol in symbols.values() {
        draw_label(&mut scene, &layout, catalog, &state, &xrefs, symbol);
    }

    draw_wire_numbers(&mut scene, doc, catalog, &layout, &wires);

    if let Some(snapshot) = snapshot {
        let subjects: BTreeSet<&Subject> = snapshot
            .faults
            .iter()
            .flat_map(|fault| std::iter::once(fault.subject()).chain(fault.related()))
            .collect();
        for subject in subjects {
            outline(
                &mut scene,
                &layout,
                &wires,
                &symbols,
                subject,
                StyleKey::Fault,
            );
        }
    }

    for subject in &options.selected {
        outline(
            &mut scene,
            &layout,
            &wires,
            &symbols,
            subject,
            StyleKey::Selected,
        );
    }

    scene
}

/// Entities sorted by ID. When an ID repeats, the first entity in document
/// order is kept, as [`LayoutIndex`] does.
fn by_id<T, K: Ord>(items: &[T], key: impl Fn(&T) -> K) -> BTreeMap<K, &T> {
    let mut map = BTreeMap::new();
    for item in items {
        map.entry(key(item)).or_insert(item);
    }
    map
}

/// The position of a symbol in subunits.
fn origin(symbol: &SymbolInstance) -> ScenePoint {
    ScenePoint::from_grid(symbol.position)
}

/// A grid rectangle in subunits.
fn scene_rect(rect: circuit_model::GridRect) -> SceneRect {
    SceneRect::from_corners(
        ScenePoint::from_grid(rect.min),
        ScenePoint::from_grid(rect.max),
    )
}

fn draw_symbol(
    scene: &mut Scene,
    layout: &LayoutIndex<'_>,
    catalog: &ComponentCatalog,
    state: &State<'_>,
    symbol: &SymbolInstance,
) {
    let subject = Some(Subject::Symbol(symbol.id));
    let (Some(definition), Some(device)) = (
        layout.definition(symbol.id),
        layout.device(symbol.device_id),
    ) else {
        placeholder(scene, symbol);
        return;
    };
    let Some(model) = catalog.model(&device.model) else {
        placeholder(scene, symbol);
        return;
    };
    let class = model.class();
    let kind = definition.kind();

    // A maintained switch can start closed; at rest it is drawn that way.
    let initially_closed = class == DeviceClass::LatchingSwitch
        && model.bool_param(&device.params, PARAM_INITIAL_CLOSED) == Some(true);
    let actuated = if state.running {
        state.actuated.contains(&device.id)
    } else {
        initially_closed
    };
    let closed = if state.running {
        state.closed.contains(&symbol.id)
    } else {
        match kind {
            SymbolKind::ContactNc => !initially_closed,
            _ => initially_closed,
        }
    };
    let operator = Actuator::of(model.operator());
    let look = Look {
        closed,
        energized: state.energized.contains(&device.id),
        active: state.active.contains(&device.id),
        actuated,
        tripped: state.tripped.contains(&device.id),
        motor: state.motors.get(&device.id).map(|(motor, _)| *motor),
        winding: state
            .motors
            .get(&device.id)
            .and_then(|(_, winding)| *winding),
    };

    let at = origin(symbol);
    let drawing = match definition.artwork() {
        Some(artwork) => symbols::library_art(artwork, look),
        None => symbols::art(kind, operator, look),
    };
    for (shape, style) in drawing {
        // A shape that cannot be placed is beyond the coordinate range; it
        // is left out rather than drawn somewhere else.
        if let Some(placed) = symbols::place(&shape, symbol.rotation, at) {
            scene.push(placed, style, subject.clone());
        }
    }
}

/// A box marking a symbol that does not resolve against the catalog.
fn placeholder(scene: &mut Scene, symbol: &SymbolInstance) {
    let at = origin(symbol);
    let corner =
        |dx: i64, dy: i64| ScenePoint::new(at.x.saturating_add(dx), at.y.saturating_add(dy));
    scene.push(
        Shape::Rect(SceneRect::from_corners(
            corner(PLACEHOLDER.saturating_neg(), PLACEHOLDER.saturating_neg()),
            corner(PLACEHOLDER, PLACEHOLDER),
        )),
        StyleKey::Unknown,
        Some(Subject::Symbol(symbol.id)),
    );
}

/// The designator of the device to the right of the symbol, and for a timer
/// coil the elapsed and preset ticks under it.
fn draw_label(
    scene: &mut Scene,
    layout: &LayoutIndex<'_>,
    catalog: &ComponentCatalog,
    state: &State<'_>,
    xrefs: &CrossReferences,
    symbol: &SymbolInstance,
) {
    let Some(device) = layout.device(symbol.device_id) else {
        return;
    };
    let Some(bounds) = layout.symbol_bounds(symbol.id).map(scene_rect) else {
        return;
    };
    let x = bounds.max.x.saturating_add(LABEL_GAP);
    let y = bounds
        .min
        .y
        .saturating_add(bounds.max.y)
        .checked_div(2)
        .unwrap_or(bounds.min.y);
    let subject = Some(Subject::Symbol(symbol.id));
    scene.push(
        Shape::Text {
            at: ScenePoint::new(x, y),
            text: device.reference.0.clone(),
            anchor: Anchor::Left,
        },
        StyleKey::Label,
        subject.clone(),
    );
    draw_terminal_labels(scene, layout, catalog, symbol);

    let is_coil = layout
        .definition(symbol.id)
        .is_some_and(|definition| definition.kind() == SymbolKind::Coil);
    let is_timer = catalog
        .model(&device.model)
        .is_some_and(|model| matches!(model.class(), DeviceClass::Timer(_)));
    if let (true, true, Some(timer)) = (is_coil, is_timer, state.timers.get(&device.id)) {
        scene.push(
            Shape::Text {
                at: ScenePoint::new(x, y.saturating_add(LABEL_LINE)),
                text: format!("{}/{}", timer.elapsed_ticks, timer.preset_ticks),
                anchor: Anchor::Left,
            },
            StyleKey::Label,
            subject.clone(),
        );
    }

    // Cross-references, under the designator (and under the count of a
    // timer, which has a line of its own while it runs).
    let location = |location: Option<Location>| {
        location.map_or_else(|| "-".to_owned(), |location| location.to_string())
    };
    let first_line: i64 = if is_coil && is_timer { 2 } else { 1 };
    let lines: Vec<String> = if let Some(contacts) = xrefs.contacts.get(&symbol.id) {
        contacts
            .iter()
            .map(|contact| {
                format!("{} {}", contact.terminals, location(contact.location))
                    .trim()
                    .to_owned()
            })
            .collect()
    } else if let Some(coil) = xrefs.coil.get(&symbol.id) {
        vec![location(*coil)]
    } else {
        Vec::new()
    };
    for (index, text) in (first_line..).zip(lines) {
        scene.push(
            Shape::Text {
                at: ScenePoint::new(x, y.saturating_add(LABEL_LINE.saturating_mul(index))),
                text,
                anchor: Anchor::Left,
            },
            StyleKey::Label,
            subject.clone(),
        );
    }
}

/// The number of every wire, once, on its longest segment (ADR-0014). Nothing
/// is drawn unless the project has a numbering rule, so a document that does
/// not use wire numbers looks as before. A number goes above a horizontal
/// segment and beside a vertical one, and is left out when the segment is too
/// short to hold it.
fn draw_wire_numbers(
    scene: &mut Scene,
    doc: &Schematic,
    catalog: &ComponentCatalog,
    layout: &LayoutIndex<'_>,
    wires: &BTreeMap<circuit_model::WireId, &Wire>,
) {
    if doc.project.wire_numbering.is_none() {
        return;
    }
    let numbers = WireNumbers::build(doc, catalog, &StaticNets::build(doc, catalog));
    for wire in wires.values() {
        let Some(text) = numbers.of_wire(wire.id) else {
            continue;
        };
        let Some(path) = layout.wire_polyline(wire) else {
            continue;
        };
        let longest = path
            .windows(2)
            .filter_map(|pair| match pair {
                [a, b] => Some((*a, *b)),
                _ => None,
            })
            .map(|(a, b)| {
                let length =
                    i64::from(a.x.abs_diff(b.x)).saturating_add(i64::from(a.y.abs_diff(b.y)));
                (length, a, b)
            })
            .fold(None, |best: Option<(i64, _, _)>, candidate| match best {
                Some(best) if best.0 >= candidate.0 => Some(best),
                _ => Some(candidate),
            });
        let Some((length, a, b)) = longest else {
            continue;
        };
        let length = length.saturating_mul(SUBUNITS_PER_GRID);
        let (from, to) = (ScenePoint::from_grid(a), ScenePoint::from_grid(b));
        let middle = ScenePoint::new(
            from.x.saturating_add(to.x).checked_div(2).unwrap_or(from.x),
            from.y.saturating_add(to.y).checked_div(2).unwrap_or(from.y),
        );
        let width = i64::try_from(text.chars().count())
            .unwrap_or(i64::MAX)
            .saturating_mul(GLYPH_ADVANCE);
        let (at, anchor) = if a.y == b.y {
            if length < width.saturating_add(WIRE_NUMBER_MARGIN) {
                continue;
            }
            (
                ScenePoint::new(middle.x, middle.y.saturating_sub(WIRE_NUMBER_GAP)),
                Anchor::Center,
            )
        } else if a.x == b.x {
            if length < LABEL_LINE.saturating_add(WIRE_NUMBER_MARGIN) {
                continue;
            }
            (
                ScenePoint::new(middle.x.saturating_add(WIRE_NUMBER_GAP), middle.y),
                Anchor::Left,
            )
        } else {
            continue;
        };
        scene.push(
            Shape::Text {
                at,
                text: text.to_owned(),
                anchor,
            },
            StyleKey::Label,
            Some(Subject::Wire(wire.id)),
        );
    }
}

/// The number of every marked terminal of a symbol (A1, 13, 95, ...), just
/// right of the terminal. See [`terminal_label`] for the markings.
fn draw_terminal_labels(
    scene: &mut Scene,
    layout: &LayoutIndex<'_>,
    catalog: &ComponentCatalog,
    symbol: &SymbolInstance,
) {
    let (Some(definition), Some(class)) = (
        layout.definition(symbol.id),
        layout
            .device(symbol.device_id)
            .and_then(|device| catalog.model(&device.model))
            .map(ComponentModel::class),
    ) else {
        return;
    };
    for terminal in definition.terminals() {
        let Some(text) = terminal_label(class, definition.kind(), symbol.slot, &terminal.key)
        else {
            continue;
        };
        let pin = PinRef {
            symbol_id: symbol.id,
            terminal: terminal.key.clone(),
        };
        let Some(at) = layout.pin_position(&pin) else {
            continue;
        };
        let at = ScenePoint::from_grid(at);
        scene.push(
            Shape::Text {
                at: ScenePoint::new(at.x.saturating_add(TERMINAL_LABEL_GAP), at.y),
                text,
                anchor: Anchor::Left,
            },
            StyleKey::Label,
            Some(Subject::Symbol(symbol.id)),
        );
    }
}

/// Outline one document element with an overlay style. Elements without a
/// place on the sheet (the document, a supply) are not outlined.
fn outline(
    scene: &mut Scene,
    layout: &LayoutIndex<'_>,
    wires: &BTreeMap<WireId, &Wire>,
    symbols: &BTreeMap<SymbolId, &SymbolInstance>,
    subject: &Subject,
    style: StyleKey,
) {
    let around_symbol = |scene: &mut Scene, id: SymbolId| {
        if let Some(bounds) = layout.symbol_bounds(id) {
            scene.push(
                Shape::Rect(scene_rect(bounds)),
                style,
                Some(subject.clone()),
            );
        }
    };
    let ring = |scene: &mut Scene, center: ScenePoint, radius: i64| {
        scene.push(
            Shape::Circle { center, radius },
            style,
            Some(subject.clone()),
        );
    };
    match subject {
        Subject::Document | Subject::Supply(_) | Subject::Sheet(_) => {}
        Subject::Device(device) => {
            for symbol in symbols.values().filter(|s| s.device_id == *device) {
                around_symbol(scene, symbol.id);
            }
        }
        Subject::Symbol(id) => around_symbol(scene, *id),
        Subject::Pin(pin) => {
            if let Some(position) = layout.pin_position(pin) {
                ring(scene, ScenePoint::from_grid(position), RING_RADIUS);
            }
        }
        Subject::Wire(id) => {
            if let Some(path) = wires.get(id).and_then(|wire| layout.wire_polyline(wire)) {
                scene.push(
                    Shape::Polyline(path.into_iter().map(ScenePoint::from_grid).collect()),
                    style,
                    Some(subject.clone()),
                );
            }
        }
        Subject::Junction(id) => {
            if let Some(position) =
                layout.endpoint_position(&circuit_model::Endpoint::Junction(*id))
            {
                ring(scene, ScenePoint::from_grid(position), RING_RADIUS);
            }
        }
    }
}
