use circuit_model::builder::pin;
use circuit_model::{ParamValue, PhaseSequence, Schematic, SymbolId};

use super::*;
use crate::compiled::Drive;
use crate::testkit::{Rig, compile_ok, dc_load, load, out};

/// Mask with the always-closed switches and the switches of `closed` symbols
/// closed.
fn mask(circuit: &CompiledCircuit, closed: &[SymbolId]) -> SwitchMask {
    let mut mask = SwitchMask::for_circuit(circuit);
    for (index, switch) in circuit.switches().iter().enumerate() {
        mask.set_index(
            index,
            switch.drive == Drive::Always || closed.contains(&switch.owner),
        );
    }
    mask
}

fn solve(circuit: &CompiledCircuit, closed: &[SymbolId]) -> ElectricalSolution {
    Solver::new(circuit)
        .solve(circuit, &mask(circuit, closed))
        .clone()
}

fn state_of(
    circuit: &CompiledCircuit,
    solution: &ElectricalSolution,
    reference: &str,
) -> LoadState {
    let device = circuit.device_by_reference(reference).unwrap().id;
    let index = circuit
        .loads()
        .iter()
        .position(|load| circuit.device(load.device).unwrap().id == device)
        .unwrap();
    solution.loads()[index]
}

fn fault_codes(solution: &ElectricalSolution) -> Vec<DiagnosticCode> {
    solution.faults().iter().map(|fault| fault.code()).collect()
}

/// `P24 -> S1 -> H1 -> P0` with a lamp of the given parameters.
fn switched_lamp(params: &[(&str, ParamValue)]) -> (Schematic, SymbolId) {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let button = rig.part("S1", "pushbutton_no", "contact", &[]);
    let lamp = rig.part("H1", "lamp.generic", "lamp", params);
    rig.wire(out(p24), pin(button, "p1"));
    rig.wire(pin(button, "p2"), pin(lamp, "x1"));
    rig.wire(pin(lamp, "x2"), out(p0));
    (rig.finish(), button)
}

/// Lamp across two taps of a three-phase supply with neutral.
fn ac_lamp(first: &str, second: &str, rated_mv: i64, connection: &str) -> CompiledCircuit {
    let mut rig = Rig::new();
    let supply = rig.three_phase_supply(PhaseSequence::L1L2L3);
    let a = rig.tap("XA", first, supply);
    let b = rig.tap("XB", second, supply);
    let lamp = rig.part("H1", "lamp.generic", "lamp", &load(rated_mv, connection));
    rig.wire(out(a), pin(lamp, "x1"));
    rig.wire(pin(lamp, "x2"), out(b));
    compile_ok(&rig.finish())
}

/// Motor fed through contactor K1. `order` says which phase reaches `u1`,
/// `v1` and `w1`; `wired_poles` allows leaving a phase unconnected.
fn motor_circuit(
    order: [usize; 3],
    sequence: PhaseSequence,
    rated_mv: i64,
    wired_poles: usize,
) -> (CompiledCircuit, SymbolId) {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let supply = rig.three_phase_supply(sequence);
    let phases = [
        rig.tap("L1", "l1", supply),
        rig.tap("L2", "l2", supply),
        rig.tap("L3", "l3", supply),
    ];
    let k1 = rig.device("K1", "contactor.generic", &dc_load());
    let coil = rig.symbol(k1, "coil", None);
    let main = rig.symbol(k1, "main", None);
    let motor = rig.part(
        "M1",
        "motor.3ph",
        "motor",
        &[("rated_voltage_mv", ParamValue::Int(rated_mv))],
    );
    rig.wire(out(p24), pin(coil, "a1"));
    rig.wire(pin(coil, "a2"), out(p0));
    for (pole, terminal) in ["u1", "v1", "w1"].into_iter().enumerate().take(wired_poles) {
        let key = pole + 1;
        rig.wire(
            out(phases[order[pole]]),
            pin(main, &format!("pole{key}.in")),
        );
        rig.wire(pin(main, &format!("pole{key}.out")), pin(motor, terminal));
    }
    (compile_ok(&rig.finish()), main)
}

#[test]
fn load_is_energized_only_through_a_closed_contact() {
    let (doc, button) = switched_lamp(&dc_load());
    let circuit = compile_ok(&doc);

    let open = solve(&circuit, &[]);
    let closed = solve(&circuit, &[button]);

    assert_eq!(state_of(&circuit, &open, "H1"), LoadState::Off);
    assert_eq!(open.faults(), []);
    assert_eq!(state_of(&circuit, &closed, "H1"), LoadState::On);
    assert_eq!(closed.faults(), []);
}

