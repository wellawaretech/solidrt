---
title: A sprite layer draws one pre-packed atlas, fixed at creation
description: Every sprite in a layer samples one texture chosen at creation, and createAtlas only decodes an already-packed sheet, so a second sheet costs a second full-size render target and runtime-supplied images have no way in at all.
created: 2026-08-22
---

# A sprite layer draws one pre-packed atlas, fixed at creation

Two symptoms, one root: the atlas is a single immutable texture, decided
before the layer exists.

**A second sheet is expensive out of proportion.** `createSpriteLayer` takes
one `atlas` and builds one pipeline target around it, so drawing from two
sheets means two layers - and a layer is not a draw, it is a full-size
offscreen texture, its own render pass, and its own composited `<texture>`
leaf in the tree. What should cost one extra draw call costs a second
canvas. Apps respond by cramming everything into one sheet, which is a real
constraint on how content is authored and a hard ceiling on texture size.

**Runtime images have no path in.** `createAtlas(bytes, ...)` decodes one
encoded sheet and slices it with `grid`/`namedFrames`. If the images arrive
while the app runs - downloaded, user-supplied, generated, decoded from a
document being edited - there is nothing to pack them into. That is the
normal case for a 2D-heavy application and the abnormal case for a game,
which is why v1 did not hit it.

## Cause

[packages/2d/src/layer.ts](../../packages/2d/src/layer.ts) binds
`textures: { uAtlas: atlas }` on the pipeline target at creation, and the
fragment stage samples exactly that one `sampler2D`.
[packages/2d/src/atlas.ts](../../packages/2d/src/atlas.ts) is
`decodeImage` + `createTexture`, one shot, no mutation.

## Proposed shape

Two independent pieces, in this order:

**1. Several atlases in one layer.** Add an atlas index to the record and
draw one range per atlas. Records already need an ordering pass if the sort
key lands ([2d-sprite-sort-key.md](2d-sprite-sort-key.md)), so grouping by
atlas within that order is nearly free - but the two interact and want
designing together: strict depth order across atlases forces a draw per
run, while grouping by atlas first is one draw per atlas and gives up
interleaved depth. The likely answer is "grouped by atlas, ordered within",
documented plainly, with the interleaved case being what a second layer is
for. A texture array binding is the other route and removes the batching
question entirely, at the cost of requiring same-size, same-format sheets.

**2. Packing at runtime.** A shelf allocator over an atlas texture:
`packImage(atlas, bytes)` decodes, finds a shelf, uploads the sub-region and
returns a `Frame`. Needs a texture sub-region upload in core - confirm
whether one exists before shaping this; if it does not, that is the first
step and it belongs in core rather than here.

Open before implementing: whether a full atlas grows (allocate bigger, re-pack,
re-emit frames - which invalidates every `Frame` an app is holding) or simply
fails, and whether frames should therefore be handles rather than plain UV
rects. That question is worth settling before the packer exists, because it
decides the public type.
