# Security policy

## Reporting a vulnerability

Do not open a public issue for a suspected vulnerability. Report it privately to
the maintainers with:

- a description of the issue;
- affected version or commit;
- reproduction steps or a proof of concept;
- potential impact and suggested mitigation, if known.

Maintainers will acknowledge the report, assess it and coordinate disclosure.

## Scope

OpenCircuit is a schematic editor and preliminary simulator, with a command
line, a desktop application, a browser build and a Modbus TCP server for
testing. Security issues include, but are not limited to:

- malformed project (`.ocircuit`), library (`.oclib`) or scenario files
  causing a crash, unbounded memory or time, or unsafe behavior;
- a Modbus TCP frame or connection pattern that crashes or stalls
  `opencircuit serve`, bypasses its limits, or makes it reachable from
  another machine without `--listen`;
- a library that causes anything to be executed, or reads or writes a file
  it does not name;
- path traversal or unsafe file handling in the application layer;
- dependency vulnerabilities.

A crash on any input is treated as a bug to fix with a regression test
(`AGENTS.md`, "Never crash").

## Limits enforced by design

These are part of the contract; weakening one is a security change and
needs its own pull request. Details are in
[`EXTENDING.md`](EXTENDING.md#safety-limits).

- Libraries are data: nothing loaded at run time is executed.
- Every reader of external input is strict and bounded (file size, number
  of entities and nesting depth) and returns a structured error.
- `opencircuit serve` is a Modbus server only, never a client. It listens on
  `127.0.0.1` unless `--listen` names another address, which prints a
  warning; it accepts at most 4 clients, frames up to 260 bytes, and closes
  idle connections after 30 s. Modbus has no authentication or encryption:
  anyone who reaches the port can operate the simulated circuit.
- The core crates have no filesystem, socket or clock access
  (`cargo run -p xtask -- check-architecture`).

## Safety boundary

OpenCircuit does not certify electrical installations, validate protection
coordination or guarantee operational safety. Do not use it as the sole basis for
a real industrial design. It is not a controller: do not connect it, or its
Modbus server, to a real plant or to equipment that can move or switch
power. Reports about incorrect electrical results are
correctness bugs, not security vulnerabilities, unless they expose a broader
memory-safety or data-safety issue.
