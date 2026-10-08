---
title: Painted boxes land off the device pixel grid
description: Layout places every node at fractional logical positions (a 16.98 px row height puts rows at 45.71, 62.69, 79.67), and the composite walk drew them there, so a 1 px border, a divider, an icon and a snapshot boundary were resampled a fraction off; now the walk snaps every axis-aligned box to the device grid at paint time, origin and far edges, as browsers do, and the text layer's and the boundaries' own snapping fell out of that one rule.
created: 2026-10-08
completed: 2026-10-08
---

# Painted boxes land off the device pixel grid

## Symptom

Taffy hands back fractional positions and sizes, and the composite walk
(`alloy/src/rendertree/composite.rs`, the `builder.translate(pos)` per
child) drew every node at exactly that position. On the text coverage
probe at 1x the sample rows sit at y 45.71, 62.69, 79.67 (a line box of
16.98 px plus a 2 px gap), and Impeller resamples whatever is drawn there
with its fraction: step 5 of okf/plans/text-own-rasterizer.md found the
hinted text rows blurred by exactly this, measured off the window as an
x-height split 64/75 over two rows where the cell had it in one. The same
happened to everything else that is one pixel sharp by design and
axis-aligned: a 1 px border read as two grey rows, a divider line, a
rasterized icon, and every snapshot boundary's texture, which carried the
blur to all of its content. The text layer snapped its own quad
(`grid_shift`, mapping the layer's origin through `ctx.to_window`), one
consumer doing what the walk should do once.

Browsers do this at paint time (Chrome's "pixel snapping" of layout rects
to device pixels, Firefox's snapping of axis-aligned transforms); Flutter
does not, and its 1x text and borders are known for it.

## What was built

One rule, in the paint walk (`alloy/src/rendertree/grid.rs`, applied in
`composite::record_node`): under a chain that is a translate and a
positive scale, every laid-out child's box is snapped to the device grid
of the target being rasterized, its origin AND its far edges each to the
nearest whole device pixel, and the subtree is placed there. Neighbours
keep sharing an edge (both round the same device coordinate), a box's
painted size differs from its layout size by under a pixel, and content
moves by under a pixel, as in every browser. Under a rotation or a 3d
transform nothing snaps: there is no grid to land on.

- The grid is a second map beside the window map: `BuildContext::grid`,
  the transform from the walk's current frame to the device pixels of the
  target being rasterized. The window's through the display scale; inside
  a snapshot raster or a capture, that texture's (`grid::raster`), reset at
  the raster root. The window map stays the damage side's and keeps
  describing the window inside a raster, which the grid must not.
- The snapped box is the walk's `ctx.size`: every box painter reads it
  (fills and strokes of the box kinds, the overflow clip, the snapshot
  crop and texture dims, the backdrop bounds, the design-size fit, the
  own matrix's center), so the four edges agree. The content box stays
  the layout's: text and inline atoms are placed fractionally inside the
  snapped box, as browsers place text runs.
- A View's own matrix (its translation) and its scroll offset snap the
  same way, in the inline path and in the composite-time hoists alike
  (`snapped_own`, `snapped_scroll`; the hoisted matrix is carried in
  `Hoist`). A recording boundary's interior is therefore recorded against
  a grid-aligned box frame and replays on the grid at every scroll offset
  and after every transform write, without re-recording. A translate
  animation steps in whole device pixels, as scroll does and as Firefox
  treats axis-aligned transforms.
- The text layer's `grid_shift` and `composite_scale` went: the layer
  reads its raster density from the grid map and snaps its quad (a
  painted box like any other, at the fractional slack inside the node)
  with the shared `grid::shift`. A snapshot boundary's shader outset is
  whole device pixels, rounded up.
- Hit testing and layout read the unsnapped boxes; the snap is paint only.
  A node capture and the window now agree on the grid (both place the
  node's origin on a whole pixel), which closes the tiny.md item about
  node snapshots misleading a crispness read.

Verified: the unit tests (`src/tests/grid.rs`, the walk test in
`src/tests/composite.rs`), the boundary and capture examples unchanged,
and `alloy/examples/pixel_snap.rs`: a scene of dividers and stroked
boxes at fractional rows, under a 0.3 scroll, a 0.5 translate, a
recording and a snapshot boundary, rendered at 1x and 1.5x, holds no
partially covered pixel at all, and a cache-hit frame equals the frame
that filled the caches.

## Findings

- A box snap alone leaves the far edges soft: a 16.98 px row's bottom
  border at origin + 16.98 is still two grey rows. Snapping each edge
  independently (Chrome's PixelSnappedIntRect) is what makes borders
  crisp on all four sides, and it costs nothing in seams since adjacent
  boxes round the same coordinate.
- The window map and the grid are different things. Reusing the damage
  map for the snap works for the window but is wrong inside a raster,
  where the grid is the texture's; a snapshot under a rotated ancestor
  would snap its interior to a grid it never composites onto.
- Snapping the hoisted ops is what makes boundaries work. With the matrix
  and scroll snapped at composite time against the current grid, a
  recording's interior (snapped at record time against a grid-aligned
  frame) is on the grid at every replay. Without it, every scroll of a
  recording boundary would land its content off the grid again.
- A stroke's inner edge is not the box's business: a 1 logical px stroke
  at 1.5x is 1.5 device pixels and its inner edge sits on a half pixel
  whatever the box does. Border widths on the grid are a separate
  decision (okf/backlog/border-width-device-pixels.md).
- Detached (d-*) nodes are not snapped: they have no layout box, draw
  their own geometry where it says, and are the animation lane.

## Not done here

- Snapshot boundaries still rasterize at the display scale only, so under
  a design-size fit or a scale transform the texture is resampled
  whatever the quad lands on: okf/backlog/snapshot-raster-composite-scale.md.
- Border widths at fractional scales: okf/backlog/border-width-device-pixels.md.
