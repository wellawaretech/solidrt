---
type: backlog-item
title: GPU resource inspection via MCP
description: MCP readback of textures as PNG, buffer ranges and pipeline state, because a one-pipeline app hides everything from the render tree; depth attachments still deferred.
status: done
timestamp: 2026-07-15T00:00:00Z
---

Implemented 2026-07-15 as proposed: MCP tools get_gpu_resources / get_texture
/ get_buffer -> /__control__/{gpu,texture,buffer} -> query kinds gpu/texture/
buffer answered synchronously on the JS thread (no paint pass needed, unlike
snapshot). alloy Context::gpu_resources() inventories the bookkeeping;
get_texture reuses read_texture_by_id (rect cropped CPU-side); get_buffer
maps via glMapBufferRange, capped at 64 KiB per call. Verified end-to-end
against the doom app (atlas + heights texture + 9288-vertex pipeline).
Still deferred from the original proposal: reading a render target's depth
attachment.

# GPU resource inspection via MCP

Motivation (doom, 2026-07-15): once an app is one big GL pipeline, the render
tree is ~3 nodes (window, rect, texture) and `get_render_tree` tells an agent
nothing - all the interesting state lives in GPU resources behind the
`<texture>` leaf. The session's longest debugging arc was a wall texture that
composited fully transparent (case-sensitive patch-name lookup): the window
snapshot showed "pillar missing" but not *why*, and the agent spent a long
loop reconstructing the atlas build CPU-side in scratch scripts to find the
blank tile. One look at the actual atlas texture would have answered it in
seconds ("tile is blank -> decode problem, not geometry/depth/shader").
Other wants from the same session: numerically verifying door/lift animation
by reading the sector-heights data texture (instead of human eyes on the
screen), checking the dynamic sprite tail of the vertex buffer after a
writeBuffer, and confirming a pipeline's actual draw count after
setDrawCount (a stale-draw-count bug class).

Proposal - three MCP tools, dev-server capability like get_snapshot:

- `get_gpu_resources(client)` -> alloy's bookkeeping: textures (id, w, h,
  sampled vs render target), buffers (id, byteLength), pipelines (id,
  vertexCount, current draw count, attribute layout, bound texture ids, last
  uniform values from `last_params`).
- `get_texture(client, id, rect?)` -> contents as PNG. Covers sampled
  textures (atlas, data textures) and render targets alike; optional flag to
  read a render target's depth attachment (sprite-occlusion debugging).
- `get_buffer(client, id, byteOffset, length, as: "f32"|"u16"|"u8")` ->
  decoded values as JSON (length capped per call).

Implementation notes:
- The per-node capture path (request_capture -> serviced at render time ->
  complete_capture -> dev server) already exists for get_snapshot; add
  resource-scoped requests serviced the same way, on the GL thread.
- Textures: GLES 3.0 can't read a texture directly - attach to a scratch FBO
  and read_pixels RGBA8. Render targets read from their own FBO.
- Buffers: glMapBufferRange with MAP_READ_BIT (in ES 3.0).
- Deliberately NOT raw GL state dumps (glGet* of enables/bindings): alloy
  owns all GL state, so the useful truth is resource *contents* plus alloy's
  own bookkeeping. Raw state dumps answer questions nobody was asking.
- Complements mcp-debug-commands.md and mcp-input-injection.md: those cover
  the "app/game state wrong" bug class (the same session's sector-tracking
  desync), this covers the "GPU data wrong" class. The session hit both.
