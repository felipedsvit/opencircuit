//! Circuits and helpers shared by the tests of this crate.

use circuit_model::builder::pin;
use circuit_model::{
    ComponentCatalog, DeviceId, DiagnosticCode, Endpoint, GridPoint, InputAction, JunctionId,
    LayoutIndex, ParamValue, PhaseSequence, Schematic, SchematicBuilder, SupplyId, SymbolId,
    orthogonal_route,
};

use crate::compile::{CompileOutput, compile};
use crate::compiled::CompiledCircuit;
use crate::runtime::{SimConfig, SimulationError, Simulator};
use crate::trace::format_trace;

/// Thin wrapper over [`SchematicBuilder`] that lays symbols out in a row and
/// unwraps the ID allocation, which cannot fail for a test-sized document.
pub(crate) struct Rig {
    builder: SchematicBuilder,
    column: i32,
}

impl Rig {
    pub(crate) fn new() -> Self {
        Self {
            builder: SchematicBuilder::new("test"),
            column: 0,
        }
    }

    fn place(&mut self) -> GridPoint {
        self.column += 4;
        GridPoint::new(self.column, 0)
    }

    pub(crate) fn device(
        &mut self,
        reference: &str,
        model: &str,
        params: &[(&str, ParamValue)],
    ) -> DeviceId {
        self.builder.device(reference, model, params).unwrap()
    }

    pub(crate) fn device_v2(
        &mut self,
        reference: &str,
        model: &str,
        params: &[(&str, ParamValue)],
    ) -> DeviceId {
        self.builder
            .device_version(reference, model, 2, params)
            .unwrap()
    }

    pub(crate) fn symbol(&mut self, device: DeviceId, key: &str, slot: Option<u8>) -> SymbolId {
        let position = self.place();
        self.builder.symbol(device, key, slot, position).unwrap()
    }

    /// A device together with its only symbol.
    pub(crate) fn part(
        &mut self,
        reference: &str,
        model: &str,
        symbol_key: &str,
        params: &[(&str, ParamValue)],
    ) -> SymbolId {
        let device = self.device(reference, model, params);
        self.symbol(device, symbol_key, None)
    }

    pub(crate) fn junction(&mut self) -> JunctionId {
        let position = self.place();
        self.builder.junction(position).unwrap()
    }

    pub(crate) fn wire(&mut self, from: impl Into<Endpoint>, to: impl Into<Endpoint>) {
        self.builder.wire(from, to).unwrap();
    }

    /// A supply terminal device with one tap.
    pub(crate) fn tap(&mut self, reference: &str, conductor: &str, supply: SupplyId) -> SymbolId {
        self.part(
            reference,
            "supply.terminal",
            "terminal",
            &[
                ("conductor", ParamValue::Text(conductor.to_owned())),
                ("supply", ParamValue::Int(i64::try_from(supply.0).unwrap())),
            ],
        )
    }

    pub(crate) fn dc_supply(&mut self, voltage_mv: u32) -> SupplyId {
        self.builder.dc_supply(voltage_mv).unwrap()
    }

    /// 24 V DC supply with taps `P24` and `P0`.
    pub(crate) fn dc24(&mut self) -> (SymbolId, SymbolId) {
        let supply = self.dc_supply(24_000);
        (
            self.tap("P24", "dc_pos", supply),
            self.tap("P0", "dc_neg", supply),
        )
    }

    pub(crate) fn single_phase_supply(&mut self, voltage_mv: u32) -> SupplyId {
        self.builder
            .supply(
                circuit_model::SupplyKind::SinglePhase,
                voltage_mv,
                None,
                Some(50_000),
                None,
            )
            .unwrap()
    }

    pub(crate) fn three_phase_supply(&mut self, sequence: PhaseSequence) -> SupplyId {
        self.builder
            .three_phase_supply(400_000, Some(230_000), 50_000, sequence)
            .unwrap()
    }

    /// 400/230 V three-phase supply with taps `L1`, `L2` and `L3`.
    pub(crate) fn three_phase(&mut self) -> [SymbolId; 3] {
        let supply = self.three_phase_supply(PhaseSequence::L1L2L3);
        [
            self.tap("L1", "l1", supply),
            self.tap("L2", "l2", supply),
            self.tap("L3", "l3", supply),
        ]
    }

