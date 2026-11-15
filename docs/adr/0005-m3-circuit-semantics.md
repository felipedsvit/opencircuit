# ADR-0005: Supply sections, changeover contacts, mechanical interlock and motor windings

Status: Accepted (2026-11-15). Spec reference: `docs/spec02.md` §6.7, §7,
§8.3, §9.3–§9.5, §10, §17.2, §20 (M3). Supersedes the "one supply profile per
conductor" rule of [ADR-0003](0003-conductor-model.md); the rest of ADR-0003
stands. Normative text: [`SIMULATION.md`](../../SIMULATION.md) and
[`FORMAT.md`](../../FORMAT.md), updated by the pull request that implements
each part.

## Context

M3 must validate a star-delta starter "with an adequate winding model and no
improper contactor overlap" and support "multiple galvanically separate
supply sections", changeover contacts and full interlocking. Each of these
touches a contract that is hard to reverse: the identity of a conductor, the
catalog versions stored in files, the commit rule of the logic and the load
rules of the solver. They are fixed here before code is written.

## Decision

### 1. Supply sections

- **A section is a supply profile.** A conductor present in a group is
  identified by `(SupplyId, Conductor)`, not by `Conductor` alone. Several
  supplies may provide the same conductor (two `l1`, two `n`); compilation no
  longer emits `OC-SEM-004` (`ConflictingSupplies`). The code stays reserved
  and is never reused.
- **Bridging sections is a short.** The conflict table of ADR-0003 is applied
  to section-qualified conductors: two or more live conductors in one group
  that come from different sections are `OC-ELEC-001`, which latches under
  `FaultPolicy::Halt`. This holds for every live conductor, `n` included.
- **`pe` is common to every section.** Protective earth is one conductor,
  whichever supply terminal provides it; it never makes two sections conflict
  by itself. `pe` with a live conductor of any section stays `OC-ELEC-002`,
  and exactly `n` and `pe` stays the `OC-ELEC-003` warning.
- **The static check follows the same rule.** A copper net that joins live
  conductors of two sections with every switch open is a compile-time error,
  as any permanent conflict is today.
- **Loads read the section of their groups.** A group that is not in fault
  carries at most one section, so voltage labels and phase sequence come from
  that section; conductors from two sections across one load leave it off
  with `OC-ELEC-004`, as conductors from two supplies do today.
- **What the solver carries.** Per group: the `ConductorSet` (unchanged, used
  by the snapshot and the renderer for colors), the first live section seen
  and a "mixed" flag. The snapshot keeps `ConductorSet`; no schema change is
  needed because `supplies` is already a list.
- **Control transformer: deferred to M4.** Sections are ideal, independent
  sources. A transformer would make a source depend on a load in the same
  network, which would break `solve(circuit, mask)` as a pure function of the
  switch mask.

### 2. Changeover contacts and catalog versions

- **New symbol kind `changeover`** with structural terminals `com`, `nc`,
  `no`; the printed numbers (11/12/14, 15/16/18) are labels. It compiles into
  two switches that share the device signal: `com`–`no` closed while the
  signal is high, `com`–`nc` closed while it is low.
- **Published models are immutable.** The changeover symbol is added in new
  versions: `relay.generic` v2 (`changeover`, 0..8, slotted) and
  `timer_on.generic`/`timer_off.generic` v2 (`delayed_co`, 0..4, slotted).
  Version 1 of each model stays in the catalog, so existing files load,
  compile and save byte for byte.
- **New devices use the latest version.** The catalog exposes the latest
  version of each model for the palette.
- **Upgrading is an explicit edit.** A registered model migration maps
  version 1 to version 2 (symbol keys unchanged) and is tested. It is applied
  only by an edit the user asks for, never silently on load: a silent upgrade
  would change the bytes of a file that was only opened.
- **Snapshot.** A changeover symbol is listed in `closed_symbols` while its
  `no` side conducts.

### 3. Full interlocking

- **Combined pushbutton** `pushbutton.combined`: one device, one `Actuated`
  signal, a `no` and an `nc` contact symbol.
- **Mechanical interlock** `interlock.mechanical`: a device of the new class
  `MechanicalInterlock` whose parameters `first` and `second` are a new
  parameter type, `DeviceRef` (a device ID, stored as a JSON integer; no
  schema change). It has one symbol, `link`, without terminals, so it can be
  selected and deleted on the canvas. Both references must name existing,
  distinct contactors, and a contactor belongs to at most one interlock;
  otherwise compilation reports the new error `OC-SEM-009`
  (`InvalidInterlock`). Deleting a contactor in the editor deletes the
  interlocks that name it.
