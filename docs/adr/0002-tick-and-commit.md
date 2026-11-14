# ADR-0002: Tick order, actuation delay, trace, checkpoint and replay

Status: Accepted (2026-11-14). Spec reference: `docs/spec02.md` §10, §12, §17.3.
Normative text: [`SIMULATION.md`](../../SIMULATION.md).

## Context

Control circuits are feedback circuits: a coil opens its own contact, a seal-in
contact holds a coil. A simulator that is not explicit about when an input and
a coil effect become visible produces results that depend on evaluation order,
on frame rate, or on both. Golden traces must be byte-identical on every
platform.

## Decision

- **Fixed virtual time.** `SimConfig::tick_us` is set once; `step` takes no
  `dt`. The runtime never reads the system clock; `t_us = tick * tick_us`.
- **Tick order.** Inputs take effect in the tick of the action; the network is
  solved; changes are recorded; the solved state is kept for `snapshot()`;
  then coil and timer logic compute the signals of tick `n + 1` and time
  advances.
- **Minimum actuation delay of one tick** (`pickup_ticks`, `dropout_ticks`
  >= 1; zero is refused with `OC-CFG-001`). A contact driven by a coil or a
  timer therefore moves at least one tick after the cause; a coil fed through
  its own NC contact toggles with a period of two ticks, at any frame rate.
- **Snapshot is the solved tick**, never the state committed for the next
  one; a front end calls `step_n` for a frame and `snapshot()` once.
- **Trace is a change log**, one line per change, with a fixed text form
  (`# opencircuit trace v1`, header with the circuit fingerprint and the
  config). Goldens compare it byte for byte.
- **Reproducibility.** A run is determined by the circuit fingerprint, the
  `SimConfig`, the initial state and the ordered input log. `checkpoint()` and
  `restore()` capture and return the full runtime state, including the fault
  latch; `step_n(n)` equals `n` calls of `step`.
- **Fail closed.** Under `FaultPolicy::Halt` (default) a short circuit or a
  protective-earth fault completes its tick, then latches until `reset_fault()`.

## Alternatives considered

- **Per-call `dt` driven by the UI frame time.** Rejected: results would
  depend on frame rate; goldens would be impossible.
- **Zero-delay evaluation to a fixed point.** Rejected: a self-interrupting
  coil has no fixed point; the loop needs a cut-off that is itself arbitrary.
- **Inputs applied at the next tick boundary.** Rejected: a pressed button
  would act one tick late, and the trace would show an input with no effect.
- **Snapshot after the commit.** Rejected: the snapshot would show coil
  signals that the solved network has not yet seen.
- **Dumping full state per tick as the trace.** Rejected: large, and hides the
  change that matters when a golden fails.

## Consequences

- Every device effect has a documented tick; tests assert tick numbers.
- The recorded trace is small and diffable; the header ties it to a circuit.
- Checkpoint and restore ship in M1 although the plan had deferred them,
  because replay equivalence is the property that makes the goldens
  trustworthy; they are covered by property tests.
- Real-time pacing is the application's job: it chooses how many ticks to run
  per frame.

## Evidence

`crates/circuit-sim/src/runtime.rs`, `runtime_tests.rs` (tick tables, the
§23 traces, checkpoint/replay properties), `trace.rs`, goldens under
`examples/` and `tests/fixtures/`.
