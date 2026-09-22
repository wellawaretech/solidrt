---
title: State-change grouping inside a draw sort bucket does not pay on desktop
description: Unity and Godot sort by shader and material inside each depth bucket; measured here on 300 draws with the program changing on every entry, the cost is under 0.1 ms of issue time per frame and inside the noise of GPU time, so the core's bind-order tiebreak stays and a state cache in the GL pass is the fix if a mobile driver ever shows it.
created: 2026-09-22
---

# State-change grouping inside a draw sort bucket

Distilled from the draw sort follow-ups (2026-09-22). True regardless of
that item.

## The question

The core's draw sort (`Spatial::set_draw_sort`) keys an opaque entry by a
logarithmic distance bucket, and breaks ties inside a bucket by bind
order. Unity and Godot sort by shader and material inside each depth
bucket; Three puts material id above depth. Bind order groups materials
only by accident of attach order, so the question was whether a
pipeline id on the sink as the in-bucket tiebreak would save anything.

## The measurement

Bench probe `probes/3d-opaque-order-bench.tsx`, mode `flip`: 300 plates,
every other one phong, so the program changes on every entry of the
sorted list, against the same 300 under one program. Desktop GL driver,
release client, GPU pass time from the control API.

- Issue time: under 0.1 ms per frame more with the program flipping.
- GPU exec time: inside the noise between runs.

Not worth a pipeline id on the sink.

## What the pass does today

The GL mesh pass (`alloy/src/gl/pass.rs`, the per-draw loop) binds the
program, re-applies the target's shared params and resets the depth,
blend and cull state for every entry unconditionally. A desktop driver
absorbs the redundant binds. If a mobile driver ever shows the cost, the
fix is a state cache in that loop (skip the bind and the shared-param
re-apply while the program is unchanged), not the sort: the cache also
covers unsorted targets, which the tiebreak never would.
