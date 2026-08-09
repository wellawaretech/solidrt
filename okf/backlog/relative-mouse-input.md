---
type: backlog-item
title: Relative mouse input (mouse look)
description: No pointer-lock / relative-motion path exists anywhere in the surface, so first-person control is impossible however good the GPU gets; SDL already has the capability and alloy already drops the deltas on the floor.
status: open
timestamp: 2026-07-31T00:00:00Z
---

# Relative mouse input (mouse look)

Pointer events carry absolute window coordinates and nothing else. There is
no way for an app to ask for unbounded mouse motion, so mouse look is not
approximable: the cursor hits the window edge and the deltas stop. Every
first-person workload is blocked on this before it is blocked on any GPU
feature.

The claim is old and has been carried across analyses since the day vertex
pipelines shipped ([gpu-stack-maturity, 2026-07-15](
../analysis/gpu-review.md) - the file was replaced by gpu-review.md on
07-30, which restates it at "First-person anything" and again in the
do-order notes). It had never been filed. Filed 2026-07-31.

## What already exists

- **SDL has it.** sdl3 0.18.4 exposes
  `MouseUtil::set_relative_mouse_mode(&window, bool)` and
  `relative_mouse_mode(&window)` over `SDL_SetWindowRelativeMouseMode`,
  plus `warp_mouse_in_window` and `show_cursor`. In relative mode SDL hides
  the cursor, keeps it from leaving the window, and reports motion as
  deltas.
- **The deltas already arrive.** `SdlEvent::MouseMotion` carries `xrel` and
  `yrel`. `alloy/src/event.rs:295` destructures `{ which, x, y, .. }` and
  discards them.
- **Cursor hiding is half-built.** `AlloyCommand::SetCursorVisible` exists
  (`alloy/src/event.rs:15`, handled at `alloy/src/app.rs:436`) and nothing
  in the tree ever sends it.
- **Android is plumbed too.** The vendored SDL Java shim has
  `supportsRelativeMouse` / `setRelativeMouseEnabled` /
  `reclaimRelativeMouseModeIfNeeded`
  (`lattice/android/app/src/main/java/org/libsdl/app/SDLControllerManager.java`),
  so the capability is not desktop-only, though it is meaningless on a
  touch-only device.
- **Gamepads are the current answer.** `packages/core/src/gamepad.ts` gives
  analog-stick look on TV and controller setups, which is why the gap has
  not been fatal so far.

## Where it plugs in

1. `alloy/src/event.rs` - keep `xrel`/`yrel` on `AlloyEvent::PointerMove`,
   scaled by the same `mouse_scale(window)` factor as `x`/`y` (SDL reports
   both in window units, which are logical points on macOS/Wayland and
   physical pixels on Android/X11).
2. `alloy/src/event.rs` + `alloy/src/app.rs` - an `AlloyCommand` for the
   mode (main-thread window op, exactly like `SetFullscreen`).
3. `lattice/src/lib.rs` + `lattice/src/runtime.rs` - carry the deltas
   through the resampler (all moves are frame-batched through
   `lattice/src/resample.rs` since frame-batched-pointer-input landed;
   the pending-move gate is gone), and a verb to set the mode.
4. `flux/src/plugins/gui/input.rs` - marshal the delta fields onto the
   dispatched event object.
5. `packages/core/src/window.ts` + `types.d.ts` - the JS surface, and the
   `PointerEvent` fields.

## Traps

- **Coalescing eats deltas.** Moves are collapsed to the latest position
  per pointer before dispatch, twice: the lattice event batch loop
  (`lattice/src/lib.rs` coalescing rules) and the resampler's History
  (`lattice/src/resample.rs`, latest-per-frame-slot for all pointer types).
  Positions collapse correctly; deltas do not. They must be **summed** into
  the History entry (and across the batch-loop collapse), never
  overwritten, or fast motion silently loses distance - the faster the
  flick, the more it loses, which is the worst possible failure shape for
  mouse look.
- **Absolute coordinates go meaningless in relative mode.** SDL keeps
  sending `x`/`y`, but they no longer track a real cursor. The contract has
  to say what `clientX`/`clientY` report while locked (the web freezes them
  at the lock point) and hit-testing/hover has to be considered: there is
  no cursor to hover with.
- **Mouse and pen only.** Touch moves go through the resampler
  (`lattice/src/runtime.rs`), a separate path with no notion of relative
  motion.
- **Per-frame hit-testing.** Whatever the locked contract says about
  coordinates, it has to survive the hit/hover recompute that runs on every
  animation frame.

## Shape

Staged, bare minimum first:

1. **Deltas on the event.** `movementX` / `movementY` on `PointerEvent`,
   always populated, no mode and no new verb. Useful on its own (drag
   deltas without bookkeeping, relative scrubbing) and it is the whole
   plumbing job for stage 2. Cheap and unbreaking.
2. **The lock.** A verb that puts the window into relative mode. Through
   the solidrt lens this is not the web's element-scoped
   `element.requestPointerLock()` plus permission dance: there is one
   application and one window, so the honest shape is a window-level call
   next to `exit()` in `packages/core/src/window.ts`, with a reactive
   accessor for the current state (the OS can drop the lock on focus loss,
   so apps must be able to observe it, not just set it).

Naming is open. `movementX`/`movementY` are worth keeping verbatim - they
are the standard names for exactly this quantity and the shadertoy-style
"runs unmodified" argument applies. The lock verb has no obvious
single-window precedent to copy.

## Open questions

- Does the lock survive backgrounding and focus loss on each platform, and
  what event tells the app it was dropped? Android's
  `reclaimRelativeMouseModeIfNeeded` implies the answer is "not always".
- Cursor visibility: implied by the lock (SDL hides it), or an independent
  knob now that `SetCursorVisible` exists unused? An FPS wants both fused;
  a custom-cursor app wants them separate.
- Sensitivity and acceleration: report raw SDL deltas and let the app scale
  them, or expose a mode? Raw is the layering-consistent answer (faithful
  facts, no core magic).
