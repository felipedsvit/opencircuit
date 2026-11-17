# Changelog

What changed in each release of OpenCircuit. A release is a `vX.Y.Z` tag; the
release workflow takes the section of its version from this file as "What is
new" in the notes of the draft release, and stops if the section is missing
(`docs/releasing.md`). The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions before 1.0
may change anything, and what changes in a file format is listed under
"Changed".

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
