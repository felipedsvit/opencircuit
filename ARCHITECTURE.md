# Architecture

This document is the one-page implementation map. The authoritative contract is
[`docs/spec02.md`](docs/spec02.md) (v0.3, Portuguese). The normative rules for
simulation and the file format are in [`SIMULATION.md`](SIMULATION.md) and
[`FORMAT.md`](FORMAT.md), which are derived from that specification.

The decisions that are hard to reverse (identity and IDs, tick semantics,
conductor model, file format) are recorded in [`docs/adr/`](docs/adr/README.md).

## Core principle

OpenCircuit is a circuit model that has a graphical representation, not a
drawing that tries to infer electricity from pixels.

## Target crates

The workspace is deliberately small. A layer becomes a crate only when a
second real consumer justifies the boundary.

| Layer | Crate | Responsibility |
| --- | --- | --- |
| Model | `circuit-model` | IDs, document, catalog, conductors, diagnostics, inputs and snapshot types |
| Simulation | `circuit-sim` | Topology compilation, static copper nets, discrete solver, logic, timers, thermal latch, runtime, fault policy and change log |
| Project | `circuit-project` | JSON `.ocircuit`, `schema_version`, strict loading, limits, migrations and canonical round-trip; strict reading of extension libraries (`LIBRARY.md`) |
| Commands | `circuit-commands` | Invertible edits, atomic transactions checked for integrity and format limits, undo/redo history, and the editing operations |
| Editor | `circuit-editor` | Editor logic without a GUI: hit-test, selection, tools (select, place, wire), drag and placement previews; returns transactions, never changes the document |
| Renderer | `circuit-renderer` | Neutral scene: drawing primitives in integer logical coordinates with semantic styles, built from a document and an optional `SimSnapshot` |
| Export | `circuit-export` | Deterministic SVG written directly and PDF written with `pdf-writer` (ADR-0006) from the neutral scene, with a print and a live theme; no I/O |
| I/O | `circuit-io` | Modbus TCP codec and the I/O map of a project (ADR-0009); no socket, no thread, no clock |
| CLI | `apps/cli` (package `opencircuit-cli`, binary `opencircuit`) | Headless validation, simulation, golden execution, export, project creation and scripted editing (`new`, `edit`, through `circuit-commands`) and the Modbus TCP server (`serve`, the only place with a socket and a real-time loop) |
| Desktop | `apps/desktop` (package and binary `opencircuit-desktop`) | egui/eframe window that creates, opens, draws, edits (palette, inspector, undo/redo through `circuit-commands` and `circuit-editor`), saves atomically and runs a project with operator controls; the only package with a GUI dependency |
| Tooling | `xtask` | Architecture checks, generation of the example projects and benchmarks |

Modules of `circuit-sim` and the imports each may use:

```text
bitset, union_find, fingerprint, logic -> nothing
compiled -> fingerprint
compile  -> compiled, fingerprint, union_find
solve    -> bitset, compiled, union_find
trace    -> compiled, fingerprint
runtime  -> bitset, compiled, fingerprint, logic, solve, trace
```

The crates arrived in this order: M1 has `circuit-model`, `circuit-sim`,
`circuit-project`, `apps/cli` and `xtask`; M2 added `circuit-renderer`,
`apps/desktop`, `circuit-commands` and `circuit-editor`; M3 added
`circuit-export`; M4 added `circuit-io`. A new crate is not a workspace member
before it has a real `src/lib.rs` and a consumer.

## Dependency rules

```text
circuit-model       -> no workspace crate
circuit-sim         -> circuit-model
circuit-project     -> circuit-model
circuit-renderer    -> circuit-model
circuit-commands    -> circuit-model
circuit-editor      -> circuit-model, circuit-commands
circuit-export      -> circuit-model, circuit-renderer
circuit-io          -> circuit-model
apps/cli            -> circuit-model, circuit-sim, circuit-project, circuit-renderer, circuit-export, circuit-io
xtask               -> circuit-model, circuit-project, circuit-sim, circuit-renderer, circuit-commands, circuit-export
apps/desktop        -> model, sim, project, renderer, editor, commands, export
```

Rules:

1. `circuit-model` has no workspace dependencies.
2. `circuit-sim` depends only on `circuit-model`.
3. `circuit-project` depends only on `circuit-model`.
4. Renderer, editor and commands consume `Schematic` and `SimSnapshot`; they do
   not depend on `circuit-sim`.
5. No `circuit-*` crate depends on egui or eframe. UI code lives in
   `apps/desktop`.
6. I/O adapters do not introduce sockets, clocks or OS dependencies into the
   core.
