---
title: Keep text layer rasters from stalling paint
description: A frame that re-rasterizes a text layer while the GPU is busy with a heavy scene paints in 16-19 ms instead of under 1 ms; the layer raster is a blocking round trip to the raster thread, and its glyph pass likely syncs on GPU work still in flight. Measure which, then take it off paint's critical path.
created: 2026-10-14
---

# Keep text layer rasters from stalling paint

A fill-mode 3d scene costing 5-10 ms of GPU, with a HUD of three small mono
readouts over it updated at 10 Hz: every frame where a readout changed was a
slow frame, `paintMs` 16-19 ms with `paintOps.textLayers` 1-2. Frames without
a text change painted in under 1 ms, and `missedPresents` stayed 0.
Throttling the readouts to 4 Hz and writing only on change removed the slow
frames at a 1261x688 window, but six remained in three seconds at full
screen (2256x1504). Observed on an Intel RPL-P iGPU, Mesa 26.1.5; not yet
isolated. 16-19 ms is about one GPU frame at that size (scene plus a 5-7 ms
window composite), which is what a CPU-GPU sync during paint would cost.

Any app that updates text over a GPU-heavy frame pays this: a score, a
timer, a speed readout.

## What the code does

`Text::build_layer` (alloy/src/rendertree/text/mod.rs) re-rasterizes a
layer whose inputs changed through `draw_layer` -> `glyph_quads` (ensures
the cells, flushes the text atlas with fire-and-forget
`update_texture_rects`) -> `Context::rasterize_glyphs`
(alloy/src/context/capture.rs). That last call is `rpc`
(alloy/src/context/mod.rs): send `RasterCmd::RasterizeGlyphs` on the one
ordered raster queue, then block on the reply. Paint stands still for the
whole trip, once per changed text.

It blocks because the display list being built needs an Impeller `Texture`
handle for the layer, and only the raster thread can make one
(`create_layer_target` + `adopt_opengl_texture` in `gl::render_glyphs`).
When the layer re-renders at the same texel size (`into`), the reply is a
clone of the handle the UI thread already holds: the wait buys nothing but
the error result. The blocking is also what keeps the atlas stable between
two layers of one frame (okf/notes/text-rasterizer-findings.md, 2026-10-09
entry), but queue order gives the same guarantee for fire-and-forget
commands: a later atlas upload executes after the earlier glyph pass.

A content-sized readout whose digit count changes changes its texel size,
so it takes the new-texture path (create, clear, adopt), not `into`.

## Where the time can go

The trip should be cheap: the UI thread builds the next frame only after
the previous present (FrameRendered follows Presented), and the idle Tick
waits for `queue_depth == 0`, which drops only after a command finished
executing. So the raster thread is normally idle when paint asks, and the
glyph pass issues no finish, fence or readback. What is left, none of it
confirmed:

- **Implicit sync on a busy resource.** The GPU is still executing the
  previous frame (fence depth 2 lets the CPU run ahead). `GlyphRig::upload`
  (alloy/src/gl/glyphs.rs) rewrites ONE shared quads texture with
  `tex_sub_image_2d` for every layer, so the second layer of a paint
  overwrites a texture the first layer's draw, queued behind the whole
  previous frame, still reads. A driver that stalls rather than ghosts on
  that write-after-read waits out a full GPU frame. The ramp texture
  (gradient text) and an atlas rect upload have the same shape. This fits
  "textLayers 1-2" and "worse at full screen" best, but a single changed
  layer per frame would have no in-flight reader of the quads texture.
- **Driver throttling** inside a GL call on the raster thread (a batch
  flush that waits for older batches when the GPU is behind).
- **Queue wait**: something heavy queued ahead of the request on the raster
  thread (a manual target render sent during the JS phase, for example).

## First read

No code change needed for the first split: `rasterCmdMs` in get_stats is
the raster thread's time on non-Frame commands, `RasterizeGlyphs` among
them. If it grows by ~16 ms per slow frame, the time is spent executing the
glyph pass (a GL-side stall); if it does not, paint waited in the queue.
Beyond that, temporary timing per step of `render_glyphs` (framebuffer
setup, quads upload, draw, adopt) names the call. Repro: a probe with a
fill-mode `<Scene>` heavy enough to cost ~10 ms GPU plus two or three
`<text>` readouts set every 100 ms, read with `/stats?frames=n`.

## Fix directions

By what the read shows:

- **GL-side stall**: never rewrite a resource an in-flight draw may read.
  Upload every layer's quads of a paint into distinct storage (a ring of
  quads textures, or one upload per frame with an offset per layer), or
  orphan the storage with `tex_image_2d` per upload. Same for the ramp.
- **The round trip itself**: make the `into` re-render fire-and-forget (the
  handle is known, and channel order puts the pixels ahead of the Frame
  that samples them; a failure logs raster-side and leaves the old pixels).
  To make `into` the common case, round layer textures up to a size class
  and keep the exact `src` rect, so a readout gaining or losing a digit
  re-renders in place; only a class change creates a texture and round
  trips.

Both may be needed; the first is the likely one.

## Done looks like

Over a GPU-bound scene, a frame that re-rasterizes one to three text layers
paints within a millisecond of one that does not, at full screen on the
iGPU above and on a tiled mobile GPU. The rasterizer notes record which of
the three causes it was.
