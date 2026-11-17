# ADR-0013: Symbol-only devices

Status: Accepted (2026-11-17). Tracking issue #73 (M6). Builds on
[ADR-0008](0008-extension-libraries.md) (libraries reuse the closed
behaviour of the core). Normative text: [`SIMULATION.md`](../../SIMULATION.md),
[`FORMAT.md`](../../FORMAT.md) and [`LIBRARY.md`](../../LIBRARY.md), updated
by the pull request that implements it.

## Context

An engineer drawing a control panel needs parts that the discrete model
cannot simulate: a diode, a rectifier, a PLC block, a pneumatic cylinder, a
residual-current device. The simulator knows continuity and energized state
only (no direction, no analog quantities, no controller program, no fluid).
Until now a library could not offer such a part honestly: every class has
behaviour, so a library author had to pick a class that simulates something
else. A diode drawn as a fuse link would conduct both ways and could latch a
protective-earth fault; a PLC drawn as a lamp would invent a load.

## Decision

- **A new class, `symbolic`** (`DeviceClass::Symbolic`), whose devices are
  drawn and wired but have no behaviour. It has no parameters, no operator
  and no signal.
- **Three new symbol kinds with no behaviour**: `block2` (`t1`, `t2`),
  `block4` (`t1` to `t4`) and `block8` (`t1` to `t8`), laid out in two rows
  of terminals (FORMAT.md). Only the class `symbolic` accepts them, and
  `symbolic` accepts only them, so a block can never be mistaken for a part
  that conducts.
- **The compiler adds nothing for them**: no switch, no source, no load. Their
  terminals take part in the copper nets only through the wires drawn to
  them. A symbol-only device never conducts, so it can never be part of a
  short circuit or a protective-earth fault, and it never latches.
- **Compilation reports each such device as information**:
  `OC-SEM-011` (`NotSimulated`, severity info, subject the device). It blocks
  nothing and the circuit still compiles and runs.
- **Not exposed**: no operator controls, no Modbus entries, no cross-reference
  as a driven contact. The designator prefix is `A`.
- **Fingerprint**: the class has its own code (15), so a circuit with such a
  device never shares a fingerprint with one without. Every existing circuit
  keeps its fingerprint.
- **Libraries** gain the class `symbolic` and the kinds `block2`, `block4`
  and `block8`; their artwork is checked like any other (inside the bounds,
  reaching every terminal). The library schema version does not change: the
  new values are additions to enumerations that v2 already has.

## Alternatives considered

- **Draw such parts with an existing class** (a diode as a fuse link).
  Rejected: it simulates something the part does not do and can produce
  faults that do not exist.
- **A free-form drawing entity in the document** (shapes and text on the
  sheet). Rejected for now: it needs a schema change and has no terminals, so
  it cannot be wired. Ladder and GRAFCET as diagrams need it and are outside
  this decision.
- **No catalog entry for what cannot be simulated.** Rejected by the user for
  M6: the catalog should offer the part, marked as not simulated, rather than
  hide it.

## Consequences

- The user interface marks symbol-only models ("not simulated") in the
  palette and in the inspector, and the diagnostic is listed with the others.
- Spec reference: the code table of `docs/spec02.md` gains `OC-SEM-011`.
- Simulating any of these parts later means a new class or a new version of
  the model; the symbol-only model stays valid for existing documents.
