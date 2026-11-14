# ADR-0001: Identity of devices, symbols, terminals and IDs

Status: Accepted (2026-11-14). Spec reference: `docs/spec02.md` §6.

## Context

A schematic is edited, saved, reloaded, diffed and referenced from simulation
output for years. Anything that identifies an entity must survive deletion,
renaming, moving and reordering. One physical contactor appears as a coil, a
main contact and several auxiliary contacts, which are separate drawings with
one reference designator. Terminals are wired by position on the sheet but
referred to by meaning (`a1`, `pole1.in`).

## Decision

- **Three levels.** A `Device` is the physical part (reference designator,
  model reference, parameters). A `SymbolInstance` is one drawing of a device
  (`symbol_key`, optional `slot`, position, rotation). A terminal is a fact of
  the symbol kind in the catalog, not stored per instance.
- **IDs are `u64` newtypes** (`DeviceId`, `SymbolId`, `WireId`, `JunctionId`,
  `SupplyId`) drawn from **one counter**, `next_id`, persisted in the document.
  IDs start at 1, are unique across all collections, are never reused, and
  stay at or below `MAX_ENTITY_ID` = 2^53 - 1 so that JavaScript consumers read
  them exactly. The newtypes have no `Default`: zero is not a valid ID.
  Allocation past the maximum is an error (`IdExhausted`), never a wrap.
- **Terminals are structural keys** (`a1`, `p1`, `pole1.in`, `u1`, `out`).
  Displayed numbers such as 13/14 are labels derived from the model. Wires
  refer to `PinRef { symbol_id, terminal }` or to a junction.
- **Reference designators are labels, not identity.** Two devices with one
  designator are a diagnostic (`OC-SEM-001`); several symbols sharing a
  device's designator is normal.
- Dense indices (`PinIx`, `NetIx`, `SigIx`, ...) exist only in the compiled
  circuit and are assigned after sorting by document ID.

## Alternatives considered

- **UUIDs.** Rejected: a dependency and a 128-bit key for a single-user file;
  text diffs get noisy; ordering for determinism needs an extra rule.
- **Reference designator or string tag as identity.** Rejected: renaming K1 to
  K2 would break every reference; designators legitimately repeat across
  symbols.
- **One counter per collection.** Rejected: an ID would not say what it names,
  and a wrong-collection reference could resolve silently.
- **Positional pin indices.** Rejected: reordering a catalog model's terminals
  would silently rewire saved files.

## Consequences

- Deleting an entity leaves a gap; the counter never goes back.
- A catalog change to terminal keys needs a new model version and a model
  migration (ADR-0004), never an in-place edit.
- Determinism (ADR-0002, ADR-0003) can sort by ID, which is stable.
- Loading does not check ID ranges: out-of-range, duplicate and dangling IDs
  are diagnostics (`OC-STR-011`, `OC-STR-001`, `OC-STR-002`) so that a broken
  project can still be opened and explained.

## Evidence

`crates/circuit-model/src/ids.rs` (allocator tests, `MAX_ENTITY_ID`),
`crates/circuit-model/src/validate.rs`, `FORMAT.md` (rules, terminal keys),
permutation-invariance property tests in `circuit-sim`.
