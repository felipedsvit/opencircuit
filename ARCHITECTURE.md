# Architecture

This document is the one-page implementation map. The authoritative design
proposal is
[`OpenCircuit_Arquitetura_Tecnica_v0.2.md`](OpenCircuit_Arquitetura_Tecnica_v0.2.md);
the original design notes are in [`ideia.md`](ideia.md).

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
| Project | `circuit-project` | JSON `.ocircuit`, `schema_version`, validation, migrations and round-trip |
| CLI | `apps/cli` | Headless validation, simulation and golden execution |
| Tooling | `xtask` | Architecture and repository checks |

M2 adds `circuit-renderer`, `circuit-editor`, `circuit-commands` and
`apps/desktop`. M3 adds `circuit-export`. Those directories are placeholders
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
7. `cargo run -p xtask -- check-architecture` verifies the M0 blacklists
   today; the M1 target is an allowlist per crate. `cargo-deny` verifies
   dependency sources, licenses and advisories.

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

M0 delivered `circuit-domain`, `circuit-diagnostics`, `xtask` and the CI
foundation. The first M1 step consolidates the two domain crates into
`circuit-model` and creates `circuit-sim`, `circuit-project` and `apps/cli`.
Topology compilation, the discrete solver and the direct-online golden are the
next milestone. The GUI starts in M2a as a read-only viewer.
