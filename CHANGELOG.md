# Changelog

What changed in each release of OpenCircuit. A release is a `vX.Y.Z` tag; the
release workflow takes the section of its version from this file as "What is
new" in the notes of the draft release, and stops if the section is missing
(`docs/releasing.md`). The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions before 1.0
may change anything, and what changes in a file format is listed under
"Changed".

## [Unreleased]

## [0.0.3] - 2026-11-20

Version 0.0.3 is the first release that can tell its users about a newer one
(ADR-0020). Installs of 0.0.2 do not have that notice: update them once by hand,
from the release page.

### Added

- **Update notice.** The window looks for a newer published release at
  startup, every 5 days by default. **Help > Updates...** sets it to every 3
  days, every 5 days, or never, and checks now. A chip in the status bar and a
  dialog show the version found. The dialog can download the installer for
  this system into the user's data folder and check it against the published
  SHA-256; the file is then shown, and installing it is left to you. The check
  runs in the background and is silent when offline. The request carries only
  the program's version and the name of the operating system.
- **Signed checksums.** The release workflow signs `SHA256SUMS` with the
  project's minisign key and publishes `SHA256SUMS.minisig` with each release.
  The update notice refuses a download whose list does not verify, or whose
  signature names another version.
- **`opencircuit --version`** (and `-V`) prints `OpenCircuit <version>`, the
  same text as Help > About in the window.
- **`opencircuit --upgrade`** does what Help > Updates does by default: it
  reads the latest published release and, when it is newer than the program,
  downloads the installer for this system, checks it against the signed
  SHA-256 list and prints where it is. Nothing is installed. When the program
  is up to date it says so and shows the latest published version.

### Changed

- The Help menu has a new entry, **Updates...**. The status bar shows the
  update chip only when a newer version is known.
- "Install automatically" is shown but not available yet (phase 3 of ADR-0020).
- The update code (release lookup, download, signature check) is the new crate
  `opencircuit-update`, used by both the window and the command line.

### Fixed

- The update check no longer stays on "Checking..." forever: the HTTPS
  request panicked for lack of a TLS provider, which ended the thread that was
  to answer. The provider is set, and a thread that ends without an answer now
  gives a connection failure.

## [0.0.2] - 2026-11-18

Version 0.0.2 closes M9 (`ROADMAP.md`): resilience, work safety,
productivity, schema version 5, new document views, and an audit of the
window (M9.5). It includes everything of 0.0.2-dev below. Two items of M9.0
are left for their own pull requests: the CI changes (clippy on Windows,
pinned actions, a release-mode test job, a coverage threshold) and the macOS
file dialogs.

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
- **Symbol sets** (File > Symbol sets; `libraries/iec-style.oclib` and
  `libraries/ansi-style.oclib`): a relay, pushbuttons, a pilot lamp and a fuse
  drawn in an IEC-like and in an ANSI-like style, as extension libraries
  drawn for OpenCircuit, with no claim of conformance with a standard.
- **Symbol editor** (File > Symbol editor…): draw the symbol of a part as
  lines of primitives, see it as the reader of libraries makes it, load it
  into the palette or save it as an `.oclib` extension library.
- **DXF export** (`opencircuit export --output drawing.dxf`, `--to dxf`,
  Export > DXF in the window): an AutoCAD R12 ASCII drawing of the canvas or
  of one sheet, in millimetres, one entity per item on layers by what it is
  (wires, symbols, labels, frame, state, marks), deterministic like the SVG
  and PDF exports.
- **Autosave in the browser build**: a project with unsaved changes is kept
  in the browser storage of the page, and the next page opened offers to
  recover it or discard it; the status bar says when the copy is current.
- **Backups next to the project** (File > Backups next to the project, off by
  default): each save keeps the file it replaces as `name.ocircuit.bak1`,
  moving older ones to `bak2`, `bak3`… and keeping the last 1, 3 or 5. A
  backup that fails does not stop the save. The setting is kept with the
  workspace.
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
- **Print preview** (File > Print preview): the pages of the PDF, at rest and
  on white, one at a time, with Export as PDF.
- **Export of several projects**: `opencircuit export a.ocircuit b.ocircuit
  --output-dir out --to pdf` writes `out/a.pdf` and `out/b.pdf`; one that fails
  is reported and the others are written (exit code 1).
- **Wire tools** (context menu of the drawing in edit mode, and the command
  line of the window): *trim* cuts a wire at a point and removes the part
  toward its nearer end (`trim x,y`); *extend* runs a free wire end along its
  last segment to the next terminal, junction or wire and connects it
  (`extend x,y`); *parallel copy* copies the selected wires alongside,
  connected to nothing (`offset dx,dy`). Each is one undoable step.
- **Macros** (Modify > Record macro, Stop recording and save the macro, Play
  macro): the edits of the window are recorded as an edit file that the
  window, `opencircuit edit --apply` and the MCP server play back; played in
  the window it is one undoable step. A change that cannot be written as
  operations is left out and counted in the status bar. The reader of edit
  files moved from the command line to `circuit_commands::script`, so that
  the window shares it.
- **Wire routing on request** (Modify > Route the selected wires, `route` on
  the command line of the window, `{ "op": "route" }` in edit files): each
  selected wire gets an orthogonal route with few bends around symbols,
  junctions and terminals, without running along another wire; one undo
  takes it back.
- **Workspace** (end of the View menu): a theme (follow the system, dark or
  light), the side panels collapsed (Ctrl+B) or swapped (palette on the
  right, inspector on the left), the tool bar hidden, named workspaces that
  bring a layout back, and Customize shortcuts, which gives fourteen
  shortcuts other keys (undo, copy, paste, the arrows, Delete and Escape keep
  theirs). All of it is kept in `workspace.json` in the user's data folder; a
  fresh window is the window as before.
