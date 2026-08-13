---
type: backlog-item
title: Zero-copy texture upload staging buffers
description: Every uploadTexture copies the pixels once more to cross onto the raster thread, so the steady state of any texture-driven app is one full-frame copy per frame no matter what the app does; a begin/endTextureUpload pair over raster-owned staging buffers is the only honestly zero-copy shape. Staged 2026-08-13 against the measured 1080p TV raster bound; stages 1+2 (owned-frame YUV upload, double-buffered plane sets) implemented, TV measurement pending.
status: in-progress
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

## Staging (2026-08-13, against the measured 1080p TV raster bound)

The demand arrived as [[video-playback]] staging item 5: 1080p decode keeps
pace on the TV but presentation is raster-bound at 16-19/25 fps (~33ms/frame
on srt-raster; the conversion pass is 1.3ms of it). That reordered this item:
the measured bound is the RASTER side of the chain, not the JS-facing copy
this note originally targeted, so the internal (Rust) video path goes first
and the begin/end JS API rides the same machinery later.

The three costs in the chain for a 1080p NV12 frame (~3.1 MB):
decode copy-out (srt-video, mandatory - the codec buffer must be released),
the per-plane `to_vec()` crossing the raster channel (srt-ui, pure waste),
and the raster-side `glTexSubImage2D` - a driver memcpy plus a potential
stall/ghost because the plane textures were sampled by the previous frame's
conversion pass, still in flight on a pipelined (tile-based) GPU.

1. DONE 2026-08-13: ownership transfer. `Context::update_yuv` takes the
   frame `Vec<u8>` by value; one `RasterCmd::UpdateYuv { planes, frame }`
   moves the whole buffer across the channel and the raster thread slices
   each plane at its offset. Kills the srt-ui full-frame copy and the
   per-plane allocs. No GL changes.
2. DONE 2026-08-13: double-buffered plane sets. A YUV texture owns TWO full
   plane sets (YuvGroup in alloy context.rs); update_yuv uploads into the
   back set and rebinds the conversion target to it through the ordinary
   set_target_textures path (sampler-graph mirror stays honest, channel
   order puts the rebind after the upload). Every upload targets planes no
   in-flight pass samples - the write-after-read stall is the prime suspect
   for the 33ms. Verified on the live GL path by alloy's yuv_texture
   example (readback after each upload alternates sets).
   TV-MEASURED 2026-08-13 (same flow as the stage-2 baseline): raster
   busy dropped ~25% (820 -> ~620-650 ms busy per wall second at 26
   uploads/s; ~31.5 -> ~24 ms per upload cycle), correctness held
   (colors, sync, zero warnings, full-clip runs) - but presented fps is
   UNCHANGED at 17-19/25 (frameMs ~55-60). The busy win didn't buy
   frames because the limiter is the per-frame CRITICAL PATH against the
   TV's 50 Hz vsync grid, not raster capacity: 25 fps needs the whole
   tick -> upload -> convert -> composite -> swap loop inside 2 vsync
   periods (40 ms), and at 1080p it slips to ~3 (55 ms -> ~17 fps),
   while 360p fits exactly (frameMs ~39, full 25 fps). Stage 3 is still
   the right next rung for exactly this reason: mapped-PBO staging takes
   the upload's driver memcpy OFF the critical path (unmap + DMA), which
   is what must shrink, where stages 1+2 mostly removed off-path work.
   A pacing probe the same day found a ~27 ms BASE frame-loop latency
   independent of resolution, so stage 3 shrinks only the 1080p delta;
   the base latency (and true fluency) is frame-driver/pacing territory,
   promoted to its own item: [[frame-pacing-fluency]] carries the probe
   findings, the measurement rules, and the trace plan.
3. Only if 1+2 measure insufficient on the TV: the actual staging-buffer
   pool. Raster-owned mapped PBOs (glMapBufferRange; map/unmap on
   srt-raster only, a Send lease over the mapped memory), leases flowing
   caller -> player -> decode worker so the decoder's stride-repack writes
   directly into GPU-visible staging and the raster upload becomes
   unmap + glTexSubImage from the PBO (DMA, no driver memcpy). Needs the
   forge decoder trait to gain decode-into-borrowed-buffer (still
   engine-free: plain `&mut [u8]`).
4. Deferred until a consumer exists: the JS `beginTextureUpload` /
   `endTextureUpload` API of this note's original wasm-game motivation,
   exposed over the same leases.
