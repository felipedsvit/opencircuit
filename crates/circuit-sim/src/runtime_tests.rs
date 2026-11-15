use circuit_model::builder::pin;
use circuit_model::{
    Conductor, ConductorSet, InputAction, MotorState, MotorView, ParamValue, Schematic, SymbolId,
    WindingConnection,
};
use proptest::prelude::*;

use super::*;
use crate::testkit::{
    Interlocks, Rig, Secondary, action, codes, command_circuit, compile_doc, compile_ok, dc_load,
    direct_online_circuit, load, out, reversing_circuit, reversing_with, run, simulator,
    transformer_circuit, two_sections,
};
use crate::trace::format_trace;

fn default_sim(doc: &Schematic) -> Simulator {
    simulator(doc, SimConfig::default())
}

fn report_sim(doc: &Schematic) -> Simulator {
    simulator(
        doc,
        SimConfig {
            fault_policy: FaultPolicy::Report,
            ..SimConfig::default()
        },
    )
}

fn id(sim: &Simulator, reference: &str) -> DeviceId {
    sim.circuit().device_by_reference(reference).unwrap().id
}

fn symbol_of(doc: &Schematic, reference: &str, key: &str) -> SymbolId {
    let device = doc
        .devices
        .iter()
        .find(|device| device.reference.0 == reference)
        .unwrap();
    doc.symbols
        .iter()
        .find(|symbol| symbol.device_id == device.id && symbol.symbol_key.0 == key)
        .unwrap()
        .id
}

/// Relay whose coil is fed through its own normally closed contact: it
/// interrupts itself every tick, like a buzzer.
fn self_interrupting_relay() -> Schematic {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let k1 = rig.device("K1", "relay.generic", &dc_load());
    let coil = rig.symbol(k1, "coil", None);
    let contact = rig.symbol(k1, "aux_nc", Some(1));
    rig.wire(out(p24), pin(contact, "p1"));
    rig.wire(pin(contact, "p2"), pin(coil, "a1"));
    rig.wire(pin(coil, "a2"), out(p0));
    rig.finish()
}

/// `P24 -> F1 97/98 -> B1 -> P0`: the signalling contact of the thermal
/// overload relay sounds a buzzer.
fn thermal_alarm() -> Schematic {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let f1 = rig.device("F1", "thermal_overload.generic", &[]);
    let signalling_contact = rig.symbol(f1, "aux_no", None);
    let buzzer = rig.part("B1", "buzzer.generic", "buzzer", &dc_load());
    rig.wire(out(p24), pin(signalling_contact, "p1"));
    rig.wire(pin(signalling_contact, "p2"), pin(buzzer, "x1"));
    rig.wire(pin(buzzer, "x2"), out(p0));
    rig.finish()
}

/// `S1` feeds timer `KT1`; its delayed contact feeds lamp `H1`.
fn timer_circuit(model: &str, preset_ms: i64) -> Schematic {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let button = rig.part("S1", "pushbutton_no", "contact", &[]);
    let mut params = dc_load();
    params.push(("preset_ms", ParamValue::Int(preset_ms)));
    let timer = rig.device("KT1", model, &params);
    let coil = rig.symbol(timer, "coil", None);
    let delayed = rig.symbol(timer, "delayed_no", Some(1));
    let lamp = rig.part("H1", "lamp.generic", "lamp", &dc_load());
    let feed = rig.junction();
    let common = rig.junction();
    rig.wire(out(p24), feed);
    rig.wire(feed, pin(button, "p1"));
    rig.wire(pin(button, "p2"), pin(coil, "a1"));
    rig.wire(pin(coil, "a2"), common);
    rig.wire(feed, pin(delayed, "p1"));
    rig.wire(pin(delayed, "p2"), pin(lamp, "x1"));
    rig.wire(pin(lamp, "x2"), common);
    rig.wire(common, out(p0));
    rig.finish()
}

const COMMAND_SCRIPT: &[(u64, &str)] = &[
    (2, "press S1"),
    (6, "release S1"),
    (10, "press S2"),
    (14, "release S2"),
];

#[test]
fn command_circuit_produces_the_reference_trace() {
    let mut sim = default_sim(&command_circuit());

    let lines = run(&mut sim, 20, COMMAND_SCRIPT);

    assert_eq!(
        lines,
        [
            "t=2 input S1.pressed 0->1",
            "t=2 load K1.coil on",
            "t=3 signal K1.active 0->1",
            "t=6 input S1.pressed 1->0",
            "t=10 input S2.pressed 0->1",
            "t=10 load K1.coil off",
            "t=11 signal K1.active 1->0",
            "t=14 input S2.pressed 1->0",
        ]
    );
}

#[test]
fn trace_text_starts_with_fingerprint_and_configuration() {
    let mut sim = default_sim(&command_circuit());
    sim.step(&[]).unwrap();
    let events = sim.take_trace();

    let text = format_trace(sim.circuit(), &sim.trace_header(), &events);

    assert_eq!(
        text,
        format!(
            "# opencircuit trace v1\n# fingerprint {}\n# tick_us=1000 pickup_ticks=1 dropout_ticks=1 fault_policy=halt\n",
            sim.circuit().fingerprint()
        )
    );
}

#[test]
fn direct_online_starter_runs_trips_and_restarts() {
    let mut sim = default_sim(&direct_online_circuit());

    let lines = run(
        &mut sim,
        22,
        &[
            (1, "close Q1"),
            (2, "press S1"),
            (4, "release S1"),
            (8, "trip F1"),
            (12, "reset F1"),
            (14, "press S1"),
            (15, "release S1"),
            (18, "press S2"),
            (19, "release S2"),
        ],
    );

    assert_eq!(
        lines,
        [
            "t=1 input Q1.closed 0->1",
            "t=2 input S1.pressed 0->1",
            "t=2 load K1.coil on",
            "t=3 signal K1.active 0->1",
            "t=3 load M1.motor forward",
            "t=4 input S1.pressed 1->0",
            "t=8 input F1.tripped 0->1",
            "t=8 load K1.coil off",
            "t=9 signal K1.active 1->0",
            "t=9 load M1.motor off",
            "t=12 input F1.tripped 1->0",
            "t=14 input S1.pressed 0->1",
            "t=14 load K1.coil on",
            "t=15 input S1.pressed 1->0",
            "t=15 signal K1.active 0->1",
            "t=15 load M1.motor forward",
            "t=18 input S2.pressed 0->1",
            "t=18 load K1.coil off",
            "t=19 input S2.pressed 1->0",
            "t=19 signal K1.active 1->0",
            "t=19 load M1.motor off",
        ]
    );
}

