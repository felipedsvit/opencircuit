# AGENTS.md

## What this repo is

OpenCircuit is a Rust electrical-schematic editor/simulator for industrial control circuits (contactors, relays, timers). It is **pre-implementation**: the workspace builds only `circuit-domain` and `circuit-diagnostics`; every other `crates/*` and `apps/*` directory is an empty placeholder.

`ideia.md` is the authoritative design spec (in Portuguese). Read it before touching domain or simulation contracts — especially §2 (critical corrections), §3 (data model), §4 (crate contracts), §12 (roadmap), and §14/§15 (final layout and decisions). Existing code already implements §2.1/§2.5/§3.1/§3.3.

## Commands

All cargo commands run from the repo root:

```sh
cargo test --workspace
cargo test -p circuit-domain
cargo test -p circuit-domain symbol_instance_references_its_device   # single test substring
cargo fmt --all -- --check
cargo clippy --workspace --all-targets
```

- No CI, `rustfmt.toml`, `clippy.toml`, or `rust-toolchain.toml` yet. Local toolchain is rustc 1.99.1; edition 2024, resolver 3.
- Only the root `Cargo.lock` should be committed. Stray `crates/*/Cargo.lock` and `crates/*/target/` already exist because cargo was once run inside crates — run from the root, do not add them.

## Workspace gotchas

- Root `Cargo.toml` lists members explicitly (no globs). A new crate under `crates/` or `apps/` must be added to `members`; intra-workspace deps go in `[workspace.dependencies]` and are consumed as `{ workspace = true }` (see `circuit-diagnostics`).
- `crates/circuit-domain/` contains its own empty `.git` from an accidental `git init`. The parent repo sees it as one embedded `?? crates/circuit-domain/` entry, so **its files cannot be committed** until that `.git` directory is removed.
- The initial commit tracks only `.gitignore`, `LICENSE`, and `README.md`; everything else is untracked.

## Design rules that override default instincts

- **Device ≠ symbol.** `DeviceId` is the physical device; `SymbolId` is a graphic instance. One contactor has one reference but many symbols (coil, NO/NC contacts). Duplicate references across distinct devices are diagnostics; several symbols sharing a device reference is normal. Never reintroduce string tags as identity.
- **Stable terminals.** Schematics store `TerminalKey` strings (`A1`, `13`, `U1`), never positional pin indices. Dense indices belong only in the compiled netlist and must be newtypes (`PinIx`, `SigIx`).
- **Geometry in the model.** Integer `GridPoint`, explicit `QuarterTurn`; wire endpoints are `Terminal(PinRef)` or `Junction(JunctionId)`; crossing wires are not connected.
- **Discrete electrical model only.** MVP is continuity + energized state. Do not model `Potential` as L1/L2/L3/N/PE; the spec replaces it with conductors plus `SupplyProfile`. Do not add virtual-potential propagation, MNA, or the six-terminal 3-phase motor before its milestone.
- **Tick discipline.** The simulator owns a fixed `tick_duration_us` (no per-call `dt`). Keep `current` vs `next` device signals and build snapshots from the state that was actually solved; logic changes apply to the next tick.
- **Determinism.** Sort entities by stable IDs before assigning dense indices; sort diagnostics/short circuits; permutation invariance is a required property test, not a nice-to-have.
- **Diagnostics are structured** (`DiagnosticCode` + `Subject`/`related`), not free-form strings. Keep compile-time structural/semantic diagnostics separate from runtime electrical faults. A disconnected terminal is not automatically an error.
- **Order of work.** M0 contracts, then M1 headless logic engine (direct-online/reversing circuits, TON/TOF, golden tests), then GUI. Do not start with symbol drawing. `circuit-domain` must have no workspace dependencies; `circuit-renderer` must not depend on egui.
- **Symbol assets.** `assets/` is for project-owned assets; do not copy IEC 60617 or other third-party symbol files (see §10).

## Conventions

- Code identifiers, test names, and diagnostic messages are English even though `ideia.md` is Portuguese.
- IDs are newtypes; derives follow `crates/circuit-domain/src/lib.rs` (`Clone, Copy, Debug, Eq, Hash, PartialEq`, plus `Ord/PartialOrd` when sorted).
- Tests are inline `#[cfg(test)] mod tests` in each `src/lib.rs`; there are no integration tests or fixtures yet.
