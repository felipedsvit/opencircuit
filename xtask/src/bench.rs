//! Rough timings of the paths a viewer runs every frame, on synthetic
//! documents of 100 to 10,000 components (spec §14.5): compilation, scene,
//! simulation, hit-test, one edit and the SVG and PDF exports, and the
//! simulation of a circuit that switches on every tick.
//!
//! This is a measurement aid, not a gate: the numbers depend on the machine
//! and are recorded by hand in `docs/benchmarks.md`. Run it in release mode:
//!
//! ```text
//! cargo run -p xtask --release -- bench
//! ```

use std::fmt::Write as _;
use std::hint::black_box;
use std::time::{Duration, Instant};

use std::collections::BTreeSet;

use circuit_commands::ops;
use circuit_model::builder::pin;
use circuit_model::{
    ComponentCatalog, GridPoint, IdExhausted, InputAction, LayoutIndex, ParamValue, Schematic,
    SchematicBuilder, Subject,
};
use circuit_renderer::{SceneOptions, build_scene};
use circuit_sim::{CompiledCircuit, SimConfig, Simulator, compile};

/// Document sizes, in devices.
const SIZES: [usize; 5] = [100, 500, 2_000, 5_000, 10_000];
/// Devices per rung of the oscillating ladder: two supply taps and a relay.
const DEVICES_PER_OSCILLATOR: usize = 3;
/// Devices per rung: two supply taps, a pushbutton and a contactor.
const DEVICES_PER_RUNG: usize = 4;
/// Ticks run per `step_n` measurement.
const TICKS: u32 = 1_000;
/// Time spent repeating one measurement before averaging it.
const BUDGET: Duration = Duration::from_millis(300);
/// Most repetitions of one measurement.
const MAX_REPEATS: u32 = 10_000;

/// A ladder of `rungs` rungs side by side. Each rung is a 24 V tap, a
/// normally open pushbutton, a contactor coil and a 0 V tap, all wired in
/// series; rung `i` sits at `x = 8 i`.
fn ladder(rungs: usize) -> Result<Schematic, IdExhausted> {
    let mut b = SchematicBuilder::new("benchmark ladder");
    let supply = b.dc_supply(24_000)?;
    let supply = ParamValue::Int(i64::try_from(supply.0).map_err(|_| IdExhausted)?);
    let load = [
        ("rated_voltage_mv", ParamValue::Int(24_000)),
        ("connection", ParamValue::Text("dc".to_owned())),
    ];
    for rung in 0..rungs {
        let x = i32::try_from(rung)
            .ok()
            .and_then(|r| r.checked_mul(8))
            .ok_or(IdExhausted)?;
        let tap = |b: &mut SchematicBuilder, name: String, conductor: &str, y: i32| {
            let device = b.device(
                &name,
                "supply.terminal",
                &[
                    ("conductor", ParamValue::Text(conductor.to_owned())),
                    ("supply", supply.clone()),
                ],
            )?;
            b.symbol(device, "terminal", None, GridPoint::new(x, y))
        };
        let top = tap(&mut b, format!("P{rung}"), "dc_pos", 0)?;
        let bottom = tap(&mut b, format!("N{rung}"), "dc_neg", 16)?;
        let button = b.device(&format!("S{rung}"), "pushbutton_no", &[])?;
        let contact = b.symbol(button, "contact", None, GridPoint::new(x, 4))?;
        let contactor = b.device(&format!("K{rung}"), "contactor.generic", &load)?;
        let coil = b.symbol(contactor, "coil", None, GridPoint::new(x, 12))?;
        b.wire(pin(top, "out"), pin(contact, "p1"))?;
        b.wire(pin(contact, "p2"), pin(coil, "a1"))?;
        b.wire(pin(coil, "a2"), pin(bottom, "out"))?;
    }
    Ok(b.finish())
}

