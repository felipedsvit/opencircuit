//! Tests of the operations, and properties of random editing sessions.

use std::collections::BTreeSet;

use circuit_model::builder::pin;
use circuit_model::{
    ComponentCatalog, DeviceId, DiagnosticCode, Endpoint, GridPoint, LayoutIndex, ModelRef,
    ParamValue, QuarterTurn, Schematic, Subject, SupplyId, SymbolId, SymbolKey, is_orthogonal,
};
use proptest::prelude::*;

use crate::ops::{self, Anchor, SupplyPreset};
use crate::{CommandError, Edit, History, Transaction};

fn model(id: &str) -> ModelRef {
    ModelRef {
        id: id.to_owned(),
        version: 1,
    }
}

fn key(key: &str) -> SymbolKey {
    SymbolKey(key.to_owned())
}

fn point(x: i32, y: i32) -> GridPoint {
    GridPoint::new(x, y)
}

/// Build a transaction from the current document and apply it through a
/// history, failing the test if either step is refused.
fn run(
    doc: &mut Schematic,
    history: &mut History,
    op: impl FnOnce(&Schematic) -> Result<Transaction, CommandError>,
) {
    let tx = op(doc).unwrap();
    history.execute(doc, tx).unwrap();
}

/// Errors that an edit must never introduce.
fn integrity(doc: &Schematic) -> Vec<DiagnosticCode> {
    doc.validate()
        .into_iter()
        .map(|d| d.code())
        .filter(|code| {
            matches!(
                code,
                DiagnosticCode::UnsupportedSchemaVersion
                    | DiagnosticCode::IdOutOfRange
                    | DiagnosticCode::DuplicateId
                    | DiagnosticCode::DanglingReference
                    | DiagnosticCode::SelfLoopWire
            )
        })
        .collect()
}

fn symbol_id(tx: &Transaction) -> SymbolId {
    tx.inserted()
        .into_iter()
        .find_map(|s| match s {
            Subject::Symbol(id) => Some(id),
            _ => None,
        })
        .unwrap()
}

fn device_of(doc: &Schematic, symbol: SymbolId) -> DeviceId {
    doc.symbols
        .iter()
        .find(|s| s.id == symbol)
        .unwrap()
        .device_id
}

fn wire_paths_are_orthogonal(doc: &Schematic, catalog: &ComponentCatalog) -> bool {
    let layout = LayoutIndex::new(doc, catalog);
    doc.wires.iter().all(|wire| {
        layout
            .wire_polyline(wire)
            .is_some_and(|p| is_orthogonal(&p))
    })
}

/// Place a symbol and return its ID.
fn place(
    doc: &mut Schematic,
    history: &mut History,
    model_id: &str,
    symbol: &str,
    at: GridPoint,
) -> SymbolId {
    let catalog = ComponentCatalog::builtin();
    let tx = ops::place(
        doc,
        &catalog,
        &model(model_id),
        &key(symbol),
        at,
        QuarterTurn::Deg0,
    )
    .unwrap();
    let id = symbol_id(&tx);
    history.execute(doc, tx).unwrap();
    id
}

#[test]
fn placed_devices_get_the_first_free_designator_of_their_kind() {
    let mut doc = Schematic::new("place");
    let mut history = History::new();

    let first = place(
        &mut doc,
        &mut history,
        "contactor.generic",
        "coil",
        point(0, 0),
    );
    let second = place(
        &mut doc,
        &mut history,
        "contactor.generic",
        "coil",
        point(4, 0),
    );
    let button = place(
        &mut doc,
        &mut history,
        "pushbutton_no",
        "contact",
        point(8, 0),
    );

    let names: Vec<&str> = [first, second, button]
        .iter()
        .map(|s| {
            let device = device_of(&doc, *s);
            doc.devices
                .iter()
                .find(|d| d.id == device)
                .unwrap()
                .reference
                .0
                .as_str()
        })
        .collect();
    assert_eq!(names, ["K1", "K2", "S1"]);
}

#[test]
fn slotted_symbol_is_placed_in_slot_one_and_the_next_one_in_slot_two() {
    let mut doc = Schematic::new("slots");
    let mut history = History::new();
    let catalog = ComponentCatalog::builtin();
    let aux = place(
        &mut doc,
        &mut history,
        "contactor.generic",
        "aux_no",
        point(0, 0),
    );
    let device = device_of(&doc, aux);

    run(&mut doc, &mut history, |doc| {
        ops::add_symbol(
            doc,
            &catalog,
            device,
            &key("aux_no"),
            point(4, 0),
            QuarterTurn::Deg0,
        )
    });

    let slots: Vec<Option<u8>> = doc.symbols.iter().map(|s| s.slot).collect();
    assert_eq!(slots, [Some(1), Some(2)]);
}

