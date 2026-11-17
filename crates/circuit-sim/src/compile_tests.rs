use circuit_model::builder::pin;
use circuit_model::{Endpoint, GridPoint, ParamValue, QuarterTurn, StaticNets, Wire, WireId};
use proptest::prelude::*;

use super::*;
use crate::testkit::{
    Interlocks, Rig, Secondary, codes, command_circuit, compile_doc, compile_ok, dc_load,
    direct_online_circuit, load, out, reversing_circuit, reversing_with, transformer_circuit,
    two_sections,
};

fn net_of(circuit: &CompiledCircuit, symbol: SymbolId, terminal: &str) -> NetIx {
    circuit
        .pins()
        .iter()
        .find(|pin| pin.pin.symbol_id == symbol && pin.pin.terminal.0 == terminal)
        .unwrap_or_else(|| panic!("no pin {symbol}:{terminal}"))
        .net
}

#[test]
fn every_load_carries_the_stable_id_of_its_device() {
    for doc in [
        direct_online_circuit(),
        reversing_circuit(),
        transformer_circuit(Secondary::Own),
    ] {
        let circuit = compile_ok(&doc);
        assert!(!circuit.loads().is_empty());
        for load in circuit.loads() {
            assert_ne!(load.device_id, DeviceId(0));
            assert_eq!(
                circuit.device(load.device).map(|device| device.id),
                Some(load.device_id)
            );
            assert!(doc.devices.iter().any(|device| device.id == load.device_id));
        }
    }
}

fn symbol_of(doc: &Schematic, reference: &str, key: &str) -> SymbolId {
    let device = doc
        .devices
        .iter()
        .find(|device| device.reference.0 == reference)
        .unwrap_or_else(|| panic!("no device {reference}"));
    doc.symbols
        .iter()
        .find(|symbol| symbol.device_id == device.id && symbol.symbol_key.0 == key)
        .unwrap_or_else(|| panic!("no symbol {reference}.{key}"))
        .id
}

/// A lamp between P24 and P0: the smallest circuit that compiles cleanly.
fn lamp_circuit() -> (Rig, SymbolId, SymbolId, SymbolId) {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let lamp = rig.part("H1", "lamp.generic", "lamp", &dc_load());
    rig.wire(out(p24), pin(lamp, "x1"));
    rig.wire(pin(lamp, "x2"), out(p0));
    (rig, p24, p0, lamp)
}

#[test]
fn command_circuit_compiles_without_diagnostics() {
    let output = compile_doc(&command_circuit());

    assert_eq!(output.diagnostics, []);
    assert!(output.circuit.is_some());
}

#[test]
fn reference_circuits_compile_without_diagnostics() {
    for doc in [direct_online_circuit(), reversing_circuit()] {
        let output = compile_doc(&doc);

        assert_eq!(output.diagnostics, [], "{}", doc.project.name);
        assert!(output.circuit.is_some());
    }
}

#[test]
fn junction_joins_its_wires_into_one_net() {
    let doc = command_circuit();
    let circuit = compile_ok(&doc);
    let stop = symbol_of(&doc, "S2", "contact");
    let start = symbol_of(&doc, "S1", "contact");
    let seal = symbol_of(&doc, "K1", "aux_no");

    assert_eq!(net_of(&circuit, stop, "p2"), net_of(&circuit, start, "p1"));
    assert_eq!(net_of(&circuit, stop, "p2"), net_of(&circuit, seal, "p1"));
}

#[test]
fn the_two_sides_of_a_contact_are_different_nets() {
    let doc = command_circuit();
    let circuit = compile_ok(&doc);
    let start = symbol_of(&doc, "S1", "contact");

    assert_ne!(net_of(&circuit, start, "p1"), net_of(&circuit, start, "p2"));
}

#[test]
fn wires_that_cross_without_a_junction_are_not_connected() {
    let (rig, _, _, lamp) = lamp_circuit();
    let mut doc = rig.finish();
    // Both wires pass through the same grid point. Only endpoints connect.
    for wire in &mut doc.wires {
        wire.route = vec![GridPoint::new(5, 5)];
    }

    let output = compile_doc(&doc);

    // The forced routes are diagonal, which is only a warning.
    assert!(
        codes(&output)
            .iter()
            .all(|code| *code == DiagnosticCode::NonOrthogonalWire)
    );
    let circuit = output.circuit.unwrap();
    assert_ne!(net_of(&circuit, lamp, "x1"), net_of(&circuit, lamp, "x2"));
}

#[test]
fn diagonal_wire_is_reported_once_and_does_not_block_compilation() {
    let (rig, _, _, _) = lamp_circuit();
    let mut doc = rig.finish();
    let wire = doc.wires[0].id;
    doc.wires[0].route.clear();
    let ends_differ_in_both_axes = {
        let catalog = ComponentCatalog::builtin();
        let layout = circuit_model::LayoutIndex::new(&doc, &catalog);
        let path = layout.wire_polyline(&doc.wires[0]).unwrap();
        path[0].x != path[1].x && path[0].y != path[1].y
    };
    assert!(ends_differ_in_both_axes);

    let output = compile_doc(&doc);

    assert_eq!(codes(&output), [DiagnosticCode::NonOrthogonalWire]);
    assert_eq!(output.diagnostics[0].subject(), &Subject::Wire(wire));
    assert!(output.circuit.is_some());
}

fn sheet(id: u64, number: u32, origin: GridPoint) -> circuit_model::Sheet {
    circuit_model::Sheet {
        id: circuit_model::SheetId(id),
        number,
        title: String::new(),
        origin,
        size: circuit_model::SheetSize::A4,
        orientation: circuit_model::Orientation::Landscape,
        zones: circuit_model::Zones {
            columns: 8,
            rows: 6,
        },
    }
}

#[test]
fn a_wire_across_the_edge_of_a_sheet_is_a_warning_and_still_compiles() {
    let mut doc = command_circuit();
    let bounds = doc
        .symbols
        .iter()
        .map(|symbol| symbol.position)
        .fold((i32::MAX, i32::MIN), |(low, high), p| {
            (low.min(p.y), high.max(p.y))
        });
    // A frame whose bottom edge runs through the middle of the circuit.
    let middle = bounds.0 + (bounds.1 - bounds.0) / 2;
    let id = doc.next_id;
    doc.sheets
        .push(sheet(id, 1, GridPoint::new(-40, middle - 59)));
    doc.next_id += 1;

    let output = compile_doc(&doc);

    assert!(codes(&output).contains(&DiagnosticCode::WireCrossesSheet));
    assert!(output.circuit.is_some());

    // The same frame away from the circuit: no warning.
    doc.sheets[0].origin = GridPoint::new(1_000, 1_000);
    assert!(!codes(&compile_doc(&doc)).contains(&DiagnosticCode::WireCrossesSheet));
}

