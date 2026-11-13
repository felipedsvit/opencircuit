# Roadmap

The roadmap follows the consolidated architecture proposal. Each milestone has an
exit criterion; features outside the current milestone are not started early.

## M0 — Foundation and contracts (in progress)

- [x] Workspace, toolchain and explicit members.
- [x] Stable IDs and device/symbol separation.
- [x] Stable terminal keys.
- [x] Conductors and explicit supply profiles.
- [x] Structural diagnostics and validation.
- [x] Architecture rule checker.
- [ ] Cross-platform CI green on all targets.
- [ ] Temporal semantics contracts.

**Exit criterion:** documented contracts that compile and are covered by initial
tests.

## M1 — Headless logic engine

- Initial component catalog.
- Topology compiler and deterministic dense indices.
- Union-Find connectivity.
- Discrete electrical solver.
- Pushbuttons, coils and contacts.
- TON and TOF timers.
- Direct-online and reversing scenarios.
- Headless CLI.
- Golden tests.

**Exit criterion:** control scenarios run without the GUI with deterministic
traces.

## M2 — Desktop editor

- egui/eframe application.
- Canvas and camera.
- Component library.
- Selection and movement.
- Wires and junctions.
- Undo and redo.
- JSON load/save and initial migrations.
- Snapshot visualization.

**Exit criterion:** draw, save, reopen and simulate a direct-online starter.

## M3 — Advanced circuits and export

- Six-terminal motor model and windings.
- Star-delta scenario.
- Specialized solver or MNA where required.
- Expanded electrical diagnostics.
- SVG and PDF export.

**Exit criterion:** star-delta scenario validated with an adequate electrical
model and no improper contactor overlap.

## M4 — Ecosystem

- WASM application.
- Modbus TCP adapter.
- Evaluation of OPC UA and MQTT.
- WASM plugins.
- More components and examples.
- Extension and community documentation.

**Exit criterion:** extensible platform with documented integration interfaces
and explicit safety limits.

## Out of scope for the MVP

Analog simulation, rigorous voltage/current/impedance calculation, transients,
pneumatics, hydraulics, real PLC execution, industrial protocols, plugins,
real-time collaboration and proprietary CAD import.
