# AGENTS.md — instructions for agents

OpenCircuit is a clean-room, open-source Rust electrical-schematic editor and simulator for industrial control circuits (contactors, relays, timers). M1, the headless vertical slice, is implemented: `circuit-model`, `circuit-sim`, `circuit-project`, `apps/cli` and `xtask`. M2a, the viewer, and M2b, the editor, are implemented: `circuit-renderer`, `circuit-commands`, `circuit-editor` and `apps/desktop` (package and binary `opencircuit-desktop`). M3, advanced circuits and export, is implemented: changeover contacts, interlocks, the six-terminal motor, supply sections and `circuit-export` (SVG and PDF). M4, the ecosystem, is implemented: the control transformer, data-only extension libraries (`LIBRARY.md`), `circuit-io` with `opencircuit serve` (Modbus TCP), and the browser build of `apps/desktop`. M5, editor precision, electrical sheets (frames over the canvas, ADR-0011), terminal numbers, an embedded PDF font (ADR-0012) and a command line that edits (`new`, `edit --apply` with JSON edit files), is implemented. M6, the drawing-program workspace and the device catalog, is implemented: palette categories and library schema 2, symbol-only devices (ADR-0013), the standard library `std`, a themed window with menus, a status bar, a tabbed palette, a command line (`circuit_commands::commandline`) and window or crossing selection. M7, electrical documentation and productivity, is implemented: schema version 3 (ADR-0014), static nets, wire numbers, `circuit-report` and `opencircuit report`, `renumber`, the drawing warnings `OC-STR-018` and `OC-STR-019`, waivers, and the documentation sections, net trace, recent parts, parts placed on a wire and problem actions of the window. No milestone after M7 is planned yet; new work starts from an issue.

## Start every session here

Read [`ROADMAP.md`](ROADMAP.md), [`ARCHITECTURE.md`](ARCHITECTURE.md), [`EXTENDING.md`](EXTENDING.md) and the milestone you are working on. The contract-level reference is [`docs/spec02.md`](docs/spec02.md) (version 0.3, Portuguese); read its §0, §2, §5.4, §6, §8–§15, §20 and §23 before touching domain or simulation contracts. [`SIMULATION.md`](SIMULATION.md), [`FORMAT.md`](FORMAT.md) and [`LIBRARY.md`](LIBRARY.md) are the normative rules for the simulator, the project format and extension libraries; the Modbus I/O map is fixed by ADR-0009.

The workspace has twelve members: `circuit-model`, `circuit-sim`, `circuit-project`, `circuit-renderer`, `circuit-commands`, `circuit-editor`, `circuit-export`, `circuit-io`, `circuit-report`, `apps/cli` (package `opencircuit-cli`, binary `opencircuit`), `apps/desktop` (package and binary `opencircuit-desktop`) and `xtask`. `apps/desktop` declares its own `rust-version` (1.95, for eframe); the other members keep the workspace MSRV 1.85. Do not add a crate to `members` before its milestone; `xtask check-architecture` fails for a workspace package without a rule.

`ROADMAP.md` is the one-page summary of milestones and exit criteria. Pick work from the current milestone only; features outside it are not started early.

## Never crash

A crash loses an engineer's schematic or simulation state. **This outranks feature work.**

- **No panics in non-test code:** no `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!`, `unimplemented!`, and no `unsafe`. Use `Result`/`Option` and `?`. An unfinished path returns a structured error or a "not implemented yet" diagnostic; it never panics.
- **Production crate roots carry the deny attributes:** `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]`, plus `#![forbid(unsafe_code)]` and, outside tests, `clippy::indexing_slicing` and `clippy::arithmetic_side_effects` in the core. Every crate root has them; a new crate adds them with its real `src/lib.rs`. `clippy.toml` allows `unwrap`/`expect`/`panic`/indexing in tests only.
- **Input is hostile** (project files, JSON, CLI arguments, any external value): use `get()` instead of `[i]`, checked arithmetic, no NaN-to-integer casts, and cap allocations and loop counts derived from input.
- **Bound recursion** (nested or cyclic references, nested sub-circuits) with an explicit depth limit.
- **Prove it:** every crash fix lands with a regression test that panics or fails without the fix.

## Non-negotiables

- **Clean room.** Do not copy code, assets, symbol artwork, strings or help text from any electrical CAD product, from IEC 60617 files, or from GPL/LGPL/AGPL projects. Read public standards and our own design documents. Check a dependency's licence before adding it; dependencies must fit `deny.toml`.
- **Assets.** `assets/` holds the project's own assets and the few third-party files listed in `ATTRIBUTION.md`. Do not copy IEC 60617 or other third-party symbol files (architecture §18.2). A file asset must be original or under a permissive, redistributable licence, and must be recorded with author, source, licence and checksum in `ATTRIBUTION.md` in the same change, with the licence text next to the file. Only the font of the PDF export is third-party today.
- **Rust only.** No handwritten JavaScript or TypeScript in this repo.
- **Layering.** `circuit-model` has no workspace dependencies. `circuit-renderer` and every `circuit-*` core crate must not depend on `egui`/`eframe`. Core crates do no filesystem, socket or clock access. `egui`/`eframe` enter only in M2a with `apps/desktop`; verify the version with `cargo add` or the registry, not an old document value.
- **Everything user-visible is driven by the core.** UI, CLI and future tools call the same model/sim APIs; the UI holds no electrical logic.
- **Quality gates** before every commit (see Commands). Commit after each change that builds and passes its tests.

