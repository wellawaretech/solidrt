---
title: Content-damage perf watchpoints
description: Perf potholes in the damage-tracking path. Open - the unbatched invalidate_paint in set_unrounded_layout making resize O(n * depth). Fixed - the O(nodes) texture walk (referencer index, 2026-09-02) and the boundary-shader-input full re-bake (shader_dirty + Compose, 2026-08-10).
created: 2026-08-10
---

# Content-damage perf watchpoints

Correctness landed in
[snapshot-gpu-content-invalidation](../done/snapshot-gpu-content-invalidation.md);
this holds the damage path's known perf potholes. Not worth fixing
speculatively - each open one has a crisp symptom and a contained fix.

## O(nodes) walk in texture_content_changed - FIXED 2026-09-02

Was: `RenderTree::texture_content_changed` iterated every node once per
frame that had GPU content writes, so an app mixing a very large tree
with continuous GPU content (camera, video) paid O(nodes) per tick.

Fixed with the sketched index, but maintained by reconciliation rather
than setter hooks: the setters live on the kind structs and never see
the tree, while every write completes through `edit`/`try_edit` - so the
tree recomputes membership there (`Element::references_textures`, a pure
function of current element state: a texture with a source, a view whose
shader has texture inputs), plus create_node and delete_recursive.
Recompute-per-edit is O(1) and cannot drift whatever the closure
touched, including the FFI path's direct field writes. `try_edit`
reconciles even on Err, since the closure may mutate before failing.
`texture_content_changed` and `referenced_texture_ids` (the
deferred-destroy sweep, the same O(nodes) shape) both iterate the set;
the boundary checks stay at query time, so behavior is unchanged. Test:
tree.rs `texture_referencer_index_tracks_lifecycle`.

## Unbatched invalidate_paint in set_unrounded_layout

`set_unrounded_layout` (`alloy/src/rendertree/layout/context.rs`) calls
the plain `invalidate_paint` for every node whose computed layout
changed, and that walk has no early-out: it clears all the way to the
root even through ancestors an earlier walk already cleared this frame.
A resize changes nearly every node's layout, so the invalidation cost is
O(n * depth) on top of the O(n) relayout. Constants are tiny (a borrow
plus a cache clear per step), so this is a constant-factor add on the
worst frame, not a new cliff.

- **Symptom:** layoutMs scaling super-linearly with tree depth on resize
  frames of a very large tree, with the relayout itself (cache-cleared
  taffy pass) not accounting for it.
- **Fix:** the batched walk already exists - `invalidate_paint_batched`
  (`tree.rs`) shares a `visited` set so common ancestors are cleared
  once per batch. Thread one `visited` set through the layout pass and
  call that instead. No contract change.
- If [partial-repaint](../plans/partial-repaint.md) lands, its per-frame damage
  rect accumulation wants old + new bounds out of exactly this walk, so
  the batched form becomes the natural accumulation point rather than
  just a saving.

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
