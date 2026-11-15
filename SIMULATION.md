# Simulation semantics

This document is the normative contract of `circuit-sim`. The design
rationale is in [`docs/spec02.md`](docs/spec02.md) §8–§12. When this document
and the code disagree, the code and its tests win and this document is fixed
in the same change.

## Model

The simulator works on ideal connectivity. It decides which conductors reach
which copper, which loads are energized and which faults exist. It never
computes voltage, current, impedance, heating or transients, and it never
interrupts a short circuit: a short is reported, not cleared by a fuse.

Two levels of connectivity:

| Level | Computed | Joins | Identity |
| --- | --- | --- | --- |
| Copper net | once, at compile time | pins and junctions through wires | numbered by its smallest pin |
| Group | every tick | copper nets through closed switches | its smallest net |

Two wires that only cross are not connected; only wire endpoints and explicit
junctions connect.

## Compilation

`compile(&Schematic, &ComponentCatalog) -> CompileOutput` returns
`circuit: Option<CompiledCircuit>` and every diagnostic in canonical order.
The circuit is present exactly when no diagnostic has error severity.

1. **Structural phase** (always complete): document rules (IDs, references,
   designators, junctions, supply profiles) and catalog rules (model and
   version, parameters, symbol keys, slots, instance limits, terminal keys).
   An error here stops compilation. A wire with a segment that is neither
   horizontal nor vertical is reported as the warning `OC-STR-009`
   (`NonOrthogonalWire`); geometry never blocks compilation.
2. **Pin table** in (symbol ID, terminal) order; **copper nets** by
   union-find over pins and junctions.
3. **Devices and signals** in device-ID order. Each class owns at most one
   signal: `Actuated` (pushbutton, latching switch), `Tripped` (thermal
   overload), `CoilActive` (contactor, relay), `TimerOut` (timer).
4. **Elements**: a contact is a switch between two nets driven by its
   device signal (NO closed while high, NC closed while low); a changeover
   contact is two switches from `com` on the same signal, `com`–`nc` closed
   while low and `com`–`no` closed while high, so exactly one side conducts;
   main contacts are one switch per pole sharing the signal; fuse links and thermal
   elements are always closed; supply terminals are sources; coils, lamps,
   buzzers and motors are loads.
5. **Static checks**: required terminals, devices without symbols, one supply
   per conductor, loads without supply, permanent conductor conflicts (only
   always-closed switches conduct), unused supplies, floating nets (with every
   switch closed), loads permanently in series.

The compiled circuit carries a 64-bit FNV-1a `fingerprint` of its canonical
encoding. Geometry and the project name do not affect it.

## Solver

`Solver::solve(&CompiledCircuit, &SwitchMask) -> &ElectricalSolution` is a
pure function of the switch mask; an unchanged mask reuses the previous
solution without recomputation.

Conflict table, applied to the conductors of each group:

| Conductors in the group | Result |
| --- | --- |
| none or one | normal |
| exactly `n` and `pe` | `OC-ELEC-003` warning; loads keep working |
| `pe` with any live conductor | `OC-ELEC-002` fault |
| two or more otherwise (including a phase with `n`, any AC/DC mix) | `OC-ELEC-001` fault |

A group in fault energizes nothing; the rest of the circuit is evaluated
normally.

Loads:

| Load | Energized when |
| --- | --- |
| Coil, lamp, buzzer (`dc`) | one side has `dc_pos`, the other `dc_neg` |
| Coil, lamp, buzzer (`line_neutral`) | one side has a phase, the other `n` |
| Coil, lamp, buzzer (`line_line`) | the sides have two different phases |
| Motor (`u1`, `v1`, `w1`) | three distinct groups, each with exactly one phase, together L1, L2 and L3 |

…and the voltage label matches exactly: `dc` uses the DC nominal voltage,
`line_neutral` the phase voltage of a three-phase supply or the nominal
voltage of a single-phase supply, `line_line` and motors the three-phase
nominal (line) voltage. Both conductors must come from the same supply.