#[test]
fn orthogonal_wires_raise_no_geometry_warning() {
    let output = compile_doc(&command_circuit());

    assert!(!codes(&output).contains(&DiagnosticCode::NonOrthogonalWire));
}

#[test]
fn nets_are_numbered_by_first_pin_in_canonical_order() {
    let circuit = compile_ok(&reversing_circuit());
    let mut next = 0;
    for pin in circuit.pins() {
        assert!(pin.net.raw() <= next, "net numbering skipped a value");
        if pin.net.raw() == next {
            next += 1;
        }
    }
}

#[test]
fn unknown_model_is_reported_and_stops_compilation() {
    let mut rig = Rig::new();
    rig.part("X1", "does.not.exist", "contact", &[]);

    let output = compile_doc(&rig.finish());

    assert_eq!(codes(&output), [DiagnosticCode::UnknownModel]);
    assert_eq!(output.circuit, None);
}

#[test]
fn unknown_model_version_is_reported() {
    let (rig, ..) = lamp_circuit();
    let mut doc = rig.finish();
    doc.devices
        .iter_mut()
        .find(|device| device.reference.0 == "H1")
        .unwrap()
        .model
        .version = 2;

    assert_eq!(codes(&compile_doc(&doc)), [DiagnosticCode::UnknownModel]);
}

#[test]
fn unknown_symbol_key_is_reported() {
    let mut rig = Rig::new();
    rig.dc24();
    let s1 = rig.device("S1", "pushbutton_no", &[]);
    rig.symbol(s1, "contact", None);
    rig.symbol(s1, "coil", None);

    let output = compile_doc(&rig.finish());

    assert!(codes(&output).contains(&DiagnosticCode::UnknownSymbolKey));
    assert_eq!(output.circuit, None);
}

#[test]
fn unknown_terminal_key_is_reported() {
    let (mut rig, p24, _, lamp) = lamp_circuit();
    rig.wire(out(p24), pin(lamp, "a1"));

    let output = compile_doc(&rig.finish());

    assert_eq!(codes(&output), [DiagnosticCode::UnknownTerminal]);
    assert_eq!(output.circuit, None);
}

#[test]
fn slotted_symbol_without_slot_is_reported() {
    let mut doc = command_circuit();
    let seal = symbol_of(&doc, "K1", "aux_no");
    doc.symbols
        .iter_mut()
        .find(|symbol| symbol.id == seal)
        .unwrap()
        .slot = None;

    assert_eq!(
        codes(&compile_doc(&doc)),
        [DiagnosticCode::SymbolInstanceLimit]
    );
}

#[test]
fn slot_on_a_symbol_that_takes_none_is_reported() {
    let mut doc = command_circuit();
    let coil = symbol_of(&doc, "K1", "coil");
    doc.symbols
        .iter_mut()
        .find(|symbol| symbol.id == coil)
        .unwrap()
        .slot = Some(1);

    assert_eq!(
        codes(&compile_doc(&doc)),
        [DiagnosticCode::SymbolInstanceLimit]
    );
}

#[test]
fn slot_used_twice_by_one_device_is_reported() {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let k1 = rig.device("K1", "contactor.generic", &dc_load());
    let coil = rig.symbol(k1, "coil", None);
    let first = rig.symbol(k1, "aux_no", Some(1));
    let second = rig.symbol(k1, "aux_no", Some(1));
    rig.wire(out(p24), pin(coil, "a1"));
    rig.wire(pin(coil, "a2"), out(p0));

    let output = compile_doc(&rig.finish());

    assert_eq!(codes(&output), [DiagnosticCode::SymbolInstanceLimit]);
    assert_eq!(output.diagnostics[0].subject(), &Subject::Symbol(first));
    assert_eq!(output.diagnostics[0].related(), [Subject::Symbol(second)]);
}

#[test]
fn more_instances_than_the_model_allows_is_reported() {
    let mut rig = Rig::new();
    let k1 = rig.device("K1", "contactor.generic", &dc_load());
    rig.symbol(k1, "coil", None);
    rig.symbol(k1, "coil", None);

    let output = compile_doc(&rig.finish());

    assert_eq!(codes(&output), [DiagnosticCode::SymbolInstanceLimit]);
    assert_eq!(output.diagnostics[0].subject(), &Subject::Device(k1));
}

#[test]
fn missing_mandatory_symbol_is_reported() {
    let mut rig = Rig::new();
    let k1 = rig.device("K1", "contactor.generic", &dc_load());
    rig.symbol(k1, "aux_no", Some(1));

    let output = compile_doc(&rig.finish());

    assert_eq!(codes(&output), [DiagnosticCode::SymbolInstanceLimit]);
    assert_eq!(
        output.diagnostics[0].message(),
        "symbol `coil` needs between 1 and 1 instances, found 0"
    );
}

#[test]
fn missing_and_invalid_parameters_are_reported() {
    let mut rig = Rig::new();
    let k1 = rig.device(
        "K1",
        "contactor.generic",
        &[("rated_voltage_mv", ParamValue::Int(-5))],
    );
    rig.symbol(k1, "coil", None);

    let output = compile_doc(&rig.finish());

    assert_eq!(
        codes(&output),
        [
            DiagnosticCode::InvalidParameter,
            DiagnosticCode::InvalidParameter
        ]
    );
    assert_eq!(output.circuit, None);
}

#[test]
fn supply_terminal_of_a_missing_supply_is_a_dangling_reference() {
    let mut rig = Rig::new();
    rig.tap("P24", "dc_pos", SupplyId(99));

    let output = compile_doc(&rig.finish());

    assert_eq!(codes(&output), [DiagnosticCode::DanglingReference]);
    assert_eq!(
        output.diagnostics[0].related(),
        [Subject::Supply(SupplyId(99))]
    );
}