#[test]
fn motor_does_not_run_while_the_main_switch_is_open() {
    let mut sim = default_sim(&direct_online_circuit());

    let lines = run(&mut sim, 6, &[(2, "press S1")]);

    assert_eq!(
        lines,
        [
            "t=2 input S1.pressed 0->1",
            "t=2 load K1.coil on",
            "t=3 signal K1.active 0->1",
        ]
    );
}

#[test]
fn reversing_starter_runs_forward_and_stops() {
    let mut sim = default_sim(&reversing_circuit());

    let lines = run(
        &mut sim,
        12,
        &[
            (2, "press S1"),
            (4, "release S1"),
            (8, "press S2"),
            (9, "release S2"),
        ],
    );

    assert_eq!(
        lines,
        [
            "t=2 input S1.pressed 0->1",
            "t=2 load K1.coil on",
            "t=3 signal K1.active 0->1",
            "t=3 load M1.motor forward",
            "t=4 input S1.pressed 1->0",
            "t=8 input S2.pressed 0->1",
            "t=8 load K1.coil off",
            "t=9 input S2.pressed 1->0",
            "t=9 signal K1.active 1->0",
            "t=9 load M1.motor off",
        ]
    );
}

#[test]
fn reversing_starter_runs_in_reverse_through_the_second_contactor() {
    let mut sim = default_sim(&reversing_circuit());

    let lines = run(&mut sim, 5, &[(2, "press S3")]);

    assert_eq!(
        lines,
        [
            "t=2 input S3.pressed 0->1",
            "t=2 load K2.coil on",
            "t=3 signal K2.active 0->1",
            "t=3 load M1.motor reverse",
        ]
    );
}

#[test]
fn interlock_blocks_the_second_contactor_while_the_first_is_active() {
    let mut sim = default_sim(&reversing_circuit());

    let lines = run(
        &mut sim,
        10,
        &[
            (2, "press S1"),
            (3, "release S1"),
            (6, "press S3"),
            (8, "release S3"),
        ],
    );

    assert_eq!(
        lines,
        [
            "t=2 input S1.pressed 0->1",
            "t=2 load K1.coil on",
            "t=3 input S1.pressed 1->0",
            "t=3 signal K1.active 0->1",
            "t=3 load M1.motor forward",
            "t=6 input S3.pressed 0->1",
            "t=8 input S3.pressed 1->0",
        ]
    );
}

const SIMULTANEOUS_START: &[(u64, &str)] = &[(2, "press S1"), (2, "press S3")];

#[test]
fn simultaneous_start_latches_at_the_first_short_circuit() {
    let mut sim = default_sim(&reversing_circuit());

    let lines = run(&mut sim, 20, SIMULTANEOUS_START);

    assert_eq!(
        lines,
        [
            "t=2 input S1.pressed 0->1",
            "t=2 input S3.pressed 0->1",
            "t=2 load K1.coil on",
            "t=2 load K2.coil on",
            "t=3 signal K1.active 0->1",
            "t=3 signal K2.active 0->1",
            "t=3 load K1.coil off",
            "t=3 load K2.coil off",
            "t=3 fault OC-ELEC-001 started L1,L3",
            "t=3 runtime latched",
        ]
    );
}

#[test]
fn faulting_tick_completes_and_its_snapshot_carries_the_fault() {
    let mut sim = default_sim(&reversing_circuit());
    run(&mut sim, 20, SIMULTANEOUS_START);

    let snapshot = sim.snapshot().unwrap();

    assert_eq!(snapshot.tick, 3);
    assert!(snapshot.faulted);
    assert_eq!(snapshot.faults.len(), 1);
    assert_eq!(snapshot.faults[0].code(), DiagnosticCode::ShortCircuit);
    assert_eq!(snapshot.active_devices, [id(&sim, "K1"), id(&sim, "K2")]);
    assert_eq!(snapshot.energized_devices, []);
}

#[test]
fn latched_runtime_refuses_to_step_and_keeps_its_state() {
    let mut sim = default_sim(&reversing_circuit());
    run(&mut sim, 20, SIMULTANEOUS_START);
    let before = sim.snapshot();

    assert_eq!(sim.step(&[]), Err(SimulationError::AlreadyFaulted));
    assert_eq!(sim.step_n(5, &[]), Err(SimulationError::AlreadyFaulted));

    assert!(sim.is_faulted());
    assert_eq!(sim.tick(), 4);
    assert_eq!(sim.snapshot(), before);
    assert_eq!(sim.take_trace(), []);
}

#[test]
fn reset_fault_clears_the_latch_and_a_persisting_cause_latches_again() {
    let mut sim = default_sim(&reversing_circuit());
    run(&mut sim, 20, SIMULTANEOUS_START);

    sim.reset_fault();
    let lines = run(&mut sim, 20, &[]);

    assert_eq!(
        lines,
        [
            "t=4 runtime latch-reset",
            "t=4 signal K1.active 1->0",
            "t=4 signal K2.active 1->0",
            "t=4 load K1.coil on",
            "t=4 load K2.coil on",
            "t=4 fault OC-ELEC-001 cleared L1,L3",
            "t=5 signal K1.active 0->1",
            "t=5 signal K2.active 0->1",
            "t=5 load K1.coil off",
            "t=5 load K2.coil off",
            "t=5 fault OC-ELEC-001 started L1,L3",
            "t=5 runtime latched",
        ]
    );
}

#[test]
fn report_policy_keeps_running_and_the_reverser_oscillates_every_two_ticks() {
    let mut sim = report_sim(&reversing_circuit());

    let lines = run(&mut sim, 6, SIMULTANEOUS_START);

    let tail: Vec<&str> = lines.iter().skip(4).map(String::as_str).collect();
    assert_eq!(
        tail,
        [
            "t=3 signal K1.active 0->1",
            "t=3 signal K2.active 0->1",
            "t=3 load K1.coil off",
            "t=3 load K2.coil off",
            "t=3 fault OC-ELEC-001 started L1,L3",
            "t=4 signal K1.active 1->0",
            "t=4 signal K2.active 1->0",
            "t=4 load K1.coil on",
            "t=4 load K2.coil on",
            "t=4 fault OC-ELEC-001 cleared L1,L3",
            "t=5 signal K1.active 0->1",
            "t=5 signal K2.active 0->1",
            "t=5 load K1.coil off",
            "t=5 load K2.coil off",
            "t=5 fault OC-ELEC-001 started L1,L3",
            "t=6 signal K1.active 1->0",
            "t=6 signal K2.active 1->0",
            "t=6 load K1.coil on",
            "t=6 load K2.coil on",
            "t=6 fault OC-ELEC-001 cleared L1,L3",
        ]
    );
    assert!(!sim.is_faulted());
}

