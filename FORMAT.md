# Project format `.ocircuit`

The first project format is JSON with the `.ocircuit` extension. The
authoritative design rationale is in
[`OpenCircuit_Arquitetura_Tecnica_v0.2.md`](OpenCircuit_Arquitetura_Tecnica_v0.2.md).

## Requirements

- `schema_version` is mandatory;
- `next_id` is mandatory and must be greater than every persisted ID;
- persisted entities use stable IDs and terminal keys, never dense indices;
- devices store `model_id` + `model_version`; terminals live in symbol models;
- serialization order is deterministic;
- the document is validated before loading;
- the application layer performs atomic saves;
- `schema_version` greater than the supported version is refused;
- unknown fields are rejected in v1; they are not preserved;
- round-trip is guaranteed only within the same schema version.

## Shape (schema version 1)

```json
{
  "schema_version": 1,
  "next_id": 42,
  "name": "Partida Direta",
  "grid_unit": 10,
  "devices": [
    {
      "id": 1,
      "reference": "K1",
      "model_id": "contactor.3p",
      "model_version": 1,
      "parameters": {}
    }
  ],
  "symbols": [
    {
      "id": 10,
      "device_id": 1,
      "symbol_model_id": "contactor.3p.coil",
      "position": { "x": 0, "y": 0 },
      "rotation": "deg0"
    }
  ],
  "junctions": [],
  "wires": [],
  "supplies": []
}
```

Notes:

- arrays are serialized sorted by ID;
- `Wire.route` stores only intermediate waypoints; the endpoints are derived
  from `from` and `to`;
- `next_id` is a global counter. New entities consume the next value; IDs are
  never reused after deletion;
- the example is minimal. A real project contains supplies, wires, junctions
  and the remaining symbols.

## Versioning

| Found version | Behavior |
| --- | --- |
| `== CURRENT` | Deserialize, validate and return. |
| `< CURRENT` | Apply migrations in order, then validate. |
| `> CURRENT` | Refuse with `UnsupportedSchemaVersion { found, supported }`. |

v1 uses `#[serde(deny_unknown_fields)]`. A field unknown to v1 is an error, not
a silently ignored value. Adding a field requires a new `schema_version` and a
migration. There is no promise to preserve unknown fields.

Round-trip guarantee:

```text
canonicalize(load(save(doc))) == canonicalize(doc)
```

for valid v1 documents. Structural validation and `next_id` validation happen
before the document is returned.

## I/O boundary

`circuit-project` converts between model structures and JSON. It does not call
`std::fs` and does not know paths. The desktop application or CLI chooses paths
and performs atomic writes:

1. write to a temporary file in the same directory;
2. flush and `fsync`;
3. rename over the destination;
4. remove the temporary file on error.

## Migrations

Every schema version must have a tested migration path. Changing the meaning or
key of a terminal requires an explicit component migration; terminal semantics
are never changed silently. The component model version may differ from the
document schema version.

A migration:

- receives the JSON of the previous version;
- produces the JSON of the next version;
- is idempotent at the destination version;
- has a test with a real document, not only an isolated field;
- never invents IDs; it uses `next_id` or computes the next free value.

## Status

The format is not implemented yet. The JSON example documents the intended v1
schema and must be updated together with the first migration.
