---
title: A snapshot boundary's retained texture as a texture id
description: repaintBoundary="snapshot" kept its subtree's rasterization in an adopted texture only the boundary shader could sample. Landed 2026-08-23 as snapshotTexture(ref), a stable borrowed TextureId re-pointed after every rasterization, so any UI subtree is live content for shader/draw targets and 3d materials - the load-bearing piece of UI-on-3D-geometry.
created: 2026-08-05
completed: 2026-08-23
---

# A snapshot boundary's retained texture as a texture id

Shipped 2026-08-23 as `snapshotTexture(ref)` (core; `flux:rendertree.snapshotTexture`
underneath), verified on Linux through the control API: live update through a
shader target, late acquisition, destroy rejection, unmount release.
Example: `packages/core/examples/snapshot-texture.tsx`; reference:
`packages/core/docs/reference/gpu.md` ("UI as a texture").

## The problem

`repaintBoundary="snapshot"` rasterizes its subtree into an adopted texture
and keeps it until the subtree changes (`raster/capture.rs`; re-rasterized in
place at unchanged dimensions, reallocated only on a layout-size or
display-scale change). Two consumers could reach those pixels and neither was
general: the boundary `shader` prop (one pass, composited in place), and
`captureSnapshot(nodeId)` - which is a CPU readback of a throwaway
re-rasterization, not a texture at all, so a still rather than a feed.

Everything in the GPU stack takes texture ids as its universal currency, so
the boundary's retained texture as an ordinary `TextureId` makes any UI
subtree - text, layout, components, an entire running screen - available as
live content to shader targets, draw targets and 3d materials, at the cost of
a boundary that already exists. The motivating consumer is UI mapped onto 3D
geometry (../notes/3d-differentiators.md, section 3; ../notes/3d-roadmap.md
item 3); the 2D consumers (a UI subtree through a multi-pass chain, a warped
copy shown by a sibling element) fall out of the same id.

## Decisions

- **Identity: the id is an indirection.** Allocated once on first
  acquisition, stable for the node's lifetime; the paint walk re-points it
  at the current backing after every rasterization (the camera precedent:
  `create_texture_at` + `note_content`). Consumers never rebind.
- **Acquisition: sync, `snapshotTexture(ref)`.** Acquisition is the intent
  signal - a bare snapshot boundary publishes nothing. Throws on a node that
  is not a snapshot boundary. No reactive primitive: the id never changes.
- **First frame: valid immediately, empty until painted.** Before the first
  raster the registry has no entry (a `<texture>` measures 0x0, a pass skips
  the binding). A boundary that already baked publishes on acquisition, so a
  static subtree acquired late is not empty forever.
- **Ownership: borrowed ids.** `Context::borrowed` marks runtime-owned ids;
  `destroyTexture` on one throws. A deleted boundary surrenders its id
  (`RenderTree::take_released_snapshot_textures`, drained at both sweep
  sites) to the deferred-destroy path, so a still-mounted consumer keeps the
  last pixels.
- **Ordering: free.** `RasterCmd::AdoptTexture` mirrors the Impeller-owned GL
  name into the raster texture map (no Drop) and seeds `dirty`; the paint
  walk's rasterize commands precede the frame command, which flushes dirty
  first, so a sampling target renders after the boundary in the same frame.
- **Self-reference guard.** A boundary showing its own id is excluded from
  `texture_content_changed` (its rasterization IS the change); without it
  every raster re-invalidates the boundary forever. Unit-tested.
- **Contract: the raw `uSource`.** Premultiplied, top-left, cropped to the
  layout box. With a boundary `shader` the raw canvas is vended (layout box
  plus `2 * outset`), never the shaded output; that stays an additive option
  if a consumer wants post-shader pixels. The default is raw, permanently.

Out of scope, unchanged: routing pointer events back through a mesh into the
mapped subtree belongs with picking (../notes/3d-roadmap.md item 4).

Related: [gpu-deferred-texture-destroy](gpu-deferred-texture-destroy.md),
[gpu-target-dependency-propagation](gpu-target-dependency-propagation.md),
[snapshot-gpu-content-invalidation](snapshot-gpu-content-invalidation.md),
../notes/3d-differentiators.md.
