---
title: Impeller wrapped targets clear; EGL buffer age facts per stack
description: Impeller never loads a wrapped FBO's existing content (every wrap_fbo draw starts cleared), so partial redraw must compose offscreen and blit; buffer-age and damage-extension availability as measured per stack.
created: 2026-09-02
---

# Impeller wrapped targets clear; EGL buffer age facts per stack

Measured by `alloy/examples/partial_repaint_probe.rs` (kept runnable) for
[partial-repaint](../done/partial-repaint.md); true independent of that
work.

**Impeller never loads a wrapped target.** A `wrap_fbo` + draw starts
from a cleared target, always: with FBO 0 pre-filled and a display list
whose root is a clip rect, every pixel outside the clip comes back
black, and a blend inside the clip blends against black, not the
previous content. A DL clip does confine the draw (nothing painted
outside it), but preserved-content composition over an existing buffer
is impossible through Impeller - compose in an offscreen target and
blit the region instead. This is also why alloy's draw paths do their
own glClear before every Impeller draw (it is not redundant paranoia).

**Rig and FBO 0 share orientation.** Impeller treats every wrapped FBO
as a bottom-up window target, so a rect blit between a window-sized rig
and FBO 0 uses the identical rect on both sides (one top-down to GL
flip, no mirroring). Validated pixel-exact in the probe's phase C.

**Buffer age per stack:**

- Linux desktop, Mesa Intel (RPL-P, GLES 3.2, SDL on EGL/Wayland):
  `EGL_EXT_buffer_age` present and honest - age settles at 3 after 4
  warm-up swaps, and the back buffer's content always matched the frame
  from `age` swaps ago. `eglSwapBuffersWithDamageKHR` and
  `eglSetDamageRegionKHR` (with `EGL_KHR_partial_update` listed) both
  resolve. Plain `eglSwapBuffers` (SDL's swap) keeps age valid; the
  damage entry points are only compositor hints.
- Philips TPM171E (MediaTek Mali-T860, Android): app-side display
  extensions probed 2026-09-02 - `EGL_KHR_partial_update` and
  `EGL_KHR_swap_buffers_with_damage` are listed, `EGL_EXT_buffer_age`
  is NOT. The KHR extension defines the same age attribute
  (`EGL_BUFFER_AGE_KHR` = 0x313D), so the age query works; an
  EXT-only string check reports a false negative on this stack (bug
  fixed in `raster::buffer_age` the same day - either extension now
  enables the query). Ages come back valid every frame at steady
  state. `eglSetDamageRegionKHR` resolves and accepts per-frame
  sub-rects, but does not reduce timed GPU frame cost on the
  multisampled fast path (see okf/backlog/display-list-op-cost.md).
  The window backbuffer there is 4x multisampled (the in-tile fast
  path), where partial composition does not apply anyway.

The raster thread can query buffer age with khronos-egl against
`eglGetCurrentDisplay`/`eglGetCurrentSurface` - the dlopen'd libEGL is
the same module SDL loaded (system lib, or the packaged ANGLE next to
the executable), so per-thread current state is shared. No SDL swap
changes needed (`alloy/src/raster/buffer_age.rs`).
