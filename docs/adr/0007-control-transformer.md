# ADR-0007: Control transformer

Status: Accepted (2026-11-15). Spec reference: `docs/spec02.md` §6.7, §9,
§10, §24 item 18. Builds on [ADR-0005](0005-m3-circuit-semantics.md) (supply
sections). Normative text: [`SIMULATION.md`](../../SIMULATION.md) and
[`FORMAT.md`](../../FORMAT.md), updated by the pull request that implements
it.

## Context

Industrial panels feed their control circuit through a transformer: the
primary is connected between two phases (or a phase and neutral) of the power
section, and the secondary is a separate, galvanically isolated section at
230 V or 24 V AC. M3 introduced sections but deferred the transformer,
because a source that exists only while a load is energized would make the
solver depend on its own result within one solve.

## Decision

- **Model `transformer.control`**, new class `Transformer`, two symbols:
  - `primary`: a two-terminal load (`a1`, `a2`), with the usual
    `rated_voltage_mv` and `connection` of loads. It is energized by the
    same rules as a coil.
  - `secondary`: two terminals `x1`, `x2` that provide `l1` and `n` of a
    single-phase supply profile named by the parameter `secondary`
    (`SupplyRef`).
- **The secondary is live from the tick after the primary is energized.** A
  new derived signal, `Powered`, has one writer: the logic at commit, with
  the pickup and dropout delays of coils (`step_actuator`). While `Powered`
  is high, `x1` and `x2` are sources of their conductors in the secondary
  section; while it is low they are not. The solver therefore stays a pure
  function of the switch mask and of the set of enabled sources, and an
  unchanged pair of masks reuses the previous solution.
- **A secondary profile belongs to one transformer.** The profile named by
  `secondary` must be single-phase, may be named by one transformer only and
  may not be tapped by any `supply.terminal`; otherwise compilation reports
  the new error `OC-SEM-010` (`InvalidTransformer`). This keeps every section
  with one origin, as in ADR-0005.
- **No feedback within a tick.** A transformer whose primary is fed from its
  own secondary never starts (the secondary needs `Powered`, which needs the
  primary energized); this is the result of the model, not an error.
- **Trace and snapshot.** `Powered` is traced like `active`
  (`t=<tick> signal T1.powered 0->1`), and the transformer is listed in
  `active_devices` while it is powered.
- **Fingerprint.** The enabling signal of a source is encoded only when a
  circuit has conditional sources, so every existing golden keeps its
  fingerprint.

## Alternatives considered

- **Secondary live in the same tick** (two passes of the solver). Rejected:
  a load would feed a source within one solve, cycles become possible and
  the solver would no longer be a pure function of the switch mask.
- **Model the transformer as a coil and supply terminals wired to a
  contact.** Rejected: it hides the galvanic separation and lets users wire
  the secondary to anything.

## Consequences

- Control circuits fed through a transformer simulate with the same tick
  discipline as coils; the trace shows the one-tick delay explicitly.
- `SignalKind` and `DeviceClass` gain one variant each; `OC-SEM-010` is added
  to the code table.
