---
title: Present-fence pacing on ANGLE (never-blocking waits)
description: ANGLE/D3D11's glClientWaitSync returns immediately instead of blocking, so depth-capped present pacing degrades to check-and-proceed there; a GetSynciv-spin fallback would restore blocking pacing if Windows drag latency ever shows a real problem. macOS (ANGLE-Metal) unmeasured.
created: 2026-08-04
---

# Present-fence pacing on ANGLE (never-blocking waits)

Measured 2026-08-04 with `alloy/examples/present_fence_probe.rs` on the
Windows box (RTX 3070, ANGLE 2.1.26628 over D3D11, ES 3.0), after
`fenceTimeouts` tracked ~1:1 with gpuPasses at a locked 60 fps while a Linux
Mesa control read 0. Three findings:

- `glClientWaitSync` never blocks on ANGLE/D3D11: an unsignaled fence
  returns `TIMEOUT_EXPIRED` in ~0.1 ms regardless of the 100 ms budget;
  `SYNC_FLUSH_COMMANDS_BIT` does not rescue it, an immediate retry is also
  instant.
- A fence created after `SDL_GL_SwapWindow` has its submission deferred:
  left alone it reports SIGNALED only after two further swaps.
- One explicit `glFlush` after `fence_sync` (or creating the fence before
  the swap) makes every fence already-signaled by the time the depth-2 wait
  pops it.

## Fixed

The false counter. `RasterState::present()` now flushes right after queueing
the present fence (alloy/src/raster/mod.rs); verified on the Windows client:
`fenceTimeouts` 0 across 5000+ passes (was ~1 per frame), frame times flat.
The flush sits at a frame boundary (the swap already flushed everything
else), so the tiled-GPU cost concern does not apply; Mesa behavior is
identical with and without.

## Open

- Blocking pacing is still absent on ANGLE. With waits that never block,
  `await_present_fence` degrades to check-and-proceed: on an over-budget GPU
  the CPU is capped by vsync alone, so ahead-of-glass depth (input-to-photon
  latency) is unprotected exactly when it matters. The probe could not
  distinguish "never blocks" from "never blocks on an idle GPU" - whether
  real pacing survives Windows load is unmeasured. If Windows drag latency
  ever shows a real problem, the fallback is a bounded spin on
  `GetSynciv(SYNC_STATUS)` (sleep a ms between polls, same 100 ms cap) used
  when a wait returns instant-expired. Do not build it without evidence.
- This interacts with [adaptive-present-fence-depth](adaptive-present-fence-depth.md):
  that design gates depth on observed fence-wait durations, which on ANGLE
  are always ~0 - its gate signal would need the spin (or the
  instant-expired count) instead of wait time.
- macOS runs ANGLE too (Metal backend, separate sync implementation);
  whether it shares the deferred-flush or never-blocking behavior is
  unmeasured. The probe example answers it in minutes once a macOS machine
  is testing again.

Full probe data and the winbox build recipe for it: memory
`project_present_fence_angle_d3d11` / `windows-dev-flow` (session
2026-08-04).
