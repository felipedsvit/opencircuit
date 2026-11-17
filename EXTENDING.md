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
| `project` | `name`, `author`, `company`, `revision`, `date` | sets what is present; `null` clears an optional field; absent keeps it |
| `add_supply` | `preset` (`dc_24v`, `single_phase_230v`, `three_phase_400v`), `as` | adds a supply profile |
| `place` | `model`, `version`, `symbol`, `at`, `rotation`, `reference`, `as` | a new device drawn as one symbol; the newest `version` unless given; `rotation` 0, 90, 180 or 270 |
| `add_symbol` | `device`, `symbol`, `at`, `rotation`, `as` | another symbol of a device (an auxiliary contact) |
| `set_param` | `device`, `key`, `value` | a boolean, a whole number, a text, `{ "supply": name or ID }`, `{ "device": designator }`, or `null` to clear |
| `set_reference` | `device`, `to` | renames a device |
| `junction` | `at`, `as` | a junction with no wire |
| `connect` | `from`, `to`, `via`, `as` | a wire; an end is `{ "symbol": ..., "terminal": "a1" }`, `{ "junction": name or ID }` or `{ "wire": name or ID, "at": [x, y] }` (a branch); `via` are waypoints; without them the route has at most one bend |
| `move` | `elements`, `by` | moves symbols, junctions or `{ "device": "K1" }` by `[dx, dy]`; wires follow |
| `rotate` | `symbols` | a quarter turn clockwise |
| `delete` | `elements` | deletes elements and the wires that hang on them |
| `add_sheet` | `number`, `title`, `origin`, `size`, `orientation`, `zones` | a sheet; the next number unless given; `size` is `a4` (default), `a3`, `a2` or `{ "custom": { "width": w, "height": h } }`; `zones` is `[columns, rows]`, `[8, 6]` by default |
| `remove_sheet` | `number` | removes a sheet; what was drawn on it stays |
| `set_device_info` | `device`, `manufacturer`, `part_no`, `description`, `location` | the catalog data of a device (ADR-0014); a field left out is kept and `null` clears it |
| `set_wire_number` | `wire` (name or ID), `number` | the number that replaces the generated one on a wire; `null` or no `number` clears it |
| `numbering` | `prefix`, `start`, `step`, `per_sheet`, `enabled` | the rule that numbers the wires: what is left out keeps its value (an empty prefix, 1, 1 and one counter for a new rule); `"enabled": false` removes it |
| `waive` | `code`, `subject`, `reason` | accepts a warning or informational diagnostic with a reason; `subject` is a name, `{ "device": "K1" }`, `{ "wire": ... }`, `{ "junction": ... }`, `{ "symbol": ..., "terminal": "p2" }` for a terminal, or `"document"`; an error cannot be waived |
| `unwaive` | `code`, `subject` | takes a waiver away |
| `renumber` | `sheet` | gives the designators the next numbers in reading order, on every sheet or on the sheet with this number |

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
sheet, and the desktop application has the same in its Export menu. Output is
deterministic: the same project gives the same bytes on every platform,
which is what the committed drawings under `examples/` are checked against.
Library artwork is drawn when its library is loaded.

A project with sheets (ADR-0011, `FORMAT.md`) draws the frame, the zone
marks and the title block of each sheet. Its PDF has one page per sheet, in
sheet-number order, each the size of the paper (A4 landscape is 840 by 590
points); `--sheet <number>` writes one sheet as a PDF or an SVG, and
`--sheet all` is the default for a PDF. An SVG without `--sheet` is the whole
canvas. Text is set in Liberation Mono, embedded in the PDF as a subset
(ADR-0012).

## Reports

`opencircuit report <project> --kind bom|wires|terminals|xref|io|problems|all`
derives tables from the project, and writes them as text, JSON (`--format`)
or, for one table, CSV. With `--output <file>` the extension picks the format
(`.csv`, `.json`, `.txt` or `.pdf`) and `--format` says how the
confirmation is written. Nothing is stored in the project: the tables are a function of it
(ADR-0014).

| Kind | One row per |
| --- | --- |
| `bom` | kind of part: model and version, manufacturer, part number and description; supply taps are not parts |
| `wires` | net with a wire, in numbering order: number, sheet, `From`, `To` and the other terminals under `Also` |
| `terminals` | terminal of every symbol, with the number of its wire and the terminals it meets |
| `xref` | exception: a coil with no contact, contacts whose coil is not drawn, a symbol outside every sheet |
| `io` | address of the Modbus map |
| `problems` | diagnostic of the project, compiled |

A PDF sets each table on A4 landscape pages in the embedded font (ADR-0012):
the title, the headings (repeated on every page), a rule, one line per row and
the page number. A cell longer than 40 characters is cut and ends in `~`, and
a character outside printable ASCII is written as `?`. The bytes depend only
on the tables.

Designators sort in natural order (`K2` before `K10`). A terminal is written
`K1:A1`, with the printed number of the terminal. A CSV cell that a
spreadsheet would read as a formula (it starts with `=`, `+`, `-`, `@`, a tab
or a carriage return) gets a leading apostrophe.

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
- **Server limits:** at most 4 clients (others are disconnected at once),
  frames up to 260 bytes (a longer or malformed frame closes the
  connection), idle connections closed after 30 s, at most 1000 ticks
  simulated per turn of the loop.
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