7. `cargo run -p xtask -- check-architecture` enforces these rules as
   allowlists that fail closed: every workspace package needs a rule, core
   crates may not reach `egui`, `eframe`, `winit`, `clap` or `anyhow` even
   transitively, every `circuit-sim` module needs an import rule, and core
   sources may not use `std::fs`, `std::net`, `std::env`, `std::process`,
   `std::thread`, `Instant` or `SystemTime`. Each core crate also carries a
   `clippy.toml` with `disallowed-types`. `cargo-deny` verifies dependency
   sources, licenses and advisories.

## Invariants

- A device is not a symbol. One contactor has one identity and several graphic
  representations. Terminals belong to the symbol model.
- Document entities use stable IDs and terminal keys. Dense indices exist only
  inside the compiled circuit and are newtypes.
- Wire crossings are not connections; junctions are explicit entities.
- Conductors identify wire function, not instantaneous voltage or current.
- Copper nets are compiled once. Per tick, only closed switches union them.
- `current` and `next` signals are distinct; the snapshot represents the network
  that was actually solved, not the next state.
- Electrical faults are fail-closed: short, protective-earth fault and
  incompatible supplies latch the runtime after the faulting tick.
- Diagnostics are structured (`DiagnosticCode` + `Subject`/`related`), not
  free-form strings.

## Determinism

Compilation sorts entities by stable IDs before assigning dense indices.
Diagnostics and fault lists are sorted. The simulator uses virtual time and a
fixed tick owned by the runtime; it never reads the system clock. With the same
compiled circuit, initial state, input sequence and tick configuration, the trace
must be reproducible. `step_n` plus an on-demand `snapshot` is the normal path;
the runtime does not allocate a full snapshot per tick.

## Current status

M1 is implemented: `circuit-model`, `circuit-sim`, `circuit-project`,
`apps/cli` and `xtask`, with golden scenarios for the direct-online and
reversing starters under `examples/` and compile-time fixtures under
`tests/fixtures/`. The example projects are generated by
`cargo run -p xtask -- generate-examples` and checked against their generator
in the tests.

M2a is implemented: `circuit-renderer` builds the neutral scene from a
document and an optional snapshot, and `apps/desktop` opens, draws and runs a
project with operator controls, one `step_n` and one snapshot per frame.
Symbol geometry (terminal offsets and bounds) lives in the catalog, and
`circuit_model::layout` derives pin positions and wire paths for every view.
M2b is implemented: `circuit-commands` turns every user action into a
transaction applied whole or not at all, with undo and redo;
`circuit-editor` holds the tools, the selection and the hit-test without a
GUI; the desktop application edits, saves atomically and reopens projects.
Hostile input is covered by proptests (loader, compiler, transactions);
`cargo-fuzz` is deferred.
M3 is implemented: changeover contacts in version 2 relays and timers,
combined pushbuttons and mechanical interlocks, the six-terminal motor with
its star/delta winding model, galvanically separate supply sections
(ADR-0005), and `circuit-export`, which writes the neutral scene as
deterministic SVG and, with `pdf-writer`, as PDF (ADR-0006), from the
command line and the desktop application. The star-delta starter under
`examples/star-delta` is validated by goldens and by a property test that
star and delta are never picked up together.

M4 is implemented. The control transformer feeds one supply section from
another, its secondary live from the tick after the primary is energized
(ADR-0007). Extension libraries are data: `circuit_project::library` reads
them and `ComponentCatalog::with_library` adds their namespaced models,
which reuse the behaviour classes of the core and may carry their own
artwork (ADR-0008, `LIBRARY.md`). `circuit-io` holds the Modbus TCP codec
and the I/O map with no socket or clock; `opencircuit serve` owns the
socket and the real-time loop (ADR-0009). OPC UA and MQTT were evaluated
and not implemented (ADR-0010). `apps/desktop` builds for
`wasm32-unknown-unknown`, with `eframe::WebRunner` and file drop and
download in place of the native dialogs; CI lints and builds it. The
interfaces and their safety limits are summarised in `EXTENDING.md`.

M5 added sheets as frames, terminal numbers and the embedded PDF font
(ADR-0011, ADR-0012) and the editing commands of the command line.

M6 is implemented. The catalog gives every model a category (the tab of the
palette), a title and a note; library schema 2 carries them. The class
`symbolic` and the block kinds (ADR-0013) let a part that the discrete model
cannot simulate be drawn and wired without behaviour: the compiler adds
nothing for it and reports it as information (`OC-SEM-011`). The standard
library `std` is assembled in code by `circuit_project::stdlib` and is part
of the catalog every front end starts from. The desktop window has its own
theme tokens and icons, a menu bar, a status bar, a palette in tabs with
drawings from `circuit_renderer::symbol_preview`, and a command line whose
meaning is `circuit_commands::commandline`: the window holds no logic of its
own, it shows requests that the core computes. The editor selects by window
or by crossing (`Preview::crossing`).
