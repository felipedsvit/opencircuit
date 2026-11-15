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

## M2a — Read-only viewer (done)

- [x] `circuit-renderer` with a neutral scene.
- [x] `apps/desktop` with egui/eframe.
- [x] Load `.ocircuit`, compile and simulate.
- [x] Play, pause, step, reset and speed.
- [x] Conductor colors, closed contacts, energized coils, motor and thermal state.
- [x] Compile and runtime diagnostics panel.
- [x] `step_n` per frame and one `snapshot()` per frame.

**Exit criterion:** open the direct-online fixture, play and pause, inspect the
diagnostics and verify that the renderer does not depend on `circuit-sim`.

## M2b — Editor (done)

- [x] `circuit-commands` with transactional edits and undo/redo.
- [x] `circuit-editor` with selection, hit-test, insertion, movement, rotation,
  deletion, wires and junctions.
- [x] Component palette, property inspector and parameter editing.
- [x] Save, close and reopen.
- [x] `egui_kittest` flows for the essential operations.

**Exit criterion:** draw, save, reopen and simulate a direct-online starter,
with undo/redo and no partially applied invalid operation.

## M3 — Advanced circuits and export (done)

- [x] Changeover contacts and full interlocking (combined pushbuttons and
  mechanical interlocks).
- [x] Six-terminal motor and windings.
- [x] Star-delta starting without star/triangle overlap.
- [x] Multiple galvanically separate supply sections.
- [x] SVG export from the neutral scene.
- [x] PDF decision after a spike: `pdf-writer`, implemented (ADR-0006).
- [x] Benchmarks for larger documents.

**Exit criterion:** star-delta is validated with an adequate winding model and
no improper contactor overlap; SVG is deterministic; PDF is either chosen with a
recorded rationale or explicitly deferred.

The decisions of M3 are in ADR-0005 (semantics) and ADR-0006 (PDF). The
control transformer, which would feed one section from another, is deferred
to M4.

## M4 — Ecosystem (done)

- [x] WASM application: the desktop application builds for the browser and
  is checked in CI; it is not published.
- [x] Plugins, as data-only extension libraries (ADR-0008): no code runs.
- [x] Modbus TCP server (`circuit-io`, `opencircuit serve`, ADR-0009); OPC UA
  and MQTT evaluated and not implemented (ADR-0010).
- [x] More components and examples: control transformer (ADR-0007), latching
  emergency stop, limit switch, pump-level example.
- [x] Third-party catalog and symbol library: namespaced models with declared
  provenance and their own artwork (`LIBRARY.md`).
- [x] Extension and community documentation (`EXTENDING.md`,
  `CONTRIBUTING.md`, `SECURITY.md`).

**Exit criterion:** extensible platform with documented integration interfaces
and explicit safety limits. The exit test loads the example third-party
library, simulates the project that uses it against its golden trace and
operates it through the Modbus server
(`m4_exit_a_library_component_is_simulated_and_served` in
`apps/cli/tests/cli.rs`); the interfaces and their limits are in
`EXTENDING.md`.

## M5 — Editor precision, electrical sheets and CLI automation (done)

Tracking issue #56. Clean room: ideas from other editors shaped the
interaction; no code, strings or artwork was copied.

Editor precision (the file format did not change):

- [x] Snapping to pins, junctions and wires within 12 screen points, with
  markers.
- [x] Visible grid; keys for the wire tool, nudging, selecting all and zooming.
- [x] Duplicate, copy and paste, with designators renumbered.
- [x] Wire bends kept when one end moves; bend direction chosen while wiring.
- [x] Editing several elements at once; align and distribute.

Electrical documentation (schema version 2, ADR-0011 and ADR-0012):

- [x] Terminal numbers (A1/A2, 13/14, 95/96) on the drawings, derived from the
  symbol, the device class and the slot, so library models get them too.
- [x] Sheets as frames over the canvas, with paper size, zones and a title
  block; a sheets and title block section in the inspector.
- [x] One PDF page per sheet, in an embedded Liberation Mono subset;
  `opencircuit export --sheet`.
- [x] Cross-references between coils and their contacts (sheet and zone).

Automation through the command line (data only, no script engine):

- [x] `--format json` on every command that reports a result.
- [x] `opencircuit new` and `opencircuit edit --apply <edit.json>`.

**Exit criterion:** a project made only through the command line, with two
sheets, terminal labels and cross-references, validates, simulates and
exports as a multi-page PDF; in the editor, snapping, duplicating and
nudging work and are undone in one step each. The exit test is
`m5_exit_a_project_made_only_through_the_command_line_validates_simulates_and_exports`
in `apps/cli/tests/cli.rs`; the editor behaviour is covered by the window
tests in `apps/desktop/tests/editor.rs`; the formats and limits are in
`EXTENDING.md`.

Considered and not done in M5: an MCP server (the edit file and the JSON
output are the interface it would sit on), separate sheets joined by off-page
connectors (it would supersede ADR-0011), DXF and DWG, and a cache of the
scene for documents beyond about 5,000 devices (`docs/benchmarks.md`). New
work starts with an issue.

## Out of scope for the MVP

Analog simulation, rigorous voltage/current/impedance calculation, transients,
pneumatics, hydraulics, real PLC execution, industrial protocols, plugins,
real-time collaboration, proprietary CAD import and a certified compliance
claim. M4 added data-only libraries and a Modbus TCP server for testing;
code plugins, a Modbus client and control of real equipment remain out of
scope.
