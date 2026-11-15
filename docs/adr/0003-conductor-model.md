# ADR-0003: Conductor model, conflict rules and solver limits

Status: Accepted (2026-11-14); the rule "one supply profile per conductor" is
superseded by [ADR-0005](0005-m3-circuit-semantics.md) (supply sections).
Spec reference: `docs/spec02.md` §8, §9.
Normative text: [`SIMULATION.md`](../../SIMULATION.md).

## Context

The product answers "is this load energized, and is anything shorted", not
"what are the voltages". Industrial control circuits mix DC control, single
and three-phase power, neutral and protective earth. The model must catch a
phase wired to neutral or to earth, and must not pretend to know physics it
does not compute.

## Decision

- **Conductors are a bit set.** `Conductor` is one of `l1`, `l2`, `l3`, `n`,
  `pe`, `dc_pos`, `dc_neg`; `ConductorSet` is one byte. There is no
  `Potential` enum of "L1/L2/L3/N/PE" and no propagated voltage.
- **One supply profile per conductor.** A supply terminal sources one
  conductor of one `SupplyProfile`. Two supplies providing the same conductor
  are a compile-time error (`OC-SEM-004`), not a runtime fault.
- **Two-level connectivity.** Copper nets (pins and junctions joined by wires)
  are computed once at compile time by union-find. Each tick, closed switches
  join nets into groups. Crossing wires are not connected; a junction is an
  explicit entity.
- **Conflict table per group.** None or one conductor: normal. Exactly `n` and
  `pe`: `OC-ELEC-003` warning, loads keep working. `pe` with a live conductor:
  `OC-ELEC-002`. Any other two or more, including any AC/DC mix and a phase
  with `n`: `OC-ELEC-001`. A group in fault energizes nothing.
- **Loads are checked by connection type and by voltage label** against the
  supply profile; a mismatch leaves the load off and warns (`OC-ELEC-004`).
  Voltage labels are compared, never computed with.
- **Limits are explicit.** Loads fed only through each other between different
  conductors are reported (`OC-MOD-002`), not guessed. No currents, no
  impedance, no fuse clearing a short, no heating.

## Alternatives considered

- **A `Potential` enum or virtual-potential propagation.** Rejected: it
  invites partial voltage semantics the solver cannot honour.
- **MNA / SPICE-style solver.** Rejected for the MVP: needs component values,
  introduces convergence failure modes, and is not needed for the questions
  the product answers. It can be added behind the same snapshot types.
- **Treating `n`-`pe` as a short.** Rejected: the bond is normal at the
  installation origin; a warning is enough.
- **Letting the nearest supply win on a conductor.** Rejected: silent.

## Consequences

- Short circuits, phase-to-earth and AC/DC mixes are detected exactly and
  deterministically from topology.
- Anything that needs magnitudes (inrush, protection coordination, voltage
  drop) is out of scope; `SIMULATION.md` states that the simulator never
  computes voltage, current, impedance, heating or transients.
- A future numeric solver replaces the solver module, not the model.

## Evidence

`crates/circuit-model/src/supply.rs`, `crates/circuit-sim/src/solve.rs`,
`solve_tests.rs` (conflict table, load rules), the fixtures `static-short` and
`series-loads`.
