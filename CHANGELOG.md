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
  still running is never offered.

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