#[test]
fn symbol_beyond_its_instance_limit_is_refused() {
    let mut doc = Schematic::new("limit");
    let mut history = History::new();
    let catalog = ComponentCatalog::builtin();
    let coil = place(
        &mut doc,
        &mut history,
        "contactor.generic",
        "coil",
        point(0, 0),
    );
    let device = device_of(&doc, coil);

    assert!(matches!(
        ops::add_symbol(
            &doc,
            &catalog,
            device,
            &key("coil"),
            point(4, 0),
            QuarterTurn::Deg0
        ),
        Err(CommandError::InstanceLimit { .. })
    ));
}

#[test]
fn unknown_model_or_symbol_is_refused() {
    let doc = Schematic::new("unknown");
    let catalog = ComponentCatalog::builtin();

    assert!(matches!(
        ops::place(
            &doc,
            &catalog,
            &model("nope"),
            &key("coil"),
            point(0, 0),
            QuarterTurn::Deg0
        ),
        Err(CommandError::UnknownModel(_))
    ));
    assert!(matches!(
        ops::place(
            &doc,
            &catalog,
            &model("pushbutton_no"),
            &key("coil"),
            point(0, 0),
            QuarterTurn::Deg0
        ),
        Err(CommandError::UnknownSymbol { .. })
    ));
}

/// Two pushbuttons side by side, not aligned, and a coil below them.
fn three_parts() -> (Schematic, History, [SymbolId; 3]) {
    let mut doc = Schematic::new("parts");
    let mut history = History::new();
    let a = place(
        &mut doc,
        &mut history,
        "pushbutton_no",
        "contact",
        point(0, 0),
    );
    let b = place(
        &mut doc,
        &mut history,
        "pushbutton_no",
        "contact",
        point(6, 3),
    );
    let coil = place(
        &mut doc,
        &mut history,
        "contactor.generic",
        "coil",
        point(0, 12),
    );
    (doc, history, [a, b, coil])
}

#[test]
fn connecting_two_pins_without_waypoints_routes_orthogonally() {
    let (mut doc, mut history, [a, b, _]) = three_parts();
    let catalog = ComponentCatalog::builtin();

    run(&mut doc, &mut history, |doc| {
        ops::connect(
            doc,
            &catalog,
            &Anchor::Endpoint(pin(a, "p2")),
            &Anchor::Endpoint(pin(b, "p1")),
            &[],
        )
    });

    assert_eq!(doc.wires.len(), 1);
    assert!(wire_paths_are_orthogonal(&doc, &catalog));
}

#[test]
fn connecting_to_the_middle_of_a_wire_splits_it_with_a_junction() {
    let (mut doc, mut history, [a, b, coil]) = three_parts();
    let catalog = ComponentCatalog::builtin();
    // a.p2 is at (0, 2), coil.a1 at (0, 10): a straight vertical wire.
    run(&mut doc, &mut history, |doc| {
        ops::connect(
            doc,
            &catalog,
            &Anchor::Endpoint(pin(a, "p2")),
            &Anchor::Endpoint(pin(coil, "a1")),
            &[],
        )
    });
    let trunk = doc.wires[0].id;

    run(&mut doc, &mut history, |doc| {
        ops::connect(
            doc,
            &catalog,
            &Anchor::Endpoint(pin(b, "p2")),
            &Anchor::OnWire {
                wire: trunk,
                at: point(0, 5),
            },
            &[],
        )
    });

    assert_eq!(doc.junctions.len(), 1);
    assert_eq!(doc.junctions[0].position, point(0, 5));
    assert_eq!(doc.wires.len(), 3);
    assert!(doc.wires.iter().all(|w| w.id != trunk));
    assert!(wire_paths_are_orthogonal(&doc, &catalog));
    assert_eq!(integrity(&doc), []);
    assert_eq!(doc.validate(), []);
}

#[test]
fn connecting_a_point_to_itself_or_off_the_wire_is_refused() {
    let (mut doc, mut history, [a, _, coil]) = three_parts();
    let catalog = ComponentCatalog::builtin();
    let same = Anchor::Endpoint(pin(a, "p1"));
    assert_eq!(
        ops::connect(&doc, &catalog, &same, &same, &[]),
        Err(CommandError::SelfLoop)
    );
    run(&mut doc, &mut history, |doc| {
        ops::connect(
            doc,
            &catalog,
            &Anchor::Endpoint(pin(a, "p2")),
            &Anchor::Endpoint(pin(coil, "a1")),
            &[],
        )
    });
    let wire = doc.wires[0].id;

    assert_eq!(
        ops::connect(
            &doc,
            &catalog,
            &Anchor::OnWire {
                wire,
                at: point(0, 4)
            },
            &Anchor::OnWire {
                wire,
                at: point(0, 6)
            },
            &[],
        ),
        Err(CommandError::SelfLoop)
    );
    assert_eq!(
        ops::connect(
            &doc,
            &catalog,
            &Anchor::Endpoint(pin(a, "p1")),
            &Anchor::OnWire {
                wire,
                at: point(3, 5)
            },
            &[],
        ),
        Err(CommandError::NotOnWire(wire))
    );
}