    /// The document, with every wire given an orthogonal route: the row
    /// layout would otherwise make most wires diagonal, and the tests are
    /// not about `NonOrthogonalWire`.
    pub(crate) fn finish(self) -> Schematic {
        let mut doc = self.builder.finish();
        let catalog = ComponentCatalog::builtin();
        let routes: Vec<Option<Vec<GridPoint>>> = {
            let layout = LayoutIndex::new(&doc, &catalog);
            doc.wires
                .iter()
                .map(|wire| {
                    match (
                        layout.endpoint_position(&wire.from),
                        layout.endpoint_position(&wire.to),
                    ) {
                        (Some(from), Some(to)) if wire.route.is_empty() => {
                            Some(orthogonal_route(from, to))
                        }
                        _ => None,
                    }
                })
                .collect()
        };
        for (wire, route) in doc.wires.iter_mut().zip(routes) {
            if let Some(route) = route {
                wire.route = route;
            }
        }
        doc
    }
}

pub(crate) fn out(tap: SymbolId) -> Endpoint {
    pin(tap, "out")
}

/// Parameters of a 24 V DC two-terminal load.
pub(crate) fn dc_load() -> Vec<(&'static str, ParamValue)> {
    load(24_000, "dc")
}

pub(crate) fn load(rated_voltage_mv: i64, connection: &str) -> Vec<(&'static str, ParamValue)> {
    vec![
        ("rated_voltage_mv", ParamValue::Int(rated_voltage_mv)),
        ("connection", ParamValue::Text(connection.to_owned())),
    ]
}

pub(crate) fn compile_doc(doc: &Schematic) -> CompileOutput {
    compile(doc, &ComponentCatalog::builtin())
}

/// Compile a document that is expected to be free of errors.
pub(crate) fn compile_ok(doc: &Schematic) -> CompiledCircuit {
    let output = compile_doc(doc);
    match output.circuit {
        Some(circuit) => circuit,
        None => panic!("document should compile: {:#?}", output.diagnostics),
    }
}

pub(crate) fn codes(output: &CompileOutput) -> Vec<DiagnosticCode> {
    output.diagnostics.iter().map(|d| d.code()).collect()
}

/// Parse `press S1`, `release S1`, `close Q1`, `open Q1`, `trip F1` or
/// `reset F1` against a compiled circuit.
pub(crate) fn action(circuit: &CompiledCircuit, text: &str) -> InputAction {
    let (verb, reference) = text
        .split_once(' ')
        .expect("action is `<verb> <reference>`");
    let device = circuit
        .device_by_reference(reference)
        .unwrap_or_else(|| panic!("no device {reference}"))
        .id;
    match verb {
        "press" => InputAction::Press(device),
        "release" => InputAction::Release(device),
        "close" => InputAction::SetClosed(device, true),
        "open" => InputAction::SetClosed(device, false),
        "trip" => InputAction::Trip(device),
        "reset" => InputAction::ResetTrip(device),
        other => panic!("unknown verb {other}"),
    }
}

pub(crate) fn simulator(doc: &Schematic, config: SimConfig) -> Simulator {
    Simulator::new(compile_ok(doc), config).unwrap()
}

/// Run from the current tick up to and including `until_tick`, applying each
/// scripted action in its tick. Stops when the runtime latches. Returns the
/// event lines of the trace, without the header.
pub(crate) fn run(sim: &mut Simulator, until_tick: u64, script: &[(u64, &str)]) -> Vec<String> {
    while sim.tick() <= until_tick {
        let tick = sim.tick();
        let actions: Vec<InputAction> = script
            .iter()
            .filter(|(at, _)| *at == tick)
            .map(|(_, text)| action(sim.circuit(), text))
            .collect();
        match sim.step(&actions) {
            Ok(outcome) if outcome.faulted => break,
            Ok(_) => {}
            Err(SimulationError::AlreadyFaulted) => break,
            Err(error) => panic!("step failed at tick {tick}: {error}"),
        }
    }
    let events = sim.take_trace();
    format_trace(sim.circuit(), &sim.trace_header(), &events)
        .lines()
        .skip(3)
        .map(str::to_owned)
        .collect()
}

