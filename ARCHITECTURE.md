# Architecture

This document summarizes the implementation-facing architecture. The
authoritative design proposal is
[`OpenCircuit_Arquitetura_Tecnica_v0.2.md`](OpenCircuit_Arquitetura_Tecnica_v0.2.md);
the original design notes are in [`ideia.md`](ideia.md).

## Core principle

OpenCircuit is a circuit model that has a graphical representation, not a drawing
that tries to infer electricity from pixels.

## Layers

| Layer | Crate | Responsibility |
| --- | --- | --- |
| Domain | `circuit-domain` | IDs, devices, symbols, terminals, wires, junctions, supplies |
| Diagnostics | `circuit-diagnostics` | Stable codes, severity, subjects and messages |
| Components | `circuit-components` | Catalog of device definitions, terminals and symbols |
| Topology | `circuit-topology` | Validation and compilation into dense netlist indices |
| Electrical | `circuit-electrical` | Ideal connectivity, conductors and discrete solver |
| Logic | `circuit-logic` | Buttons, coils, contacts, relays and timers |
| Simulation | `circuit-simulation` | Fixed tick, runtime state, snapshots and replay |
| Project | `circuit-project` | JSON serialization, schema validation and migrations |
| Commands | `circuit-commands` | Transactional edit commands and undo/redo |
| Editor | `circuit-editor` | Selection, hit-testing, routing and interaction state |
| Renderer | `circuit-renderer` | Neutral scene built from the document and snapshots |
| Export | `circuit-export` | SVG and PDF export from the neutral scene |

`apps/desktop` owns egui/eframe code. `apps/cli` runs headless workflows.

## Non-negotiable rules

1. `circuit-domain` has no workspace dependencies.
2. `simulation`, `electrical` and `logic` never depend on editor, renderer,
   commands or project.
3. Editor and renderer consume neutral snapshot contracts, not simulator
   internals.
4. No `circuit-*` crate depends on egui or eframe.
5. Document entities use stable IDs and terminal keys. Dense indices exist only
   inside the compiled circuit.
6. A device is not a symbol: one contactor has one identity and several graphic
   representations.
7. Wire crossings are not connections; junctions are explicit entities.
8. Conductors identify wire function. They are not instantaneous voltage or
   current values.

The first five rules are verified by `cargo run -p xtask -- check-architecture`.

## Determinism

Compilation sorts entities by stable IDs before assigning dense indices.
Diagnostics and fault lists are sorted. The simulator uses virtual time and a
fixed tick owned by the runtime; it never reads the system clock. With the same
compiled circuit, initial state and input sequence, the trace must be
reproducible.

## Current status

The domain and diagnostics contracts are in place. Topology compilation, the
discrete solver and the runtime are the next milestone.