#[test]
fn deleting_a_symbol_removes_its_wires_its_orphan_junction_and_its_device() {
    let (mut doc, mut history, [a, b, coil]) = three_parts();
    let catalog = ComponentCatalog::builtin();
    run(&mut doc, &mut history, |doc| {
        ops::connect(
            doc,
            &catalog,
            &Anchor::Endpoint(pin(a, "p2")),
            &Anchor::Endpoint(pin(coil, "a1")),
            &[],
        )
    });
    let trunk = doc.wires[0].id;
    run(&mut doc, &mut history, |doc| {
        ops::connect(
            doc,
            &catalog,
            &Anchor::Endpoint(pin(b, "p2")),
            &Anchor::OnWire {
                wire: trunk,
                at: point(0, 5),
            },
            &[],
        )
    });
    let device = device_of(&doc, coil);

    run(&mut doc, &mut history, |doc| {
        ops::delete(doc, &BTreeSet::from([Subject::Symbol(coil)]))
    });

    assert!(doc.devices.iter().all(|d| d.id != device));
    assert!(doc.symbols.iter().all(|s| s.id != coil));
    // The branch from b to the junction and the wire from a remain; the
    // junction keeps two wires, so it stays.
    assert_eq!((doc.wires.len(), doc.junctions.len()), (2, 1));
    assert_eq!(integrity(&doc), []);
}

#[test]
fn deleting_a_device_removes_every_symbol_of_it() {
    let mut doc = Schematic::new("device");
    let mut history = History::new();
    let catalog = ComponentCatalog::builtin();
    let coil = place(
        &mut doc,
        &mut history,
        "contactor.generic",
        "coil",
        point(0, 0),
    );
    let device = device_of(&doc, coil);
    run(&mut doc, &mut history, |doc| {
        ops::add_symbol(
            doc,
            &catalog,
            device,
            &key("aux_no"),
            point(4, 0),
            QuarterTurn::Deg0,
        )
    });

    run(&mut doc, &mut history, |doc| {
        ops::delete(doc, &BTreeSet::from([Subject::Device(device)]))
    });

    assert_eq!((doc.devices.len(), doc.symbols.len()), (0, 0));
}

#[test]
fn deleting_something_missing_is_refused() {
    let doc = Schematic::new("missing");

    assert!(matches!(
        ops::delete(&doc, &BTreeSet::from([Subject::Symbol(SymbolId(9))])),
        Err(CommandError::Missing(_))
    ));
}

#[test]
fn moving_keeps_wires_orthogonal() {
    let (mut doc, mut history, [a, b, coil]) = three_parts();
    let catalog = ComponentCatalog::builtin();
    for (from, to) in [
        (pin(a, "p2"), pin(coil, "a1")),
        (pin(a, "p1"), pin(b, "p1")),
    ] {
        run(&mut doc, &mut history, |doc| {
            ops::connect(
                doc,
                &catalog,
                &Anchor::Endpoint(from),
                &Anchor::Endpoint(to),
                &[],
            )
        });
    }

    run(&mut doc, &mut history, |doc| {
        ops::move_by(
            doc,
            &catalog,
            &BTreeSet::from([Subject::Symbol(a)]),
            point(3, -2),
        )
    });

    assert_eq!(
        doc.symbols.iter().find(|s| s.id == a).unwrap().position,
        point(3, -2)
    );
    assert!(wire_paths_are_orthogonal(&doc, &catalog));
}

#[test]
fn moving_both_ends_shifts_the_route_with_them() {
    let (mut doc, mut history, [a, b, _]) = three_parts();
    let catalog = ComponentCatalog::builtin();
    run(&mut doc, &mut history, |doc| {
        ops::connect(
            doc,
            &catalog,
            &Anchor::Endpoint(pin(a, "p2")),
            &Anchor::Endpoint(pin(b, "p1")),
            &[],
        )
    });
    let route = doc.wires[0].route.clone();

    run(&mut doc, &mut history, |doc| {
        ops::move_by(
            doc,
            &catalog,
            &BTreeSet::from([Subject::Symbol(a), Subject::Symbol(b)]),
            point(10, 10),
        )
    });

    let shifted: Vec<GridPoint> = route.iter().map(|p| point(p.x + 10, p.y + 10)).collect();
    assert_eq!(doc.wires[0].route, shifted);
}

#[test]
fn move_beyond_the_coordinate_range_is_refused_and_changes_nothing() {
    let (doc, _, [a, _, _]) = three_parts();
    let catalog = ComponentCatalog::builtin();

    assert_eq!(
        ops::move_by(
            &doc,
            &catalog,
            &BTreeSet::from([Subject::Symbol(a)]),
            point(i32::MAX, 0)
        ),
        Err(CommandError::OutOfRange)
    );
}

