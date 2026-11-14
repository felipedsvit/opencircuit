# Project format `.ocircuit`

This document is the normative contract of `circuit-project`. The design
rationale is in [`docs/spec02.md`](docs/spec02.md) §6 and §15. When this
document and the code disagree, the code and its tests win and this document
is fixed in the same change.

## Rules

- The file is JSON (UTF-8) with the `.ocircuit` extension.
- `schema_version` is mandatory. The current version is 1.
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

## Shape (schema version 1)

```json
{
  "schema_version": 1,
  "next_id": 6,
  "project": {
    "name": "demo",
    "grid_unit": 10
  },
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

Field values:

| Field | Values |
| --- | --- |
| `supplies[].kind` | `dc`, `single_phase`, `three_phase` |
| `supplies[].phase_sequence` | `l1_l2_l3`, `l1_l3_l2`, `null` |
| `symbols[].rotation` | `deg0`, `deg90`, `deg180`, `deg270` |
| `symbols[].slot` | positive integer for slotted symbols (auxiliary contacts), otherwise `null` |
| endpoint | `{ "terminal": { "symbol_id": <id>, "terminal": <key> } }` or `{ "junction": <id> }` |
| `params` values | boolean, integer or string |
| supply terminal `conductor` | `l1`, `l2`, `l3`, `n`, `pe`, `dc_pos`, `dc_neg` |
| load `connection` | `dc`, `line_neutral`, `line_line` |

A supply profile must be consistent with its kind: DC has no frequency, phase
voltage or sequence; single-phase has a frequency and no sequence;
three-phase has a frequency and a sequence; voltages are positive.

## Terminal keys

Keys are structural and belong to the symbol kind; displayed numbers such as
13/14 are labels, not keys.

| Symbol kind | Terminals |
| --- | --- |
| coil | `a1`, `a2` (both required) |
| contact, fuse link | `p1`, `p2` |
| main contacts, thermal element | `pole1.in`, `pole1.out`, … per pole |
| motor | `u1`, `v1`, `w1` |
| lamp, buzzer | `x1`, `x2` |
| supply terminal | `out` |

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
| coil, contact, fuse link, lamp, buzzer | first terminal `(0, -2)`, second `(0, 2)` |
| main contacts, thermal element (`n` poles) | pole `k`: `x = (2k - n - 1) * 2`, `in` at `y = -2`, `out` at `y = 2` |
| motor | `u1` `(-4, -2)`, `v1` `(0, -2)`, `w1` `(4, -2)` |
| supply terminal | `out` `(0, 0)` |

The poles of a multi-pole symbol are 4 units apart and centred on its origin.
Geometry carries no electrical meaning and does not enter the circuit
fingerprint. The editor draws wires with horizontal and vertical segments
only; a file may contain other wires, which load normally and compile with
the warning `OC-STR-009` (`NonOrthogonalWire`).

## Catalog (version 1 of each model)

| Model | Symbols (instances) | Parameters |
| --- | --- | --- |
| `supply.terminal` | `terminal` (1..255) | `conductor`, `supply` |
| `pushbutton_no`, `pushbutton_nc` | `contact` (1) | — |
| `selector.latching` | `contact` (1, NO) | `initial_closed` (default `false`) |
| `main_switch.3p` | `main` (1, three poles) | `initial_closed` (default `false`) |
| `contactor.generic` | `coil` (1), `main` (0..1), `aux_no` (0..8, slotted), `aux_nc` (0..8, slotted) | `rated_voltage_mv`, `connection` |
| `relay.generic` | `coil` (1), `aux_no`, `aux_nc` (0..8, slotted) | `rated_voltage_mv`, `connection` |
| `thermal_overload.generic` | `element` (0..1, three poles), `aux_nc` (0..1), `aux_no` (0..1) | — |
| `timer_on.generic`, `timer_off.generic` | `coil` (1), `delayed_no`, `delayed_nc` (0..4, slotted) | `rated_voltage_mv`, `connection`, `preset_ms` |
| `fuse.generic` | `link` (1) | — |
| `motor.3ph` | `motor` (1) | `rated_voltage_mv` |
| `lamp.generic`, `buzzer.generic` | `lamp` / `buzzer` (1) | `rated_voltage_mv`, `connection` |

A published `(model id, version)` never changes. Changing a terminal key, a
terminal offset, a parameter or a behaviour creates a new model version with a
tested model migration.

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
   value.

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
