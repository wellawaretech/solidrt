---
title: Painted boxes land off the device pixel grid
description: Layout places every node at fractional logical positions (a 16.98 px row height puts rows at 45.71, 62.69, 79.67), and the composite walk draws them there, so a 1 px border, a divider, an icon and a snapshot boundary are resampled a fraction off; the text layer snaps itself today and that snap, and the boundary leftover, should fall out of one paint-time rule for axis-aligned boxes, as browsers do.
created: 2026-10-08
---

# Painted boxes land off the device pixel grid

## Symptom

Taffy hands back fractional positions and sizes, and the composite walk
(`alloy/src/rendertree/composite.rs`, the `builder.translate(pos)` per
child) draws every node at exactly that position. On the text coverage
probe at 1x the sample rows sit at y 45.71, 62.69, 79.67 (a line box of
16.98 px plus a 2 px gap), and Impeller resamples whatever is drawn there
with its fraction: step 5 of okf/plans/text-own-rasterizer.md found the
hinted text rows blurred by exactly this, measured off the window as an
x-height split 64/75 over two rows where the cell had it in one. The same
happens to everything else that is one pixel sharp by design and
axis-aligned: a 1 px border reads as two grey rows, a divider line, a
rasterized icon, and every snapshot boundary's texture, which carries the
blur to all of its content. The text layer now snaps its own quad
(`grid_shift` in `alloy/src/rendertree/text/mod.rs`): the layer's origin
is mapped through `ctx.to_window` to device pixels and the quad shifted
by the residual, under a plain translate-and-scale only. That is one
consumer doing what the walk should do once.

Browsers do this at paint time (Chrome's "pixel snapping" of layout rects
to device pixels, Firefox's snapping of axis-aligned transforms); Flutter
does not, and its 1x text and borders are known for it.

## Done looks like

An axis-aligned node is painted with its box's device origin on a whole
pixel, at every scroll offset and transform that is a translate and a
scale, and left alone under a rotation or a 3d transform, where there is
no grid to land on. Content moves by under a pixel, as in every browser.
A 1 px border at 1x is one solid row. The text layer's `grid_shift` and
the snapshot boundary's composite quad no longer snap on their own: the
walk's snap covers them. Hit testing and layout read the unsnapped boxes
(the snap is paint only), so carets, selection and pointer routing do not
change. The layout baseline, the text tests and the boundary examples
pass unchanged, and a new example reads a border's rows back solid.

The open design question is where the snap sits: on the per-child
translate of the composite walk (each node's device origin rounded, the
residual carried so a child's snap does not compound with its parent's),
or on the painted box only, with the translate kept exact for children.
The first matches what browsers do and keeps nested content on the grid;
the second is smaller. A text whose line boxes are fractional inside a
snapped node still needs the layer's own baseline rounding, which the
glyph quads already do.

## Involves

`composite.rs`: the device-origin rounding on the child translate, with
`ctx.to_window` carrying the residual; `boundary.rs`: the snapshot
boundary's composite quad on the same rule (today the "Text inside a
snapshot boundary" leftover, folded in here); `text/mod.rs`: `grid_shift`
retired once the walk covers it; `cull.rs` if the window map grows the
residual. A probe with a 1 px border, a divider and a text at fractional
positions, read off the window root crop (rows summed; a crisp edge is a
one-row step), on the 1x monitor and on the 1.5x tablet, where the grid
is the device's, not the logical one.