- **Commit rule (break before make).** The actuation counters run as before.
  At the commit of tick `n`, a contactor of an interlocked pair whose
  `CoilActive` would go high goes high only if its partner is low in tick `n`
  and stays low at this commit. When both would go high at the same commit,
  neither does, and the runtime reports the non-latching warning `OC-ELEC-007`
  (`InterlockContention`) in that tick. A blocked contactor keeps counting its
  energized ticks, so it picks up at the first commit where the rule allows
  it. Consequently the two contactors of a pair are never active in the same
  tick, and there is at least one tick with both inactive between them.

### 4. Six-terminal motor and windings

- **New model `motor.3ph.6t`** (a new ID, because its terminals differ from
  `motor.3ph`): symbol kind with terminals `u1`, `v1`, `w1`, `u2`, `v2`,
  `w2`; three windings `u1`–`u2`, `v1`–`v2`, `w1`–`w2`.
- **One load, one rule.** The motor is a single load evaluated on the six
  groups, never three separate winding loads; a floating star point inside
  the motor is part of the rule, not a series-load topology (`OC-MOD-002` does
  not apply to it). No virtual potential is propagated.
- **Rule**, in order, on the groups `g(t)` of the six terminals:
  1. any group in fault: off, no diagnostic;
  2. every terminal dead: off;
  3. a terminal on a conductor that is not a phase: off, `OC-ELEC-004`;
  4. **star**: `g(u2) = g(v2) = g(w2)`, that group carries no conductor and
     differs from `g(u1)`, `g(v1)`, `g(w1)`, which are three groups with one
     distinct phase each, from one section;
  5. **delta**: `g(u1) = g(w2)`, `g(v1) = g(u2)`, `g(w1) = g(v2)`, three
     distinct groups with one distinct phase each, from one section;
  6. a star or delta pattern with only some phases present: off,
     `OC-ELEC-006`;
  7. every winding open (no star point, no delta ring), as in the gap between
     star and delta: off, no diagnostic;
  8. any other energized connection: off, new error `OC-MOD-004`
     (`UnsupportedWindingConnection`), which does not latch.
- **Direction** follows the phases on `(u1, v1, w1)` against the phase
  sequence of the section, as for `motor.3ph`. **Voltage label:**
  `rated_voltage_mv` is the line voltage of the motor in delta and is
  compared with the line voltage of the section, in star and in delta.
- **Observable state.** `MotorView` gains `winding: Option<star | delta>`;
  `MotorState` and the renderer's `StyleKey::Motor` do not change. The trace
  writes `forward-star`, `forward-delta`, `reverse-star` or `reverse-delta`
  for six-terminal motors; three-terminal motors keep `forward`/`reverse`, so
  existing goldens do not change.
- **Star-delta overlap is detected, not prevented by the model.** With the
  star and delta contactors closed together the three phases meet at the star
  point and the group is an `OC-ELEC-001` short. The interlocked star-delta
  example must never reach that state; a property test drives it with random
  operator inputs and delays to prove it.

## Alternatives considered

- **Keep one profile per conductor and add new conductor names per section.**
  Rejected: the conductor set would grow without bound and lose the meaning of
  `l1`/`n` across sections.
- **A separate fault code for bridged sections.** Rejected: physically it is
  a short between two systems; one code keeps the latch rule and the UI
  simple.
- **Lower `DeviceId` wins a simultaneous interlock contention.** Rejected: the
  result would change when the user recreates devices in another order.
- **Windings as three two-terminal loads.** Rejected: the star point would be
  a series topology the discrete model refuses, and delta would depend on load
  ordering.
- **Silent model upgrade on load.** Rejected: opening a file would change it.
- **Control transformer in M3.** Deferred: it needs conditional sources and a
  new tick contract.

## Consequences

- Industrial panels with a 400 V power section, a separate 230 V section and a
  24 V DC control section compile and simulate; joining them is a latched
  short.
- Star-delta runs with a stated winding model; the trace shows star, the open
  gap and delta.
- Three new codes (`OC-SEM-009`, `OC-ELEC-007`, `OC-MOD-004`) are added to the
  tested code table; `OC-SEM-004` is retired.
- Files written before M3 load unchanged; new devices use the new model
  versions.
