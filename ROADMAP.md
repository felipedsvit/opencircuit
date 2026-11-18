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

## M6 — Drawing-program workspace and device catalog (done)

Tracking issue #73. Clean room: the layout of another open-source editor
shaped the structure of the window only; the colours, the icons (painted in
code) and every string are this project's own.

Catalog (library schema version 2, ADR-0013):

- [x] Palette categories (eight tabs), titles and notes on component models.
- [x] Symbol-only devices: class `symbolic`, block kinds of 2, 4 and 8
  terminals, informational `OC-SEM-011`; they never conduct and never latch.
- [x] The standard library `std` (`circuit_project::stdlib`): 63 parts, 43
  simulated and 20 symbol-only, for the eight tabs; the command line starts
  from it.

Window (`apps/desktop`, no new dependency):

- [x] Theme tokens for a dark and a light theme, and 44 icons drawn in code.
- [x] Menu bar, quick-access bar, document tab, status bar (pointer, sheet
  tabs, grid and snap, problems), zoom buttons, Help windows.
- [x] Palette with a tab per kind of part, search and symbol drawings.
- [x] Inspector in folding sections with a summary of the selection.
- [x] A command line over the drawing, read by `circuit_commands::commandline`.
- [x] Selection by window or crossing, a context menu and a cross cursor.

**Exit criterion:** a project is drawn with parts from the eight tabs,
including symbol-only ones that are marked as such, through the palette, the
context menu and the command line; the same project validates and simulates
from the command line with no library file. The tests are in
`apps/desktop/tests/` (`palette.rs`, `commandline.rs`, `canvas.rs`,
`chrome.rs`, `properties.rs`), `circuit-commands` (`commandline`), `circuit-project`
(`stdlib`) and `apps/cli/tests/cli.rs`
(`the_standard_parts_are_available_to_every_command_without_a_library_file`).

Considered and not done in M6 (each needs a change of the simulator or of
the format, and starts with an issue): supply symbols with several
conductors, a simulated three-phase transformer and rectifier, selectors
with a middle position, interlocks between switches, a static colour for a
wire (schema version 3), layers, a text and annotation entity (ladder and
GRAFCET as diagrams), clicking for the points of a command, an ORTHO switch
(wires are always orthogonal), tabs for several documents, localisation.

## M7 — Electrical documentation and productivity (done)

Tracking issue #85. Clean room: the workflows of other electrical CAD tools
shaped the scope only; no code, strings, artwork or symbols were copied. The
window changed only by addition (the `Conservative interface` rule of
`AGENTS.md`). Schema version 3 and ADR-0014.

Derived from the document, never stored:

- [x] Static nets in `circuit-model` (`StaticNets`), kept in step with the
  compiler by a test.
- [x] Wire numbers from a rule of the project (prefix, start, step, restart
  on each sheet), an override on a wire, `OC-SEM-012` for conflicting
  overrides, and the number drawn on the wire.
- [x] Reports: `Table` and the crate `circuit-report` (bill of materials,
  wires, terminals, cross-reference exceptions, I/O map, problems);
  `opencircuit report` writes text, JSON, CSV and, in the embedded font, PDF.
- [x] `renumber`: designators in reading order, one undo, from the Modify
  menu, the command line and the edit file.
- [x] Drawings that look connected and are not: `OC-STR-018` (a terminal or
  junction on another terminal, junction or wire) and `OC-STR-019` (wires of
  different nets over one line).

Stored (schema version 3, all optional):

- [x] `Wire.number`, `Device.info` (manufacturer, part number, description,
  location), `project.wire_numbering` and `waivers`; migration from version 2.
- [x] Waivers: a warning accepted with a reason (`OC-STR-020` for a waiver
  that cannot work); `validate` marks them and `--deny-warnings` ignores them.

Window (additions to the existing panels and menus):

- [x] Inspector sections: Connections, Cross-references, Catalog data, Wire
  number and Project tree, and the numbering rule in the sheets section.
