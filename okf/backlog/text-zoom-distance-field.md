---
title: A text under a zoom stretches, then pops
description: The raster density policy for text is the layer-era model: under a scale animation a text draws its kept cells stretched and remakes them once the scale rests, so a zoom is soft and then sharpens in one frame. With per-glyph cells, a text in scale motion or at a large size could draw distance-field cells instead, which scale without a re-raster. A design question for after own-renderer stage 4.
created: 2026-11-19
---

# A text under a zoom stretches, then pops

Split out of [text-after-direct-draw](../done/text-after-direct-draw.md), whose
other items are cleanup; this one is a design decision.

## Symptom

The raster density policy in `alloy/src/rendertree/text/mod.rs`
(`raster_density`, the scale hysteresis of `LAYER_SCALE_MOTION_BOUND`,
the hinted remake at rest) is the layer-era model: a text under a scale
animation draws the cells for its kept scale, stretched, and re-makes
them once the scale rests, so a zoom is soft and then sharpens in one
frame. A layer was costly to re-raster, which is what the policy saved.
Since own-renderer stage 2 the glyph cells are per glyph and shared
through the text atlas, so the saving is smaller and the pop stays.

## Direction

A text in scale motion, or at a large size, draws distance-field cells,
which scale without a re-raster; the hinted mask cells come back at rest
for small sizes, where hinting matters. The MSDF cells exist already for
`flux:font` (`alloy/src/rendertree/text/glyphs/msdf.rs`).

## Open questions

- The cell kind per run: who decides, and from what (scale motion, the
  size at rest, both).
- A distance-field mode in the glyph program, kept minimal as stage 2
  taught on the Mali (one program, one branch, or two programs).
- The size where "large" starts, and whether it is a tuning constant or
  follows the atlas cell budget.
- The switch between the two kinds without a visible jump: the
  distance-field cell and the hinted mask differ in weight at small
  sizes, so a crossfade or a matched threshold is needed.

## When

After [own-renderer](../plans/own-renderer.md) stage 4 (paths), which
settles the program set the glyph mode has to fit in.
