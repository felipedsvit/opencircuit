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
   overload), `CoilActive` (contactor, relay), `TimerOut` (timer), `Powered`
   (control transformer).
4. **Elements**: a contact is a switch between two nets driven by its
   device signal (NO closed while high, NC closed while low); a changeover
   contact is two switches from `com` on the same signal, `com`–`nc` closed
   while low and `com`–`no` closed while high, so exactly one side conducts;
   main contacts are one switch per pole sharing the signal; fuse links and thermal
   elements are always closed; supply terminals are sources; the secondary
   of a control transformer is a pair of sources (`x1` provides `l1`, `x2`
   provides `n` of its single-phase section) enabled by its `Powered`
   signal; coils, transformer primaries, lamps, buzzers and motors are
   loads. The secondary profile of a transformer must be single-phase,
   named by that transformer only and tapped by no supply terminal
   (`OC-SEM-010`). A symbol-only device (class `symbolic`, kinds `block2`,
   `block4`, `block8`, ADR-0013) adds no switch, source or load: its
   terminals join nets only through the wires drawn to them, so it never
   conducts, and compilation reports it as information (`OC-SEM-011`).
5. **Static checks**: required terminals, devices without symbols, loads
   without supply, permanent conductor conflicts (only always-closed switches
   conduct, the table below applied with sections), unused supplies, floating nets (with every
   switch closed), loads permanently in series.

The compiled circuit carries a 64-bit FNV-1a `fingerprint` of its canonical
encoding. Geometry and the project name do not affect it.

## Solver

`Solver::solve(&CompiledCircuit, &SwitchMask) -> &ElectricalSolution` is a
pure function of the switch mask; an unchanged mask reuses the previous
solution without recomputation.

Every supply profile is a galvanically separate **section**: a conductor in
a group is identified by its supply and its conductor, so two supplies may
both provide `l1` or `n`. Protective earth `pe` is common to every section.
Conflict table, applied to the conductors of each group:

| Conductors in the group | Result |
| --- | --- |
| live conductors of two or more sections (`pe` aside) | `OC-ELEC-001` fault |
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
| Six-terminal motor | its windings are connected in star or delta, see below |

…and the voltage label matches exactly: `dc` uses the DC nominal voltage,
`line_neutral` the phase voltage of a three-phase supply or the nominal
voltage of a single-phase supply, `line_line` and motors the three-phase
nominal (line) voltage. Both conductors must come from the same supply
section; conductors of two sections across one load leave it off with
`OC-ELEC-004`. `OC-SEM-004` (one conductor provided by two supplies) is no
longer emitted since sections exist; the code stays reserved.

- Wrong label or connection type: load off, `OC-ELEC-004` warning.
- Both terminals in one group: load off, `OC-ELEC-005` warning.
- Motor with only part of its phases: off, `OC-ELEC-006` warning.
- Motor direction: `Forward` when `(u1, v1, w1)` follows the supply phase
  sequence (any rotation of it), otherwise `Reverse`.
- Six-terminal motor (`u1`–`u2`, `v1`–`v2`, `w1`–`w2`), one load evaluated
  on its six groups, in order: a group in fault, or every terminal dead:
  off; a terminal on a conductor that is not a phase: off, `OC-ELEC-004`;
  **star** when `u2`, `v2`, `w2` share one group that carries no conductor
  and is none of the groups of `u1`, `v1`, `w1`; **delta** when `u1` shares
  the group of `w2`, `v1` that of `u2` and `w1` that of `v2`, in three
  distinct groups; in star or delta the motor runs by the motor rule above
  on `(u1, v1, w1)` (phase loss and label mismatch included); otherwise, with
  `u2`, `v2`, `w2` dead and apart (open windings, as between star and
  delta): off, no diagnostic; any other connection: off, `OC-MOD-004`
  (`UnsupportedWindingConnection`), which does not latch. `rated_voltage_mv`
  is the line voltage of the motor in delta, compared with the line voltage
  of the supply in both connections. The star point inside the motor is not
  a series topology, and a copper net made only of second ends of
  six-terminal motors is not a floating net.
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
6. **Logic and commit.** Coil, transformer and timer logic compute
   `CoilActive`, `Powered` and
   `TimerOut` for tick `n + 1`, mechanical interlocks correct them; time
   advances. The logic may be computed before step 4, since it only reads the
   solution of this tick; an interlock contention it finds is recorded with
   the faults of this tick.

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

## Control transformer

A `transformer.control` (ADR-0007) has a primary, a two-terminal load
energized like a coil, and a secondary that provides a separate single-phase
section. `Powered` follows the primary with the actuation delays of coils:
after `pickup_ticks` energized ticks it goes high at the next commit, after
`dropout_ticks` de-energized ticks it goes low. While `Powered` is high in a
tick, `x1` and `x2` provide `l1` and `n` of the secondary section in that
tick's solve; otherwise they provide nothing. The secondary is therefore live
from the tick after the primary is energized, and dies the tick after the
primary loses power. A transformer is listed in `active_devices` while
powered. A transformer fed from its own secondary never starts. The solver
remains a pure function of the closed switches and the enabled sources.

## Mechanical interlock

An `interlock.mechanical` device pairs two contactors (`first`, `second`);
compilation refuses a pair that does not name two distinct contactors, or a
contactor named by two interlocks (`OC-SEM-009`). At each commit, break
before make:

- a contactor whose `CoilActive` would rise rises only if its partner is low
  in this tick and stays low at this commit;
- when both would rise at the same commit, neither does, and the tick
  reports the warning `OC-ELEC-007` (`InterlockContention`, subject the first
  contactor, related the second), which does not latch;
- a blocked contactor keeps counting energized ticks, so it picks up at the
  first commit the rule allows.

The two contactors of a pair are never active in the same tick, and at least
one tick separates the drop-out of one from the pick-up of the other. The
logic runs before the tick is recorded so that the contention appears in the
trace and the snapshot of the tick that computed it; nothing it computes is
effective before the next tick.

A `pushbutton.combined` is one button with a normally open and a normally
closed contact on its single `Actuated` signal.

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
set), `motors` (with `winding`, `star` or `delta`, for a running six-terminal
motor), `timers`, `faulted`, `faults`.

Two fields differ in what they describe. `faults` lists the faults of the
solved tick. `faulted` is the fault latch at the moment `snapshot()` is
called, the same value as `is_faulted()`. They agree after a `halt` fault,
where the tick that latched shows `faulted = true` and its fault. They differ
under `report` (faults, no latch) and right after `reset_fault()`, which
clears the latch without rewriting the solved tick: `faulted = false` with
the faults of that tick, until the next tick replaces them. A front end that
needs to know whether it may run reads `faulted`; one that lists diagnostics
reads `faults`.

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
t=<tick> signal <REF>.<active|out|powered> <0|1>-><0|1>
t=<tick> load <REF>.<symbol key> <on|off|forward|reverse|forward-star|forward-delta|reverse-star|reverse-delta>
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

The CLI reads scenarios (`*.scenario.json`, schema version 1 or 2):

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

Version 2 adds `libraries`: extension library files (`*.oclib`,
[`LIBRARY.md`](LIBRARY.md)) the project needs, relative to the scenario file,
added to the built-in catalog in order; version 1 is still read and has no
libraries.
`action` is `press`, `release`, `close`, `open`, `trip` or `reset_trip`;
`device` is a reference designator. `until_tick` is the last tick executed,
inclusive, at most 10 000 000. Paths are relative to the scenario file and
may not leave its directory. `expect.diagnostics` lists the codes compilation
and configuration must report, in any order. Unknown fields are rejected.