#[test]
fn coil_fed_through_its_own_normally_closed_contact_toggles_every_tick() {
    let mut sim = default_sim(&self_interrupting_relay());

    let lines = run(&mut sim, 4, &[]);

    assert_eq!(
        lines,
        [
            "t=0 load K1.coil on",
            "t=1 signal K1.active 0->1",
            "t=1 load K1.coil off",
            "t=2 signal K1.active 1->0",
            "t=2 load K1.coil on",
            "t=3 signal K1.active 0->1",
            "t=3 load K1.coil off",
            "t=4 signal K1.active 1->0",
            "t=4 load K1.coil on",
        ]
    );
}

#[test]
fn thermal_overload_closes_its_normally_open_contact_in_the_tick_of_the_trip() {
    let mut sim = default_sim(&thermal_alarm());

    let lines = run(&mut sim, 6, &[(2, "trip F1"), (5, "reset F1")]);

    assert_eq!(
        lines,
        [
            "t=2 input F1.tripped 0->1",
            "t=2 load B1.buzzer on",
            "t=5 input F1.tripped 1->0",
            "t=5 load B1.buzzer off",
        ]
    );
}

#[test]
fn on_delay_timer_closes_its_contact_after_the_preset() {
    let mut sim = default_sim(&timer_circuit("timer_on.generic", 3));

    let lines = run(&mut sim, 12, &[(2, "press S1"), (8, "release S1")]);

    assert_eq!(
        lines,
        [
            "t=2 input S1.pressed 0->1",
            "t=2 load KT1.coil on",
            "t=5 signal KT1.out 0->1",
            "t=5 load H1.lamp on",
            "t=8 input S1.pressed 1->0",
            "t=8 load KT1.coil off",
            "t=9 signal KT1.out 1->0",
            "t=9 load H1.lamp off",
        ]
    );
}

#[test]
fn off_delay_timer_holds_its_contact_for_the_preset_after_the_input_drops() {
    let mut sim = default_sim(&timer_circuit("timer_off.generic", 3));

    let lines = run(&mut sim, 12, &[(2, "press S1"), (5, "release S1")]);

    assert_eq!(
        lines,
        [
            "t=2 input S1.pressed 0->1",
            "t=2 load KT1.coil on",
            "t=3 signal KT1.out 0->1",
            "t=3 load H1.lamp on",
            "t=5 input S1.pressed 1->0",
            "t=5 load KT1.coil off",
            "t=8 signal KT1.out 1->0",
            "t=8 load H1.lamp off",
        ]
    );
}

#[test]
fn timer_view_shows_the_counter_of_the_solved_tick() {
    let mut sim = default_sim(&timer_circuit("timer_on.generic", 3));
    run(&mut sim, 3, &[(2, "press S1")]);

    let timers = sim.snapshot().unwrap().timers;

    assert_eq!(
        timers,
        [TimerView {
            device: id(&sim, "KT1"),
            elapsed_ticks: 2,
            preset_ticks: 3,
            out: false,
        }]
    );
}

#[test]
fn preset_that_is_not_a_multiple_of_the_tick_is_rounded_up_and_reported() {
    let sim = simulator(
        &timer_circuit("timer_on.generic", 5),
        SimConfig {
            tick_us: 2_000,
            ..SimConfig::default()
        },
    );

    let diagnostics = sim.diagnostics();

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code(), DiagnosticCode::PresetQuantized);
    assert_eq!(diagnostics[0].subject(), &Subject::Device(id(&sim, "KT1")));
}

#[test]
fn latching_switch_that_starts_closed_feeds_its_load_from_tick_zero() {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let selector = rig.part(
        "SA1",
        "selector.latching",
        "contact",
        &[("initial_closed", ParamValue::Bool(true))],
    );
    let lamp = rig.part("H1", "lamp.generic", "lamp", &dc_load());
    rig.wire(out(p24), pin(selector, "p1"));
    rig.wire(pin(selector, "p2"), pin(lamp, "x1"));
    rig.wire(pin(lamp, "x2"), out(p0));
    let mut sim = default_sim(&rig.finish());

    let lines = run(&mut sim, 4, &[(2, "open SA1"), (4, "close SA1")]);

    assert_eq!(
        lines,
        [
            "t=0 load H1.lamp on",
            "t=2 input SA1.closed 1->0",
            "t=2 load H1.lamp off",
            "t=4 input SA1.closed 0->1",
            "t=4 load H1.lamp on",
        ]
    );
}

#[test]
fn pickup_delay_requires_consecutive_energized_ticks() {
    let config = SimConfig {
        delays: DelayConfig {
            pickup_ticks: 3,
            dropout_ticks: 1,
        },
        ..SimConfig::default()
    };
    let mut held = simulator(&command_circuit(), config);
    let mut tapped = simulator(&command_circuit(), config);

    let held_lines = run(&mut held, 8, &[(2, "press S1")]);
    let tapped_lines = run(&mut tapped, 8, &[(2, "press S1"), (4, "release S1")]);

    assert_eq!(
        held_lines,
        [
            "t=2 input S1.pressed 0->1",
            "t=2 load K1.coil on",
            "t=5 signal K1.active 0->1",
        ]
    );
    assert_eq!(
        tapped_lines,
        [
            "t=2 input S1.pressed 0->1",
            "t=2 load K1.coil on",
            "t=4 input S1.pressed 1->0",
            "t=4 load K1.coil off",
        ]
    );
}

#[test]
fn snapshot_is_none_before_the_first_step() {
    let sim = default_sim(&command_circuit());

    assert_eq!(sim.snapshot(), None);
    assert_eq!(sim.tick(), 0);
}

#[test]
fn snapshot_shows_the_solved_tick_not_the_committed_next_state() {
    let doc = command_circuit();
    let mut sim = default_sim(&doc);
    run(&mut sim, 2, COMMAND_SCRIPT);

    let energizing = sim.snapshot().unwrap();
    sim.step(&[]).unwrap();
    let picked_up = sim.snapshot().unwrap();

    let k1 = id(&sim, "K1");
    assert_eq!((energizing.tick, energizing.t_us), (2, 2_000));
    assert_eq!(energizing.energized_devices, [k1]);
    assert_eq!(energizing.active_devices, []);
    assert_eq!(energizing.actuated_devices, [id(&sim, "S1")]);
    assert_eq!(
        energizing.closed_symbols,
        [
            symbol_of(&doc, "S2", "contact"),
            symbol_of(&doc, "S1", "contact")
        ]
    );
    assert_eq!((picked_up.tick, picked_up.t_us), (3, 3_000));
    assert_eq!(picked_up.active_devices, [k1]);
    assert!(
        picked_up
            .closed_symbols
            .contains(&symbol_of(&doc, "K1", "aux_no"))
    );
}