## Commands

All cargo commands run from the repo root:

```sh
cargo test --workspace --locked
cargo test -p circuit-sim
cargo test -p circuit-sim simultaneous_start   # single test substring
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked   # no broken or ambiguous doc links
cargo run -p xtask -- check-architecture
cargo run -p xtask -- generate-examples --check   # example projects match their generator
cargo run -p xtask --release -- bench               # timings recorded in docs/benchmarks.md
cargo run -p opencircuit-cli -- test examples tests/fixtures
cargo run -p opencircuit-desktop -- examples/direct-starter/starter.ocircuit   # open the viewer
cargo deny check
```

- Example projects are generated from `xtask/src/examples.rs`; edit the generator and run `cargo run -p xtask -- generate-examples`, never the `.ocircuit` by hand. Golden traces are rewritten only with `opencircuit test <path> --update-golden`, which is refused when `CI` is set; review every golden diff.

- CI is in `.github/workflows/ci.yml` and runs fmt, clippy, rustdoc with warnings denied, tests, `cargo-deny` and the architecture checker. Its jobs are the quality gate; run the same commands locally before opening a pull request.
- `rust-toolchain.toml` pins rustc 1.99.1. The root `clippy.toml` allows `unwrap`, `expect`, `panic` and indexing in tests only, and the `clippy.toml` of each core crate adds `disallowed-types` (clock, `HashMap`/`HashSet`, files and sockets). There is no `rustfmt.toml`: rustfmt runs with its defaults. Edition 2024, resolver 3, MSRV 1.85.
- Only the root `Cargo.lock` should be committed. Do not run cargo from inside a crate directory; do not add nested `Cargo.lock` or `target/` files.
- If a command cannot run in your environment, say so in the pull request and rely on CI.

## Workspace gotchas

- Root `Cargo.toml` lists members explicitly (no globs). A new crate under `crates/` or `apps/` must be added to `members`; intra-workspace deps go in `[workspace.dependencies]` and are consumed as `{ workspace = true }` (see `circuit-diagnostics`).
- Placeholder directories are tracked with `.gitkeep`. Do not add a placeholder crate to `members` until it has a real `src/lib.rs`.
- Do not run `git init` inside a crate. The accidental nested repository in `crates/circuit-domain/` was removed; keep the repository root as the only Git repository.
- Shell gotcha: `mv`/`cp` may be aliased to interactive mode; use `/bin/mv -f` / `/bin/cp -f` in scripts you run.
- Parallel agents: use a separate `CARGO_TARGET_DIR` per agent and edit only the crates you own.

## Design rules that override default instincts

- **Device ≠ symbol.** `DeviceId` is the physical device; `SymbolId` is a graphic instance. One contactor has one reference but many symbols (coil, NO/NC contacts). Duplicate references across distinct devices are diagnostics; several symbols sharing a device reference is normal. Never reintroduce string tags as identity.
- **Stable terminals.** Schematics store structural `TerminalKey` strings (`a1`, `p1`, `pole1.in`, `u1`, `out`), never positional pin indices; displayed numbers such as 13/14 are labels, not keys. Dense indices belong only in the compiled circuit and are newtypes (`PinIx`, `NetIx`, `SigIx`, `SwitchIx`, `LoadIx`, `DeviceIx`).
- **Geometry in the model.** Integer `GridPoint`, explicit `QuarterTurn`; wire endpoints are `Endpoint::Terminal(PinRef)` or `Endpoint::Junction(JunctionId)`; crossing wires are not connected. `Wire.route` stores only intermediate waypoints.
- **Document identity.** Devices store `model_id` + `model_version`, not a persisted `DeviceKind`. Terminals belong to `SymbolModel`. The document persists `next_id`; IDs are `u64` and never reused. No `uuid`.
- **Discrete electrical model only.** MVP is continuity + energized state. Copper nets (wires + junctions) are compiled once; per tick, only closed switches union them. Do not model `Potential` as L1/L2/L3/N/PE; use `ConductorSet` plus `SupplyProfile`. Do not add virtual-potential propagation or MNA. The six-terminal motor is one load evaluated by the star/delta rule of ADR-0005, never three winding loads; other series loads and star points are runtime diagnostics, not guessed energization.
- **Tick discipline.** The simulator owns a fixed `SimConfig::tick_us` (no per-call `dt`). Keep `current` vs `next` signals, timers and thermals. Build the on-demand snapshot from the `TickView` that was actually solved; logic changes apply to the next tick. External inputs are applied at the tick boundary.
- **Fail closed.** Under the default `FaultPolicy::Halt`, a short circuit (`OC-ELEC-001`, including any AC/DC mix) or a protective-earth fault (`OC-ELEC-002`) completes the faulting tick, emits a snapshot with `faulted = true`, then latches until `reset_fault()`. Each supply profile is a separate section: live conductors of two sections meeting in one group are a short (`OC-ELEC-001`), and `OC-SEM-004` is retired (ADR-0005). Rating mismatch, shorted load terminals and unsupported series loads do not latch.
- **Derived, not stored.** Cross-references, wire numbers, reports, the project tree and the drawing warnings are functions of the document and the catalog (ADR-0011, ADR-0014). Only what a person decides is stored: a wire number override, the catalog data of a device, the numbering rule and a waiver with its reason. Do not add a stored copy of something that can be derived.
- **Determinism.** Sort entities by stable IDs before assigning dense indices; sort diagnostics and short circuits; permutation invariance is a required property test, not a nice-to-have. Prefer `step_n` plus one on-demand `snapshot()` over a snapshot per tick.
- **Diagnostics are structured** (`DiagnosticCode` string codes like `OC-ELEC-001` + `Subject`/`related`), not free-form strings. Keep compile-time structural/semantic diagnostics separate from runtime electrical faults. A disconnected terminal is not automatically an error.
- **Order of work.** M0 contracts, then M1 `circuit-model` + `circuit-sim` contracts, direct-online and reversing goldens (including simultaneous S1/S3), then `circuit-project` JSON, then the read-only viewer, then the editor. Do not start with symbol drawing.

