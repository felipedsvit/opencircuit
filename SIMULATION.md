# Simulation semantics

This document describes the executable contract of `circuit-sim`. The
authoritative design rationale is in [`docs/spec02.md`](docs/spec02.md).

## Virtual time

The runtime uses virtual time and never reads the system clock. The simulator
owns a fixed tick duration (`tick_duration_us`); callers do not pass `dt` per
step. The desktop UI may advance zero, one or several ticks per frame, but it
cannot change the logical duration of a tick. The M1 default is 1 ms.

## Tick order

Each tick follows this sequence:

1. **Sample inputs.** Receive `Inputs`, compare with `inputs_previous` for pulse
   edges and apply the input signals to `current`.
2. **Solve the network.** Call `electrical::solve` with `current`.
3. **Evaluate logic.** Copy `current` to `next` and compute coils, timers,
   thermal latch, diagnostics and derived signals from the solution.
4. **Evaluate the fault policy.** Mark `faulted` if a latching runtime
   diagnostic is present. The tick still completes.
5. **Retain the tick view.** Store the effective signals, electrical solution,
   effective timers and thermals, active diagnostics and fault flag in a compact
   `TickView`. This is not a full `SimSnapshot`.
6. **Record changes.** Append signal and diagnostic changes to the change log.
7. **Commit.** `current = next`; the same swap applies to timers and thermals.
8. **Advance virtual time.** `t_us += tick_duration_us`.

The snapshot is built on demand from the last `TickView`. It must not mix the
solution with `next`. A contact controlled by a coil becomes effective on the
next tick; this is a consequence of the synchronous model, not necessarily the
real mechanical delay of a contactor.

## State discipline

- keep `current` and `next` device signals;
- keep `timers`/`next_timers` and `thermals`/`next_thermals` with the same
  discipline;
- apply logic changes to the next tick;
- associate every signal with a stable entity, origin and update policy;
- do not make buttons, coils and timer outputs share an index by accident;
- `snapshot()` returns the last completed tick. Before the first `step` it
  returns the initial state at tick 0.

## Inputs

`Inputs` is the set of active inputs for the tick. The catalog declares the
behavior:

- `Momentary`: level; the contact follows the active state.
- `Maintained`: level; the UI keeps the toggled state and reports it.
- `Pulse`: rising edge; the action occurs once.

A momentary input must remain active for at least one tick to be sampled. A UI
that runs several ticks per frame must hold the input for the whole batch.

The reference direct-online circuit uses:

```text
Q1.closed   maintained   main switch
S1.press    momentary    START (NO)
S2.press    momentary    STOP (NC)
F1.trip     pulse        manual thermal test
F1.reset    pulse        manual thermal reset
```

## Timers

### TON

- input off: output off, timing reset;
- input on: start counting;
- preset reached: output on in the next committed tick; an input change before
  the commit cancels completion.

### TOF

- input on: output on (after a tick if it was off);
- input off: start the off-delay;
- during the preset: output stays on;
- preset reached: output off in the next committed tick;
- input on again: cancel the off-delay.

The comparison is `elapsed_next >= preset_us`. The preset is rounded up in
ticks. Inputs are processed before the comparison, so an input change on the
boundary can cancel completion. Once the output is committed, an input change on
the following tick does not retroactively cancel it.

## Thermal overload

The M1 thermal relay is a latch, not a thermal model. It does not measure
current, temperature or heating time.

- `trip` on a rising edge sets `tripped = true`.
- `reset` on a rising edge clears `tripped` if it was set.
- `trip` wins over `reset` in the same tick.
- The NC contact uses the effective (`current`) thermal state; a change in tick
  `n` affects the contact in tick `n+1`.
- Reset does not restart the contactor; the operator must press START again.

## Electrical faults

Faults are simulation results, not Rust errors.

The tick that detects a short, protective-earth fault or incompatible supplies
completes normally:

1. the snapshot for that tick contains the diagnostic and `faulted = true`;
2. the runtime enters the fault latch;
3. further `step` calls return `AlreadyFaulted` until `reset_fault()`.

The following do not latch the simulation:

- `OC-ELEC-004 load-rating-mismatch`: the load is not energized;
- `OC-ELEC-005 load-terminals-shorted`: the load is not energized;
- `OC-MOD-002 unsupported-series-load`: the affected loads are not energized.

Short, protective-earth fault and incompatible supplies latch because
continuing would produce fictitious electrical states. This also makes the
reversing-circuit golden deterministic: the simultaneous START case ends at the
first short, not in an indefinite oscillation.

## Snapshot and change log

A snapshot represents one logical instant and uses only document IDs
(`WireId`, `JunctionId`, `SymbolId`, `DeviceId`) plus `ConductorMask`. It never
exposes `CopperNetIx`, `DynamicNetIx` or other internal indices. Use ordered
vectors in the output; avoid `HashMap` when serialization and comparison order
matter.

The change log records what changed:

- `(tick, signal, before, after)` for signals;
- diagnostics raised and cleared, not repeated every tick;
- timer `elapsed_us` is visible in the on-demand snapshot, not logged every tick.

The CLI and golden tests use the change log. The GUI uses `snapshot()` once per
frame or inspection and calls `step_n` for the ticks in that frame.

## Determinism and replay

A reproducible run records:

- the compiled circuit or its fingerprint;
- the tick configuration;
- the initial state;
- the ordered input event log;
- periodic checkpoints for rewind.

Recompiling the same document must produce the same fingerprint. Do not restore
a checkpoint against an incompatible circuit version or tick configuration.
`Clone` alone does not implement rewind; replay is the input log applied to a
restored checkpoint.
