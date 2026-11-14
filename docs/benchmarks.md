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

Release build, x86_64 Linux, 8 hardware threads, 2026-11-14 (the move edit
column was added with `circuit-commands`, in a second run on the same
machine). Times are
microseconds per call, averaged over repeated calls.

| devices | compile | scene, at rest | scene, running | new + step_n(1000) | snapshot | symbol_at | move edit |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 100 | 172.7 | 90.4 | 103.1 | 490.6 | 1.7 | 4.0 | 347.9 |
| 500 | 988.2 | 546.4 | 607.8 | 2370.0 | 5.5 | 53.8 | 1773.1 |
| 2000 | 4180.0 | 2186.3 | 2390.7 | 9236.3 | 17.6 | 269.2 | 7222.8 |

- **compile:** `circuit_sim::compile` of the whole document.
- **scene, at rest / running:** `build_scene` without a snapshot and with the
  snapshot of the running ladder.
- **new + step_n(1000):** a new `Simulator`, then 1,000 ticks with every
  button pressed on the first; trace recording off.
- **snapshot:** one `Simulator::snapshot` after those ticks.
- **symbol_at:** `LayoutIndex::symbol_at` on the first coil, the query behind
  a click on the canvas.
- **move edit:** `circuit_commands::ops::move_by` of one symbol, then
  `Transaction::apply_to`: what one drag step of the editor costs. Both
  steps copy the document and validate it, so the cost grows with the
  document, not with the selection.

## Reading

At 60 frames per second a frame has 16.6 ms. With 2,000 devices the viewer
spends about 2.7 ms building the scene and, at real-time speed with 1 ms
ticks (about 16 ticks per frame), about 0.2 ms stepping, plus one snapshot.
The default tick of 1 ms (spec §24, item 3) leaves ample room at these sizes.

Editing is heavier. With 2,000 devices one move costs about 7 ms, so a drag
preview (the move applied to a copy every frame, plus the scene) takes about
10 ms of the 16.6 ms frame. That is enough for M2b; the first optimization,
if needed, is to keep the integrity errors of the current document instead of
recomputing them for every transaction.

Two limits of this measurement:

- The ladder settles after two ticks, so most of the 1,000 ticks reuse the
  memoized solution of an unchanged switch mask. A circuit whose switches
  change every tick costs more per tick.
- Painting with egui (tessellation and the GPU) is not measured; only the
  work of the core and of the scene is.