#[test]
fn conductor_the_supply_kind_cannot_provide_is_rejected() {
    let mut rig = Rig::new();
    let supply = rig.dc_supply(24_000);
    rig.tap("L1", "l1", supply);

    assert_eq!(
        codes(&compile_doc(&rig.finish())),
        [DiagnosticCode::InvalidParameter]
    );
}

#[test]
fn coil_with_an_unwired_terminal_does_not_compile() {
    let mut rig = Rig::new();
    let (p24, _) = rig.dc24();
    let coil = rig.part("K1", "relay.generic", "coil", &dc_load());
    rig.wire(out(p24), pin(coil, "a1"));

    let output = compile_doc(&rig.finish());

    assert!(codes(&output).contains(&DiagnosticCode::MissingRequiredTerminal));
    assert_eq!(output.circuit, None);
}

#[test]
fn unwired_contact_terminal_is_allowed() {
    let (mut rig, ..) = lamp_circuit();
    rig.part("S9", "pushbutton_no", "contact", &[]);

    let output = compile_doc(&rig.finish());

    assert_eq!(output.diagnostics, []);
}

#[test]
fn one_conductor_from_two_supplies_is_allowed_in_separate_sections() {
    // OC-SEM-004 is retired (ADR-0005): each supply is its own section.
    let mut rig = Rig::new();
    let first = rig.dc_supply(24_000);
    let second = rig.dc_supply(12_000);
    let p24 = rig.tap("P24", "dc_pos", first);
    let n24 = rig.tap("N24", "dc_neg", first);
    let p12 = rig.tap("P12", "dc_pos", second);
    let n12 = rig.tap("N12", "dc_neg", second);
    for (pos, neg, reference, voltage) in [(p24, n24, "H1", 24_000), (p12, n12, "H2", 12_000)] {
        let lamp = rig.part(reference, "lamp.generic", "lamp", &load(voltage, "dc"));
        rig.wire(out(pos), pin(lamp, "x1"));
        rig.wire(pin(lamp, "x2"), out(neg));
    }

    let output = compile_doc(&rig.finish());

    assert_eq!(output.diagnostics, []);
    assert!(output.circuit.is_some());
}

#[test]
fn sections_wired_together_permanently_are_a_static_short() {
    let mut rig = Rig::new();
    let first = rig.dc_supply(24_000);
    let second = rig.dc_supply(24_000);
    let a = rig.tap("P24", "dc_pos", first);
    let b = rig.tap("Q24", "dc_pos", second);
    rig.wire(out(a), out(b));

    let output = compile_doc(&rig.finish());

    assert!(codes(&output).contains(&DiagnosticCode::ShortCircuit));
    assert!(!codes(&output).contains(&DiagnosticCode::ConflictingSupplies));
    assert_eq!(output.circuit, None);
}

#[test]
fn device_without_symbols_is_a_warning() {
    let (mut rig, ..) = lamp_circuit();
    let f1 = rig.device("F1", "thermal_overload.generic", &[]);

    let output = compile_doc(&rig.finish());

    assert_eq!(codes(&output), [DiagnosticCode::DeviceWithoutSymbols]);
    assert_eq!(output.diagnostics[0].subject(), &Subject::Device(f1));
    assert!(output.circuit.is_some());
}

#[test]
fn loads_without_any_supply_are_a_warning() {
    let mut rig = Rig::new();
    let lamp = rig.part("H1", "lamp.generic", "lamp", &dc_load());
    let button = rig.part("S1", "pushbutton_no", "contact", &[]);
    rig.wire(pin(lamp, "x1"), pin(button, "p1"));

    let output = compile_doc(&rig.finish());

    assert_eq!(codes(&output), [DiagnosticCode::NoSupply]);
    assert!(output.circuit.is_some());
}

#[test]
fn supply_terminal_that_reaches_no_load_is_informational() {
    let (mut rig, ..) = lamp_circuit();
    let supply = rig.three_phase_supply(PhaseSequence::L1L2L3);
    rig.tap("L1", "l1", supply);

    let output = compile_doc(&rig.finish());

    assert_eq!(codes(&output), [DiagnosticCode::UnusedSupply]);
    assert!(output.circuit.is_some());
}

#[test]
fn net_no_supply_can_reach_is_a_warning() {
    let (mut rig, ..) = lamp_circuit();
    let lamp = rig.part("H2", "lamp.generic", "lamp", &dc_load());
    let button = rig.part("S1", "pushbutton_no", "contact", &[]);
    rig.wire(pin(lamp, "x1"), pin(button, "p1"));

    let output = compile_doc(&rig.finish());

    assert_eq!(codes(&output), [DiagnosticCode::FloatingNet]);
    assert_eq!(
        output.diagnostics[0].subject(),
        &Subject::Pin(PinRef {
            symbol_id: lamp,
            terminal: circuit_model::TerminalKey("x1".to_owned()),
        })
    );
}

#[test]
fn supply_poles_wired_together_are_a_static_short_circuit() {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    rig.wire(out(p24), out(p0));

    let output = compile_doc(&rig.finish());

    assert!(codes(&output).contains(&DiagnosticCode::ShortCircuit));
    assert_eq!(output.circuit, None);
}

#[test]
fn short_circuit_through_a_fuse_is_still_static() {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let fuse = rig.part("F2", "fuse.generic", "link", &[]);
    rig.wire(out(p24), pin(fuse, "p1"));
    rig.wire(pin(fuse, "p2"), out(p0));

    let output = compile_doc(&rig.finish());

    assert!(codes(&output).contains(&DiagnosticCode::ShortCircuit));
    assert_eq!(output.circuit, None);
}

#[test]
fn short_circuit_behind_an_open_contact_is_left_to_the_runtime() {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let button = rig.part("S1", "pushbutton_no", "contact", &[]);
    rig.wire(out(p24), pin(button, "p1"));
    rig.wire(pin(button, "p2"), out(p0));

    let output = compile_doc(&rig.finish());

    assert!(!codes(&output).contains(&DiagnosticCode::ShortCircuit));
    assert!(output.circuit.is_some());
}

#[test]
fn earth_wired_to_a_phase_is_a_static_earth_fault() {
    let mut rig = Rig::new();
    let supply = rig.three_phase_supply(PhaseSequence::L1L2L3);
    let line = rig.tap("L1", "l1", supply);
    let earth = rig.tap("PE", "pe", supply);
    rig.wire(out(line), out(earth));

    let output = compile_doc(&rig.finish());

    assert!(codes(&output).contains(&DiagnosticCode::ProtectiveEarthFault));
    assert_eq!(output.circuit, None);
}