#[test]
fn snapshot_lists_the_conductor_on_every_live_wire() {
    let doc = command_circuit();
    let mut sim = default_sim(&doc);
    sim.step(&[]).unwrap();

    let snapshot = sim.snapshot().unwrap();

    let positive = ConductorSet::of(Conductor::DcPos);
    let negative = ConductorSet::of(Conductor::DcNeg);
    let conductors: Vec<ConductorSet> = snapshot
        .live_wires
        .iter()
        .map(|live| live.conductors)
        .collect();
    // Idle: P24 reaches S2, the node above the start button and both wires
    // leaving it; P0 reaches the coil.
    assert_eq!(
        conductors,
        [positive, positive, positive, positive, negative]
    );
    assert_eq!(snapshot.live_junctions.len(), 1);
    assert!(
        snapshot
            .live_wires
            .windows(2)
            .all(|pair| pair[0].id < pair[1].id)
    );
}

#[test]
fn snapshot_reports_motor_direction_and_tripped_overload() {
    let mut sim = default_sim(&direct_online_circuit());
    run(&mut sim, 4, &[(1, "close Q1"), (2, "press S1")]);
    let running = sim.snapshot().unwrap();
    run(&mut sim, 6, &[(5, "trip F1")]);
    let tripped = sim.snapshot().unwrap();

    let motor = id(&sim, "M1");
    assert_eq!(
        running.motors,
        [MotorView {
            device: motor,
            state: MotorState::Forward,
            winding: None,
        }]
    );
    assert_eq!(running.tripped_devices, []);
    assert_eq!(
        tripped.motors,
        [MotorView {
            device: motor,
            state: MotorState::Stopped,
            winding: None,
        }]
    );
    assert_eq!(tripped.tripped_devices, [id(&sim, "F1")]);
}

#[test]
fn unknown_device_is_rejected_without_consuming_the_tick() {
    let mut sim = default_sim(&command_circuit());

    let result = sim.step(&[InputAction::Press(DeviceId(9_999))]);

    assert_eq!(
        result,
        Err(SimulationError::InvalidInput(InvalidInput::UnknownDevice(
            DeviceId(9_999)
        )))
    );
    assert_eq!(sim.tick(), 0);
    assert_eq!(sim.snapshot(), None);
}

#[test]
fn action_that_does_not_fit_the_device_class_is_rejected() {
    let mut sim = default_sim(&command_circuit());
    let (k1, s1) = (id(&sim, "K1"), id(&sim, "S1"));

    for action in [
        InputAction::Press(k1),
        InputAction::SetClosed(s1, true),
        InputAction::Trip(s1),
    ] {
        assert_eq!(
            sim.step(&[action]),
            Err(SimulationError::InvalidInput(InvalidInput::ClassMismatch(
                action.device()
            )))
        );
    }
    assert_eq!(sim.tick(), 0);
}

#[test]
fn format_event_gives_each_event_line_of_the_trace() {
    let mut sim = default_sim(&command_circuit());
    let s1 = id(&sim, "S1");
    sim.step(&[InputAction::Press(s1)]).unwrap();
    sim.step_n(3, &[InputAction::Release(s1)]).unwrap();
    let events = sim.take_trace();

    let text = format_trace(sim.circuit(), &sim.trace_header(), &events);
    let lines: Vec<String> = events
        .iter()
        .map(|event| crate::trace::format_event(sim.circuit(), event))
        .collect();

    assert!(!lines.is_empty());
    assert_eq!(text.lines().skip(3).collect::<Vec<_>>(), lines);
}

#[test]
fn operator_controls_list_exactly_the_actions_the_simulator_accepts() {
    let sim = default_sim(&direct_online_circuit());
    let mut classes_seen = std::collections::BTreeSet::new();

    for device in sim.circuit().devices() {
        classes_seen.insert(device.class);
        let offered = circuit_model::operator_controls(device.class);
        for control in circuit_model::Control::ALL {
            for action in control.actions(device.id) {
                let accepted = !matches!(
                    sim.clone().step(&[action]),
                    Err(SimulationError::InvalidInput(InvalidInput::ClassMismatch(
                        _
                    )))
                );

                assert_eq!(
                    accepted,
                    offered.contains(&control),
                    "{} ({:?}): {action:?}",
                    device.reference.0,
                    device.class
                );
            }
        }
    }
    // The circuit covers every class that has a control, and classes that
    // have none.
    for class in [
        DeviceClass::Pushbutton,
        DeviceClass::LatchingSwitch,
        DeviceClass::ThermalOverload,
        DeviceClass::Contactor,
        DeviceClass::Motor,
        DeviceClass::PowerSupply,
    ] {
        assert!(classes_seen.contains(&class), "{class:?}");
    }
}

#[test]
fn two_actions_on_one_device_in_one_tick_are_rejected() {
    let mut sim = default_sim(&command_circuit());
    let s1 = id(&sim, "S1");

    let result = sim.step(&[InputAction::Press(s1), InputAction::Release(s1)]);

    assert_eq!(
        result,
        Err(SimulationError::InvalidInput(
            InvalidInput::DuplicateDevice(s1)
        ))
    );
}

#[test]
fn rejected_batch_applies_none_of_its_actions() {
    let mut sim = default_sim(&command_circuit());
    let s1 = id(&sim, "S1");

    let rejected = sim.step(&[InputAction::Press(s1), InputAction::Press(DeviceId(9_999))]);
    let lines = run(&mut sim, 3, &[]);

    assert!(rejected.is_err());
    assert_eq!(lines, Vec::<String>::new());
}

#[test]
fn repeating_an_action_records_nothing() {
    let mut sim = default_sim(&command_circuit());

    let lines = run(
        &mut sim,
        4,
        &[(1, "press S2"), (2, "press S2"), (3, "press S2")],
    );

    assert_eq!(lines, ["t=1 input S2.pressed 0->1"]);
}

#[test]
fn step_outcome_reports_tick_and_whether_anything_changed() {
    let mut sim = default_sim(&command_circuit());
    let press = action(sim.circuit(), "press S1");

    let idle = sim.step(&[]).unwrap();
    let pressed = sim.step(&[press]).unwrap();

    assert_eq!(
        idle,
        StepOutcome {
            tick: 0,
            changed: false,
            faulted: false
        }
    );
    assert_eq!(
        pressed,
        StepOutcome {
            tick: 1,
            changed: true,
            faulted: false
        }
    );
}

#[test]
fn step_n_applies_inputs_to_the_first_tick_only_and_stops_at_a_latch() {
    let mut sim = default_sim(&reversing_circuit());
    let start = [
        action(sim.circuit(), "press S1"),
        action(sim.circuit(), "press S3"),
    ];

    let outcome = sim.step_n(10, &start).unwrap();

    assert_eq!(
        outcome,
        StepOutcome {
            tick: 1,
            changed: true,
            faulted: true
        }
    );
    assert_eq!(sim.tick(), 2);
}

