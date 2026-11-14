# OpenCircuit

OpenCircuit is an open-source electrical schematic editor and simulator focused on
industrial control circuits: contactors, relays, timers, pushbuttons and
protective devices.

The project treats the schematic as a domain model, not as a drawing. The editor
changes the model, the compiler turns it into an executable representation, and
the simulator produces deterministic snapshots. The electrical core does not
depend on the GUI.

## Status

M1, the headless vertical slice, is implemented: circuits are loaded,
compiled, simulated and checked against golden traces from the command line.
M2a, the read-only viewer, is implemented: the desktop application opens a
project, draws it and runs it with operator controls. The editor, M2b, is
next. See
[`ROADMAP.md`](ROADMAP.md).

## Workspace

```text
apps/
  cli/                 `opencircuit`: validate, simulate, test
  desktop/             `opencircuit-desktop`: egui viewer (open, draw, run)
crates/
  circuit-model/       IDs, document, catalog and geometry, diagnostics, inputs, snapshot
  circuit-sim/         compile, solve, logic, runtime, trace
  circuit-project/     JSON .ocircuit: strict loading, canonical writing, migrations
  circuit-renderer/    neutral scene: primitives with semantic styles, no GUI
xtask/                 architecture checks and example generation
examples/              reference projects with scenarios and golden traces
tests/fixtures/        projects that must be rejected, with their scenarios
```

M2b adds `circuit-editor` and `circuit-commands`; M3 adds `circuit-export`.
The other directories are placeholders for those milestones.

The dependency direction is enforced by
`cargo run -p xtask -- check-architecture`:

```text
circuit-model <- circuit-sim
circuit-model <- circuit-project
circuit-model <- circuit-renderer (and commands/editor in M2b)
the renderer consumes SimSnapshot, never simulator internals
apps/* may depend on GUI crates; circuit-* crates may not
```

## Try it

```sh
cargo run -p opencircuit-cli -- validate examples/direct-starter/starter.ocircuit
cargo run -p opencircuit-cli -- simulate examples/forward-reverse/reversing.ocircuit \
    --scenario examples/forward-reverse/simultaneous.scenario.json
cargo run -p opencircuit-cli -- test examples tests/fixtures
```

`simulate` prints the change-log trace. The simultaneous start of the
reversing starter ends with `runtime latched` at the first short circuit;
add `--fault-policy report` to watch it oscillate instead. Exit codes: `0`
success, `1` validation or golden failure, `2` usage or I/O error.

### Running the viewer

```sh
cargo run -p opencircuit-desktop -- examples/direct-starter/starter.ocircuit
```

A project can also be dropped on the window or typed in the path field.
Play, Pause, Step, Reset and the speed control run the simulation; the
Operator panel (or a click on the symbol) presses S1, opens a switch or trips
a thermal relay, and the events panel shows the trace as it happens. Wires
take the colour of the conductors on them; a wire with more than one is drawn
as a conflict. The right or middle button pans, the wheel zooms, a double
click fits the sheet. The desktop needs Rust 1.95 (eframe 0.37); the other
crates build with Rust 1.85.

## Commands

Run everything from the repository root:

```sh
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run -p xtask -- check-architecture
cargo run -p xtask -- generate-examples --check
cargo run -p opencircuit-cli -- test examples tests/fixtures
cargo deny check
```

## Documentation

- [`docs/spec02.md`](docs/spec02.md) — consolidated technical specification v0.3 (Portuguese); the contract reference.
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — implementation-facing architecture summary.
- [`FORMAT.md`](FORMAT.md) — `.ocircuit` project format.
- [`SIMULATION.md`](SIMULATION.md) — virtual time and tick semantics.
- [`docs/adr/`](docs/adr/README.md) — architecture decision records (identity and IDs, tick semantics, conductor model, file format).
- [`ROADMAP.md`](ROADMAP.md) — milestones and scope.

## Safety notice

OpenCircuit is a drawing, learning and preliminary simulation tool. Its results
do not replace a professional electrical design, risk analysis, commissioning,
protection validation or compliance with applicable standards. The software does
not certify installations or guarantee operational safety.

## License

Licensed under either of

- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE));
- MIT License ([`LICENSE-MIT`](LICENSE-MIT)).

at your option.
