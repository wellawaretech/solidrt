---
title: Present-fence pacing on ANGLE (never-blocking waits)
description: ANGLE/D3D11's glClientWaitSync returns immediately instead of blocking, so depth-capped present pacing degrades to check-and-proceed there; a GetSynciv-spin fallback would restore blocking pacing if Windows drag latency ever shows a real problem. macOS (ANGLE-Metal) fence behavior unmeasured; its swap is known not to block.
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

## Headless retirement (found and fixed 2026-10-19)

The same never-blocking wait broke correctness, not just pacing, on the
headless path: `RasterState::finish_gpu_work` (alloy/src/raster/frame.rs)
waits on a fence right after the frame's draw and publishes the frame
retired when it signals. On the winbox (ANGLE 2.1.28027 over D3D11) that
wait returned TIMEOUT_EXPIRED on every frame, an explicit `glFlush` after
the fence included, so `retiredFrames` stayed 0 for the whole run. Every
consumer of retirement then misbehaved: the resource pool handed out no
freed target (every take a miss, every chunk a fresh texture), and a re-bake
of a drawn chunk swapped instead of writing in place. Three 2d tile tests
failed on it (materials: per-cell style re-bake; tiles-residency: the pool
handover and the maxTexels budget), on Linux and macOS (ANGLE over Metal)
none. Fixed with the fallback proposed below, on the headless path only:
an expired wait polls `GetSynciv(SYNC_STATUS)` every millisecond until the
same 100 ms deadline. Verified by the whole app suite on the winbox. The
present path still pops fences two frames later and is as described under
Open: on an over-budget GPU its waits also expire instantly, and
`retiredFrames` would then lag there too, with the pool allocating instead
of reusing under exactly that load; the same poll is the fix if that is
ever measured.

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
  whether it shares the deferred-flush or never-blocking fence behavior is
  unmeasured. The probe example answers it in minutes once a macOS machine
  is testing again. What is measured there (2026-09-25): the swap itself
  never blocks on ANGLE-Metal, which is why macOS paces on a display link
  (okf/design/frame-timing.md, "Frame signal, macOS"); the fence question
  is separate and still open.

Full probe data and the winbox build recipe for it: memory
`project_present_fence_angle_d3d11` / `windows-dev-flow` (session
2026-08-04).
