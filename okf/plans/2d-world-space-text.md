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
elements re-projected from JS: a canvas app bumps a signal when it applies
its camera and a memo per label recomputes its `projectCamera` position on
every camera change. That is fine at a dozen labels and
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
  costs startup ms), or `sol bundle` time as a packaged asset?
- One shared glyph atlas per font/size, or pack multiple sizes and let
  zoom pick (mip-like)?
- Does the helper own updates (setText re-diffing glyph sprites) or stay
  build-once, replace-on-change (labels rarely mutate)?
- The glyph atlas is the second atlas a layer wants alongside its art;
  since [2d-atlas-limits](../done/2d-atlas-limits.md) a layer declares
  several atlases and draws them in one batch, so a glyph sheet is one
  more entry in the layer's `atlases` and text sprites are ordinary
  sprites of the same layer.

## Decision (2026-10-06)

Re-examined before building. The premise above overstates the gap: a
label does not need a projection per camera change. Put `d-text` labels
at world positions under one `<view>` carrying the camera transform
(what `<TileLayer>` does for its chunks: origin at the camera point,
rotate and scale there, the camera point translated onto the pivot) and
a camera move is four property writes on the container, no layout, no
per-label JS, with the engine rasterizing every glyph at the real
on-screen scale. That is the right answer into the hundreds of labels
and it is the higher-quality one. The relay demo's per-label memo was
the demo's choice; it migrates to the container on the user's go.

What remains is the case past that: thousands of live labels (an element
is about 15 KB and 0.65 us of paint; a sprite is 64 bytes and no JS), a
label that must interleave in the layer's draw order, and a zoom
animation that would re-rasterize hundreds of labels per frame. For that
case the answer is the SDF tier, built properly, over the glyph engine of
[text-own-rasterizer](text-own-rasterizer.md) stage 1 rather than a bake
through `captureSnapshot`: an MTSDF atlas generated in the engine from
glyph outlines, which Impeller does not expose, so the capture route
would have been the Impeller-era stopgap and the only part thrown away
when text rendering is replaced.

## State (2026-10-06)

| Step | State |
|---|---|
| 1. Shader and records | built: `Atlas.sdf`, per-atlas field decode with outline, style 16 floats, raw record 20; `tests/text.test.tsx` pins a disc field's edge at 2x and the outline ring |
| 2. `createSpriteFont` | built over `flux:font`; default cells "mask" until the generator lands (then "msdf") |
| 3. Layout | built, `text-layout.ts`, `tests/text-layout.test.ts` (9) |
| 4. Runs and `<Text2d>` | built; `tests/text.test.tsx` (5) on the headless client |
| 5. Example, docs | built: `examples/text.tsx`, AGENTS.md section and traps, READMEs |
| The relay demo's labels | not migrated (the demos are untracked; on the user's go) |

## Plan (started 2026-10-06, after stage 1 of the engine)

1. **Shader and records.** `Atlas` gains `sdf?: { range }`; the
   generated fragment stage decodes an MTSDF atlas in that atlas's branch
   (median of rgb for the edge with derivative-based anti-aliasing, the
   alpha field for the outline); the style record grows from 12 to 16
   floats and the raw record from 16 to 20 for a per-sprite outline
   (rgb plus width in world pixels, the vertex stage scaling it by the
   camera zoom). A mask atlas needs no shader change: coverage in alpha,
   white premultiplied, drawn by the existing stage.
2. **`createSpriteFont(font, opts)`** over `flux:font`: a handle from
   frame zero whose atlas a layer declares at creation; ASCII warms up;
   a run needing other glyphs draws them blank at the right advance and
   is re-framed when the cells land; growth re-stamps the font's frames
   and re-frames its own runs (glyph frames never leave the font);
   `font.ready()` awaits a quiet font.
3. **Layout** (`text-layout.ts`, pure, headless-tested): placements
   from the prepared units' glyphs, `\n` and `maxWidth` through
   `layoutNextLine`, per-line align, `letterSpacing`, `lineHeight`, x
   anchor start/middle/end as d-text, y anchor top/middle/baseline/
   bottom.
4. **Runs** (`text.ts`): `addText` / `setText` / `destroyText`, a run is
   one group holding one sprite per glyph, scaled by size over the
   atlas's size per em, so camera, tint, `orderBy`, picking, transitions
   and bubbled pointer events already work; outline per run. Node layer
   only. `<Text2d>` beside `<Sprite>` and `<Group>`.
5. Tests (`tests/text-layout.test.ts`, `tests/text.test.tsx`: a glyph
   drawn at two zooms pins edge sharpness and the outline colour), an
   example, the AGENTS.md section and traps, the READMEs.

Deliberately out, all additive: shadow and glow (a second run), a
per-run screen-size clamp, a faux-bold dilation knob.

## Findings
