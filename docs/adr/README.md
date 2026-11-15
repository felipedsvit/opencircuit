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