#[test]
fn neutral_bonded_to_earth_is_a_warning_and_still_compiles() {
    let mut rig = Rig::new();
    let supply = rig.three_phase_supply(PhaseSequence::L1L2L3);
    let neutral = rig.tap("N", "n", supply);
    let earth = rig.tap("PE", "pe", supply);
    rig.wire(out(neutral), out(earth));

    let output = compile_doc(&rig.finish());

    assert!(codes(&output).contains(&DiagnosticCode::NeutralEarthBond));
    assert!(output.circuit.is_some());
}

#[test]
fn loads_hard_wired_in_series_do_not_compile() {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let first = rig.part("H1", "lamp.generic", "lamp", &dc_load());
    let second = rig.part("H2", "lamp.generic", "lamp", &dc_load());
    rig.wire(out(p24), pin(first, "x1"));
    rig.wire(pin(first, "x2"), pin(second, "x1"));
    rig.wire(pin(second, "x2"), out(p0));

    let output = compile_doc(&rig.finish());

    assert!(codes(&output).contains(&DiagnosticCode::StaticSeriesLoads));
    assert_eq!(output.circuit, None);
}

#[test]
fn loads_sharing_a_switched_node_compile() {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let button = rig.part("S1", "pushbutton_no", "contact", &[]);
    let coil = rig.part("K1", "relay.generic", "coil", &dc_load());
    let lamp = rig.part("H1", "lamp.generic", "lamp", &dc_load());
    let node = rig.junction();
    let common = rig.junction();
    rig.wire(out(p24), pin(button, "p1"));
    rig.wire(pin(button, "p2"), node);
    rig.wire(node, pin(coil, "a1"));
    rig.wire(node, pin(lamp, "x1"));
    rig.wire(pin(coil, "a2"), common);
    rig.wire(pin(lamp, "x2"), common);
    rig.wire(common, out(p0));

    assert_eq!(compile_doc(&rig.finish()).diagnostics, []);
}

#[test]
fn main_contacts_become_one_switch_per_pole_with_a_shared_signal() {
    let doc = direct_online_circuit();
    let circuit = compile_ok(&doc);
    let main = symbol_of(&doc, "K1", "main");
    let drives: Vec<Drive> = circuit
        .switches()
        .iter()
        .filter(|switch| switch.owner == main)
        .map(|switch| switch.drive)
        .collect();

    assert_eq!(drives.len(), 3);
    assert!(matches!(
        drives[0],
        Drive::Signal {
            closed_when: Level::High,
            ..
        }
    ));
    assert!(drives.iter().all(|drive| *drive == drives[0]));
}

#[test]
fn changeover_becomes_two_switches_from_its_common_on_one_signal() {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let k1 = rig.device_v2("K1", "relay.generic", &dc_load());
    let coil = rig.symbol(k1, "coil", None);
    let changeover = rig.symbol(k1, "changeover", Some(1));
    rig.wire(out(p24), pin(coil, "a1"));
    rig.wire(pin(coil, "a2"), out(p0));
    rig.wire(out(p24), pin(changeover, "com"));
    let doc = rig.finish();
    let circuit = compile_ok(&doc);
    let switches: Vec<&CompiledSwitch> = circuit
        .switches()
        .iter()
        .filter(|switch| switch.owner == changeover)
        .collect();
    let common = net_of(&circuit, changeover, "com");

    assert_eq!(switches.len(), 2);
    let sides: Vec<(NetIx, Level)> = switches
        .iter()
        .map(|switch| {
            assert_eq!(switch.a, common);
            let Drive::Signal { closed_when, .. } = switch.drive else {
                panic!("a changeover is driven by its device");
            };
            (switch.b, closed_when)
        })
        .collect();
    assert_eq!(
        sides,
        [
            (net_of(&circuit, changeover, "nc"), Level::Low),
            (net_of(&circuit, changeover, "no"), Level::High)
        ]
    );
    let [first, second] = [switches[0].drive, switches[1].drive].map(|drive| match drive {
        Drive::Signal { sig, .. } => Some(sig),
        Drive::Always => None,
    });
    assert_eq!(first, second);
}

#[test]
fn version_1_relays_offer_no_changeover() {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let k1 = rig.device("K1", "relay.generic", &dc_load());
    let coil = rig.symbol(k1, "coil", None);
    rig.symbol(k1, "changeover", Some(1));
    rig.wire(out(p24), pin(coil, "a1"));
    rig.wire(pin(coil, "a2"), out(p0));

    assert!(codes(&compile_doc(&rig.finish())).contains(&DiagnosticCode::UnknownSymbolKey));
}

#[test]
fn thermal_element_and_fuse_are_always_closed_switches() {
    let doc = direct_online_circuit();
    let circuit = compile_ok(&doc);
    let element = symbol_of(&doc, "F1", "element");
    let always = circuit
        .switches()
        .iter()
        .filter(|switch| switch.owner == element && switch.drive == Drive::Always)
        .count();

    assert_eq!(always, 3);
}

#[test]
fn normally_closed_contact_is_closed_while_its_signal_is_low() {
    let doc = command_circuit();
    let circuit = compile_ok(&doc);
    let stop = symbol_of(&doc, "S2", "contact");
    let switch = circuit
        .switches()
        .iter()
        .find(|switch| switch.owner == stop)
        .unwrap();

    assert!(matches!(
        switch.drive,
        Drive::Signal {
            closed_when: Level::Low,
            ..
        }
    ));
}

#[test]
fn every_signal_belongs_to_one_device_and_has_one_kind() {
    let circuit = compile_ok(&direct_online_circuit());
    let kinds: Vec<(String, SignalKind)> = circuit
        .signals()
        .iter()
        .map(|signal| {
            (
                circuit
                    .device_by_id(signal.owner)
                    .unwrap()
                    .reference
                    .0
                    .clone(),
                signal.kind,
            )
        })
        .collect();

    assert_eq!(
        kinds,
        [
            ("Q1".to_owned(), SignalKind::Actuated),
            ("F1".to_owned(), SignalKind::Tripped),
            ("S2".to_owned(), SignalKind::Actuated),
            ("S1".to_owned(), SignalKind::Actuated),
            ("K1".to_owned(), SignalKind::CoilActive),
        ]
    );
}

