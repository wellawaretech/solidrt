---
title: What the Adreno 610 taught about a heavy splat pass and the submission budget
description: The measurements behind okf/done/splat-adreno-gpu-fault.md - the SM-T500's splat ladder (the pass is vertex-stage bound per bin, not fill, and nearly resolution-independent), the u8 against pq cost, the planner faults the tests caught, the sentinel-seed probe gap and the device runs that verified the budget.
created: 2026-10-19
---

# What the Adreno 610 taught about a heavy splat pass and the submission budget

Cut from the Findings of
[done/splat-adreno-gpu-fault.md](../done/splat-adreno-gpu-fault.md)
(part B, the submission budget in alloy gl/slicing.rs).

- **Tablet rows, 2026-10-19** (SM-T500, Adreno 610, the pq SH3 lizard,
  786k splats, probes/splat-mesh-probe.tsx on the dev server over the
  LAN, cadence hold off, the orbit on, a 10 s window after a 6 s settle;
  the pass figure is the pass timer's, the frame figure the wall clock
  per presented frame, and the two agree). A fresh process, the probe
  pinned to quarter scale before its first mount so nothing faulted;
  `contextLosses` 0 throughout:

  | rung | target | splat pass ms | frame ms |
  |---|---|---|---|
  | stock 0.25 | 500x273 | 276 | 347 |
  | stock 0.5 | 1000x546 | 339 | 401 |
  | stock 0.75 | 1500x819 | 390 | 472 |
  | stock 1 | 2000x1092 | 493 | 539 |
  | noraster 1 | 2000x1092 | 6.3 | 68 |

  The reads: (1) the pass costs ~280 ms at a sixteenth of the pixels and
  1.8x that at all of them: fill is the minor term, and what costs is
  per vertex and per bin - a tiler's render pass runs the full vertex
  stage (fifteen texel fetches per corner at pq SH3) for every primitive
  in every bin it touches, and the quarter-res target is a few bins.
  (2) The noraster rung culls every primitive at the vertex stage, so on
  Adreno it measures the binning pass alone (a position-only vertex
  stage): 6 ms. It is not this GPU's vertex cost, and the "vertex ALU is
  invisible" verdict of okf/notes/gaussian-splats.md is the Pixel's
  Mali's, not a tiler's in general; the stock rung at a small scale is
  the vertex-stage price here. (3) The full-res lizard draws at 493 ms
  per pass in steady state without a fault; the fault the day's first
  run hit was the TRAIN (1M splats, SH 0, large floaters, more bins per
  quad) at the window size on the probe's startup mount, under 3 s from
  its load. The train at full resolution is the verification case.
  (4) The backlog's 1119 ms at renderScale 0.25 was the u8 SH3 lizard:
  three random 16-byte texels per corner into a 37 MB block, against
  pq's two label texels and a 32 KB palette. On this GPU the encoding
  is a 4x on the pass, which puts u8 past the limit at half resolution;
  the pq default stands, with a reason beyond memory now.
  (5) The instance-attribute against texel-fetch split the plan asked
  for if the pass read vertex-bound is NOT measured: it needs a probe
  variant feeding records as instance attributes (the retired K=1 form),
  and its answer would shape an Adreno-specific record path, not this
  item; it is listed under the open items of the done record.
- **A splat mesh created after a context recovery draws black on the
  SM-T500** (2026-10-19): the day's first ladder ran on a context
  recovered from the startup fault, the lizard loaded fifteen seconds
  after the recovery, and every rung read 2 ms per pass with a black
  scene (the resolve target's rgb 0): the data textures carried their
  content (the colors texture read back right), the id stream held a
  sorted order, the program linked, no GL error was logged. Part A
  verified the HUD and the loss probe's content back, never a 3d scene
  mounted after the recovery. Filed as
  okf/backlog/context-recovery-3d-scene-black.md; the rows above were
  taken on a fresh process because of it.
- **The engine's id stream is safe before the first sort**: the first
  asynchronous seed writes the sentinel first (alloy context/order.rs,
  `ids_written`), so a fresh entry draws nothing rather than record 0 at
  every slot. Checked while reading the startup fault; not a cause.
- **Two planner faults the tests caught before any device did**
  (2026-10-19): the slice loop subtracted a strip's overlap after the
  LAST slice too, so a triangle strip's loop never reached the end (the
  unit test spun; `advance` now ends the draw); and the rate kept for the
  next render was the slowest slice's, which the tail slice - two
  instances measuring the submission round trip alone, about 500 us for
  12 vertices - set to seconds per draw, so the headless app test's
  second render sliced again. A submission's fixed cost (the flush, the
  kernel round trip, a tiler's tile store and load) is not per vertex:
  `RateMeter` takes a slice's own rate only when the slice ran at least
  `SLICE_RATE_MIN_MICROS` (10 ms), and never under the render's total
  rate, which spreads the fixed cost over all the work.
- **The tablet's first run of part B, 2026-10-19:** the train at the
  window size - the fault case - drew: `contextLosses` 0, no KGSL line,
  the log "draw splat-quads (6159072 vertices) issued as 7 submissions,
  the slowest 34 ms against the 500 ms budget", later renders 7 slices
  with the longest at 229 ms. The lizard loaded next faulted on its
  second render: its first render ran while the sort had not landed, so
  the id stream was the sentinel seed, every splat culled in the vertex
  stage, and the probe measured a draw of nothing (seven slices, 31 ms
  at most); the next render, with real ids, was predicted light, went
  out whole and ran past 2 s - about four times its steady-state 493 ms
  (the GPU clock ramps from idle, the caches are cold). The train's
  probe very likely measured its seed too (0.6 s after a load whose sort
  takes about 0.3 s); its whole render stayed under the kernel's 2 s and
  the one-slice bound then re-sliced it, which is the margin working,
  not the design. The engine's order registry knows that transition:
  it now sends `ResetDrawGauge` with the first real publish over a
  sentinel seed (context/order.rs `Mirror::seeded`), so the draw probes
  again on its first real render. A content change the engine does not
  know about (an app rewriting an instance buffer from zeros to a
  population) has no such hook: its first real render is bounded by the
  previous measurement's 4x margin only, which the design accepts.
- **The tablet, final run, 2026-10-19:** a fresh process, the train at the
  window size (the fault case) drew - `contextLosses` 0, no KGSL line,
  its draw issued as 7 submissions on the probe (slowest 32 ms) and 3
  afterwards (longest 311 ms); the pq lizard loaded next probed on its
  seed, probed again when its first order landed (the reset), and drew
  at 2 submissions (longest 201 ms, 241 ms over a 20 s orbit at the
  window size, 51 renders), the snapshot the lizard; `contextLosses` 0
  throughout, `gpuDrawSlices` 74. On this machine's Mesa the app test
  reads the probe at 5 to 6 slices and one draw call from the second
  render on, pixels identical.