#[test]
fn closing_a_contact_extends_the_conductor_to_the_far_net() {
    let (doc, button) = switched_lamp(&dc_load());
    let circuit = compile_ok(&doc);
    let lamp_side = circuit
        .pins()
        .iter()
        .find(|pin| pin.pin.symbol_id == button && pin.pin.terminal.0 == "p2")
        .unwrap()
        .net;

    assert!(solve(&circuit, &[]).conductors(lamp_side).is_empty());
    assert_eq!(
        solve(&circuit, &[button]).conductors(lamp_side),
        ConductorSet::of(Conductor::DcPos)
    );
}

#[test]
fn load_with_a_different_voltage_label_stays_off_and_warns() {
    let (doc, button) = switched_lamp(&load(12_000, "dc"));
    let circuit = compile_ok(&doc);

    let solution = solve(&circuit, &[button]);

    assert_eq!(state_of(&circuit, &solution, "H1"), LoadState::Off);
    assert_eq!(fault_codes(&solution), [DiagnosticCode::LoadSupplyMismatch]);
}

#[test]
fn load_across_conductors_of_the_wrong_kind_stays_off_and_warns() {
    let circuit = ac_lamp("l1", "n", 230_000, "dc");

    let solution = solve(&circuit, &[]);

    assert_eq!(state_of(&circuit, &solution, "H1"), LoadState::Off);
    assert_eq!(fault_codes(&solution), [DiagnosticCode::LoadSupplyMismatch]);
}

#[test]
fn line_to_neutral_load_is_checked_against_the_phase_voltage() {
    let matching = ac_lamp("l2", "n", 230_000, "line_neutral");
    let rated_for_line_voltage = ac_lamp("l2", "n", 400_000, "line_neutral");

    assert_eq!(
        state_of(&matching, &solve(&matching, &[]), "H1"),
        LoadState::On
    );
    assert_eq!(
        fault_codes(&solve(&rated_for_line_voltage, &[])),
        [DiagnosticCode::LoadSupplyMismatch]
    );
}

#[test]
fn line_to_line_load_is_checked_against_the_line_voltage() {
    let circuit = ac_lamp("l1", "l3", 400_000, "line_line");

    assert_eq!(
        state_of(&circuit, &solve(&circuit, &[]), "H1"),
        LoadState::On
    );
}

#[test]
fn line_to_line_load_needs_two_different_phases() {
    let circuit = ac_lamp("l1", "n", 400_000, "line_line");

    let solution = solve(&circuit, &[]);

    assert_eq!(state_of(&circuit, &solution, "H1"), LoadState::Off);
    assert_eq!(fault_codes(&solution), [DiagnosticCode::LoadSupplyMismatch]);
}

#[test]
fn single_phase_load_is_checked_against_the_nominal_voltage() {
    let mut rig = Rig::new();
    let supply = rig.single_phase_supply(230_000);
    let line = rig.tap("L", "l1", supply);
    let neutral = rig.tap("N", "n", supply);
    let lamp = rig.part("H1", "lamp.generic", "lamp", &load(230_000, "line_neutral"));
    rig.wire(out(line), pin(lamp, "x1"));
    rig.wire(pin(lamp, "x2"), out(neutral));
    let circuit = compile_ok(&rig.finish());

    assert_eq!(
        state_of(&circuit, &solve(&circuit, &[]), "H1"),
        LoadState::On
    );
}