- Wrong label or connection type: load off, `OC-ELEC-004` warning.
- Both terminals in one group: load off, `OC-ELEC-005` warning.
- Motor with only part of its phases: off, `OC-ELEC-006` warning.
- Motor direction: `Forward` when `(u1, v1, w1)` follows the supply phase
  sequence (any rotation of it), otherwise `Reverse`.
- Loads that are fed only through each other, between different conductors:
  off, `OC-MOD-002`. Loads that merely share a switched-off node are not a
  fault.

## Virtual time and configuration

```text
SimConfig { tick_us, delays: { pickup_ticks, dropout_ticks }, fault_policy, record_trace }
```

Defaults: `tick_us = 1000`, pickup and dropout 1 tick, `fault_policy = halt`,
trace on. A zero tick or a zero delay is refused (`OC-CFG-001`). The runtime
never reads the system clock; `t_us = tick * tick_us`.

## Tick order

Let `S` be the signals effective at the start of tick `n`.

1. **Inputs.** Every action is validated first; then `Actuated`/`Tripped`
   change *in this tick*.
2. **Switches.** `closed = { always } ∪ { driven switches whose signal has the
   closing level }`.
3. **Solve** the network.
4. **Record** what changed against the previous tick: input signals, then
   derived signals, then loads (by device ID, then symbol ID), then faults
   (canonical order), then the latch.
5. **Keep** the solved state for `snapshot()`.
6. **Logic and commit.** Coil and timer logic compute `CoilActive` and
   `TimerOut` for tick `n + 1`; time advances.

Consequences: a contact driven by a pushbutton or a thermal trip moves in the
tick of the action; a contact driven by a coil or a timer moves at least one
tick later. A coil fed through its own NC contact toggles every tick (period
of two ticks), independent of frame rate.

## Inputs

```text
Press(DeviceId) | Release(DeviceId)   pushbutton
SetClosed(DeviceId, bool)             latching switch
Trip(DeviceId) | ResetTrip(DeviceId)  thermal overload
```

`step(&[InputAction])` rejects, without changing state or consuming the tick:
an unknown device, an action that does not fit the device class, and two
actions on one device in the same tick (so trip and reset in one tick is an
error). Repeating an action that changes nothing records nothing. Inputs are
levels held by the runtime: a pushbutton stays pressed until released.

## Actuation delay

Per contactor or relay, counted in consecutive ticks of the solved coil
state: after `pickup_ticks` energized ticks `CoilActive` goes high at the
next commit; after `dropout_ticks` de-energized ticks it goes low. A run that
is interrupted restarts from zero.

## Timers

`P = max(1, ceil(preset_ms * 1000 / tick_us))`; a preset that is not a
multiple of the tick is rounded up and reported as `OC-CFG-002`. The input is
whether the timer coil is energized in the tick's solution. The input is
evaluated before the counter advances.

TON: input on → `elapsed = min(elapsed + 1, P)`, `out = elapsed >= P`; input
off → `elapsed = 0`, `out = false`.

TOF: input on → `elapsed = 0`, `out = true`; input off while out →
`elapsed = min(elapsed + 1, P)`, `out = elapsed < P`; otherwise `elapsed = 0`.

```text
TON, P = 3                         TOF, P = 3
tick        0 1 2 3 4 5 6 7 8      tick        0 1 2 3 4 5 6 7 8
input       0 0 1 1 1 1 0 0 0      input       0 0 1 1 1 0 0 0 0
elapsed     0 0 1 2 3 3 0 0 0      elapsed     0 0 0 0 0 1 2 3 0
out (eff.)  0 0 0 0 0 1 1 0 0      out (eff.)  0 0 0 1 1 1 1 1 0
```

## Thermal overload

