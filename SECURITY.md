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

OpenCircuit is a desktop schematic editor and preliminary simulator. Security
issues include, but are not limited to:

- malformed `.ocircuit` files causing crashes or unsafe behavior;
- path traversal or unsafe file handling in the application layer;
- dependency vulnerabilities;
- denial of service through crafted project files;
- future industrial I/O adapters exposing unintended network access.

## Safety boundary

OpenCircuit does not certify electrical installations, validate protection
coordination or guarantee operational safety. Do not use it as the sole basis for
a real industrial design. Reports about incorrect electrical results are
correctness bugs, not security vulnerabilities, unless they expose a broader
memory-safety or data-safety issue.
