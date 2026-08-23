---
title: GPU-only redraws never invalidate snapshot boundaries
description: A texture id whose pixels change through GPU writes (draw/shader targets, uploadTexture, camera frames) froze inside repaintBoundary="snapshot" - GPU writes produced no rendertree damage, so the cached bake composited forever; fixed by making content changes first-class damage via Context change tracking + RenderTree::texture_content_changed.
created: 2026-08-10
completed: 2026-08-10
---

# GPU-only redraws never invalidate snapshot boundaries

## The bug (fixed 2026-08-10)

A `render: "auto"` target re-rendering into the same `TextureId` (a 3D
scene, a shader chain), an `uploadTexture` into a mutable texture, a
camera frame, or any other pure-content change never became visible
inside a `repaintBoundary="snapshot"` subtree. The snapshot composited
its stale bake forever; with a boundary `shader` whose params animate,
the effect kept re-running over the frozen source.

Root cause: texture content is an input to paint but was not part of the
damage model. Snapshot validity was keyed on tree damage only
(`invalidate_paint`, whose callers were all tree mutations), GPU writes
only latched a frame request, and the present-only clean path
(`lattice/src/plugins/draw.rs`) never even ran the composite gates -
texture-registry generation deliberately excludes content uploads.
Everything OUTSIDE snapshot boundaries was already correct: display
lists hold live texture references and the raster thread's
`flush_dirty()` runs before drawing and before every `RasterizeDl*`
command, so re-rasterization - once triggered - samples fresh pixels.

The asymmetry this dissolved: driving a target through the `<texture
params>` prop damaged the node and worked, while identical state through
`setTargetParams` did not.

## The fix: content changes are damage

Push-based, exact per id, one damage mechanism. Three pieces, all on the
main thread (the registry, mirrors, and composite gates were already
mutually visible there - no cross-thread state):

1. **Context tracks changed ids** (`alloy/src/context.rs`). Every
   content-mutating method notes its texture id into a drained
   `content_changes` set: target mutations (`set_target_params/textures`,
   `add_draw`/`remove_draw`/`set_draw_order`, per-entry params/textures/
   range, `set_draw`), `update_texture`, replace-at-id
   (`create_texture_at` on a live id - which is also the camera frame and
   stream-resize path), `resize_target`, `render_target`, `copy_texture`,
   and `write_gpu_buffer` (via retained buffer ids on the target/entry
   mirrors). `content_closure` (a pure free function beside
   `samples_transitively`) expands each id through the UI-side sampler
   graph (`shader_sources`) to every flush-rendered target sampling it,
   transitively - so a chain's displayed tail is covered when its head is
   written. Manual targets are barriers: writes to one stage state (no
   note), `render_target`/`copy_texture` are the moments their pixels
   change (forced note). The HashSet dedupes per-frame write bursts to
   one closure walk.
2. **The rendertree turns ids into damage**
   (`RenderTree::texture_content_changed`). One walk over the nodes finds
   every reference to a changed id - a texture element's src, or a
   boundary shader sampling it as an extra input - and, ONLY where an
   inclusive ancestor probe finds a snapshot boundary (`Snapshot`/
   `SnapshotNoAa` mode), calls the existing `invalidate_paint` and bumps
   the revision. No snapshot consumer -> no bump -> pure-GPU frames keep
   the present-only reuse path untouched (the probe is the fast-path
   guarantee, asserted in tests).
3. **The frame build drains before its clean check**
   (`lattice/src/plugins/draw.rs`): `take_content_changes` ->
   `texture_content_changed` right before the reuse gate, so a hit forces
   the rebuild (which re-bakes into the existing allocation; `RasterizeDl*`
   flushes dirty targets first, so the re-bake samples same-frame pixels)
   and a miss changes nothing.

Tests: `tests/gpu_graph.rs` (closure: chains, manual barriers, diamonds,
ping-pong termination), `tests/tree.rs` (snapshot hit bumps revision +
clears caches, no-boundary/recording/unrelated/detached all no-ops,
shader-input reference, SnapshotNoAa).

## Notes

- `destroy_texture` deliberately does not note content (deferred-destroy
  keeps a mounted texture drawing; nothing changes until reclaim).
- Open follow-ups were promoted to their own items so this record stays
  closed: [content-damage-perf](../backlog/content-damage-perf.md) (the O(nodes)
  walk and the boundary-shader-input full re-bake, with symptoms) and
  [texture-params-prop-write-path](texture-params-prop-write-path.md)
  (the params prop's double re-bake and the one-write-path redesign).

## Relations

- [snapshot-boundary-texture-id](snapshot-boundary-texture-id.md) is the
  reverse direction (UI subtree as a GPU-side texture); its "ordering
  within a frame" question can lean on the same flush-first guarantee.
- [gpu-target-dependency-propagation](gpu-target-dependency-propagation.md)
  owns raster-side ordering; the content closure walks the same
  UI-mirrored edges for invalidation and adds no second raster graph.
