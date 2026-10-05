---
title: A cancelled touch ends its pointer
description: When the system takes a touch away from the app (a system gesture, a palm rejection, a window losing the touch), the pointer stays down forever - the cancel is never translated on any platform, so a press, a drag or a pinch that was in flight never ends. Translate it, give the app a way to tell a cancel from a lift so a cancelled press fires no tap, and give the recognizers one cause-agnostic cancel with a consumer callback.
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
  became ours, okf/plans/event-timestamp.md). The activity's surface
  already forwards the cancel with every pointer's position. The
  platform sends one when a system gesture takes the touch over (the
  back swipe, the notification shade), on palm rejection, and when the
  window loses the gesture.
- Every other platform: SDL sends `SDL_EVENT_FINGER_CANCELED` (Wayland's
  `wl_touch.cancel`, UIKit's `touchesCancelled`, macOS) with the finger
  id and its last position in the `tfinger` payload; the sdl3 crate has
  no variant for it, so it arrives as `Event::Unknown { type_ }` with the
  payload gone, and `translate_event` (`alloy/src/event.rs`) has no arm
  for it.

Not a cancel source, checked 2026-10-05: a desktop focus loss. SDL's
Wayland backend releases every pressed mouse button on pointer leave,
so an up arrives; and the dev-tool input mute passes releases through.
The platform touch cancels above are the only producers, which is why
the synthetic `cancel` action below is the desk reproduction.

## Design

**One contract, the web's.** A cancel is the second way a pointer ends,
beside the lift, with the web's name at every layer: `PointerCancel` in
alloy, `InputEvent::PointerCancel` and `RoutedKind::Cancel` in the
router, `"pointerCancel"` on the bus, `onPointerCancel` on elements, on
sprites, groups and layers, on nodes and scenes. The simplified
contract: a cancel is the last event for its pointer and carries its
last known position and no button; it routes along the frozen down path
like an up, and a touch gets its final leave after it exactly as after
an up. Nothing else about the pointer survives it.

**A variant, never a flag on up.** A flag would make every existing up
handler responsible for checking it, and one missed check is a tap that
should not have fired. A cancel reaching a handler that only knows ups
is ignored, which is the safe default.

**The runtime side is symmetric with the lift.** The router already
models "pressed" as the frozen down path and the resampler as a pointer
history; both drop their entry on a cancel. The resampler sends no flush
move ahead of it: a cancelled pointer has no last position worth
reading. The runner's input state drops a touch pointer on it as on an
up.

**The SDL payload is read raw.** The pump loop polls raw `SDL_Event`s
through two thin wrappers in `sdl_utils.rs` (poll, and wait with a
timeout). A `FINGER_CANCELED` is translated from its `tfinger` payload;
every other event goes through the crate's own `Event::from_ll` as
before, and the `EventPump` handle stays for the crate's one-pump
invariant. The crate gap is recorded in
okf/upstream/sdl3-no-finger-cancelled-variant.md; when the variant
lands, the wrappers go.

**The recognizers get one cause-agnostic cancel with a callback.** The
recognizers in `packages/core` (pan, transform, swipe, long press,
double tap, the pointer feed) already end a gesture without its result
through `cancel()`, for an arena steal and for an unmount. A platform
cancel is a third cause of the same thing, and the first two had the
same hole this plan is about: a ScrollView whose pan loses the arena
mid-drag got no `onPanEnd` and never settled. So the one path gets a
consumer callback, Flutter's `onCancel` and Hammer's `pancancel`:
`onPanCancel`, `onTransformCancel`, `onSwipeCancel`,
`onLongPressCancel`. `onEnd` never fires for a cancel of any cause, and
a cancelled press (`createPress` in components) fires no `onPress`. The
double tap settles its first tap as lost and releases its second; the
pointer feed ends its brackets with no velocity and no swipe.

**A synthetic `cancel` pointer action** in the input plan
(`lattice/src/input_plan.rs`), so `/input`, the MCP `send_input` tool
and the app test harness can cancel a pointer on the desk. Without it
the feature is verified on a device once and never again.

Not widened, on purpose: a runtime `cancel_all` for pointers the runner
knows will never lift (no producer today, see above); a reason on the
cancel event (the web has none, no consumer would branch on it);
unifying the Rust pointer variants into one `Pointer { kind, .. }`
(a refactor of its own, unrelated to cancel).

## What done looks like

- A cancelled touch ends its pointer on every platform: the resampler
  history goes, the router's press path is released, the input state
  drops the pointer, the touch gets its final leave.
- An app can tell a cancel from a lift: `onPointerCancel` beside
  `onPointerUp`, the same event shape. A cancelled press is not a tap, a
  cancelled drag is not a fling, a cancelled long press does not fire.
- The recognizers' cancel is one path with one callback per
  recognizer, run for an arena steal and for a platform cancel alike.
- The 2d and 3d dispatchers end the press on it and walk it to the
  press target; never a tap.
- `{ "type": "pointer", "action": "cancel" }` is an input action.

## Stages

1. alloy: the event, the raw SDL poll, the Android arm, the resampler,
   the router and its interest bit. Tests in `src/tests/router.rs` and
   `src/tests/resample.rs`.
2. flux and lattice: marshal and route it like an up; the `cancel`
   input action with its test in `lattice/src/tests/input.rs`; the
   `debugging.md` action list.
3. core: the bus subscription, the element prop and its doc, the
   interest bit, the recognizers' cancel callbacks and handlers, the
   pointer feed. Tests in `packages/core/tests/gesture.test.ts`.
4. components: `createPress`, every wired component, the pass-through
   props.
5. 2d and 3d: the handler sets, the dispatchers, the component props,
   `feedPointer`. Tests in their `dispatch.test.ts`.
6. An app test: a down then a cancel on a Pressable fires no press and
   reaches `onPointerCancel`.
7. On a device: a back swipe started inside a dragged element on
   Android ends the drag.

## State (2026-10-05)

Stages 1 to 6 are built and verified headless: alloy's router and
resampler tests, lattice's input plan test, core's gesture tests (the
timer ones on the headless client), the 2d and 3d dispatch tests, and an
app test driving `down`, `move`, `cancel` through the real pipeline
(`packages/test/tests/app.test.tsx`); the whole JS suite passes. Stage 7 ran
on the Android tablet (SM-T500, Android 12, three-button navigation)
the same day with the new client installed: a real `ACTION_CANCEL`
injected into the app's touch stream (`adb shell input motionevent
DOWN/MOVE/MOVE/CANCEL`, the stream a system gesture produces) ended a
drag with `onPointerCancel`, `onPanCancel` and the final leave, no up,
and the next tap on the same pointer id was a fresh press. The SDL path
(`sdl_utils::finger_cancel`, Wayland's `wl_touch.cancel`) is type-checked
and reviewed but has run on no touch screen: no machine here has one;
okf/tiny.md carries that check.

## Findings

- What cancels a pointer and what only looks like it does (a desktop
  focus loss, the input mute): okf/notes/pointer-cancel-sources.md.
