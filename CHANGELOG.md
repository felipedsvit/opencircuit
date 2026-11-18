# Changelog

What changed in each release of OpenCircuit. A release is a `vX.Y.Z` tag; the
release workflow takes the section of its version from this file as "What is
new" in the notes of the draft release, and stops if the section is missing
(`docs/releasing.md`). The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions before 1.0
may change anything, and what changes in a file format is listed under
"Changed".

## [Unreleased]

Version 0.0.2 closes M9 (`ROADMAP.md`): resilience, work safety,
productivity, schema version 5 and new document views. Each item is listed
here when it lands.

### Added

- **Autosave and recovery** in the window: while a project has unsaved
  changes, a copy is kept every 30 seconds in the user's data folder
  (`~/.local/share/opencircuit/recovery`, `~/Library/Application
  Support/OpenCircuit/recovery` or `%LOCALAPPDATA%\OpenCircuit\recovery`),
  never next to the project, and the status bar says so. The copy is removed
  when the project is saved, replaced or closed on purpose. After a crash, a
  killed process or a power loss, the next window offers to recover it (as
  unsaved changes, with its original file) or to discard it. A window that is
  still running is never offered: it holds an operating-system lock that the
  system releases however the process ends, so a copy is offered as soon as
  the window is gone. A panic in the window writes the newest changes before
  the process ends.
- **Version history** (File > Version history): every save that changes the
  project keeps a version in the user's data folder (the newest 20 per
  project). The window lists them with their date (UTC) and how many elements
  differ from the project, and opens one as a new, unsaved project; the file
  is never touched.
- **`opencircuit diff <before> <after>`**: what changed between two versions of
  a project, one row per element added, removed or changed with the fields
  that differ, as text, JSON or CSV. It exits with 1 when they differ. A project
  saved by an older build compares as the same document after its migration.
- **File menu:** New from template (four starters built from the examples),
  Open recent (the last ten projects opened or saved, kept in the user's data
  folder; a file that is gone is taken off the list) and Save a copy (writes
  the project to another file and keeps working on this one, unsaved changes
  included).
- **Command palette** (Ctrl+K, or Help > Command palette, in both modes): a
  field that runs a command of the window by name (new, open, save, save a
  copy, undo, redo, fit, edit, simulate, export, load library, keys) or finds
  an element of the project (devices by designator, model, catalog data or
  parameter; wires by number; sheets; text notes) and selects it. Arrows move,
  Enter chooses, Esc closes.
- **Find and replace** (Modify > Find and replace, Ctrl+H, in edit mode;
  `{ "op": "replace" }` in edit files): replaces a text in the designators,
  text notes and text parameters in one undoable step, shows how many changes
  it makes, and refuses a result with an invalid or repeated designator.
- **Catalog data of several devices**: with two or more devices selected, of
  any models, the inspector has a closed section "Catalog data of N devices"
  whose fields set the manufacturer, part number, description or location of
  all of them.
- **A label typed over the drawing** (F2, Modify > Edit label, or the context
  menu): the designator of the selected symbol's device, the words of a text
  note or the number of a wire. Enter commits, Esc drops it.
- **Navigation:** View > Previous view (Alt+Left) goes back through the last
  32 views left by a zoom, a pan or a jump; G opens a field at the pointer for
  the point of the next click (`x,y`, or `@dx,dy` from the pointer); and three
  View options, off unless turned on: snap the wire tool to the ends, bends,
  middles and crossings of wires (`circuit_editor::snap_with`), outline the
  element under the pointer, and rulers along the drawing.
- **Report `devices`**: one row per device (model, symbols, sheets and catalog
  data), in `opencircuit report --kind devices` and in `--kind all`, where it
  is the last table.
- **Sheet renumbering**: the sheets are numbered 1, 2, 3… in reading order of
  their frames, in one undoable step: `{ "op": "renumber_sheets" }` in an edit
  file, `sheets renumber` on the command line of the window.
- **A project open in two windows** is said in the status bar of the second
  ("Open in another window"): saving in both overwrites each other's changes.
  The lock lives in the user's data folder, never next to the project, and is
  released when the first window lets the project go or ends.

### Fixed

- **A command no longer writes over a file it reads**: `simulate --trace` or
  `--snapshot-json` naming the project, the scenario, a library or the other
  output, `edit --output` naming the edit file or a library, and
  `test --update-golden` with an `expect.trace` that names the project or the
  scenario replaced that file and lost it. They now stop with a usage error
  (exit code 2) and write nothing. Links and other names of the same file are
  recognized. `edit --output` may still name the project.
- **Saving keeps the permissions, the group and the links of a file** (window
  and command line): a project readable only by its owner (`0600`) became
  readable by everyone (`0664`) after a save, and saving through a symbolic
  link replaced the link with a copy and left the file it pointed to as it
  was. A replaced file now keeps its mode and group (or loses group access
  when the group cannot be kept), and a link is followed to its file.
- **A project with a name near the 255-byte limit** opened but could not be
  saved: the temporary file of a save has a short name of its own.
- **A file that starts with a byte order mark** (written by some Windows
  editors) is read: projects, libraries, scenarios and edit files.
- **`opencircuit serve` refused a client** that arrived right after other
  clients had closed their connections: closed connections still held the four
  slots until the next turn. They now free their slot first.
- **`opencircuit` with an argument that is not UTF-8** (a file name made of
  other bytes) stops with a usage error (exit code 2) instead of crashing.
