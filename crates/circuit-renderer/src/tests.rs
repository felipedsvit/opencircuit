//! Tests of `build_scene` against small documents built in code.

use std::collections::BTreeSet;

use circuit_model::builder::pin;
use circuit_model::{
    ComponentCatalog, Conductor, ConductorSet, Diagnostic, DiagnosticCode, GridPoint, LayoutIndex,
    Live, ModelRef, MotorState, MotorView, ParamValue, QuarterTurn, Schematic, SchematicBuilder,
    SimSnapshot, Subject, SymbolId, TimerView, WindingConnection,
};
use proptest::prelude::*;

use crate::{Scene, SceneOptions, ScenePoint, Shape, StyleKey, build_scene};

/// Symbols and wires of the control circuit of a direct-online starter.
struct Starter {
    doc: Schematic,
    stop: SymbolId,
    start: SymbolId,
    coil: SymbolId,
}

fn starter() -> Starter {
    let mut b = SchematicBuilder::new("starter");
    let supply = b.dc_supply(24_000).unwrap();
    let supply_ref = ParamValue::Int(i64::try_from(supply.0).unwrap());
    let p24 = b
        .device(
            "P24",
            "supply.terminal",
            &[
                ("conductor", ParamValue::Text("dc_pos".to_owned())),
                ("supply", supply_ref.clone()),
            ],
        )
        .unwrap();
    let p0 = b
        .device(
            "P0",
            "supply.terminal",
            &[
                ("conductor", ParamValue::Text("dc_neg".to_owned())),
                ("supply", supply_ref),
            ],
        )
        .unwrap();
    let top = b
        .symbol(p24, "terminal", None, GridPoint::new(0, 0))
        .unwrap();
    let bottom = b
        .symbol(p0, "terminal", None, GridPoint::new(0, 16))
        .unwrap();
    let s2 = b.device("S2", "pushbutton_nc", &[]).unwrap();
    let stop = b.symbol(s2, "contact", None, GridPoint::new(0, 4)).unwrap();
    let s1 = b.device("S1", "pushbutton_no", &[]).unwrap();
    let start = b.symbol(s1, "contact", None, GridPoint::new(0, 8)).unwrap();
    let k1 = b.device("K1", "contactor.generic", &[]).unwrap();
    let coil = b.symbol(k1, "coil", None, GridPoint::new(0, 12)).unwrap();
    b.wire(pin(top, "out"), pin(stop, "p1")).unwrap();
    b.wire(pin(stop, "p2"), pin(start, "p1")).unwrap();
    b.wire(pin(start, "p2"), pin(coil, "a1")).unwrap();
    b.wire(pin(coil, "a2"), pin(bottom, "out")).unwrap();
    Starter {
        doc: b.finish(),
        stop,
        start,
        coil,
    }
}

fn snapshot() -> SimSnapshot {
    SimSnapshot {
        tick: 1,
        t_us: 1_000,
        energized_devices: Vec::new(),
        active_devices: Vec::new(),
        actuated_devices: Vec::new(),
        tripped_devices: Vec::new(),
        closed_symbols: Vec::new(),
        live_wires: Vec::new(),
        live_junctions: Vec::new(),
        motors: Vec::new(),
        timers: Vec::new(),
        faulted: false,
        faults: Vec::new(),
    }
}

/// Styles of the items that belong to one subject.
fn styles_of(scene: &crate::Scene, subject: &Subject) -> Vec<StyleKey> {
    scene
        .items
        .iter()
        .filter(|item| item.subject.as_ref() == Some(subject))
        .map(|item| item.style)
        .collect()
}

fn device_of(doc: &Schematic, symbol: SymbolId) -> circuit_model::DeviceId {
    doc.symbols
        .iter()
        .find(|s| s.id == symbol)
        .map(|s| s.device_id)
        .unwrap()
}

#[test]
fn scene_does_not_depend_on_the_order_of_the_collections() {
    let catalog = ComponentCatalog::builtin();
    let doc = starter().doc;
    let mut reversed = doc.clone();
    reversed.devices.reverse();
    reversed.symbols.reverse();
    reversed.wires.reverse();
    reversed.supplies.reverse();

    assert_eq!(
        build_scene(&reversed, &catalog, None, &SceneOptions::default()),
        build_scene(&doc, &catalog, None, &SceneOptions::default())
    );
}

#[test]
fn without_a_snapshot_a_normally_closed_contact_is_drawn_closed() {
    let catalog = ComponentCatalog::builtin();
    let starter = starter();

    let scene = build_scene(&starter.doc, &catalog, None, &SceneOptions::default());

    assert!(styles_of(&scene, &Subject::Symbol(starter.stop)).contains(&StyleKey::ContactClosed));
}

#[test]
fn without_a_snapshot_a_normally_open_contact_is_drawn_open() {
    let catalog = ComponentCatalog::builtin();
    let starter = starter();

    let scene = build_scene(&starter.doc, &catalog, None, &SceneOptions::default());

    assert!(!styles_of(&scene, &Subject::Symbol(starter.start)).contains(&StyleKey::ContactClosed));
}

#[test]
fn contact_in_the_closed_symbols_of_the_snapshot_is_drawn_closed() {
    let catalog = ComponentCatalog::builtin();
    let starter = starter();
    let mut state = snapshot();
    state.closed_symbols = vec![starter.start];

    let scene = build_scene(
        &starter.doc,
        &catalog,
        Some(&state),
        &SceneOptions::default(),
    );

    assert!(styles_of(&scene, &Subject::Symbol(starter.start)).contains(&StyleKey::ContactClosed));
}

#[test]
fn normally_closed_contact_missing_from_the_snapshot_is_drawn_open() {
    let catalog = ComponentCatalog::builtin();
    let starter = starter();

    let scene = build_scene(
        &starter.doc,
        &catalog,
        Some(&snapshot()),
        &SceneOptions::default(),
    );

    assert!(!styles_of(&scene, &Subject::Symbol(starter.stop)).contains(&StyleKey::ContactClosed));
}

#[test]
fn held_pushbutton_draws_its_actuator_as_actuated() {
    let catalog = ComponentCatalog::builtin();
    let starter = starter();
    let mut state = snapshot();
    state.actuated_devices = vec![device_of(&starter.doc, starter.start)];

    let scene = build_scene(
        &starter.doc,
        &catalog,
        Some(&state),
        &SceneOptions::default(),
    );

    assert!(styles_of(&scene, &Subject::Symbol(starter.start)).contains(&StyleKey::Actuated));
}

#[test]
fn live_wire_carries_its_conductors() {
    let catalog = ComponentCatalog::builtin();
    let starter = starter();
    let wire = starter.doc.wires[0].id;
    let conductors = ConductorSet::of(Conductor::DcPos);
    let mut state = snapshot();
    state.live_wires = vec![Live {
        id: wire,
        conductors,
    }];

    let scene = build_scene(
        &starter.doc,
        &catalog,
        Some(&state),
        &SceneOptions::default(),
    );

    assert_eq!(
        styles_of(&scene, &Subject::Wire(wire)),
        [StyleKey::Wire { conductors }]
    );
}

