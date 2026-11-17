# ADR-0014: Derived documentation, with overrides stored

Status: Accepted (2026-11-17). Builds on [ADR-0004](0004-canonical-json-format.md)
(canonical JSON, version policy) and [ADR-0011](0011-sheets-as-frames.md)
(sheets as frames, derived cross-references). Tracking issue: #85 (M7).

## Context

Electrical documentation needs things that the document does not carry
today: a number for every wire, a bill of materials with part numbers, a
from/to list, designators renumbered by position, and a way to accept a
warning with a reason. Cross-references already show the pattern that works:
they are a pure function of the document and the catalog, drawn as generated
text and never stored (ADR-0011), so they cannot go stale when a symbol moves.

The same pattern fits wire numbers, reports and the new geometry diagnostics.
What cannot be derived is what a person decides: a number typed over the
generated one, a manufacturer and part number, the numbering rule, and the
reason for accepting a warning.

## Decision

- **Derived, never stored:**
  - the nets of the copper graph (wires and junctions), computed in
    `circuit-model` so that the renderer, the editor and the reports can use
    them without `circuit-sim`;
  - the number of every wire (one number per net) and its two ends;
  - every report (bill of materials, wires, terminals, cross-reference
    exceptions, I/O map, problems) as a `Table`;
  - the geometry diagnostics `OC-STR-018` and `OC-STR-019`.
- **Persisted, in schema version 3**, each omitted from the file when empty:
  - `Wire.number`: an optional override of the generated number;
  - `Device.info`: optional `manufacturer`, `part_no`, `description` and
    `location`, plain text of at most 256 bytes each;
  - `project.wire_numbering`: the rule (prefix, first number, step and
    whether numbers restart on each sheet), so that the editor, the export and
    the command line draw the same numbers;
  - `waivers`: a diagnostic code, its subject and a reason.
- **Numbering order is a contract.** Nets are ordered by their first point:
  the sheet number (points outside every sheet last), then the zone (row, then
  column), then `y`, then `x`, then the smallest ID. The same order renumbers
  designators. Moving a part can change generated numbers; an override fixes
  one.
- **Override resolution.** A net takes the override of its wire with the
  smallest ID. A second, different override in the same net is a warning
  (`OC-SEM-012`), not an error. Splitting a wire keeps its override on both
  halves.
- **Waivers are narrow.** Only warnings and informational diagnostics can be
  waived. An error never can, and a runtime electrical fault is not a
  diagnostic of the document. A waiver that names an unknown code, a
  diagnostic that cannot be waived or a subject that no longer exists is
  `OC-STR-020`. Waiving hides nothing from the compiler: the diagnostic is
  still produced and is marked as waived by whoever presents it.
- **The simulator does not change.** The static nets are a second
  implementation of the compiler's copper phase, kept honest by a test that
  compares the partitions of pins on the examples and on generated documents.
  `compile.rs` is not touched.

## Alternatives considered

- **Store every wire number.** Numbers go stale when a part moves or a wire is
  split, and a document that was edited by hand and one edited in the
  application would disagree. Storing only overrides keeps one source of
  truth.
- **Keep the numbering rule outside the document** (application setting or
  command-line flag). The same project would print different numbers in the
  window, the PDF and a report.
- **Catalog data as typed model parameters.** Parameters are declared by the
  model and typed (`ParamType` has no free text), so every library model would
  have to declare the same four fields. Catalog data belongs to the physical
  device, not to the behaviour of its model.
- **A sidecar file for waivers.** It separates the acceptance from the
  document it applies to, and the editor and the command line would have to
  find it.
- **Net labels in the compiler** (to number or join nets by name). This is the
  separate-canvas model that ADR-0011 left possible. It is not needed here.

## Consequences

- `CURRENT_SCHEMA_VERSION` becomes 3 with a migration from version 2 that only
  raises the version; every committed `.ocircuit` is regenerated, and version
  2 files keep loading. A build that knows version 2 rejects version 3 as a
  newer schema.
- A document that uses none of the new fields is identical to its version 2
  file apart from `schema_version`, and is drawn as before: wire numbers are
  drawn only when `project.wire_numbering` is set.
- New edits (`SetWireNumber`, `SetDeviceInfo`, `InsertWaiver`,
  `RemoveWaiver`) have inverses, so undo and redo keep working.
- A new core crate, `circuit-report`, derives the tables; it does no
  input or output. `circuit-export` draws a `Table` as PDF pages with the
  existing line and text primitives.
