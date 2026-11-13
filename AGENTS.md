# AGENTS.md

## What this repo is

OpenCircuit is a Rust electrical-schematic editor/simulator for industrial control circuits (contactors, relays, timers). It is **pre-implementation**: M0 builds `circuit-domain`, `circuit-diagnostics` and the `xtask` repository tool; every other `crates/*` and `apps/*` directory is a tracked placeholder.

The M1 target is five workspace members: `circuit-model`, `circuit-sim`, `circuit-project`, `apps/cli` and `xtask`. At the start of M1, `circuit-domain` and `circuit-diagnostics` are consolidated into `circuit-model`; `circuit-topology`, `circuit-electrical`, `circuit-logic` and `circuit-simulation` become modules of `circuit-sim`. Do not add the M2/M3 crates to `members` before their milestone.

`ideia.md` is the original design review (in Portuguese). `OpenCircuit_Arquitetura_Tecnica_v0.2.md` (document version 0.2.1) is the authoritative contract-level reference; read §0, §2, §5.6, §6, §8–§15, §20 and §23 before touching domain or simulation contracts. Existing M0 code implements the foundational identity, geometry and supply contracts; the document records the M1 migration rather than pretending the repo already has the target layout.

## Commands

All cargo commands run from the repo root:

```sh
cargo test --workspace --locked
cargo test -p circuit-domain
cargo test -p circuit-domain symbol_instance_references_its_device   # single test substring
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p xtask -- check-architecture
cargo deny check
```

- CI is in `.github/workflows/ci.yml` and runs fmt, clippy, tests, `cargo-deny` and the architecture checker. `rust-toolchain.toml` pins rustc 1.99.1; there is no `rustfmt.toml` or `clippy.toml` yet. Edition 2024, resolver 3, MSRV 1.85.
- Only the root `Cargo.lock` should be committed. Do not run cargo from inside a crate directory; do not add nested `Cargo.lock` or `target/` files.
- After the M1 consolidation, replace `-p circuit-domain` with `-p circuit-model`.

## Workspace gotchas

- Root `Cargo.toml` lists members explicitly (no globs). A new crate under `crates/` or `apps/` must be added to `members`; intra-workspace deps go in `[workspace.dependencies]` and are consumed as `{ workspace = true }` (see `circuit-diagnostics`).
- Do not run `git init` inside a crate. The accidental nested repository in `crates/circuit-domain/` was removed; keep the repository root as the only Git repository.
- Placeholder directories are tracked with `.gitkeep`. Do not add a placeholder crate to `members` until it has a real `src/lib.rs`.
- `egui`/`eframe` are not workspace dependencies in M1. They enter in M2a with `apps/desktop`; verify the version with `cargo add` or the registry instead of copying an old document value.

## Design rules that override default instincts

- **Device ≠ symbol.** `DeviceId` is the physical device; `SymbolId` is a graphic instance. One contactor has one reference but many symbols (coil, NO/NC contacts). Duplicate references across distinct devices are diagnostics; several symbols sharing a device reference is normal. Never reintroduce string tags as identity.
- **Stable terminals.** Schematics store `TerminalKey` strings (`A1`, `13`, `U1`), never positional pin indices. Dense indices belong only in the compiled circuit and must be newtypes (`PinIx`, `SignalIx`).
- **Geometry in the model.** Integer `GridPoint`, explicit `QuarterTurn`; wire endpoints are `Terminal(PinRef)` or `Junction(JunctionId)`; crossing wires are not connected. `Wire.route` stores only intermediate waypoints.
- **Document identity.** Devices store `model_id` + `model_version`, not a persisted `DeviceKind`. Terminals belong to `SymbolModel`. The document persists `next_id`; IDs are `u64` and never reused. No `uuid`.
- **Discrete electrical model only.** MVP is continuity + energized state. Copper nets (wires + junctions) are compiled once; per tick, only closed switches union them. Do not model `Potential` as L1/L2/L3/N/PE; use `ConductorMask` plus `SupplyProfile`. Do not add virtual-potential propagation, MNA, or the six-terminal 3-phase motor before its milestone. Series loads and floating star points are runtime diagnostics, not guessed energization.
- **Tick discipline.** The simulator owns a fixed `tick_duration_us` (no per-call `dt`). Keep `current` vs `next` signals, timers and thermals. Build the on-demand snapshot from the `TickView` that was actually solved; logic changes apply to the next tick. External inputs are applied at the tick boundary.
- **Fail closed.** Short, protective-earth fault and incompatible supplies complete the faulting tick, emit a snapshot with `faulted = true`, then latch until `reset_fault()`. Rating mismatch, shorted load terminals and unsupported series loads do not latch.
- **Determinism.** Sort entities by stable IDs before assigning dense indices; sort diagnostics/short circuits; permutation invariance is a required property test, not a nice-to-have. Prefer `step_n` plus one on-demand `snapshot()` over a snapshot per tick.
- **Diagnostics are structured** (`DiagnosticCode` string codes like `OC-ELEC-001` + `Subject`/`related`), not free-form strings. Keep compile-time structural/semantic diagnostics separate from runtime electrical faults. A disconnected terminal is not automatically an error.
- **Order of work.** M0 contracts, then M1 `circuit-model` + `circuit-sim` contracts, direct-online and reversing goldens (including simultaneous S1/S2), then `circuit-project` JSON, then the read-only viewer, then the editor. Do not start with symbol drawing. `circuit-model` must have no workspace dependencies; `circuit-renderer` must not depend on egui.
- **Symbol assets.** `assets/` is for project-owned assets; do not copy IEC 60617 or other third-party symbol files (see architecture §18.2).

## Conventions

- Code identifiers, test names, and diagnostic messages are English even though `ideia.md` is Portuguese.
- IDs are newtypes; derives follow `crates/circuit-model/src/ids.rs` after the M1 consolidation (currently `crates/circuit-domain/src/ids.rs`): `Clone, Copy, Debug, Eq, Hash, PartialEq`, plus `Ord/PartialOrd` when sorted.
- Tests are inline `#[cfg(test)] mod tests` next to the code they cover (module file or `src/lib.rs`); golden fixtures live under `tests/fixtures` and `tests/integration`.
