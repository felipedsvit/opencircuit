# Project format `.ocircuit`

This document is the normative contract of `circuit-project`. The design
rationale is in [`docs/spec02.md`](docs/spec02.md) §6 and §15. When this
document and the code disagree, the code and its tests win and this document
is fixed in the same change.

## Rules

- The file is JSON (UTF-8) with the `.ocircuit` extension.
- `schema_version` is mandatory. The current version is 3.
- `next_id` is mandatory. Every entity of every collection draws its ID from
  this one counter: IDs start at 1, are unique across collections, are below
  `next_id`, are at most `2^53 - 1`, and are never reused.
- There is no floating point anywhere. Voltages are in millivolts, frequencies
  in millihertz, times in milliseconds. `1.0` is rejected where an integer is
  expected.
- Devices store a versioned model reference and parameters, never a behaviour
  class. Terminals belong to the symbols of the catalog.
- Wires store endpoints and intermediate waypoints only. Two wires that cross
  are not connected; a junction is an explicit entity.
- Unknown fields, repeated fields and repeated parameter keys are rejected.
  Unknown fields are not preserved.

## Shape (schema version 3)

```json
{
  "schema_version": 3,
  "next_id": 6,
  "project": {
    "name": "demo",
    "grid_unit": 10,
    "author": null,
    "company": null,
    "revision": null,
    "date": null
  },
  "sheets": [],
  "supplies": [
    {
      "id": 1,
      "kind": "dc",
      "nominal_voltage_mv": 24000,
      "phase_voltage_mv": null,
      "frequency_millihz": null,
      "phase_sequence": null
    }
  ],
  "devices": [
    {
      "id": 2,
      "reference": "P24",
      "model": {
        "id": "supply.terminal",
        "version": 1
      },
      "params": {
        "conductor": "dc_pos",
        "supply": 1
      }
    }
  ],
  "symbols": [
    {
      "id": 3,
      "device_id": 2,
      "symbol_key": "terminal",
      "slot": null,
      "position": {
        "x": 0,
        "y": -4
      },
      "rotation": "deg0"
    }
  ],
  "junctions": [
    {
      "id": 4,
      "position": {
        "x": 2,
        "y": 0
      }
    }
  ],
  "wires": [
    {
      "id": 5,
      "from": {
        "terminal": {
          "symbol_id": 3,
          "terminal": "out"
        }
      },
      "to": {
        "junction": 4
      },
      "route": [
        {
          "x": 1,
          "y": 0
        }
      ]
    }
  ]
}
```

This text is the canonical form of a small document. Unit tests of
`circuit-project` compare it with the test document byte for byte, and compare
the catalog table below with the built-in catalog (every model, symbol key and
parameter key must appear in its row). Complete projects are in `examples/`.

Documentation fields (ADR-0014) are optional, and a file omits each one that
is empty, so the document above is also the shape of a project that uses
none of them:

| Field | Meaning |
| --- | --- |
| `wires[].number` | text of at most 256 bytes that replaces the generated number of the wire's net |
| `devices[].info` | object with the optional text fields `manufacturer`, `part_no`, `description` and `location`, each at most 256 bytes |
| `project.wire_numbering` | `{ "prefix": <text of at most 16 bytes>, "start": <integer>, "step": <integer, at least 1>, "per_sheet": <boolean> }`; absent: wires are not numbered |
| `waivers` | list of `{ "code": "OC-STR-018", "subject": <subject>, "reason": <text> }`, sorted by code and then subject, at most 10 000 |

Wire numbers are derived from the nets and are never stored (ADR-0014): one
number per net, in the order of the first point of the net (sheet, then zone
row, then zone column, then `y`, then `x`). `wire_numbering` gives the prefix
and the counter; `per_sheet` writes `<prefix><sheet>.<counter>` and restarts
the counter on every sheet. A `wires[].number` replaces the number of its net
without moving the others; if two wires of one net carry different overrides,
the one on the wire with the smaller ID wins and the document has the warning
`OC-SEM-012`. A project without `wire_numbering` draws no wire number.

A subject is the tagged form the command line prints: `"document"`,
`{ "supply": 1 }`, `{ "device": 2 }`, `{ "symbol": 3 }`, `{ "wire": 5 }`,
`{ "junction": 4 }`, `{ "sheet": 7 }` or
`{ "pin": { "symbol_id": 3, "terminal": "out" } }`.

