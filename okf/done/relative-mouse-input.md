---
title: Relative mouse input (mouse look)
description: No pointer-lock / relative-motion path exists anywhere in the surface, so first-person control is impossible however good the GPU gets; SDL already has the capability and alloy already drops the deltas on the floor.
created: 2026-07-31
---

# Relative mouse input (mouse look)

Both stages are implemented (2026-08-20, uncommitted): movementX/movementY
on pointer events end to end, and lockPointer()/pointerLocked() in
packages/core/src/window.ts over AlloyCommand::SetPointerLock and the sticky
"pointerLock" fact. Resampler movement contract covered by unit tests
(alloy/src/tests/resample.rs). Open before this moves to done: on-device
verification with a real mouse (lock engage/release, deltas while locked).

Pointer events carry absolute window coordinates and nothing else. There is
no way for an app to ask for unbounded mouse motion, so mouse look is not
approximable: the cursor hits the window edge and the deltas stop. Every
first-person workload is blocked on this before it is blocked on any GPU
feature.

The claim is old and has been carried across analyses since the day vertex
pipelines shipped ([gpu-stack-maturity, 2026-07-15](
../notes/gpu-review.md) - the file was replaced by gpu-review.md on
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
  `yrel`. `alloy/src/event.rs:369` destructures `{ which, x, y, .. }` and
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
3. `alloy/src/app.rs` + `lattice/src/runtime.rs` - carry the deltas
   through the resampler (all moves are frame-batched through
   `alloy/src/resample.rs`; the pump feeds it producer-side and moves
   never travel as events since frame-batched-pointer-input landed and
   alloy took over the feeding), and a verb to set the mode.
4. `flux/src/alloy_plugins/input.rs` - marshal the delta fields onto the
   dispatched event object.
5. `packages/core/src/window.ts` + `types.d.ts` - the JS surface, and the
   `PointerEvent` fields.

## Traps

- **Coalescing eats deltas.** Moves are collapsed to the latest position
  per pointer before dispatch in the resampler's History
  (`alloy/src/resample.rs`, latest-per-frame-slot for all pointer types;
  the old batch-loop collapse in `lattice/src/lib.rs` is gone - moves
  never cross the channel anymore). Positions collapse correctly; deltas
  do not. They must be **summed** into the History entry, never
  overwritten, or fast motion silently loses distance - the faster the
  flick, the more it loses, which is the worst possible failure shape for
  mouse look.
- **Absolute coordinates go meaningless in relative mode.** SDL keeps
  sending `x`/`y`, but they no longer track a real cursor. The contract has
  to say what `clientX`/`clientY` report while locked (the web freezes them
  at the lock point) and hit-testing/hover has to be considered: there is
  no cursor to hover with.
- **Touch deltas are ambiguous under extrapolation.** Touch moves go
  through the resampler's gap-bridging extrapolation, a path with no notion
  of relative motion: summing real deltas reports 0 on a bridged frame then
  a double; mirroring dispatched positions makes the settle-back step report
  a reverse delta the finger never made. Contract: touch (and pen, which has
  no hardware deltas either) reports `movementX`/`movementY` as the
  dispatched-position diff, so deltas bounce exactly when positions bounce -
  consistent, and the `prev` tracking already exists.
- **Per-frame hit-testing.** Whatever the locked contract says about
  coordinates, it has to survive the hit/hover recompute that runs on every
  animation frame.
- **down() destroys the accumulator.** The resampler's `down()` re-seeds the
  History wholesale and `remove()` drops it on up, so a click mid-flick
  discards that frame's accumulated, undispatched delta - and clicking while
  turning is THE mouse-look gesture. The delta accumulator must survive the
  down-reseed.
- **The push signature change ripples to every producer.** Under the
  producer-side rule, synthetic input (MCP `send_input`, record/playback)
  feeds the resampler at its own send sites (`lattice/src/lib.rs:383`) with
  positions only - no `xrel` exists there. Synthetic moves report movement as
  the position diff, which is the honest synthetic delta.

## Shape

Staged, bare minimum first:

1. **Deltas on the event.** `movementX` / `movementY` on `PointerEvent`,
   always populated, no mode and no new verb. Useful on its own (drag
   deltas without bookkeeping, relative scrubbing) and it is the whole
   plumbing job for stage 2. Cheap and unbreaking. Mouse sums hardware
   `xrel`/`yrel`; touch and pen report the dispatched-position diff;
   synthetic producers report the position diff; down/up events report 0
   (matching browser practice).
2. **The lock.** A verb that puts the window into relative mode. Through
   the solidrt lens this is not the web's element-scoped
   `element.requestPointerLock()` plus permission dance: there is one
   application and one window, so the honest shape is a window-level call
   next to `exit()` in `packages/core/src/window.ts`, with a reactive
   accessor for the current state (the OS can drop the lock on focus loss,
   so apps must be able to observe it, not just set it).

`movementX`/`movementY` stay verbatim - they are the standard names for
exactly this quantity and the shadertoy-style "runs unmodified" argument
applies. The lock verb follows window.ts conventions (verbs like `exit()`,
reactive noun getters like `windowFocused()`): `lockPointer(locked)` plus a
reactive `pointerLocked()` accessor.

## Decisions

- **Frozen coordinates are OURS to enforce.** SDL does NOT freeze `x`/`y`
  in relative mode: it reports a window-clamped simulated position (verified
  on Wayland and Windows), so hit testing would follow an invisible point
  and clicks land somewhere unknowable. Alloy freezes mouse coordinates at
  the lock point while locked (web parity; `pointer_lock_frozen` in the
  pump), so clicks are deterministic - lock via a button and clicks hit
  that button. Hover consequently sticks to the frozen point through the
  per-frame hit-test; v1 accepts that (a locked app is not a hover app)
  rather than suppressing hover.
- **Cursor visibility is implied by the lock** (SDL hides it). The unused
  `SetCursorVisible` stays an independent knob for a later custom-cursor
  story; the lock does not touch it.
- **Raw deltas.** No sensitivity or acceleration mode; the app scales.
  Faithful facts, no core magic - and consistent with
  [pointer-position-filtering](pointer-position-filtering.md): any future
  runtime filter smooths positions, deltas stay raw.

## Verification findings (Windows, 2026-08-20)

Verified on the winbox (RTX 3070, ANGLE D3D11, native win32 client): the OS
clips the locked cursor properly - unbounded deltas through arbitrary
sweeps, no margins, no dead states, and the lock drops on alt-tab and
re-engages on click-back. With the coordinate freeze, clientX/clientY hold
the lock point and clicks land deterministically (locking via a button
means clicks while locked hit that button). Windows is the reference
behavior.

## Verification findings (Linux/Wayland, 2026-08-20)

Verified on-device with probes/mouse-look-probe.tsx (real mouse, Hyprland
0.56, two monitors). The engine layer is correct end to end: the WAYLAND_DEBUG
trace shows lock requested -> zwp_locked_pointer granted -> raw
relative_motion deltas consumed -> movementX/movementY in JS, including
while locked. Synthetic moves report position diffs as designed, movement
survives reload, and the sticky "pointerLock" fact tracks the applied state.

Retested after the coordinate freeze landed: three consecutive locked
sessions on Hyprland worked perfectly (frozen clientX, deterministic
clicks, clamped virtual cursor). The freeze removed every artifact that
depended on SDL's drifting simulated position; what remains exposed to the
compositor's non-pinning is only delta loss while the hidden cursor is
outside the window, which normal mouse-look wiggling rarely triggers.

The rough edges are the compositor's, both documented upstream:

- The lock does not survive focus loss (Super-key window switch drops it:
  unlocked() + keyboard/pointer leave). SDL re-requests immediately, but the
  re-lock only activates once the pointer is over the surface again - a
  keyboard-refocused window stays delta-dead until the mouse re-enters.
  App-visible signal: windowBlur/windowFocus. The game pattern is
  pointerLocked() && !windowFocused() -> pause + "click to resume".
- Hyprland grants the lock but does not pin the cursor
  (hyprwm/Hyprland#4650); with multiple monitors the cursor is released
  when it crosses to another output (hyprwm/Hyprland#4464). Symptoms: a
  hidden cursor that wanders, clientX clamping at window edges, movement
  dying while the cursor is off-surface, and an app-drawn virtual cursor
  drifting against the real one. Nothing to fix on our side; newer Hyprland
  reworked cursor handling.

While locked, hit testing follows the SDL-reported (clamped, not frozen on
this backend) position - an app drawing its own virtual cursor from
movement deltas must do its own picking for aim-clicks, same as on the web.

## Verification scope

Verified on-device: Linux/Wayland (Hyprland) and Windows. Unverified:
macOS, Android-with-mouse, X11 - each has documented expected behavior
(the coordinate freeze is ours and platform-independent; the lock is a
best-effort OS grant that drops on focus loss everywhere, covered by the
`pointerLocked() && !windowFocused()` -> click-to-resume app pattern).
macOS uses an OS-level primitive like Windows and should match it; Android
pointer capture drops on focus loss aggressively and SDL reclaims
(`reclaimRelativeMouseModeIfNeeded`); touch devices never need the lock -
touch drag deltas flow through movementX/movementY without it. Each
platform gets its pass when its client is next exercised.
