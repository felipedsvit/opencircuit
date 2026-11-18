# Extension libraries

This document is the normative contract of extension libraries, read by
`circuit_project::library::from_str` and added to a catalog by
`ComponentCatalog::with_library`. The design rationale is in
[ADR-0008](docs/adr/0008-extension-libraries.md). When this document and the
code disagree, the code and its tests win and this document is fixed in the
same change.

A library is data. It defines component models that reuse the behaviour
classes and symbol kinds of OpenCircuit, with its own parameters, symbols
and, optionally, artwork. Nothing in a library is executed.

## Rules

- The file is JSON (UTF-8). It is read strictly: unknown fields, repeated
  fields and floating-point numbers are errors.
- `schema_version` is mandatory; this build reads versions 1, 2 and 3. Version 2
  adds the optional model fields `category`, `title` and `note`; a version 1
  file may not use them, and its models take the category of their class and
  their identifier as title.
- `id` is the namespace of the library: a lowercase letter followed by 1 to
  31 lowercase letters, digits or underscores. It may not be `opencircuit` or
  `std` (the standard library),
  nor the first segment of a built-in model identifier (`relay`,
  `contactor`, `pushbutton_no`, …), nor the namespace of a library already
  loaded.
- Every model identifier is `<id>.<name>`, where `name` has 1 to 64
  lowercase letters, digits, underscores or dots.
- Models are checked exactly like built-in models: the class must accept the
  symbol kinds, the parameters the class reads must be present with the
  right type, a coil has one instance, and so on.
- `provenance` is mandatory: `origin`, `license` (an SPDX expression) and
  `standard_reference` (or `null`). Every model takes it. The tool shows it;
  it does not judge it.
- Limits: 1 MiB of text, 256 models, 32 symbols and 64 parameters per model,
  256 primitives per artwork list and 256 points per polyline, 256 bytes per
  text.
- A project refers to a library model by its identifier and version, like any
  model. When the library is not loaded, compilation reports `OC-STR-003`
  (`UnknownModel`).

## The standard library

OpenCircuit ships one library of its own, namespace `std`
(`circuit_project::stdlib`). It is built in code, so it is not a file, and
the command line adds it to the built-in catalog before any library of yours:
a project may use `std.*` models with no `--library`. The namespace is
therefore taken, like `opencircuit`. Its parts follow the eight palette
tabs (`category`):

| Tab (`category`) | Parts |
| --- | --- |
| Supplies and sources (`supplies`) | terminals of each conductor (L1, L2, L3, N, PE, DC +, DC −, DC ground), battery terminals, socket contacts; symbol-only: diode, bridge rectifier, three-phase transformer, autotransformer |
| Protection (`protection`) | cartridge, Diazed, NH and glass fuses; circuit breakers of 1 to 4 poles; a motor protection breaker; symbol-only: SPD, RCD (2 and 4 poles) |
| Controls (`controls`) | contactors of 1, 2 and 4 poles, two-position selector, phase monitoring relay (approximation) |
| Motors and power (`motors`) | single-phase and DC motors and a resistive heater (energized or not), a three-phase heater, soft starter and frequency inverter as a RUN-driven pass-through (approximation); symbol-only: servo and stepper motors |
| Contacts and terminals (`contacts`) | square, round and stacked terminals, connection point |
| Automation and logic (`automation`) | solenoid or valve coil, inductive, capacitive, optical and ultrasonic sensors with a contact output; symbol-only: PLC, AND/OR/NOT blocks, GRAFCET step, initial step and transition |
| Mechanical actuators (`actuators`) | symbol-only: single and double-acting cylinders, directional valves |
| Lines and connections (`lines`) | busbar or through link |

A part that is only a symbol says so in its `note`
([ADR-0013](docs/adr/0013-symbolic-devices.md)); so does one that
approximates a behaviour the discrete model does not have. What is missing
on purpose (multi-conductor supply symbols, a simulated three-phase
transformer or rectifier, selectors with a middle position, switch
interlocks, static wire colours) needs a change of the simulator and is
tracked separately.

## Example

```json
{
  "schema_version": 2,
  "id": "acme",
  "version": 1,
  "name": "ACME control relays",
  "provenance": {
    "origin": "ACME example library, written for the OpenCircuit documentation",
    "license": "MIT OR Apache-2.0",
    "standard_reference": null
  },
  "models": [
    {
      "id": "acme.relay_r2",
      "version": 1,
      "class": "relay",
      "operator": null,
      "category": "automation", "title": "ACME relay R2", "note": "Example only",
      "params": [
        { "key": "rated_voltage_mv", "type": { "int": { "min": 1, "max": 4294967295 } }, "default": 24000 },
        { "key": "connection", "type": { "enum": ["dc"] }, "default": "dc" }
      ],
      "symbols": [
        { "key": "coil", "kind": "coil", "min": 1, "max": 1, "slotted": false, "artwork": null },
        {
          "key": "no",
          "kind": "contact_no",
          "min": 0,
          "max": 2,
          "slotted": true,
          "artwork": {
            "rest": [
              { "line": { "from": [0, -20], "to": [0, -8] } },
              { "line": { "from": [0, 8], "to": [0, 20] } },
              { "line": { "from": [0, 8], "to": [-7, -7] } },
              { "rect": { "from": [-12, -12], "to": [12, 12] } }
            ],
            "operated": [
              { "line": { "from": [0, -20], "to": [0, -8] } },
              { "line": { "from": [0, 8], "to": [0, 20] } },
              { "line": { "from": [0, 8], "to": [0, -8] } },
              { "rect": { "from": [-12, -12], "to": [12, 12] } }
            ]
          }
        }
      ]
    }
  ]
}
```

