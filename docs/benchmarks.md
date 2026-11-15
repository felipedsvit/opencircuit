# Benchmarks

Rough timings of what the viewer and the editor do per frame, on synthetic
documents (spec §14.5). They are a reference for budgets, not a CI gate: the
numbers depend on the machine. Regenerate them with:

```sh
cargo run -p xtask --release -- bench
```

## Documents

`xtask/src/bench.rs` builds two kinds of ladder, rungs side by side:

- **Ladder.** Each rung is a 24 V tap, a normally open pushbutton, a
  contactor coil and a 0 V tap wired in series: four devices, four symbols
  and three wires. The run presses every pushbutton on the first tick, so
  every coil is energized and every contactor picks up. The circuit then
  settles, and most ticks reuse the memoized solution of an unchanged switch
  mask.
- **Oscillators.** Each rung is a 24 V tap, a relay coil fed through the
  relay's own normally closed contact, and a 0 V tap: three devices. Every
  relay toggles on every tick, so no tick reuses the previous solution. This
  is the worst case for the simulator.

## Measurements

Release build, x86_64 Linux, 8 hardware threads, 2026-11-15, after the M3
features (interlocks, six-terminal motors, supply sections, export). Times
are microseconds per call, averaged over repeated calls.

| devices | compile | scene, at rest | scene, running | new + step_n(1000) | snapshot | symbol_at | move edit | SVG | PDF |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 100 | 218.0 | 91.6 | 106.8 | 476.6 | 2.0 | 4.3 | 340.5 | 222.2 | 74.9 |
| 500 | 1192.3 | 533.8 | 610.4 | 2342.6 | 7.3 | 54.0 | 1790.6 | 1103.3 | 366.5 |
| 2000 | 5052.4 | 2204.8 | 2591.1 | 9268.5 | 23.3 | 274.2 | 7812.4 | 4494.3 | 1480.5 |
| 5000 | 14880.4 | 5915.5 | 6870.2 | 23927.4 | 57.3 | 750.4 | 24843.4 | 11323.8 | 3822.4 |
| 10000 | 32661.3 | 13424.5 | 15342.2 | 48623.9 | 113.4 | 1573.8 | 54781.9 | 23772.0 | 8125.3 |

Switching on every tick (oscillators):

| devices | new + step_n(1000) | per tick |
| ---: | ---: | ---: |
| 102 | 8794.3 | 8.8 |
| 501 | 52440.3 | 52.4 |
| 2001 | 245621.9 | 245.6 |
| 5001 | 640797.7 | 640.8 |
| 10002 | 1417520.6 | 1417.5 |

- **compile:** `circuit_sim::compile` of the whole document.
- **scene, at rest / running:** `build_scene` without a snapshot and with the
  snapshot of the running ladder.
- **new + step_n(1000):** a new `Simulator`, then 1,000 ticks (ladder: every
  button pressed on the first); trace recording off.
- **snapshot:** one `Simulator::snapshot` after those ticks.
- **symbol_at:** `LayoutIndex::symbol_at` on the first coil, the query behind
  a click on the canvas.
- **move edit:** `circuit_commands::ops::move_by` of one symbol, then
  `Transaction::apply_to`: what one drag step of the editor costs. Both
  steps copy the document and validate it, so the cost grows with the
  document, not with the selection.
- **SVG / PDF:** `circuit_export::svg` and `circuit_export::pdf` of the scene
  at rest, print theme.

The first run (M2a/M2b, 2026-11-14) measured 4,180 µs to compile 2,000
devices; the M3 checks (interlocks, star points, sections) bring it to about
5,050 µs. The other columns did not move.

## Reading

At 60 frames per second a frame has 16.6 ms.

- **Up to 2,000 devices** every path fits with room to spare: about 2.6 ms
  for the scene, 0.15 ms for 16 settled ticks, 4 ms for 16 ticks of a
  circuit that switches every tick, and 10 ms for a drag step plus its
  scene. Exports take a few milliseconds and run once.
- **5,000 devices** still view and simulate in real time (about 7 ms of
  scene, 10 ms per frame of constant switching), but a drag step costs about
  25 ms plus the scene, so dragging is no longer smooth.
- **10,000 devices** exceed the frame: the scene alone takes 13 to 15 ms,
  16 ticks of constant switching take about 23 ms, a drag step about 55 ms,
  and recompiling after an edit about 33 ms.

The documents of the M3 examples have tens of devices, and a control panel
drawn by hand rarely reaches a thousand; the default tick of 1 ms (spec
§24, item 3) stays adequate. The measured limits point at the first
optimizations, to be made only when a real document needs them: keep the
integrity errors of the current document instead of recomputing them for
every transaction (editing), cull the scene to the visible area and cache
the symbol artwork (viewing), and reuse the per-group buffers of the solver
instead of rebuilding them on every solve (constant switching).

Painting with egui (tessellation and the GPU) is not measured; only the work
of the core, the scene and the exporters is.