- [x] Trace the net of the selection (View menu, key T).
- [x] A strip of the parts chosen most recently, and a two-terminal part
  clicked on a wire goes in series with it.
- [x] The problems list: Connect, Ignore with a reason, Waived and Restore,
  Save list.

Command line: `report`, and the edit-file operations `set_device_info`,
`set_wire_number`, `numbering`, `waive`, `unwaive` and `renumber`.

**Exit criterion:** a project made only through the command line, on two
sheets, with a numbering rule and the catalog data of its parts, validates,
reports its bill of materials, wires, terminals, cross-reference exceptions
and I/O map as the committed CSV files, writes them as a PDF, renumbers
without duplicates, and has a warning waived with a reason that no longer
fails `--deny-warnings`. The exit test is
`m7_exit_a_project_documented_through_the_command_line_reports_numbers_and_exports`
in `apps/cli/tests/cli.rs`; the window is covered by
`apps/desktop/tests/project.rs`; the formats and limits are in `FORMAT.md` and
`EXTENDING.md`.

Considered and not done in M7: off-page signal arrows (they would supersede
ADR-0011), ladder diagrams and a text and annotation entity, terminal strips
and PLC modules, a library tab of ready-made circuits with parameters, an MCP
server (the edit file and the JSON output are still the interface it would sit
on), manufacturer catalog import, and a static wire colour. M8 did the signal
arrows, the text entity, the terminal strips and the MCP server.

## M8 — Panel documentation and an agent interface (done)

Tracking issue #107 (issues #101 to #106 and #103). Clean room, as for M7: the
workflows of other electrical CAD tools shaped the scope only. The window
changed only by addition (`Conservative interface` in `AGENTS.md`). Schema
version 4 (ADR-0015), library schema 3 (ADR-0016), ADR-0017.

