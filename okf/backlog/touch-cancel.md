---
title: A cancelled touch ends its pointer
description: When the system takes a touch away from the app (a system gesture, a palm rejection, a window losing the touch), the pointer stays down forever - the cancel is never translated on any platform, so a press, a drag or a pinch that was in flight never ends. Translate it and give the app a way to tell a cancel from a lift, so a cancelled press fires no tap.
created: 2026-09-30
---

# A cancelled touch ends its pointer

Symptom: a touch the system cancels leaves its pointer down. The router
keeps the frozen down-path, the input state keeps the pointer, a
recognizer keeps its drag or its pinch armed, and the next touch with
the same id starts on top of the stale one. Nothing ends it.

Where it is dropped:

- Android: `alloy/src/touch.rs` passes on down, move and up and returns
  on `ACTION_CANCEL` (kept as SDL's path had it when the touch path
  became ours, okf/plans/event-timestamp.md). The platform sends a cancel
  when a system gesture takes the touch over (the back swipe, the
  notification shade), on palm rejection, and when the window loses the
  gesture.
- Every other platform: SDL sends `SDL_EVENT_FINGER_CANCELED` (Wayland's
  `wl_touch.cancel`, UIKit's `touchesCancelled`, macOS); the sdl3 crate
  has no variant for it, it arrives as `Unknown`, and
  `translate_event` (`alloy/src/event.rs`) has no arm for it.

## What done looks like

- A cancelled touch ends its pointer on every platform: the resampler
  history goes, the router's press path is released, the input state
  drops the pointer.
- An app can tell a cancel from a lift. A cancelled press is not a tap, a
  cancelled drag is not a fling, a cancelled long press does not fire.
  The web's shape is a `pointercancel` event next to `pointerup`; the
  recognizers in `packages/core` (pan, transform, swipe, tap, double
  tap, long press, the pointer feed) and the 2d and 3d dispatchers end
  their gesture on it without its result.
- The 2d and 3d layer and scene events pass it on.

## What it involves

- alloy: a cancel event beside `PointerUp` (or a flag on it; to decide
  with the JS shape), fed through the resampler like an up but without
  the flush move: a cancelled pointer has no last position worth
  reading. The SDL arm recovers the event by raw type id, as the theme
  and camera events are.
- flux and lattice: route and dispatch it like an up.
- core: `onPointerCancel` on the element props and the recognizers'
  handler sets, which today are down, move and up; every consumer that
  spreads `handlers` picks it up, every hand-wired one has to add it.
- Verification needs a real device: a back swipe started inside a
  dragged element on Android is the cheap reproduction.
