---
title: A pass's execMs does not say whether the cost is vertices or fill
description: Landed 2026-09-23 with the per-target attribution: the vertices (indices on an indexed draw) each pass submits are counted from the draw ranges at issue, per target, and reported per presented frame in get_stats' window.targets and cumulatively in /gpu; fragments shaded stay a documented gap on the GLES 3.0 baseline, so fill still needs subtraction.
created: 2026-09-08
completed: 2026-09-23
---

# A pass's execMs does not say whether the cost is vertices or fill

## Symptom

Chasing the frame time of a 14.3M-point lit cloud, the question was
whether the cost was per-point vertex work or fragment fill.
`execMs` on the draw target is one number for the whole pass, so the
only way to attribute it was to change one variable and difference:
shrink the splats fivefold (24x less fill), re-measure, see 22.0 ms
against 21.6, conclude fill was not the cost. That is two measurement
rounds, a key binding to vary the thing with, and it only works for a
cost you already guessed at.

## Cause

`execMs` comes from GL timer queries (`alloy/src/gl/timing.rs`,
`TIME_ELAPSED` under `EXT_disjoint_timer_query`) around the whole pass.
Nothing counts what the pass submitted or shaded.

Half of it is not a measurement problem at all: primitives submitted is
known on the CPU. The `/gpu` entry already reports `indexCount` and
`instanceCount`, and their product is the vertex count going in - the
figure that diagnosed the record-mesh case (see the instancing cost
model in `packages/3d/AGENTS.md`). What is genuinely missing is the
fragment side.

## Done looks like

The cheap half first, then an honest limit:

- Report primitives (or vertices) submitted per entry and per pass, from
  the counts the draw already carries. No GL feature needed, and it
  makes "a big instanceCount beside a tiny indexCount" a number rather
  than a pattern to notice.
- Fragments shaded has NO portable answer on our baseline: GLES 3.0
  offers only boolean occlusion queries (`ANY_SAMPLES_PASSED`), and
  `SAMPLES_PASSED` / `PRIMITIVES_GENERATED` need desktop GL or ES 3.2.
  So either it stays a documented gap, or it becomes an opt-in counter
  on stacks that expose the query, which cuts against measurements
  meaning the same thing on every platform.
- Documented either way: `debugging.md` now says which question the
  existing counts answer and that vertex-versus-fill still needs
  subtraction.

## What it involves

Plumbing counts already present in the draw entry out through the same
report `execMs` rides (`lattice/src/go/connection.rs`), plus the
per-target attribution work in
okf/done/gpu-per-target-pass-attribution.md, which is the same
report and landed together with this.

## Landed (2026-09-23)

The cheap half, as shaped: `run_pass` (alloy/src/gl/pass.rs) sums
`vertex_count x instance_count` over the entries it draws (and the
fullscreen triangle's three) into a raster-thread local that the pass
accounting owner (`timed_pass` in raster/targets.rs, and the node shader
pass) takes around each pass and credits to the target beside its pass
count. Reported cumulatively as `vertices` on the `/gpu` inventory and per
presented frame as `verticesPerFrame` in `window.targets`. The honest
limit stands: fragments shaded has no portable answer on GLES 3.0, so
debugging.md keeps saying fill needs subtraction.