#[test]
fn idle_ticks_solve_the_network_once() {
    let mut sim = default_sim(&command_circuit());

    sim.step_n(50, &[]).unwrap();

    assert_eq!(sim.solve_count(), 1);
}

#[test]
fn trace_can_be_switched_off_without_changing_behaviour() {
    let mut sim = simulator(
        &command_circuit(),
        SimConfig {
            record_trace: false,
            ..SimConfig::default()
        },
    );
    let press = action(sim.circuit(), "press S1");

    let outcome = sim.step_n(3, &[press]).unwrap();

    assert!(outcome.changed);
    assert_eq!(sim.take_trace(), []);
    assert_eq!(sim.snapshot().unwrap().active_devices, [id(&sim, "K1")]);
}

#[test]
fn full_trace_buffer_stops_the_run_until_the_trace_is_taken() {
    let mut sim = default_sim(&self_interrupting_relay());
    sim.set_trace_limit(3);

    let result = sim.step_n(10, &[]);
    let tick_at_refusal = sim.tick();
    let taken = sim.take_trace();
    let resumed = sim.step(&[]);

    assert_eq!(result, Err(SimulationError::TraceBufferFull));
    assert_eq!(tick_at_refusal, 2);
    assert_eq!(taken.len(), 3);
    assert!(resumed.is_ok());
}

#[test]
fn exhausted_tick_counter_is_an_error_not_an_overflow() {
    let mut sim = default_sim(&command_circuit());
    sim.set_tick(u64::MAX);

    assert_eq!(sim.step(&[]), Err(SimulationError::TickOverflow));
    assert_eq!(sim.tick(), u64::MAX);
}

#[test]
fn zero_tick_and_zero_delays_are_rejected() {
    let circuit = compile_ok(&command_circuit());
    let with = |tick_us, pickup_ticks, dropout_ticks| {
        Simulator::new(
            circuit.clone(),
            SimConfig {
                tick_us,
                delays: DelayConfig {
                    pickup_ticks,
                    dropout_ticks,
                },
                ..SimConfig::default()
            },
        )
        .map(|_| ())
    };

    assert_eq!(with(0, 1, 1), Err(ConfigError::ZeroTick));
    assert_eq!(with(1_000, 0, 1), Err(ConfigError::ZeroPickup));
    assert_eq!(with(1_000, 1, 0), Err(ConfigError::ZeroDropout));
    assert_eq!(
        ConfigError::ZeroTick.diagnostic().code(),
        DiagnosticCode::InvalidConfig
    );
}

#[test]
fn reset_returns_to_tick_zero_and_the_run_repeats_exactly() {
    let mut sim = default_sim(&command_circuit());
    let first = run(&mut sim, 20, COMMAND_SCRIPT);

    sim.reset();
    let pristine = (sim.tick(), sim.snapshot(), sim.is_faulted());
    let second = run(&mut sim, 20, COMMAND_SCRIPT);

    assert_eq!(pristine, (0, None, false));
    assert_eq!(second, first);
}

#[test]
fn reset_clears_a_fault_latch() {
    let mut sim = default_sim(&reversing_circuit());
    run(&mut sim, 20, SIMULTANEOUS_START);

    sim.reset();

    assert!(!sim.is_faulted());
    assert!(sim.step(&[]).is_ok());
}

#[test]
fn restore_then_replay_reproduces_trace_and_snapshot() {
    let mut sim = default_sim(&command_circuit());
    run(&mut sim, 7, COMMAND_SCRIPT);
    let checkpoint = sim.checkpoint();

    let first = run(&mut sim, 20, COMMAND_SCRIPT);
    let first_snapshot = sim.snapshot();
    sim.restore(&checkpoint).unwrap();
    let restored_snapshot_tick = sim.snapshot().map(|snapshot| snapshot.tick);
    let second = run(&mut sim, 20, COMMAND_SCRIPT);

    assert_eq!(checkpoint.tick(), 8);
    assert_eq!(restored_snapshot_tick, Some(7));
    assert_eq!(second, first);
    assert_eq!(sim.snapshot(), first_snapshot);
}

#[test]
fn restored_snapshot_matches_the_snapshot_at_the_checkpoint() {
    let mut sim = default_sim(&direct_online_circuit());
    run(&mut sim, 4, &[(1, "close Q1"), (2, "press S1")]);
    let checkpoint = sim.checkpoint();
    let at_checkpoint = sim.snapshot();
    run(&mut sim, 9, &[(6, "press S2")]);

    sim.restore(&checkpoint).unwrap();

    assert_eq!(sim.snapshot(), at_checkpoint);
}

#[test]
fn checkpoint_carries_the_fault_latch() {
    let mut sim = default_sim(&reversing_circuit());
    run(&mut sim, 20, SIMULTANEOUS_START);
    let latched = sim.checkpoint();
    sim.reset();

    sim.restore(&latched).unwrap();

    assert!(sim.is_faulted());
    assert_eq!(sim.step(&[]), Err(SimulationError::AlreadyFaulted));
}

#[test]
fn checkpoint_of_another_circuit_or_configuration_is_refused() {
    let mut sim = default_sim(&command_circuit());
    let other_circuit = default_sim(&reversing_circuit()).checkpoint();
    let other_config = report_sim(&command_circuit()).checkpoint();

    assert_eq!(
        sim.restore(&other_circuit),
        Err(RestoreError::CircuitMismatch)
    );
    assert_eq!(
        sim.restore(&other_config),
        Err(RestoreError::ConfigMismatch)
    );
    assert_eq!(sim.tick(), 0);
}

#[test]
fn trace_does_not_depend_on_collection_order() {
    let doc = reversing_circuit();
    let mut reversed = doc.clone();
    reversed.devices.reverse();
    reversed.symbols.reverse();
    reversed.junctions.reverse();
    reversed.wires.reverse();
    reversed.supplies.reverse();
    let script = &[(2, "press S3"), (5, "press S2"), (6, "release S2")];
    let mut first = default_sim(&doc);
    let mut second = default_sim(&reversed);

    assert_eq!(run(&mut second, 10, script), run(&mut first, 10, script));
    assert_eq!(second.trace_header(), first.trace_header());
}

/// One scripted input: tick, button index and whether it is pressed.
type Stroke = (u64, usize, bool);

fn strokes() -> impl Strategy<Value = Vec<Stroke>> {
    prop::collection::vec((0_u64..40, 0_usize..3, any::<bool>()), 0..24).prop_map(|mut list| {
        list.sort_unstable();
        list.dedup_by_key(|(tick, button, _)| (*tick, *button));
        list
    })
}

