---
type: backlog-item
title: GPU context loss
description: A lost GL context used to leave the app running against a dead swapchain; swap-result checking and exit after two failed presents shipped, real recreation still open.
status: partial
timestamp: 2026-07-27T00:00:00Z
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
another process hanging the GPU, sleep/resume edges. On Android it is a
normal lifecycle event (app backgrounded), which today we only survive
because the whole process is torn down.

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

   Scoping note from [gpu-review](../analysis/gpu-review.md) (lesson 10):
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
