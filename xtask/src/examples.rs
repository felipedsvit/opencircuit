//! Reference circuits, generated into `examples/` and `tests/fixtures/`.
//!
//! The circuits are built in code so that their wiring is reviewed as code
//! and their files are always canonical. The files are committed; a test
//! fails when a file on disk no longer matches its generator.

use circuit_model::builder::pin;
use circuit_model::{
    DeviceId, Endpoint, GridPoint, IdExhausted, JunctionId, ParamValue, PhaseSequence, Schematic,
    SchematicBuilder, SupplyId, SymbolId,
};

/// One generated project file.
pub(crate) struct Example {
    /// Path relative to the workspace root.
    pub(crate) path: &'static str,
    build: fn() -> Result<Schematic, IdExhausted>,
}

impl Example {
    /// The canonical text of the project file.
    pub(crate) fn render(&self) -> Result<String, String> {
        let doc = (self.build)().map_err(|error| format!("{}: {error}", self.path))?;
        circuit_project::to_string(&doc).map_err(|error| format!("{}: {error}", self.path))
    }
}

/// Every generated project file.
pub(crate) const EXAMPLES: &[Example] = &[
    Example {
        path: "examples/direct-starter/command.ocircuit",
        build: command,
    },
    Example {
        path: "examples/direct-starter/starter.ocircuit",
        build: starter,
    },
    Example {
        path: "examples/forward-reverse/reversing.ocircuit",
        build: reversing,
    },
    Example {
        path: "tests/fixtures/static-short/static-short.ocircuit",
        build: static_short,
    },
    Example {
        path: "tests/fixtures/series-loads/series-loads.ocircuit",
        build: series_loads,
    },
];

type Built<T> = Result<T, IdExhausted>;

/// A builder with explicit sheet positions.
struct Sheet(SchematicBuilder);

impl Sheet {
    fn new(name: &str) -> Self {
        Self(SchematicBuilder::new(name))
    }

    fn device(
        &mut self,
        reference: &str,
        model: &str,
        params: &[(&str, ParamValue)],
    ) -> Built<DeviceId> {
        self.0.device(reference, model, params)
    }

    fn symbol(
        &mut self,
        device: DeviceId,
        key: &str,
        slot: Option<u8>,
        at: (i32, i32),
    ) -> Built<SymbolId> {
        self.0.symbol(device, key, slot, GridPoint::new(at.0, at.1))
    }

    /// A device together with its only symbol.
    fn part(
        &mut self,
        reference: &str,
        model: &str,
        key: &str,
        params: &[(&str, ParamValue)],
        at: (i32, i32),
    ) -> Built<SymbolId> {
        let device = self.device(reference, model, params)?;
        self.symbol(device, key, None, at)
    }

    /// A supply terminal with one tap.
    fn tap(
        &mut self,
        reference: &str,
        conductor: &str,
        supply: SupplyId,
        at: (i32, i32),
    ) -> Built<SymbolId> {
        let supply = i64::try_from(supply.0).map_err(|_| IdExhausted)?;
        self.part(
            reference,
            "supply.terminal",
            "terminal",
            &[
                ("conductor", ParamValue::Text(conductor.to_owned())),
                ("supply", ParamValue::Int(supply)),
            ],
            at,
        )
    }

    fn junction(&mut self, at: (i32, i32)) -> Built<JunctionId> {
        self.0.junction(GridPoint::new(at.0, at.1))
    }

    fn wire(&mut self, from: impl Into<Endpoint>, to: impl Into<Endpoint>) -> Built<()> {
        self.0.wire(from, to).map(|_| ())
    }

    /// A wire through waypoints, so that every segment is horizontal or
    /// vertical.
    fn route(
        &mut self,
        from: impl Into<Endpoint>,
        to: impl Into<Endpoint>,
        via: &[(i32, i32)],
    ) -> Built<()> {
        let route: Vec<GridPoint> = via.iter().map(|&(x, y)| GridPoint::new(x, y)).collect();
        self.0.wire_routed(from, to, &route).map(|_| ())
    }

