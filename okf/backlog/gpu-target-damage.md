---
title: Damage rectangles for flush-rendered draw targets
description: A flush-rendered target re-renders in full when any of its inputs changes, however small the area the change affects, because a target is dirty or not and nothing says where on it a change lands; the sub-target partial path already renders rectangles and keeps the rest, and a draw list already knows which records a write touched and their boxes, so the missing piece is damage rectangles on a target's next render, derived by the draw list first and declared by an app only once a second consumer is decided.
created: 2026-11-05
---

# Damage rectangles for flush-rendered draw targets

## Symptom

A draw target re-renders when its entries or their inputs change, and
the unit of change is the input: a buffer, a param, a bound texture.
Nothing can say that a buffer write affects only part of the target, so
every write costs a full pass.

The case that shows it: a 2d record layer of about 600 sprites in a
full-window view on an Adreno 610 (SM-T500, display scale 1.5, target
2000x1200 `rgba8`, single-sampled, no depth). The view pass costs 35.5 ms
of GPU with a material that does real work per pixel, and 17.5 ms with
the same records under a stock `unlit()`; the window composite after it
adds about 4.3 ms a frame. A press that animates one sprite, under 1% of
the target, pays the whole pass on every frame of the animation: about
40 ms a frame (26 fps) with the real material, and 21.8 ms with the stock
one, still over a 60 Hz budget. More than 99% of each pass redraws pixels
that did not change, and no cheaper material brings it into budget on
this device.

The scope is a sparse change under a view that holds still. A pan, a
reveal of a few hundred writes a frame and a churn of every record stay
full passes: their damage is the target. Those are the scenarios
[draw-order-and-culling](../design/draw-order-and-culling.md) measures,
and this item changes none of their numbers.

## What exists

Three pieces are in the tree already. None of them carries a rectangle
from a change to a pass.

**The partial pass.** A sub-target
([gpu-subrect-draws](../done/gpu-subrect-draws.md)) renders into a
rectangle of its parent, and when only some tiles of a parent are dirty,
alloy renders the parent partially: no pass-level clear, each changed
tile wiped by a covering-triangle draw and drawn under viewport and
scissor, the clean tiles keeping their pixels. One pass either way
(`flush_dirty_within` in `alloy/src/raster/targets.rs`, `pass_groups` in
`alloy/src/gl/target.rs`).

**The draw list.** A culled or ordered population draws per view from a
draw list (D3 of
[draw-order-and-culling](../design/draw-order-and-culling.md)). At the
rebuild point the list knows the slots a write changed, each record's
cull box and its view (`ListEntry::refresh` in
`alloy/src/context/draw_list.rs`). A write to a record the view does not
draw already costs no render: the list is left as it was, its buffers
are not written and the target is not dirtied. A write to a drawn record
patches the list's own buffer, and that buffer write dirties the whole
target.

**The window damage.** A content change behind a texture id already
damages the box of every node that shows it
(`RenderTree::texture_content_changed`, resolved without a walk on the
reuse path by `resolve_reuse_damage`), and partial repaint patches the
window from it ([partial-repaint](../done/partial-repaint.md)). Two
limits: the damage is the whole leaf, so a full-window leaf damages the
whole window; and a window whose default framebuffer is multisampled
takes the fast path, which never patches (`window_fast_path`,
`DamageTracker::route`).

What is missing is the rectangle itself. A target is dirty or it is not,
on both threads: `RasterState::dirty` and `Context::content_changes` are
sets of ids.

Tiling a view into a grid of sub-targets does get dirty rectangles
today, for a population with a draw list: each tile has its own list, so
a write dirties only the tiles that draw the record. For a layer drawn
directly (a record layer without `cull`) every tile binds the layer's
one instance buffer and a write dirties them all. The grid serves as a
measurement rig (Done looks like, step 1). It is the wrong shape to
ship: a list, a cull and a camera per tile, and the rectangles belong to
the change, not to the target's layout. Sub-targets have no manual
render mode either, so an app cannot step only the tiles it knows
changed.

## Shape of the fix

### The mechanism, in alloy

A target's pending dirt is full or a set of rectangles, on both threads.
Every dirtying that exists today stays full. Rectangles arrive through
one of the two feeds below, union until the target renders, and the
render consumes them.

A target whose pending dirt is rectangles renders through the partial
path with the rectangles as the groups: no pass-level clear, each
rectangle wiped by the tile-clear draw, then the target's own entries
under the rectangle's scissor. The viewport stays the target's, where a
tile group sets its own: a damage group is the same picture cut down,
not a second picture. Everything outside the rectangles keeps its
pixels.

Core renders in full whenever it cannot keep them: the first render, a
resize, context loss, any dirtying that carried no rectangle (a params
write, an entry or order change, a buffer written without damage), a
source of the target that re-rendered, or damage covering more than a
threshold of the target.

This is a load on a flush-rendered target, which
[gpu-purity-decision](../done/gpu-purity-decision.md) gates: output must
not depend on how many times the flush ran. The sub-target partial path
passes that gate because a clean tile's pixels equal what re-rendering it
would produce, and damage passes it on the same argument, as long as the
rectangles are right: the pixels outside them equal a full render's, and
rendering again after a render gives the same pixels. Rectangles union
until a render takes them, so a flush that ran zero times or twice ends
at the same pixels. The full-render fallbacks are the cases where no
previous pixels exist to keep.

The term has a standard precedent: a Wayland surface's `damage_buffer`
and EGL's `swap_buffers_with_damage` declare which rectangles of a
surface's contents changed, the same statement one level down. Alloy's
window partial repaint already works in these terms (`damagePx`).

### Feed 1: derived by the draw list