/// Control circuit of a direct-online starter: `P24 -> S2 (NC) -> [S1 (NO)
/// in parallel with the K1 seal-in contact] -> K1 coil -> P0`.
pub(crate) fn command_circuit() -> Schematic {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let s2 = rig.part("S2", "pushbutton_nc", "contact", &[]);
    let s1 = rig.part("S1", "pushbutton_no", "contact", &[]);
    let k1 = rig.device("K1", "contactor.generic", &dc_load());
    let coil = rig.symbol(k1, "coil", None);
    let seal = rig.symbol(k1, "aux_no", Some(1));
    let top = rig.junction();
    let bottom = rig.junction();

    rig.wire(out(p24), pin(s2, "p1"));
    rig.wire(pin(s2, "p2"), top);
    rig.wire(top, pin(s1, "p1"));
    rig.wire(top, pin(seal, "p1"));
    rig.wire(pin(s1, "p2"), bottom);
    rig.wire(pin(seal, "p2"), bottom);
    rig.wire(bottom, pin(coil, "a1"));
    rig.wire(pin(coil, "a2"), out(p0));
    rig.finish()
}

/// Direct-online starter with its power circuit: main switch Q1, contactor
/// K1, thermal overload F1 and motor M1 on a 400 V supply, with the 24 V
/// control circuit of [`command_circuit`] plus the F1 trip contact.
pub(crate) fn direct_online_circuit() -> Schematic {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let phases = rig.three_phase();

    let q1 = rig.part("Q1", "main_switch.3p", "main", &[]);
    let f1 = rig.device("F1", "thermal_overload.generic", &[]);
    let element = rig.symbol(f1, "element", None);
    let trip_contact = rig.symbol(f1, "aux_nc", None);
    let s2 = rig.part("S2", "pushbutton_nc", "contact", &[]);
    let s1 = rig.part("S1", "pushbutton_no", "contact", &[]);
    let k1 = rig.device("K1", "contactor.generic", &dc_load());
    let coil = rig.symbol(k1, "coil", None);
    let main = rig.symbol(k1, "main", None);
    let seal = rig.symbol(k1, "aux_no", Some(1));
    let motor = rig.part(
        "M1",
        "motor.3ph",
        "motor",
        &[("rated_voltage_mv", ParamValue::Int(400_000))],
    );

    // Control: P24 -> F1 95/96 -> S2 -> [S1 || K1 seal] -> K1 coil -> P0.
    let top = rig.junction();
    let bottom = rig.junction();
    rig.wire(out(p24), pin(trip_contact, "p1"));
    rig.wire(pin(trip_contact, "p2"), pin(s2, "p1"));
    rig.wire(pin(s2, "p2"), top);
    rig.wire(top, pin(s1, "p1"));
    rig.wire(top, pin(seal, "p1"));
    rig.wire(pin(s1, "p2"), bottom);
    rig.wire(pin(seal, "p2"), bottom);
    rig.wire(bottom, pin(coil, "a1"));
    rig.wire(pin(coil, "a2"), out(p0));

    // Power: L1..L3 -> Q1 -> K1 -> F1 -> M1.
    for (index, (phase, terminal)) in phases.into_iter().zip(["u1", "v1", "w1"]).enumerate() {
        let pole = index + 1;
        rig.wire(out(phase), pin(q1, &format!("pole{pole}.in")));
        rig.wire(
            pin(q1, &format!("pole{pole}.out")),
            pin(main, &format!("pole{pole}.in")),
        );
        rig.wire(
            pin(main, &format!("pole{pole}.out")),
            pin(element, &format!("pole{pole}.in")),
        );
        rig.wire(
            pin(element, &format!("pole{pole}.out")),
            pin(motor, terminal),
        );
    }
    rig.finish()
}

/// Reversing starter: K1 runs the motor forward, K2 swaps L1 and L3. Each
/// coil is interlocked by a normally closed contact of the other contactor.
/// S1 starts forward, S3 starts reverse, S2 stops.
pub(crate) fn reversing_circuit() -> Schematic {
    reversing_with(Interlocks {
        electrical: true,
        mechanical: false,
    })
}