fn actions_at(sim: &Simulator, script: &[Stroke], tick: u64) -> Vec<InputAction> {
    script
        .iter()
        .filter(|(at, _, _)| *at == tick)
        .map(|(_, button, pressed)| {
            let device = id(sim, ["S1", "S2", "S3"][*button]);
            if *pressed {
                InputAction::Press(device)
            } else {
                InputAction::Release(device)
            }
        })
        .collect()
}

fn mechanical_only() -> Schematic {
    reversing_with(Interlocks {
        electrical: false,
        mechanical: true,
    })
}

fn active(sim: &Simulator, reference: &str) -> bool {
    sim.snapshot()
        .is_some_and(|snapshot| snapshot.active_devices.contains(&id(sim, reference)))
}

#[test]
fn simultaneous_start_with_a_mechanical_interlock_picks_up_neither_and_warns() {
    let mut sim = default_sim(&mechanical_only());

    let trace = run(&mut sim, 6, &[(2, "press S1"), (2, "press S3")]);

    assert!(!active(&sim, "K1") && !active(&sim, "K2"));
    assert!(!sim.snapshot().unwrap().faulted);
    assert!(
        trace.contains(&"t=2 fault OC-ELEC-007 started K1,K2".to_owned()),
        "{trace:?}"
    );
    assert!(
        !trace.iter().any(|line| line.contains("OC-ELEC-001")),
        "{trace:?}"
    );
    let faults = sim.snapshot().unwrap().faults;
    assert_eq!(
        faults.iter().map(Diagnostic::code).collect::<Vec<_>>(),
        [DiagnosticCode::InterlockContention]
    );
}

#[test]
fn releasing_one_button_after_a_contention_lets_the_other_contactor_pick_up() {
    let mut sim = default_sim(&mechanical_only());

    let trace = run(
        &mut sim,
        8,
        &[(2, "press S1"), (2, "press S3"), (5, "release S3")],
    );

    assert!(active(&sim, "K1") && !active(&sim, "K2"));
    assert!(
        trace.contains(&"t=5 fault OC-ELEC-007 cleared K1,K2".to_owned()),
        "{trace:?}"
    );
    assert!(
        trace.contains(&"t=6 signal K1.active 0->1".to_owned()),
        "{trace:?}"
    );
}

#[test]
fn active_contactor_blocks_its_partner_even_with_the_coil_energized() {
    let mut sim = default_sim(&mechanical_only());

    run(
        &mut sim,
        6,
        &[(2, "press S1"), (3, "release S1"), (5, "press S3")],
    );
    let snapshot = sim.snapshot().unwrap();

    assert!(active(&sim, "K1") && !active(&sim, "K2"));
    assert!(snapshot.energized_devices.contains(&id(&sim, "K2")));
    assert!(snapshot.faults.is_empty());
}

#[test]
fn partner_picks_up_only_after_the_first_contactor_has_dropped_out() {
    // K1 runs; S2 stops it while S3 is held. K2 may pick up only at the
    // commit after K1 is low, so there is a tick with both out.
    let mut sim = default_sim(&mechanical_only());

    let trace = run(
        &mut sim,
        12,
        &[
            (2, "press S1"),
            (3, "release S1"),
            (5, "press S3"),
            (7, "press S2"),
            (8, "release S2"),
        ],
    );

    let tick_of = |line: &str| {
        trace
            .iter()
            .find(|l| l.ends_with(line))
            .and_then(|l| l.strip_prefix("t="))
            .and_then(|l| l.split(' ').next())
            .and_then(|t| t.parse::<u64>().ok())
            .unwrap_or_else(|| panic!("no `{line}` in {trace:?}"))
    };
    let k1_out = tick_of("K1.active 1->0");
    let k2_in = tick_of("K2.active 0->1");
    assert!(k2_in > k1_out, "{trace:?}");
}

proptest! {
    #[test]
    fn mechanically_interlocked_contactors_are_never_active_together(script in strokes()) {
        let mut sim = default_sim(&mechanical_only());
        let (k1, k2) = (id(&sim, "K1"), id(&sim, "K2"));
        for tick in 0..45 {
            let actions = actions_at(&sim, &script, tick);
            sim.step(&actions).unwrap();
            let snapshot = sim.snapshot().unwrap();

            prop_assert!(
                !(snapshot.active_devices.contains(&k1) && snapshot.active_devices.contains(&k2)),
                "tick {}", tick
            );
            prop_assert!(!snapshot.faulted, "tick {}", tick);
        }
    }
}

/// A six-terminal motor `M1` on a 400 V supply, each terminal wired to the
/// phases listed for it (none leaves it unconnected), and the second ends in
/// `star` joined at a junction.
fn six_terminal(wiring: [&[usize]; 6], star: bool) -> Schematic {
    let mut rig = Rig::new();
    let phases = rig.three_phase();
    let motor = rig.part(
        "M1",
        "motor.3ph.6t",
        "motor",
        &[("rated_voltage_mv", ParamValue::Int(400_000))],
    );
    for (terminal, feeds) in ["u1", "v1", "w1", "u2", "v2", "w2"].into_iter().zip(wiring) {
        for phase in feeds {
            rig.wire(out(phases[*phase]), pin(motor, terminal));
        }
    }
    if star {
        let point = rig.junction();
        for terminal in ["u2", "v2", "w2"] {
            rig.wire(pin(motor, terminal), point);
        }
    }
    rig.finish()
}

fn motor_after_one_tick(doc: &Schematic) -> (Vec<String>, Vec<MotorView>, Vec<DiagnosticCode>) {
    let mut sim = default_sim(doc);
    let trace = run(&mut sim, 0, &[]);
    let snapshot = sim.snapshot().unwrap();
    let codes = snapshot.faults.iter().map(Diagnostic::code).collect();
    (trace, snapshot.motors, codes)
}

#[test]
fn six_terminal_motor_runs_in_star_with_a_dead_star_point() {
    let doc = six_terminal([&[0], &[1], &[2], &[], &[], &[]], true);
    assert_eq!(compile_doc(&doc).diagnostics, []);

    let (trace, motors, codes) = motor_after_one_tick(&doc);

    assert_eq!(trace, ["t=0 load M1.motor forward-star"]);
    assert_eq!(motors[0].state, MotorState::Forward);
    assert_eq!(motors[0].winding, Some(WindingConnection::Star));
    assert_eq!(codes, []);
}

#[test]
fn six_terminal_motor_runs_in_delta_with_each_line_on_the_next_second_end() {
    let doc = six_terminal([&[0], &[1], &[2], &[1], &[2], &[0]], false);

    let (trace, motors, codes) = motor_after_one_tick(&doc);

    assert_eq!(trace, ["t=0 load M1.motor forward-delta"]);
    assert_eq!(motors[0].winding, Some(WindingConnection::Delta));
    assert_eq!(codes, []);
}