#[test]
fn timer_preset_and_initial_switch_state_reach_the_compiled_device() {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let mut timer_params = dc_load();
    timer_params.push(("preset_ms", ParamValue::Int(250)));
    let coil = rig.part("KT1", "timer_on.generic", "coil", &timer_params);
    let selector = rig.part(
        "SA1",
        "selector.latching",
        "contact",
        &[("initial_closed", ParamValue::Bool(true))],
    );
    rig.wire(out(p24), pin(selector, "p1"));
    rig.wire(pin(selector, "p2"), pin(coil, "a1"));
    rig.wire(pin(coil, "a2"), out(p0));

    let circuit = compile_ok(&rig.finish());

    let timer = circuit.device_by_reference("KT1").unwrap();
    let switch = circuit.device_by_reference("SA1").unwrap();
    assert_eq!(timer.preset_ms, Some(250));
    assert!(timer.coil.is_some());
    assert!(switch.initial_closed);
}

#[test]
fn compiling_twice_gives_the_same_fingerprint() {
    let doc = reversing_circuit();

    assert_eq!(
        compile_ok(&doc).fingerprint(),
        compile_ok(&doc).fingerprint()
    );
}

#[test]
fn geometry_and_project_name_do_not_change_the_fingerprint() {
    let doc = command_circuit();
    let mut moved = doc.clone();
    moved.project.name = "renamed".to_owned();
    for symbol in &mut moved.symbols {
        symbol.position = GridPoint::new(-symbol.position.x, 99);
        symbol.rotation = QuarterTurn::Deg270;
    }
    for wire in &mut moved.wires {
        wire.route.push(GridPoint::new(1, 2));
    }

    assert_eq!(
        compile_ok(&doc).fingerprint(),
        compile_ok(&moved).fingerprint()
    );
}

#[test]
fn renaming_a_device_changes_the_fingerprint() {
    let doc = command_circuit();
    let mut renamed = doc.clone();
    renamed
        .devices
        .iter_mut()
        .find(|device| device.reference.0 == "K1")
        .unwrap()
        .reference
        .0 = "K7".to_owned();

    assert_ne!(
        compile_ok(&doc).fingerprint(),
        compile_ok(&renamed).fingerprint()
    );
}

#[test]
fn rewiring_changes_the_fingerprint() {
    let (rig, ..) = lamp_circuit();
    let doc = rig.finish();
    let mut rewired = doc.clone();
    let wire = rewired.wires.last_mut().unwrap();
    std::mem::swap(&mut wire.from, &mut wire.to);
    let first = rewired.wires.first_mut().unwrap();
    first.to = pin(symbol_of(&doc, "H1", "lamp"), "x2");
    let last = rewired.wires.last_mut().unwrap();
    last.to = pin(symbol_of(&doc, "H1", "lamp"), "x1");

    assert_ne!(
        compile_ok(&doc).fingerprint(),
        compile_ok(&rewired).fingerprint()
    );
}

#[test]
fn rated_voltage_reaches_the_load_rule() {
    let mut rig = Rig::new();
    let supply = rig.three_phase_supply(PhaseSequence::L1L2L3);
    let line = rig.tap("L1", "l1", supply);
    let neutral = rig.tap("N", "n", supply);
    let lamp = rig.part("H1", "lamp.generic", "lamp", &load(230_000, "line_neutral"));
    rig.wire(out(line), pin(lamp, "x1"));
    rig.wire(pin(lamp, "x2"), out(neutral));

    let circuit = compile_ok(&rig.finish());

    assert_eq!(
        circuit.loads()[0].rule,
        LoadRule::TwoTerminal {
            connection: Connection::LineNeutral,
            rated_voltage_mv: 230_000
        }
    );
}

fn reversed(doc: &Schematic) -> Schematic {
    let mut reversed = doc.clone();
    reversed.supplies.reverse();
    reversed.devices.reverse();
    reversed.symbols.reverse();
    reversed.junctions.reverse();
    reversed.wires.reverse();
    reversed
}

/// A document with several independent problems, for order-invariance of
/// diagnostics.
fn broken_circuit() -> Schematic {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let k1 = rig.device("K1", "contactor.generic", &[]);
    let coil = rig.symbol(k1, "coil", None);
    rig.symbol(k1, "aux_no", None);
    rig.symbol(k1, "aux_no", Some(3));
    rig.symbol(k1, "aux_no", Some(3));
    rig.part("X1", "nothing", "contact", &[]);
    let lamp = rig.part("H1", "lamp.generic", "lamp", &dc_load());
    rig.wire(out(p24), pin(coil, "zz"));
    rig.wire(out(p24), pin(lamp, "x1"));
    rig.wire(pin(lamp, "x2"), out(p0));
    rig.finish()
}

#[test]
fn compilation_does_not_depend_on_collection_order() {
    for doc in [reversing_circuit(), direct_online_circuit()] {
        assert_eq!(compile_doc(&reversed(&doc)), compile_doc(&doc));
    }
}

#[test]
fn diagnostics_do_not_depend_on_collection_order() {
    let doc = broken_circuit();
    let output = compile_doc(&doc);

    assert!(output.diagnostics.len() >= 5, "{:#?}", output.diagnostics);
    assert_eq!(compile_doc(&reversed(&doc)), output);
}

fn shuffled(doc: Schematic) -> impl Strategy<Value = Schematic> {
    (
        Just(doc.supplies.clone()).prop_shuffle(),
        Just(doc.devices.clone()).prop_shuffle(),
        Just(doc.symbols.clone()).prop_shuffle(),
        Just(doc.junctions.clone()).prop_shuffle(),
        Just(doc.wires.clone()).prop_shuffle(),
    )
        .prop_map(
            move |(supplies, devices, symbols, junctions, wires)| Schematic {
                supplies,
                devices,
                symbols,
                junctions,
                wires,
                ..doc.clone()
            },
        )
}