#[test]
fn rotating_turns_the_symbol_and_keeps_its_wires_orthogonal() {
    let (mut doc, mut history, [a, _, coil]) = three_parts();
    let catalog = ComponentCatalog::builtin();
    run(&mut doc, &mut history, |doc| {
        ops::connect(
            doc,
            &catalog,
            &Anchor::Endpoint(pin(a, "p2")),
            &Anchor::Endpoint(pin(coil, "a1")),
            &[],
        )
    });

    run(&mut doc, &mut history, |doc| {
        ops::rotate(doc, &catalog, &BTreeSet::from([a]))
    });

    assert_eq!(
        doc.symbols.iter().find(|s| s.id == a).unwrap().rotation,
        QuarterTurn::Deg90
    );
    assert!(wire_paths_are_orthogonal(&doc, &catalog));
}

#[test]
fn renaming_refuses_invalid_and_duplicate_designators() {
    let (mut doc, mut history, [a, b, _]) = three_parts();
    let (first, second) = (device_of(&doc, a), device_of(&doc, b));

    assert_eq!(
        ops::set_reference(&doc, first, "k 1"),
        Err(CommandError::InvalidReference("k 1".to_owned()))
    );
    assert_eq!(
        ops::set_reference(&doc, first, "S2"),
        Err(CommandError::DuplicateReference("S2".to_owned()))
    );
    run(&mut doc, &mut history, |doc| {
        ops::set_reference(doc, second, "S9")
    });
    assert!(doc.devices.iter().any(|d| d.reference.0 == "S9"));
}

#[test]
fn parameters_are_checked_against_the_model() {
    let mut doc = Schematic::new("params");
    let mut history = History::new();
    let catalog = ComponentCatalog::builtin();
    let coil = place(
        &mut doc,
        &mut history,
        "contactor.generic",
        "coil",
        point(0, 0),
    );
    let device = device_of(&doc, coil);

    assert!(matches!(
        ops::set_param(
            &doc,
            &catalog,
            device,
            "rated_voltage_mv",
            Some(ParamValue::Text("x".to_owned()))
        ),
        Err(CommandError::InvalidParameter(_))
    ));
    assert!(matches!(
        ops::set_param(&doc, &catalog, device, "colour", Some(ParamValue::Int(1))),
        Err(CommandError::InvalidParameter(_))
    ));
    run(&mut doc, &mut history, |doc| {
        ops::set_param(
            doc,
            &catalog,
            device,
            "rated_voltage_mv",
            Some(ParamValue::Int(24_000)),
        )
    });
    run(&mut doc, &mut history, |doc| {
        ops::set_param(
            doc,
            &catalog,
            device,
            "connection",
            Some(ParamValue::Text("dc".to_owned())),
        )
    });

    let params = &doc.devices[0].params;
    assert_eq!(
        params.get("rated_voltage_mv"),
        Some(&ParamValue::Int(24_000))
    );
    assert_eq!(
        params.get("connection"),
        Some(&ParamValue::Text("dc".to_owned()))
    );
}

#[test]
fn supply_terminal_must_name_an_existing_supply_and_keeps_it_from_removal() {
    let mut doc = Schematic::new("supplies");
    let mut history = History::new();
    let catalog = ComponentCatalog::builtin();
    run(&mut doc, &mut history, |doc| {
        ops::add_supply(doc, SupplyPreset::Dc24V)
    });
    let supply = doc.supplies[0].id;
    let tap = place(
        &mut doc,
        &mut history,
        "supply.terminal",
        "terminal",
        point(0, 0),
    );
    let device = device_of(&doc, tap);

    assert!(matches!(
        ops::set_param(&doc, &catalog, device, "supply", Some(ParamValue::Int(999))),
        Err(CommandError::InvalidParameter(_))
    ));
    run(&mut doc, &mut history, |doc| {
        ops::set_param(
            doc,
            &catalog,
            device,
            "supply",
            Some(ParamValue::Int(i64::try_from(supply.0).unwrap())),
        )
    });

    assert!(matches!(
        ops::remove_supply(&doc, &catalog, supply),
        Err(CommandError::InUse { .. })
    ));
}

#[test]
fn inconsistent_supply_profile_is_refused() {
    let mut doc = Schematic::new("supply");
    let mut history = History::new();
    run(&mut doc, &mut history, |doc| {
        ops::add_supply(doc, SupplyPreset::Dc24V)
    });
    let mut profile = doc.supplies[0].clone();
    profile.frequency_millihz = Some(50_000);

    assert!(matches!(
        ops::set_supply(&doc, profile),
        Err(CommandError::InvalidSupply(_))
    ));
}