Field values:

| Field | Values |
| --- | --- |
| `supplies[].kind` | `dc`, `single_phase`, `three_phase` |
| `supplies[].phase_sequence` | `l1_l2_l3`, `l1_l3_l2`, `null` |
| `symbols[].rotation` | `deg0`, `deg90`, `deg180`, `deg270` |
| `symbols[].slot` | positive integer for slotted symbols (auxiliary contacts), otherwise `null` |
| endpoint | `{ "terminal": { "symbol_id": <id>, "terminal": <key> } }` or `{ "junction": <id> }` |
| `params` values | boolean, integer or string; a supply or device reference is the integer ID |
| supply terminal `conductor` | `l1`, `l2`, `l3`, `n`, `pe`, `dc_pos`, `dc_neg` |
| load `connection` | `dc`, `line_neutral`, `line_line` |
| `project.author`, `company`, `revision`, `date` | text for the title block, or `null`; the date is written by the author, the core has no clock |
| `sheets[].size` | `a4`, `a3`, `a2`, or `{ "custom": { "width": <grid units>, "height": <grid units> } }` |
| `sheets[].orientation` | `landscape`, `portrait` |
| `sheets[].zones` | `{ "columns": 1..26, "rows": 1..26 }` |

A supply profile must be consistent with its kind: DC has no frequency, phase
voltage or sequence; single-phase has a frequency and no sequence;
three-phase has a frequency and a sequence; voltages are positive.

## Sheets

A sheet is a frame over the canvas (ADR-0011): an `id` from the document
counter, a `number` printed in the title block, a `title`, the `origin` of its
top-left corner, a paper `size`, an `orientation` and its `zones`. Sheets have
no electrical meaning; wires connect across them as everywhere else.

- One grid unit is 10 points (about 3.53 mm), the PDF scale. A paper size is
  the largest whole number of grid units that fits it, in landscape: A4 is
  84 by 59, A3 119 by 84, A2 168 by 119. Portrait swaps the two.
- Zones divide the frame evenly: columns are numbered from 1, left to right,
  and rows lettered from A, top to bottom, so a point is in zone `4B`.
- The sheet of a symbol is the frame that holds its origin. It is derived,
  never stored; a symbol outside every frame is on no sheet.
- A document without sheets is drawn and exported as one page sized to its
  content, as in version 1.

Drawing (the renderer, not the file): the frame is two rectangles, 12
subunits apart, with zone ticks across the band between them and the zone
numbers (top and bottom) and letters (left and right) centred in it. The title
block is 30 by 6.4 grid units in the bottom right corner inside the border,
when the sheet is large enough, with four rows: the sheet title; the project
name and the company; the author and the date; the revision and `Sheet n/N`.
Texts are cut to the width of their cell. The page of a sheet in a PDF or SVG
is exactly its frame; what lies outside is not drawn, and what crosses the
edge is cut.

Cross-references are drawn when the project has sheets and are never stored
(`circuit_model::cross_references`). Under the designator of a coil (or a
transformer primary) there is one line per contact of its device, as the first
and last terminal number and the place of the contact, such as `13-14 1/2C`
(sheet 1, column 2, row C) or `1-6 2/2B`; `-` stands for a contact outside
every sheet. Beside each contact, under its designator, goes the place of the
coil. Contacts are listed by sheet and zone, then by symbol. A device without
a coil, such as a pushbutton or a thermal relay, has none.

Validation: a number that is 0 or used twice is `OC-STR-015`
(`DuplicateSheetNumber`); a frame with 0 or more than 26 zone columns or
rows, a side of 0 or more than 1 000 grid units, a frame beyond the
coordinate range, or two overlapping frames is `OC-STR-016`
(`InvalidSheet`). The compiler warns with `OC-STR-017` (`WireCrossesSheet`)
about a wire that crosses the edge of a frame, which printing would cut.

## Terminal keys

Keys are structural and belong to the symbol kind; displayed numbers such as
13/14 are labels, not keys. A label is never stored: it is derived from the
symbol kind, the class of the device and the slot of the symbol
(`circuit_model::terminal_label`): A1/A2 on a coil, n3/n4 on the normally
open contact in slot n and n1/n2 on a normally closed one (n7/n8 and n5/n6
on a timer), 95/96 and 97/98 on a thermal overload relay, 1 to 6 on power
contacts, U1 to W2 on a motor, X1/X2 on a lamp, the number of the key on a
block (`t3` is 3). A slot above 9 and a supply terminal get no label.

