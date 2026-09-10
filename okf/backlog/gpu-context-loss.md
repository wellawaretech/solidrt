---
title: GPU context loss
description: A lost GL context used to leave the app running against a dead swapchain; swap-result checking, exit after two failed presents and the Android background case (no window work while backgrounded) shipped; real recreation after a genuine loss still open.
created: 2026-07-27
---

# GPU context loss

Source: Windows client debugging session 2026-07-19. When the GPU device
backing the GL context dies (D3D11 device removed under ANGLE, EGL context
lost), every subsequent present fails while the app keeps running normally:
JS, layout, and frame building all continue against a dead swapchain. The
user sees a frozen or black window with no message; the process looks
healthy.

Context loss arrives for reasons outside our control even with correct GL
usage: a graphics driver update while the app runs, a TDR reset triggered by
another process hanging the GPU, sleep/resume edges. Android's background
is not one of them: the surface goes away and comes back, the context
survives, and the raster thread now holds off the window meanwhile (Done,
2026-09-10).

## Done (2026-07-19)

Detection + logging: `sdl_utils::gl_swap_window_checked` surfaces the
`SDL_GL_SwapWindow` result the sdl3 crate discards; `RasterState::present`
(alloy/src/raster.rs) logs one error per failure streak (SDL's error text
includes the EGL error, e.g. `EGL_CONTEXT_LOST`).

Fail loudly: two consecutive failed presents confirm the loss and exit(1)
with a message. Two because a demand-driven app may attempt very few
presents after the loss (observed frozen-window traces stopped at two); one
tolerated failure covers a transient glitch
(`PRESENT_FAILURE_EXIT_THRESHOLD` in raster.rs).

Rebind-and-redraw between the two (landed with the resize work): after the
first failed present the raster thread recreates the wrapped window surface
(`rebind_window_surface`) and redraws before counting the second failure,
which recovers EGL-surface-level losses (the Android background/resume
case) without process exit. Full context/Impeller/resource recreation is
still Remaining #2.

## Done (2026-09-10)

No window work while backgrounded: `WINDOW_BACKGROUNDED` in
alloy/src/lib.rs, set by the event watch at `WILL_ENTER_BACKGROUND`, cleared
by the raster thread's return-to-visible rebind. Frames queued behind a slow
producer no longer present into the destroyed surface, so a raster-bound app
survives the home press (it used to exit through the two-failure threshold
on `EGL_BAD_SURFACE`, which is "no surface", not a loss). Details in
Findings below; reproduce with `probes/gpu-load-probe.tsx`.

## Remaining

1. Windows diagnostic garnish: query the D3D11 device's
   `GetDeviceRemovedReason` via `EGL_EXT_device_query` (safe bindings from
   the `windows` crate, cfg(windows)) and include it in the log line -
   distinguishes "our workload hung the GPU" (DEVICE_HUNG) from driver
   faults (DRIVER_INTERNAL_ERROR) in field reports.
2. Real recovery (long-term): recreate EGL display + both contexts +
   Impeller context and re-upload all GPU resources (textures, shader
   targets, buffers), then repaint. This is the same machinery Android
   context-loss-on-background needs, so it should be designed against the
   Android lifecycle, not as a Windows special case.

   Scoping note from [gpu-review](../notes/gpu-review.md) (lesson 10):
   recovery here can be *transparent* in a way neither WebGL nor WebGPU can
   offer, because apps hold registry ids rather than device-bound handles,
   and the registries already retain what recreation needs - each target's
   pipeline, spec, sampler bindings and last params, each pipeline's desc
   and program, each texture's size and sampler state. Shader targets,
   pipelines and programs are therefore recreatable engine-side (recreate
   the GL objects behind the same ids, mark everything dirty, flush). The
   app-visible half shrinks to content the engine cannot reproduce -
   uploaded texture pixels and buffer contents - which needs either retained
   CPU copies (memory cost) or a re-upload event. The standards are right
   that some loss is unrecoverable, so the app-visible event still needs to
   exist; but the default can be repair rather than teardown.

Related: the cross-thread GL race that *caused* device removals on Windows
is fixed separately by the single-context + raster-thread architecture (all
GL on one thread; see angle-cross-context-impeller-textures.md - the
earlier `lock_gl` mentioned here was deleted with it); this item is about
losses that arrive anyway.

## Findings

- 2026-09-10, Pixel 7 (Mali-G710, Android 17), go client with the lifecycle
  probe (static text) and `packages/3d/examples/instanced.tsx` (continuous
  animation): home-press gaps of 3 s, 20 s, 30 s, 90 s and a 180 s screen-off
  did not produce a single failed present, rebind or context loss; the
  process survived every cycle and repainted on resume. Same result on the
  SM-T500 (Adreno 610) the same day. The loss this note expects on Android
  background is not what a plain background does on either device.
- What every cycle longer than a few seconds did show: the dev connection
  drops about 5 s after backgrounding (`[sgo] Connection ... lost`; the
  server sees a peer reset), and on resume the reconnect receives the
  server's `currentReload` push, which the client applies as a full engine
  reload. The app comes back at its first frame with its state gone, and
  after longer gaps the reconnect itself took up to 20 s. That is part of
  the demo's "loses its run when backgrounded" symptom, and it is the dev
  loop working as intended: a reconnecting client takes the server's
  current app. Not a bug, not to be changed.
- The opening paragraph's "which today we only survive because the whole
  process is torn down" predates the rebind-and-redraw in Done: the
  Android background/resume case survives in-process.
- Reproduced and fixed the same day, once the missing variable was named: a
  raster-bound app. `probes/gpu-load-probe.tsx` (16 full-screen blurred
  layers, 23 fps on the Pixel 7) died 0.4 s after the home press:
  `surfaceDestroyed`, a queued frame's `eglSwapBuffers` failed with
  `EGL_BAD_SURFACE`, the rebind "succeeded" (a context goes current without
  a surface), the retry present failed the same way, two failures, exit. A
  light app is idle when the surface goes, so it never presents into it;
  a slow one always has a frame queued. `EGL_BAD_SURFACE` is "no surface",
  the normal background state, not a loss.
  Fix: `WINDOW_BACKGROUNDED` in alloy/src/lib.rs, set by the event watch at
  push time of `WILL_ENTER_BACKGROUND` (the same watch that delivers
  suspend) and cleared by the raster thread's `RebindWindowSurface` arm on
  return to visible. While set, `frame` drops window frames like undrawn
  ones (damage kept for the frame after the rebind), a swap that already
  failed is not counted, and no rebind-and-retry runs. A flag, not a raster
  command: a command would queue behind the very frames that must be
  dropped. Verified on the Pixel 7: same probe at 29 fps, home press, no
  present failure, process alive, repaint on resume.
  This is also the iOS rule (an app that touches the GPU in the background
  is killed), from the same event; what a port still needs is to hold the
  offscreen work too (`flush_dirty` at the top of `frame` still runs).