#[test]
fn dead_wire_has_no_conductor() {
    let catalog = ComponentCatalog::builtin();
    let starter = starter();
    let wire = starter.doc.wires[0].id;

    let scene = build_scene(&starter.doc, &catalog, None, &SceneOptions::default());

    assert_eq!(
        styles_of(&scene, &Subject::Wire(wire)),
        [StyleKey::Wire {
            conductors: ConductorSet::default()
        }]
    );
}

#[test]
fn energized_coil_uses_the_energized_style() {
    let catalog = ComponentCatalog::builtin();
    let starter = starter();
    let mut state = snapshot();
    state.energized_devices = vec![device_of(&starter.doc, starter.coil)];

    let scene = build_scene(
        &starter.doc,
        &catalog,
        Some(&state),
        &SceneOptions::default(),
    );

    assert!(styles_of(&scene, &Subject::Symbol(starter.coil)).contains(&StyleKey::Energized));
}

#[test]
fn picked_up_contactor_marks_its_coil_active() {
    let catalog = ComponentCatalog::builtin();
    let starter = starter();
    let mut state = snapshot();
    state.active_devices = vec![device_of(&starter.doc, starter.coil)];

    let scene = build_scene(
        &starter.doc,
        &catalog,
        Some(&state),
        &SceneOptions::default(),
    );

    assert!(styles_of(&scene, &Subject::Symbol(starter.coil)).contains(&StyleKey::Active));
}

#[test]
fn fault_subject_and_related_elements_are_outlined() {
    let catalog = ComponentCatalog::builtin();
    let starter = starter();
    let wire = starter.doc.wires[1].id;
    let mut state = snapshot();
    state.faulted = true;
    state.faults = vec![
        Diagnostic::new(DiagnosticCode::ShortCircuit, Subject::Wire(wire), "short")
            .with_related(vec![Subject::Symbol(starter.coil)]),
    ];

    let scene = build_scene(
        &starter.doc,
        &catalog,
        Some(&state),
        &SceneOptions::default(),
    );

    assert!(styles_of(&scene, &Subject::Wire(wire)).contains(&StyleKey::Fault));
    assert!(styles_of(&scene, &Subject::Symbol(starter.coil)).contains(&StyleKey::Fault));
}

#[test]
fn a_traced_net_is_outlined_in_its_own_style_under_the_selection() {
    let catalog = ComponentCatalog::builtin();
    let doc = starter().doc;
    let wire = doc.wires[0].id;
    let options = SceneOptions {
        selected: BTreeSet::from([Subject::Wire(wire)]),
        traced: BTreeSet::from([Subject::Wire(wire), Subject::Wire(doc.wires[1].id)]),
    };

    let scene = build_scene(&doc, &catalog, None, &options);

    // The selected wire has both outlines, the traced one only its own, and
    // the selection is drawn last, over the trace.
    assert_eq!(
        styles_of(&scene, &Subject::Wire(wire)),
        [
            StyleKey::Wire {
                conductors: ConductorSet::default()
            },
            StyleKey::Traced,
            StyleKey::Selected
        ]
    );
    assert_eq!(
        styles_of(&scene, &Subject::Wire(doc.wires[1].id))
            .iter()
            .filter(|style| **style == StyleKey::Traced)
            .count(),
        1
    );
}

#[test]
fn nothing_is_traced_by_default() {
    let catalog = ComponentCatalog::builtin();
    let scene = build_scene(&starter().doc, &catalog, None, &SceneOptions::default());

    assert!(
        scene
            .items
            .iter()
            .all(|item| item.style != StyleKey::Traced)
    );
}

#[test]
fn selected_device_outlines_every_symbol_of_it() {
    let catalog = ComponentCatalog::builtin();
    let starter = starter();
    let device = device_of(&starter.doc, starter.coil);
    let options = SceneOptions {
        selected: BTreeSet::from([Subject::Device(device)]),
        ..SceneOptions::default()
    };

    let scene = build_scene(&starter.doc, &catalog, None, &options);

    assert_eq!(
        styles_of(&scene, &Subject::Device(device)),
        [StyleKey::Selected]
    );
}

#[test]
fn symbol_of_an_unknown_model_is_drawn_as_a_placeholder() {
    let catalog = ComponentCatalog::builtin();
    let mut starter = starter();
    let device = device_of(&starter.doc, starter.coil);
    for d in &mut starter.doc.devices {
        if d.id == device {
            d.model = ModelRef {
                id: "does.not.exist".to_owned(),
                version: 1,
            };
        }
    }

    let scene = build_scene(&starter.doc, &catalog, None, &SceneOptions::default());

    assert_eq!(
        styles_of(&scene, &Subject::Symbol(starter.coil))
            .into_iter()
            .filter(|style| *style != StyleKey::Label)
            .collect::<Vec<_>>(),
        [StyleKey::Unknown]
    );
}

#[test]
fn wire_to_a_missing_symbol_is_left_out() {
    let catalog = ComponentCatalog::builtin();
    let mut starter = starter();
    let wire = starter.doc.wires[0].id;
    starter.doc.symbols.retain(|s| s.id != starter.stop);

    let scene = build_scene(&starter.doc, &catalog, None, &SceneOptions::default());

    assert_eq!(styles_of(&scene, &Subject::Wire(wire)), []);
}

#[test]
fn every_symbol_is_labelled_with_its_designator() {
    let catalog = ComponentCatalog::builtin();
    let starter = starter();

    let scene = build_scene(&starter.doc, &catalog, None, &SceneOptions::default());

    let labels: BTreeSet<&str> = scene
        .items
        .iter()
        .filter_map(|item| match &item.shape {
            Shape::Text { text, .. } if item.style == StyleKey::Label => Some(text.as_str()),
            _ => None,
        })
        .collect();
    // The designators, plus the terminal numbers of the next test.
    assert!(labels.is_superset(&BTreeSet::from(["K1", "P0", "P24", "S1", "S2"])));
}

