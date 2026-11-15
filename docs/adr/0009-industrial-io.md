# ADR-0009: Industrial I/O through a Modbus TCP server

Status: Accepted (2026-11-15). Spec reference: `docs/spec02.md` §2.5, §5.3,
§5.4, §10.3, §20 (M4). OPC UA and MQTT are evaluated separately (ADR-0010).

## Context

M4 asks for Modbus TCP through an adapter crate, `circuit-io`. The intended
use is virtual commissioning and teaching: a PLC program or an HMI under test
reads the simulated outputs and writes the simulated inputs. The core must
stay free of sockets and clocks (architecture rule 6), and a simulator that
talks to a network must not become a way to command real equipment.

## Decision

- **The simulator is a server, never a client.** `opencircuit serve
  <project> --modbus <address>` answers Modbus TCP requests; OpenCircuit
  never connects to another device.
- **Split between core and application.** `circuit-io` is a core crate: it
  encodes and decodes Modbus TCP frames (MBAP header and PDU) and builds the
  I/O map of a compiled circuit, with no socket, no thread and no clock, so
  it builds for WebAssembly and is tested with arbitrary bytes. The command
  line owns the socket and the real-time loop.
- **I/O map**, stable by device ID, printed by `opencircuit serve
  --print-map`:
  - coils (read with function 1, written with 5 and 15): one per operable
    input in device-ID order: pushbuttons (1 = pressed), maintained switches
    (1 = closed), thermal overload relays (1 = tripped);
  - discrete inputs (function 2): one per observable output in device-ID
    order: contactors, relays and transformers (picked up or powered),
    timers (output), lamps and buzzers (energized);
  - input registers (function 4): the tick as four 16-bit words, the fault
    latch, then one register per motor (0 stopped, 1 forward, 2 reverse).
  Any other function, or an address outside the map, is answered with a
  Modbus exception (illegal function, illegal data address).
- **Tick discipline.** Writes are applied as input actions at the next tick
  boundary, exactly as operator actions; reads return the last solved
  snapshot. Two writes to one input before a tick: the last one wins.
- **Safety limits.** The server listens on `127.0.0.1` unless `--listen`
  names another address, which prints a warning; at most 4 connections;
  frames longer than the Modbus maximum (260 bytes) close the connection;
  idle connections close after 30 s; Modbus has no authentication and the
  documentation says so. The documentation states that OpenCircuit is not a
  controller and must not be connected to a real plant.

## Alternatives considered

- **A Modbus library with an async runtime** (`tokio-modbus`). Rejected: it
  brings an async runtime for a protocol whose subset here is a few hundred
  lines of codec, and the codec must be testable without a runtime.
- **Client role.** Rejected: it would let the simulator write to real
  devices.
- **Holding registers for parameters** (presets, delays). Deferred: the
  first use is discrete I/O.

## Consequences

- A new core crate, `circuit-io`, and a `serve` command; the CLI may then
  depend on `circuit-io` and keeps the socket and the clock.
- The I/O map is a published contract; changing its order needs a new
  version of this ADR.