## Conventions

- Code identifiers, test names, and diagnostic messages are English even though `docs/spec02.md` is Portuguese.
- IDs are newtypes declared in `crates/circuit-model/src/ids.rs`: `Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd`, serde-transparent, no `Default` (zero is not a valid ID).
- Tests are inline `#[cfg(test)] mod tests` next to the code they cover (module file or `src/lib.rs`); golden fixtures live under `tests/fixtures` and `tests/integration`; golden scenarios live under `examples/` too.
- Every behaviour change needs a regression test. Every schema change needs a migration. Every component declares its terminals and provenance.

## Conservative interface

The window of M6 is intuitive and its users know it. Interface changes are incremental: add to the existing panels, menus and shortcuts, and do not rearrange them.

- Do not move, rename or restyle an existing control, tab, menu entry, shortcut, accessible label or icon.
- Do not refactor `apps/desktop` (`app.rs`, `palette.rs`, `inspector.rs`) beyond what the feature needs. New controls go into an existing section, or into a new collapsible section placed after the current ones.
- A fresh window looks and behaves as before: same default tab, panel sizes and mode.
- The tests in `apps/desktop/tests/` pass unchanged. Add assertions or new test files; do not edit an existing test. A needed edit is explained in the pull request.
- A new top-level panel, a new palette tab or a change of a default needs its own issue first.
- The same holds for the command line: flags, JSON field names and exit codes of existing commands do not change; additions only.

## Working rules

- **A person is accountable.** Whoever runs an agent answers for what it changes and for its review comments.
- **Check, then claim.** Before writing code for an issue, read its comments, search open and closed pull requests for the issue number and keywords, and check recent commits on `main`. Comment on the issue first. Release a claim you will not finish.
- **Verify, don't guess.** Open a definition before using it. Confirm a bug's cause in the code, even when an issue names a location.
- **Keep fixes narrow and tested.** Fix what the issue describes, where the bug is. Add a test that fails without the fix.
- **Review the diff before opening a pull request.** Identifiers exist as used, the new test fails without the fix, nothing unrelated changed, and the description is true.
- **Look for a better pull request.** If another open pull request fixes the same thing, close yours when theirs is better or equal, and say why.
- **Report only what ran.** State what you tested and what you could not run. Without a local build or test, say so and rely on CI before marking ready.
- **`main` changes through a pull request.** Do not push to `main`.
- **Stop and ask only about** licences outside the allowlist in `deny.toml`, publishing, releases, a new git remote, secrets, spending money, destructive git operations, and anything else hard to undo. Otherwise keep working.
- **Keep secrets and personal details out** of the repository, logs and requests.
- **CI changes are separate.** Do not change `.github/workflows/` in a feature or fix pull request; open its own pull request or issue.
- **Keep pull requests moving.** Answer actionable review feedback promptly. Name the affected rows of `ROADMAP.md` in the pull request description; maintainers update milestone status in batches, which keeps open pull requests from conflicting.
- **Guards say why.** A check that looks removable says what it prevents. Read that before changing it.
- **Instructions must be readable by everyone.** No notes about one machine, and no links to files a reader cannot open.
- **Local agent skills.** Rust skills are installed in `.agents/skills/` and linked from `.claude/skills/` (`rust-best-practices`, `rust-testing`, `rust-skills`, `rust-engineer`), with their pins in `skills-lock.json`. Treat their advice as guidance; this file and the architecture document win on conflicts.