/// The control circuit of a direct-online starter, drawn from nothing with
/// the operations alone.
fn draw_starter_control() -> (Schematic, History) {
    let mut doc = Schematic::new("drawn");
    let mut history = History::new();
    let catalog = ComponentCatalog::builtin();
    run(&mut doc, &mut history, |doc| {
        ops::add_supply(doc, SupplyPreset::Dc24V)
    });
    let supply = ParamValue::Int(i64::try_from(doc.supplies[0].id.0).unwrap());

    let p24 = place(
        &mut doc,
        &mut history,
        "supply.terminal",
        "terminal",
        point(0, 0),
    );
    let p0 = place(
        &mut doc,
        &mut history,
        "supply.terminal",
        "terminal",
        point(0, 22),
    );
    for (tap, conductor) in [(p24, "dc_pos"), (p0, "dc_neg")] {
        let device = device_of(&doc, tap);
        run(&mut doc, &mut history, |doc| {
            ops::set_param(doc, &catalog, device, "supply", Some(supply.clone()))
        });
        run(&mut doc, &mut history, |doc| {
            ops::set_param(
                doc,
                &catalog,
                device,
                "conductor",
                Some(ParamValue::Text(conductor.to_owned())),
            )
        });
    }
    let stop = place(
        &mut doc,
        &mut history,
        "pushbutton_nc",
        "contact",
        point(0, 4),
    );
    let start = place(
        &mut doc,
        &mut history,
        "pushbutton_no",
        "contact",
        point(-4, 11),
    );
    let coil = place(
        &mut doc,
        &mut history,
        "contactor.generic",
        "coil",
        point(0, 18),
    );
    let k1 = device_of(&doc, coil);
    for (param, value) in [
        ("rated_voltage_mv", ParamValue::Int(24_000)),
        ("connection", ParamValue::Text("dc".to_owned())),
    ] {
        run(&mut doc, &mut history, |doc| {
            ops::set_param(doc, &catalog, k1, param, Some(value))
        });
    }
    let seal_tx = ops::add_symbol(
        &doc,
        &catalog,
        k1,
        &key("aux_no"),
        point(4, 11),
        QuarterTurn::Deg0,
    )
    .unwrap();
    let seal = symbol_id(&seal_tx);
    history.execute(&mut doc, seal_tx).unwrap();

    let connect = |doc: &mut Schematic, history: &mut History, from: Anchor, to: Anchor| {
        let tx = ops::connect(doc, &catalog, &from, &to, &[]).unwrap();
        history.execute(doc, tx).unwrap();
    };
    let end = |e: Endpoint| Anchor::Endpoint(e);
    connect(
        &mut doc,
        &mut history,
        end(pin(p24, "out")),
        end(pin(stop, "p1")),
    );
    connect(
        &mut doc,
        &mut history,
        end(pin(stop, "p2")),
        end(pin(coil, "a1")),
    );
    connect(
        &mut doc,
        &mut history,
        end(pin(coil, "a2")),
        end(pin(p0, "out")),
    );
    // Branches: S1 and the seal-in contact in parallel between y=8 and y=14.
    let trunk = doc
        .wires
        .iter()
        .find(|w| w.from == pin(stop, "p2"))
        .unwrap()
        .id;
    connect(
        &mut doc,
        &mut history,
        end(pin(start, "p1")),
        Anchor::OnWire {
            wire: trunk,
            at: point(0, 8),
        },
    );
    let lower = doc
        .wires
        .iter()
        .find(|w| w.to == pin(coil, "a1"))
        .unwrap()
        .id;
    connect(
        &mut doc,
        &mut history,
        end(pin(start, "p2")),
        Anchor::OnWire {
            wire: lower,
            at: point(0, 14),
        },
    );
    let top = Endpoint::Junction(
        doc.junctions
            .iter()
            .find(|j| j.position == point(0, 8))
            .unwrap()
            .id,
    );
    let bottom = Endpoint::Junction(
        doc.junctions
            .iter()
            .find(|j| j.position == point(0, 14))
            .unwrap()
            .id,
    );
    connect(&mut doc, &mut history, end(pin(seal, "p1")), end(top));
    connect(&mut doc, &mut history, end(pin(seal, "p2")), end(bottom));
    (doc, history)
}

#[test]
fn starter_control_drawn_from_nothing_is_structurally_sound_and_orthogonal() {
    let (doc, _) = draw_starter_control();
    let catalog = ComponentCatalog::builtin();

    assert_eq!(doc.validate(), []);
    assert!(wire_paths_are_orthogonal(&doc, &catalog));
    assert_eq!((doc.devices.len(), doc.junctions.len()), (5, 2));
}

#[test]
fn undoing_the_whole_drawing_returns_to_an_empty_document() {
    let (mut doc, mut history) = draw_starter_control();
    let next_id = doc.next_id;

    while history.undo(&mut doc).unwrap() {}

    let mut empty = Schematic::new("drawn");
    empty.next_id = next_id;
    assert_eq!(doc, empty);
}

#[test]
fn redoing_the_whole_drawing_gives_back_the_same_document() {
    let (mut doc, mut history) = draw_starter_control();
    let drawn = doc.clone();
    while history.undo(&mut doc).unwrap() {}

    while history.redo(&mut doc).unwrap() {}

    assert_eq!(doc, drawn);
}