/// A ladder of `rungs` relays, each fed through its own normally closed
/// contact: every relay toggles on every tick, so no tick can reuse the
/// solution of the previous one. Rung `i` sits at `x = 8 i`.
fn oscillators(rungs: usize) -> Result<Schematic, IdExhausted> {
    let mut b = SchematicBuilder::new("benchmark oscillators");
    let supply = b.dc_supply(24_000)?;
    let supply = ParamValue::Int(i64::try_from(supply.0).map_err(|_| IdExhausted)?);
    let load = [
        ("rated_voltage_mv", ParamValue::Int(24_000)),
        ("connection", ParamValue::Text("dc".to_owned())),
    ];
    for rung in 0..rungs {
        let x = i32::try_from(rung)
            .ok()
            .and_then(|r| r.checked_mul(8))
            .ok_or(IdExhausted)?;
        let tap = |b: &mut SchematicBuilder, name: String, conductor: &str, y: i32| {
            let device = b.device(
                &name,
                "supply.terminal",
                &[
                    ("conductor", ParamValue::Text(conductor.to_owned())),
                    ("supply", supply.clone()),
                ],
            )?;
            b.symbol(device, "terminal", None, GridPoint::new(x, y))
        };
        let top = tap(&mut b, format!("P{rung}"), "dc_pos", 0)?;
        let bottom = tap(&mut b, format!("N{rung}"), "dc_neg", 16)?;
        let relay = b.device(&format!("K{rung}"), "relay.generic", &load)?;
        let contact = b.symbol(relay, "aux_nc", Some(1), GridPoint::new(x, 4))?;
        let coil = b.symbol(relay, "coil", None, GridPoint::new(x, 12))?;
        b.wire(pin(top, "out"), pin(contact, "p1"))?;
        b.wire(pin(contact, "p2"), pin(coil, "a1"))?;
        b.wire(pin(coil, "a2"), pin(bottom, "out"))?;
    }
    Ok(b.finish())
}

/// Mean duration of `run`, repeated until [`BUDGET`] or [`MAX_REPEATS`].
fn time<T>(mut run: impl FnMut() -> T) -> Duration {
    let start = Instant::now();
    let mut repeats = 0_u32;
    while repeats < MAX_REPEATS && (repeats == 0 || start.elapsed() < BUDGET) {
        black_box(run());
        repeats = repeats.saturating_add(1);
    }
    start.elapsed().checked_div(repeats).unwrap_or_default()
}

fn micros(duration: Duration) -> String {
    format!("{:.1}", duration.as_secs_f64() * 1.0e6)
}

/// Every pushbutton pressed: the first tick of the measured run.
fn press_all(circuit: &CompiledCircuit) -> Vec<InputAction> {
    circuit
        .devices()
        .iter()
        .filter(|device| device.reference.0.starts_with('S'))
        .map(|device| InputAction::Press(device.id))
        .collect()
}

