use circuit_model::builder::pin;
use circuit_model::{GridPoint, ParamValue, QuarterTurn};
use proptest::prelude::*;

use super::*;
use crate::testkit::{
    Rig, codes, command_circuit, compile_doc, compile_ok, dc_load, direct_online_circuit, load,
    out, reversing_circuit,
};

fn net_of(circuit: &CompiledCircuit, symbol: SymbolId, terminal: &str) -> NetIx {
    circuit
        .pins()
        .iter()
        .find(|pin| pin.pin.symbol_id == symbol && pin.pin.terminal.0 == terminal)
        .unwrap_or_else(|| panic!("no pin {symbol}:{terminal}"))
        .net
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

    assert_eq!(output.diagnostics, []);
    let circuit = output.circuit.unwrap();
    assert_ne!(net_of(&circuit, lamp, "x1"), net_of(&circuit, lamp, "x2"));
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
fn one_conductor_from_two_supplies_is_rejected() {
    let mut rig = Rig::new();
    let first = rig.dc_supply(24_000);
    let second = rig.dc_supply(12_000);
    rig.tap("P24", "dc_pos", first);
    rig.tap("P12", "dc_pos", second);

    let output = compile_doc(&rig.finish());

    assert!(codes(&output).contains(&DiagnosticCode::ConflictingSupplies));
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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn any_permutation_of_a_valid_document_compiles_identically(
        doc in shuffled(reversing_circuit())
    ) {
        prop_assert_eq!(compile_doc(&doc), compile_doc(&reversing_circuit()));
    }

    #[test]
    fn any_permutation_of_a_broken_document_reports_identically(
        doc in shuffled(broken_circuit())
    ) {
        prop_assert_eq!(compile_doc(&doc), compile_doc(&broken_circuit()));
    }
}