/// One random user action. Indices pick among what exists at the time.
#[derive(Clone, Debug)]
enum Action {
    Place(prop::sample::Index, prop::sample::Index, i8, i8),
    AddSymbol(prop::sample::Index, prop::sample::Index, i8, i8),
    Connect(prop::sample::Index, prop::sample::Index),
    Branch(
        prop::sample::Index,
        prop::sample::Index,
        prop::sample::Index,
    ),
    Delete(prop::sample::Index, prop::sample::Index),
    Move(prop::sample::Index, i8, i8),
    Rotate(prop::sample::Index),
    Rename(prop::sample::Index, prop::sample::Index),
    SetParam(prop::sample::Index, prop::sample::Index, i32),
    AddSupply(prop::sample::Index),
    RemoveSupply(prop::sample::Index),
    Undo,
    Redo,
}

fn action() -> impl Strategy<Value = Action> {
    use prop::sample::Index;
    let index = any::<Index>;
    prop_oneof![
        3 => (index(), index(), -20_i8..20, -20_i8..20).prop_map(|(a, b, x, y)| Action::Place(a, b, x, y)),
        1 => (index(), index(), -20_i8..20, -20_i8..20).prop_map(|(a, b, x, y)| Action::AddSymbol(a, b, x, y)),
        3 => (index(), index()).prop_map(|(a, b)| Action::Connect(a, b)),
        2 => (index(), index(), index()).prop_map(|(a, b, c)| Action::Branch(a, b, c)),
        1 => (index(), index()).prop_map(|(a, b)| Action::Delete(a, b)),
        2 => (index(), -5_i8..5, -5_i8..5).prop_map(|(a, x, y)| Action::Move(a, x, y)),
        1 => index().prop_map(Action::Rotate),
        1 => (index(), index()).prop_map(|(a, b)| Action::Rename(a, b)),
        2 => (index(), index(), any::<i32>()).prop_map(|(a, b, v)| Action::SetParam(a, b, v)),
        1 => index().prop_map(Action::AddSupply),
        1 => index().prop_map(Action::RemoveSupply),
        2 => Just(Action::Undo),
        1 => Just(Action::Redo),
    ]
}

/// Every terminal of every symbol, and every junction.
fn endpoints(doc: &Schematic, catalog: &ComponentCatalog) -> Vec<Endpoint> {
    let layout = LayoutIndex::new(doc, catalog);
    let mut all: Vec<Endpoint> = doc
        .symbols
        .iter()
        .filter_map(|s| layout.definition(s.id).map(|d| (s.id, d)))
        .flat_map(|(id, d)| {
            d.terminals()
                .iter()
                .map(move |t| pin(id, &t.key.0))
                .collect::<Vec<_>>()
        })
        .collect();
    all.extend(doc.junctions.iter().map(|j| Endpoint::Junction(j.id)));
    all
}

/// The subjects a random action can select.
fn subjects(doc: &Schematic) -> Vec<Subject> {
    let mut all: Vec<Subject> = doc.devices.iter().map(|d| Subject::Device(d.id)).collect();
    all.extend(doc.symbols.iter().map(|s| Subject::Symbol(s.id)));
    all.extend(doc.junctions.iter().map(|j| Subject::Junction(j.id)));
    all.extend(doc.wires.iter().map(|w| Subject::Wire(w.id)));
    all
}