- [x] **MCP server** (`opencircuit mcp`, #104): JSON-RPC 2.0 over stdio with the
  tools `new`, `validate`, `simulate`, `edit`, `report` and `export`, confined to
  one root directory (no absolute path, no `..`, no symlink out), with bounded
  messages and results. It calls the same code as the command line.
- [x] **Terminal blocks and the terminal plan** (#105, ADR-0016): the name
  parameter type (library schema 3), `std.terminal_block` (strip and number), the
  report `strips`, and `OC-SEM-013` for a number used twice on a strip.
- [x] **Signal arrows between sheets** (#106, ADR-0016): every arrow with one
  name is one net, on a sheet or across sheets, joined by the compiler and by the
  static nets; each arrow shows where the others are; `OC-SEM-014` for an arrow
  with no partner. `std.signal_arrow`.
- [x] **Text notes** (#101, ADR-0015, schema version 4): one line, 1 to 120
  characters, documentation only (no effect on the circuit or the reports), drawn
  and exported; the Text tool, `text` and the inspector section; `OC-STR-021`.
- [x] **Layers** (#102, ADR-0015): hide (not drawn, exported or hit) and lock
  (edits refused) named groups of symbols, wires and notes, with no effect on the
  simulation; the `layer` command and a section of the inspector;
  `OC-STR-022`.
- [x] **The window in Brazilian Portuguese** (#103, ADR-0017): a catalog with
  English as the source and default, chosen from the environment and from the
  Help menu; diagnostics, the command line and the file formats stay English.

Command line: `mcp`, and the edit-file operations `add_text`, `set_text`,
`add_layer`, `set_layer`, `remove_layer` and `assign_layer`.

**Exit criterion:** a control circuit drawn on two sheets with no wire between
them (`examples/signal-arrows`) validates without warnings, simulates to the
committed trace, and its wires report shows one net across both sheets; a strip of
terminals (`examples/terminals`) is listed in the committed terminal plan and the
circuit still runs through it; a project made only through the edit file gets
notes and layers, a hidden layer is left out of the exported drawing, a locked
layer refuses edits and the simulation is the same either way; the MCP server
runs the same operations over a pipe and stays inside its root; and the window
opens in Portuguese, edits and undoes. The tests are
`a_circuit_continues_from_one_sheet_to_the_next_through_signal_arrows`,
`the_terminal_plan_of_the_example_is_the_committed_one_and_the_circuit_runs_through_the_strip`,
`text_notes_are_drawn_edited_and_left_out_of_every_report`,
`layers_are_made_assigned_hidden_and_a_hidden_layer_is_left_out_of_the_drawing`
and `the_mcp_server_answers_over_a_pipe_and_stays_inside_its_root` in
`apps/cli/tests/cli.rs`, and `apps/desktop/tests/{notes,layers,language}.rs`.

Considered and not done in M8: ladder diagrams and GRAFCET, PLC modules, a
library tab of ready-made circuits, manufacturer catalog import, a static wire
colour, colours and line types per layer, a "current layer", rotated or
several-line text, other languages and a stored language setting, and a
translated command line. New work starts with an issue.

## M9 — Work safety, productivity and new document views (implemented, except two items of M9.0; version 0.0.2)

Tracking issue #118. Version 0.0.2 closes M9. The scope comes from a triage of the features of
electrical CAD tools against this repository: what already exists was left
out, and what conflicts with the discrete model, the clean room or the scope
below is listed at the end of this section. Clean room as for M5 to M8: other
tools shaped the scope only; no code, strings, artwork or symbol files are
copied.

Decisions (taken by default, open to change in #118):

- **PLC:** modules with channels and an I/O mapping, documentation only. A
  PLC program still runs outside OpenCircuit, against the Modbus server; "real
  PLC execution" stays out of scope.
- **Single-line diagram and panel layout** are kinds of sheet in the same
  project, sharing its devices (the K1 of the panel is the K1 of the
  schematic). They take no part in the simulation.
- **Recovery copies** live in a per-user data folder (browser storage in the
  web build), never next to the person's file; backups next to the project
  are opt-in.
- **Interface:** every addition keeps a fresh window as it is (same default
  tab, panels, sizes and mode). Docking, toolbars, themes and workspaces are
  opt-in, and the window tests in `apps/desktop/tests/` pass unchanged.
- **One schema change for the milestone:** version 5 (ADR-0018), with a
  migration from version 4. Documentation fields never change compilation,
  energization or faults.
- **No new workspace crate:** the work fits the crates that exist (the
  window, the command line, `circuit-model`, `circuit-commands`,
  `circuit-report`, `circuit-export`).

### M9.0 — Resilience (done, except where marked: both items left are outside a feature pull request)

- [x] No crash on a command-line argument that is not UTF-8 or on a standard
  stream that cannot be written.
- [x] Saves: exclusive temporary file with a short name, the directory flushed
  after the rename, permissions, group and links of the replaced file kept.
- [x] A command never writes over a file it reads (`simulate --trace`,
  `--snapshot-json`, `edit --output`, `test --update-golden`).
- [x] A byte order mark at the start of a file is accepted.
- [x] The files of v0.0.1 (`tests/compat/v0.0.1`) open, migrate and reproduce
  their traces; a truncated project is refused, never read as another one.
- [x] Load tests of the Modbus server; hostile-input property tests of the MCP
  server and of scenarios.
- [x] CI runs locally with `act` (`docs/local-ci.md`).
- [x] The Modbus server frees the slots of closed connections before it
  refuses a new client.
- [ ] CI, in its own pull request (`.github/workflows/` never changes in a
  feature pull request): clippy on Windows, actions pinned by commit, a
  `cargo test --release` job and a coverage threshold.
- [ ] macOS: reproduce the native file dialogs opened inside the event loop;
  switch to asynchronous dialogs if it is confirmed.

### M9.1 — Work safety (window and command line, no schema change)

- [x] Autosave to a recovery copy, recovery offered after an unexpected exit,
  and an indicator in the status bar (native window;
  `apps/desktop/tests/recovery.rs`).
- [x] Autosave in the browser build, into browser storage (`localStorage`,
  offered again when the page is opened; `apps/desktop/src/browser_recovery.rs`,
  `apps/desktop/tests/browser_and_backups.rs`).
- [x] A panic in the window writes the recovery copy before the process ends,
  and a copy whose window has stopped is offered at once (the window holds an
  operating-system lock while it runs).
- [x] Optional rotating backups next to the project (File > Backups next to
  the project: none, 1, 3 or 5 as `name.ocircuit.bak1`…; off by default).
- [x] Project lock: a second window on the same file is warned in the status
  bar; a lock is released however its window ends, so it is never stale
  (`apps/desktop/tests/project_lock.rs`). Same user and machine only; the
  command line does not take it.
- [x] Version history: a copy per save in the data folder (the newest 20),
  opened as a new, unsaved project from File > Version history
  (`apps/desktop/tests/versions.rs`).
- [x] Semantic comparison of two projects: `opencircuit diff`
  (`apps/cli/tests/diff.rs`), and in the window the number of differences of
  each version from the project.

### M9.2 — Productivity (no schema change)

- [x] Recent projects, project templates (File > New from template) and a
  saved copy of a project (`apps/desktop/tests/files_m9.rs`).
- [x] Command palette (Ctrl+K) and search over the whole project
  (`circuit_commands::find`, `apps/desktop/tests/command_palette.rs`).
- [x] Reports: `devices` (one row per device) and renumbering of sheets
  (`renumber_sheets` in edit files, `sheets renumber` on the command line of
  the window).
- [x] Navigation: previous view (Alt+Left), highlight of the element under the
  pointer, snapping to wire ends, middles and crossings
  (`circuit_editor::snap_with`), coordinates typed at the pointer (G), rulers;
  the three view options are off in a fresh window
  (`apps/desktop/tests/navigation.rs`).
- [x] Print preview (File > Print preview, `apps/desktop/tests/print_preview.rs`);
  export of several projects in one command (`opencircuit export ...
  --output-dir <dir> --to svg|pdf`, `apps/cli/tests/export_many.rs`).
- [x] Find and replace (designators, notes, parameters; `replace` in edit
  files); editing every common field of a selection (catalog data of devices
  of any model, besides the parameters of one model); editing a label on the
  canvas (F2) (`apps/desktop/tests/find_replace.rs`).
- [x] Wire tools: trim, extend and a parallel copy (`circuit_commands::wire_tools`;
  `trim`, `extend` and `offset` on the command line of the window and in the
  context menu of the drawing; `apps/desktop/tests/wire_tools.rs`).
- [x] Macros: record the edits of a session as an edit file and play it back
  (`circuit_commands::record`, verified by running the operations; the edit
  file reader moved to `circuit_commands::script`; `apps/desktop/tests/macros.rs`).
- [x] Customizable shortcuts, themes, a collapsible sidebar, toolbars, dockable
  panels, saved workspaces: View > Theme, Show the side panels (Ctrl+B), Show
  the tool bar, panels on either side, Workspaces and Customize shortcuts,
  kept in the user's data folder (`apps/desktop/src/workspace.rs`,
  `apps/desktop/tests/workspace.rs`). Panels dock on the left or the right;
  floating panels are not part of it.
- [x] Several open projects, in tabs (File > New tab, Open in a new tab, Close
  tab; each tab keeps its autosave and lock; `apps/desktop/tests/tabs.rs`).
- [x] Wire routing on request: an orthogonal route around symbols, undone in
  one step (`circuit_commands::route`; Modify > Route the selected wires,
  `route` on the command line and in edit files; `apps/desktop/tests/route.rs`).

### M9.3 — Schema version 5 (ADR-0018)

- [x] ADR-0018 and schema version 5, with the migration from version 4
  (`crates/circuit-project/src/migrate.rs`) and the model of every item below
  (`crates/circuit-model/src/annotations.rs`, warnings `OC-STR-023` to
  `OC-STR-025`).
- [x] The client in the project data (title block; inspector Project tree >
  Project data; `apps/desktop/tests/schema5.rs`).
- [x] Wires: a stored colour, cross-section, type, material and insulation, in
  the wires report (`set_wire_spec`; inspector Wire data;
  `apps/desktop/tests/schema5.rs`).
- [x] Revision clouds, comments on an element, and shapes (polyline,
  rectangle, circle, arc) as annotations (`circuit_commands::annotations`,
  `shape` and `comment` commands, inspector Project tree > Comments).
- [x] Groups of elements, and mirrored symbols (Modify menu, M,
  `group`/`ungroup`/`mirror`).
- [x] QR codes as annotations (`qrcodegen`, `qr` command).
- [x] Manufacturer data imported from CSV into the catalog data of devices, and
  a datasheet link per device opened by the system
  (`circuit_commands::catalog_csv`, Modify > Import catalog data,
  `import_catalog`; inspector Catalog data > Open datasheet).
- [x] Sheet kinds: single-line diagram and panel layout, with cross-references
  to the schematic (device views with the schematic place of their device;
  `OC-STR-024`; `examples/panel-documentation`).
- [x] PLC modules with channels and an I/O mapping report (with the Modbus
  map): `opencircuit report --kind plc`, `OC-STR-025`.

### M9.4 — Symbols and exchange

- [x] A symbol editor that writes extension libraries (`LIBRARY.md`): File >
  Symbol editor… (`apps/desktop/src/symbol_editor.rs`,
  `apps/desktop/tests/symbols.rs`).
- [x] Symbol sets in an IEC-like and an ANSI-like style, drawn for this
  project, with no claim of conformance (`libraries/iec-style.oclib`,
  `libraries/ansi-style.oclib`, File > Symbol sets).
- [x] DXF export from the neutral scene (`circuit_export::dxf`, AutoCAD R12;
  `opencircuit export --output file.dxf`, `--to dxf`; Export > DXF in the
  window).

**Exit criterion:** a project is drawn in the window with autosave on; the
process is killed and the project is recovered with no edit lost; a second
window on the same file is warned; the project gets a single-line sheet and a
panel-layout sheet that share its devices, wire data and a PLC module; it
exports to PDF, SVG and DXF, and its `devices`, wires and I/O reports list
the new data; a v0.0.1 and a v0.0.2-dev project open and simulate to their
traces; every check of `docs/releasing.md` passes. The tests are named in this
section when each item lands.

Order of work: M9.0, then M9.1, M9.2, M9.3 (ADR-0018 before any field) and
M9.4, in stacked pull requests, as in M7 and M8 (#119 to #130).

Not part of M9 and left for later: drawing every shape kind with the pointer
(the command line of the window and the edit files add them; the pointer
picks, moves and deletes them), comments drawn on the sheet, nested groups, a
panel layout with real dimensions and rails, and floating panels.

Considered and not done in M9: free drawing, scaling, fillet, chamfer, explode
and polar tracking (they do not fit the orthogonal, quantized drawing);
voltage drop, cable sizing, short-circuit current and load calculations
(outside the discrete model, and a risk of being read as certification);
cabinet layout in 3D, collaboration, user permissions and cloud
synchronization (local-first); DWG export (proprietary format); copied IEC or
ANSI symbol files.

## Out of scope for the MVP

Analog simulation, rigorous voltage/current/impedance calculation, transients,
pneumatics, hydraulics, real PLC execution, industrial protocols, plugins,
real-time collaboration, proprietary CAD import and a certified compliance
claim. M4 added data-only libraries and a Modbus TCP server for testing;
code plugins, a Modbus client and control of real equipment remain out of
scope.
