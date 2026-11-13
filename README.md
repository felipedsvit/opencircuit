# OpenCircuit

OpenCircuit is an open-source electrical schematic editor and simulator focused on
industrial control circuits: contactors, relays, timers, pushbuttons and
protective devices.

The project treats the schematic as a domain model, not as a drawing. The editor
changes the model, the compiler turns it into an executable representation, and
the simulator produces deterministic snapshots. The electrical core does not
depend on the GUI.

## Status

Pre-implementation. M0 currently builds:

- `circuit-domain` — schematic entities, stable IDs, conductors and supply profiles;
- `circuit-diagnostics` — structured diagnostic codes and subjects;
- `xtask` — repository checks, including the architecture rule verifier.

The first M1 step consolidates `circuit-domain` and `circuit-diagnostics` into
`circuit-model`, then adds `circuit-sim`, `circuit-project` and `apps/cli`. The
GUI starts in M2a as a read-only viewer; the full editor is M2b. The remaining
directories are tracked placeholders for the milestones in
[`ROADMAP.md`](ROADMAP.md).

## Workspace

Current M0 layout:

```text
apps/                  placeholders
crates/
  circuit-domain/      document model and structural validation
  circuit-diagnostics/ diagnostic codes, severity and subjects
xtask/                 repository automation
```

M1 target:

```text
apps/
  cli/                 headless validation and simulation
crates/
  circuit-model/       IDs, document, catalog, diagnostics, inputs, snapshot
  circuit-sim/         topology, electrical, logic, runtime, trace
  circuit-project/     JSON .ocircuit, validation and migrations
xtask/                 repository automation
```

M2a adds `circuit-renderer` and `apps/desktop`; M2b adds `circuit-editor` and
`circuit-commands`; M3 adds `circuit-export`. `assets/`, `examples/` and
`tests/` hold project-owned symbols, reference circuits and fixtures.

The target dependency direction will be enforced by
`cargo run -p xtask -- check-architecture`; the M0 checker currently validates
the three M0 packages:

```text
circuit-model <- circuit-sim
circuit-model <- circuit-project
circuit-model <- renderer/commands/editor (M2)
editor/renderer consume neutral contracts, never simulator internals
apps/* may depend on GUI crates; circuit-* crates may not
```

## Commands

Run everything from the repository root:

```sh
cargo test --workspace --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p xtask -- check-architecture
cargo deny check
```

## Documentation

- [`ideia.md`](ideia.md) — authoritative design notes (Portuguese).
- [`OpenCircuit_Arquitetura_Tecnica_v0.2.md`](OpenCircuit_Arquitetura_Tecnica_v0.2.md) — consolidated architecture proposal.
- [`ARCHITECTURE.md`](ARCHITECTURE.md) — implementation-facing architecture summary.
- [`FORMAT.md`](FORMAT.md) — `.ocircuit` project format.
- [`SIMULATION.md`](SIMULATION.md) — virtual time and tick semantics.
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