/// Measure every size and return a Markdown table, in microseconds.
pub(crate) fn run() -> Result<String, Vec<String>> {
    let catalog = ComponentCatalog::builtin();
    let mut table = String::from(
        "| devices | compile | scene, at rest | scene, running | new + step_n(1000) | snapshot | symbol_at | move edit | SVG | PDF |\n\
         | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
    );
    for size in SIZES {
        let doc = ladder(size.div_ceil(DEVICES_PER_RUNG))
            .map_err(|error| vec![format!("{size} devices: {error}")])?;
        let output = compile(&doc, &catalog);
        let Some(circuit) = output.circuit.clone() else {
            return Err(output
                .diagnostics
                .iter()
                .map(|d| format!("{size} devices: {} {}", d.code(), d.message()))
                .collect());
        };
        let inputs = press_all(&circuit);
        let config = SimConfig {
            record_trace: false,
            ..SimConfig::default()
        };
        let mut sim = Simulator::new(circuit.clone(), config)
            .map_err(|error| vec![format!("{size} devices: {error}")])?;
        sim.step_n(TICKS, &inputs)
            .map_err(|error| vec![format!("{size} devices: {error}")])?;
        let snapshot = sim
            .snapshot()
            .ok_or_else(|| vec![format!("{size} devices: no snapshot after stepping")])?;
        let options = SceneOptions::default();
        let layout = LayoutIndex::new(&doc, &catalog);
        let probe = GridPoint::new(0, 12);

        let compile_time = time(|| compile(&doc, &catalog));
        let rest = time(|| build_scene(&doc, &catalog, None, &options));
        let running = time(|| build_scene(&doc, &catalog, Some(&snapshot), &options));
        let stepping = time(|| {
            Simulator::new(circuit.clone(), config).map(|mut sim| sim.step_n(TICKS, &inputs))
        });
        let snapshotting = time(|| sim.snapshot());
        let hit = time(|| layout.symbol_at(probe));
        // Moving one symbol: build the transaction, then apply it to a copy
        // with the integrity and limit checks, as the editor does.
        let selection: BTreeSet<Subject> = doc
            .symbols
            .first()
            .map(|symbol| Subject::Symbol(symbol.id))
            .into_iter()
            .collect();
        let edit = time(|| {
            ops::move_by(&doc, &catalog, &selection, GridPoint::new(1, 0))
                .and_then(|tx| tx.apply_to(&doc))
        });
        let scene = build_scene(&doc, &catalog, None, &options);
        let svg = time(|| circuit_export::svg(&scene, circuit_export::Theme::Print));
        let pdf = time(|| circuit_export::pdf(&scene, circuit_export::Theme::Print));

        let _ = writeln!(
            table,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            doc.devices.len(),
            micros(compile_time),
            micros(rest),
            micros(running),
            micros(stepping),
            micros(snapshotting),
            micros(hit),
            micros(edit),
            micros(svg),
            micros(pdf),
        );
    }

    let mut switching = String::from(
        "| devices | new + step_n(1000) | per tick |\n\
         | ---: | ---: | ---: |\n",
    );
    for size in SIZES {
        let doc = oscillators(size.div_ceil(DEVICES_PER_OSCILLATOR))
            .map_err(|error| vec![format!("{size} devices: {error}")])?;
        let circuit = compile(&doc, &catalog)
            .circuit
            .ok_or_else(|| vec![format!("{size} devices: the oscillators do not compile")])?;
        let config = SimConfig {
            record_trace: false,
            ..SimConfig::default()
        };
        let stepping =
            time(|| Simulator::new(circuit.clone(), config).map(|mut sim| sim.step_n(TICKS, &[])));
        let _ = writeln!(
            switching,
            "| {} | {} | {} |",
            doc.devices.len(),
            micros(stepping),
            micros(stepping.checked_div(TICKS).unwrap_or_default()),
        );
    }
    Ok(format!(
        "times in microseconds per call\n\n{table}\nswitching on every tick\n\n{switching}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder_has_the_requested_devices_and_compiles_cleanly() {
        let doc = ladder(3).unwrap();
        let output = compile(&doc, &ComponentCatalog::builtin());

        assert_eq!(doc.devices.len(), 3 * DEVICES_PER_RUNG);
        assert_eq!(output.diagnostics, []);
    }

    #[test]
    fn pressing_every_button_energizes_every_coil() {
        let doc = ladder(3).unwrap();
        let circuit = compile(&doc, &ComponentCatalog::builtin()).circuit.unwrap();
        let inputs = press_all(&circuit);
        let mut sim = Simulator::new(circuit, SimConfig::default()).unwrap();

        sim.step(&inputs).unwrap();

        assert_eq!(sim.snapshot().unwrap().energized_devices.len(), 3);
    }

    #[test]
    fn every_oscillator_toggles_on_every_tick() {
        let doc = oscillators(3).unwrap();
        let output = compile(&doc, &ComponentCatalog::builtin());
        assert_eq!(output.diagnostics, []);
        let mut sim = Simulator::new(output.circuit.unwrap(), SimConfig::default()).unwrap();

        let mut active = Vec::new();
        for _ in 0..4 {
            sim.step(&[]).unwrap();
            active.push(sim.snapshot().unwrap().active_devices.len());
        }

        assert_eq!(doc.devices.len(), 3 * DEVICES_PER_OSCILLATOR);
        assert_eq!(active, [0, 3, 0, 3]);
    }
}
