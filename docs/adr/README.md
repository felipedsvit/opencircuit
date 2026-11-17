# Architecture decision records

An ADR records a decision that is hard to reverse, with the alternatives that
were rejected, so that nobody has to re-derive it from the code. The
contracts themselves are in [`SIMULATION.md`](../../SIMULATION.md) and
[`FORMAT.md`](../../FORMAT.md); the code and its tests win over any ADR text
that disagrees, and the ADR is fixed in the same change.

Each ADR has a status (`Accepted`, `Superseded by ADR-NNN`), context,
decision, alternatives considered and consequences. A decision is changed by a
new ADR that supersedes the old one, not by rewriting history.

| ADR | Decision |
| --- | --- |
| [0001](0001-identity-and-ids.md) | Device, symbol and terminal identity; `u64` IDs from one persisted counter |
| [0002](0002-tick-and-commit.md) | Tick order, minimum actuation delay, change-log trace, checkpoint and replay |
| [0003](0003-conductor-model.md) | Conductor bit set, one profile per conductor (superseded by 0005), conflict table, two-level connectivity |
| [0004](0004-canonical-json-format.md) | Canonical JSON without floating point, two-phase reading, version policy |
| [0005](0005-m3-circuit-semantics.md) | Supply sections, changeover contacts and model versions, mechanical interlock, six-terminal motor windings |
| [0006](0006-pdf-export.md) | PDF export with `pdf-writer`: one page sized to the drawing, deterministic bytes; text font amended by 0012 |
| [0007](0007-control-transformer.md) | Control transformer: secondary live from the tick after the primary is energized |
| [0008](0008-extension-libraries.md) | Extension libraries: data only, namespaced models, own artwork, no code |
| [0009](0009-industrial-io.md) | Industrial I/O: Modbus TCP server, codec in the core, socket in the CLI, safety limits |
| [0010](0010-opcua-mqtt-evaluation.md) | OPC UA and MQTT evaluated: OPC UA not implemented (MPL-2.0 only), MQTT feasible and deferred |
| [0011](0011-sheets-as-frames.md) | Sheets as frames over the single canvas: paper size, zones and title block; simulator unchanged |
| [0012](0012-pdf-font.md) | Embedded Liberation Mono subset in the PDF export, with a small font reader instead of an unmaintained crate |
| [0013](0013-symbolic-devices.md) | Symbol-only devices: class `symbolic` and block kinds, no behaviour in the simulation, informational `OC-SEM-011` |
| [0014](0014-derived-documentation.md) | Derived documentation (nets, wire numbers, reports, geometry diagnostics) with only overrides, device catalog data, the numbering rule and waivers stored; schema version 3 |
| [0015](0015-text-and-layers.md) | Text notes and layers (schema 4) |
| [0016](0016-names-and-terminals.md) | A name parameter, terminal blocks that always conduct and the terminal plan; names that join nets (signal arrows) |
| [0017](0017-window-language.md) | The language of the window: English source, Brazilian Portuguese catalog, chosen from the environment and the Help menu |
