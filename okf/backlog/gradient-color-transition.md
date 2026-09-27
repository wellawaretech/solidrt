---
title: Animate gradient colors
description: A gradient-valued `color` never animates (the color lane reads a gradient as not animatable), so a lit fill snaps on a theme switch where a solid fill cross-fades; interpolate gradients that share a kind and stop count, stop colors in oklab and positions linearly, and keep snapping for the rest.
created: 2026-09-27
---

# Animate gradient colors

`Element::anim_value` (alloy/src/rendertree/transitions.rs) answers None
for the color property when the paint holds a gradient, and a gradient
write to a running color track cancels it. So an element whose fill is a
gradient snaps every color change. The lit components make every face a
sheen gradient, and there a theme switch snaps the faces while flat ones
cross-fade beside them. The prototype reports this from the docs, not from
a measurement: verify first.

## Done looks like

- Two gradients of the same kind (linear/radial/...) and stop count
  animate: each stop's color in oklab with alpha as its own lane (as solid
  colors do), stop positions and the gradient's geometry linearly.
- A gradient against a solid color, or gradients of different shapes,
  snap as today, documented on `color`.
- A test in alloy/src/tests/transitions.rs for a two-stop linear fade.

## Involves

- A gradient lane kind in `AnimValue`/`AnimKind`: lanes are a fixed
  `[f32; 8]` today; a gradient needs 4 per stop plus geometry, so either
  a capped stop count or a `Vec` track (the spatial arena's node tracks
  already use `Vec<f32>` lanes, see alloy/src/spatial/transitions.rs).
- `PaintState.gradient` read and write in `anim_value`/`set_anim_value`.
- flux `set_property`'s color arm decoding a gradient object into the
  target (it hands only strings and packed numbers to the color lane now).