The first consumer needs no claim from the app. For a list whose view
held still, the damage of a frame's writes is the boxes of the slots
they touched, old and new, through the view, grown by a pixel for the
edge. That covers a patch and a rebuild alike: a record whose key
changed draws over or under different neighbours, and only inside its
own box; a slot that joined or left the population changes its own box.
A view change stays a full render.

The trust is the cull box's, which culling already takes: a vertex stage
that moves geometry past its declared `cullMargin` is culled early at a
view's edge today, and would leave stale pixels here. The failure is
easier to see, the contract is the same one.

It needs:

- The old box. `Mirror::write_boxes` and `refresh_boxes` overwrite it;
  the box a changed slot had at the last rebuild point has to be kept
  until the next.
- The list's own write not to count as a full dirtying. The list issues
  the buffer write and the damage together, so the pairing is internal.
- A list. A record layer draws its buffers directly unless `cull` is on,
  and a directly drawn layer has no mirror and no boxes. Whether damage
  is its own opt-in on the layer (a list kept for the boxes, with no
  culling) or comes with `cull` is the layer API's default to choose
  (D5).

### Feed 2: declared by an app

Not built until one of the consumers below is decided. A core verb in
`@solidrt/core/gpu` states it: "this write changed the target only
inside these rectangles." The contract is explicit: pixels outside the
declared damage are taken as unchanged, and a wrong claim leaves stale
pixels, the trade every dirty-rect system makes.

One constraint is settled now, because the fallback rule depends on it:
the write itself must say it is covered. A buffer write dirties every
target that draws from the buffer, in full, at the write
(`RasterState::write_buffer`). A free-standing `damageTarget(id, rects)`
made after it could only downgrade that full dirt to rectangles, and the
downgrade would also cover whatever else dirtied the target before the
call: a camera write from another module in the same frame would be
narrowed to the sprite's rectangle. So a covered write is marked at the
write (it dirties its targets with no rectangle of its own), the
rectangles are declared per target (one buffer can feed several views),
and an unmarked write stays full. A covered write with no rectangle for
a target is the claim that nothing changed there.

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

## Consumers

The first is this item's reason and is fed by feed 1. The others need
feed 2; each is its own decision, and none is implied by the mechanism.

- A 2d sprite or record view patched where records changed.
- A tile layer re-bake of only the changed cells of a dirty chunk,
  instead of the chunk in full, so the bake budget covers more change per
  frame.
- A 3d shadow tile updated only where a dynamic caster moved. The tile
  is a sub-target, so this one needs damage on a tile defined (the
  parent's partial pass, the tile's group cut to the rectangle). The
  rectangle is wiped and every caster that meets it redrawn; a cached
  map of the static casters to restore from (HDRP's cached shadows) is a
  different mechanism and not this one.
- Persistent textures driven by draw entries: fog of war, decals, paint
  layers, a minimap reveal.

## Open questions

- What a partial render costs on a tiler. The sub-target measurements
  found that a partial (no-clear) render loads the whole surface once on
  the Adreno 610, on top of the flat ~2.15 ms per pass; the fix there was
  rendering in full when every tile changed, so the cost of a small
  partial render against a full one was never measured on its own. If the
  load dominates, the mechanism saves fragment work but not bandwidth,
  and pays off only for fill-heavy passes.
- MSAA targets. `loadOp: "load"` throws with `samples` today; keeping
  multisample storage across renders is expensive on a tiler. Full render
  only, or no damage on MSAA targets, needs a decision.
- A suspected bug in the partial path that this would inherit, read from
  the code and not yet reproduced. On the explicit MSAA flavor (no
  multisampled-render-to-texture extension) `resolve_msaa` invalidates
  the samples after every pass. A partial render of a parent with
  `samples` then draws only the changed tiles over that storage, with no
  clear, and blits the whole surface into the texture, so the clean
  tiles take undefined pixels on a driver that honours the invalidate.
  Nothing guards it, and `alloy/examples/sub_target.rs` runs at
  `samples: 1`. Confirm with that example at `samples: 4` and a partial
  render; if it holds it is its own backlog item, and the answer to the
  question above is probably the same guard.
- The costs that stay whole-surface. After a partial render the MSAA
  resolve and the mip regeneration of a target with a chain still cover
  the target.
- Depth. A target with depth needs the damaged rectangles of depth wiped
  to the far plane too, as the tile-clear program does for tiles.
- Many rectangles. Merging into a coarse cell grid (a ring-shaped reveal
  should not dirty its empty middle) against one rectangle per change:
  where the merge lives, and the threshold for the full fallback.
- The composite. A damaged target saves its pass, not the window repaint
  of the leaf that shows it (4.3 ms a frame in the case above, a fifth of
  the stock-material frame). That share is saved only by the window side
  above, and only where the window is not on the fast path; which route
  the SM-T500's window takes is not recorded.
- The shape of feed 2's verb, inside the constraint above: an option on
  the write verbs, or a bracket around a group of writes.

## Done looks like

1. The case reproduced in a repo probe and measured first, on the Adreno
   610 with the ballast protocol from
   [gpu-subrect-draws](../done/gpu-subrect-draws.md) and the governor
   and thermal state recorded: a view tiled into sub-targets over a
   culled layer, a fill-heavy material, one sprite animating, partial
   render against full render. No new code in alloy. That number decides
   whether the rest is worth building.
2. The mechanism in alloy with its fallbacks, fed by the draw list,
   verified by an alloy example (pixels inside the damage updated,
   outside unchanged, each fallback taken). Facts per target in `/gpu`:
   partial and full renders, damaged pixels. A debug switch that forces
   full renders, so stale pixels that go away under it name a wrong
   rectangle.
3. The window side: a damaged target damages only the corresponding part
   of the leaf that shows it, `damagePx` following the rectangle.
4. Feed 2, with a JS test over the verb, when a consumer for it is
   decided.