The M1 thermal relay is a manual latch, not a thermal model. `Trip` sets
`Tripped`, `ResetTrip` clears it, both effective in the tick of the action:
the NC contact opens and the NO contact closes in that tick. Reset does not
restart anything; the operator presses START again.

## Faults and the fault policy

Only `OC-ELEC-001` (short circuit) and `OC-ELEC-002` (protective-earth fault)
latch. Conflicting supplies are a compile-time error, not a runtime fault.

- `halt` (default, fail-closed): the faulting tick completes; its snapshot
  carries the fault and `faulted = true`; the trace records the fault and
  `runtime latched`. Every later `step`/`step_n` returns `AlreadyFaulted`
  without changing state until `reset_fault()`. `reset_fault()` clears only
  the latch; a persisting cause latches again in the next tick.
- `report`: no latch; faults start and clear as transitions.

`reset()` returns to tick 0. `restore(&Checkpoint)` returns to a checkpoint,
including its latch; a checkpoint of another circuit or configuration is
refused.

## Snapshot

`snapshot()` is `None` before the first step and otherwise describes the last
*solved* tick, never the state committed for the next one. It uses document
IDs only and every list is sorted: `tick`, `t_us`, `energized_devices`,
`active_devices` (picked-up contactors/relays, timers with output high),
`actuated_devices`, `tripped_devices`, `closed_symbols` (driven contacts
closed in the solve; a changeover contact while its `no` side conducts),
`live_wires`, `live_junctions` (with their conductor
set), `motors`, `timers`, `faulted`, `faults`.

`step` allocates no snapshot. A front end calls `step_n` for the ticks of a
frame and `snapshot()` once. `step_n(n, inputs)` applies the inputs to the
first tick only and stops early at a latch; it is equivalent to `n` calls of
`step` (property-tested).

## Trace

The trace is a change log. Its text form is compared byte for byte by golden
tests:

```text
# opencircuit trace v1
# fingerprint <16 lowercase hex digits>
# tick_us=<u32> pickup_ticks=<u32> dropout_ticks=<u32> fault_policy=<halt|report>
t=<tick> input <REF>.<pressed|closed|tripped> <0|1>-><0|1>
t=<tick> signal <REF>.<active|out> <0|1>-><0|1>
t=<tick> load <REF>.<symbol key> <on|off|forward|reverse>
t=<tick> fault <OC-CODE> <started|cleared> <subject>[,<related>...]
t=<tick> runtime latched
t=<tick> runtime latch-reset
```

Fields are separated by one space and every line, the last included, ends
with a line feed. Fault subjects are reference designators for devices and
`kind#id` for other elements. Within a tick the order is the order of the
tick steps above.

The pending trace is bounded (`MAX_PENDING_TRACE_EVENTS`); when full, `step`
returns `TraceBufferFull` until `take_trace()` is called.

## Reproducibility

A run is reproduced from the circuit fingerprint, the `SimConfig`, the initial
state (tick 0, or a checkpoint) and the ordered input log. Checkpoint, restore
and replay reproduce trace and snapshot (property-tested).

## Scenarios

The CLI reads scenarios (`*.scenario.json`, schema version 1):

```json
{
  "schema_version": 1,
  "circuit": "command.ocircuit",
  "config": { "tick_us": 1000, "pickup_ticks": 1, "dropout_ticks": 1, "fault_policy": "halt" },
  "events": [ { "tick": 2, "action": "press", "device": "S1" } ],
  "until_tick": 20,
  "expect": { "trace": "command.trace", "diagnostics": [] }
}
```

`action` is `press`, `release`, `close`, `open`, `trip` or `reset_trip`;
`device` is a reference designator. `until_tick` is the last tick executed,
inclusive, at most 10 000 000. Paths are relative to the scenario file and
may not leave its directory. `expect.diagnostics` lists the codes compilation
and configuration must report, in any order. Unknown fields are rejected.
