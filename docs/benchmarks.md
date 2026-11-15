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

Release build, x86_64 Linux, 8 hardware threads, 2026-11-15, after M4 and the
optimizations of the post-M4 review (issue #50). Times are microseconds per
call, averaged over repeated calls.

| devices | compile | scene, at rest | scene, running | new + step_n(1000) | snapshot | symbol_at | move edit | save | SVG | PDF |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 100 | 213.8 | 93.6 | 110.2 | 490.2 | 2.1 | 4.1 | 131.9 | 87.9 | 238.0 | 88.3 |
| 500 | 1199.7 | 533.7 | 611.7 | 2416.3 | 6.8 | 55.1 | 655.9 | 432.8 | 1141.7 | 364.1 |
| 2000 | 4902.9 | 2147.2 | 2473.9 | 9578.9 | 23.8 | 277.3 | 2606.3 | 1732.2 | 4604.9 | 1478.2 |
| 5000 | 12261.5 | 5667.3 | 6569.3 | 24522.8 | 57.8 | 727.6 | 7966.4 | 4470.4 | 11511.6 | 3630.8 |
| 10000 | 27113.4 | 12565.6 | 14617.1 | 49053.8 | 112.6 | 1484.6 | 18956.0 | 10050.9 | 23624.6 | 7560.0 |

Switching on every tick (oscillators):

| devices | new + step_n(1000) | per tick |
| ---: | ---: | ---: |
| 102 | 2187.3 | 2.2 |
| 501 | 10475.8 | 10.5 |
| 2001 | 41579.9 | 41.6 |
| 5001 | 104660.6 | 104.7 |
| 10002 | 208866.0 | 208.9 |

- **compile:** `circuit_sim::compile` of the whole document.
- **scene, at rest / running:** `build_scene` without a snapshot and with the
  snapshot of the running ladder.
- **new + step_n(1000):** a new `Simulator`, then 1,000 ticks (ladder: every
  button pressed on the first); trace recording off.
- **snapshot:** one `Simulator::snapshot` after those ticks.
- **symbol_at:** `LayoutIndex::symbol_at` on the first coil, the query behind
  a click on the canvas.
- **move edit:** `circuit_commands::ops::move_by` of one symbol, then
  `Transaction::apply_to`: what one drag step of the editor costs. Each
  step copies the document and the committed result is checked for
  integrity, so the cost grows with the document, not with the selection.
- **save:** `circuit_project::to_string` of the document, already in
  canonical order as it is after an edit.
- **SVG / PDF:** `circuit_export::svg` and `circuit_export::pdf` of the scene
  at rest, print theme.

The first run (M2a/M2b, 2026-11-14) measured 4,180 µs to compile 2,000
devices; the M3 checks (interlocks, star points, sections) bring it to about
5,050 µs. The other columns did not move.

## Reading

At 60 frames per second a frame has 16.6 ms.

- **Up to 2,000 devices** every path fits with room to spare: about 2.1 ms
  for the scene, 0.15 ms for 16 settled ticks, 0.7 ms for 16 ticks of a
  circuit that switches every tick, and 4.7 ms for a drag step plus its
  scene. Exports take a few milliseconds and run once.
- **5,000 devices** view, simulate and drag within a frame: about 6 ms of
  scene, 1.7 ms for 16 ticks of constant switching, and 8 ms for a drag step
  (14 ms with the scene).
- **10,000 devices** still exceed the frame in the viewer: the scene alone
  takes 13 to 15 ms, and a drag step 19 ms on top of it. Constant switching
  now fits (3.3 ms for 16 ticks), and recompiling after an edit takes about
  27 ms.

The documents of the M3 examples have tens of devices, and a control panel
drawn by hand rarely reaches a thousand; the default tick of 1 ms (spec
§24, item 3) stays adequate.

## What the review changed, and what it left

The first measurements (2026-11-15, after M3) pointed at three
optimizations. Two were made, measured against that run:

| Path, 10,000 devices | Before | After |
| --- | ---: | ---: |
| drag step (`move edit`) | 54.8 ms | 19.0 ms |
| saving a project | 15.0 ms | 10.1 ms |
| 1,000 ticks of constant switching | 1,418 ms | 209 ms |

- **Editing.** `move_by` and `rotate` applied their transaction with every
  check only to route wires, `apply_to` validated the original document on
  every call, and it ran every structural check when it needed five codes.
  The intermediate document is no longer checked, the original is validated
  only when the result has an integrity error, and `integrity_errors` runs
  the four passes that can emit those codes.
- **Solver.** A profile of the oscillators showed 83% of the tick in
  `Solver::solve`: 640 µs per tick in the map of feeds by group and 360 µs
  in the series-load check, at 10,000 devices. Both use buffers indexed by
  group that the solver keeps.
- **Saving.** `to_string` copies the document only when it is not already in
  canonical order.

Not done, with the reason:

- **Scene (culling, caching static geometry, `LayoutIndex`).** The index is
  1.1 ms of the 12.6 ms of a scene at 10,000 devices (9%). The rest is
  spread over wires (23%), symbol shapes (25 to 35%) and labels (27%), each
  allocating its own shapes, so there is no cheap fix: it takes a cache of
  the static geometry keyed by document revision, or a visible area passed to
  `build_scene`, and either changes how the desktop application builds a
  frame. The scene fits a frame up to about 5,000 devices; revisit it when a
  real document is larger.
- **`snapshot()` in one pass.** It takes 0.11 ms at 10,000 devices, 0.7% of a
  frame; a single pass would save a fraction of that.

Painting with egui (tessellation and the GPU) is not measured; only the work
of the core, the scene and the exporters is.