#[test]
fn load_bypassed_by_a_closed_contact_is_reported_as_shorted() {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let lamp = rig.part("H1", "lamp.generic", "lamp", &dc_load());
    let relay = rig.part("K1", "relay.generic", "coil", &dc_load());
    let bypass = rig.part("S1", "pushbutton_no", "contact", &[]);
    let feed = rig.junction();
    let node = rig.junction();
    // P24 -> lamp -> node, and P24 -> bypass -> node; node -> S2 -> relay.
    let series_switch = rig.part("S2", "pushbutton_no", "contact", &[]);
    rig.wire(out(p24), feed);
    rig.wire(feed, pin(lamp, "x1"));
    rig.wire(feed, pin(bypass, "p1"));
    rig.wire(pin(lamp, "x2"), node);
    rig.wire(pin(bypass, "p2"), node);
    rig.wire(node, pin(series_switch, "p1"));
    rig.wire(pin(series_switch, "p2"), pin(relay, "a1"));
    rig.wire(pin(relay, "a2"), out(p0));
    let circuit = compile_ok(&rig.finish());

    let solution = solve(&circuit, &[bypass, series_switch]);

    assert_eq!(state_of(&circuit, &solution, "H1"), LoadState::Off);
    assert_eq!(state_of(&circuit, &solution, "K1"), LoadState::On);
    assert_eq!(
        fault_codes(&solution),
        [DiagnosticCode::LoadTerminalsShorted]
    );
}

/// Three-phase supply with neutral and earth; a button joins `first` and
/// `second`; one lamp sits across L1-L3 and another across L3-N.
fn conflict_circuit(first: &str, second: &str) -> (CompiledCircuit, SymbolId) {
    let mut rig = Rig::new();
    let supply = rig.three_phase_supply(PhaseSequence::L1L2L3);
    let taps: Vec<(&str, SymbolId)> = ["l1", "l2", "l3", "n", "pe"]
        .into_iter()
        .map(|conductor| {
            (
                conductor,
                rig.tap(&conductor.to_uppercase(), conductor, supply),
            )
        })
        .collect();
    let nodes: Vec<(&str, _)> = taps
        .iter()
        .map(|(key, symbol)| {
            let node = rig.junction();
            rig.wire(out(*symbol), node);
            (*key, node)
        })
        .collect();
    let node = |key: &str| nodes.iter().find(|(k, _)| *k == key).unwrap().1;

    let button = rig.part("S1", "pushbutton_no", "contact", &[]);
    rig.wire(node(first), pin(button, "p1"));
    rig.wire(pin(button, "p2"), node(second));
    let line_lamp = rig.part("H1", "lamp.generic", "lamp", &load(400_000, "line_line"));
    rig.wire(node("l1"), pin(line_lamp, "x1"));
    rig.wire(pin(line_lamp, "x2"), node("l3"));
    let phase_lamp = rig.part("H2", "lamp.generic", "lamp", &load(230_000, "line_neutral"));
    rig.wire(node("l3"), pin(phase_lamp, "x1"));
    rig.wire(pin(phase_lamp, "x2"), node("n"));
    (compile_ok(&rig.finish()), button)
}

#[test]
fn two_phases_joined_by_a_contact_are_a_short_circuit() {
    let (circuit, button) = conflict_circuit("l1", "l2");

    let solution = solve(&circuit, &[button]);

    assert_eq!(fault_codes(&solution), [DiagnosticCode::ShortCircuit]);
    let fault = &solution.faults()[0];
    let l1 = circuit.device_by_reference("L1").unwrap().id;
    let l2 = circuit.device_by_reference("L2").unwrap().id;
    assert_eq!(fault.subject(), &Subject::Device(l1));
    assert_eq!(fault.related(), [Subject::Device(l2)]);
}

#[test]
fn group_in_fault_energizes_nothing_and_the_rest_keeps_working() {
    let (circuit, button) = conflict_circuit("l1", "l2");

    let healthy = solve(&circuit, &[]);
    let shorted = solve(&circuit, &[button]);

    assert_eq!(state_of(&circuit, &healthy, "H1"), LoadState::On);
    assert_eq!(state_of(&circuit, &healthy, "H2"), LoadState::On);
    assert_eq!(state_of(&circuit, &shorted, "H1"), LoadState::Off);
    assert_eq!(state_of(&circuit, &shorted, "H2"), LoadState::On);
}

#[test]
fn phase_joined_to_neutral_is_a_short_circuit() {
    let (circuit, button) = conflict_circuit("l2", "n");

    assert_eq!(
        fault_codes(&solve(&circuit, &[button])),
        [DiagnosticCode::ShortCircuit]
    );
}

#[test]
fn phase_joined_to_earth_is_an_earth_fault() {
    let (circuit, button) = conflict_circuit("l2", "pe");

    assert_eq!(
        fault_codes(&solve(&circuit, &[button])),
        [DiagnosticCode::ProtectiveEarthFault]
    );
}

