# ADR-0016: Names that join nets, and terminals that always conduct

Status: Accepted (2026-11-17). Extends [ADR-0011](0011-sheets-as-frames.md)
(sheets as frames over one canvas) and builds on
[ADR-0013](0013-symbolic-devices.md) (devices with no behaviour of their own).
Tracking issues: #105 (terminal blocks), #106 (signal arrows), #107 (M8).

## Context

Two things of a panel drawing are not in the model. A **terminal strip** is a
numbered row of screw terminals that a cable ends at and a wire leaves from; the
list of what is on each side of each terminal is a deliverable of its own (the
terminal plan). A **signal arrow** continues a circuit from one place to another,
on the same sheet or on another one, without a wire across the page: ADR-0011
left that "considered and not done" because the sheet model has one canvas and
joins nets only by wires.

Both need a short text that identifies something (the strip `X1`, the signal
`START`), and the catalog has no parameter of that kind: a text parameter is
only a choice from a list.

## Decision

- **A name parameter.** `ParamType::Name` is a text of 1 to 32 characters of
  letters, digits, `_`, `-` and `.`. A library may use it from library schema
  version 3; older files may not, and keep loading.
- **A terminal block** is a device of the class `TerminalBlock` with the symbol
  kind `TerminalPoint`: two terminals, `p1` and `p2`, that always conduct, as a
  fuse does. It has two parameters, `strip` (a name) and `number` (a whole
  number from 1). The compiler joins its two terminals for good (a switch driven
  `Always`); the static nets of `circuit-model` do not, as they do not for a
  fuse, which is why the terminal plan can list the two sides apart.
- **The terminal plan** is derived, never stored: one row for every terminal,
  ordered by strip (naturally, so `X2` comes before `X10`) and number, with the
  wire number and the terminals on each side. It is the report `strips`, and its
  CSV is the table a manufacturer's import wants.
- **A strip and a number are given to one device.** Two devices with the same
  strip and number are the warning `OC-SEM-013`; it is a warning because a
  drawing in progress has them.
- Signal arrows are decided in the same ADR when they are implemented (see the
  section below).

## Alternatives considered

- **A terminal as a plain wire junction.** It would not appear in the bill of
  materials or the plan, and a junction has no number.
- **The terminal number as part of the designator** (`X1:3`). The designator
  pattern allows no colon and a device has one designator; two parameters keep the
  strip and the number apart, so both can be sorted and checked.
- **Free text parameters.** They would let a library put anything, of any size,
  in a parameter; a name has a small, checked alphabet and size.

## Consequences

- `DeviceClass` and `SymbolKind` gain a variant each, which the compiler, the
  renderer, the designator prefixes (`XT`) and the library reader handle; the
  fingerprint of a compiled circuit has a code for the class (16), and no existing
  circuit changes its fingerprint.
- The library schema is 3. The standard library has a part for it
  (`std.terminal_block`, in the Lines tab).
- The report `strips` joins the others, and `--kind all` includes it.
