---
type: backlog-item
title: Node captures round-trip through a texture nobody wants
description: "Done 2026-08-06: captureSnapshot now resolves { width, height, data } directly and no texture is created; the padding-aware capture texture (variant 2) stays unbuilt until a caller wants a texture."
status: done
timestamp: 2026-07-27T00:00:00Z
---

# Node captures round-trip through a texture nobody wants

## Update 2026-08-06: variant 1 implemented as a breaking change

`captureSnapshot(nodeId)` now resolves `{ width, height, data }` (the
`readTexture` result shape) and creates no texture; there is nothing to
destroy afterward. The texture-returning form is gone at every layer:
`Context::capture_node_texture` became `capture_node_pixels` (the
`RasterizeReadback` RPC without the re-upload), `CaptureInfo` carries
`pixels` instead of `texture_id`, and MCP `get_snapshot` PNG-encodes the
callback's pixels directly. A caller that wants a texture composes with
`createTexture` / `create_texture_from_pixels` (alloy's `capture_crop`
example demonstrates it). All three in-repo callers (terminal atlas bake,
sandbox runtime ink measure, changelog-shot) dropped their
readTexture + destroyTexture pairs.

Variant 2 (the padding-aware capture texture) stays unbuilt as this note
decided: no caller wants a capture as a texture today. What would revive it
is unchanged - a freeze-frame consumer that samples rather than reads.

Original analysis follows.

`capture_node_texture` (alloy/src/context.rs) rasterizes a node's display
list, reads the pixels back to the CPU, and uploads them again as a
registered texture, returning the id. Both callers then immediately read
that texture back down and destroy it:

- JS `captureSnapshot` (flux/src/plugins/gui/gpu.rs) - projects/linux
  bakes its terminal glyph atlas by capturing 64 laid-out cells per batch,
  calling `readTexture` on each, keeping the red channel as coverage, then
  `destroyTexture`.
- MCP `get_snapshot` (lattice/src/go/connection.rs `request_snapshot`) -
  `read_texture_by_id` then `destroy_texture`, to PNG-encode the result.

So the mechanism materializes a GPU texture that every caller throws away
unread-as-a-texture. Per capture that is: rasterize, `glReadPixels`,
upload + adopt, `glReadPixels` again, delete. The version that serves both
callers is: rasterize, `glReadPixels`, done.

The bytes are not the problem - a supersampled terminal cell is a few KB.
The sync points are: each `glReadPixels` stalls the pipeline, so the atlas
bake pays roughly twice the stalls it needs across ~400 glyphs at terminal
startup, plus ~400 texture create/adopt/destroy lifecycles.

## Why the round trip is there

Not an oversight. `render_display_list_to_texture` over-allocates the render
target to a 64px tile boundary (an Android requirement - unaligned offscreen
targets come back shifted or channel-scrambled on some GPUs), and reading
back exactly `width` x `height` from the origin is how the padding gets
cropped. The re-uploaded texture is therefore unpadded, and neither
`read_texture_by_id` nor `<texture src>` needs origin-specific knowledge.

## Two independent changes

1. **A pixels-returning capture.** `RasterizeReadback` already produces the
   `Vec<u8>`; stop uploading it. Wants a JS-facing shape that resolves to
   `{ width, height, data }` directly (the `readTexture` result shape) and
   an alloy entry point that skips `create_texture_from_pixels`. Serves both
   existing callers with strictly less work and no new concepts. The
   existing texture-returning form can stay for compatibility or go, since
   nothing depends on it being a texture.
2. **A padding-aware capture texture.** Keep the padded texture and carry
   the content rect alongside it, instead of round-tripping to crop. The
   engine already does exactly this in the snapshot-boundary path
   (alloy/src/rendertree/composite.rs maps a `src` rect of the true content
   size onto the quad), and `TextureProps` already exposes
   `srcX/srcY/srcW/srcH`, so a padded capture texture would be usable from
   the tree as-is. Only pays off for a caller that genuinely wants a texture
   rather than bytes - a freeze-frame transition being the plausible one -
   and there is no such caller today. It also costs the registry entry
   knowing about padding.

(1) is where the waste is and does not block (2).

Priority: low. Both consumers are one-shot (a startup bake, a rare dev-server
query), so nothing here is on a critical path, and the atlas bake's own JS-side
phase search and downsample over ~400 glyphs plausibly dominates the readback
stalls anyway (unmeasured - do not pick this up assuming it is the win). Worth
doing opportunistically, for three small reasons: it deletes a step rather than
adding one, it settles the JS-facing return shape while exactly one app depends
on it, and it drops two of the three blocking RPCs per capture, which is
adjacent to stage 2 of angle-cross-context-impeller-textures.md. What would
raise it: a measurement showing readbacks are a real share of bake time, or a
second capture consumer.

Related: the capture docstrings in packages/flux-types/gui/gpu.d.ts and
packages/core/src/gpu.ts now describe this as the one-shot bake path and
warn it off per-frame use, which is the behaviour (1) makes cheaper rather
than changes.
