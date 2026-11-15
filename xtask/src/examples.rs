//! Reference circuits, generated into `examples/` and `tests/fixtures/`.
//!
//! The circuits are built in code so that their wiring is reviewed as code
//! and their files are always canonical. The files are committed; a test
//! fails when a file on disk no longer matches its generator.

use circuit_model::builder::pin;
use circuit_model::{
    DeviceId, Endpoint, GridPoint, IdExhausted, JunctionId, ParamValue, PhaseSequence, QuarterTurn,
    Schematic, SchematicBuilder, SupplyId, SymbolId,
};

/// One generated project file.
pub(crate) struct Example {
    /// Path relative to the workspace root.
    pub(crate) path: &'static str,
    build: fn() -> Result<Schematic, IdExhausted>,
    /// Text of the extension library the project uses, if any.
    library: Option<&'static str>,
}

/// The example library of `LIBRARY.md`, committed next to the project that
/// uses it.
const ACME_LIBRARY: &str = include_str!("../../examples/library/acme.oclib");

impl Example {
    /// The canonical text of the project file.
    pub(crate) fn render(&self) -> Result<String, String> {
        let doc = (self.build)().map_err(|error| format!("{}: {error}", self.path))?;
        circuit_project::to_string(&doc).map_err(|error| format!("{}: {error}", self.path))
    }

    /// The drawings of an example under `examples/`, at rest, in the print
    /// theme, next to its project file: SVG and PDF, the goldens of the
    /// exporters. Fixtures have none.
    pub(crate) fn drawings(&self) -> Vec<Result<(String, Vec<u8>), String>> {
        let Some(stem) = self
            .path
            .strip_prefix("examples/")
            .and_then(|rest| rest.strip_suffix(".ocircuit"))
        else {
            return Vec::new();
        };
        let doc = match (self.build)() {
            Ok(doc) => doc,
            Err(error) => return vec![Err(format!("{}: {error}", self.path))],
        };
        let catalog = match self.catalog() {
            Ok(catalog) => catalog,
            Err(error) => return vec![Err(error)],
        };
        let scene = circuit_renderer::build_scene(
            &doc,
            &catalog,
            None,
            &circuit_renderer::SceneOptions::default(),
        );
        let theme = circuit_export::Theme::Print;
        vec![
            Ok((
                format!("examples/{stem}.svg"),
                circuit_export::svg(&scene, theme).into_bytes(),
            )),
            Ok((
                format!("examples/{stem}.pdf"),
                circuit_export::pdf(&scene, theme),
            )),
        ]
    }
}

impl Example {
    /// The catalog the project is drawn and checked with: the built-in one,
    /// plus the example's library when it uses one.
    pub(crate) fn catalog(&self) -> Result<circuit_model::ComponentCatalog, String> {
        let builtin = circuit_model::ComponentCatalog::builtin();
        let Some(text) = self.library else {
            return Ok(builtin);
        };
        let library = circuit_project::library::from_str(text)
            .map_err(|error| format!("{}: {error}", self.path))?;
        builtin
            .with_library(library)
            .map_err(|error| format!("{}: {error}", self.path))
    }
}

/// Every generated file, project files then drawings, as `(path, bytes)`.
pub(crate) fn outputs() -> Vec<Result<(String, Vec<u8>), String>> {
    let projects = EXAMPLES.iter().map(|example| {
        example
            .render()
            .map(|text| (example.path.to_owned(), text.into_bytes()))
    });
    let drawings = EXAMPLES.iter().flat_map(Example::drawings);
    projects.chain(drawings).collect()
}

