# Roadmap

The roadmap follows the consolidated architecture proposal. Each milestone has
an observable exit criterion; features outside the current milestone are not
started early. The authoritative detail is in
[`docs/spec02.md`](docs/spec02.md); milestone numbers here are the ones used
throughout the repository (the specification's §20 follows them).

## M0 — Foundation and contracts (done)

- [x] Workspace, toolchain, resolver and explicit members.
- [x] Stable IDs and device/symbol separation.
- [x] Stable terminal keys.
- [x] Conductors and explicit supply profiles.
- [x] Structural diagnostics and validation.
- [x] Architecture rule checker.
- [x] CI with fmt, clippy, tests, `cargo-deny` and architecture checks.
- [x] Initial tests for the domain and diagnostics contracts.

**Exit criterion:** documented contracts that compile and are covered by initial
tests. All checks pass at the M0 revision.

M0 built `circuit-domain` and `circuit-diagnostics`; M1 consolidated them
into `circuit-model`.

## M1 — Headless vertical slice (done)

- [x] Consolidate `circuit-domain` + `circuit-diagnostics` into `circuit-model`.
- [x] `circuit-sim` with compile, solve, logic, runtime and trace modules.
- [x] Static copper nets; per-tick union only through closed switches.
- [x] Discrete electrical solver with conductor sets and the conflict table.
- [x] Catalog for Q1, F1, S1, S2, S3, K1, K2, timers, lamps and a simplified
  three-phase motor.
- [x] Pushbuttons, maintained switch, coils, contacts, TON/TOF and thermal latch.
- [x] Direct-online golden and reversing golden, including simultaneous S1+S3.
- [x] `circuit-project` JSON v1 with `next_id`, strict loading and round-trip.
- [x] `apps/cli` with `validate`, `simulate` and `test`.
- [x] Change-log trace, not one snapshot per tick.
- [x] CI runs the goldens on Linux, Windows and macOS and checks the core for
  `wasm32-unknown-unknown`.

**Exit criterion:** direct-online and reversing scenarios run headless with
deterministic change-log traces; the simultaneous reversing case latches at the
first short; JSON round-trip and CLI exit codes are tested; the properties of
spec §17.3 that apply to M1 pass. No GUI dependency in the core.

## M2a — Read-only viewer

- `circuit-renderer` with a neutral scene.
- `apps/desktop` with egui/eframe.
- Load `.ocircuit`, compile and simulate.
- Play, pause, step, reset and speed.
- Conductor colors, closed contacts, energized coils, motor and thermal state.
- Compile and runtime diagnostics panel.
- `step_n` per frame and one `snapshot()` per frame.

**Exit criterion:** open the direct-online fixture, play and pause, inspect the
diagnostics and verify that the renderer does not depend on `circuit-sim`.

## M2b — Editor

- `circuit-commands` with transactional edits and undo/redo.
- `circuit-editor` with selection, hit-test, insertion, movement, rotation,
  deletion, wires and junctions.
- Component palette, property inspector and parameter editing.
- Save, close and reopen.
- `egui_kittest` flows for the essential operations.

**Exit criterion:** draw, save, reopen and simulate a direct-online starter,
with undo/redo and no partially applied invalid operation.

## M3 — Advanced circuits and export

- Changeover contacts and full interlocking.
- Six-terminal motor and windings.
- Star-delta starting without star/triangle overlap.
- Multiple galvanically separate supply sections.
- SVG export from the neutral scene.
- PDF decision after a spike.
- Benchmarks for larger documents.

**Exit criterion:** star-delta is validated with an adequate winding model and
no improper contactor overlap; SVG is deterministic; PDF is either chosen with a
recorded rationale or explicitly deferred.

## M4 — Ecosystem

- WASM application.
- Plugins.
- Modbus TCP, OPC UA and MQTT evaluation.
- More components and examples.
- Third-party catalog and symbol library.
- Extension and community documentation.

**Exit criterion:** extensible platform with documented integration interfaces
and explicit safety limits.

## Out of scope for the MVP

Analog simulation, rigorous voltage/current/impedance calculation, transients,
pneumatics, hydraulics, real PLC execution, industrial protocols, plugins,
real-time collaboration, proprietary CAD import and a certified compliance
claim.
