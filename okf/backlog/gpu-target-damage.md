---
title: Declared damage for flush-rendered draw targets
description: A flush-rendered target re-renders in full when any of its inputs changes, however small the area the change affects, because dirtiness is tracked per input and nothing can say where on the target a change lands; the sub-target partial path already renders rectangles and keeps the rest, so the missing piece is one core primitive that declares the damaged rectangles of a target.
created: 2026-11-05
---

# Declared damage for flush-rendered draw targets

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

## What exists

Most of the mechanism is in core already. A sub-target
([gpu-subrect-draws](../done/gpu-subrect-draws.md)) renders into a
rectangle of its parent, and when only some tiles of a parent are dirty,
alloy renders the parent partially: no pass-level clear, each changed
tile wiped by a covering-triangle draw and drawn under viewport and
scissor, the clean tiles keeping their pixels. One pass either way.

It cannot serve this case. Every tile of a view over one layer binds the
layer's one instance buffer, so a record write dirties all of them and
the parent renders in full. Sub-targets have no manual render mode, so an
app cannot step only the tiles it knows changed. And tiling a view into a
grid of sub-targets just to get dirty rectangles would be the wrong
abstraction: the rectangles belong to the change, not to the target's
layout.

## Shape of the fix

One primitive in `@solidrt/core/gpu`: damage declared on a flush-rendered
draw target, for the next render. "These inputs changed, and the pixels
they affect lie only inside these rectangles." Core renders the target
through the partial path with the declared rectangles as the groups (the
target's own entries, under each rectangle's scissor, with the rectangle
wiped first), and keeps the rest.

The contract is explicit and the app states it: pixels outside the
declared damage are taken as unchanged. A wrong claim leaves stale
pixels, the trade every dirty-rect system makes. Core falls back to a
full render whenever it cannot honour the claim: the first render, a
resize, context loss, a change no damage was declared for (a params
write, an input written without damage), or damage covering more than a
threshold of the target.

This is a load on a flush-rendered target, which
[gpu-purity-decision](../done/gpu-purity-decision.md) gates: output must
not depend on how many times the flush ran. The sub-target partial path
passes that gate because a clean tile's pixels equal what re-rendering it
would produce, and declared damage passes it on the same argument, as
long as the claim is correct: the pixels outside the damage equal a full
render's, and rendering again after a render gives the same pixels. The
full-render fallbacks are the cases where no previous pixels exist to
keep. A wrong claim is the one way to break the invariant, and it is the
app's to make.

The term has a standard precedent: a Wayland surface's `damage_buffer`
and EGL's `swap_buffers_with_damage` declare which rectangles of a
surface's contents changed, the same claim one level down. Alloy's window
partial repaint already works in these terms (`damagePx`).

A target that knows its damage can also pass it on. A `<texture>` leaf
over a target whose content changed repaints the whole window today
([live-texture-content-damage](live-texture-content-damage.md)), because
the tree has no rectangle for a content change; a declared target damage,
mapped through the leaf's box, is exactly that rectangle. The two items
share the missing link and should be designed together.

## Consumers

Each one is its own decision; none is implied by the primitive.

- A 2d sprite or record view patched where records changed, the bounds
  of each written record old and new, through the view's camera.
- A tile layer re-bake of only the changed cells of a dirty chunk,
  instead of the chunk in full, so the bake budget covers more change per
  frame.
- A 3d shadow tile updated only where a dynamic caster moved, over the
  static casters' cached map (HDRP's cached shadows).
- Persistent textures driven by draw entries: fog of war, decals, paint
  layers, a minimap reveal.

## Open questions

- What a partial render costs on a tiler. The sub-target measurements
  found that a partial (no-clear) render loads the whole surface once on
  the Adreno 610, on top of the flat ~2.15 ms per pass; the fix there was
  rendering in full when every tile changed, so the cost of a small
  partial render against a full one was never measured on its own. If the
  load dominates, the primitive saves fragment work but not bandwidth,
  and pays off only for fill-heavy passes.
- MSAA targets. `loadOp: "load"` throws with `samples` today; keeping
  multisample storage across renders is expensive on a tiler. Full render
  only, or no damage on MSAA targets, needs a decision.
- Depth. A target with depth needs the damaged rectangles of depth wiped
  to the far plane too, as the tile-clear program does for tiles.
- Many rectangles. Merging into a coarse cell grid (a ring-shaped reveal
  should not dirty its empty middle) against one rectangle per change:
  where the merge lives, and the threshold for the full fallback.
- The composite. A damaged target saves its pass, not the window repaint
  of the leaf that shows it (4.3 ms a frame in the case above, a fifth of
  the stock-material frame); that share is saved only together with
  [live-texture-content-damage](live-texture-content-damage.md).
- The verb's shape: a call on the target (`damageTarget(id, rects)`) or a
  rectangle carried by the write that causes it. The first keeps every
  write verb as it is.

## Done looks like

1. Measured first, on the Adreno 610 with the ballast protocol from
   [gpu-subrect-draws](../done/gpu-subrect-draws.md): a parent target with
   a fill-heavy pass, one small tile changed, partial render against full
   render. That number decides whether the rest is worth building.
2. The primitive in core with its contract and fallbacks, verified by an
   alloy example (pixels inside the damage updated, outside unchanged, each
   fallback taken) and a JS test over the verb.
3. The window side settled with
   [live-texture-content-damage](live-texture-content-damage.md): a damaged
   target damages only the corresponding part of the leaf that shows it.