/// Every generated project file.
pub(crate) const EXAMPLES: &[Example] = &[
    Example {
        path: "examples/direct-starter/command.ocircuit",
        build: command,
        library: None,
    },
    Example {
        path: "examples/direct-starter/starter.ocircuit",
        build: starter,
        library: None,
    },
    Example {
        path: "examples/forward-reverse/reversing.ocircuit",
        build: reversing,
        library: None,
    },
    Example {
        path: "examples/forward-reverse/interlocked.ocircuit",
        build: interlocked,
        library: None,
    },
    Example {
        path: "examples/star-delta/star-delta.ocircuit",
        build: star_delta,
        library: None,
    },
    Example {
        path: "tests/fixtures/star-delta-overlap/star-delta-overlap.ocircuit",
        build: star_delta_overlap,
        library: None,
    },
    Example {
        path: "examples/multi-section/multi-section.ocircuit",
        build: multi_section,
        library: None,
    },
    Example {
        path: "examples/control-transformer/control-transformer.ocircuit",
        build: control_transformer,
        library: None,
    },
    Example {
        path: "examples/pump-level/pump-level.ocircuit",
        build: pump_level,
        library: None,
    },
    Example {
        path: "examples/library/acme-lamp.ocircuit",
        build: acme_lamp,
        library: Some(ACME_LIBRARY),
    },
    Example {
        path: "tests/fixtures/sections-bridged/sections-bridged.ocircuit",
        build: sections_bridged,
        library: None,
    },
    Example {
        path: "tests/fixtures/static-short/static-short.ocircuit",
        build: static_short,
        library: None,
    },
    Example {
        path: "tests/fixtures/series-loads/series-loads.ocircuit",
        build: series_loads,
        library: None,
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

    fn device_version(
        &mut self,
        reference: &str,
        model: &str,
        version: u32,
        params: &[(&str, ParamValue)],
    ) -> Built<DeviceId> {
        self.0.device_version(reference, model, version, params)
    }

    fn symbol_turned(
        &mut self,
        device: DeviceId,
        key: &str,
        slot: Option<u8>,
        at: (i32, i32),
        rotation: QuarterTurn,
    ) -> Built<SymbolId> {
        self.0
            .symbol_turned(device, key, slot, GridPoint::new(at.0, at.1), rotation)
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

    reversing_power(&mut sheet, phases, k1_main, k2_main, motor)?;
    Ok(sheet.finish())
}

/// Power circuit of a reversing starter: K1 passes the phases straight down
/// to the motor, K2 swaps L1 and L3.
fn reversing_power(
    sheet: &mut Sheet,
    phases: [SymbolId; 3],
    k1_main: SymbolId,
    k2_main: SymbolId,
    motor: SymbolId,
) -> Built<()> {
    // The feeders of K2 leave the line junctions on separate rows above
    // them, and its outputs return to the motor-side junctions on separate
    // rows below K1, so different phases only cross and never run along
    // each other.
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
    Ok(())
}

/// Reversing starter with full interlocking: each start button is a
/// combined pushbutton whose NC contact opens the other direction, each coil
/// is fed through the NC auxiliary contact of the other contactor, and a
/// mechanical interlock `KX1` keeps K1 and K2 from being picked up together.
/// Pressing the other start button reverses without pressing stop.
fn interlocked() -> Built<Schematic> {
    let mut sheet = Sheet::new("Reversing starter with full interlocking");
    let (p24, p0) = sheet.control_supply((0, 0), (0, 39))?;
    let phases = sheet.power_supply(30, 0)?;

    let stop = sheet.part("S2", "pushbutton_nc", "contact", &[], (0, 3))?;
    let s1 = sheet.device("S1", "pushbutton.combined", &[])?;
    let s1_start = sheet.symbol(s1, "no", None, (-12, 11))?;
    let s1_block = sheet.symbol(s1, "nc", None, (8, 19))?;
    let s3 = sheet.device("S3", "pushbutton.combined", &[])?;
    let s3_start = sheet.symbol(s3, "no", None, (12, 11))?;
    let s3_block = sheet.symbol(s3, "nc", None, (-8, 19))?;
    let k1 = sheet.device("K1", "contactor.generic", &dc_load())?;
    let k1_coil = sheet.symbol(k1, "coil", None, (-8, 31))?;
    let k1_main = sheet.symbol(k1, "main", None, (34, 10))?;
    let k1_seal = sheet.symbol(k1, "aux_no", Some(1), (-4, 11))?;
    let k1_lock = sheet.symbol(k1, "aux_nc", Some(2), (8, 25))?;
    let k2 = sheet.device("K2", "contactor.generic", &dc_load())?;
    let k2_coil = sheet.symbol(k2, "coil", None, (8, 31))?;
    let k2_main = sheet.symbol(k2, "main", None, (50, 10))?;
    let k2_seal = sheet.symbol(k2, "aux_no", Some(1), (4, 11))?;
    let k2_lock = sheet.symbol(k2, "aux_nc", Some(2), (-8, 25))?;
    let motor = sheet.part("M1", "motor.3ph", "motor", &motor_400v(), (34, 21))?;
    let id = |device: DeviceId| i64::try_from(device.0).map_err(|_| IdExhausted);
    sheet.part(
        "KX1",
        "interlock.mechanical",
        "link",
        &[
            ("first", ParamValue::Int(id(k1)?)),
            ("second", ParamValue::Int(id(k2)?)),
        ],
        (0, 31),
    )?;

    // Control. Each direction is a column under `feed`: its start button
    // beside its seal-in contact, then the NC contact of the other button,
    // the NC auxiliary contact of the other contactor and the coil.
    let feed = sheet.junction((0, 7))?;
    let forward = sheet.junction((-8, 15))?;
    let reverse = sheet.junction((8, 15))?;
    let common = sheet.junction((0, 36))?;
    sheet.wire(out(p24), pin(stop, "p1"))?;
    sheet.wire(pin(stop, "p2"), feed)?;
    for (start, seal, node, block, lock, coil, (start_x, seal_x, x)) in [
        (
            s1_start,
            k1_seal,
            forward,
            s3_block,
            k2_lock,
            k1_coil,
            (-12, -4, -8),
        ),
        (
            s3_start,
            k2_seal,
            reverse,
            s1_block,
            k1_lock,
            k2_coil,
            (12, 4, 8),
        ),
    ] {
        sheet.route(feed, pin(start, "p1"), &[(start_x, 7)])?;
        sheet.route(feed, pin(seal, "p1"), &[(seal_x, 7)])?;
        sheet.route(pin(start, "p2"), node, &[(start_x, 15)])?;
        sheet.route(pin(seal, "p2"), node, &[(seal_x, 15)])?;
        sheet.wire(node, pin(block, "p1"))?;
        sheet.wire(pin(block, "p2"), pin(lock, "p1"))?;
        sheet.wire(pin(lock, "p2"), pin(coil, "a1"))?;
        sheet.route(pin(coil, "a2"), common, &[(x, 36)])?;
    }
    sheet.wire(common, out(p0))?;

    reversing_power(&mut sheet, phases, k1_main, k2_main, motor)?;
    Ok(sheet.finish())
}

/// Star-delta starter, the M3 reference (spec §17.2): KM1 line, KM2 delta,
/// KM3 star and timer KT1.
///
/// S1 energizes KM3 (star) through the resting side of the KT1 changeover;
/// KM3 brings in KM1, which seals itself and keeps KT1 running. When KT1
/// runs out, its changeover drops KM3 and, once KM3 is out, brings in KM2
/// (delta). With `interlocked`, KM3 is fed through an NC auxiliary of KM2,
/// KM2 through an NC auxiliary of KM3, and a mechanical interlock joins
/// them; without, they are not interlocked at all.
fn star_delta_with(interlocked: bool, name: &str) -> Built<Schematic> {
    let mut sheet = Sheet::new(name);
    let (p24, p0) = sheet.control_supply((0, 0), (0, 54))?;
    let phases = sheet.power_supply(40, 0)?;
    let timer = [
        ("rated_voltage_mv", ParamValue::Int(24_000)),
        ("connection", ParamValue::Text("dc".to_owned())),
        ("preset_ms", ParamValue::Int(10)),
    ];

    let q1 = sheet.part(
        "Q1",
        "main_switch.3p",
        "main",
        &[("initial_closed", ParamValue::Bool(true))],
        (44, 5),
    )?;
    let f1 = sheet.device("F1", "thermal_overload.generic", &[])?;
    let f1_element = sheet.symbol(f1, "element", None, (44, 11))?;
    let f1_contact = sheet.symbol(f1, "aux_nc", None, (0, 3))?;
    let stop = sheet.part("S0", "pushbutton_nc", "contact", &[], (0, 8))?;
    let start = sheet.part("S1", "pushbutton_no", "contact", &[], (-4, 16))?;
    let km1 = sheet.device("KM1", "contactor.generic", &dc_load())?;
    let km1_coil = sheet.symbol(km1, "coil", None, (16, 36))?;
    let km1_main = sheet.symbol(km1, "main", None, (44, 17))?;
    let km1_seal = sheet.symbol(km1, "aux_no", Some(1), (4, 16))?;
    let km1_hold = sheet.symbol(km1, "aux_no", Some(2), (20, 27))?;
    let km2 = sheet.device("KM2", "contactor.generic", &dc_load())?;
    let km2_coil = sheet.symbol(km2, "coil", None, (8, 42))?;
    let km2_main = sheet.symbol(km2, "main", None, (60, 30))?;
    let km2_lock = if interlocked {
        Some(sheet.symbol(km2, "aux_nc", Some(1), (-4, 35))?)
    } else {
        None
    };
    let km3 = sheet.device("KM3", "contactor.generic", &dc_load())?;
    let km3_coil = sheet.symbol(km3, "coil", None, (-4, 42))?;
    let km3_main = sheet.symbol(km3, "main", None, (44, 48))?;
    let km3_start = sheet.symbol(km3, "aux_no", Some(1), (14, 27))?;
    let km3_lock = if interlocked {
        Some(sheet.symbol(km3, "aux_nc", Some(2), (8, 35))?)
    } else {
        None
    };
    let kt1 = sheet.device_version("KT1", "timer_on.generic", 2, &timer)?;
    let kt1_coil = sheet.symbol(kt1, "coil", None, (-12, 30))?;
    // Turned upside down: `com` on top, `no` below it, `nc` below left.
    let kt1_changeover =
        sheet.symbol_turned(kt1, "delayed_co", Some(1), (2, 27), QuarterTurn::Deg180)?;
    let motor = sheet.part("M1", "motor.3ph.6t", "motor", &motor_400v(), (44, 30))?;
    if interlocked {
        let id = |device: DeviceId| i64::try_from(device.0).map_err(|_| IdExhausted);
        sheet.part(
            "KX1",
            "interlock.mechanical",
            "link",
            &[
                ("first", ParamValue::Int(id(km2)?)),
                ("second", ParamValue::Int(id(km3)?)),
            ],
            (2, 42),
        )?;
    }

    // Control: P24, the thermal relay contact, stop, then start beside the
    // seal-in of KM1 into node `a`, and a bus from `a` to the rungs.
    let feed = sheet.junction((0, 12))?;
    let a = sheet.junction((0, 20))?;
    let bus = sheet.junction((0, 22))?;
    let to_timer = sheet.junction((-12, 22))?;
    let to_changeover = sheet.junction((2, 22))?;
    let to_line = sheet.junction((14, 22))?;
    let line_node = sheet.junction((16, 31))?;
    let common_timer = sheet.junction((-12, 50))?;
    let common_star = sheet.junction((-4, 50))?;
    let common = sheet.junction((0, 50))?;
    let common_delta = sheet.junction((8, 50))?;
    let common_line = sheet.junction((16, 50))?;
    sheet.wire(out(p24), pin(f1_contact, "p1"))?;
    sheet.wire(pin(f1_contact, "p2"), pin(stop, "p1"))?;
    sheet.wire(pin(stop, "p2"), feed)?;
    sheet.route(feed, pin(start, "p1"), &[(-4, 12)])?;
    sheet.route(feed, pin(km1_seal, "p1"), &[(4, 12)])?;
    sheet.route(pin(start, "p2"), a, &[(-4, 20)])?;
    sheet.route(pin(km1_seal, "p2"), a, &[(4, 20)])?;
    sheet.wire(a, bus)?;
    sheet.wire(bus, to_timer)?;
    sheet.wire(bus, to_changeover)?;
    sheet.wire(to_changeover, to_line)?;
    // Timer.
    sheet.wire(to_timer, pin(kt1_coil, "a1"))?;
    sheet.wire(pin(kt1_coil, "a2"), common_timer)?;
    // Star: the resting side of the changeover, then KM2 NC, then KM3.
    sheet.wire(to_changeover, pin(kt1_changeover, "com"))?;
    let star_feed = pin(kt1_changeover, "nc");
    match km2_lock {
        Some(lock) => {
            sheet.route(star_feed, pin(lock, "p1"), &[(0, 31), (-4, 31)])?;
            sheet.wire(pin(lock, "p2"), pin(km3_coil, "a1"))?;
        }
        None => sheet.route(star_feed, pin(km3_coil, "a1"), &[(0, 31), (-4, 31)])?,
    }
    sheet.wire(pin(km3_coil, "a2"), common_star)?;
    // Delta: the working side of the changeover, then KM3 NC, then KM2.
    let delta_feed = pin(kt1_changeover, "no");
    match km3_lock {
        Some(lock) => {
            sheet.route(delta_feed, pin(lock, "p1"), &[(2, 31), (8, 31)])?;
            sheet.wire(pin(lock, "p2"), pin(km2_coil, "a1"))?;
        }
        None => sheet.route(delta_feed, pin(km2_coil, "a1"), &[(2, 31), (8, 31)])?,
    }
    sheet.wire(pin(km2_coil, "a2"), common_delta)?;
    // Line: KM3 NO beside KM1 NO, then KM1.
    sheet.wire(to_line, pin(km3_start, "p1"))?;
    sheet.route(to_line, pin(km1_hold, "p1"), &[(20, 22)])?;
    sheet.route(pin(km3_start, "p2"), line_node, &[(14, 31)])?;
    sheet.route(pin(km1_hold, "p2"), line_node, &[(20, 31)])?;
    sheet.wire(line_node, pin(km1_coil, "a1"))?;
    sheet.wire(pin(km1_coil, "a2"), common_line)?;
    // Common rail.
    sheet.wire(common_timer, common_star)?;
    sheet.wire(common_star, common)?;
    sheet.wire(common, common_delta)?;
    sheet.wire(common_delta, common_line)?;
    sheet.wire(common, out(p0))?;

    // Power: Q1, F1, KM1 straight down; the line ends of the motor leave
    // KM1 on staggered junctions so that the feeders of KM2 only cross them.
    for (phase, (input, _)) in phases.into_iter().zip(POLES) {
        sheet.wire(out(phase), pin(q1, input))?;
    }
    for (input, output) in POLES {
        sheet.wire(pin(q1, output), pin(f1_element, input))?;
        sheet.wire(pin(f1_element, output), pin(km1_main, input))?;
    }
    let line_ends = [
        sheet.junction((40, 21))?,
        sheet.junction((44, 23))?,
        sheet.junction((48, 25))?,
    ];
    for ((node, (_, output)), terminal) in line_ends.into_iter().zip(POLES).zip(MOTOR_TERMINALS) {
        sheet.wire(pin(km1_main, output), node)?;
        sheet.wire(node, pin(motor, terminal))?;
    }
    // Second ends, staggered below the motor.
    let second_ends = [
        sheet.junction((40, 40))?,
        sheet.junction((44, 42))?,
        sheet.junction((48, 44))?,
    ];
    for (terminal, node) in ["u2", "v2", "w2"].into_iter().zip(second_ends) {
        sheet.wire(pin(motor, terminal), node)?;
    }
    // KM2 closes the delta: u1 to w2, v1 to u2, w1 to v2. Each feeder
    // leaves its line junction on its own row; each output turns on the row
    // of the second end it reaches.
    let [u_line, v_line, w_line] = line_ends;
    let [u_end, v_end, w_end] = second_ends;
    for ((line, end), ((input, output), (column, feeder_row, return_row))) in
        [u_line, v_line, w_line]
            .into_iter()
            .zip([w_end, u_end, v_end])
            .zip(
                POLES
                    .into_iter()
                    .zip([(56, 21, 44), (60, 23, 40), (64, 25, 42)]),
            )
    {
        sheet.route(line, pin(km2_main, input), &[(column, feeder_row)])?;
        sheet.route(pin(km2_main, output), end, &[(column, return_row)])?;
    }
    // KM3 closes the star point.
    let star_point = sheet.junction((44, 53))?;
    for (end, (input, _)) in second_ends.into_iter().zip(POLES) {
        sheet.wire(end, pin(km3_main, input))?;
    }
    sheet.route(pin(km3_main, "pole1.out"), star_point, &[(40, 53)])?;
    sheet.wire(pin(km3_main, "pole2.out"), star_point)?;
    sheet.route(pin(km3_main, "pole3.out"), star_point, &[(48, 53)])?;
    Ok(sheet.finish())
}

fn star_delta() -> Built<Schematic> {
    star_delta_with(true, "Star-delta starter")
}

/// The star-delta starter without any interlock between star and delta:
/// with a dropout longer than the pickup, KM2 closes while KM3 still holds
/// the star point and the phases meet there.
fn star_delta_overlap() -> Built<Schematic> {
    star_delta_with(false, "Fixture - star-delta without interlocks")
}

/// Three galvanically separate supply sections (ADR-0005 §1): 24 V DC
/// control, a 230 V single-phase lighting section and a 400 V three-phase
/// power section. Holding S1 picks up K1, whose main contacts run the motor
/// and whose auxiliary contact lights H1 from the lighting section. With
/// `bridged`, selector S2 joins the line of the lighting section to L1 of
/// the power section.
fn sections_with(bridged: bool, name: &str) -> Built<Schematic> {
    let mut sheet = Sheet::new(name);
    let (p24, p0) = sheet.control_supply((0, 0), (0, 15))?;
    let lighting = sheet.0.supply(
        circuit_model::SupplyKind::SinglePhase,
        230_000,
        None,
        Some(50_000),
        None,
    )?;
    let la = sheet.tap("LA", "l1", lighting, (10, 0))?;
    let na = sheet.tap("NA", "n", lighting, (10, 15))?;
    let phases = sheet.power_supply(20, 0)?;

    let start = sheet.part("S1", "pushbutton_no", "contact", &[], (0, 4))?;
    let k1 = sheet.device("K1", "contactor.generic", &dc_load())?;
    let coil = sheet.symbol(k1, "coil", None, (0, 10))?;
    let lamp_contact = sheet.symbol(k1, "aux_no", Some(1), (10, 4))?;
    let main = sheet.symbol(k1, "main", None, (24, 5))?;
    let lamp = sheet.part(
        "H1",
        "lamp.generic",
        "lamp",
        &[
            ("rated_voltage_mv", ParamValue::Int(230_000)),
            ("connection", ParamValue::Text("line_neutral".to_owned())),
        ],
        (10, 10),
    )?;
    let motor = sheet.part("M1", "motor.3ph", "motor", &motor_400v(), (24, 12))?;

    sheet.wire(out(p24), pin(start, "p1"))?;
    sheet.wire(pin(start, "p2"), pin(coil, "a1"))?;
    sheet.wire(pin(coil, "a2"), out(p0))?;
    sheet.wire(out(la), pin(lamp_contact, "p1"))?;
    sheet.wire(pin(lamp_contact, "p2"), pin(lamp, "x1"))?;
    sheet.wire(pin(lamp, "x2"), out(na))?;
    for ((phase, (input, output)), terminal) in phases.into_iter().zip(POLES).zip(MOTOR_TERMINALS) {
        sheet.wire(out(phase), pin(main, input))?;
        sheet.wire(pin(main, output), pin(motor, terminal))?;
    }
    if bridged {
        let selector = sheet.part("S2", "selector.latching", "contact", &[], (16, 4))?;
        sheet.route(out(la), pin(selector, "p1"), &[(16, 0)])?;
        sheet.route(
            pin(selector, "p2"),
            pin(main, "pole1.in"),
            &[(18, 6), (18, 3)],
        )?;
    }
    Ok(sheet.finish())
}

fn multi_section() -> Built<Schematic> {
    sections_with(false, "Separate supply sections")
}

/// The multi-section example with a selector that joins two sections: a
/// short circuit once it is closed.
fn sections_bridged() -> Built<Schematic> {
    sections_with(true, "Fixture - supply sections bridged")
}

/// Direct-online starter whose 230 V control circuit is fed by a control
/// transformer (ADR-0007): T1's primary sits between L1 and L2 after the
/// main switch Q1, and its secondary is a separate single-phase section.
/// Opening Q1 stops the motor at once and the control circuit one tick
/// later, when the secondary dies.
fn control_transformer() -> Built<Schematic> {
    let mut sheet = Sheet::new("Starter with a control transformer");
    let phases = sheet.power_supply(30, 0)?;
    let control = sheet.0.supply(
        circuit_model::SupplyKind::SinglePhase,
        230_000,
        None,
        Some(50_000),
        None,
    )?;
    let control = i64::try_from(control.0).map_err(|_| IdExhausted)?;
    let control_load = [
        ("rated_voltage_mv", ParamValue::Int(230_000)),
        ("connection", ParamValue::Text("line_neutral".to_owned())),
    ];

    let q1 = sheet.part(
        "Q1",
        "main_switch.3p",
        "main",
        &[("initial_closed", ParamValue::Bool(true))],
        (34, 5),
    )?;
    let f1 = sheet.device("F1", "thermal_overload.generic", &[])?;
    let f1_element = sheet.symbol(f1, "element", None, (34, 11))?;
    let f1_contact = sheet.symbol(f1, "aux_nc", None, (4, 6))?;
    let t1 = sheet.device(
        "T1",
        "transformer.control",
        &[
            ("rated_voltage_mv", ParamValue::Int(400_000)),
            ("connection", ParamValue::Text("line_line".to_owned())),
            ("secondary", ParamValue::Int(control)),
        ],
    )?;
    let primary = sheet.symbol(t1, "primary", None, (22, 12))?;
    let secondary = sheet.symbol(t1, "secondary", None, (16, 12))?;
    let stop = sheet.part("S2", "pushbutton_nc", "contact", &[], (4, 12))?;
    let start = sheet.part("S1", "pushbutton_no", "contact", &[], (0, 21))?;
    let k1 = sheet.device("K1", "contactor.generic", &control_load)?;
    let k1_coil = sheet.symbol(k1, "coil", None, (4, 29))?;
    let k1_main = sheet.symbol(k1, "main", None, (34, 17))?;
    let k1_seal = sheet.symbol(k1, "aux_no", Some(1), (8, 21))?;
    let k1_lamp = sheet.symbol(k1, "aux_no", Some(2), (12, 21))?;
    let lamp = sheet.part("H1", "lamp.generic", "lamp", &control_load, (12, 27))?;
    let motor = sheet.part("M1", "motor.3ph", "motor", &motor_400v(), (34, 24))?;

    // Power: Q1, F1 and K1 straight down to the motor. The primary of T1
    // takes L1 after Q1 and L2 after F1.
    let l1_after_q1 = sheet.junction((30, 8))?;
    let l2_after_f1 = sheet.junction((34, 14))?;
    for (phase, (input, _)) in phases.into_iter().zip(POLES) {
        sheet.wire(out(phase), pin(q1, input))?;
    }
    for ((input, output), terminal) in POLES.into_iter().zip(MOTOR_TERMINALS) {
        match input {
            "pole1.in" => {
                sheet.wire(pin(q1, output), l1_after_q1)?;
                sheet.wire(l1_after_q1, pin(f1_element, input))?;
                sheet.wire(pin(f1_element, output), pin(k1_main, input))?;
            }
            "pole2.in" => {
                sheet.wire(pin(q1, output), pin(f1_element, input))?;
                sheet.wire(pin(f1_element, output), l2_after_f1)?;
                sheet.wire(l2_after_f1, pin(k1_main, input))?;
            }
            _ => {
                sheet.wire(pin(q1, output), pin(f1_element, input))?;
                sheet.wire(pin(f1_element, output), pin(k1_main, input))?;
            }
        }
        sheet.wire(pin(k1_main, output), pin(motor, terminal))?;
    }
    sheet.route(l1_after_q1, pin(primary, "a1"), &[(22, 8)])?;
    sheet.wire(l2_after_f1, pin(primary, "a2"))?;

    // Control, from the secondary: F1 95/96, stop, start beside the seal-in
    // of K1, then K1; an auxiliary of K1 lights H1.
    let feed = sheet.junction((4, 17))?;
    let to_seal = sheet.junction((8, 17))?;
    let node = sheet.junction((4, 25))?;
    let rail = sheet.junction((4, 34))?;
    let rail_lamp = sheet.junction((12, 34))?;
    sheet.route(
        pin(secondary, "x1"),
        pin(f1_contact, "p1"),
        &[(16, 2), (4, 2)],
    )?;
    sheet.wire(pin(f1_contact, "p2"), pin(stop, "p1"))?;
    sheet.wire(pin(stop, "p2"), feed)?;
    sheet.route(feed, pin(start, "p1"), &[(0, 17)])?;
    sheet.wire(feed, to_seal)?;
    sheet.wire(to_seal, pin(k1_seal, "p1"))?;
    sheet.route(to_seal, pin(k1_lamp, "p1"), &[(12, 17)])?;
    sheet.route(pin(start, "p2"), node, &[(0, 25)])?;
    sheet.route(pin(k1_seal, "p2"), node, &[(8, 25)])?;
    sheet.wire(node, pin(k1_coil, "a1"))?;
    sheet.wire(pin(k1_coil, "a2"), rail)?;
    sheet.wire(pin(k1_lamp, "p2"), pin(lamp, "x1"))?;
    sheet.wire(pin(lamp, "x2"), rail_lamp)?;
    sheet.wire(rail, rail_lamp)?;
    sheet.route(rail_lamp, pin(secondary, "x2"), &[(16, 34)])?;
    Ok(sheet.finish())
}

/// A pump that fills a tank between two level switches: B2 (low level
/// reached) starts K1, which seals itself until B1 (high level reached)
/// opens. The latching emergency stop S0 cuts the control circuit; after it
/// is released the pump waits for the low level again. H1 shows the pump
/// running.
fn pump_level() -> Built<Schematic> {
    let mut sheet = Sheet::new("Pump with level switches");
    let (p24, p0) = sheet.control_supply((0, 0), (0, 31))?;
    let phases = sheet.power_supply(20, 0)?;

    let estop = sheet.part("S0", "estop.latching", "contact", &[], (0, 3))?;
    let high = sheet.device("B1", "limit_switch", &[])?;
    let high_contact = sheet.symbol(high, "nc", None, (0, 19))?;
    let low = sheet.device("B2", "limit_switch", &[])?;
    let low_contact = sheet.symbol(low, "no", None, (-4, 11))?;
    let k1 = sheet.device("K1", "contactor.generic", &dc_load())?;
    let k1_coil = sheet.symbol(k1, "coil", None, (0, 25))?;
    let k1_main = sheet.symbol(k1, "main", None, (24, 11))?;
    let k1_seal = sheet.symbol(k1, "aux_no", Some(1), (4, 11))?;
    let k1_lamp = sheet.symbol(k1, "aux_no", Some(2), (8, 11))?;
    let lamp = sheet.part("H1", "lamp.generic", "lamp", &dc_load(), (8, 19))?;
    let q1 = sheet.part(
        "Q1",
        "main_switch.3p",
        "main",
        &[("initial_closed", ParamValue::Bool(true))],
        (24, 5),
    )?;
    let motor = sheet.part("M1", "motor.3ph", "motor", &motor_400v(), (24, 18))?;

    // Control.
    let feed = sheet.junction((0, 7))?;
    let to_seal = sheet.junction((4, 7))?;
    let node = sheet.junction((0, 15))?;
    let rail = sheet.junction((0, 29))?;
    let rail_lamp = sheet.junction((8, 29))?;
    sheet.wire(out(p24), pin(estop, "p1"))?;
    sheet.wire(pin(estop, "p2"), feed)?;
    sheet.route(feed, pin(low_contact, "p1"), &[(-4, 7)])?;
    sheet.wire(feed, to_seal)?;
    sheet.wire(to_seal, pin(k1_seal, "p1"))?;
    sheet.route(to_seal, pin(k1_lamp, "p1"), &[(8, 7)])?;
    sheet.route(pin(low_contact, "p2"), node, &[(-4, 15)])?;
    sheet.route(pin(k1_seal, "p2"), node, &[(4, 15)])?;
    sheet.wire(node, pin(high_contact, "p1"))?;
    sheet.wire(pin(high_contact, "p2"), pin(k1_coil, "a1"))?;
    sheet.wire(pin(k1_coil, "a2"), rail)?;
    sheet.wire(pin(k1_lamp, "p2"), pin(lamp, "x1"))?;
    sheet.wire(pin(lamp, "x2"), rail_lamp)?;
    sheet.wire(rail, rail_lamp)?;
    sheet.wire(rail, out(p0))?;

    // Power.
    for ((phase, (input, output)), terminal) in phases.into_iter().zip(POLES).zip(MOTOR_TERMINALS) {
        sheet.wire(out(phase), pin(q1, input))?;
        sheet.wire(pin(q1, output), pin(k1_main, input))?;
        sheet.wire(pin(k1_main, output), pin(motor, terminal))?;
    }
    Ok(sheet.finish())
}

/// A project that uses a model of the example library `acme`
/// (`examples/library/acme.oclib`, the example of `LIBRARY.md`): S1 picks up
/// the ACME relay K1, whose own NO contact lights H1.
fn acme_lamp() -> Built<Schematic> {
    let mut sheet = Sheet::new("Lamp switched by an ACME relay");
    let (p24, p0) = sheet.control_supply((0, 0), (0, 16))?;
    let start = sheet.part("S1", "pushbutton_no", "contact", &[], (0, 4))?;
    let k1 = sheet.device("K1", "acme.relay_r2", &dc_load())?;
    let coil = sheet.symbol(k1, "coil", None, (0, 10))?;
    let contact = sheet.symbol(k1, "no", Some(1), (8, 4))?;
    let lamp = sheet.part("H1", "lamp.generic", "lamp", &dc_load(), (8, 10))?;
    let feed = sheet.junction((0, 1))?;
    let rail = sheet.junction((0, 14))?;
    sheet.wire(out(p24), feed)?;
    sheet.wire(feed, pin(start, "p1"))?;
    sheet.route(feed, pin(contact, "p1"), &[(8, 1)])?;
    sheet.wire(pin(start, "p2"), pin(coil, "a1"))?;
    sheet.wire(pin(coil, "a2"), rail)?;
    sheet.wire(pin(contact, "p2"), pin(lamp, "x1"))?;
    sheet.route(pin(lamp, "x2"), rail, &[(8, 14)])?;
    sheet.wire(rail, out(p0))?;
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
        for example in EXAMPLES {
            let catalog = example.catalog().unwrap();
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
        for example in EXAMPLES {
            let catalog = example.catalog().unwrap();
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
        for output in outputs() {
            let (path, text) = output.unwrap();
            let on_disk = std::fs::read(root.join(&path)).unwrap_or_else(|error| {
                panic!("{path}: {error}; run `cargo run -p xtask -- generate-examples`")
            });

            assert_eq!(
                on_disk, text,
                "{path} is stale; run `cargo run -p xtask -- generate-examples`"
            );
        }
    }

    mod star_delta {
        use circuit_model::{ComponentCatalog, DiagnosticCode, InputAction, ParamValue, Schematic};
        use circuit_sim::{DelayConfig, FaultPolicy, SimConfig, Simulator, compile};
        use proptest::prelude::*;

        /// One operator action: tick, device (S0, S1, F1) and whether it is
        /// engaged (pressed, tripped) or released.
        type Stroke = (u64, usize, bool);

        fn strokes() -> impl Strategy<Value = Vec<Stroke>> {
            prop::collection::vec((0_u64..80, 0_usize..3, any::<bool>()), 0..30).prop_map(
                |mut list| {
                    list.sort_unstable();
                    list.dedup_by_key(|(tick, device, _)| (*tick, *device));
                    list
                },
            )
        }

        fn with_preset(mut doc: Schematic, preset_ms: i64) -> Schematic {
            for device in &mut doc.devices {
                if device.reference.0 == "KT1" {
                    device
                        .params
                        .insert("preset_ms".to_owned(), ParamValue::Int(preset_ms));
                }
            }
            doc
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(128))]

            /// The exit criterion of M3: whatever the operator does and
            /// whatever the delays, the star and delta contactors are never
            /// picked up together and the starter never faults.
            #[test]
            fn star_and_delta_never_overlap(
                script in strokes(),
                preset_ms in 1_i64..20,
                pickup_ticks in 1_u32..4,
                dropout_ticks in 1_u32..5,
            ) {
                let doc = with_preset(super::super::star_delta().unwrap(), preset_ms);
                let output = compile(&doc, &ComponentCatalog::builtin());
                prop_assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                let circuit = output.circuit.unwrap();
                let config = SimConfig {
                    delays: DelayConfig { pickup_ticks, dropout_ticks },
                    fault_policy: FaultPolicy::Halt,
                    record_trace: false,
                    ..SimConfig::default()
                };
                let mut sim = Simulator::new(circuit, config).unwrap();
                let id = |reference: &str| {
                    sim.circuit().device_by_reference(reference).unwrap().id
                };
                let (km2, km3) = (id("KM2"), id("KM3"));
                let devices = [id("S0"), id("S1"), id("F1")];
                for tick in 0..90 {
                    let actions: Vec<InputAction> = script
                        .iter()
                        .filter(|(at, _, _)| *at == tick)
                        .map(|(_, device, engaged)| {
                            let target = devices[*device];
                            match (device, engaged) {
                                (2, true) => InputAction::Trip(target),
                                (2, false) => InputAction::ResetTrip(target),
                                (_, true) => InputAction::Press(target),
                                (_, false) => InputAction::Release(target),
                            }
                        })
                        .collect();
                    sim.step(&actions).unwrap();
                    let snapshot = sim.snapshot().unwrap();

                    prop_assert!(
                        !(snapshot.active_devices.contains(&km2)
                            && snapshot.active_devices.contains(&km3)),
                        "tick {}: star and delta together", tick
                    );
                    prop_assert!(!snapshot.faulted, "tick {}: {:?}", tick, snapshot.faults);
                    prop_assert!(
                        !snapshot
                            .faults
                            .iter()
                            .any(|f| f.code() == DiagnosticCode::UnsupportedWindingConnection),
                        "tick {}: {:?}", tick, snapshot.faults
                    );
                }
            }
        }
    }

    #[test]
    fn every_example_has_two_drawings_and_fixtures_have_none() {
        for example in EXAMPLES {
            let expected = if example.path.starts_with("examples/") {
                2
            } else {
                0
            };

            assert_eq!(example.drawings().len(), expected, "{}", example.path);
        }
    }

    #[test]
    fn drawings_of_an_example_are_the_same_twice() {
        let drawings = || EXAMPLES[0].drawings();

        assert_eq!(drawings(), drawings());
    }

    #[test]
    fn the_committed_example_library_is_the_example_of_library_md() {
        let markdown = include_str!("../../LIBRARY.md");
        let example = markdown
            .split_once("```json\n")
            .and_then(|(_, rest)| rest.split_once("```"))
            .map(|(json, _)| json)
            .unwrap();

        assert_eq!(ACME_LIBRARY, example);
    }
}
