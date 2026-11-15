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