| Symbol kind | Terminals |
| --- | --- |
| coil | `a1`, `a2` (both required) |
| transformer primary | `a1`, `a2` (both required) |
| transformer secondary | `x1` (provides `l1`), `x2` (provides `n`) |
| contact, fuse link | `p1`, `p2` |
| changeover contact | `com`, `nc`, `no` |
| main contacts, thermal element | `pole1.in`, `pole1.out`, … per pole |
| motor | `u1`, `v1`, `w1` |
| six-terminal motor | `u1`, `v1`, `w1`, `u2`, `v2`, `w2` |
| lamp, buzzer | `x1`, `x2` |
| supply terminal | `out` |
| interlock link | none |
| block (2, 4 and 8 terminals), symbol-only | `t1`, `t2` / `t1` to `t4` / `t1` to `t8` |

## Symbol geometry

Coordinates are integer grid units; `x` grows to the right and `y` grows
downwards. A file stores the `position` and `rotation` of each symbol and the
waypoints of each wire; every other position is derived from the catalog:

- a pin is at the symbol `position` plus the terminal offset rotated by the
  symbol `rotation`;
- a wire runs from the position of `from`, through `route`, to the position of
  `to`;
- `deg90` is a quarter turn clockwise on screen: the offset `(0, -1)` becomes
  `(1, 0)`.

Terminal offsets, unrotated:

| Symbol kind | Offsets |
| --- | --- |
| coil, contact, fuse link, lamp, buzzer, transformer primary and secondary | first terminal `(0, -2)`, second `(0, 2)` |
| changeover contact | `com` `(0, 2)`, `nc` `(2, -2)`, `no` `(0, -2)` |
| main contacts, thermal element (`n` poles) | pole `k`: `x = (2k - n - 1) * 2`, `in` at `y = -2`, `out` at `y = 2` |
| motor | `u1` `(-4, -2)`, `v1` `(0, -2)`, `w1` `(4, -2)` |
| six-terminal motor | as the motor, and `u2` `(-4, 8)`, `v2` `(0, 8)`, `w2` `(4, 8)` |
| supply terminal | `out` `(0, 0)` |
| block of 2 terminals | `t1` `(0, -2)`, `t2` `(0, 2)` |
| block of 4 terminals | `t1` `(-2, -2)`, `t2` `(2, -2)`, `t3` `(-2, 2)`, `t4` `(2, 2)` |
| block of 8 terminals | `t1` to `t4` at `x = -6, -2, 2, 6` and `y = -2`; `t5` to `t8` at the same `x` and `y = 2` |

The poles of a multi-pole symbol are 4 units apart and centred on its origin.
Geometry carries no electrical meaning and does not enter the circuit
fingerprint. The editor draws wires with horizontal and vertical segments
only; a file may contain other wires, which load normally and compile with
the warning `OC-STR-009` (`NonOrthogonalWire`).

## Catalog