#[test]
fn six_terminal_motor_with_swapped_phases_runs_in_reverse() {
    let doc = six_terminal([&[2], &[1], &[0], &[], &[], &[]], true);

    let (trace, motors, _) = motor_after_one_tick(&doc);

    assert_eq!(trace, ["t=0 load M1.motor reverse-star"]);
    assert_eq!(motors[0].state, MotorState::Reverse);
}

#[test]
fn six_terminal_motor_with_open_windings_is_off_without_a_diagnostic() {
    let doc = six_terminal([&[0], &[1], &[2], &[], &[], &[]], false);

    let (trace, motors, codes) = motor_after_one_tick(&doc);

    assert_eq!(trace, Vec::<String>::new());
    assert_eq!(motors[0].state, MotorState::Stopped);
    assert_eq!(motors[0].winding, None);
    assert_eq!(codes, []);
}

#[test]
fn six_terminal_motor_in_star_with_a_missing_phase_reports_phase_loss() {
    let doc = six_terminal([&[0], &[1], &[], &[], &[], &[]], true);

    let (_, motors, codes) = motor_after_one_tick(&doc);

    assert_eq!(motors[0].state, MotorState::Stopped);
    assert_eq!(codes, [DiagnosticCode::PhaseLoss]);
}

#[test]
fn single_winding_across_two_phases_is_an_unsupported_connection() {
    let doc = six_terminal([&[0], &[], &[], &[1], &[], &[]], false);

    let (_, motors, codes) = motor_after_one_tick(&doc);

    assert_eq!(motors[0].state, MotorState::Stopped);
    assert_eq!(codes, [DiagnosticCode::UnsupportedWindingConnection]);
    assert!(!sim_latches(&doc));
}

fn sim_latches(doc: &Schematic) -> bool {
    let mut sim = default_sim(doc);
    sim.step(&[]).unwrap().faulted
}

#[test]
fn star_point_joined_to_the_lines_is_a_short_circuit() {
    // Star and delta at once: the three phases meet at the star point.
    let doc = six_terminal([&[0], &[1], &[2], &[1], &[2], &[0]], true);
    let output = compile_doc(&doc);

    assert!(codes(&output).contains(&DiagnosticCode::ShortCircuit));
}

#[test]
fn separate_sections_energize_their_own_loads() {
    let doc = two_sections("l1");
    assert_eq!(compile_doc(&doc).diagnostics, []);
    let mut sim = default_sim(&doc);

    run(&mut sim, 1, &[]);
    let snapshot = sim.snapshot().unwrap();

    assert_eq!(snapshot.energized_devices, [id(&sim, "H1"), id(&sim, "H2")]);
    assert!(snapshot.faults.is_empty());
}

#[test]
fn bridging_two_sections_is_a_latched_short_circuit() {
    for bridge in ["l1", "n"] {
        let mut sim = default_sim(&two_sections(bridge));

        let trace = run(&mut sim, 6, &[(2, "close S1")]);

        assert!(
            trace
                .iter()
                .any(|line| line.starts_with("t=2 fault OC-ELEC-001 started")),
            "{bridge}: {trace:?}"
        );
        assert!(
            trace.contains(&"t=2 runtime latched".to_owned()),
            "{bridge}: {trace:?}"
        );
    }
}

#[test]
fn load_across_two_sections_stays_off() {
    // H3 between the l1 of A and the n of B: two systems, no label applies.
    let mut rig = Rig::new();
    let a = rig.single_phase_supply(230_000);
    let b = rig.single_phase_supply(230_000);
    let a_line = rig.tap("LA", "l1", a);
    let b_neutral = rig.tap("NB", "n", b);
    let lamp = rig.part("H3", "lamp.generic", "lamp", &load(230_000, "line_neutral"));
    rig.wire(out(a_line), pin(lamp, "x1"));
    rig.wire(pin(lamp, "x2"), out(b_neutral));
    let mut sim = default_sim(&rig.finish());

    run(&mut sim, 0, &[]);
    let snapshot = sim.snapshot().unwrap();

    assert_eq!(snapshot.energized_devices, []);
    assert_eq!(
        snapshot
            .faults
            .iter()
            .map(Diagnostic::code)
            .collect::<Vec<_>>(),
        [DiagnosticCode::LoadSupplyMismatch]
    );
}

#[test]
fn transformer_secondary_is_live_from_the_tick_after_the_primary() {
    let doc = transformer_circuit(Secondary::Own);
    // L3 of the power supply is unused: an info, nothing more.
    assert_eq!(codes(&compile_doc(&doc)), [DiagnosticCode::UnusedSupply]);
    let mut sim = default_sim(&doc);

    let trace = run(&mut sim, 9, &[(2, "close S1"), (6, "open S1")]);

    assert_eq!(
        trace,
        [
            "t=2 input S1.closed 0->1",
            "t=2 load T1.primary on",
            "t=3 signal T1.powered 0->1",
            "t=3 load H1.lamp on",
            "t=6 input S1.closed 1->0",
            "t=6 load T1.primary off",
            "t=7 signal T1.powered 1->0",
            "t=7 load H1.lamp off",
        ]
    );
}

#[test]
fn powered_transformer_is_listed_as_active() {
    let doc = transformer_circuit(Secondary::Own);
    let mut sim = default_sim(&doc);

    run(&mut sim, 3, &[(2, "close S1")]);

    assert!(
        sim.snapshot()
            .unwrap()
            .active_devices
            .contains(&id(&sim, "T1"))
    );
}

#[test]
fn transformer_fed_from_its_own_secondary_never_starts() {
    let mut rig = Rig::new();
    let [l1, ..] = rig.three_phase();
    let section = rig.single_phase_supply(230_000);
    let mut params = load(230_000, "line_neutral");
    params.push((
        "secondary",
        ParamValue::Int(i64::try_from(section.0).unwrap()),
    ));
    let t1 = rig.device("T1", "transformer.control", &params);
    let primary = rig.symbol(t1, "primary", None);
    let secondary = rig.symbol(t1, "secondary", None);
    rig.wire(pin(secondary, "x1"), pin(primary, "a1"));
    rig.wire(pin(primary, "a2"), pin(secondary, "x2"));
    let lamp = rig.part("H1", "lamp.generic", "lamp", &load(400_000, "line_line"));
    rig.wire(out(l1), pin(lamp, "x1"));
    let mut sim = default_sim(&rig.finish());

    run(&mut sim, 10, &[]);

    assert!(
        !sim.snapshot()
            .unwrap()
            .active_devices
            .contains(&id(&sim, "T1"))
    );
}

fn trace_text(sim: &mut Simulator) -> String {
    let events = sim.take_trace();
    format_trace(sim.circuit(), &sim.trace_header(), &events)
}

