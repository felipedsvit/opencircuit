# Extending OpenCircuit

OpenCircuit is extended through data and documented interfaces, never by
running third-party code. This document is the map of those interfaces: what
each one is for, where its normative contract lives and which limits it
enforces. Read [Safety limits](#safety-limits) before connecting anything to
the simulator.

| Interface | Use it to | Contract |
| --- | --- | --- |
| [Extension libraries](#extension-libraries) | add component models, parameters and symbol artwork | [`LIBRARY.md`](LIBRARY.md), [ADR-0008](docs/adr/0008-extension-libraries.md) |
| [Scenarios and traces](#scenarios-and-traces) | drive a simulation from a file and compare it with a golden trace | [`SIMULATION.md`](SIMULATION.md) |
| [Editing from a script](#editing-from-a-script) | create and change a project from another program, with no GUI | this file |
| [JSON output](#json-output) | read the result of any command from another program | this file |
| [Project files](#project-files) | generate or transform schematics with another tool | [`FORMAT.md`](FORMAT.md) |
| [Modbus TCP server](#modbus-tcp-server) | let a PLC program or an HMI under test press buttons and read outputs | [ADR-0009](docs/adr/0009-industrial-io.md) |
| [Export](#export) | publish drawings as SVG or PDF | [ADR-0006](docs/adr/0006-pdf-export.md) |
| [Rust crates](#rust-crates) | embed the compiler and the simulator in another Rust program | the crate documentation |

## Extension libraries

A library (`*.oclib`) is a JSON file of component models that reuse the
behaviour classes of OpenCircuit (contactor, relay, timer, pushbutton, …)
with their own identifiers, parameters, provenance and, optionally, symbol
artwork. Behaviour cannot be added by a library: a model picks one of the
existing classes and symbol kinds, so the simulator stays the one described
in [`SIMULATION.md`](SIMULATION.md). [`LIBRARY.md`](LIBRARY.md) is the
normative format; [`examples/library`](examples/library) holds a library, a
project that uses it, a scenario with its golden trace and its drawings.

The standard parts of OpenCircuit are a library of their own, `std`, which
is built in and needs no file: see "The standard library" in
[`LIBRARY.md`](LIBRARY.md). A library may use the class `symbolic` for a
part that the simulator does not model.

Loading a library:

- **Command line:** `--library <file or directory>` on `validate`,
  `simulate`, `export` and `serve`; a directory loads every `*.oclib` in it,
  in name order. A scenario lists its libraries in `libraries` (schema
  version 2), so `opencircuit test` loads them too.

  ```sh
  cargo run -p opencircuit-cli -- validate examples/library/acme-lamp.ocircuit \
      --library examples/library/acme.oclib
  cargo run -p opencircuit-cli -- test examples/library
  ```

- **Desktop:** **Library…** opens libraries, or drop the files on the
  window; in a browser, drop them on the page. Their models join the
  palette; hovering one shows its provenance.

A project stores the identifier and version of each model, never the
library itself. Opened without the library, a project that uses one of its
models reports `OC-STR-003` (`UnknownModel`) instead of guessing.

## Scenarios and traces

A scenario (`*.scenario.json`) names a project, the operator events and the
tick to stop at, and may expect a golden trace and a list of diagnostic
codes. `opencircuit simulate` prints the change-log trace; `opencircuit test
<directory>` runs every scenario below a directory and compares the traces
byte for byte. Both formats are specified in [`SIMULATION.md`](SIMULATION.md);
exit codes are `0` success, `1` validation or golden failure, `2` usage or
I/O error. This is the interface for regression tests of your own circuits
in continuous integration.

## Editing from a script

`opencircuit new <file>` writes an empty project, and `opencircuit edit
<project> --apply <edit.json>` changes one. The edit file is **data**: a list
of operations, applied one after the other through the same operations the
editor uses, so every rule of the editor holds (a designator is unique, a
sheet does not overlap another, a wire ends on something that exists, the
size limits of the format). Nothing in the file is executed, and there is no
loop or condition: a program that needs them writes the file.

All of it or nothing: the project is replaced (atomically, or `--output
<other>` is written) only when every operation succeeded; `--dry-run` checks
and writes nothing. A file that is not an edit file (not JSON, an unknown
field or operation, a list over its limit, a name that is not valid) is a
usage error, exit code 2. An operation the project refuses is a failure,
exit code 1, with its position and the reason: `operation 7 (connect): ...`.
The compiler is not asked to accept the result: `errors` and `warnings` in
the output say what it finds, and `validate` is the check to run after.

The window writes edit files too: Modify > Record macro records the edits
that follow, and Stop recording saves them as an edit file that Modify >
Play macro, `opencircuit edit --apply` and the MCP server play back. Each
change is written as operations and kept only when running them on the
project as it was gives exactly the change; one that cannot be (a wire route
dragged by hand, a layer, a supply) is left out, and the status bar counts
it. Elements the macro creates are named `s12`, `j13`, `w14`, `t15` after
their IDs at recording time; the others are named by their IDs and devices
by their designators, so a macro that refers to elements it did not create
plays back on the project it was recorded on. The reader of edit files is
`circuit_commands::script`.

```json
{
  "schema_version": 1,
  "operations": [
    { "op": "project", "name": "Pump", "author": "Ana", "revision": "A" },
    { "op": "add_supply", "preset": "dc_24v", "as": "control" },
    { "op": "place", "model": "supply.terminal", "symbol": "terminal",
      "at": [0, 0], "reference": "P24", "as": "p24" },
    { "op": "set_param", "device": "P24", "key": "supply", "value": { "supply": "control" } },
    { "op": "place", "model": "contactor.generic", "symbol": "coil",
      "at": [0, 20], "reference": "K1", "as": "coil" },
    { "op": "connect", "from": { "symbol": "p24", "terminal": "out" },
      "to": { "symbol": "coil", "terminal": "a1" } },
    { "op": "add_sheet", "title": "Control", "origin": [-14, -8] }
  ]
}
```

`examples/edit/starter-sheets.edit.json` builds the two-sheet starter of
`examples/sheets/` this way; the PDF it gives is byte for byte the committed
one.
`examples/edit/documentation.edit.json` is that file with the numbering rule
and the catalog data of the parts added; it makes the project of
`examples/documentation/`, whose reports (`bom.csv`, `wires.csv`,
`terminals.csv`, `xref.csv`, `io.csv`) are what `opencircuit report` gives
for it.

**Names.** An operation that creates an element may give it a name with
`"as"` (1 to 64 letters, digits, `_`, `-` or `.`; each name once), which the
operations after it use, since the IDs exist only once it runs. A **device**
is always named by its reference designator, such as `"K1"`. A **symbol** is a
name, an ID, or `{ "device": "K1", "symbol": "aux_no", "slot": 1 }` (the slot
only when the device has several of that symbol). The output lists the named
elements with their kind and ID.

**Operations.**

| `op` | Fields | Does |
| --- | --- | --- |
| `project` | `name`, `author`, `company`, `revision`, `date`, `client` | sets what is present; `null` clears an optional field; absent keeps it |
| `add_supply` | `preset` (`dc_24v`, `single_phase_230v`, `three_phase_400v`), `as` | adds a supply profile |
| `place` | `model`, `version`, `symbol`, `at`, `rotation`, `reference`, `as` | a new device drawn as one symbol; the newest `version` unless given; `rotation` 0, 90, 180 or 270 |
| `add_symbol` | `device`, `symbol`, `at`, `rotation`, `as` | another symbol of a device (an auxiliary contact) |
| `set_param` | `device`, `key`, `value` | a boolean, a whole number, a text, `{ "supply": name or ID }`, `{ "device": designator }`, or `null` to clear |
| `set_reference` | `device`, `to` | renames a device |
| `junction` | `at`, `as` | a junction with no wire |
| `connect` | `from`, `to`, `via`, `as` | a wire; an end is `{ "symbol": ..., "terminal": "a1" }`, `{ "junction": name or ID }` or `{ "wire": name or ID, "at": [x, y] }` (a branch); `via` are waypoints; without them the route has at most one bend |
| `move` | `elements`, `by` | moves symbols, junctions or `{ "device": "K1" }` by `[dx, dy]`; wires follow |
| `rotate` | `symbols` | a quarter turn clockwise |
| `delete` | `elements` | deletes elements and the wires that hang on them; a note is `{ "text": name or ID }` here and in `move`; a shape is `{ "shape": ... }` and a view `{ "view": ... }` |
| `add_sheet` | `number`, `title`, `origin`, `size`, `orientation`, `zones` | a sheet; the next number unless given; `size` is `a4` (default), `a3`, `a2` or `{ "custom": { "width": w, "height": h } }`; `zones` is `[columns, rows]`, `[8, 6]` by default |
| `remove_sheet` | `number` | removes a sheet; what was drawn on it stays |
| `set_device_info` | `device`, `manufacturer`, `part_no`, `description`, `location`, `datasheet` | the catalog data of a device (ADR-0014); a field left out is kept and `null` clears it; `datasheet` is an `http`, `https` or `file` link or a relative path (ADR-0018) |
| `set_wire_number` | `wire` (name or ID), `number` | the number that replaces the generated one on a wire; `null` or no `number` clears it |
| `set_wire_spec` | `wire` (name or ID), `colour`, `cross_section`, `kind`, `material`, `insulation` | what a wire is made of (ADR-0018), each field at most 32 characters; a field left out is kept and `null` clears it |
| `numbering` | `prefix`, `start`, `step`, `per_sheet`, `enabled` | the rule that numbers the wires: what is left out keeps its value (an empty prefix, 1, 1 and one counter for a new rule); `"enabled": false` removes it |
| `waive` | `code`, `subject`, `reason` | accepts a warning or informational diagnostic with a reason; `subject` is a name, `{ "device": "K1" }`, `{ "wire": ... }`, `{ "junction": ... }`, `{ "symbol": ..., "terminal": "p2" }` for a terminal, or `"document"`; an error cannot be waived |
| `unwaive` | `code`, `subject` | takes a waiver away |
| `renumber` | `sheet` | gives the designators the next numbers in reading order, on every sheet or on the sheet with this number |
| `renumber_sheets` | | numbers the sheets 1, 2, 3… in reading order of their frames (top to bottom, then left to right); the cross-references follow |
| `route` | `wires` (names or IDs) | gives each wire an orthogonal route around symbols, junctions and other wires, with few bends; refused when none is found within 20 grid units of its ends |
| `replace` | `find`, `with`, `designators`, `notes`, `parameters` | replaces `find` (exactly, with its case) by `with` in the designators, the text notes and the text parameters; each kind is on unless set to `false`. Refused as a whole when a designator would be invalid or taken twice, or a value would not fit |
| `add_text` | `at`, `text`, `anchor`, `as` | a text note (ADR-0015): one line of 1 to 120 characters; `anchor` is `left` (default) or `center` |
| `set_text` | `text` (name or ID), `to` | changes the words of a note |
| `add_layer` | `name`, `visible`, `locked`, `as` | a layer (ADR-0015), visible and unlocked unless said otherwise; a name of 1 to 32 characters, with no space at either end, not used by another layer |
| `set_layer` | `layer` (name or ID), `name`, `visible`, `locked` | renames, shows, hides, locks or unlocks a layer; what is left out stays |
| `remove_layer` | `layer` | removes a layer that nothing is on |
| `assign_layer` | `layer` (name or ID, or `null` for the default layer), `elements` | puts symbols, wires, notes and the symbols of `{ "device": ... }` on a layer; junctions have no layer |
| `add_shape` | `shape`, `layer`, `as` | a shape of ADR-0018 written as the project file writes it: `{ "rectangle": { "from": { "x", "y" }, "to": ... } }`, `circle`, `arc`, `polyline`, `cloud` or `qr` |
| `add_comment` | `subject`, `text`, `author`, `as` | a comment about an element; `subject` as `waive` takes it |
| `group` | `name`, `elements`, `as` | a group: a click on a member selects them all; an element is in one group at most |
| `ungroup` | `group` (name or ID) | takes a group apart; its elements stay |
| `mirror` | `symbols` | mirrors symbols about their vertical axis, or undoes it |
| `set_sheet_kind` | `number`, `kind` | `schematic`, `single_line` or `panel_layout` |
| `add_view` | `device`, `at`, `size`, `as` | shows a device again on a panel layout or single-line sheet; `size` is `[width, height]`, `[6, 8]` by default |
| `add_plc` | `name`, `description`, `at`, `channels` | a PLC module; a channel is `{ "address", "direction": "input" or "output", "device", "label" }`; `at` places the box that lists them |
| `set_plc` | `plc` (its name), `description`, `at`, `channels` | changes a PLC module: what is given replaces what it has; `"at": null` takes the box off the drawing |
| `import_catalog` | `csv` | the catalog data of devices from CSV text: a header with `reference` and any of `manufacturer`, `part_no`, `description`, `location` and `datasheet` (RFC 4180 quoting), one row per device; an empty cell keeps the field, and a row for a designator the project does not have refuses the file |

A name given with `as` lasts for the one file; a later file refers to a note or
a wire by its ID (the layers also by their own name). An operation that changes
an element on a locked layer is refused with `locked`, and nothing of the file
is applied.

Limits: a file of at most 1 MiB, 10 000 operations, 10 000 entries in a list
and 1 024 waypoints in a wire. Positions are grid units.

## JSON output

`validate`, `simulate`, `test`, `export`, `new`, `edit`, `report` and `serve --print-map` take
`--format json` and then write **one JSON document on stdout**, in place of
their text. Every document starts with

| Field | Meaning |
| --- | --- |
| `format` | version of the document layout, `1` today |
| `command` | the command that wrote it |
| `ok` | whether it succeeded: the exit code is `0` when true and `1` when false |

A field may be added without a new `format`; one that changes meaning or goes
away takes a new version. A failure with exit code 1 (a project that does not
load or compile, a golden trace that differs, a sheet that does not exist) is
a document with `ok: false`, an `error` text and `diagnostics`, so a program
reads one stream. A usage or I/O error (exit code 2) still writes on stderr.

| Command | Fields after the common ones |
| --- | --- |
| `validate` | `error` (why the file did not load, or `null`), `errors`, `warnings` (those the project does not waive), `waived` (how many it does), `diagnostics`: each with `waived`, `null` or `{ "reason": ... }` |
| `simulate` | `tick` (the last solved tick), `faulted`, `diagnostics`, `trace` (the text of the trace, `null` when `--trace` wrote it to a file), `snapshot` (the last solved tick, as `--snapshot-json` writes it) |
| `test` | `total`, `failed`, `scenarios`: a list of `path`, `status` (`pass`, `updated` or `fail`) and `reasons` |
| `export` | `file`, `kind` (`svg` or `pdf`), `pages` |
| `new` | `file` |
| `edit` | `operations`, `created` (each named element with `kind` and `id`), `wrote` (the file, or `null` for a dry run), `errors`, `warnings`, `diagnostics` |
| `diff` | `same` (whether the projects are the same document) and `changes`: `kind` (`diff`), `title`, `columns` and `rows` |
| `report` | `reports`: a list of `kind`, `title`, `columns` and `rows` (every cell is text); with `--output`, `file`, `kind` (`csv`, `json`, `txt` or `pdf`) and `reports` (how many tables) |
| `serve --print-map` | `map`: a list of `table` (`coil`, `discrete_input` or `input_register`), `address`, `words`, `name`, `meaning`; the same lines as the text map |

`diagnostics` are the structured diagnostics of `FORMAT.md` (`code`,
severity, `subject`, `related`, `message`). A project can waive a warning or an
informational diagnostic with a reason (`waivers` in `FORMAT.md`, ADR-0014):
`validate` still lists it, marks it, leaves it out of `warnings` and does not
fail `--deny-warnings` for it. An error is never waived.

## Project files

`.ocircuit` files are canonical JSON: sorted, stable identifiers, structural
terminal keys and integer grid geometry, read strictly. A tool that writes
them must follow [`FORMAT.md`](FORMAT.md) and should run `opencircuit
validate` on what it produced; the loader refuses unknown fields, duplicate
identifiers and values beyond its limits instead of repairing them.

## Modbus TCP server

`opencircuit serve <project>` simulates the project in real time and answers
Modbus TCP requests, so a PLC program or an HMI under test can operate the
circuit as an operator would. OpenCircuit is only a server: it never
connects to another device.

```sh
cargo run -p opencircuit-cli -- serve examples/direct-starter/command.ocircuit --print-map
cargo run -p opencircuit-cli -- serve examples/direct-starter/command.ocircuit
```

`--print-map` prints the addresses and exits:

```text
coil 0 S2 (1 = pressed)
coil 1 S1 (1 = pressed)
discrete input 0 K1 (1 = active)
input registers 0-3 tick (highest word first)
input register 4 fault latch (1 = latched)
```

The map is derived from the project, in device-ID order, so it does not
change when devices are moved or renamed:

| Table | Function codes | Entries |
| --- | --- | --- |
| Coils | 1 read, 5 write one, 15 write many | pushbuttons (1 = pressed), maintained switches (1 = closed), thermal overload relays (1 = tripped) |
| Discrete inputs | 2 read | contactors and relays (picked up), transformers (powered), timers (output), lamps and buzzers (energized) |
| Input registers | 4 read | the tick as four 16-bit words, the fault latch, one register per motor (0 stopped, 1 forward, 2 reverse) |

Any other function code, and any address outside the map, gets a Modbus
exception (illegal function, illegal data address). Writes take effect at
the next tick boundary, like operator actions, and the last write to an
input before a tick wins; reads answer from the last solved tick. Options:
`--listen <address:port>` (default `127.0.0.1:5020`), `--library`, and
`--duration-ms` to stop after a while. The codec and the map are the
`circuit-io` crate, which has no socket and no clock; the command line owns
the socket and the real-time loop.

OPC UA and MQTT were evaluated and are not implemented
([ADR-0010](docs/adr/0010-opcua-mqtt-evaluation.md)).

## Export

`opencircuit export <project> --output <file.svg | file.pdf>` draws the
sheet, and the desktop application has the same in its Export menu, with File
> Print preview showing the pages of the PDF first. Output is
deterministic: the same project gives the same bytes on every platform,
which is what the committed drawings under `examples/` are checked against.
Library artwork is drawn when its library is loaded.

Several projects go into one folder with `opencircuit export <project>...
--output-dir <dir> --to svg|pdf`: each is written as `<dir>/<file name>.<svg|pdf>`
(`a.scenario.json` gives `a.scenario.pdf`), with the same `--theme` and
`--sheet` for all. A project that cannot be drawn is reported and the others
are still written, with exit code 1; two projects that would write the same
file are refused before anything is written. With `--format json` the
document has `kind` and `files`, one entry per project with `project`, `ok`,
and `file` and `pages`, or `error`. `--output` and `--output-dir` do not
mix, and `--to` goes only with `--output-dir`.

A project with sheets (ADR-0011, `FORMAT.md`) draws the frame, the zone
marks and the title block of each sheet. Its PDF has one page per sheet, in
sheet-number order, each the size of the paper (A4 landscape is 840 by 590
points); `--sheet <number>` writes one sheet as a PDF or an SVG, and
`--sheet all` is the default for a PDF. An SVG without `--sheet` is the whole
canvas. Text is set in Liberation Mono, embedded in the PDF as a subset
(ADR-0012).

## Reports

`opencircuit report <project> --kind bom|wires|terminals|strips|xref|io|problems|devices|comments|plc|all`
derives tables from the project, and writes them as text, JSON (`--format`)
or, for one table, CSV. With `--output <file>` the extension picks the format
(`.csv`, `.json`, `.txt` or `.pdf`) and `--format` says how the
confirmation is written. Nothing is stored in the project: the tables are a function of it
(ADR-0014).

| Kind | One row per |
| --- | --- |
| `bom` | kind of part: model and version, manufacturer, part number and description; supply taps are not parts |
| `wires` | net with a wire, in numbering order: number, sheet, `From`, `To` and the other terminals under `Also`; in a project where a wire has data (ADR-0018), also its colour, cross-section, type, material and insulation, each distinct value of the net once |
| `terminals` | terminal of every symbol, with the number of its wire and the terminals it meets |
| `strips` | terminal of a terminal strip (class `terminal_block`, parameters `strip` and `number`), by strip and number: the wire and the terminals on each side. It is the terminal plan, and its CSV is the table for a manufacturer's import |
| `xref` | exception: a coil with no contact, contacts whose coil is not drawn, a symbol outside every sheet |
| `io` | address of the Modbus map |
| `problems` | diagnostic of the project, compiled |
| `devices` | device, in natural order of designators: model and version, how many symbols draw it, the sheets they are on, manufacturer, part number, description, datasheet and the sheets of its views (ADR-0018); supply taps and signal arrows are not devices here. Unlike `bom`, two devices of one part are two rows |
| `comments` | comment (ADR-0018): what it is about, a name for it, the author and the text. In `all` only for a project with comments |
| `plc` | channel of a PLC module (ADR-0018): module, address, direction, device, label and where the device is in the Modbus map (`coil 3`, `discrete input 0`). In `all` only for a project with PLC modules |

A PDF sets each table on A4 landscape pages in the embedded font (ADR-0012):
the title, the headings (repeated on every page), a rule, one line per row and
the page number. A cell longer than 40 characters is cut and ends in `~`, and
a character outside printable ASCII is written as `?`. The bytes depend only
on the tables.

Designators sort in natural order (`K2` before `K10`). A terminal is written
`K1:A1`, with the printed number of the terminal. A CSV cell that a
spreadsheet would read as a formula (it starts with `=`, `+`, `-`, `@`, a tab
or a carriage return) gets a leading apostrophe.

## Comparing two versions

`opencircuit diff <before> <after>` lists what changed between two versions
of a project, one row per element added, removed or changed: `Change`,
`Element` (`project`, `sheet`, `supply`, `device`, `symbol`, `junction`,
`wire`, `waiver`, `text` or `layer`), `ID`, a `Name` a person recognizes (a
designator, `K1 coil`, a sheet title, a layer name) and the `Fields` that
differ. Elements are matched by their stable IDs; waivers by code and
subject. `next_id` and the schema version are left out, so a project saved by
an older build compares as the same document after its migration. The output
is text (`no changes` when there are none), JSON or CSV (`--format`). The
exit code is `0` when the projects are the same and `1` when they differ, as
for `diff(1)`; with `--format json` the document then has `ok: true` and
`same: false`. A file that is not a project is a failure with `ok: false`.

## Terminals and signal arrows

Two standard parts (`std`, the Lines tab) carry a name, a parameter of the type
`name` (1 to 32 letters, digits, `_`, `-` or `.`). `std.terminal_block` is a
terminal of a strip with the parameters `strip` and `number`: it always conducts,
and `opencircuit report --kind strips` lists every terminal by strip and number,
with the wire and the terminals on each side (the terminal plan; its CSV is the
table for a manufacturer's import). `std.signal_arrow` has the parameter `name`:
every arrow with the same name is one net, on the sheet or on another, so a
circuit can continue where a wire cannot go. Each arrow shows under its
designator where the others of its name are. In an edit file they are `place`
and `set_param`. ADR-0016 has the rules; `examples/terminals/` and
`examples/signal-arrows/` are the examples.

## MCP server

`opencircuit mcp [--root <directory>]` is a [Model Context Protocol](https://modelcontextprotocol.io)
server on standard input and output, so that an agent can make, change, check
and draw projects. Messages are JSON-RPC 2.0, one per line; the server answers
`initialize`, `ping`, `tools/list` and `tools/call` (protocol versions
2025-06-18, 2025-03-26 and 2024-11-05), ignores notifications and refuses
batches. Each tool runs the command of the same name with `--format json` and
returns its document as text content (`format: 1`, see JSON output above);
`isError` is true when the command fails.

| Tool | Arguments | Runs |
| --- | --- | --- |
| `new` | `project`, `name`, `force` | `opencircuit new` |
| `validate` | `project`, `library`, `deny_warnings` | `opencircuit validate` |
| `simulate` | `project`, `scenario`, `library`, `tick_us`, `fault_policy` | `opencircuit simulate` |
| `edit` | `project`, `operations` (the list of an edit file), `dry_run`, `library` | `opencircuit edit --apply` |
| `report` | `project`, `kind`, `library` | `opencircuit report` |
| `export` | `project`, `output`, `theme`, `sheet`, `library` | `opencircuit export` |

**Limits.** Every path is relative to the root (the current directory unless
`--root` says otherwise) and is refused when it is absolute, has `..`, or
leaves the root through a symbolic link; nothing is read or written outside
it. A message is at most 4 MiB and a result at most 4 MiB. The `edit` tool
writes its operations to a temporary file in the root and removes it
afterwards. The server has no authentication: run it for the agent that
started it, as a child process, and give it a directory that holds only what
the agent may change.

## Rust crates

The crates under `crates/` are libraries with no GUI, filesystem, socket or
clock access, and they build for WebAssembly; their dependency rules are in
[`ARCHITECTURE.md`](ARCHITECTURE.md). A program that embeds the simulator
reads a project and, when it needs one, a library, compiles, then steps:

```rust
use circuit_model::{ComponentCatalog, InputAction};
use circuit_sim::{SimConfig, Simulator, compile};

fn run(project: &str, library: &str) -> Result<(), Box<dyn std::error::Error>> {
    let catalog = ComponentCatalog::builtin()
        .with_library(circuit_project::library::from_str(library)?)?;
    let doc = circuit_project::from_str(project)?;
    let output = compile(&doc, &catalog);
    let Some(circuit) = output.circuit else {
        for diagnostic in &output.diagnostics {
            eprintln!("{diagnostic}");
        }
        return Err("the project does not compile".into());
    };
    let s1 = doc
        .devices
        .iter()
        .find(|device| device.reference.0 == "S1")
        .ok_or("no S1")?
        .id;

    let mut simulator = Simulator::new(circuit, SimConfig::default())?;
    simulator.step_n(2, &[])?;
    simulator.step_n(3, &[InputAction::Press(s1)])?;
    if let Some(snapshot) = simulator.snapshot() {
        println!("tick {}: active {:?}", snapshot.tick, snapshot.active_devices);
    }
    Ok(())
}
```

Inputs are applied at the boundary of the first tick of `step_n`; the
snapshot describes the last solved tick. The crates are not published on
crates.io yet and their API may change between milestones; the file formats
above are the stable interfaces.

## Not extension points

- **Code plugins.** Nothing loaded at run time is executed: no scripts, no
  dynamic libraries, no WebAssembly modules. New behaviour classes are
  contributed to the core (see [`CONTRIBUTING.md`](CONTRIBUTING.md)).
- **Controlling equipment.** There is no Modbus client and no output to real
  hardware.

## Safety limits

OpenCircuit is a drawing, learning and preliminary simulation tool. Its
electrical model is discrete (continuity and energized state, no voltage,
current or impedance); it does not certify installations, validate
protection or guarantee operational safety.

- **Never connect the Modbus server to a real plant.** It is a simulator,
  not a controller. Modbus has no authentication: anyone who reaches the
  port can operate the simulated circuit. The server listens on loopback
  unless `--listen` names another address, and then it prints a warning.
- **Server limits:** at most 4 clients (others are disconnected at once;
  a connection already closed by its client frees its slot first),
  frames up to 260 bytes (a longer or malformed frame closes the
  connection), idle connections closed after 30 s, at most 1000 ticks
  simulated per turn of the loop.
- **A command never writes over a file it reads.** An output (`--trace`,
  `--snapshot-json`, `edit --output`, a golden trace written by
  `--update-golden`) that names the project, the scenario, the edit file, a
  library or another output of the same command, through any link, is a
  usage error (exit code 2) and nothing is written.
- **Hostile input is refused, never repaired.** Projects, libraries,
  scenarios and Modbus frames are read strictly with size and count limits
  (a library: 1 MiB, 256 models, 32 symbols per model; a library directory:
  256 files; a scenario: 64 libraries). Malformed input is a structured
  error, not a crash; a crash is a bug to report under
  [`SECURITY.md`](SECURITY.md).
- **Libraries are data, and their provenance is declared, not verified.**
  The tool shows the origin, licence and standard reference a library
  claims; checking them is the responsibility of whoever distributes or
  uses it.
- **The browser build runs locally.** It reads only files you drop on the
  page and writes only files you download; it is not published by the
  project.
