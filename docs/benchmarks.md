# Benchmarks

Rough timings of what the viewer does per frame, on synthetic documents
(spec §14.5). They are a reference for budgets, not a CI gate: the numbers
depend on the machine. Regenerate them with:

```sh
cargo run -p xtask --release -- bench
```

## Document

`xtask/src/bench.rs` builds a ladder of independent rungs side by side. Each
rung is a 24 V tap, a normally open pushbutton, a contactor coil and a 0 V
tap wired in series: four devices, four symbols and three wires. The run
presses every pushbutton on the first tick, so every coil is energized and
every contactor picks up.

## Measurements

Release build, x86_64 Linux, 8 hardware threads, 2026-11-14. Times are
microseconds per call, averaged over repeated calls.

| devices | compile | scene, at rest | scene, running | new + step_n(1000) | snapshot | symbol_at |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 100 | 205.1 | 100.3 | 114.6 | 565.6 | 1.7 | 5.3 |
| 500 | 1078.3 | 542.0 | 621.6 | 2574.3 | 5.7 | 54.9 |
| 2000 | 4470.2 | 2408.3 | 2733.9 | 10422.7 | 21.8 | 271.0 |

- **compile:** `circuit_sim::compile` of the whole document.
- **scene, at rest / running:** `build_scene` without a snapshot and with the
  snapshot of the running ladder.
- **new + step_n(1000):** a new `Simulator`, then 1,000 ticks with every
  button pressed on the first; trace recording off.
- **snapshot:** one `Simulator::snapshot` after those ticks.
- **symbol_at:** `LayoutIndex::symbol_at` on the first coil, the query behind
  a click on the canvas.

## Reading

At 60 frames per second a frame has 16.6 ms. With 2,000 devices the viewer
spends about 2.7 ms building the scene and, at real-time speed with 1 ms
ticks (about 16 ticks per frame), about 0.2 ms stepping, plus one snapshot.
The default tick of 1 ms (spec §24, item 3) leaves ample room at these sizes.

Two limits of this measurement:

- The ladder settles after two ticks, so most of the 1,000 ticks reuse the
  memoized solution of an unchanged switch mask. A circuit whose switches
  change every tick costs more per tick.
- Painting with egui (tessellation and the GPU) is not measured; only the
  work of the core and of the scene is.
