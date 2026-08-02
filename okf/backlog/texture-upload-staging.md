---
type: backlog-item
title: Zero-copy texture upload staging buffers
description: Every uploadTexture copies the pixels once more to cross onto the raster thread, so the steady state of any texture-driven app is one full-frame copy per frame no matter what the app does; a begin/endTextureUpload pair over raster-owned staging buffers is the only honestly zero-copy shape.
status: open
timestamp: 2026-08-02T00:00:00Z
---

# Zero-copy texture upload staging buffers

Source: the wasm game-port demo feedback (2026-08-02), the "deeper one, for
later" behind [[flux-wasm-memory-access]].

Today the upload path sends
`RasterCmd::UpdateTexture { pixels: pixels[offset..end].to_vec() }`
(alloy/src/context.rs). That copy is correct as written - the JS-side
borrow is only valid for the duration of the call and the raster thread
outlives it - but it means no API change on the input side can ever remove
the last full-frame copy per frame for a texture-driven app.

If it becomes worth attacking, the shape is a pool of runtime-owned staging
buffers the app writes into directly: `beginTextureUpload(id)` hands back a
Uint8Array view over a buffer the raster thread already owns;
`endTextureUpload(id)` hands ownership over and queues the raster command
with no copy at all. The guest (wasm or JS) then renders straight into
memory the GPU path is going to read - the only arrangement that is
honestly zero-copy end to end.

Demand-gated: at a retro framebuffer's size the copy is measurably free. It
stops being free when video playback or camera frames arrive, where the
per-frame buffers are 10-40x larger.
