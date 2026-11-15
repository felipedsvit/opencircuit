//! Tests of `build_scene` against small documents built in code.

use std::collections::BTreeSet;

use circuit_model::builder::pin;
use circuit_model::{
    ComponentCatalog, Conductor, ConductorSet, Diagnostic, DiagnosticCode, GridPoint, Live,
    ModelRef, MotorState, MotorView, ParamValue, QuarterTurn, Schematic, SchematicBuilder,
    SimSnapshot, Subject, SymbolId, TimerView, WindingConnection,
};
use proptest::prelude::*;

use crate::{SceneOptions, Shape, StyleKey, build_scene};

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
fn selected_device_outlines_every_symbol_of_it() {
    let catalog = ComponentCatalog::builtin();
    let starter = starter();
    let device = device_of(&starter.doc, starter.coil);
    let options = SceneOptions {
        selected: BTreeSet::from([Subject::Device(device)]),
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
    assert_eq!(labels, BTreeSet::from(["K1", "P0", "P24", "S1", "S2"]));
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
