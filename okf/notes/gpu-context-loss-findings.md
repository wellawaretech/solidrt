---
title: GPU context loss findings
description: What a lost GL context looks like on ANGLE/D3D11 and what in-process recovery needs, measured on the Windows box (probe example and the built recovery, 2026-09/10).
created: 2026-10-15
---

# GPU context loss findings

Measurements behind okf/done/gpu-context-loss.md, in the order they were
made.


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
- 2026-10-15, Windows box (RTX 3070, driver 591.86, ANGLE 2.1.26628 over
  D3D11), `alloy/examples/context_loss_probe.rs` (untracked, like the other
  probes). Three measurements:
  - A context created the way alloy creates it (no robustness attribute)
    hides `glLoseContextCHROMIUM(GUILTY, GUILTY)` completely: `glGetError`
    no-error, reset status no-error, the swap and a rebind succeed. The
    robustness spec says exactly that for `NO_RESET_NOTIFICATION`. SDL's EGL
    backend passes no reset-notification attribute at all (only its GLX/WGL
    backends do), but `SDL_EGL_SetAttributeCallbacks` appends context
    attributes at `eglCreateContext`.
  - With `EGL_CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY_EXT =
    EGL_LOSE_CONTEXT_ON_RESET_EXT` through that callback, the same loss reads
    `guilty-context-reset` from `glGetGraphicsResetStatus` while the swap and
    the rebind still succeed: a context-level loss on ANGLE never fails a
    present. A new context on the same window recovers it; the old context
    destroys cleanly.
  - A real device removal (a fragment shader that never ends; Windows'
    two-second watchdog reset the driver): ANGLE's own stderr line named it,
    `The D3D11 device was removed, HRESULT: 0x887A0006`
    (`DXGI_ERROR_DEVICE_HUNG`); `glFinish` returned after 10 s; the next
    swap failed with `EGL_CONTEXT_LOST`; `glGetError` read
    `GL_OUT_OF_MEMORY` once, then `GL_CONTEXT_LOST`; the reset status read
    `unknown-context-reset`; the rebind failed `EGL_CONTEXT_LOST`; a new
    context on the same window failed `EGL_CONTEXT_LOST` (ANGLE's display
    is marked lost, and every validated EGL call errs until `eglTerminate`);
    destroying and recreating the window brought a context that presented
    30/30 frames, and Impeller came up on it. An adopted Impeller texture
    and the Impeller context dropped cleanly on the lost context before the
    recreation.
  - The probe's window recreation is SDL's `eglTerminate`: destroying the
    last GL window unloads the GL library (`SDL_GL_UnloadLibrary`), which
    terminates the display, and the next GL window initializes it afresh.
- 2026-10-15, the built recovery on the same box, dev client against a
  local dev server, `probes/context-loss-probe.tsx` (a pixel texture the
  app re-uploads in `onContextRestored`, a `createImage` from bytes, a
  fragment target, text):
  - `/gpu?lose=1` (the ANGLE hook): detected at the next frame's start,
    window recreated, 4 textures and 1 target rebuilt, the app told about 3
    textures; after the resync every node's snapshot read as before the
    loss, the restores counter 1, `/stats contextLosses` 1, the process
    alive.
  - a real removal (`/debug?name=hang` draws a fragment shader that never
    ends; Windows' watchdog reset the driver): the loss line carried
    `DXGI_ERROR_DEVICE_HUNG (this process's work hung the GPU)` from the
    device probe, then the same recovery; the probe's hang target, a
    flush-rendered one, was re-rendered on the new context before the
    app's deferred destroy landed and hung the GPU once more, and the
    second recovery passed too (counter 2, every node restored).
  - Three things the probe example did not show and the engine run did:
    (1) a context lost since the last present fails every allocation the
    next frame makes, so the draw never reaches the swap - the reset status
    has to be asked at the frame's start, not only after a present; (2)
    holding an AddRef'd `ID3D11Device` (from `EGL_ANGLE_device_d3d`) across
    the recovery fast-fails the raster thread in dxcore.dll
    (`FAST_FAIL_CORRUPT_LIST_ENTRY`, under d3d11.dll, first frame on the
    new context) - the probe keeps the raw pointer only and asks it at the
    loss, while ANGLE still owns the device; (3) SDL unloads ANGLE's DLLs
    with the last GL window, so they are pinned for the process life
    (libloading, forgotten) and the recovery is ANGLE's own eglTerminate /
    eglInitialize on one loaded library.
  - SDL's reset-notification attribute goes through
    `SDL_EGL_SetAttributeCallbacks`; ANGLE granted it (no refusal logged).
  - Not run: the same cycle on macOS (the other window-recreation
    platform) and on the Android emulator (the same-window context path).
