# ADR-0010: Evaluation of OPC UA and MQTT

Status: Accepted (2026-11-15). Spec reference: `docs/spec02.md` §2.5, §20
(M4: "Modbus TCP por adaptador; avaliação de OPC UA e MQTT"). Builds on
[ADR-0009](0009-industrial-io.md) (Modbus TCP server).

## Context

M4 asks for an evaluation of OPC UA and MQTT, not an implementation. The
evaluation follows the rules every dependency meets here: licences in
`deny.toml`, the core MSRV (Rust 1.85), WebAssembly for anything in a core
crate, a small dependency tree, and the safety position of ADR-0009 (the
simulator never becomes a way to command real equipment).

## Spike

Each candidate was resolved in an empty crate outside the repository and
inspected with `cargo metadata` (2026-11-15):

| Protocol | Crate | Licence | Crates pulled | Notes |
| --- | --- | --- | ---: | --- |
| OPC UA | `opcua` 0.12 | MPL-2.0 | 108 | client and server; MPL-2.0 is outside the allowlist |
| OPC UA | `async-opcua` 0.19 | MPL-2.0 | 177 | maintained fork; same licence, larger tree |
| OPC UA | open62541 (C, through bindings) | MPL-2.0 | — | C library behind FFI; same licence |
| MQTT | `rumqttc` 0.25 | Apache-2.0 | 73 | async client on `tokio`; TLS through `rustls`/`ring` |
| MQTT | `mqtt-protocol` 0.12 | MIT/Apache-2.0 | 9 | packet codec only |
| MQTT | `mqttbytes` 0.6 | Apache-2.0 | 2 | packet codec only; no release since 2021 |
| MQTT | `paho-mqtt` 0.14 | EPL-2.0 | 37 | C library; EPL-2.0 is outside the allowlist |

## Decision

- **OPC UA is not implemented in M4.** Every Rust and C implementation found
  is under MPL-2.0, outside the licence allowlist. Adopting one is a licence
  decision for the maintainers, not an engineering one; until it is made, or
  a permissively licensed implementation appears, OPC UA stays out. A server
  written here is not justified: OPC UA's information model, security
  profiles and session handling are far larger than the Modbus subset of
  ADR-0009.
- **MQTT is feasible and deferred past M4.** It fits the licences (a codec
  crate or a hand-written MQTT 3.1.1 subset). If a user need appears, it
  follows the pattern of ADR-0009:
  - the codec and the topic map in `circuit-io` (no socket, no clock), the
    connection and the loop in the command line (`opencircuit serve --mqtt
    <broker>`);
  - topics under one prefix chosen by the user, `<prefix>/out/<REF>` for
    outputs (published on change, retained, QoS 0) and `<prefix>/in/<REF>`
    for inputs; subscribing to inputs is a separate, explicit option, so the
    default only publishes the state of the simulation;
  - MQTT is a client of a broker that real devices may also use, so the
    documentation must say that a broker shared with a plant is out of
    bounds; TLS (which brings `rustls` and `ring`) is a separate decision.
- **Modbus TCP remains the integration interface of M4**, documented in
  `EXTENDING.md` with its I/O map and limits.

## Alternatives considered

- **Accept MPL-2.0 for an OPC UA crate.** Not decided here: it changes the
  licence policy of the project and needs the maintainers.
- **Implement MQTT now with `rumqttc`.** Rejected for M4: an async runtime
  and a TLS stack for an optional interface nobody has asked for yet, and the
  exit criterion asks for an evaluation.

## Consequences

- No new dependency in M4 for OPC UA or MQTT.
- The design above is the starting point if MQTT is requested; OPC UA waits
  for a licence decision.