- **`opencircuit-desktop --help`** no longer crashes when its output cannot be
  written (a closed pipe, a full disk).
- **Saving on Linux and macOS** also flushes the directory after the new file
  is renamed over the old one, so that the rename itself reaches the disk and
  not only the contents (where the file system allows it). The temporary file
  is created only if nothing has its name, so a file or a link that is already
  there is never written through nor removed (window, command line and MCP
  `edit`).

## [0.0.2-dev] - 2026-11-17

A development build: M7 (electrical documentation) and M8 (panel documentation
and an agent interface) on top of 0.0.1.

### Added

- **Windows installer (`.msi`)**: installs the window and the command line for
  all users, adds `opencircuit` to the `PATH`, a Start menu entry and the
  association with `.ocircuit` files, and upgrades or removes cleanly. The `.zip`
  stays for a portable copy.
- **Window in Brazilian Portuguese**, chosen from the environment
  (`LC_ALL`, `LC_MESSAGES`, `LANG`) or from Help > Language while it runs. English
  is the default; diagnostics, the command line and the file formats stay English.
- **Terminal strips and the terminal plan**: the part `std.terminal_block` (strip
  and number), the report `strips`, and the warning `OC-SEM-013` for a number used
  twice on a strip.
- **Signal arrows between sheets**: arrows with one name are one net, on a sheet
  or on another, with no wire between them; each arrow shows where the others are
  (`-> 2/4B`); `OC-SEM-014` for an arrow with no partner. New example
  `examples/signal-arrows`.
- **Text notes** (one line, up to 120 characters) with the Text tool, the `text`
  command and an inspector section; they are drawn and exported and have no effect
  on the circuit or the reports.
- **Layers** to hide (not drawn, exported or clickable) and lock (edits refused)
  groups of symbols, wires and notes, with no effect on the simulation; the
  `layer` command and a Layers section in the inspector.
- **MCP server** (`opencircuit mcp`): JSON-RPC 2.0 over stdio with the tools
  `new`, `validate`, `simulate`, `edit`, `report` and `export`, confined to one
  root directory.
- **Wire numbers** from a rule of the project (prefix, start, step, restart on
  each sheet) with an override on a wire, drawn on the wire; `OC-SEM-012` for
  conflicting overrides.
- **Reports**: bill of materials, wires, terminals, cross-reference exceptions,
  I/O map and problems, as text, JSON, CSV and PDF (`opencircuit report`, and
  `--kind all`).
- **Catalog data of a device** (manufacturer, part number, description,
  location), **`renumber`** of designators in reading order, **waivers** (a warning
  accepted with a reason, which `--deny-warnings` then ignores), and warnings for
  drawings that look connected and are not (`OC-STR-018`, `OC-STR-019`).
- **In the window**: a project tree, the connections and cross-references of a
  part, a net trace (View menu, key `T`), the parts used most recently, a
  two-terminal part placed on a wire goes in series with it, and Connect, Ignore,
  Restore and Save list on the problems list.
- **Edit files** (`opencircuit edit --apply`) gain `set_device_info`,
  `set_wire_number`, `numbering`, `waive`, `unwaive`, `renumber`, `add_text`,
  `set_text`, `add_layer`, `set_layer`, `remove_layer` and `assign_layer`.

### Changed

- **Project files are now schema version 4** (was 2). A project of an older
  version opens and is migrated when it is saved; a project saved by this version
  cannot be opened by 0.0.1. Library files may use schema version 3 (the `name`
  parameter type and the classes `terminal_block` and `signal_arrow`); schema 1
  and 2 libraries keep loading.
- The standard library `std` has two more parts: the terminal block and the
  signal arrow, both in the Lines tab.

### Fixed

- The core crates build again on Rust 1.85, their minimum version (no `let`
  chains).

## [0.0.1] - 2026-11-17

First public build: an editor and simulator for industrial control circuits
(contactors, relays, timers, motors), with the window (`opencircuit-desktop`)
and the command line (`opencircuit`).

### Added

- A discrete simulator (continuity and energized state, fixed ticks, short
  circuits that stop the run) for direct-online, reversing and star-delta
  starters, interlocks, changeover contacts, timers, the six-terminal motor,
  supply sections and the control transformer, with golden traces.
- A project format in canonical JSON (`.ocircuit`), data-only extension libraries
  (`.oclib`), the standard parts library `std` and symbol-only devices.
- A window with menus, a status bar, an eight-tab palette, a command line, an
  inspector, selection by window or crossing, copy and paste, aligning, undo and
  redo, drawing sheets with a frame, zones and a title block, cross-references
  between coils and contacts, and a simulation mode with an operator panel.
- Export to SVG and PDF (one page per sheet, embedded font).
- The command line: `validate`, `simulate`, `test`, `export`, `new`, `edit` and
  `serve` (Modbus TCP for testing PLC programs and HMIs), and a browser build of
  the window.
- Packages for Windows (`.zip`), macOS (`.dmg`), Debian and Ubuntu (`.deb`),
  Fedora (`.rpm`) and any Linux (AppImage and `.tar.gz`).

[0.0.2-dev]: https://github.com/felipedsvit/opencircuit/compare/v0.0.1...v0.0.2-dev
[0.0.1]: https://github.com/felipedsvit/opencircuit/releases/tag/v0.0.1
