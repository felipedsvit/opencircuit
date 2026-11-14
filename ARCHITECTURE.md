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

The M1 workspace is deliberately small. A layer becomes a crate only when a
second real consumer justifies the boundary.

| Layer | Crate | Responsibility |
| --- | --- | --- |
| Model | `circuit-model` | IDs, document, catalog, conductors, diagnostics, inputs and snapshot types |
| Simulation | `circuit-sim` | Topology compilation, static copper nets, discrete solver, logic, timers, thermal latch, runtime, fault policy and change log |
| Project | `circuit-project` | JSON `.ocircuit`, `schema_version`, strict loading, limits, migrations and canonical round-trip |
| Renderer | `circuit-renderer` | Neutral scene: drawing primitives in integer logical coordinates with semantic styles, built from a document and an optional `SimSnapshot` |
| CLI | `apps/cli` (package `opencircuit-cli`, binary `opencircuit`) | Headless validation, simulation and golden execution |
| Desktop | `apps/desktop` (package and binary `opencircuit-desktop`) | egui/eframe window that opens, draws and runs a project with operator controls; the only package with a GUI dependency |
| Tooling | `xtask` | Architecture checks and generation of the example projects |

Modules of `circuit-sim` and the imports each may use:

```text
bitset, union_find, fingerprint, logic -> nothing
compiled -> fingerprint
compile  -> compiled, fingerprint, union_find
solve    -> bitset, compiled, union_find
trace    -> compiled, fingerprint
runtime  -> bitset, compiled, fingerprint, logic, solve, trace
```

M2 adds `circuit-renderer` and `apps/desktop` (both present), then
`circuit-editor` and `circuit-commands`. M3 adds `circuit-export`. Those directories are placeholders
until their milestone; they are not workspace members before they have a real
`src/lib.rs` and a consumer.

## Dependency rules

```text
circuit-model       -> no workspace crate
circuit-sim         -> circuit-model
circuit-project     -> circuit-model
circuit-renderer    -> circuit-model
circuit-commands    -> circuit-model
circuit-editor      -> circuit-model, circuit-commands
circuit-export      -> circuit-model, circuit-renderer
apps/cli            -> circuit-model, circuit-sim, circuit-project
xtask               -> circuit-model, circuit-project
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
in the tests. The GUI starts in M2a as a read-only viewer.