/// The transaction of a random action, or `None` when nothing applies.
fn transaction(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    action: &Action,
) -> Option<Result<Transaction, CommandError>> {
    let models = catalog.models();
    let names = ["K1", "S1", "X9", "Q1", "bad name", "KT3"];
    let pick = |items: &[Subject], i: &prop::sample::Index| {
        (!items.is_empty()).then(|| i.get(items).clone())
    };
    Some(match action {
        Action::Place(m, s, x, y) => {
            let model = m.get(models);
            let symbol = s.get(model.symbols());
            ops::place(
                doc,
                catalog,
                &ModelRef {
                    id: model.id().to_owned(),
                    version: model.version(),
                },
                symbol.key(),
                point((*x).into(), (*y).into()),
                QuarterTurn::Deg0,
            )
        }
        Action::AddSymbol(d, s, x, y) => {
            let device = (!doc.devices.is_empty()).then(|| d.get(&doc.devices))?;
            let model = catalog.model(&device.model)?;
            let symbol = s.get(model.symbols());
            ops::add_symbol(
                doc,
                catalog,
                device.id,
                symbol.key(),
                point((*x).into(), (*y).into()),
                QuarterTurn::Deg0,
            )
        }
        Action::Connect(a, b) => {
            let all = endpoints(doc, catalog);
            if all.is_empty() {
                return None;
            }
            ops::connect(
                doc,
                catalog,
                &Anchor::Endpoint(a.get(&all).clone()),
                &Anchor::Endpoint(b.get(&all).clone()),
                &[],
            )
        }
        Action::Branch(a, w, p) => {
            let all = endpoints(doc, catalog);
            if all.is_empty() || doc.wires.is_empty() {
                return None;
            }
            let wire = w.get(&doc.wires);
            let path = LayoutIndex::new(doc, catalog).wire_polyline(wire)?;
            ops::connect(
                doc,
                catalog,
                &Anchor::Endpoint(a.get(&all).clone()),
                &Anchor::OnWire {
                    wire: wire.id,
                    at: *p.get(&path),
                },
                &[],
            )
        }
        Action::Delete(a, b) => {
            let all = subjects(doc);
            let chosen: BTreeSet<Subject> = [pick(&all, a)?, pick(&all, b)?].into_iter().collect();
            ops::delete(doc, &chosen)
        }
        Action::Move(a, x, y) => {
            let all = subjects(doc);
            ops::move_by(
                doc,
                catalog,
                &BTreeSet::from([pick(&all, a)?]),
                point((*x).into(), (*y).into()),
            )
        }
        Action::Rotate(a) => {
            let symbol = (!doc.symbols.is_empty()).then(|| a.get(&doc.symbols).id)?;
            ops::rotate(doc, catalog, &BTreeSet::from([symbol]))
        }
        Action::Rename(d, n) => {
            let device = (!doc.devices.is_empty()).then(|| d.get(&doc.devices).id)?;
            ops::set_reference(doc, device, n.get::<&str>(names.as_slice()))
        }
        Action::SetParam(d, p, value) => {
            let device = (!doc.devices.is_empty()).then(|| d.get(&doc.devices))?;
            let model = catalog.model(&device.model)?;
            let definition = (!model.params().is_empty()).then(|| p.get(model.params()))?;
            let value = match &definition.ty {
                circuit_model::ParamType::Bool => ParamValue::Bool(value % 2 == 0),
                circuit_model::ParamType::Int { .. } | circuit_model::ParamType::SupplyRef => {
                    ParamValue::Int((*value).into())
                }
                circuit_model::ParamType::Enum(choices) => {
                    let index =
                        usize::try_from(value.unsigned_abs()).unwrap_or(0) % (choices.len() + 1);
                    ParamValue::Text(
                        choices
                            .get(index)
                            .cloned()
                            .unwrap_or_else(|| "nope".to_owned()),
                    )
                }
            };
            let value = (value != ParamValue::Int(0)).then_some(value);
            ops::set_param(doc, catalog, device.id, &definition.key, value)
        }
        Action::AddSupply(preset) => ops::add_supply(doc, *preset.get(&SupplyPreset::ALL)),
        Action::RemoveSupply(s) => {
            let supply = (!doc.supplies.is_empty()).then(|| s.get(&doc.supplies).id)?;
            ops::remove_supply(doc, catalog, supply)
        }
        Action::Undo | Action::Redo => return None,
    })
}

