---
type: backlog-item
title: Content-damage perf watchpoints
description: One remaining perf pothole in the GPU-content-damage path (the O(nodes) walk, unrealistic at current app scales; symptom and drop-in fix recorded); the boundary-shader-input full re-bake was fixed 2026-08-10 (shader_dirty + Compose instead of invalidate_paint).
status: open
timestamp: 2026-08-10T00:00:00Z
---

# Content-damage perf watchpoints

Correctness landed in
[snapshot-gpu-content-invalidation](snapshot-gpu-content-invalidation.md);
this holds its known perf potholes. Not worth fixing speculatively - the
open one has a crisp symptom and a contained fix.

## O(nodes) walk in texture_content_changed

`RenderTree::texture_content_changed` iterates every node once per frame
that had GPU content writes (~5-20ns/node: linear map iteration plus a
kind-discriminant check). At realistic tree sizes (hundreds to a few
thousand nodes) that is tens of microseconds against a frame budget -
invisible. It could matter only at ~50k+ nodes combined with per-frame
GPU writes.

- **Symptom:** jsMs/frameMs growth on exactly the frames that carry GPU
  writes, scaling with node count, in an app that mixes a very large UI
  tree with continuous GPU content.
- **Fix:** maintain an index of texture-referencing node ids (the dual of
  the walk; must hook texture src set/unset, boundary-shader binding
  writes, and node destroy) and iterate that instead. No contract change.

## Boundary-shader INPUT hit does a full re-bake - FIXED 2026-08-10

Fixed as proposed, same day: `texture_content_changed` splits the two
reference shapes. A texture element under a snapshot boundary is IN the
bake and gets `invalidate_paint`; a view whose boundary shader samples a
changed id as an extra input gets `mark_shader_dirty` (new pub(crate) on
View) plus `Damage::Compose` - the exact params-write shape, so the bake
survives and only the pass re-runs over it (RerunNodeShader flushes dirty
targets first, so the rerun samples fresh input pixels). The view-shader
hit also now requires the view's OWN boundary mode to be snapshot (a
shader without one is ignored with a warning, so its inputs are inert).
Tests: tree.rs `boundary_shader_input_counts_as_reference` (flag set),
`boundary_shader_input_hit_keeps_the_bake` (own cache survives, parent
recording repaints).
