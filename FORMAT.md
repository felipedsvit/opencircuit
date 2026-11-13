# Project format `.ocircuit`

The first project format is JSON with the `.ocircuit` extension.

## Requirements

- `schema_version` is mandatory;
- persisted entities use stable IDs and terminal keys, never dense indices;
- serialization order is deterministic;
- migrations are explicit and tested;
- the document is validated before loading;
- the application layer performs atomic saves;
- unknown fields are preserved when compatible with the schema strategy.

## Shape (schema version 1)

```json
{
  "schema_version": 1,
  "project": {
    "name": "Partida Direta",
    "grid_unit": 10
  },
  "devices": [],
  "symbols": [],
  "junctions": [],
  "wires": [],
  "supplies": []
}
```

## I/O boundary

`circuit-project` converts between domain structures and JSON. It does not call
`std::fs`. The desktop application or CLI chooses paths and performs atomic
writes.

## Migrations

Every schema version must have a tested migration path. Changing the meaning or
key of a terminal requires an explicit component migration; terminal semantics
are never changed silently. The component model version may differ from the
document schema version.

## Status

The format is not implemented yet. The JSON example documents the intended
schema and must be updated together with the first migration.
