---
title: World-space text for 2d layers (labels that ride the camera)
description: Text living IN a layer's world - node labels, cluster names, damage numbers - has no path: apps re-project laid-out <text> elements per camera change, which works for tens of labels and not at all for thousands; give the layer an atlas-text answer.
created: 2026-09-02
---

# World-space text for 2d layers (labels that ride the camera)

## Symptom

A canvas world needs text at world positions: cluster names on a map,
labels on a node editor's nodes, damage numbers over a game's sprites.
The layers draw atlas cells only, so today's answer is laid-out `<text>`
elements re-projected from JS - Relay bumps a signal in applyCamera and
16 label memos recompute `projectCamera` positions per camera change
(packages/2d/demos/src/relay.tsx). That is fine at 16 labels and
collapses at canvas-app scale: a thousand labeled nodes would mean a
thousand elements re-laid-out per camera move, exactly the per-element
cost the sprite layer exists to avoid. There is also no clipping tie-in
(labels overhang the layer's box unless the app clips) and no draw-order
interleaving with sprites (labels are always above the whole layer).

The baked-layers item already notes "bitmap fonts ride the same
machinery (glyphs are atlas cells)" for the TILE layer; this item is the
live-layer spelling of that thought, and the two should share whatever
glyph-atlas plumbing emerges.

## Shape

Two tiers, the first cheap and possibly sufficient for a long time:

- A glyph-atlas helper: rasterize a chosen font/size/weight set into an
  atlas at startup (the platform already shapes and rasterizes text;
  a dev-time bake through the existing text pipeline keeps metrics
  honest), returning per-glyph Frames plus advances. A
  `textSprites(layer, text, { x, y, ... })` helper then lays a string
  out as sprite records - text becomes ordinary sprites: camera, tint,
  groups, orderBy, removal all already work. Kerning/shaping quality is
  whatever the bake captured; ASCII-plus-latin coverage is fine for the
  target use (labels, HUD numbers), and the run's sprites can parent one
  group so a label moves as one handle.
- If crispness across a wide zoom range matters (labels readable from
  0.1x to 3x through one atlas), the bake becomes SDF/MSDF and the
  fragment stage learns one branch. Decide on evidence, not up front -
  the linear-sampled bitmap tier with a 2x-oversampled bake may already
  be acceptable under the layer's own oversample machinery.

Comparison: Three has no built-in either and the ecosystem converged on
troika-three-text (SDF, generated at runtime) - the gap pushes every
user to a third-party answer; Unity ships TextMeshPro (SDF) as the
standard; Godot's Label2D/Font system draws text as atlas quads in
world space natively. All three ended at atlas-quads-in-world-space,
which is exactly what the sprite layer already draws.

## Open questions

- Where does the bake run - createAtlas-style at app startup (simple,
  costs startup ms), or `srt bundle` time as a packaged asset?
- One shared glyph atlas per font/size, or pack multiple sizes and let
  zoom pick (mip-like)?
- Does the helper own updates (setText re-diffing glyph sprites) or stay
  build-once, replace-on-change (labels rarely mutate)?
- Interaction with 2d-atlas-limits.md: glyphs are the second atlas a
  layer wants alongside its art - the multi-atlas answer may gate this.
