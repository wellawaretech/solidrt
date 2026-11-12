---
title: Damage rectangles - the window side and app-declared damage
description: A draw target re-renders only its damage rectangles now (fed by culled draw lists), but the window still repaints the whole leaf that shows it, and no app can declare damage for a write the core cannot derive; the window side maps a target's rectangles through the showing leaf (worth nothing where the window takes the multisampled fast path, as on the SM-T500), feed 2, a core verb, waits for a consumer to be decided, and the built path is still to be timed and checked on a tiler.
created: 2026-11-05
---

# Damage rectangles: the window side and declared damage

The first half is built:
[gpu-target-damage-draw-lists](../done/gpu-target-damage-draw-lists.md)
(the mechanism, feed 1 from culled draw lists, the facts, the force-full
switch), its decision and contract in
[draw-order-and-culling](../design/draw-order-and-culling.md) D8, its
measurements in
[target-damage-measurements](../notes/target-damage-measurements.md).
This item is what is left.

## Symptom

A culled 2d view where one sprite animates now renders only that
sprite's rectangle into its target. Two costs stay whole:

- **The window repaint.** A content change behind a texture id damages
  the whole box of every node that shows it
  (`RenderTree::texture_content_changed`), so a full-window view damages
  the whole window every frame of the animation. On the SM-T500, a
  press frame whose pass redraws one sprite's box (0.59 to 0.71 ms)
  still sits 2.5 ms of GPU above an idle frame, the composite of the
  full-window leaf included
  ([target-damage-measurements](../notes/target-damage-measurements.md)).
- **Writes the core cannot derive.** Only a culled draw list knows where
  its writes land. A layer drawn directly (a record layer without
  `cull`), a tile layer re-bake, a shadow tile and a persistent texture
  driven by draw entries all re-render in full on any write.

## What exists

- **Target damage**, built: a target's or sub-target's pending dirt is
  full or a few rectangles; a rectangle-dirty owner renders through the
  partial path inside them alone (`flush_dirty_within` in
  `alloy/src/raster/targets.rs`). Feed 1 derives the rectangles in
  `alloy/src/gpu/damage.rs` (`ListDamage`).
- **The window damage**: `texture_content_changed`, resolved without a
  walk on the reuse path by `resolve_reuse_damage`, and partial repaint
  patching the window from it
  ([partial-repaint](../done/partial-repaint.md)). The damage is the
  whole leaf. A window whose default framebuffer is multisampled takes
  the fast path, which never patches (`window_fast_path`,
  `DamageTracker::route`). The SM-T500's window backbuffer is 4x
  multisampled (resolved in tile at the swap), so it takes the fast path:
  the window side would save nothing there.

## Shape of the fix

### The window side

A content change carries the target's rectangles to the tree, and
`texture_content_changed` damages each rectangle mapped through the
showing leaf (its fit, its source rectangle, its transform; anything not
2d is the whole box) in place of the whole node. A full target dirt is
the whole node, as today. This is the part
[live-texture-content-damage](live-texture-content-damage.md) shares.
That item's index from texture id to the nodes showing it exists
(`texture_referencers`); what it has left is content latched on the
raster thread, after the frame's damage is resolved, and the fast path.
Measure first which devices take the fast path: on one that does, this
part buys nothing.

### Feed 2: declared by an app

Not built until one of the consumers below is decided. A core verb in
`@solidrt/core/gpu` states it: "this write changed the target only
inside these rectangles." The contract is explicit: pixels outside the
declared damage are taken as unchanged, and a wrong claim leaves stale
pixels, the trade every dirty-rect system makes. The force-full switch
(`/force-full-renders`) diagnoses a wrong claim as it does a wrong cull
box.

One constraint is settled, because the fallback rule depends on it: the
write itself must say it is covered. A buffer write dirties every target
that draws from the buffer, in full, at the write
(`RasterState::write_buffer`). A free-standing `damageTarget(id, rects)`
made after it could only downgrade that full dirt to rectangles, and the
downgrade would also cover whatever else dirtied the target before the
call: a camera write from another module in the same frame would be
narrowed to the sprite's rectangle. So a covered write is marked at the
write (it dirties its targets with no rectangle of its own), the
rectangles are declared per target (one buffer can feed several views),
and an unmarked write stays full. A covered write with no rectangle for a
target is the claim that nothing changed there. The rectangles enter the
same dirt model feed 1 uses (`DirtySet::damage`).

### Consumers

Each needs feed 2 and is its own decision; none is implied by the
mechanism.

- A record layer drawn directly (`cull` off), whose JS loop knows which
  records it rewrote. Or a list kept for the boxes alone, with no
  culling: the layer API's default to choose (D5 of the design).
- A tile layer re-bake of only the changed cells of a dirty chunk,
  instead of the chunk in full, so the bake budget covers more change per
  frame.
- A 3d shadow tile updated only where a dynamic caster moved. The tile is
  a sub-target, which the dirt model already covers; the rectangle is
  wiped and every caster that meets it redrawn. A cached map of the
  static casters to restore from (HDRP's cached shadows) is a different
  mechanism and not this one.
- Persistent textures driven by draw entries: fog of war, decals, paint
  layers, a minimap reveal.

## Known limits of what is built

- Vertex work multiplies by the rectangle count: each rectangle redraws
  the target's whole entry list under its scissor (at most 4
  rectangles). The better form is known: per rectangle, the sub-list of
  the records that meet it, queried from the cell index and drawn at an
  instance offset. Alloy has no instance-offset draw today. At 600
  records the whole-list redraw measures 0.22 ms more than a per-tile
  list on the SM-T500.
- A layer drawn directly (no `cull`) has no boxes, so every write renders
  in full (feed 2, or a list kept for the boxes).
- The wipe and the clear bytes are verified on desktop GL and on the
  Adreno 610, not on ANGLE.
- An owner holding a draw over the slicing floor (65,536
  vertex-instances) that the submission budget has not measured renders
  in full once, at its first change, so the budget can measure it.
- After a damage render the mip regeneration of a target with a chain
  still covers the whole target. A multisampled target renders in full,
  so no resolve follows a damage render.
- A suspected bug in the partial path, read from the code and not
  reproduced. On the explicit MSAA flavor (no
  multisampled-render-to-texture extension) `resolve_msaa` invalidates
  the samples after every pass. A partial render of a parent with
  `samples` then draws only the changed tiles over that storage, with no
  clear, and blits the whole surface into the texture, so the clean tiles
  take undefined pixels on a driver that honours the invalidate. Damage
  does not reach it (a multisampled pass renders in full), but the
  sub-target tile path does. Confirm with `alloy/examples/sub_target.rs`
  at `samples: 4` and a partial render; if it holds it is its own item,
  and the guard is the same rule damage uses.

## Open questions

- The shape of feed 2's verb, inside the constraint above: an option on
  the write verbs, or a bracket around a group of writes.
- Which devices take the window fast path, and so whether the window
  side pays anywhere a damaged target is the bottleneck.

## Done looks like

1. The window side: a damaged target damages only the corresponding
   part of the leaf that shows it, `damagePx` following the rectangle,
   measured on a device that does not take the fast path.
2. Feed 2, with a JS test over the verb, when a consumer for it is
   decided.