- **Several projects in tabs** (File > New tab, Open in a new tab, Close
  tab): each tab keeps its project, mode, view, selection, undo history,
  autosave copy and lock. The tab bar shows one tab per project and a button
  that closes it; with one project open, the window is as before. A project
  open already is shown instead of opened twice. Closing the window asks
  about each tab with unsaved changes in turn; at most 16 projects are open
  at once.
- **The client** of a project (Project tree > Project data in the inspector,
  `"client"` in the `project` operation of edit files), printed in a row
  above the title block of every sheet; a project without a client draws the
  title block as before.
- **Wire data** (ADR-0018): the colour, cross-section, type, material and
  insulation of a wire, in the closed section Wire data of the inspector and
  in `{ "op": "set_wire_spec" }` of edit files. The `wires` report gets five
  columns for them in a project that has any; the report of any other
  project is as before.
- **Annotations** (ADR-0018): rectangles, circles, arcs, lines, revision
  clouds with a revision mark, and QR codes, drawn in SVG and PDF and picked
  on their lines, moved, deleted and put on layers like notes. They are added
  on the command line of the window (`shape rect|circle|arc|line|cloud`, `qr`)
  and with `{ "op": "add_shape" }` in edit files.
- **Comments on an element** (Project tree > Comments in the inspector,
  `comment` on the command line, `add_comment` in edit files). A comment is
  not drawn; deleting the element deletes its comments.
- **Groups** (Modify > Group selection, Ungroup selection; `group` and
  `ungroup`): a click on a member selects the whole group.
- **Mirrored symbols** (M, Modify > Mirror selection, `mirror`): a symbol is
  flipped about its vertical axis; its wires follow and its connections do
  not change.
- **Datasheets**: a link per device in its catalog data (`datasheet`), opened
  with the system from the inspector.
- **Sheet kinds, device views and PLC modules** (ADR-0018): a sheet can be a
  single-line diagram or a panel layout (`sheets kind`, `set_sheet_kind`); a
  device of the schematic is shown there again as a view (`view`,
  `add_view`) with its designator and where it is on the schematic, as a
  footprint or a single-line glyph. PLC modules list their channels and the
  devices they read or drive (`add_plc`, `set_plc`). `OC-STR-024` warns about
  a view off those sheets or a symbol on them, and `OC-STR-025` about two
  channels on one address or one device.
- **Reports `comments` and `plc`** (`opencircuit report --kind comments|plc`):
  the comments of a project, and one row per PLC channel with its device and
  where that device is in the Modbus map. The `devices` report gets the
  columns Datasheet and Views. A new example,
  `examples/panel-documentation`, has a single-line diagram, a panel layout,
  a PLC module, wire data, a revision cloud, a comment and a QR code, with its
  reports.
- **Import of catalog data from CSV** (Modify > Import catalog data;
  `{ "op": "import_catalog" }` in edit files): manufacturer, part number,
  description, location and datasheet of each device, by designator, in one
  undoable step.
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

### Changed

- **The library in the left column is responsive** (ADR-0019): a part's
  symbols are drawn in rows of four, never larger than before, and shrink in
  proportion when the column is narrower (down to about half the size), as do
  the eight category icons, so nothing breaks into another line and nothing goes
  past the edge of the column. The column goes from about 174 points to a few
  points past four full cells. The default width is as it was, so a new window
  shows four smaller cells in a row where it showed three.
- **Dialogs and errors have a modern look** (ADR-0019): the question about
  unsaved changes and the offers to recover work have an icon, a title, a
  message and buttons by importance (the main one filled, the one that loses
  work outlined in red), a corner radius and a button order of the system the
  window runs on, the main button with the keyboard, and Escape to dismiss.
  An error is a banner with a button that clears it. File pickers stay native.
  Every window of the application gets the corner radius and the shadow of the
  system too.
- **The window reads its keymap in place** instead of copying it three times a
  frame, and `tests/compat/v0.0.2` freezes the examples, library, scenarios,
  golden traces and edit files of this release (schema version 5), so that a
  later build must open and simulate them as this one does.
- **Project files are schema version 5** (ADR-0018). A version 4 file opens and
  is saved as version 5; the migration only raises the version, because every
  new field is optional and left out when empty. A version 5 file does not
  open in an older build. The new fields are documentation only: the compiled
  circuit, the simulation and every golden trace are the same.

### Fixed

- **A disabled icon shows its tooltip**: the undo and redo icons of a window
  with nothing to undo said nothing when the pointer rested on them, because
  egui shows the hover text of an enabled widget only. Every icon button now
  names itself, enabled or not.
- **Open sections and windows keep their state when the language changes**: the
  sections of the inspector, the Find and replace, Command palette, Print
  preview, Version history, Symbol editor, shortcut and About windows took
  their identity from their translated title, so choosing another language
  closed the sections and moved the windows back to their first place.
- **Contrast**: the secondary text over the drawing (zoom, rulers) in the dark
  theme and the "no sim" warning in the light theme now reach 4.5:1 (WCAG AA);
  two colours moved a few steps and nothing else in the palette changed.
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

[Unreleased]: https://github.com/felipedsvit/opencircuit/compare/v0.0.3...HEAD
[0.0.3]: https://github.com/felipedsvit/opencircuit/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/felipedsvit/opencircuit/compare/v0.0.2-dev...v0.0.2
[0.0.2-dev]: https://github.com/felipedsvit/opencircuit/compare/v0.0.1...v0.0.2-dev
[0.0.1]: https://github.com/felipedsvit/opencircuit/releases/tag/v0.0.1
