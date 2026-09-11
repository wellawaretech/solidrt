---
title: 2D layer views, the additive half - tile-layer views, the layers bitmask, into tiling, per-view tint
description: A view of a sprite or record layer exists (2d-layer-views), but a tile map cannot be shown twice, a minimap cannot admit marker sprites only, several views cannot share one atlas target, and a view cannot tint itself apart from the layer. Each is an additive step on the landed view contract.
created: 2026-09-07
---

# 2D layer views, the additive half

## Symptom

[2d-layer-views](../done/2d-layer-views.md) landed `layer.createView` on
the sprite and record layers. What a full minimap over a tile-map game
still lacks, each a separate additive step on that contract:

- Tile-layer views. A `<TileLayer>` composites baked chunk targets as
  `d-texture` leaves inside one `<view>` carrying the camera transform;
  showing the same world twice is a second `<view>` over the SAME chunk
  textures under another camera - JSX only, no GPU work, no re-bake.
  Today the component owns its one `<view>`, so an app cannot ask for
  two.
- The `layers` bitmask. A markers-only minimap (the player, the
  objectives, no scenery) needs the selection vocabulary settled in
  [3d-view-mesh-selection](../done/3d-view-mesh-selection.md): `layers`
  on the sprite (default 1) and a mask on the layer and each view. On the
  instance-buffer model this is not entry attach/detach as in 3d: every
  view draws the one buffer, so a mask is a per-record field the vertex
  stage tests against a view param (`uLayers`), collapsing masked-out
  instances - one float in the style record, one uniform per target.
- `into` tiling: every view rendered into a rectangle of one app-owned
  draw target, one pass for all of them (`ViewOptions.into`/`x`/`y`,
  `view.setRect`), as 3d's views have.
- Per-view tint: today the layer's tint fans out to every view. A view
  that wants its own (a dimmed minimap) needs the 3d rule - names the
  view writes itself become view-owned and the layer's fan-out skips
  them.
- Fill mode. `<View2d width height>` is the only shape, so a second view
  cannot say "fill this box" and follow it at device density the way
  `<SpriteLayer>` does (both or neither, mount-fixed; `output` needs
  explicit sizes). Exactly the `<View3d>` half of
  [3d-scene-views-additive](3d-scene-views-additive.md), and the fill
  branch is shared code in both packages.
- `view.pick(x, y)`. The 3d view handle picks through ITS camera and
  size; the 2d one has `project`/`unproject` but no pick, so a view picks
  by composing `layer.pick(...view.unproject(x, y))` by hand. That
  composition is correct and one line, which is why this is last: the
  reason to add it is that the pointer walk already does exactly this
  internally, and the pair reads better with both halves on both
  handles.

## The goal these serve: a viewport over several layers

In Unity one Camera renders every sprite, tilemap and particle system of
its mask in one pass; in Godot one Viewport draws every CanvasItem of its
World2D under one Camera2D. Our view is per LAYER: a game with a tile
map, a sprite layer and a particle record layer needs three views under
one camera for one pane, composited by the app (`createCamera2d([a, b,
c])` keeps them in step). The parity item is a viewport over several
layers of any kind under one camera into one target - where tile-layer
views, `into` tiling and the layer mask below converge.

## Done looks like

A tile-map game shows a corner minimap: one tile-layer view at 1/16 zoom
under a sprite-layer view admitting the marker sprites only, both
composited from the same bakes and buffers, the minimap dimmed on its
own; several sprite-layer views for a split-screen sharing one atlas
target.

## Not in this item

Post effects on views, camera-driven chunk residency
([2d-baked-layers](2d-baked-layers.md)). The `<View2d>` component and a
layer without a view of its own (`<SpriteLayer output={false}>`, views
only) landed with [2d-layer-views](../done/2d-layer-views.md).