/// The static nets of `circuit-model` (ADR-0014) number every pin, junction
/// and wire exactly as the compiler does. The renderer and the reports rely on
/// it, and the compiler is not allowed to change without this test saying so.
fn assert_static_nets_agree(doc: &Schematic) {
    let Some(circuit) = compile_doc(doc).circuit else {
        return;
    };
    let nets = StaticNets::build(doc, &ComponentCatalog::builtin());

    for compiled in circuit.pins() {
        assert_eq!(
            nets.net_of_pin(&compiled.pin).map(|net| net.0),
            Some(compiled.net.0),
            "pin {:?}",
            compiled.pin
        );
    }
    for (junction, net) in circuit.junctions() {
        assert_eq!(
            nets.net_of_junction(*junction).map(|net| net.0),
            Some(net.0)
        );
    }
    for (wire, net) in circuit.wires() {
        assert_eq!(nets.net_of_wire(*wire).map(|net| net.0), Some(net.0));
    }
    let pins: usize = nets.nets().iter().map(|net| net.pins.len()).sum();
    assert_eq!(pins, circuit.pins().len());
}

#[test]
fn static_nets_number_the_example_circuits_as_the_compiler_does() {
    for doc in [
        command_circuit(),
        direct_online_circuit(),
        reversing_circuit(),
        reversing_with(Interlocks {
            electrical: false,
            mechanical: true,
        }),
        two_sections("l1"),
        transformer_circuit(Secondary::Own),
    ] {
        assert!(compile_doc(&doc).circuit.is_some());
        assert_static_nets_agree(&doc);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn static_nets_follow_the_compiler_when_wires_are_added_at_random(
        extra in prop::collection::vec((any::<prop::sample::Index>(), any::<prop::sample::Index>()), 0..6)
    ) {
        let mut doc = direct_online_circuit();
        let nets = StaticNets::build(&doc, &ComponentCatalog::builtin());
        let mut ends: Vec<Endpoint> = nets
            .nets()
            .iter()
            .flat_map(|net| net.pins.iter().cloned().map(Endpoint::Terminal))
            .collect();
        ends.extend(doc.junctions.iter().map(|junction| Endpoint::Junction(junction.id)));
        prop_assume!(!ends.is_empty());
        for (a, b) in extra {
            let id = WireId(doc.next_id);
            doc.next_id += 1;
            doc.wires.push(Wire {
                id,
                from: a.get(&ends).clone(),
                to: b.get(&ends).clone(),
                route: Vec::new(),
                number: None,
            });
        }

        // A document that does not compile proves nothing; too many of them
        // would fail the run instead of passing it vacuously.
        prop_assume!(compile_doc(&doc).circuit.is_some());
        assert_static_nets_agree(&doc);
    }
}

/// Two contactors and a relay with their coils wired, and interlocks between
/// the named devices.
fn interlock_rig(links: &[(&str, &str)]) -> Schematic {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let mut ids = BTreeMap::new();
    for (reference, model) in [
        ("K1", "contactor.generic"),
        ("K2", "contactor.generic"),
        ("K3", "contactor.generic"),
        ("KA1", "relay.generic"),
    ] {
        let device = rig.device(reference, model, &dc_load());
        let coil = rig.symbol(device, "coil", None);
        rig.wire(out(p24), pin(coil, "a1"));
        rig.wire(pin(coil, "a2"), out(p0));
        ids.insert(reference, i64::try_from(device.0).unwrap());
    }
    for (index, (first, second)) in links.iter().enumerate() {
        let id = |reference: &str| ids.get(reference).copied().unwrap_or(9_999);
        let link = rig.device(
            &format!("KX{}", index + 1),
            "interlock.mechanical",
            &[
                ("first", ParamValue::Int(id(first))),
                ("second", ParamValue::Int(id(second))),
            ],
        );
        rig.symbol(link, "link", None);
    }
    rig.finish()
}

#[test]
fn interlock_between_two_contactors_compiles_into_one_sorted_pair() {
    let doc = interlock_rig(&[("K2", "K1")]);
    let circuit = compile_ok(&doc);
    let index = |reference: &str| {
        circuit
            .device_index(circuit.device_by_reference(reference).unwrap().id)
            .unwrap()
    };

    assert_eq!(circuit.interlocks(), [(index("K1"), index("K2"))]);
}

#[test]
fn interlock_that_does_not_name_two_contactors_is_an_error() {
    for links in [
        &[("K1", "K1")][..],
        &[("K1", "KA1")],
        &[("K1", "missing")],
        &[("K1", "K2"), ("K2", "K3")],
    ] {
        let output = compile_doc(&interlock_rig(links));

        assert!(output.circuit.is_none(), "{links:?}");
        assert!(
            codes(&output).contains(&DiagnosticCode::InvalidInterlock),
            "{links:?}: {:?}",
            output.diagnostics
        );
    }
}

#[test]
fn interlocks_change_the_fingerprint_and_circuits_without_them_keep_theirs() {
    let plain = compile_ok(&reversing_with(Interlocks {
        electrical: true,
        mechanical: false,
    }));
    let linked = compile_ok(&reversing_with(Interlocks {
        electrical: true,
        mechanical: true,
    }));

    assert_eq!(
        plain.fingerprint(),
        compile_ok(&reversing_circuit()).fingerprint()
    );
    assert_ne!(plain.fingerprint(), linked.fingerprint());
}

#[test]
fn transformer_secondary_must_be_a_single_phase_section_of_its_own() {
    for secondary in [Secondary::ThreePhase, Secondary::Tapped, Secondary::Shared] {
        let output = compile_doc(&transformer_circuit(secondary));

        assert!(codes(&output).contains(&DiagnosticCode::InvalidTransformer));
        assert_eq!(output.circuit, None);
    }
}

#[test]
fn transformer_secondary_compiles_into_sources_enabled_by_powered() {
    let circuit = compile_ok(&transformer_circuit(Secondary::Own));
    let conditional: Vec<(Conductor, Option<SigIx>)> = circuit
        .sources()
        .iter()
        .filter(|source| source.enabled_by.is_some())
        .map(|source| (source.conductor, source.enabled_by))
        .collect();
    let powered = circuit
        .signals()
        .iter()
        .position(|signal| signal.kind == SignalKind::Powered)
        .map(|index| SigIx(u32::try_from(index).unwrap()));

    assert_eq!(
        conditional,
        [(Conductor::L1, powered), (Conductor::N, powered)]
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn any_permutation_of_a_valid_document_compiles_identically(
        doc in shuffled(reversing_circuit())
    ) {
        prop_assert_eq!(compile_doc(&doc), compile_doc(&reversing_circuit()));
    }

    #[test]
    fn any_permutation_of_a_multi_section_document_compiles_identically(
        doc in shuffled(two_sections("l1"))
    ) {
        prop_assert_eq!(compile_doc(&doc), compile_doc(&two_sections("l1")));
    }

    #[test]
    fn any_permutation_of_a_transformer_document_compiles_identically(
        doc in shuffled(transformer_circuit(Secondary::Own))
    ) {
        prop_assert_eq!(compile_doc(&doc), compile_doc(&transformer_circuit(Secondary::Own)));
    }

    #[test]
    fn any_permutation_of_a_broken_document_reports_identically(
        doc in shuffled(broken_circuit())
    ) {
        prop_assert_eq!(compile_doc(&doc), compile_doc(&broken_circuit()));
    }
}

mod hostile {
    //! Documents a hostile or broken file could produce: duplicate and zero
    //! IDs, unknown models and keys, parameters of any type and size,
    //! dangling endpoints, extreme coordinates. Compiling and simulating
    //! them must never panic.

    use circuit_model::{
        Device, DeviceId, Endpoint, Junction, JunctionId, ModelRef, PinRef, ProjectMeta,
        ReferenceDesignator, Schematic, SupplyId, SupplyKind, SupplyProfile, SymbolId,
        SymbolInstance, SymbolKey, TerminalKey, Wire, WireId,
    };

    use super::*;
    use crate::runtime::{SimConfig, Simulator};

    fn id() -> impl Strategy<Value = u64> {
        prop_oneof![8 => 0_u64..24, 1 => Just(u64::MAX)]
    }

    fn coordinate() -> impl Strategy<Value = i32> {
        prop_oneof![8 => -40_i32..40, 1 => Just(i32::MIN), 1 => Just(i32::MAX)]
    }

    fn text(choices: &'static [&'static str]) -> impl Strategy<Value = String> {
        prop::sample::select(choices).prop_map(str::to_owned)
    }

    fn param() -> impl Strategy<Value = (String, ParamValue)> {
        let key = text(&[
            "conductor",
            "supply",
            "rated_voltage_mv",
            "connection",
            "preset_ms",
            "initial_closed",
            "unknown",
        ]);
        let value = prop_oneof![
            any::<bool>().prop_map(ParamValue::Bool),
            prop_oneof![
                Just(0_i64),
                Just(1),
                Just(24_000),
                Just(-1),
                Just(i64::MAX),
                Just(i64::MIN)
            ]
            .prop_map(ParamValue::Int),
            text(&[
                "dc_pos",
                "dc_neg",
                "l1",
                "l2",
                "l3",
                "n",
                "pe",
                "dc",
                "line_line",
                "x"
            ])
            .prop_map(ParamValue::Text),
        ];
        (key, value)
    }

    fn supply() -> impl Strategy<Value = SupplyProfile> {
        (
            id(),
            prop::sample::select(vec![
                SupplyKind::Dc,
                SupplyKind::SinglePhase,
                SupplyKind::ThreePhase,
            ]),
            prop_oneof![Just(0_u32), Just(24_000), Just(400_000), Just(u32::MAX)],
            prop::option::of(Just(230_000_u32)),
            prop::option::of(Just(50_000_u32)),
            prop::option::of(Just(PhaseSequence::L1L2L3)),
        )
            .prop_map(
                |(id, kind, nominal, phase, frequency, sequence)| SupplyProfile {
                    id: SupplyId(id),
                    kind,
                    nominal_voltage_mv: nominal,
                    phase_voltage_mv: phase,
                    frequency_millihz: frequency,
                    phase_sequence: sequence,
                },
            )
    }

    fn device() -> impl Strategy<Value = Device> {
        let models = ComponentCatalog::builtin()
            .models()
            .iter()
            .map(|m| m.id().to_owned())
            .chain(["no.such.model".to_owned()])
            .collect::<Vec<_>>();
        (
            id(),
            text(&["K1", "K2", "S1", "P24", "P0", "L1", "bad", ""]),
            prop::sample::select(models),
            prop_oneof![Just(1_u32), Just(2)],
            prop::collection::btree_map(
                param().prop_map(|(k, _)| k),
                param().prop_map(|(_, v)| v),
                0..4,
            ),
        )
            .prop_map(|(id, reference, model, version, params)| Device {
                id: DeviceId(id),
                reference: ReferenceDesignator(reference),
                model: ModelRef { id: model, version },
                params,
                info: None,
            })
    }

    fn terminal() -> impl Strategy<Value = String> {
        text(&[
            "a1",
            "a2",
            "p1",
            "p2",
            "x1",
            "x2",
            "out",
            "u1",
            "v1",
            "w1",
            "pole1.in",
            "pole3.out",
            "zz",
        ])
    }

    fn symbol() -> impl Strategy<Value = SymbolInstance> {
        (
            id(),
            id(),
            text(&[
                "coil", "contact", "main", "aux_no", "aux_nc", "element", "terminal", "motor",
                "lamp", "link", "nope",
            ]),
            prop::option::of(0_u8..4),
            coordinate(),
            coordinate(),
            prop::sample::select(QuarterTurn::ALL.to_vec()),
        )
            .prop_map(|(id, device, key, slot, x, y, rotation)| SymbolInstance {
                id: SymbolId(id),
                device_id: DeviceId(device),
                symbol_key: SymbolKey(key),
                slot,
                position: GridPoint::new(x, y),
                rotation,
            })
    }

    fn endpoint() -> impl Strategy<Value = Endpoint> {
        prop_oneof![
            (id(), terminal()).prop_map(|(symbol, key)| Endpoint::Terminal(PinRef {
                symbol_id: SymbolId(symbol),
                terminal: TerminalKey(key),
            })),
            id().prop_map(|j| Endpoint::Junction(JunctionId(j))),
        ]
    }

    prop_compose! {
        fn hostile_document()(
            next_id in prop_oneof![Just(0_u64), 1_u64..32, Just(u64::MAX)],
            supplies in prop::collection::vec(supply(), 0..3),
            devices in prop::collection::vec(device(), 0..8),
            symbols in prop::collection::vec(symbol(), 0..10),
            junctions in prop::collection::vec((id(), coordinate(), coordinate()), 0..4),
            wires in prop::collection::vec(
                (id(), endpoint(), endpoint(), prop::collection::vec((coordinate(), coordinate()), 0..3)),
                0..10,
            ),
        ) -> Schematic {
            Schematic {
                schema_version: 1,
                next_id,
                project: ProjectMeta {
                    name: "hostile".to_owned(),
                    grid_unit: 10,
                    author: None,
                    company: None,
                    revision: None,
                    date: None, wire_numbering: None,
                },
                sheets: Vec::new(),
                supplies,
                devices,
                symbols,
                junctions: junctions
                    .into_iter()
                    .map(|(id, x, y)| Junction { id: JunctionId(id), position: GridPoint::new(x, y) })
                    .collect(),
                wires: wires
                    .into_iter()
                    .map(|(id, from, to, route)| Wire {
                        id: WireId(id),
                        from,
                        to,
                        route: route.into_iter().map(|(x, y)| GridPoint::new(x, y)).collect(), number: None,
                    })
                    .collect(), waivers: Vec::new(),
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn compiling_and_simulating_any_document_never_panics(doc in hostile_document()) {
            let output = compile(&doc, &ComponentCatalog::builtin());
            if let Some(circuit) = output.circuit {
                if let Ok(mut sim) = Simulator::new(circuit, SimConfig::default()) {
                    let devices: Vec<DeviceId> = sim.circuit().devices().iter().map(|d| d.id).collect();
                    for device in devices {
                        let _ = sim.step(&[circuit_model::InputAction::Press(device)]);
                        let _ = sim.step(&[circuit_model::InputAction::Trip(device)]);
                    }
                    let _ = sim.step_n(5, &[]);
                    let _ = sim.snapshot();
                }
            }
        }

        #[test]
        fn compiling_any_document_is_deterministic(doc in hostile_document()) {
            let catalog = ComponentCatalog::builtin();

            prop_assert_eq!(compile(&doc, &catalog), compile(&doc, &catalog));
        }
    }
}

// Symbol-only devices (ADR-0013).

/// The built-in catalog plus `test.diode`, a symbol-only device with two
/// terminals.
fn symbolic_catalog() -> circuit_model::ComponentCatalog {
    use circuit_model::catalog::{ComponentModel, DeviceClass, Provenance, SymbolDefinition};
    let mut models = circuit_model::ComponentCatalog::builtin().models().to_vec();
    models.push(ComponentModel::new(
        "test.diode",
        1,
        DeviceClass::Symbolic,
        Vec::new(),
        vec![SymbolDefinition::new(
            "body",
            circuit_model::SymbolKind::Block2,
            1,
            1,
            false,
        )],
        Provenance {
            origin: "test".to_owned(),
            license: "MIT OR Apache-2.0".to_owned(),
            standard_reference: None,
        },
    ));
    circuit_model::ComponentCatalog::new(models).unwrap_or_else(|error| panic!("{error}"))
}

/// P24 → A1 (symbol-only, between `t1` and `t2`) → H1 → P0.
fn lamp_behind_a_symbolic_device() -> (Schematic, SymbolId) {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let a1 = rig.part("A1", "test.diode", "body", &[]);
    let lamp = rig.part("H1", "lamp.generic", "lamp", &dc_load());
    rig.wire(out(p24), pin(a1, "t1"));
    rig.wire(pin(a1, "t2"), pin(lamp, "x1"));
    rig.wire(pin(lamp, "x2"), out(p0));
    (rig.finish(), a1)
}

#[test]
fn a_symbolic_device_is_reported_as_not_simulated_and_blocks_nothing() {
    let (doc, _) = lamp_behind_a_symbolic_device();

    let output = compile(&doc, &symbolic_catalog());

    let reported: Vec<_> = output
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code() == DiagnosticCode::NotSimulated)
        .collect();
    assert_eq!(reported.len(), 1, "{:#?}", output.diagnostics);
    assert_eq!(reported[0].severity(), circuit_model::Severity::Info);
    assert!(matches!(reported[0].subject(), Subject::Device(_)));
    assert!(!circuit_model::has_errors(&output.diagnostics));
    assert!(output.circuit.is_some());
}

#[test]
fn a_symbolic_device_adds_no_switch_source_or_load_and_does_not_conduct() {
    let (doc, a1) = lamp_behind_a_symbolic_device();

    let circuit = compile(&doc, &symbolic_catalog())
        .circuit
        .unwrap_or_else(|| panic!("the document should compile"));

    // Only the lamp is a load and only the two supply terminals are sources.
    assert_eq!(circuit.loads().len(), 1);
    assert_eq!(circuit.sources().len(), 2);
    assert_eq!(circuit.switches().len(), 0);
    // Its two terminals are different nets: nothing joins them.
    assert_ne!(net_of(&circuit, a1, "t1"), net_of(&circuit, a1, "t2"));
}

#[test]
fn a_symbolic_device_changes_the_fingerprint_and_the_order_of_the_document_does_not() {
    let (with_it, _) = lamp_behind_a_symbolic_device();
    let catalog = symbolic_catalog();
    let (lamp_only, ..) = lamp_circuit();
    let fingerprint = |doc: &Schematic| {
        compile(doc, &catalog)
            .circuit
            .unwrap_or_else(|| panic!("the document should compile"))
            .fingerprint()
    };

    assert_ne!(fingerprint(&with_it), fingerprint(&lamp_only.finish()));
    assert_eq!(fingerprint(&reversed(&with_it)), fingerprint(&with_it));
}

#[test]
fn a_symbolic_device_between_the_poles_is_not_a_short_circuit() {
    // If the device conducted, P24 and P0 would be shorted. It has no
    // behaviour, so the circuit runs without a fault.
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let a1 = rig.part("A1", "test.diode", "body", &[]);
    rig.wire(out(p24), pin(a1, "t1"));
    rig.wire(pin(a1, "t2"), out(p0));
    let circuit = compile(&rig.finish(), &symbolic_catalog())
        .circuit
        .unwrap_or_else(|| panic!("the document should compile"));
    let mut sim = crate::Simulator::new(circuit, crate::SimConfig::default()).unwrap();

    let outcome = sim.step_n(10, &[]).unwrap();

    assert!(!outcome.faulted);
    assert!(sim.snapshot().is_some_and(|snapshot| !snapshot.faulted));
}