Unit tests of `circuit-project` read this example and add it to the built-in
catalog.

## Field values

| Field | Values |
| --- | --- |
| `class` | `contactor`, `relay`, `thermal_overload`, `timer_on`, `timer_off`, `pushbutton`, `latching_switch`, `fuse`, `motor`, `lamp`, `buzzer`, `power_supply`, `mechanical_interlock`, `transformer`, `terminal_block` (schema 3), `signal_arrow` (schema 3), `symbolic` |
| `operator` | `null` (the default of the class), `none`, `push`, `latch`, `emergency`, `position` |
| `category` (schema 2) | `null` or absent (the default of the class), `supplies`, `protection`, `controls`, `motors`, `contacts`, `automation`, `actuators`, `lines`: the palette tab of the model |
| `title`, `note` (schema 2) | `null` or absent, or text: a one-line title shown in the palette (the identifier by default) and a note shown to the user, such as the limits of an approximation. Neither may be empty, hold a control character or exceed 256 bytes (512 for a note) |
| `kind` | `coil`, `contact_no`, `contact_nc`, `changeover`, `{ "power_contacts": { "poles": n } }`, `{ "thermal_element": { "poles": n } }`, `fuse_link`, `motor`, `motor_six_terminal`, `lamp`, `buzzer`, `supply_terminal`, `interlock_link`, `transformer_primary`, `transformer_secondary`, `terminal_point` (schema 3), `signal_arrow` (schema 3), `block2`, `block4`, `block8` |
| parameter `type` | `"bool"`, `{ "int": { "min": a, "max": b } }`, `{ "enum": [ … ] }`, `"supply_ref"`, `"device_ref"`, `"name"` (schema 3: a text of 1 to 32 letters, digits, `_`, `-` or `.`) |
| parameter `default` | `null` (mandatory parameter), a boolean, an integer or a string |

Terminals, their keys and their positions come from the kind
([`FORMAT.md`](FORMAT.md), "Terminal keys" and "Symbol geometry"), and so
do the terminal numbers drawn next to them, from the kind, the class and
the slot: a library model gets the same markings as a built-in one.

## Artwork

A symbol without `artwork` is drawn with the artwork of its kind. With
artwork, `rest` is drawn at rest and `operated`, when present, while the
device is operated: a pushbutton held, a maintained switch closed, a
contactor or relay picked up, a timer output high, a thermal relay tripped
(so a normally closed contact draws itself open in `operated`). The lines
take the style of the state (operated, energized, tripped, motor running)
and texts are labels, as for built-in symbols. Terminal positions, and
therefore wires, never depend on the artwork.

Coordinates are integers in scene subunits (ten per grid unit) around the
symbol origin, unrotated, `y` downwards. Primitives:

| Primitive | Shape |
| --- | --- |
| `{ "line": { "from": [x, y], "to": [x, y] } }` | a segment |
| `{ "polyline": [[x, y], …] }` | connected segments, 2 to 256 points |
| `{ "rect": { "from": [x, y], "to": [x, y] } }` | a rectangle outline |
| `{ "circle": { "center": [x, y], "radius": r } }` | a circle outline |
| `{ "dot": { "center": [x, y], "radius": r } }` | a filled circle |
| `{ "text": { "at": [x, y], "text": "…" } }` | a short label, centred |

Every primitive must lie inside the bounds of the kind, and in each list a
line or polyline must end on every terminal of the kind, so that wires meet
the drawing. A library that breaks these rules is refused when it is loaded.

## Symbol sets and the symbol editor

Two libraries ship under `libraries/` (M9.4): `iec-style.oclib` (namespace
`iec`) and `ansi-style.oclib` (namespace `ansi`). Each has a relay (coil, and
slotted normally open and normally closed contacts), a normally open and a
normally closed pushbutton, a pilot lamp and a fuse, drawn in an IEC-like or
an ANSI-like style. They are drawn for OpenCircuit from the geometry of the
symbol kinds, copy no symbol file, and claim no conformance with any
standard. They are generated by `cargo run -p xtask -- generate-examples`
(`xtask/src/symbol_sets.rs`), which `--check` keeps in step; the window loads
them from File > Symbol sets, and the command line from `--library
libraries/iec-style.oclib`.

The window has a symbol editor (File > Symbol editor…) that writes a library
of one part: a namespace, a part name and title, what the part is (lamp,
buzzer, fuse, relay coil, pushbutton, block of 2 or 4 terminals), and its
drawing typed one primitive per line (`line x1 y1 x2 y2`, `polyline x y x y
…`, `rect`, `circle x y r`, `dot x y r`, `text x y words`, in subunits). The
editor checks the library with the same reader as a file, shows the drawing,
loads it into the window (replacing a library of the same namespace) or saves
it as an `.oclib`.

