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
- `schema_version` is mandatory; this build reads version 1.
- `id` is the namespace of the library: a lowercase letter followed by 1 to
  31 lowercase letters, digits or underscores. It may not be `opencircuit`,
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

## Example

```json
{
  "schema_version": 1,
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
| `class` | `contactor`, `relay`, `thermal_overload`, `timer_on`, `timer_off`, `pushbutton`, `latching_switch`, `fuse`, `motor`, `lamp`, `buzzer`, `power_supply`, `mechanical_interlock`, `transformer` |
| `operator` | `null` (the default of the class), `none`, `push`, `latch`, `emergency`, `position` |
| `kind` | `coil`, `contact_no`, `contact_nc`, `changeover`, `{ "power_contacts": { "poles": n } }`, `{ "thermal_element": { "poles": n } }`, `fuse_link`, `motor`, `motor_six_terminal`, `lamp`, `buzzer`, `supply_terminal`, `interlock_link`, `transformer_primary`, `transformer_secondary` |
| parameter `type` | `"bool"`, `{ "int": { "min": a, "max": b } }`, `{ "enum": [ … ] }`, `"supply_ref"`, `"device_ref"` |
| parameter `default` | `null` (mandatory parameter), a boolean, an integer or a string |

Terminals, their keys and their positions come from the kind
([`FORMAT.md`](FORMAT.md), "Terminal keys" and "Symbol geometry").

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