    /// 24 V DC control supply with taps `P24` and `P0`.
    fn control_supply(
        &mut self,
        top: (i32, i32),
        bottom: (i32, i32),
    ) -> Built<(SymbolId, SymbolId)> {
        let supply = self.0.dc_supply(24_000)?;
        Ok((
            self.tap("P24", "dc_pos", supply, top)?,
            self.tap("P0", "dc_neg", supply, bottom)?,
        ))
    }

    /// 400/230 V, 50 Hz power supply with taps `L1`, `L2` and `L3`.
    fn power_supply(&mut self, left: i32, y: i32) -> Built<[SymbolId; 3]> {
        let supply =
            self.0
                .three_phase_supply(400_000, Some(230_000), 50_000, PhaseSequence::L1L2L3)?;
        Ok([
            self.tap("L1", "l1", supply, (left, y))?,
            self.tap("L2", "l2", supply, (left.saturating_add(4), y))?,
            self.tap("L3", "l3", supply, (left.saturating_add(8), y))?,
        ])
    }

    fn finish(self) -> Schematic {
        self.0.finish()
    }
}

fn out(tap: SymbolId) -> Endpoint {
    pin(tap, "out")
}

fn dc_load() -> [(&'static str, ParamValue); 2] {
    [
        ("rated_voltage_mv", ParamValue::Int(24_000)),
        ("connection", ParamValue::Text("dc".to_owned())),
    ]
}

fn motor_400v() -> [(&'static str, ParamValue); 1] {
    [("rated_voltage_mv", ParamValue::Int(400_000))]
}

const MOTOR_TERMINALS: [&str; 3] = ["u1", "v1", "w1"];
const POLES: [(&str, &str); 3] = [
    ("pole1.in", "pole1.out"),
    ("pole2.in", "pole2.out"),
    ("pole3.in", "pole3.out"),
];

/// Control circuit of a direct-online starter: `P24 -> S2 (NC) -> [S1 (NO)
/// in parallel with the K1 seal-in contact] -> K1 coil -> P0`.
fn command() -> Built<Schematic> {
    let mut sheet = Sheet::new("Direct-online starter - control circuit");
    let (p24, p0) = sheet.control_supply((0, 0), (0, 22))?;
    let stop = sheet.part("S2", "pushbutton_nc", "contact", &[], (0, 4))?;
    let start = sheet.part("S1", "pushbutton_no", "contact", &[], (-4, 11))?;
    let k1 = sheet.device("K1", "contactor.generic", &dc_load())?;
    let coil = sheet.symbol(k1, "coil", None, (0, 18))?;
    let seal = sheet.symbol(k1, "aux_no", Some(1), (4, 11))?;
    let top = sheet.junction((0, 8))?;
    let bottom = sheet.junction((0, 14))?;

    sheet.wire(out(p24), pin(stop, "p1"))?;
    sheet.wire(pin(stop, "p2"), top)?;
    sheet.route(top, pin(start, "p1"), &[(-4, 8)])?;
    sheet.route(top, pin(seal, "p1"), &[(4, 8)])?;
    sheet.route(pin(start, "p2"), bottom, &[(-4, 14)])?;
    sheet.route(pin(seal, "p2"), bottom, &[(4, 14)])?;
    sheet.wire(bottom, pin(coil, "a1"))?;
    sheet.wire(pin(coil, "a2"), out(p0))?;
    Ok(sheet.finish())
}

/// Direct-online starter with its power circuit: main switch Q1, contactor
/// K1, thermal overload F1 and motor M1. The trip contact of F1 is in series
/// with the control circuit.
fn starter() -> Built<Schematic> {
    let mut sheet = Sheet::new("Direct-online starter");
    let (p24, p0) = sheet.control_supply((0, 0), (0, 28))?;
    let phases = sheet.power_supply(20, 0)?;

    let q1 = sheet.part("Q1", "main_switch.3p", "main", &[], (24, 4))?;
    let f1 = sheet.device("F1", "thermal_overload.generic", &[])?;
    let element = sheet.symbol(f1, "element", None, (24, 16))?;
    let trip = sheet.symbol(f1, "aux_nc", None, (0, 4))?;
    let stop = sheet.part("S2", "pushbutton_nc", "contact", &[], (0, 10))?;
    let start = sheet.part("S1", "pushbutton_no", "contact", &[], (-4, 17))?;
    let k1 = sheet.device("K1", "contactor.generic", &dc_load())?;
    let coil = sheet.symbol(k1, "coil", None, (0, 24))?;
    let main = sheet.symbol(k1, "main", None, (24, 10))?;
    let seal = sheet.symbol(k1, "aux_no", Some(1), (4, 17))?;
    let motor = sheet.part("M1", "motor.3ph", "motor", &motor_400v(), (24, 22))?;

    let top = sheet.junction((0, 14))?;
    let bottom = sheet.junction((0, 20))?;
    sheet.wire(out(p24), pin(trip, "p1"))?;
    sheet.wire(pin(trip, "p2"), pin(stop, "p1"))?;
    sheet.wire(pin(stop, "p2"), top)?;
    sheet.route(top, pin(start, "p1"), &[(-4, 14)])?;
    sheet.route(top, pin(seal, "p1"), &[(4, 14)])?;
    sheet.route(pin(start, "p2"), bottom, &[(-4, 20)])?;
    sheet.route(pin(seal, "p2"), bottom, &[(4, 20)])?;
    sheet.wire(bottom, pin(coil, "a1"))?;
    sheet.wire(pin(coil, "a2"), out(p0))?;

    for ((phase, (input, output)), terminal) in phases.into_iter().zip(POLES).zip(MOTOR_TERMINALS) {
        sheet.wire(out(phase), pin(q1, input))?;
        sheet.wire(pin(q1, output), pin(main, input))?;
        sheet.wire(pin(main, output), pin(element, input))?;
        sheet.wire(pin(element, output), pin(motor, terminal))?;
    }
    Ok(sheet.finish())
}

/// Reversing starter. K1 runs the motor forward; K2 swaps L1 and L3. Each
/// coil is interlocked by a normally closed contact of the other contactor.
/// S1 starts forward, S3 starts reverse and S2 stops.
fn reversing() -> Built<Schematic> {
    let mut sheet = Sheet::new("Reversing starter");
    let (p24, p0) = sheet.control_supply((0, 0), (0, 30))?;
    let phases = sheet.power_supply(30, 0)?;

    let stop = sheet.part("S2", "pushbutton_nc", "contact", &[], (0, 3))?;
    let start_forward = sheet.part("S1", "pushbutton_no", "contact", &[], (-12, 11))?;
    let start_reverse = sheet.part("S3", "pushbutton_no", "contact", &[], (12, 11))?;
    let k1 = sheet.device("K1", "contactor.generic", &dc_load())?;
    let k1_coil = sheet.symbol(k1, "coil", None, (-8, 24))?;
    let k1_main = sheet.symbol(k1, "main", None, (34, 10))?;
    let k1_seal = sheet.symbol(k1, "aux_no", Some(1), (-4, 11))?;
    let k1_lock = sheet.symbol(k1, "aux_nc", Some(2), (8, 18))?;
    let k2 = sheet.device("K2", "contactor.generic", &dc_load())?;
    let k2_coil = sheet.symbol(k2, "coil", None, (8, 24))?;
    let k2_main = sheet.symbol(k2, "main", None, (50, 10))?;
    let k2_seal = sheet.symbol(k2, "aux_no", Some(1), (4, 11))?;
    let k2_lock = sheet.symbol(k2, "aux_nc", Some(2), (-8, 18))?;
    let motor = sheet.part("M1", "motor.3ph", "motor", &motor_400v(), (34, 21))?;

    // Control. `feed` is the node after the stop button. Each branch is a
    // column: the start button and the seal-in contact side by side, then
    // the interlock contact and the coil under their common node at `x`.
    let feed = sheet.junction((0, 7))?;
    let forward = sheet.junction((-8, 15))?;
    let reverse = sheet.junction((8, 15))?;
    let common = sheet.junction((0, 28))?;
    sheet.wire(out(p24), pin(stop, "p1"))?;
    sheet.wire(pin(stop, "p2"), feed)?;
    for (start, seal, node, lock, coil, (start_x, seal_x, x)) in [
        (
            start_forward,
            k1_seal,
            forward,
            k2_lock,
            k1_coil,
            (-12, -4, -8),
        ),
        (
            start_reverse,
            k2_seal,
            reverse,
            k1_lock,
            k2_coil,
            (12, 4, 8),
        ),
    ] {
        sheet.route(feed, pin(start, "p1"), &[(start_x, 7)])?;
        sheet.route(feed, pin(seal, "p1"), &[(seal_x, 7)])?;
        sheet.route(pin(start, "p2"), node, &[(start_x, 15)])?;
        sheet.route(pin(seal, "p2"), node, &[(seal_x, 15)])?;
        sheet.wire(node, pin(lock, "p1"))?;
        sheet.wire(pin(lock, "p2"), pin(coil, "a1"))?;
        sheet.route(pin(coil, "a2"), common, &[(x, 28)])?;
    }
    sheet.wire(common, out(p0))?;

    // Power. K1 passes the phases straight down; K2 swaps L1 and L3. Its
    // feeders leave the line junctions on separate rows above them, and its
    // outputs return to the motor-side junctions on separate rows below K1,
    // so different phases only cross and never run along each other.
    let lines = [
        sheet.junction((30, 4))?,
        sheet.junction((34, 4))?,
        sheet.junction((38, 4))?,
    ];
    let motor_side = [
        sheet.junction((30, 17))?,
        sheet.junction((34, 17))?,
        sheet.junction((38, 17))?,
    ];
    for (phase, line) in phases.into_iter().zip(lines) {
        sheet.wire(out(phase), line)?;
    }
    for (node, terminal) in motor_side.into_iter().zip(MOTOR_TERMINALS) {
        sheet.wire(node, pin(motor, terminal))?;
    }
    let swapped = [lines[2], lines[1], lines[0]];
    // Per K2 pole: the waypoints of its feeder and of its output.
    type Waypoints = [(i32, i32); 2];
    let k2_routes: [(Waypoints, Waypoints); 3] = [
        ([(38, 3), (46, 3)], [(46, 14), (30, 14)]),
        ([(34, 2), (50, 2)], [(50, 15), (34, 15)]),
        ([(30, 1), (54, 1)], [(54, 16), (38, 16)]),
    ];
    for ((((straight, crossed), node), (input, output)), (feeder, back)) in lines
        .into_iter()
        .zip(swapped)
        .zip(motor_side)
        .zip(POLES)
        .zip(k2_routes)
    {
        sheet.wire(straight, pin(k1_main, input))?;
        sheet.wire(pin(k1_main, output), node)?;
        sheet.route(crossed, pin(k2_main, input), &feeder)?;
        sheet.route(pin(k2_main, output), node, &back)?;
    }
    Ok(sheet.finish())
}

/// A lamp across the supply with the two poles also wired together: a
/// permanent short circuit, rejected at compile time.
fn static_short() -> Built<Schematic> {
    let mut sheet = Sheet::new("Fixture - permanent short circuit");
    let (p24, p0) = sheet.control_supply((0, 0), (0, 12))?;
    let lamp = sheet.part("H1", "lamp.generic", "lamp", &dc_load(), (0, 6))?;
    let top = sheet.junction((0, 3))?;
    let bottom = sheet.junction((0, 9))?;
    sheet.wire(out(p24), top)?;
    sheet.wire(top, pin(lamp, "x1"))?;
    sheet.wire(pin(lamp, "x2"), bottom)?;
    sheet.wire(bottom, out(p0))?;
    sheet.route(top, bottom, &[(4, 3), (4, 9)])?;
    Ok(sheet.finish())
}

/// Two lamps wired in series: outside the discrete model, rejected at
/// compile time instead of being approximated.
fn series_loads() -> Built<Schematic> {
    let mut sheet = Sheet::new("Fixture - loads in series");
    let (p24, p0) = sheet.control_supply((0, 0), (0, 14))?;
    let upper = sheet.part("H1", "lamp.generic", "lamp", &dc_load(), (0, 4))?;
    let lower = sheet.part("H2", "lamp.generic", "lamp", &dc_load(), (0, 10))?;
    sheet.wire(out(p24), pin(upper, "x1"))?;
    sheet.wire(pin(upper, "x2"), pin(lower, "x1"))?;
    sheet.wire(pin(lower, "x2"), out(p0))?;
    Ok(sheet.finish())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn every_example_is_canonical_and_loads_back_unchanged() {
        for example in EXAMPLES {
            let text = example.render().unwrap();
            let loaded = circuit_project::from_str(&text).unwrap();

            assert_eq!(circuit_project::to_string(&loaded).unwrap(), text);
        }
    }

    #[test]
    fn examples_in_the_examples_directory_are_structurally_valid() {
        for example in EXAMPLES
            .iter()
            .filter(|example| example.path.starts_with("examples/"))
        {
            let doc = (example.build)().unwrap();

            assert_eq!(doc.validate(), [], "{}", example.path);
        }
    }

    #[test]
    fn every_generated_wire_resolves_and_is_orthogonal() {
        let catalog = circuit_model::ComponentCatalog::builtin();
        for example in EXAMPLES {
            let doc = (example.build)().unwrap();
            let layout = circuit_model::LayoutIndex::new(&doc, &catalog);
            for wire in &doc.wires {
                let path = layout.wire_polyline(wire).unwrap_or_else(|| {
                    panic!("{}: wire {} does not resolve", example.path, wire.id)
                });

                assert!(
                    circuit_model::is_orthogonal(&path),
                    "{}: wire {} is not orthogonal: {path:?}",
                    example.path,
                    wire.id
                );
            }
        }
    }

    #[test]
    fn symbols_of_a_generated_project_do_not_overlap() {
        let catalog = circuit_model::ComponentCatalog::builtin();
        for example in EXAMPLES {
            let doc = (example.build)().unwrap();
            let layout = circuit_model::LayoutIndex::new(&doc, &catalog);
            let placed: Vec<_> = doc
                .symbols
                .iter()
                .map(|symbol| (symbol.id, layout.symbol_bounds(symbol.id).unwrap()))
                .collect();
            for (index, (id, bounds)) in placed.iter().enumerate() {
                for (other, other_bounds) in &placed[index + 1..] {
                    assert!(
                        !bounds.overlaps(other_bounds),
                        "{}: symbols {id} and {other} overlap",
                        example.path
                    );
                }
            }
        }
    }

    #[test]
    fn files_on_disk_match_their_generators() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives one level below the workspace root");
        for example in EXAMPLES {
            let on_disk =
                std::fs::read_to_string(root.join(example.path)).unwrap_or_else(|error| {
                    panic!(
                        "{}: {error}; run `cargo run -p xtask -- generate-examples`",
                        example.path
                    )
                });

            assert_eq!(
                on_disk,
                example.render().unwrap(),
                "{} is stale; run `cargo run -p xtask -- generate-examples`",
                example.path
            );
        }
    }
}
