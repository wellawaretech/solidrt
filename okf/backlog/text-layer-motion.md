---
title: Text layers blur at fractional offsets and drop glyphs while scaling
description: Every <text> draws as a cached layer composited with bilinear sampling at its logical position, so text between device pixels (a momentum scroll, a translate animation, a 1.5x display) softens by up to half a pixel; and a scale animation (zoom on hover or focus) re-rasterizes every 2%, making a fresh set of cells at every intermediate size, which costs frame time on the TV and churns the atlas.
created: 2026-10-07
---

# Text layers blur at fractional offsets and drop glyphs while scaling

## Symptom

Every `<text>` is rasterized into a texture of its painted box and
composited as one `draw_texture_rect` with `TextureSampling::Linear` at the
node's logical origin (`Text::build_layer`,
`alloy/src/rendertree/text/mod.rs`). Two kinds of motion degrade it.

**Fractional device offsets.** Nothing snaps the composite to device
pixels, and the glyphs' subpixel phases are taken against the layer's own
origin (`glyph_quads`: `x0 = (run.x - box_origin.x) * scale`), not the
window's. Wherever the layer lands between device pixels the whole texture
is bilinear-resampled: crisp at .0, every stem spread over two pixels at
half contrast at .5. Cases: a ScrollView mid-momentum (the offset is an
eased transition, fractional every frame), a translate animation, any odd
logical coordinate on a 1.5x display. In motion the sharpness pulses with
the fraction; a scroll that comes to rest on a fraction stays soft until
the next move.

**Scale animation** (a button or a focused tile growing to 1.1 over some
150 ms, the usual remote-navigation focus idiom):

1. The layer re-rasterizes whenever the composite scale drifts past
   `LAYER_SCALE_TOLERANCE` (2%): every frame or two, some five
   intermediate sizes per animation.
2. `StyleKey` carries the exact device ppem (`font_size * scale`), so each
   intermediate size needs fresh cells for every glyph of the label.
3. A mask cell costs about 1 ms on the armv7 TV (0.4 on the tablet, 0.1 on
   the desktop), and every cell a frame draws is made in that frame
   ([text-complete-frames](../done/text-complete-frames.md)), so a label
   of twenty letters costs the TV some 20 ms of cell making per
   re-rasterization: the animation drops frames. (Before that decision the
   cells past a 3 ms budget went to the worker and the layer drew without
   them, so letters blinked out mid-animation instead.) The same holds for
   any re-rasterization at a new scale (a window moved to a display of
   another scale, a pinch zoom).
4. Each intermediate size is a style seen for the first time, so
   `TextAtlas::ensure` warms its printable ASCII at every phase: 285 cells
   per size, some 1400 per animation, over a second of worker time on the
   TV, held in the atlas for `EVICT_AFTER_FRAMES` (120).

On the desktop the scale case costs under a frame; on the TV it does not,
and on the tablet only just.

## Done looks like

Text at rest is crisp at any position and scale; motion never draws a
label with fewer glyphs than it had; a scale animation makes cells only
for the sizes text comes to rest at; scrolling text neither visibly steps
nor pulses.

## Involves

The cases pull in different directions and the choice is open.

Fractional offsets, three ways:

- **Snap** the layer's device origin while the composite transform is
  translate and scale only (rotation and skew keep `Linear`). Crisp every
  frame, the position off by at most half a device pixel: invisible at
  scroll speed, a visible one-pixel step in very slow motion (the tail of
  a momentum scroll, a slow drag), half a logical pixel at 2x. GPUI's
  vertical scrolling does exactly this
  ([gpui-text-rendering](../notes/gpui-text-rendering.md)).
- **Resample**, as today: the exact position, sharpness pulsing with the
  fraction, soft at rest on a fraction.
- **Re-rasterize at the true fraction**: crisp and exact, but the glyph
  pass runs every frame of a scroll for every visible text node (a render
  target switch each, on tiled GPUs), giving up the cache exactly when it
  matters; the vertical axis needs vertical phases in the cell key, which
  multiplies the atlas.

Scale animation:

- **Re-rasterize at rest**: while the composite scale differs from the
  previous frame's, composite the existing layer stretched, and
  re-rasterize once it holds for a frame. Chrome treats transform
  animations this way, rasterizing at the larger of start and end scale
  when it knows the end, so the animation minifies. No cell churn, no
  blinking, slightly soft text during the motion. This gives up "a
  zooming card must not smear", the reason the layer re-rasterizes on
  scale drift today.
- **Quantize in motion**: keep re-rasterizing, but round the device ppem
  to a coarse ladder while the scale changes, cells per rung.
- **Fields in motion**: draw from MTSDF cells while the scale changes and
  from masks at rest. Scale-free, but at 14 to 16 px the field draws
  visibly different glyphs, so the switch at rest pops.

Either way, warm-up would key on resting sizes only, and the cells of
the sizes passed through would never be made.