#[test]
fn neutral_joined_to_earth_is_a_warning_and_loads_keep_working() {
    let (circuit, button) = conflict_circuit("n", "pe");

    let solution = solve(&circuit, &[button]);

    assert_eq!(fault_codes(&solution), [DiagnosticCode::NeutralEarthBond]);
    assert_eq!(state_of(&circuit, &solution, "H2"), LoadState::On);
}

#[test]
fn dc_pole_joined_to_a_phase_is_a_short_circuit() {
    let mut rig = Rig::new();
    let (p24, _) = rig.dc24();
    let supply = rig.three_phase_supply(PhaseSequence::L1L2L3);
    let line = rig.tap("L1", "l1", supply);
    let button = rig.part("S1", "pushbutton_no", "contact", &[]);
    rig.wire(out(p24), pin(button, "p1"));
    rig.wire(pin(button, "p2"), out(line));
    let circuit = compile_ok(&rig.finish());

    assert_eq!(
        fault_codes(&solve(&circuit, &[button])),
        [DiagnosticCode::ShortCircuit]
    );
}

#[test]
fn motor_runs_forward_when_its_terminals_follow_the_phase_sequence() {
    let (circuit, main) = motor_circuit([0, 1, 2], PhaseSequence::L1L2L3, 400_000, 3);

    let solution = solve(&circuit, &[main]);

    assert_eq!(
        state_of(&circuit, &solution, "M1"),
        LoadState::Rotating(Direction::Forward)
    );
    assert_eq!(solution.faults(), []);
}

#[test]
fn motor_runs_forward_for_every_rotation_of_the_phase_sequence() {
    for order in [[1, 2, 0], [2, 0, 1]] {
        let (circuit, main) = motor_circuit(order, PhaseSequence::L1L2L3, 400_000, 3);

        assert_eq!(
            state_of(&circuit, &solve(&circuit, &[main]), "M1"),
            LoadState::Rotating(Direction::Forward)
        );
    }
}

#[test]
fn motor_runs_in_reverse_when_two_phases_are_swapped() {
    for order in [[2, 1, 0], [1, 0, 2], [0, 2, 1]] {
        let (circuit, main) = motor_circuit(order, PhaseSequence::L1L2L3, 400_000, 3);

        assert_eq!(
            state_of(&circuit, &solve(&circuit, &[main]), "M1"),
            LoadState::Rotating(Direction::Reverse)
        );
    }
}

#[test]
fn supply_phase_sequence_decides_what_forward_means() {
    let (circuit, main) = motor_circuit([0, 1, 2], PhaseSequence::L1L3L2, 400_000, 3);

    assert_eq!(
        state_of(&circuit, &solve(&circuit, &[main]), "M1"),
        LoadState::Rotating(Direction::Reverse)
    );
}

#[test]
fn motor_with_open_contactor_is_stopped_without_any_diagnostic() {
    let (circuit, _) = motor_circuit([0, 1, 2], PhaseSequence::L1L2L3, 400_000, 3);

    let solution = solve(&circuit, &[]);

    assert_eq!(state_of(&circuit, &solution, "M1"), LoadState::Off);
    assert_eq!(solution.faults(), []);
}

#[test]
fn motor_fed_by_two_phases_reports_phase_loss() {
    let (circuit, main) = motor_circuit([0, 1, 2], PhaseSequence::L1L2L3, 400_000, 2);

    let solution = solve(&circuit, &[main]);

    assert_eq!(state_of(&circuit, &solution, "M1"), LoadState::Off);
    assert_eq!(fault_codes(&solution), [DiagnosticCode::PhaseLoss]);
}

#[test]
fn motor_with_a_different_voltage_label_stays_off_and_warns() {
    let (circuit, main) = motor_circuit([0, 1, 2], PhaseSequence::L1L2L3, 230_000, 3);

    let solution = solve(&circuit, &[main]);

    assert_eq!(state_of(&circuit, &solution, "M1"), LoadState::Off);
    assert_eq!(fault_codes(&solution), [DiagnosticCode::LoadSupplyMismatch]);
}

#[test]
fn unchanged_switch_mask_reuses_the_previous_solution() {
    let (doc, button) = switched_lamp(&dc_load());
    let circuit = compile_ok(&doc);
    let mut solver = Solver::new(&circuit);
    let open = mask(&circuit, &[]);
    let closed = mask(&circuit, &[button]);

    solver.solve(&circuit, &open);
    solver.solve(&circuit, &open);
    assert_eq!(solver.solve_count(), 1);

    solver.solve(&circuit, &closed);
    solver.solve(&circuit, &closed);
    assert_eq!(solver.solve_count(), 2);

    let reopened = solver.solve(&circuit, &open).clone();
    assert_eq!(solver.solve_count(), 3);
    assert_eq!(reopened, solve(&circuit, &[]));
}