/// Which interlocks a reversing starter has.
#[derive(Clone, Copy)]
pub(crate) struct Interlocks {
    /// Each coil is fed through the NC auxiliary contact of the other.
    pub(crate) electrical: bool,
    /// A mechanical interlock `KX1` joins K1 and K2.
    pub(crate) mechanical: bool,
}

/// The reversing starter of [`reversing_circuit`] with the chosen
/// interlocks. Elements are created in the same order whatever the choice,
/// so the default variant keeps its IDs.
pub(crate) fn reversing_with(interlocks: Interlocks) -> Schematic {
    let mut rig = Rig::new();
    let (p24, p0) = rig.dc24();
    let [l1, l2, l3] = rig.three_phase();

    let s2 = rig.part("S2", "pushbutton_nc", "contact", &[]);
    let s1 = rig.part("S1", "pushbutton_no", "contact", &[]);
    let s3 = rig.part("S3", "pushbutton_no", "contact", &[]);
    let k1 = rig.device("K1", "contactor.generic", &dc_load());
    let k1_coil = rig.symbol(k1, "coil", None);
    let k1_main = rig.symbol(k1, "main", None);
    let k1_seal = rig.symbol(k1, "aux_no", Some(1));
    let k1_lock = interlocks
        .electrical
        .then(|| rig.symbol(k1, "aux_nc", Some(2)));
    let k2 = rig.device("K2", "contactor.generic", &dc_load());
    let k2_coil = rig.symbol(k2, "coil", None);
    let k2_main = rig.symbol(k2, "main", None);
    let k2_seal = rig.symbol(k2, "aux_no", Some(1));
    let k2_lock = interlocks
        .electrical
        .then(|| rig.symbol(k2, "aux_nc", Some(2)));
    let motor = rig.part(
        "M1",
        "motor.3ph",
        "motor",
        &[("rated_voltage_mv", ParamValue::Int(400_000))],
    );

    // Control. `feed` is the node after the stop button.
    let feed = rig.junction();
    let forward = rig.junction();
    let reverse = rig.junction();
    let common = rig.junction();
    rig.wire(out(p24), pin(s2, "p1"));
    rig.wire(pin(s2, "p2"), feed);
    for (start, seal, node, lock, coil) in [
        (s1, k1_seal, forward, k2_lock, k1_coil),
        (s3, k2_seal, reverse, k1_lock, k2_coil),
    ] {
        rig.wire(feed, pin(start, "p1"));
        rig.wire(feed, pin(seal, "p1"));
        rig.wire(pin(start, "p2"), node);
        rig.wire(pin(seal, "p2"), node);
        match lock {
            Some(lock) => {
                rig.wire(node, pin(lock, "p1"));
                rig.wire(pin(lock, "p2"), pin(coil, "a1"));
            }
            None => rig.wire(node, pin(coil, "a1")),
        }
        rig.wire(pin(coil, "a2"), common);
    }
    rig.wire(common, out(p0));

    // Power. K1 passes the phases straight through; K2 swaps L1 and L3.
    let lines = [rig.junction(), rig.junction(), rig.junction()];
    let motor_side = [rig.junction(), rig.junction(), rig.junction()];
    for (phase, line) in [l1, l2, l3].into_iter().zip(lines) {
        rig.wire(out(phase), line);
    }
    for (terminal, node) in ["u1", "v1", "w1"].into_iter().zip(motor_side) {
        rig.wire(node, pin(motor, terminal));
    }
    for pole in 0..3 {
        let key = pole + 1;
        rig.wire(lines[pole], pin(k1_main, &format!("pole{key}.in")));
        rig.wire(pin(k1_main, &format!("pole{key}.out")), motor_side[pole]);
        rig.wire(lines[2 - pole], pin(k2_main, &format!("pole{key}.in")));
        rig.wire(pin(k2_main, &format!("pole{key}.out")), motor_side[pole]);
    }
    if interlocks.mechanical {
        let link = rig.device(
            "KX1",
            "interlock.mechanical",
            &[
                ("first", ParamValue::Int(i64::try_from(k1.0).unwrap())),
                ("second", ParamValue::Int(i64::try_from(k2.0).unwrap())),
            ],
        );
        rig.symbol(link, "link", None);
    }
    rig.finish()
}
