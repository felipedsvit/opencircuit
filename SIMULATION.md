# Simulation semantics

## Virtual time

The runtime uses virtual time and does not read the system clock. The simulator
owns a fixed tick duration (`tick_duration_us`); callers do not pass `dt` per
step. The desktop UI may advance zero, one or several ticks, but it cannot change
the logical duration of a tick.

For the MVP the recommended tick is 1 ms, subject to performance and semantics
validation.

## Tick order

Each tick follows a defined sequence:

1. Capture and apply external inputs for the current time boundary.
2. Solve the network using the effective current contact states.
3. Evaluate coils, timers and devices from that solution.
4. Build a snapshot for the network that was actually solved.
5. Confirm the next device states for the following tick.
6. Advance virtual time.

The snapshot must not mix the solved state with future signals. A contact
controlled by a coil becomes effective on the next tick; this is a consequence
of the synchronous model, not necessarily the real mechanical delay of a
contactor.

## State discipline

- keep `current` and `next` device signals;
- apply logic changes to the next tick;
- associate every signal with a stable entity, origin and update policy;
- do not make buttons, coils and timer outputs share an index by accident.

## Timers

### TON

- input off: output off, timing reset;
- input on: start counting;
- preset reached: output on;
- input off: output off and reset.

### TOF

- input on: output on immediately;
- input off: start the off-delay;
- during the preset: output stays on;
- preset reached: output off;
- input on again: cancel the off-delay.

The exact behavior at the preset boundary, including an input change on the same
boundary, must be covered by tests.

## Snapshot

A snapshot represents one logical instant. Use ordered vectors or ordered maps in
the output; avoid `HashMap` when serialization and comparison order matter. The
snapshot may be consumed by the editor and renderer without exposing simulator
internals.

## Determinism and replay

A reproducible run records:

- the compiled circuit or its fingerprint;
- the tick configuration;
- the initial state;
- the ordered input event log;
- periodic checkpoints for rewind.

Do not restore a checkpoint against an incompatible circuit version.