/// `S1` feeds relay `K1` (version 2); its changeover contact takes the
/// 24 V from `com` to lamp `H1` on `nc` at rest and to lamp `H2` on `no`
/// when picked up. `model`, `key` and `extra` select a timer instead.
fn changeover_circuit(model: &str, key: &str, extra: &[(&'static str, ParamValue)]) -> Schematic {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let button = rig.part("S1", "pushbutton_no", "contact", &[]);
    let mut params = dc_load();
    params.extend_from_slice(extra);
    let k1 = rig.device_v2("K1", model, &params);
    let coil = rig.symbol(k1, "coil", None);
    let changeover = rig.symbol(k1, key, Some(1));
    let rest = rig.part("H1", "lamp.generic", "lamp", &dc_load());
    let work = rig.part("H2", "lamp.generic", "lamp", &dc_load());
    let feed = rig.junction();
    let common = rig.junction();
    rig.wire(out(p24), feed);
    rig.wire(feed, pin(button, "p1"));
    rig.wire(pin(button, "p2"), pin(coil, "a1"));
    rig.wire(pin(coil, "a2"), common);
    rig.wire(feed, pin(changeover, "com"));
    rig.wire(pin(changeover, "nc"), pin(rest, "x1"));
    rig.wire(pin(changeover, "no"), pin(work, "x1"));
    rig.wire(pin(rest, "x2"), common);
    rig.wire(pin(work, "x2"), common);
    rig.wire(common, out(p0));
    rig.finish()
}

#[test]
fn changeover_moves_its_common_from_nc_to_no_one_tick_after_the_coil() {
    let doc = changeover_circuit("relay.generic", "changeover", &[]);
    let mut sim = default_sim(&doc);
    let changeover = symbol_of(&doc, "K1", "changeover");
    let (h1, h2, k1) = (id(&sim, "H1"), id(&sim, "H2"), id(&sim, "K1"));

    run(&mut sim, 1, &[]);
    let rest = sim.snapshot().unwrap();
    run(&mut sim, 2, &[(2, "press S1")]);
    let energizing = sim.snapshot().unwrap();
    sim.step(&[]).unwrap();
    let picked_up = sim.snapshot().unwrap();

    assert_eq!(rest.energized_devices, [h1]);
    assert!(!rest.closed_symbols.contains(&changeover));
    assert_eq!(energizing.energized_devices, [k1, h1]);
    assert_eq!(picked_up.energized_devices, [k1, h2]);
    assert!(picked_up.closed_symbols.contains(&changeover));
}

#[test]
fn changeover_never_connects_both_sides_at_once() {
    let doc = changeover_circuit("relay.generic", "changeover", &[]);
    let mut sim = default_sim(&doc);
    let (h1, h2) = (id(&sim, "H1"), id(&sim, "H2"));

    for tick in 0..12 {
        let actions = match tick {
            2 => vec![action(sim.circuit(), "press S1")],
            7 => vec![action(sim.circuit(), "release S1")],
            _ => Vec::new(),
        };
        sim.step(&actions).unwrap();
        let lit = sim.snapshot().unwrap().energized_devices;

        assert!(
            lit.contains(&h1) != lit.contains(&h2),
            "tick {tick}: {lit:?}"
        );
    }
}

#[test]
fn timer_changeover_follows_the_delayed_output() {
    let doc = changeover_circuit(
        "timer_on.generic",
        "delayed_co",
        &[("preset_ms", ParamValue::Int(3))],
    );
    let mut sim = default_sim(&doc);
    let (h1, h2) = (id(&sim, "H1"), id(&sim, "H2"));

    let trace = run(&mut sim, 8, &[(2, "press S1")]);

    let switched = trace
        .iter()
        .position(|line| line.contains("H2.lamp on"))
        .unwrap();
    // TON with P = 3 and the input from tick 2: the output is effective
    // from tick 5 (SIMULATION.md, timer table), and the changeover moves
    // with it.
    assert!(trace[switched].starts_with("t=5 "), "{trace:?}");
    assert!(
        trace.contains(&"t=5 load H1.lamp off".to_owned()),
        "{trace:?}"
    );
    let lit = sim.snapshot().unwrap().energized_devices;
    assert!(lit.contains(&h2) && !lit.contains(&h1));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn step_n_equals_the_same_number_of_single_steps(script in strokes()) {
        let doc = reversing_circuit();
        let mut single = report_sim(&doc);
        let mut batched = report_sim(&doc);

        for tick in 0..45 {
            let actions = actions_at(&single, &script, tick);
            single.step(&actions).unwrap();
        }
        // One batch per stretch between scripted ticks.
        let mut boundaries: Vec<u64> = script.iter().map(|(tick, _, _)| *tick).collect();
        boundaries.push(0);
        boundaries.push(45);
        boundaries.sort_unstable();
        boundaries.dedup();
        for pair in boundaries.windows(2) {
            let actions = actions_at(&batched, &script, pair[0]);
            let length = u32::try_from(pair[1] - pair[0]).unwrap();
            batched.step_n(length, &actions).unwrap();
        }

        prop_assert_eq!(batched.tick(), single.tick());
        prop_assert_eq!(batched.snapshot(), single.snapshot());
        prop_assert_eq!(trace_text(&mut batched), trace_text(&mut single));
    }

    #[test]
    fn checkpoint_restore_and_replay_reproduce_any_run(
        script in strokes(),
        checkpoint_tick in 0_u64..45,
    ) {
        let mut sim = report_sim(&reversing_circuit());
        let mut checkpoint = sim.checkpoint();
        let mut reference = String::new();
        for tick in 0..45 {
            if tick == checkpoint_tick {
                checkpoint = sim.checkpoint();
                sim.take_trace();
            }
            let actions = actions_at(&sim, &script, tick);
            sim.step(&actions).unwrap();
        }
        reference.push_str(&trace_text(&mut sim));
        let reference_snapshot = sim.snapshot();

        sim.restore(&checkpoint).unwrap();
        for tick in checkpoint_tick..45 {
            let actions = actions_at(&sim, &script, tick);
            sim.step(&actions).unwrap();
        }

        prop_assert_eq!(trace_text(&mut sim), reference);
        prop_assert_eq!(sim.snapshot(), reference_snapshot);
    }

    #[test]
    fn rerunning_from_reset_is_deterministic(script in strokes()) {
        let mut sim = default_sim(&reversing_circuit());
        let mut texts = Vec::new();
        for _ in 0..2 {
            sim.reset();
            for tick in 0..45 {
                let actions = actions_at(&sim, &script, tick);
                if sim.step(&actions).is_err() {
                    break;
                }
            }
            texts.push((trace_text(&mut sim), sim.snapshot()));
        }

        prop_assert_eq!(&texts[0], &texts[1]);
    }
}