/// IDs created by a transaction: they must all be fresh.
fn created_ids(tx: &Transaction) -> Vec<u64> {
    tx.edits
        .iter()
        .filter_map(|edit| match edit {
            Edit::InsertSupply(s) => Some(s.id.0),
            Edit::InsertDevice(d) => Some(d.id.0),
            Edit::InsertSymbol(s) => Some(s.id.0),
            Edit::InsertJunction(j) => Some(j.id.0),
            Edit::InsertWire(w) => Some(w.id.0),
            _ => None,
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn random_editing_never_breaks_the_document_and_undo_restores_it(
        actions in prop::collection::vec(action(), 1..60)
    ) {
        let catalog = ComponentCatalog::builtin();
        let mut doc = Schematic::new("random");
        let initial = doc.clone();
        let mut history = History::new();

        for action in &actions {
            let next_id = doc.next_id;
            match action {
                Action::Undo => { prop_assert!(history.undo(&mut doc).is_ok()); }
                Action::Redo => { prop_assert!(history.redo(&mut doc).is_ok()); }
                other => {
                    if let Some(Ok(tx)) = transaction(&doc, &catalog, other) {
                        for id in created_ids(&tx) {
                            prop_assert!(id >= next_id, "{id} reused below next_id {next_id}");
                        }
                        let before = doc.clone();
                        if history.execute(&mut doc, tx).is_err() {
                            prop_assert_eq!(&doc, &before);
                        }
                    }
                }
            }
            prop_assert!(doc.next_id >= next_id);
            prop_assert_eq!(integrity(&doc), Vec::<DiagnosticCode>::new());
            prop_assert!(circuit_model::limits::check(&doc).is_ok());
            let mut sorted = doc.clone();
            sorted.sort_canonical();
            prop_assert_eq!(&sorted, &doc);
        }

        let next_id = doc.next_id;
        while history.undo(&mut doc).map_err(|e| TestCaseError::fail(e.to_string()))? {}
        prop_assert_eq!(doc, Schematic { next_id, ..initial });
    }
}

#[test]
fn supply_ids_of_presets_are_fresh() {
    let doc = Schematic::new("ids");
    let tx = ops::add_supply(&doc, SupplyPreset::ThreePhase400V).unwrap();

    assert_eq!(created_ids(&tx), [doc.next_id]);
    assert!(matches!(&tx.edits[0], Edit::InsertSupply(p) if p.id == SupplyId(doc.next_id)));
}

#[test]
fn placing_a_symbol_whose_area_leaves_the_coordinate_range_is_refused() {
    let doc = Schematic::new("edge");
    let catalog = ComponentCatalog::builtin();

    assert_eq!(
        ops::place(
            &doc,
            &catalog,
            &model("pushbutton_no"),
            &key("contact"),
            point(i32::MAX, 0),
            QuarterTurn::Deg0
        ),
        Err(CommandError::OutOfRange)
    );
}

/// A primitive edit with random targets, valid or not.
fn arbitrary_edit() -> impl Strategy<Value = Edit> {
    use circuit_model::{
        Device, Junction, JunctionId, ReferenceDesignator, SupplyKind, SupplyProfile,
        SymbolInstance, Wire, WireId,
    };
    let id = || prop_oneof![8 => 0_u64..40, 1 => Just(u64::MAX)];
    let at = || (-30_i32..30, -30_i32..30).prop_map(|(x, y)| point(x, y));
    let endpoint = move || {
        prop_oneof![
            (id(), prop::sample::select(vec!["p1", "p2", "a1", "out"]))
                .prop_map(|(s, t)| pin(SymbolId(s), t)),
            id().prop_map(|j| Endpoint::Junction(JunctionId(j))),
        ]
    };
    prop_oneof![
        id().prop_map(|i| Edit::InsertSupply(SupplyProfile {
            id: SupplyId(i),
            kind: SupplyKind::Dc,
            nominal_voltage_mv: 24_000,
            phase_voltage_mv: None,
            frequency_millihz: None,
            phase_sequence: None,
        })),
        id().prop_map(|i| Edit::RemoveSupply(SupplyId(i))),
        (id(), prop::sample::select(vec!["K1", "S1", "bad name", ""])).prop_map(|(i, r)| {
            Edit::InsertDevice(Device {
                id: DeviceId(i),
                reference: ReferenceDesignator(r.to_owned()),
                model: model("pushbutton_no"),
                params: Default::default(),
            })
        }),
        id().prop_map(|i| Edit::RemoveDevice(DeviceId(i))),
        (id(), any::<i64>()).prop_map(|(i, v)| Edit::SetParam {
            device: DeviceId(i),
            key: "rated_voltage_mv".to_owned(),
            value: Some(ParamValue::Int(v)),
        }),
        (id(), id(), at()).prop_map(|(i, d, p)| Edit::InsertSymbol(SymbolInstance {
            id: SymbolId(i),
            device_id: DeviceId(d),
            symbol_key: key("contact"),
            slot: None,
            position: p,
            rotation: QuarterTurn::Deg0,
        })),
        id().prop_map(|i| Edit::RemoveSymbol(SymbolId(i))),
        (id(), at()).prop_map(|(i, p)| Edit::Place {
            symbol: SymbolId(i),
            position: p,
            rotation: QuarterTurn::Deg90,
        }),
        (id(), at()).prop_map(|(i, p)| Edit::InsertJunction(Junction {
            id: JunctionId(i),
            position: p,
        })),
        id().prop_map(|i| Edit::RemoveJunction(JunctionId(i))),
        (id(), at()).prop_map(|(i, p)| Edit::MoveJunction {
            junction: JunctionId(i),
            position: p,
        }),
        (id(), endpoint(), endpoint()).prop_map(|(i, from, to)| Edit::InsertWire(Wire {
            id: WireId(i),
            from,
            to,
            route: Vec::new(),
        })),
        id().prop_map(|i| Edit::RemoveWire(WireId(i))),
        (id(), prop::collection::vec(at(), 0..3)).prop_map(|(i, route)| Edit::SetRoute {
            wire: WireId(i),
            route,
        }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn arbitrary_transactions_apply_entirely_or_not_at_all(
        transactions in prop::collection::vec(
            (prop::collection::vec(arbitrary_edit(), 1..6), 0_u64..48),
            1..12,
        )
    ) {
        let (mut doc, mut history) = draw_starter_control();
        for (edits, next_id) in transactions {
            let before = doc.clone();
            let mut tx = Transaction::new("arbitrary", next_id);
            tx.edits = edits;
            match history.execute(&mut doc, tx) {
                Ok(()) => {
                    prop_assert_eq!(integrity(&doc), Vec::<DiagnosticCode>::new());
                    prop_assert!(doc.next_id >= before.next_id);
                }
                Err(_) => prop_assert_eq!(&doc, &before),
            }
        }
        while history.undo(&mut doc).map_err(|e| TestCaseError::fail(e.to_string()))? {}
        prop_assert_eq!(doc.devices.len(), 0);
    }
}