| Model | Version | Symbols (instances) | Parameters |
| --- | --- | --- | --- |
| `supply.terminal` | 1 | `terminal` (1..255) | `conductor`, `supply` |
| `pushbutton_no`, `pushbutton_nc` | 1 | `contact` (1) | — |
| `pushbutton.combined` | 1 | `no` (1), `nc` (1), one button | — |
| `estop.latching` | 1 | `contact` (1, NC, mushroom head) | `initial_closed` (default `false`) |
| `limit_switch` | 1 | `no` (0..1), `nc` (0..1), roller | — |
| `selector.latching` | 1 | `contact` (1, NO) | `initial_closed` (default `false`) |
| `main_switch.3p` | 1 | `main` (1, three poles) | `initial_closed` (default `false`) |
| `contactor.generic` | 1 | `coil` (1), `main` (0..1), `aux_no` (0..8, slotted), `aux_nc` (0..8, slotted) | `rated_voltage_mv`, `connection` |
| `relay.generic` | 1 | `coil` (1), `aux_no`, `aux_nc` (0..8, slotted) | `rated_voltage_mv`, `connection` |
| `relay.generic` | 2 | `coil` (1), `aux_no`, `aux_nc`, `changeover` (0..8, slotted) | `rated_voltage_mv`, `connection` |
| `thermal_overload.generic` | 1 | `element` (0..1, three poles), `aux_nc` (0..1), `aux_no` (0..1) | — |
| `timer_on.generic`, `timer_off.generic` | 1 | `coil` (1), `delayed_no`, `delayed_nc` (0..4, slotted) | `rated_voltage_mv`, `connection`, `preset_ms` |
| `timer_on.generic`, `timer_off.generic` | 2 | `coil` (1), `delayed_no`, `delayed_nc`, `delayed_co` (0..4, slotted) | `rated_voltage_mv`, `connection`, `preset_ms` |
| `fuse.generic` | 1 | `link` (1) | — |
| `interlock.mechanical` | 1 | `link` (1, no terminals) | `first`, `second` (device IDs of two contactors) |
| `motor.3ph` | 1 | `motor` (1) | `rated_voltage_mv` |
| `motor.3ph.6t` | 1 | `motor` (1, six terminals) | `rated_voltage_mv` (line voltage in delta) |
| `transformer.control` | 1 | `primary` (1), `secondary` (1) | `rated_voltage_mv`, `connection` (primary), `secondary` (single-phase supply ID) |
| `lamp.generic`, `buzzer.generic` | 1 | `lamp` / `buzzer` (1) | `rated_voltage_mv`, `connection` |

An emergency stop is a maintained switch: `close` latches it pressed, which
opens its NC contact, and `open` releases it. A limit switch is momentary:
`press` while the machine or the level holds it. Their operator (mushroom
head, roller) only changes the drawing and the designator prefix (`B` for a
limit switch).

A published `(model id, version)` never changes. Changing a terminal key, a
terminal offset, a parameter or a behaviour creates a new model version with a
tested model migration. Older versions stay in the catalog, so a file keeps
loading and saving byte for byte; new devices use the newest version. A
migration is applied only when the user asks for it (an edit that changes the
device's `model`), never on load. The registered migrations keep every symbol
and parameter key: `relay.generic`, `timer_on.generic` and `timer_off.generic`
from version 1 to version 2, which add changeover contacts.

## Canonical serialization

- fields in the fixed order shown above;
- every collection sorted by ID; parameters sorted by key;
- two spaces of indentation, `null` written explicitly, LF line endings and a
  final newline.

Saving a canonical file that was just loaded reproduces it byte for byte.
Loading a non-canonical file (other whitespace, other collection order,
absent `null` fields) is accepted and saves canonically. The repository keeps
`*.ocircuit` files LF through `.gitattributes`.

## Reading

1. **Phase 1:** parse JSON and read only `schema_version`, which must be a
   non-negative integer.
   - Greater than supported: `NewerSchema { found, supported }`. Nothing else
     is read.
   - Smaller: apply the migrations in order.
2. **Phase 2:** deserialize the current schema strictly.
3. **Limits:** text at most 16 MiB; at most 100 000 entities per collection,
   1 024 route points per wire, 64 parameters per device, 256 bytes per text
   value, 10 000 waivers.

Loading checks syntax, schema and limits only. Whether the document makes
sense (ID ranges and uniqueness, references, designators, models, terminals,
parameters) is reported by `Schematic::validate` and the compiler as
diagnostics, so that a broken project can still be opened and explained.

## I/O boundary

`circuit-project` converts between text and the model. It does not call
`std::fs` and does not know paths. The application writes atomically:

1. write a temporary file in the same directory;
2. flush and `fsync`;
3. rename it over the destination;
4. remove the temporary file on error.

## Migrations

A migration is a pure function from the JSON value of version `N` to the
JSON value of version `N + 1`. It must set `schema_version` to `N + 1`, has a
pair of before/after fixtures with a test, never invents IDs (it uses
`next_id`), and is idempotent at its destination. Every schema change adds a
migration and an entry below.

## Changelog

| Version | Change |
| --- | --- |
| 1 | First schema. |
| 2 | `sheets`, and `author`, `company`, `revision` and `date` in `project` (ADR-0011). The migration adds no sheet and sets the four fields to `null`. |
| 3 | `wires[].number`, `devices[].info`, `project.wire_numbering` and `waivers` (ADR-0014). All are optional and omitted when empty, so the migration only raises the version. |
