---
title: Context loss recovery faults on NVIDIA Wayland
description: A GPU context loss on the NVIDIA proprietary driver under Wayland ended the client in SIGSEGV instead of a recovery - `recreate_gl` destroyed the dead context after the raster thread released it, and this driver faults in exactly that destruction. Fixed by forgetting the dead context on the same-window path; `/gpu?lose=1` now simulates a loss on drivers without ANGLE's hook, so the recovery is testable everywhere.
created: 2026-11-15
---

# Context loss recovery faults on NVIDIA Wayland

## Symptom

Measured on Linux, NVIDIA proprietary driver 615.78.08, Wayland, with a
loss simulated in the raster thread (the frame after `/gpu?lose=1` reports
`FrameExit::ContextLost`; the edit is under Reproduction below). The log
reads `GPU context lost: simulated; recovering`, then the process dies in
SIGSEGV. The dump: the main thread in
`libnvidia-eglcore`, from `SDL_EGL_DestroyContext` in
`ore::window::Window::recreate_gl`, from `alloy::app::recover_gl`. The
faulting function is the one both exit crashes of
[exit-races-raster-thread](../done/exit-races-raster-thread.md) ended in.

The simulation crashed the client every time, without a real reset. A real loss on this
driver was not provoked (the `GL_CHROMIUM_lose_context` hook is ANGLE's,
and NVIDIA has it not); what a real reset adds to the driver's state is
unknown, and does not matter for this fault, which is in the destruction
the recovery performs on any loss.

## Reproduction

`/gpu?lose=1` on a driver without `GL_CHROMIUM_lose_context` (kept, see
Outcome): release client, a dev window
(`sol run examples/hello-world/src/index.tsx --project --port 34899`), `GET
/gpu?lose=1` (replies `{ forced: false, simulated: true }`), then something
that demands a frame (`POST /window?width=1000&height=600` does). Before the
fix the client log showed `GPU context lost: simulated loss (no reset);
recovering` and the client died; `coredumpctl list` gained an entry whose
main thread sat in `SDL_EGL_DestroyContext` under `recreate_gl`.

## Cause

`recover` (`alloy/src/raster/recover.rs`) drops the renderer, releases the
dead context on the raster thread (`eglMakeCurrent` with no context) and
asks the main thread for a new one; `recreate_gl` destroys the old context
first (`SDL_GL_DestroyContext`), then creates the new one on the same
window. On this driver, destroying a context after it was released from the
thread it was current on faults in `libnvidia-eglcore`, whoever destroys
it. The exit fix avoids the fault by never releasing: the raster thread
parks with its context current and nothing destroys it. A recovery cannot
park; it has to get rid of a context and bind a new one on the same thread.

## Done looks like

- A simulated loss (the experiment above, or a kept debug hook behind it)
  on NVIDIA Wayland recovers: `contextLosses` counts 1, `onContextRestored`
  fires, the next frame draws, no dump.
- The recovery still passes on Mesa, and ANGLE's forced loss (`/gpu?lose=1`
  on Windows, macOS, the Android emulator) still recovers.

## What it involves

Pick the destruction the driver survives, measured with the reproduction
above, not reasoned:

1. Do not destroy the dead context at all: `recreate_gl` leaks it (forgets
   the SDL handle) and only creates the new one. One leaked context per
   loss; losses are rare and the leak is bounded by the driver's own
   accounting of a dead context. Cheapest, and the shape the exit fix
   already takes.
2. Destroy it on the raster thread while it is still current, before the
   release: SDL's destroy makes it non-current on the calling thread first,
   so the release happens inside the destroy, on the same thread. Needs the
   context handed to the raster thread for that one call, against ore's
   contract that SDL creates and destroys contexts on the main thread.
3. Recreate the window (`RECOVERY_RECREATES_WINDOW` on this driver too):
   SDL's destroy of the last GL window terminates the EGL display. Whether
   the terminate survives a released context is as unmeasured as the
   destroy.

Whichever it is, a kept way to simulate a loss on drivers without the
ANGLE hook would make the recovery testable here; today the hook's answer
is the only trigger, and the fix in `Robustness::load` (the hook now
resolves by the extension string) makes NVIDIA answer `forced: false`
honestly instead of a no-op `true`.

## Outcome

Option 1, measured: `recreate_gl` forgets the dead context's SDL handle on
the same-window path (`std::mem::forget`) and destroys it only together with
the window (`RECOVERY_RECREATES_WINDOW`, where the destroy was measured safe
under ANGLE and the display is terminated anyway). What leaks is the
driver's bookkeeping of one dead context per loss; its GL objects died with
the reset. Applied on Mesa too, one behaviour per path rather than a second
driver list.

Verified on NVIDIA Wayland with the reproduction above: three simulated
losses in a row, each logging `GL context recreated` and `GPU context
restored`, `contextLosses` at 3, frames drawn after each, no dump, and a
clean exit with three forgotten contexts.

The simulation is kept behind the existing switch: `RasterCmd::LoseContext`
falls back to a simulated loss where the driver has no hook (the next drawn
frame reports `ContextLost` with status `Unknown`), and `/gpu?lose=1`
replies `{ forced, simulated }`. The recovery is now testable on every
driver, with the caveat that a simulated loss never resets the GPU: whether
a new context on the same window is healthy after a real NVIDIA reset stays
unmeasured, and the answer, if not, is NVIDIA on the window-recreation list.
