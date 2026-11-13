# OpenCircuit

OpenCircuit is an open-source electrical schematic editor and simulator focused on
industrial control circuits: contactors, relays, timers, pushbuttons and
protective devices.

The project treats the schematic as a domain model, not as a drawing. The editor
changes the model, the compiler turns it into an executable representation, and
the simulator produces deterministic snapshots. The electrical core does not
depend on the GUI.

## Status

Pre-implementation. The workspace currently builds:

- `circuit-domain` — schematic entities, stable IDs, conductors and supply profiles;
- `circuit-diagnostics` — structured diagnostic codes and subjects;
- `xtask` — repository checks, including the architecture rule verifier.

The remaining crates and applications are tracked placeholders for the
milestones described in [`ROADMAP.md`](ROADMAP.md). The GUI is intentionally not
started until the domain model is stable.

## Workspace

```text
apps/
  cli/                 headless validation and simulation (planned)
  desktop/             egui/eframe application (planned)
crates/
  circuit-domain/      document model and structural validation
  circuit-diagnostics/ diagnostic codes, severity and subjects
  ...                  remaining layers (placeholders)
assets/                project-owned symbols, icons and themes
examples/              reference circuits and golden scenarios
tests/                 integration fixtures
xtask/                 repository automation
```

The dependency direction is enforced by `cargo run -p xtask -- check-architecture`:

```text
domain <- diagnostics <- topology <- electrical <- simulation
                                \-> logic ------/
editor/renderer consume neutral contracts, never simulator internals
apps/* may depend on GUI crates; circuit-* crates may not
```

## Commands

Run everything from the repository root:

```sh
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p xtask -- check-architecture
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