/// The texts the scene draws for wires.
fn wire_texts(scene: &Scene) -> Vec<&str> {
    scene
        .items
        .iter()
        .filter_map(|item| match (&item.shape, &item.subject) {
            (Shape::Text { text, .. }, Some(Subject::Wire(_))) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// Three horizontal wires of ten grid units, one above the other, and a
/// vertical one of ten at their left; each its own net.
fn numbered_starter() -> Schematic {
    let mut b = SchematicBuilder::new("numbered");
    for y in [0, 5, 10] {
        let left = b.junction(GridPoint::new(0, y)).unwrap();
        let right = b.junction(GridPoint::new(10, y)).unwrap();
        b.wire(left, right).unwrap();
    }
    let top = b.junction(GridPoint::new(20, 0)).unwrap();
    let bottom = b.junction(GridPoint::new(20, 10)).unwrap();
    b.wire(top, bottom).unwrap();
    let mut doc = b.finish();
    doc.project.wire_numbering = Some(circuit_model::WireNumbering {
        prefix: "W".to_owned(),
        start: 1,
        step: std::num::NonZeroU32::MIN,
        per_sheet: false,
    });
    doc
}

#[test]
fn a_project_without_a_numbering_rule_draws_no_wire_number() {
    let catalog = ComponentCatalog::builtin();
    let mut doc = numbered_starter();
    doc.project.wire_numbering = None;
    doc.wires[0].number = Some("L1".to_owned());

    let scene = build_scene(&doc, &catalog, None, &SceneOptions::default());

    assert_eq!(wire_texts(&scene), Vec::<&str>::new());
}

#[test]
fn a_numbering_rule_draws_each_wire_number_once_in_reading_order() {
    let catalog = ComponentCatalog::builtin();
    let doc = numbered_starter();

    let scene = build_scene(&doc, &catalog, None, &SceneOptions::default());

    // Nets are numbered by where they start, top row first and then left to
    // right: the upper row, the vertical wire (it starts on that row, further
    // right), the middle row, the lower row. The scene lists wires by ID.
    assert_eq!(wire_texts(&scene), ["W1", "W3", "W4", "W2"]);
}

#[test]
fn an_override_replaces_the_generated_number_on_the_drawing() {
    let catalog = ComponentCatalog::builtin();
    let mut doc = numbered_starter();
    doc.wires[1].number = Some("L1".to_owned());

    let scene = build_scene(&doc, &catalog, None, &SceneOptions::default());

    assert_eq!(wire_texts(&scene), ["W1", "L1", "W4", "W2"]);
}

#[test]
fn a_wire_too_short_to_hold_its_number_is_left_unlabelled() {
    let catalog = ComponentCatalog::builtin();
    let mut b = SchematicBuilder::new("short");
    let a = b.junction(GridPoint::new(0, 0)).unwrap();
    let c = b.junction(GridPoint::new(1, 0)).unwrap();
    b.wire(a, c).unwrap();
    let mut doc = b.finish();
    doc.project.wire_numbering = Some(circuit_model::WireNumbering {
        prefix: "WIRE".to_owned(),
        start: 100,
        step: std::num::NonZeroU32::MIN,
        per_sheet: false,
    });

    let scene = build_scene(&doc, &catalog, None, &SceneOptions::default());

    assert_eq!(wire_texts(&scene), Vec::<&str>::new());
}

#[test]
fn wire_numbers_do_not_depend_on_the_order_of_the_collections() {
    let catalog = ComponentCatalog::builtin();
    let doc = numbered_starter();
    let mut shuffled = doc.clone();
    shuffled.devices.reverse();
    shuffled.symbols.reverse();
    shuffled.wires.reverse();

    assert_eq!(
        build_scene(&doc, &catalog, None, &SceneOptions::default()),
        build_scene(&shuffled, &catalog, None, &SceneOptions::default())
    );
}

#[test]
fn terminals_are_numbered_next_to_their_pins() {
    let catalog = ComponentCatalog::builtin();
    let starter = starter();
    let layout = LayoutIndex::new(&starter.doc, &catalog);

    let scene = build_scene(&starter.doc, &catalog, None, &SceneOptions::default());

    let texts: Vec<(&str, ScenePoint)> = scene
        .items
        .iter()
        .filter_map(|item| match &item.shape {
            Shape::Text { text, at, .. } if item.style == StyleKey::Label => {
                Some((text.as_str(), *at))
            }
            _ => None,
        })
        .collect();
    let numbers: BTreeSet<&str> = texts.iter().map(|(text, _)| *text).collect();
    for expected in ["A1", "A2", "11", "12", "13", "14"] {
        assert!(numbers.contains(expected), "{expected} in {numbers:?}");
    }
    // A1 sits three subunits right of the a1 pin of the coil.
    let coil = starter
        .doc
        .symbols
        .iter()
        .find(|symbol| symbol.symbol_key.0 == "coil")
        .unwrap();
    let a1 = layout
        .pin_position(&circuit_model::PinRef {
            symbol_id: coil.id,
            terminal: circuit_model::TerminalKey("a1".to_owned()),
        })
        .map(ScenePoint::from_grid)
        .unwrap();
    assert!(texts.contains(&("A1", ScenePoint::new(a1.x + 3, a1.y))));
    // Supply terminals carry no number.
    assert!(!numbers.contains("out"));
}

#[test]
fn maintained_switch_that_starts_closed_is_drawn_closed_at_rest() {
    let catalog = ComponentCatalog::builtin();
    let mut b = SchematicBuilder::new("selector");
    let device = b
        .device(
            "S5",
            "selector.latching",
            &[("initial_closed", ParamValue::Bool(true))],
        )
        .unwrap();
    let contact = b
        .symbol(device, "contact", None, GridPoint::new(0, 0))
        .unwrap();
    let doc = b.finish();

    let scene = build_scene(&doc, &catalog, None, &SceneOptions::default());

    assert!(styles_of(&scene, &Subject::Symbol(contact)).contains(&StyleKey::ContactClosed));
}

/// Relay `K1` (version 2) with one changeover contact at the origin.
fn changeover_relay() -> (Schematic, SymbolId) {
    let mut b = SchematicBuilder::new("changeover");
    let device = b
        .device_version(
            "K1",
            "relay.generic",
            2,
            &[
                ("rated_voltage_mv", ParamValue::Int(24_000)),
                ("connection", ParamValue::Text("dc".to_owned())),
            ],
        )
        .unwrap();
    b.symbol(device, "coil", None, GridPoint::new(8, 0))
        .unwrap();
    let changeover = b
        .symbol(device, "changeover", Some(1), GridPoint::new(0, 0))
        .unwrap();
    (b.finish(), changeover)
}

/// Where the conducting blade of a contact at the origin ends.
fn blade_tip(scene: &crate::Scene, symbol: SymbolId) -> Option<(i64, i64)> {
    scene
        .items
        .iter()
        .filter(|item| {
            item.subject == Some(Subject::Symbol(symbol)) && item.style == StyleKey::ContactClosed
        })
        .find_map(|item| match &item.shape {
            Shape::Line { to, .. } => Some((to.x, to.y)),
            _ => None,
        })
}

#[test]
fn changeover_rests_on_its_nc_side_and_moves_to_no_when_closed() {
    let catalog = ComponentCatalog::builtin();
    let (doc, changeover) = changeover_relay();
    let mut picked_up = snapshot();
    picked_up.closed_symbols = vec![changeover];

    let at_rest = build_scene(&doc, &catalog, None, &SceneOptions::default());
    let running = build_scene(&doc, &catalog, Some(&picked_up), &SceneOptions::default());
    let released = build_scene(&doc, &catalog, Some(&snapshot()), &SceneOptions::default());

    assert_eq!(blade_tip(&at_rest, changeover), Some((6, -8)));
    assert_eq!(blade_tip(&released, changeover), Some((6, -8)));
    assert_eq!(blade_tip(&running, changeover), Some((0, -8)));
}

#[test]
fn running_six_terminal_motor_is_marked_with_its_winding_connection() {
    let catalog = ComponentCatalog::builtin();
    let mut b = SchematicBuilder::new("motor");
    let device = b
        .device(
            "M1",
            "motor.3ph.6t",
            &[("rated_voltage_mv", ParamValue::Int(400_000))],
        )
        .unwrap();
    let motor = b
        .symbol(device, "motor", None, GridPoint::new(0, 0))
        .unwrap();
    let doc = b.finish();
    let texts = |scene: &crate::Scene| -> Vec<String> {
        scene
            .items
            .iter()
            .filter(|item| item.subject == Some(Subject::Symbol(motor)))
            .filter_map(|item| match &item.shape {
                Shape::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    };
    let mut running = snapshot();
    running.motors = vec![MotorView {
        device,
        state: MotorState::Forward,
        winding: Some(WindingConnection::Delta),
    }];

    let at_rest = build_scene(&doc, &catalog, None, &SceneOptions::default());
    let delta = build_scene(&doc, &catalog, Some(&running), &SceneOptions::default());

    assert!(!texts(&at_rest).iter().any(|t| t == "Y" || t == "\u{394}"));
    assert!(texts(&delta).contains(&"\u{394}".to_owned()));
}

#[test]
fn limit_switch_is_drawn_with_a_roller_and_emergency_stop_without() {
    let catalog = ComponentCatalog::builtin();
    let mut b = SchematicBuilder::new("operators");
    let limit = b.device("B1", "limit_switch", &[]).unwrap();
    let roller = b.symbol(limit, "no", None, GridPoint::new(0, 0)).unwrap();
    let estop = b.device("S0", "estop.latching", &[]).unwrap();
    let mushroom = b
        .symbol(estop, "contact", None, GridPoint::new(8, 0))
        .unwrap();
    let doc = b.finish();
    let scene = build_scene(&doc, &catalog, None, &SceneOptions::default());
    let circles = |symbol: SymbolId| {
        scene
            .items
            .iter()
            .filter(|item| item.subject == Some(Subject::Symbol(symbol)))
            .filter(|item| matches!(item.shape, Shape::Circle { .. }))
            .count()
    };

    assert_eq!(circles(roller), 1);
    assert_eq!(circles(mushroom), 0);
    assert!(styles_of(&scene, &Subject::Symbol(mushroom)).contains(&StyleKey::ContactClosed));
}

/// A line of a scene: start, end and style.
type DrawnLine = ((i64, i64), (i64, i64), StyleKey);

/// The built-in catalog with library `acme`: relay `acme.r2` whose NO
/// contact has its own artwork, a blade open at rest and closed operated.
fn acme_catalog() -> ComponentCatalog {
    use circuit_model::library::{ArtPoint, ArtPrimitive, Artwork, Library};
    use circuit_model::{
        ComponentModel, DeviceClass, ParamDefinition, ParamType, Provenance, SymbolDefinition,
        SymbolKind,
    };
    let p = |x, y| ArtPoint { x, y };
    let leads = || {
        vec![
            ArtPrimitive::Line {
                from: p(0, -20),
                to: p(0, -8),
            },
            ArtPrimitive::Line {
                from: p(0, 8),
                to: p(0, 20),
            },
        ]
    };
    let with_blade = |tip: ArtPoint| {
        let mut list = leads();
        list.push(ArtPrimitive::Line {
            from: p(0, 8),
            to: tip,
        });
        list
    };
    let params = vec![
        ParamDefinition {
            key: "rated_voltage_mv".to_owned(),
            ty: ParamType::Int {
                min: 1,
                max: i64::from(u32::MAX),
            },
            default: None,
        },
        ParamDefinition {
            key: "connection".to_owned(),
            ty: ParamType::Enum(vec!["dc".to_owned()]),
            default: None,
        },
    ];
    let relay = ComponentModel::new(
        "acme.r2",
        1,
        DeviceClass::Relay,
        params,
        vec![
            SymbolDefinition::new("coil", SymbolKind::Coil, 1, 1, false),
            SymbolDefinition::new("no", SymbolKind::ContactNo, 0, 2, true).with_artwork(Artwork {
                rest: with_blade(p(-7, -7)),
                operated: Some(with_blade(p(0, -8))),
            }),
        ],
        Provenance {
            origin: "test".to_owned(),
            license: "MIT".to_owned(),
            standard_reference: None,
        },
    );
    ComponentCatalog::builtin()
        .with_library(Library {
            id: "acme".to_owned(),
            version: 1,
            name: "ACME".to_owned(),
            provenance: Provenance {
                origin: "test".to_owned(),
                license: "MIT".to_owned(),
                standard_reference: None,
            },
            models: vec![relay],
        })
        .unwrap()
}

#[test]
fn library_artwork_replaces_the_kind_artwork_and_follows_the_state() {
    let catalog = acme_catalog();
    let mut b = SchematicBuilder::new("library");
    let relay = b
        .device(
            "K1",
            "acme.r2",
            &[
                ("rated_voltage_mv", ParamValue::Int(24_000)),
                ("connection", ParamValue::Text("dc".to_owned())),
            ],
        )
        .unwrap();
    b.symbol(relay, "coil", None, GridPoint::new(8, 0)).unwrap();
    let contact = b
        .symbol(relay, "no", Some(1), GridPoint::new(0, 0))
        .unwrap();
    let doc = b.finish();
    let lines = |scene: &crate::Scene| -> Vec<DrawnLine> {
        scene
            .items
            .iter()
            .filter(|item| item.subject == Some(Subject::Symbol(contact)))
            .filter_map(|item| match item.shape {
                Shape::Line { from, to } => Some(((from.x, from.y), (to.x, to.y), item.style)),
                _ => None,
            })
            .collect()
    };
    let mut picked_up = snapshot();
    picked_up.active_devices = vec![relay];

    let at_rest = build_scene(&doc, &catalog, None, &SceneOptions::default());
    let operated = build_scene(&doc, &catalog, Some(&picked_up), &SceneOptions::default());

    assert_eq!(lines(&at_rest).len(), 3);
    assert!(lines(&at_rest).contains(&((0, 8), (-7, -7), StyleKey::Symbol)));
    assert!(lines(&operated).contains(&((0, 8), (0, -8), StyleKey::ContactClosed)));
}

#[test]
fn timer_coil_shows_elapsed_and_preset_ticks() {
    let catalog = ComponentCatalog::builtin();
    let mut b = SchematicBuilder::new("timer");
    let device = b.device("KT1", "timer_on.generic", &[]).unwrap();
    b.symbol(device, "coil", None, GridPoint::new(0, 0))
        .unwrap();
    let doc = b.finish();
    let mut state = snapshot();
    state.timers = vec![TimerView {
        device,
        elapsed_ticks: 120,
        preset_ticks: 500,
        out: false,
    }];

    let scene = build_scene(&doc, &catalog, Some(&state), &SceneOptions::default());

    assert!(
        scene
            .items
            .iter()
            .any(|item| matches!(&item.shape, Shape::Text { text, .. } if text == "120/500"))
    );
}

#[test]
fn rotated_symbol_keeps_its_leads_on_its_pins() {
    let catalog = ComponentCatalog::builtin();
    let mut starter = starter();
    for symbol in &mut starter.doc.symbols {
        if symbol.id == starter.start {
            symbol.rotation = QuarterTurn::Deg90;
        }
    }
    let layout = circuit_model::LayoutIndex::new(&starter.doc, &catalog);
    let pin_p1 = layout
        .pin_position(&circuit_model::PinRef {
            symbol_id: starter.start,
            terminal: circuit_model::TerminalKey("p1".to_owned()),
        })
        .unwrap();

    let scene = build_scene(&starter.doc, &catalog, None, &SceneOptions::default());

    let expected = crate::ScenePoint::from_grid(pin_p1);
    assert!(scene.items.iter().any(|item| {
        item.subject == Some(Subject::Symbol(starter.start))
            && matches!(item.shape, Shape::Line { from, to } if from == expected || to == expected)
    }));
}

#[test]
fn symbol_at_the_edge_of_the_coordinate_range_does_not_panic() {
    let catalog = ComponentCatalog::builtin();
    let mut starter = starter();
    for (symbol, corner) in starter.doc.symbols.iter_mut().zip([
        GridPoint::new(i32::MAX, i32::MAX),
        GridPoint::new(i32::MIN, i32::MIN),
        GridPoint::new(i32::MAX, i32::MIN),
    ]) {
        symbol.position = corner;
        symbol.rotation = QuarterTurn::Deg270;
    }

    let scene = build_scene(&starter.doc, &catalog, None, &SceneOptions::default());

    assert!(scene.bounds.is_some());
}

/// Model references to draw from: every builtin model plus one that does
/// not exist.
/// Every model and version of the catalog, and one that does not exist.
fn model_ids() -> Vec<(String, u32)> {
    let mut ids: Vec<(String, u32)> = ComponentCatalog::builtin()
        .models()
        .iter()
        .map(|model| (model.id().to_owned(), model.version()))
        .collect();
    ids.push(("missing.model".to_owned(), 1));
    ids
}

fn coordinate() -> impl Strategy<Value = i32> {
    prop_oneof![
        8 => -50_i32..50,
        1 => Just(i32::MIN),
        1 => Just(i32::MAX),
    ]
}

prop_compose! {
    /// A document with random symbols of random models, random rotations and
    /// positions (including the extremes), and random wires between symbol
    /// terminals and junctions that may not exist.
    fn arbitrary_document()(
        parts in prop::collection::vec(
            (any::<prop::sample::Index>(), any::<prop::sample::Index>(), 0_usize..4, coordinate(), coordinate()),
            0..12,
        ),
        wires in prop::collection::vec(
            (any::<prop::sample::Index>(), any::<prop::sample::Index>(), any::<bool>(), 0_u64..64),
            0..16,
        ),
    ) -> Schematic {
        let catalog = ComponentCatalog::builtin();
        let ids = model_ids();
        let mut b = SchematicBuilder::new("random");
        let mut symbols = Vec::new();
        for (index, (model, key, rotation, x, y)) in parts.iter().enumerate() {
            let (model_id, version) = model.get(ids.as_slice());
            let device = b
                .device_version(&format!("D{index}"), model_id, *version, &[])
                .unwrap();
            let keys: Vec<String> = catalog
                .models()
                .iter()
                .find(|m| m.id() == model_id && m.version() == *version)
                .map(|m| m.symbols().iter().map(|s| s.key().0.clone()).collect())
                .unwrap_or_else(|| vec!["nothing".to_owned()]);
            let symbol = b.symbol(device, key.get(keys.as_slice()).as_str(), None, GridPoint::new(*x, *y)).unwrap();
            symbols.push((symbol, QuarterTurn::ALL[*rotation]));
        }
        let junction = b.junction(GridPoint::new(1, 1)).unwrap();
        for (from, to, to_junction, missing) in wires {
            if symbols.is_empty() {
                break;
            }
            let terminals: [&str; 8] = ["p1", "p2", "a1", "a2", "x1", "pole1.in", "u1", "out"];
            let start = pin(from.get(symbols.as_slice()).0, to.get::<&str>(terminals.as_slice()));
            let end = if to_junction {
                circuit_model::Endpoint::Junction(if missing == 0 { circuit_model::JunctionId(9_999) } else { junction })
            } else {
                pin(to.get(symbols.as_slice()).0, from.get::<&str>(terminals.as_slice()))
            };
            b.wire_routed(start, end, &[GridPoint::new(0, i32::MAX)]).unwrap();
        }
        let mut doc = b.finish();
        for (instance, (_, rotation)) in doc.symbols.iter_mut().zip(&symbols) {
            instance.rotation = *rotation;
        }
        doc
    }
}

proptest! {
    #[test]
    fn building_any_document_never_panics(doc in arbitrary_document()) {
        let catalog = ComponentCatalog::builtin();
        let mut state = snapshot();
        state.closed_symbols = doc.symbols.iter().map(|s| s.id).collect();
        state.faults = doc
            .wires
            .iter()
            .map(|w| Diagnostic::new(DiagnosticCode::ShortCircuit, Subject::Wire(w.id), "short"))
            .collect();

        let _ = build_scene(&doc, &catalog, None, &SceneOptions::default());
        let _ = build_scene(&doc, &catalog, Some(&state), &SceneOptions::default());
    }

    #[test]
    fn scene_of_any_document_ignores_the_order_of_the_collections(doc in arbitrary_document()) {
        let catalog = ComponentCatalog::builtin();
        let mut reversed = doc.clone();
        reversed.devices.reverse();
        reversed.symbols.reverse();
        reversed.wires.reverse();
        reversed.junctions.reverse();

        prop_assert_eq!(
            build_scene(&reversed, &catalog, None, &SceneOptions::default()),
            build_scene(&doc, &catalog, None, &SceneOptions::default())
        );
    }
}

// Sheets: frames over the canvas (ADR-0011).

fn a4(id: u64, number: u32, title: &str, origin: GridPoint) -> circuit_model::Sheet {
    circuit_model::Sheet {
        id: circuit_model::SheetId(id),
        number,
        title: title.to_owned(),
        origin,
        size: circuit_model::SheetSize::A4,
        orientation: circuit_model::Orientation::Landscape,
        zones: circuit_model::Zones {
            columns: 8,
            rows: 6,
        },
    }
}

/// The starter with two sheets side by side, the second one far to the
/// right of everything the starter draws.
fn two_sheets() -> Schematic {
    let mut doc = starter().doc;
    doc.project.author = Some("Ana".to_owned());
    doc.project.revision = Some("B".to_owned());
    let first = doc.next_id;
    doc.sheets = vec![
        a4(first, 1, "Comando", GridPoint::new(-20, -10)),
        a4(first + 1, 2, "For\u{e7}a", GridPoint::new(200, -10)),
    ];
    doc.next_id = first + 2;
    doc
}

fn texts(scene: &Scene, style: StyleKey) -> Vec<&str> {
    scene
        .items
        .iter()
        .filter(|item| item.style == style)
        .filter_map(|item| match &item.shape {
            Shape::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_document_without_sheets_draws_no_frame() {
    let catalog = ComponentCatalog::builtin();

    let scene = build_scene(&starter().doc, &catalog, None, &SceneOptions::default());

    assert!(scene.items.iter().all(|item| item.style != StyleKey::Frame));
    assert_eq!(scene.page, None);
    assert!(
        crate::sheet_scenes(&starter().doc, &catalog, None, &SceneOptions::default()).is_empty()
    );
}

#[test]
fn a_sheet_draws_its_border_zone_marks_and_title_block() {
    let catalog = ComponentCatalog::builtin();
    let doc = two_sheets();

    let scene = build_scene(&doc, &catalog, None, &SceneOptions::default());

    let marks = texts(&scene, StyleKey::Frame);
    for expected in [
        "1",
        "8",
        "A",
        "F",
        "Comando",
        "For\u{e7}a",
        "Ana",
        "Sheet 1/2",
        "Sheet 2/2",
    ] {
        assert!(
            marks.iter().any(|text| text.contains(expected)),
            "{expected} in {marks:?}"
        );
    }
    assert!(marks.contains(&"Rev B"));
    assert!(marks.contains(&"Author Ana"));
    // Every frame item belongs to one of the two sheets.
    assert!(
        scene
            .items
            .iter()
            .filter(|item| item.style == StyleKey::Frame)
            .all(|item| matches!(item.subject, Some(Subject::Sheet(_))))
    );
    // The frame of the first sheet is the paper: 84 by 59 grid units.
    let outer = scene
        .items
        .iter()
        .find_map(|item| match &item.shape {
            Shape::Rect(rect) if item.style == StyleKey::Frame => Some(*rect),
            _ => None,
        })
        .unwrap();
    assert_eq!((outer.min.x, outer.min.y), (-200, -100));
    assert_eq!((outer.max.x, outer.max.y), (640, 490));
}

#[test]
fn the_scene_of_a_sheet_is_its_paper_with_what_lies_inside() {
    let catalog = ComponentCatalog::builtin();
    let doc = two_sheets();
    let ids: Vec<circuit_model::SheetId> = doc.sheets.iter().map(|s| s.id).collect();

    let first =
        crate::build_sheet_scene(&doc, &catalog, None, &SceneOptions::default(), ids[0]).unwrap();
    let second =
        crate::build_sheet_scene(&doc, &catalog, None, &SceneOptions::default(), ids[1]).unwrap();

    assert_eq!(first.page, first.bounds);
    assert_eq!(
        first.page.map(|p| (p.max.x - p.min.x, p.max.y - p.min.y)),
        Some((840, 590))
    );
    // The starter lies on the first sheet only.
    assert!(texts(&first, StyleKey::Label).contains(&"K1"));
    assert!(texts(&second, StyleKey::Label).is_empty());
    // Each page keeps its own frame and no other.
    for (scene, own) in [(&first, ids[0]), (&second, ids[1])] {
        assert!(
            scene
                .items
                .iter()
                .filter(|item| item.style == StyleKey::Frame)
                .all(|item| item.subject == Some(Subject::Sheet(own)))
        );
    }
    assert_eq!(
        texts(&second, StyleKey::Frame)
            .iter()
            .filter(|t| t.starts_with("Sheet"))
            .count(),
        1
    );
    // A sheet the document does not have gives nothing.
    assert!(
        crate::build_sheet_scene(
            &doc,
            &catalog,
            None,
            &SceneOptions::default(),
            circuit_model::SheetId(999_999)
        )
        .is_none()
    );
}

#[test]
fn sheet_scenes_come_in_sheet_number_order_whatever_the_order_in_the_document() {
    let catalog = ComponentCatalog::builtin();
    let mut doc = two_sheets();
    doc.sheets.reverse();

    let pages = crate::sheet_scenes(&doc, &catalog, None, &SceneOptions::default());

    assert_eq!(pages.len(), 2);
    assert!(texts(&pages[0], StyleKey::Frame).contains(&"Comando"));
    assert!(texts(&pages[1], StyleKey::Frame).contains(&"For\u{e7}a"));
}

#[test]
fn title_block_texts_that_do_not_fit_are_cut_and_a_small_sheet_has_no_title_block() {
    let catalog = ComponentCatalog::builtin();
    let mut doc = two_sheets();
    doc.sheets[0].title = "x".repeat(200);
    doc.sheets[1].size = circuit_model::SheetSize::Custom {
        width: 20,
        height: 20,
    };

    let scene = build_scene(&doc, &catalog, None, &SceneOptions::default());

    let marks = texts(&scene, StyleKey::Frame);
    assert!(
        marks
            .iter()
            .any(|t| t.chars().count() == 58 && t.ends_with('\u{2026}'))
    );
    // The small sheet has its border and marks but not its "Sheet 2/2".
    assert!(!marks.contains(&"Sheet 2/2"));
    assert!(marks.contains(&"Sheet 1/2"));
}

proptest! {
    /// Sheets of any size and place never panic and keep the scene's bounds
    /// around everything it holds.
    #[test]
    fn any_sheet_gives_a_scene_whose_bounds_hold_every_item(
        x in -2_000_i32..2_000,
        y in -2_000_i32..2_000,
        width in 0_u32..1_200,
        height in 0_u32..1_200,
        columns in 0_u8..30,
        rows in 0_u8..30,
    ) {
        let catalog = ComponentCatalog::builtin();
        let mut doc = starter().doc;
        let id = doc.next_id;
        doc.next_id += 1;
        doc.sheets.push(circuit_model::Sheet {
            size: circuit_model::SheetSize::Custom { width, height },
            zones: circuit_model::Zones { columns, rows },
            ..a4(id, 1, "t", GridPoint::new(x, y))
        });

        let scene = build_scene(&doc, &catalog, None, &SceneOptions::default());
        let page = crate::build_sheet_scene(
            &doc, &catalog, None, &SceneOptions::default(), circuit_model::SheetId(id),
        );

        if let Some(bounds) = scene.bounds {
            for item in &scene.items {
                if let Some(area) = item.shape.bounds() {
                    prop_assert!(bounds.union(&area) == bounds);
                }
            }
        }
        if let Some(page) = page {
            prop_assert_eq!(page.page, page.bounds);
        }
    }
}

#[test]
fn a_coil_lists_where_its_contacts_are_and_each_contact_says_where_its_coil_is() {
    let catalog = ComponentCatalog::builtin();
    let mut doc = two_sheets();
    // A seal-in contact of K1 beside its coil.
    let coil_symbol = doc
        .symbols
        .iter()
        .find(|symbol| symbol.symbol_key.0 == "coil")
        .unwrap()
        .clone();
    let id = doc.next_id;
    doc.next_id += 1;
    doc.symbols.push(circuit_model::SymbolInstance {
        id: SymbolId(id),
        symbol_key: circuit_model::SymbolKey("aux_no".to_owned()),
        slot: Some(1),
        position: GridPoint::new(coil_symbol.position.x + 6, coil_symbol.position.y),
        ..coil_symbol
    });

    let scene = build_scene(&doc, &catalog, None, &SceneOptions::default());

    let coil = doc
        .symbols
        .iter()
        .find(|symbol| symbol.symbol_key.0 == "coil")
        .unwrap();
    let seal = doc
        .symbols
        .iter()
        .find(|symbol| symbol.symbol_key.0 == "aux_no")
        .unwrap();
    let labels_of = |id: SymbolId| -> Vec<&str> {
        scene
            .items
            .iter()
            .filter(|item| {
                item.style == StyleKey::Label && item.subject == Some(Subject::Symbol(id))
            })
            .filter_map(|item| match &item.shape {
                Shape::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    };
    // The coil: designator, its terminals, then the seal-in contact with its
    // terminals and place. Both are on sheet 1.
    let on_coil = labels_of(coil.id);
    assert!(on_coil.contains(&"K1"));
    assert!(
        on_coil.iter().any(|text| text.starts_with("13-14 1/")),
        "{on_coil:?}"
    );
    // The contact: its coil is on sheet 1 too.
    let on_contact = labels_of(seal.id);
    assert!(
        on_contact.iter().any(|text| text.starts_with("1/")),
        "{on_contact:?}"
    );
}

#[test]
fn a_document_without_sheets_has_no_cross_reference_lines() {
    let catalog = ComponentCatalog::builtin();

    let scene = build_scene(&starter().doc, &catalog, None, &SceneOptions::default());

    assert!(
        !texts(&scene, StyleKey::Label)
            .iter()
            .any(|text| text.contains('/'))
    );
}

#[test]
fn every_builtin_symbol_has_a_preview_inside_the_area_of_its_kind() {
    let catalog = ComponentCatalog::builtin();
    for model in catalog.models() {
        for definition in model.symbols() {
            let scene = crate::symbol_preview(model, definition.key())
                .unwrap_or_else(|| panic!("no preview for {}", model.id()));
            let area = definition.kind().bounds();
            let limit = crate::SceneRect::from_corners(
                ScenePoint::from_grid(area.min),
                ScenePoint::from_grid(area.max),
            );

            assert!(!scene.items.is_empty(), "{}", model.id());
            let bounds = scene
                .bounds
                .unwrap_or_else(|| panic!("{} has no bounds", model.id()));
            // Texts may stick out of the area; the lines and shapes may not.
            let has_text = scene
                .items
                .iter()
                .any(|item| matches!(item.shape, Shape::Text { .. }));
            assert!(
                has_text || limit.union(&bounds) == limit,
                "{} {:?} leaves {limit:?}: {bounds:?}",
                model.id(),
                definition.key()
            );
            assert!(scene.items.iter().all(|item| item.subject.is_none()));
        }
    }
}

#[test]
fn a_preview_has_no_designator_and_an_unknown_symbol_has_none() {
    let catalog = ComponentCatalog::builtin();
    let model = catalog
        .model(&ModelRef {
            id: "contactor.generic".to_owned(),
            version: 1,
        })
        .unwrap();

    let coil = crate::symbol_preview(model, &circuit_model::SymbolKey("coil".to_owned())).unwrap();

    assert!(texts(&coil, StyleKey::Label).is_empty());
    assert!(
        crate::symbol_preview(model, &circuit_model::SymbolKey("nothing".to_owned())).is_none()
    );
}

mod signal_arrows {
    use circuit_model::catalog::{
        Category, ComponentModel, DeviceClass, ParamDefinition, ParamType, Provenance,
        SymbolDefinition, SymbolKind,
    };
    use circuit_model::library::Library;
    use circuit_model::{Orientation, Sheet, SheetId, SheetSize, Zones};

    use super::*;

    fn catalog() -> ComponentCatalog {
        let provenance = Provenance {
            origin: "test".to_owned(),
            license: "MIT".to_owned(),
            standard_reference: None,
        };
        let model = ComponentModel::new(
            "std.signal_arrow",
            1,
            DeviceClass::SignalArrow,
            vec![ParamDefinition {
                key: "name".to_owned(),
                ty: ParamType::Name,
                default: Some(ParamValue::Text("SIGNAL".to_owned())),
            }],
            vec![SymbolDefinition::new(
                "arrow",
                SymbolKind::SignalArrow,
                1,
                1,
                false,
            )],
            provenance.clone(),
        )
        .with_category(Category::Lines);
        ComponentCatalog::builtin()
            .with_library(Library {
                id: "std".to_owned(),
                version: 1,
                name: "test".to_owned(),
                provenance,
                models: vec![model],
            })
            .unwrap()
    }

    fn doc(names: &[(&str, i32)]) -> Schematic {
        let mut b = SchematicBuilder::new("arrows");
        for (index, (name, x)) in names.iter().enumerate() {
            let device = b
                .device(
                    &format!("SA{}", index + 1),
                    "std.signal_arrow",
                    &[("name", ParamValue::Text((*name).to_owned()))],
                )
                .unwrap();
            b.symbol(device, "arrow", None, GridPoint::new(*x, 10))
                .unwrap();
        }
        let mut doc = b.finish();
        for (id, number, x) in [(900, 1, -5), (901, 2, 45)] {
            doc.sheets.push(Sheet {
                id: SheetId(id),
                number,
                title: String::new(),
                origin: GridPoint::new(x, -5),
                size: SheetSize::Custom {
                    width: 40,
                    height: 30,
                },
                orientation: Orientation::Landscape,
                zones: Zones {
                    columns: 4,
                    rows: 2,
                },
            });
        }
        doc
    }

    fn texts(scene: &Scene) -> Vec<&str> {
        scene
            .items
            .iter()
            .filter_map(|item| match &item.shape {
                Shape::Text { text, .. } if item.style == StyleKey::Label => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn an_arrow_shows_its_name_and_where_the_others_of_that_name_are() {
        let scene = build_scene(
            &doc(&[("FEED", 0), ("FEED", 50)]),
            &catalog(),
            None,
            &SceneOptions::default(),
        );

        let shown = texts(&scene);
        assert_eq!(shown.iter().filter(|t| **t == "FEED").count(), 2);
        assert!(shown.iter().any(|t| t.starts_with("-> 2/")), "{shown:?}");
        assert!(shown.iter().any(|t| t.starts_with("-> 1/")), "{shown:?}");
    }

    #[test]
    fn a_lone_arrow_shows_its_name_and_no_places() {
        let scene = build_scene(
            &doc(&[("FEED", 0)]),
            &catalog(),
            None,
            &SceneOptions::default(),
        );

        let shown = texts(&scene);
        assert!(shown.contains(&"FEED"));
        assert!(!shown.iter().any(|t| t.starts_with("->")));
    }

    #[test]
    fn a_drawing_without_arrows_draws_none_of_it() {
        let scene = build_scene(
            &starter().doc,
            &ComponentCatalog::builtin(),
            None,
            &SceneOptions::default(),
        );

        assert!(!texts(&scene).iter().any(|t| t.starts_with("->")));
    }
}

mod text_notes {
    use crate::Anchor;
    use circuit_model::{TextAnchor, TextId, TextNote};

    use super::*;

    fn note(id: u64, x: i32, text: &str, anchor: TextAnchor) -> TextNote {
        TextNote {
            id: TextId(id),
            position: GridPoint::new(x, 6),
            text: text.to_owned(),
            anchor,
            layer: None,
        }
    }

    fn with_notes(notes: Vec<TextNote>) -> Schematic {
        let mut doc = starter().doc;
        doc.texts = notes;
        doc
    }

    fn shapes_of(scene: &Scene, id: TextId) -> Vec<&Shape> {
        scene
            .items
            .iter()
            .filter(|item| item.subject == Some(Subject::Text(id)))
            .map(|item| &item.shape)
            .collect()
    }

    #[test]
    fn a_note_is_drawn_as_a_label_at_its_point_with_its_anchor() {
        let doc = with_notes(vec![
            note(900, 3, "Feed", TextAnchor::Left),
            note(901, 9, "Title", TextAnchor::Center),
        ]);

        let scene = build_scene(
            &doc,
            &ComponentCatalog::builtin(),
            None,
            &SceneOptions::default(),
        );

        assert_eq!(
            shapes_of(&scene, TextId(900)),
            [&Shape::Text {
                at: ScenePoint::from_grid(GridPoint::new(3, 6)),
                text: "Feed".to_owned(),
                anchor: Anchor::Left,
            }]
        );
        assert!(matches!(
            shapes_of(&scene, TextId(901)).as_slice(),
            [Shape::Text {
                anchor: Anchor::Center,
                ..
            }]
        ));
    }

    #[test]
    fn a_selected_note_is_outlined_by_its_area_and_a_drawing_without_notes_is_unchanged() {
        let catalog = ComponentCatalog::builtin();
        let plain = build_scene(&starter().doc, &catalog, None, &SceneOptions::default());
        let doc = with_notes(vec![note(900, 3, "Feed", TextAnchor::Left)]);
        let options = SceneOptions {
            selected: BTreeSet::from([Subject::Text(TextId(900))]),
            ..SceneOptions::default()
        };

        let scene = build_scene(&doc, &catalog, None, &options);

        let styles = styles_of(&scene, &Subject::Text(TextId(900)));
        assert_eq!(styles, [StyleKey::Label, StyleKey::Selected]);
        assert!(
            shapes_of(&scene, TextId(900))
                .iter()
                .any(|s| matches!(s, Shape::Rect(_)))
        );
        assert_eq!(scene.items.len(), plain.items.len() + 2);
    }

    #[test]
    fn a_note_with_a_control_character_or_too_many_characters_is_drawn_clean_and_cut() {
        let long = format!("a\tb{}", "x".repeat(200));
        let doc = with_notes(vec![note(900, 3, &long, TextAnchor::Left)]);

        let scene = build_scene(
            &doc,
            &ComponentCatalog::builtin(),
            None,
            &SceneOptions::default(),
        );

        let [Shape::Text { text, .. }] = shapes_of(&scene, TextId(900)).as_slice() else {
            panic!("one label expected");
        };
        assert!(text.starts_with("a b"));
        assert_eq!(text.chars().count(), circuit_model::MAX_NOTE_CHARS);
    }

    #[test]
    fn the_scene_does_not_depend_on_the_order_of_the_notes() {
        let catalog = ComponentCatalog::builtin();
        let mut notes = vec![
            note(900, 3, "A", TextAnchor::Left),
            note(901, 9, "B", TextAnchor::Center),
        ];
        let forward = build_scene(
            &with_notes(notes.clone()),
            &catalog,
            None,
            &SceneOptions::default(),
        );
        notes.reverse();

        let reversed = build_scene(&with_notes(notes), &catalog, None, &SceneOptions::default());

        assert_eq!(forward, reversed);
    }
}

mod layers {
    use circuit_model::{Layer, LayerId, TextAnchor, TextId, TextNote};

    use super::*;

    const HIDDEN: LayerId = LayerId(9_001);

    fn layered(visible: bool) -> (Schematic, Starter) {
        let starter = starter();
        let mut doc = starter.doc.clone();
        doc.layers.push(Layer {
            id: HIDDEN,
            name: "Hidden".to_owned(),
            visible,
            locked: false,
        });
        doc.texts.push(TextNote {
            id: TextId(9_002),
            position: GridPoint::new(3, 6),
            text: "note".to_owned(),
            anchor: TextAnchor::Left,
            layer: Some(HIDDEN),
        });
        doc.next_id = doc.next_id.max(9_003);
        (doc, starter)
    }

    fn drawn(scene: &Scene, subject: &Subject) -> bool {
        scene
            .items
            .iter()
            .any(|item| item.subject.as_ref() == Some(subject))
    }

    #[test]
    fn nothing_on_a_hidden_layer_is_drawn_and_it_is_drawn_when_shown() {
        let catalog = ComponentCatalog::builtin();
        let (mut doc, starter) = layered(false);
        let wire = doc.wires[0].id;
        doc.wires[0].layer = Some(HIDDEN);
        doc.symbols
            .iter_mut()
            .find(|s| s.id == starter.coil)
            .unwrap()
            .layer = Some(HIDDEN);

        let hidden = build_scene(&doc, &catalog, None, &SceneOptions::default());

        assert!(!drawn(&hidden, &Subject::Wire(wire)));
        assert!(!drawn(&hidden, &Subject::Symbol(starter.coil)));
        assert!(!drawn(&hidden, &Subject::Text(TextId(9_002))));
        // What is on no layer is still there.
        assert!(drawn(&hidden, &Subject::Wire(doc.wires[1].id)));

        doc.layers[0].visible = true;
        let shown = build_scene(&doc, &catalog, None, &SceneOptions::default());
        assert!(drawn(&shown, &Subject::Wire(wire)));
        assert!(drawn(&shown, &Subject::Symbol(starter.coil)));
        assert!(drawn(&shown, &Subject::Text(TextId(9_002))));
    }

    #[test]
    fn a_wire_still_reaches_a_pin_of_a_hidden_symbol() {
        let catalog = ComponentCatalog::builtin();
        let (mut doc, starter) = layered(false);
        doc.symbols
            .iter_mut()
            .find(|s| s.id == starter.coil)
            .unwrap()
            .layer = Some(HIDDEN);
        let wires_of_the_coil = doc
            .wires
            .iter()
            .filter(|w| {
                [&w.from, &w.to].iter().any(|end| {
                    matches!(end, circuit_model::Endpoint::Terminal(pin) if pin.symbol_id == starter.coil)
                })
            })
            .count();
        assert!(wires_of_the_coil > 0);

        let scene = build_scene(&doc, &catalog, None, &SceneOptions::default());

        for wire in &doc.wires {
            assert!(drawn(&scene, &Subject::Wire(wire.id)), "{}", wire.id);
        }
    }

    #[test]
    fn a_hidden_element_is_not_outlined_when_selected_and_a_drawing_without_layers_is_unchanged() {
        let catalog = ComponentCatalog::builtin();
        let (doc, starter) = layered(false);
        let mut hidden = doc.clone();
        hidden
            .symbols
            .iter_mut()
            .find(|s| s.id == starter.coil)
            .unwrap()
            .layer = Some(HIDDEN);
        let options = SceneOptions {
            selected: BTreeSet::from([Subject::Symbol(starter.coil), Subject::Text(TextId(9_002))]),
            ..SceneOptions::default()
        };

        let scene = build_scene(&hidden, &catalog, None, &options);

        assert!(!drawn(&scene, &Subject::Symbol(starter.coil)));
        assert!(!drawn(&scene, &Subject::Text(TextId(9_002))));
        // No layer at all: the same scene as before layers existed.
        let plain = starter.doc;
        let mut with_unused_layer = plain.clone();
        with_unused_layer.layers.push(Layer {
            id: HIDDEN,
            name: "Unused".to_owned(),
            visible: false,
            locked: true,
        });
        assert_eq!(
            build_scene(&plain, &catalog, None, &SceneOptions::default()),
            build_scene(&with_unused_layer, &catalog, None, &SceneOptions::default())
        );
    }

    #[test]
    fn the_dot_of_a_junction_whose_wires_are_all_hidden_is_hidden_too() {
        let catalog = ComponentCatalog::builtin();
        let (mut doc, _) = layered(false);
        let Some(junction) = doc.junctions.first().map(|j| j.id) else {
            return;
        };
        for wire in &mut doc.wires {
            let touches = [&wire.from, &wire.to]
                .iter()
                .any(|end| matches!(end, circuit_model::Endpoint::Junction(id) if *id == junction));
            if touches {
                wire.layer = Some(HIDDEN);
            }
        }

        let scene = build_scene(&doc, &catalog, None, &SceneOptions::default());

        assert!(!drawn(&scene, &Subject::Junction(junction)));
    }
}
