---
title: Damage rectangles declared by an app (feed 2) - no consumer yet
description: A draw target re-renders only its damage rectangles (fed by culled draw lists) and the window follows them; what is left is a core verb for a write the core cannot derive, whose constraint is settled (the write itself says it is covered) but which no consumer needs today - the proposal is to build none until one of two named consumers exists, and the reasons each candidate falls away are recorded here.
created: 2026-11-05
---

# Damage rectangles declared by an app (feed 2)

Built so far:
[gpu-target-damage-draw-lists](../done/gpu-target-damage-draw-lists.md)
(the mechanism, feed 1 from culled draw lists, the facts, the force-full
switch) and
[gpu-target-damage-window](../done/gpu-target-damage-window.md) (the
window follows the rectangles through the node showing the target). The
decision and contract are in
[draw-order-and-culling](../design/draw-order-and-culling.md) D8, the
measurements in
[target-damage-measurements](../notes/target-damage-measurements.md).
This item is what is left: feed 2.

## Symptom

Only a culled draw list knows where its writes land. Every other write
re-renders its target in full: a layer drawn directly (a record layer
without `cull`), a shadow tile, a persistent texture driven by draw
entries.

## Shape of the fix

A core verb in `@solidrt/core/gpu` states it: "this write changed the
target only inside these rectangles." The contract is explicit: pixels
outside the declared damage are taken as unchanged, and a wrong claim
leaves stale pixels, the trade every dirty-rect system makes. The
force-full switch (`/force-full-renders`) diagnoses a wrong claim as it
does a wrong cull box.

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
same dirt model feed 1 uses, on both threads (`DirtySet::damage`,
`Context::note_content_dirt`), so the window side comes with it.

## Proposal: build nothing until a consumer exists

Each candidate was looked at on 2026-11-12. None needs the verb today.

- **A record layer drawn directly.** `cull: true` on the layer is the
  answer and it exists: the list then derives the rectangles itself, with
  no claim an app can get wrong. The record layer's default is `cull:
  false` for the loop that rewrites every record each frame (D5), and
  that loop gains nothing from damage either: every record changed. A
  layer where a few records change and that wants no culling is not a
  case anyone has.
- **A tile layer re-bake of only the changed cells.** Ruled out by how
  the bake works. Chunk targets are `render: "manual"`, and a re-bake
  goes into a retired target from the pool, never into the one the
  frames in flight sample
  ([2d-tile-world-frame-hitches](2d-tile-world-frame-hitches.md)): the
  target a bake draws into does not hold the chunk's last pixels, so
  there is nothing to keep outside a rectangle. The chunks also carry a
  mip chain, regenerated whole. A partial bake would need the copy
  forward of the old pixels that the pool was built to avoid.
- **A 3d shadow tile updated where a dynamic caster moved.** The one
  candidate with a real shape: the tile is a sub-target, the dirt model
  covers it, and the rectangle is the caster's bounds in light space,
  before and after. It is unmeasured: nothing says what share of a
  frame the shadow pass of a mostly static scene takes on the low-end
  baseline. It is its own item, measurement first
  ([3d-low-end-gpu-performance](3d-low-end-gpu-performance.md) is where
  that figure would come from), and it may be better served by a culled
  list in light space (feed 1) than by a declared rectangle.
- **Persistent textures driven by draw entries** (fog of war, decals,
  paint layers, a minimap reveal). No package offers one. The verb
  would come with that API, shaped by it.

So the trigger to build feed 2 is one of two: the shadow pass measured
as a bottleneck with a caster's light-space rectangle cheaper to state
than a list, or a persistent-texture API in a package. Until then the
constraint above is the whole design, and the verb is additive when it
comes (an absent declaration keeps meaning full).

## Known limits of what is built

- Vertex work multiplies by the rectangle count: each rectangle redraws
  the target's whole entry list under its scissor (at most 4
  rectangles). The better form is known: per rectangle, the sub-list of
  the records that meet it, queried from the cell index and drawn at an
  instance offset. Alloy has no instance-offset draw today. At 600
  records the whole-list redraw measures 0.22 ms more than a per-tile
  list on the SM-T500.
- A layer drawn directly (no `cull`) has no boxes, so every write renders
  in full (`cull: true`, above).
- The window follows the rectangles only where the window patches: a
  single-sample backbuffer, which is every desktop platform and no
  Android device. Under Recording boundaries it follows them where the
  node shows now, composed up the boundaries' frames. A node under a
  snapshot boundary or under a view with a blur filter, a target with a
  mip chain and a map that is not 2d damage the whole node (the
  outermost boundary or blurring view, under one).
- The window damage of a frame is one rectangle. A target keeps up to 4
  damage rectangles, but the window takes the bounding box of all that
  changed in the frame. Two sprites that change in opposite corners of a
  full-window view repaint nearly the whole window. This is the window
  damage model, not a gap of target damage.
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

- The shape of the verb, inside the constraint above: an option on the
  write verbs, or a bracket around a group of writes. Left for the first
  consumer to settle.

## Done looks like

Feed 2, with a JS test over the verb, when a consumer for it is decided;
or this item closed as not needed, its limits moved to the design
document.
