---
title: Discrete pointer gestures - swipe, fling velocity, long-press, double-tap
description: The recognizer family stops at press, pan and transform; a swipe (direction decided at the lift), a fling (velocity handed to whoever animates on), a long-press and a double-tap are each rebuilt by hand or missing (ScrollView has no momentum, ContextMenu has no touch path, nothing dismisses on a swipe), and none is bindable through the input map. One velocity tracker under every recognizer, three recognizers in the arena with the wait-for-failure relation double-tap needs, and the same gestures as pulsing button sources on the pointer feed.
created: 2026-09-22
---

# Discrete pointer gestures

## Symptom

The arena family (okf/done/component-gestures.md) covers what tracks a
finger while it is down: `createPress` (components), `createPan` and
`createTransform` (core). What happens at a lift, or after a wait, or
across two touches is not a recognizer anywhere:

- **Swipe**: a card dismissed sideways, a carousel paged, a row revealed.
  Nothing decides "this drag was a leftward swipe"; an app reads
  `onPanEnd` and re-derives distance and direction, without velocity,
  which `createPan` does not deliver.
- **Fling**: `scroll-view.tsx` says it outright ("there is no momentum
  yet; a fling stops when the finger lifts"). The 2d camera estimates its
  own release velocity from drag deltas (`camera2d.ts`, an EMA with its
  own decay and gate constants); the orbit control would need the same
  again. Velocity at release is a recognizer fact, computed once.
- **Long-press**: `ContextMenu` opens on a secondary click and has no
  touch path; `text-selection-touch-word` (backlog) wants long-press to
  start a selection; `createPress` has no duration.
- **Double-tap**: zoom-to-point in an image or map viewer, select a word;
  nothing recognizes two taps, and the delay problem it brings (a single
  tap must wait out the double-tap window when both are wanted) has no
  arena support.
- **Input map**: `pointer.drag/pan/pinch/twist/wheel/mouseDelta` are the
  feed's sources; a game that dodges on a swipe or charges on a long
  press binds nothing, while the same on a pad is `pad.button("east")`
  or `hold(...)`.

Every framework with a gesture layer ships these four as recognizers
(the 2026-07-23 survey in the done record lists them as the common
vocabulary: tap, long-press, double-tap, pan, fling, pinch, rotate).

## What the engines do

| | Flutter | RN gesture handler | UIKit | Android | Godot | Unity | Web |
|---|---|---|---|---|---|---|---|
| Swipe | drag end with velocity (`onHorizontalDragEnd`, `Dismissible`) | `Fling` gesture: direction flags, fires at recognition | `UISwipeGestureRecognizer`: direction, discrete | `GestureDetector.onFling(vx, vy)` | none (addons) | none (samples) | none |
| Velocity | `VelocityTracker` (least squares over 100 ms), on every drag end | `velocityX/Y` on pan events | `velocity(in:)` on pan | `VelocityTracker` (100 ms horizon) | n/a | n/a | n/a |
| Long-press | `LongPressGestureRecognizer`, 500 ms, slop 18 px | `LongPress` gesture, minDuration, maxDist | `UILongPressGestureRecognizer`, 0.5 s, 10 pt | `onLongPress`, 400-500 ms (ViewConfiguration) | none | none | `contextmenu` on touch, platform-defined |
| Double-tap | `DoubleTapGestureRecognizer`, 300 ms, 100 px slop; single tap delayed when both present | `Tap` with numberOfTaps, `Exclusive(double, single)` | `numberOfTapsRequired`, `require(toFail:)` | `onDoubleTap`, 300 ms | none | none | `dblclick`, single fires too |
| Arbitration | per-pointer arena, `GestureRecognizer` wins/loses, eager victory | relations (simultaneous, waitFor) | `require(toFail:)`, delegate | parent intercept | n/a | n/a | none |

Two things every mature layer has that ours lacks: a velocity tracker
shared by every recognizer, and a "wait for that recognizer to fail"
relation in the arena (UIKit's `require(toFail:)`, RNGH's `waitFor`,
Flutter's delayed tap): without it a double-tap on a node makes its single
tap either fire twice or never.

## Vocabulary

One definition per word, the units the arena already uses (slop and
velocity thresholds in window pixels, finger travel; deltas and velocity
values in the node's parent frame, see `pan.ts` "Frames"):

- **Velocity**: the pointer's speed at the lift, px/s in the parent
  frame, from the positions of the last `VELOCITY_WINDOW_MS` (100 ms,
  Flutter's and Android's horizon) as a least-squares fit, not the last
  two samples (a lift's final sample is often stationary, and frame
  batching makes the last delta a whole frame old). Clamped to
  `VELOCITY_MAX` (8000 px/s, Flutter's `kMaxFlingVelocity`), zero when
  the finger rested `VELOCITY_REST_MS` (50 ms) before lifting.
- **Fling**: a pan's end with velocity above `FLING_MIN_VELOCITY`
  (50 px/s, Flutter's `kMinFlingVelocity`). Not a recognizer: a fact on
  `onPanEnd` and `onTransformEnd`, for whoever animates on.
- **Swipe**: a one-pointer drag that, at the lift, has travelled at least
  `SWIPE_MIN_DISTANCE` (24 px) with velocity at least `SWIPE_MIN_VELOCITY`
  (300 px/s, Android's typical `minimumFlingVelocity` of 50 dp/s times
  the density it assumes) and whose direction is within
  `SWIPE_ANGLE_TOLERANCE` (30 degrees) of one of the four axes. Discrete:
  fires once, at the lift, with `direction` and `velocity`. Direction is
  the dominant axis of the velocity, not of the total travel (a finger
  that wandered then flicked swipes where it flicked).
- **Long-press**: a pointer held `LONG_PRESS_MS` (500 ms: Flutter, iOS,
  Android's upper value) without leaving `LONG_PRESS_SLOP` (10 px, iOS)
  of its down point. Fires at the timer, while still down; the lift
  after it is the end, with the same continuous move stream a pan has
  meanwhile (drag-after-long-press is the reorder idiom).
- **Double-tap**: two taps, each within tap slop and shorter than
  `TAP_MAX_MS`, the second down within `DOUBLE_TAP_MS` (300 ms) of the
  first up and within `DOUBLE_TAP_SLOP` (100 px, Flutter) of it, and at
  least `DOUBLE_TAP_MIN_MS` (40 ms, Flutter's `kDoubleTapMinTime`) after
  it so a bouncing contact is not two taps. Fires on the second down
  (Flutter, iOS) so it is not delayed by the second lift.

Mouse and touch take the same recognizers (a mouse drag swipes, a held
button long-presses, a double-click double-taps); pen counts as touch.
The wheel is not a swipe: a trackpad's two-finger scroll arrives as wheel
events and belongs to `createScroll`.

## Design

### 1. One velocity tracker (core)

`velocity.ts`: a ring of the last samples per pointer (position in the
parent frame, `performance.now()` at handler time, the precedent in
`transform.ts`), fed from the pan and transform recognizers' move path
and read at the lift. Least-squares over the window, the rest and
clamp rules above, the same estimator whatever recognizer asks. It is
the one piece the 2d camera's EMA (`camera2d.ts` `FLING_*`, the
per-update velocity estimate) and any future control would each
re-derive; the camera keeps its own decay but takes the release
velocity from the gesture (see 5).

### 2. Velocity on the existing recognizers (core)

- `createPan`: `onPanEnd(velocity: { vx, vy })`; `createTransform`:
  `onTransformEnd(velocity)` with the focal point's velocity. Zero when
  the gesture ended by cancel.
- `createScroll` and ScrollView: momentum is the first consumer
  (okf/notes/app-structure-performance.md forbids a JS momentum loop, so
  ScrollView projects ONE destination from the velocity under an
  exponential decay, clamps through scrollTo and writes it under an
  ease-out transition scaled by the velocity, Rust animating; a finger
  landing mid-glide stops it, which `createPan`'s start already does).

### 3. Three recognizers in the arena (core, beside pan.ts)

- `createSwipe({ directions?, onSwipe(direction, velocity), onSwipeMove?,
  onSwipeEnd? })`: a pan with axis-aware slop along the allowed
  directions (so a horizontal swipe inside a vertical ScrollView takes
  only horizontal drags, exactly as nested scrollers do today), stealing
  on slop, streaming moves if the consumer tracks the finger (a
  dismissible card moves with it), classifying at the lift. A drag that
  qualified as a pan but not as a swipe ends with `onSwipeEnd` and no
  `onSwipe`, so the consumer snaps back. `directions` defaults to all
  four.
- `createLongPress({ ms?, onLongPress, onLongPressMove?, onLongPressEnd?
  })`: claims provisionally on the down like a press (it is one until
  the timer), arms the timer, cancels itself on slop or on a steal (a
  pan crossing its slop first wins: scrolling a list of long-pressable
  rows must scroll), steals its pointer outright at the timer (so an
  ancestor pan can no longer take the finger: reorder-by-drag after the
  hold), then streams moves. Pressed feedback is the press recognizer's;
  a node with both a press and a long-press shows pressed, fires the
  long-press at the timer and the press never fires (the long-press
  steals it). Timer: `setTimeout` at the down, cleared on any end.
- `createDoubleTap({ onDoubleTap(x, y) })`: tracks the first tap without
  claiming (it must not disturb a press on the same node), then on the
  second qualifying down steals the pointer and fires. Its first tap is
  where the arena relation comes in (4).

Every recognizer follows the lifecycle the done record settled: cancel
retracts without firing, options read at event time, moves and ups on
the frozen down path, an external `cancel()`.

### 4. Wait-for-failure in the arena (core, arena.ts)

A press on a node that also has a double-tap must not fire on the first
lift, or double-tap-to-zoom also single-taps. Every framework answers
this with a relation, not a timeout inside the press: the press's win
is deferred until the double-tap recognizer has failed (the window
passed with no second down, or the second down was too far) or won
(then the press is cancelled). `arena.defer(pointerId, owner, until)`:
a resolved claim whose `until` recognizer is still undecided holds its
firing; the arena fires it on that recognizer's failure and cancels it
on that recognizer's win. The press is the only recognizer that needs to
honor it now (`createPress` gains the deferred fire between its up and
its `onPress`), and a node WITHOUT a double-tap keeps firing at the lift
with no delay, as Flutter guarantees. `createPress`'s pending state is
unrelated (that is the async action guard).

### 5. Sources on the pointer feed (core, input-pointer.ts)

Gestures are what the map binds, so the feed grows:

- `pointer.swipe("Left")` etc.: a button source that pulses for one task
  on the swipe (the shape `tap()` has in input-processors.ts), so
  `input.bind("dodge", pointer.swipe("Left"), pad.button("east"))` and
  the consumer reads `onPress`. The spec grammar is the chord one with
  the direction as the trailing word (`swipe("Ctrl+Right")`), as a drag
  spec ends in its button; a swipe never has a button word (it is the
  primary-button drag's end).
- `pointer.longPress`, `pointer.doubleTap`: pulsing button sources,
  chordable.
- The drag's `end` bracket carries the release velocity in element
  heights per second (the feed's unit): `DeltaSink.end(velocity?)`,
  `GestureListener.end(velocity?)`, `Axes.end(name, velocity?)`,
  `AxesHooks.onEnd(name, velocity?)`. The 2d camera's fling takes it
  instead of its EMA; the orbit control gains the same glide-on-release
  (Three's `enableDamping` momentum) for free. Injection by name:
  `input.end(action, velocity?)`.
- Ids: `pointer:swipe:Left`, `pointer:longPress`, `pointer:doubleTap`;
  `resolve` and `listen` cover them (a rebind on a button action can be
  answered by a swipe).

The feed's swipe, long-press and double-tap run inside its one merged
recognizer rather than as separate arena claimants, as pan/pinch/twist
already do (the header's reason: gestures on one element must not fight
each other over the same fingers); the classifier and timer logic is
shared with the standalone recognizers, not duplicated.

### 6. Consumers that land with it (components, 2d)

- ScrollView momentum (the fling, 2).
- `ContextMenu`: long-press opens it on touch.
- A `Dismissible` (swipe-to-dismiss row/card: tracks the finger, snaps
  back or leaves with the swipe's velocity; the components richness
  direction has it on the list) and `Carousel` paging on swipe.
- 2d camera: fling from the gesture velocity; double-tap zoom-to-point
  as a preset binding (`camera2dBindings` binds `pointer.doubleTap` to a
  `zoomStep` button action if the vocabulary gets one, else the app
  binds it).
- text-selection-touch-word: long-press starts the selection, its move
  stream drags the handle.

## Done looks like

- `velocity.ts` with a headless check over synthetic samples: a constant
  speed reads exactly, a stationary tail reads zero, the window and clamp
  hold, a frame-batched stream (several same-age samples) does not skew.
- `createPan`/`createTransform` deliver velocity; ScrollView flings and
  stops on a landing finger; the 2d camera's own estimator is gone.
- `createSwipe`, `createLongPress`, `createDoubleTap` in core with the
  arena relation; a probe with nested ScrollView + swipeable rows +
  long-press reorder + a double-tap zoom target, driven through
  `send_input` (drags with `delayMs 16` steps give the velocity tracker
  real timestamps; a tap-tap at 100 ms and at 400 ms tells double from
  two singles; a `holdMs 600` tap long-presses) and verified from the
  tree: the row leaves on a fast swipe and snaps back on a slow one,
  the list scrolls on a vertical drag over a row, the single tap fires
  once and late only where a double-tap is registered.
- The feed's sources and the velocity on `end`; `input-map-check.ts`
  covers the pulse, the ids, `resolve` and `listen`; the camera checks
  cover `end(velocity)`.
- AGENTS.md (core: the recognizer family and the arena relation; the
  feed's sources), components docs regenerated, the design note's table
  gains a "Gestures" row, `pointer-coalescing-traps.md` gains the
  velocity-at-lift trap (the last sample is stale, fit the window).

## Not in this item

Multi-finger swipes (three-finger gestures are OS-level), edge swipes
(the system back gesture on Android and iOS is the platform's), hover
gestures, pen pressure and tilt, force touch, and a `Draggable` with
drop targets (a pan consumer, once long-press-then-drag exists it is a
components item of its own). Tap-count beyond two (triple-tap) is a
parameter on double-tap if a consumer appears, not a recognizer.