/// Two lamps in series with a contact from the middle node to `P0`, so the
/// middle net has a switch terminal and passes the static check.
fn series_circuit() -> (CompiledCircuit, SymbolId) {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let upper = rig.part("H1", "lamp.generic", "lamp", &dc_load());
    let lower = rig.part("H2", "lamp.generic", "lamp", &dc_load());
    let bypass = rig.part("S1", "pushbutton_no", "contact", &[]);
    let middle = rig.junction();
    let common = rig.junction();
    rig.wire(out(p24), pin(upper, "x1"));
    rig.wire(pin(upper, "x2"), middle);
    rig.wire(middle, pin(lower, "x1"));
    rig.wire(middle, pin(bypass, "p1"));
    rig.wire(pin(lower, "x2"), common);
    rig.wire(pin(bypass, "p2"), common);
    rig.wire(common, out(p0));
    (compile_ok(&rig.finish()), bypass)
}

#[test]
fn loads_in_series_at_run_time_are_rejected_not_approximated() {
    let (circuit, _) = series_circuit();

    let solution = solve(&circuit, &[]);

    assert_eq!(state_of(&circuit, &solution, "H1"), LoadState::Off);
    assert_eq!(state_of(&circuit, &solution, "H2"), LoadState::Off);
    assert_eq!(
        fault_codes(&solution),
        [DiagnosticCode::DynamicUnsupportedTopology]
    );
    let h1 = circuit.device_by_reference("H1").unwrap().id;
    let h2 = circuit.device_by_reference("H2").unwrap().id;
    assert_eq!(solution.faults()[0].subject(), &Subject::Device(h1));
    assert_eq!(solution.faults()[0].related(), [Subject::Device(h2)]);
}

#[test]
fn closing_the_bypass_removes_the_series_condition() {
    let (circuit, bypass) = series_circuit();

    let solution = solve(&circuit, &[bypass]);

    assert_eq!(state_of(&circuit, &solution, "H1"), LoadState::On);
    assert_eq!(state_of(&circuit, &solution, "H2"), LoadState::Off);
    assert_eq!(
        fault_codes(&solution),
        [DiagnosticCode::LoadTerminalsShorted]
    );
}

#[test]
fn parallel_loads_behind_an_open_contact_are_not_a_series_fault() {
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
    let circuit = compile_ok(&rig.finish());

    let open = solve(&circuit, &[]);
    let closed = solve(&circuit, &[button]);

    assert_eq!(open.faults(), []);
    assert_eq!(state_of(&circuit, &closed, "K1"), LoadState::On);
    assert_eq!(state_of(&circuit, &closed, "H1"), LoadState::On);
}

#[test]
fn three_loads_in_series_are_detected_across_two_dead_nodes() {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let lamps: Vec<SymbolId> = ["H1", "H2", "H3"]
        .into_iter()
        .map(|reference| rig.part(reference, "lamp.generic", "lamp", &dc_load()))
        .collect();
    // Each intermediate node also carries an unused contact terminal so the
    // document passes the static series check.
    let first_tap = rig.part("S1", "pushbutton_no", "contact", &[]);
    let second_tap = rig.part("S2", "pushbutton_no", "contact", &[]);
    let first = rig.junction();
    let second = rig.junction();
    rig.wire(out(p24), pin(lamps[0], "x1"));
    rig.wire(pin(lamps[0], "x2"), first);
    rig.wire(first, pin(lamps[1], "x1"));
    rig.wire(first, pin(first_tap, "p1"));
    rig.wire(pin(lamps[1], "x2"), second);
    rig.wire(second, pin(lamps[2], "x1"));
    rig.wire(second, pin(second_tap, "p1"));
    rig.wire(pin(lamps[2], "x2"), out(p0));
    let circuit = compile_ok(&rig.finish());

    let solution = solve(&circuit, &[]);

    assert_eq!(
        fault_codes(&solution),
        [DiagnosticCode::DynamicUnsupportedTopology]
    );
    assert_eq!(solution.faults()[0].related().len(), 2);
}
