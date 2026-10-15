---
title: GPU context loss
description: A lost GL context is recovered in-process - the platform side gives the raster thread a fresh context, the raster thread rebuilds every GPU object behind the same ids, and the app is told only what it alone can re-supply; exit stays the outcome when recovery itself fails.
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
survives, and the raster thread holds off the window meanwhile (Done,
2026-09-10).

## Done (2026-07-19)

Detection + logging: `sdl_utils::gl_swap_window_checked` surfaces the
`SDL_GL_SwapWindow` result the sdl3 crate discards; `RasterState::present`
(alloy/src/raster/frame.rs) logs one error per failure streak (SDL's error
text includes the EGL error, e.g. `EGL_CONTEXT_LOST`).

Fail loudly: two consecutive failed presents confirm the loss and exit(1)
with a message. Two because a demand-driven app may attempt very few
presents after the loss (observed frozen-window traces stopped at two); one
tolerated failure covers a transient glitch
(`PRESENT_FAILURE_EXIT_THRESHOLD` in raster.rs).

Rebind-and-redraw between the two (landed with the resize work): after the
first failed present the raster thread recreates the wrapped window surface
(`rebind_window_surface`) and redraws before counting the second failure,
which recovers EGL-surface-level losses (the Android background/resume
case) without process exit.

## Done (2026-09-10)

No window work while backgrounded: `WINDOW_BACKGROUNDED` in
alloy/src/lib.rs, set by the event watch at `WILL_ENTER_BACKGROUND`, cleared
by the raster thread's return-to-visible rebind. Frames queued behind a slow
producer no longer present into the destroyed surface, so a raster-bound app
survives the home press (it used to exit through the two-failure threshold
on `EGL_BAD_SURFACE`, which is "no surface", not a loss). Details in
Findings below; reproduce with `probes/gpu-load-probe.tsx`.

## Plan (2026-10-15): recovery in process

The loss becomes an engine-handled event. Exit stays as the outcome when
recovery itself fails (the window cannot be recreated, the new context
refuses) and in the headless stepped mode (a pbuffer has nothing to
recreate).

**Detection, raster thread.** A failed swap is classified by its EGL error:
`EGL_CONTEXT_LOST` is a loss; `EGL_BAD_SURFACE` and `EGL_BAD_NATIVE_WINDOW`
stay on the rebind path (the surface went away, Android background); anything
else keeps the two-failure threshold. The context is created with
`EGL_LOSE_CONTEXT_ON_RESET_EXT` through SDL's EGL attribute callback (SDL's
EGL backend never asks for it itself, and without it the robustness spec
requires the reset status to read no-error forever), and
`glGetGraphicsResetStatus` is polled once per presented frame: a
context-level loss keeps swapping on ANGLE (measured), so the status is the
only signal for that class. Losses are counted in `RasterStats`
(`contextLosses` in `/stats`).

**Platform reset, main thread.** The raster thread releases the context and
asks the main loop for a new binding over the frame-output channel. The main
loop's policy is per platform: where ANGLE translates GL (Windows, macOS) the
window is destroyed and recreated, because ANGLE marks the whole EGL display
lost and only `eglTerminate` clears that, which SDL does when the last GL
window goes; on Android and Mesa a new context on the same window is the
recovery. Title, size, position, fullscreen state, pointer lock, text input
and the icon are carried over; the resize event and the liveness latch
repaint it.

**Rebuild behind the same ids, raster thread.** `RasterState::recover`
consumes the state: it takes a blueprint of every registry (textures: size,
shape, format, sampler, label; buffers: size; stages and programs: the
sources as compiled; pipelines: descriptor and program; targets: kind, draw
entries with pipeline, buffer ids, range, params, bindings, clear, depth
storage, samples, load, manual, format, region; the window shader and
overlay declarations; sub-target order; depth ownership), drops the GL
objects and Impeller last, binds the new context, and rebuilds in dependency
order with undefined content, re-adopts the 2D names into the new Impeller,
and marks every flush-rendered target dirty so the flush re-renders it from
its retained state. The one retention this adds is the shader source text on
stages and programs.

**UI-side resync.** Frames and handle-carrying RPCs (offscreen rasters,
readbacks, snapshot adoptions) are refused between the loss and the resync,
since their Impeller handles belong to the dead context. The restore event
carries the new handle per id; the UI thread installs them in the texture
registry, drops every paint cache and text layer, forgets the text atlas's
GPU side (re-uploaded whole at the next flush), marks all target content
changed, sends the resync and repaints.

**App contract.** `onContextRestored` in `@solidrt/core`, next to the other
lifecycle hooks, with `{ textures, buffers, targets }`: the ids whose content
the engine cannot reproduce - textures created from pixels, buffers (zeroed),
and manual targets (cleared). `createImage` re-decodes and re-uploads its own
textures. An app that listens to nothing keeps running with those textures
transparent black.

**Diagnostics.** The log line carries the EGL error, the reset status and,
on Windows, `GetDeviceRemovedReason` of the D3D11 device behind ANGLE (held
from startup through `EGL_EXT_device_query`; a lost display answers no
query), so field reports separate a hung workload (`DXGI_ERROR_DEVICE_HUNG`)
from a driver fault. A loss can be forced through `GL_CHROMIUM_lose_context`
wherever ANGLE runs (`POST /__control__/gpu?lose=1`), which is what makes the
path testable on Windows, macOS and the Android emulator without a driver
reset.

## Status (2026-10-15)

Built and verified on the Windows box, both ways: the forced loss through
ANGLE's hook and a real device removal (a GPU hang through the watchdog).
The findings are in [gpu-context-loss-findings](../notes/gpu-context-loss-findings.md).
The macOS and Android-emulator runs of the same cycle are listed in tiny.md.

## Findings

Cut to [gpu-context-loss-findings](../notes/gpu-context-loss-findings.md).

Related: the cross-thread GL race that *caused* device removals on Windows
is fixed separately by the single-context + raster-thread architecture (all
GL on one thread; see angle-cross-context-impeller-textures.md - the
earlier `lock_gl` mentioned here was deleted with it); this item is about
losses that arrive anyway.
