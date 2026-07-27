---
type: backlog-item
title: Backdrop filters through Impeller (blur, glass)
description: save_layer already takes a backdrop ImageFilter and we already call it with None; wiring Impeller's built-in blur/dilate/erode/matrix filters gives frosted panels with correct see-through semantics, no GLSL and no root layer.
status: open
timestamp: 2026-07-27T00:00:00Z
---

# Backdrop filters through Impeller (blur, glass)

A frosted panel blurring what is behind it is reachable today with a small
amount of plumbing, and is not the same feature as
okf/plans/root-layer-effects.md. That plan gives custom GLSL over the whole
window; this gives Impeller's own filters over a region, with the backdrop
semantics that a subtree effect fundamentally cannot provide (a subtree's
texture does not contain the pixels behind it).

## What is already there

- `DisplayListBuilder::save_layer(bounds, paint, backdrop:
  Option<&ImageFilter>)` in impellers 0.4.2. We call it at
  alloy/src/rendertree/composite.rs:501 for a non-boundary view's group
  opacity, passing `None` for the backdrop. The feature is one argument away
  at a call site that already exists.
- `ImageFilter::new_blur(x_sigma, y_sigma, tile_mode)`, `new_dilate`,
  `new_erode`, `new_matrix`, plus `Paint::set_image_filter`,
  `set_color_filter` and `set_mask_filter` for filters applied to a drawn
  element rather than to what is behind it.
- None of these need `impellerc`. They are built-in filters, not fragment
  programs, so the toolchain question that rules out custom Impeller shaders
  (see the plan's Deferred section) does not arise.

## What to work out

- **Where it hangs in the props.** A view-level prop naming a filter, in the
  vocabulary the rest of the API uses (a blur radius in points, not a
  gaussian sigma in pixels). The nearest existing model is the paint
  properties work in flux/src/plugins/gui/properties/.
- **Interaction with repaint boundaries.** A save_layer with a backdrop
  reads the current target, so a boundary between the panel and its intended
  backdrop puts them on opposite sides of an offscreen surface. This is the
  same trap already documented for blend modes inside a snapshot boundary
  (see texture-element-compositing.md) and needs the same warning, or a
  rule that refuses the combination.
- **Cost.** A backdrop filter forces an offscreen and a read of the current
  target at that point in the display list, so it is not free the way a
  blend mode is. Region-sized, but worth measuring on Android before
  offering it as a casual style prop.
- **Bounds.** save_layer takes explicit bounds; the natural choice is the
  node's layout box, but a blur samples beyond its own edge, so the tile
  mode and the bounds together decide what the panel edge looks like.
