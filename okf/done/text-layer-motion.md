---
title: Text re-rasterizes at every step of a scale animation, or never inside a boundary
description: A text layer rasterizes at the composite scale and re-rasterizes whenever that scale drifts past 2 percent, so a zoom to 1.1 makes fresh cells at five intermediate sizes and a breathing title makes them every frame; a text inside a recording boundary under the same zoom is never re-rasterized at all and stays stretched at rest. The raster scale should follow rest, as browsers do: composite the existing raster stretched while the scale moves, re-rasterize once it holds.
created: 2026-10-07
completed: 2026-10-09
---

# Text re-rasterizes at every step of a scale animation, or never inside a boundary

## Symptom

Every `<text>` is rasterized into a texture of its painted box at the
composite scale (the display scale times the ancestors' scale, read off
the grid map in `Text::build_layer`, `alloy/src/rendertree/text/mod.rs`)
and composited as one quad. A `scale` write on a view is `Damage::Compose`
(`kinds/view.rs`), which means two different things for a text below it:

- **Under a plain view** the subtree is rebuilt every frame of the
  animation, and the layer re-rasterizes whenever the composite scale
  drifts past `LAYER_SCALE_TOLERANCE` (2 percent): a focus zoom to 1.1
  over 150 ms re-rasterizes some five times, a breathing title every
  frame. `StyleKey` carries the exact device ppem, so each size needs
  fresh cells for every glyph (about 1 ms each on the armv7 TV, 0.4 on
  the tablet, 0.1 on the desktop), and every cell a frame draws is made
  in that frame ([text-complete-frames](../done/text-complete-frames.md)),
  so a twenty-letter label costs the TV some 20 ms per step: the animation
  drops frames. Each intermediate size is a style seen for the first time,
  so its ASCII is warmed too, hundreds of cells per step held in the atlas
  for the eviction age.
- **Under a recording boundary** the recording replays with the new
  matrix (`composite.rs`, the `BoundaryMode::Recording` arm) and the text
  inside is never rebuilt: stretched during the motion, and stretched at
  rest for good if the motion rests at a scale other than the recorded
  one. The stock Button is this shape (a boundary with a press scale of
  0.97 on a spring) and gets away with it because it returns to the
  recorded scale; a focus zoom that rests at 1.1 would not.

The fractional-offset half this item once carried (a momentum scroll, a
translate animation, odd coordinates at 1.5x softening the layer) landed
with [pixel-snapped-paint-boxes](../done/pixel-snapped-paint-boxes.md):
the layer snaps its quad through the shared grid shift, scroll offsets and
translates snap at composite time, and a detached text snaps the same way
since the shift sits in the text's own build.

## Decision

The raster scale follows rest, as Chrome treats transform animations. One
rule, applied at the two places a text's density is decided:

- A new raster is made when the composite scale has held for three
  consecutive looks (`SCALE_REST_LOOKS`, 50 ms at 60 Hz, past the gap of
  a writer at half the refresh rate) and differs from the raster scale
  at all: a raster is for exactly the scale it rests at, no tolerance.
  While it moves, the existing raster is composited stretched. One bound
  in motion: a ratio past `LAYER_SCALE_MOTION_BOUND` (1.5, either way)
  re-rasterizes at once, so softness is capped (Chrome's pinch rule uses
  2). A content change mid-motion rebuilds at the raster scale the layer
  already has, so the cells stay stable.
- A known end is rastered for at once. A native scale transition knows
  its target; the paint walk carries the chain's product of target over
  current (`BuildContext::scale_target`, from
  `RenderTree::scale_transition_factor`), and a raster that would be
  magnified toward that end is made for the end on the first look and
  minified during the motion, as Chrome rasters a known animation at its
  larger end. Headed down, the raster is kept and minified, and re-made
  at the resting scale. A per-frame write has no end and keeps the
  hysteresis. (`text::raster_density`, the one rule all three sites use.)
- "Held" needs looks at the frames after the last write, which an idle
  app never paints: a text, a recording or a snapshot that decides to
  wait registers with the tree, which gives it `Damage::Compose` at the
  next frame start (caches intact, a frame requested), so it is looked at
  again and rasters once the scale held, or waits again if it moved. A
  node not looked at last frame starts its count over.
- A raster made for a scale the text rests at is hinted: at rest, at the
  display scale, or at a known end. One made mid-motion at an in-between
  scale is not (its x-height would pop between rows as the size moves),
  keeps its hinting while the scale moves, and is re-made hinted once at
  rest.
- The text layer applies the rule in `build_layer` from its own
  observation of the composite scale per build.
- A recording boundary applies the same rule at replay against the
  density its text layers were made for, when it holds any (the record
  walk counts them, nested recordings included). Re-made, the cache is
  dropped and the subtree re-recorded with the walk told the scale is at
  rest when it is, since the texts inside have no observation of their
  own from the replayed frames. A vector-only recording never re-records
  for scale.
- A snapshot boundary rasterizes at the same density (the grid inside its
  own matrix, followed by the same rule), where it took the display scale
  alone and was resampled under any scale on the chain. This closes
  [snapshot-raster-composite-scale](../done/snapshot-raster-composite-scale.md).
- Warm-up stays at the display scale only. Rotation and 3d transforms
  have no grid and keep the display-scale raster, unchanged.

Rejected: quantizing the ppem to a ladder in motion (still cells per
rung, still a pop at each), and drawing from distance-field cells in
motion (at 14 to 16 px the field draws visibly different glyphs, so the
switch at rest pops).

## Done looks like

A focus zoom to 1.1 rasterizes its label once, for its end as it starts,
instead of five times on the way, and is crisp throughout; a breathing
title never re-rasterizes; a Button press never. A text inside a
recording boundary or a snapshot boundary that rests at another scale is
re-rasterized there, crisp and hinted. Pinned by
`alloy/src/tests/text_motion.rs`: the rule over a ramp with and without
a known end, the three-look rest, and a recording under a scale write
that replays while the scale moves and re-records once it holds. Read
live on the desktop through `window.textLayers` of the stats; the TV
read of the focus idiom is an `okf/tiny.md` line beside the cold-style
read from text-complete-frames.

## Findings

What holds without this plan was cut into
[text-rasterizer-findings](../notes/text-rasterizer-findings.md) (rest
takes three looks, a recording replays its text without rebuilding it,
the desktop read, what `textLayers` counts, the snapshot walk told it is
at rest). What stays here is the record:

- Rejected on the way: a one-look rest (a paused clock holds every scale,
  and a writer at half the refresh rate rests between its writes), and a
  2 percent tolerance at rest (a text resting at 1.015 stayed stretched;
  the held-epsilon covers float noise, so rest means exactly that scale).
