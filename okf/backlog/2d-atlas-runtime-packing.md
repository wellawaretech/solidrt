---
title: Images that arrive while the app runs have no way into a sprite layer
description: A layer draws from the atlases it declares and createAtlas uploads one finished sheet, so an image downloaded, user-supplied or generated at runtime cannot become a frame without its own atlas, and sixteen atlases is the layer's whole budget; a packer that allocates runtime images into a few pages needs a sub-rect texture upload in core first.
created: 2026-10-06
---

# Images that arrive while the app runs have no way into a sprite layer

## Symptom

A photo wall, a document editor, a map with user pins: the images are not
known when the sheet is authored. Today each one is its own `createAtlas`
and the layer's `atlases` list is fixed at creation and capped at
`limits.maxTextureUnits` (16), so an app either recreates the layer per
batch of images or gives up on the sprite layer for them. A game never
hits this; a 2D-heavy application hits it first.

## Cause

Two primitives are missing, one in core and one in the package:

- Core's `uploadTexture(id, data, offset)` replaces whole frames; its
  `offset` selects a frame in the SOURCE buffer, not a destination
  rectangle. A packer that places a decoded image into a region of a
  page needs a destination-rect upload (WebGPU's `writeTexture` origin,
  GL's `texSubImage2D`).
- The package has the allocator (`packRects` in `pack.ts`, the shelf
  packer extrusion uses) but nothing that owns pages: a mutable page
  texture, the shelves' state across calls, and the frames it hands out.

## Done looks like

```ts
let pages = createAtlasPacker({ pageSize: 2048, pages: 4, filter: "linear" })
let layer = createSpriteLayer(pages.atlases)
let frame = pages.pack(decodeImage(bytes))   // a Frame over one of the pages
pages.free(frame)                              // optional: reclaim
```

The packer owns N mutable page textures created up front (so the layer's
atlas list is complete at creation and nothing rebinds), allocates with
the shelf packer per page, uploads the image into its region through the
core's sub-rect upload, and returns an ordinary `Frame`. A full packer
fails at `pack` (throw in dev); growth, a re-pack that invalidates every
`Frame` an app holds, and frames as handles rather than plain rects are
the questions to settle before the packer exists, because they decide the
public type.

## Involves

Core: `uploadTexture` taking a destination rect (alloy `update_texture`
over `tex_sub_image_2d`, the flux marshal, flux-types, the docs). The
package: `packer.ts` over `pack.ts`, a shelf state that survives calls
(the pure packer places a whole set at once; the runtime one adds to
shelves incrementally), and a mipmap story - a page that is written over
time regenerates its chain per upload, which argues for `mipmap: false`
pages or extrusion at pack time.
