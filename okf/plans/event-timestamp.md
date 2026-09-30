---
title: Input events carry a timeStamp
description: PointerEvent carries no timestamp, so the velocity tracker, double-tap and the 2d and 3d dispatchers stamp events with performance.now() at handler time - a wall read that inherits the handler's execution jitter and cannot be stepped by a test or by srt render. Give every input event a timeStamp on an input reading of the paced clock (counted refreshes like the animation timeline, no suspension skip, the arrival time for an event that lands between frames), move package logic off performance.now(), and keep performance.now() for measuring work only. Prerequisite of the test harness's app layer.
created: 2026-09-29
---

# Input events carry a timeStamp

Symptom: gesture logic cannot be tested deterministically, and its timing
input is noisier than the data it is paired with. A delivered move is a
resampled position, one per pointer per frame slot (`alloy/src/resample.rs`),
but the time the velocity tracker pairs it with is `performance.now()` read
whenever the handler happened to run. The tracker's own comment names the
cause: "PointerEvent carries no timestamp". `VELOCITY_MIN_STEP_MS` in
`packages/core/src/velocity.ts` exists to absorb the result (handler-time
stamps within a frame differ by microseconds).

[frame-timing](../design/frame-timing.md) already rules against this for
animation: "A wall read inside a tick inherits the tick's execution jitter
even when presents are metronomic." The recognizers are consumers still
riding the wrong clock.

## Where this stands (2026-09-30)

Two layers. The first (the input reading, `timeStamp` on every event,
package logic off `performance.now()`) is built, verified on the desktop
client and two Android devices, and committed. The second,
[Real sample times](#real-sample-times-decided-2026-09-30), is built,
uncommitted, and verified on the desktop client and both Android devices
with synthetic and injected input; what is owed is a real finger (see
there). The
bullets and measurements below describe the first layer; where they say a
move carries its slot's time or an arrival its arrival time, the second
layer replaced that with the event's own time.

- The input reading in `lattice/src/paced_clock.rs`, five tests beside
  the existing seven.
- The frame verb ticks the clock ahead of the move dispatch; lattice
  stamps every event on arrival and hands the stamp to flux
  (`input::dispatch`, `input::refresh_hover`, `input::frame_end`,
  `events::forward` each take it).
- `timeStamp` on pointer, wheel, key and global pointer events, on the
  `pointerFrame` terminator, on the `gamepads` event, and on the 2d and
  3d layer and scene events.
- The velocity tracker, pan, swipe, transform, the pointer feed, the
  double-tap and the 2d and 3d dispatchers read the event's stamp;
  `tap` and `doubleTap` read their source's `changedAt`.
- The tests: 308 in 29 files under `srt test`. Ten gesture tests and
  three interaction tests came back from `checks/` on stated times, the
  double-tap window asserted to the millisecond (39, 40, 300, 301 ms).
  Four tests stay parked because they wait on a timer:
  `packages/core/checks/gesture-timers.test.ts` (3) and
  `input-map-hold.test.ts` (1).

Measured on the release desktop client through the control API
(`probes/timestamp-probe.tsx`): a tap held 30 ms reads 30.2 ms between
its down and up and the next down 98.8 ms later, so arrival events carry
their arrival time; six moves of one drag read exactly 16.667 ms apart
and the up 4.8 ms after the last; the lift velocity of 20 px per frame
reads 1200.0 px/s; the double tap and the key tap fire. With the dev
clock paused a down and an up 200 ms apart read the same stamp, and
three steps move the reading 50.0 ms. `srt render` still writes its
frames.

On the two Android devices (Pixel 7 at 90 Hz, SM-T500 at 60 Hz), the
same probe driven through the control API, synthetic touch:

| check | Pixel 7 | SM-T500 |
| --- | --- | --- |
| tap held 30 ms, second tap 100 ms later | 33.3 and 101.5 ms, double tap | 30.8 and 101.5 ms, double tap |
| moves of one drag | 11.111 ms apart | 16.667 ms apart |
| the same taps with the app held near 15 fps | 30.5 and 101.3 ms, double tap | 30.5 and 101.4 ms, double tap |
| tap, 4 s in the background, tap | 6390 ms apart, two singles | 6498 ms apart, two singles |

The third row is what the first device run got wrong and the reason an
event is stamped where it is sent: the UI loop shares its thread with
the engine, so under a 45 ms frame an event sat in the channel and a
30 ms press read 47 to 58 ms when it was stamped on being taken out.
`EventSender` (`lattice/src/runtime.rs`) now notes the wall reading of
every send on the sender's thread, and an arrival stamped after the next
tick keeps its own time.

Found by the same runs and open, see
[Open details](#open-details): below the refresh rate a rest before the
lift is not detected.

Flings by a real finger, 29 drags over the two devices (2026-09-30).
The probe tracks each drag twice over the same positions, on the event
stamps and on `performance.now()` in the handler, which is the before
and after on one input:

- Every move stamp of every drag is a whole number of periods from the
  one before (11.111 ms on the Pixel, 16.667 ms on the tablet). The
  handler times of the same moves spread by 0.0 to 1.2 ms (standard
  deviation per drag) around that, 3.9 ms in one drag.
- The lift velocities agree within 2 percent on every drag (the largest
  differences -1.7 and +1.6 percent). Fifteen of the 29 are at the
  tracker's 8000 px/s clamp either way, where the two cannot differ.
- A drag that stopped for 196 ms before the lift reads zero on both.
- So on these devices, at the refresh rate and with nothing else running
  in JS, the change is neutral for the fling: the handler's jitter was
  already small. What it removes is the dependence on that, since a
  handler delayed by other work in the frame no longer moves a stamp.

## Decision (2026-09-29, the clock revised 2026-09-30)

- Every input event carries `timeStamp`, the web name, in milliseconds.
- The stamp is a reading of its own on the paced clock, the input
  reading (see the next section for why it is neither existing timeline).
- Under `srt render` and under a test the stamp comes from the stepped
  clock, with no test-mode special case in the recognizers.
- `performance.now()` stays real elapsed time, for measuring work. Package
  logic does not read it.

Reversed 2026-09-30: real sample times were rejected here at first (SDL's
Android path carries none, so the resampler modelled slots and a move was
stamped with its slot's time). The rest detection below the refresh rate
showed what that costs, and the investigation showed the times are within
reach. See [Real sample times](#real-sample-times-decided-2026-09-30),
which replaces the slot model and the slot stamp of a move.

Rejected: virtualizing `performance.now()` under test. See decision D6 in
[test-harness](test-harness.md).

## The input reading

The first form of this decision (2026-09-29) put the stamp on the
animation timeline. Reading the code on 2026-09-30 showed that neither
timeline can carry it:

| property | animation timeline | timer timeline | input needs |
| --- | --- | --- | --- |
| advances by counted refreshes, so moves are evenly spaced | yes | no | yes |
| lives through a suspension | no | yes | yes |
| has a reading between frame signals | no | yes | yes |
| dev clock pause, scale and step | yes | yes | yes |

Why each row is needed:

- Counted refreshes. The resampler delivers one position per pointer per
  frame slot and assumes consecutive samples one slot apart
  (`alloy/src/resample.rs`). A slot position paired with the frame
  signal's wall time is the mismatch this item exists to remove;
  `VELOCITY_MIN_STEP_MS` is the patch over it.
- A reading between signals. Moves dispatch inside the frame; down, up,
  key and wheel dispatch on arrival. Stamped with the last signal's
  reading they are late by up to one frame interval. At the refresh rate
  that is a period and harmless (an idle client ticks at the refresh
  rate too: [idle-onframe-tick-rate](../done/idle-onframe-tick-rate.md),
  closed 2026-09-24). Below it, it is not: in a JS-bound app at 20 fps
  the interval is 50 ms, the size of `VELOCITY_REST_MS`, so whether a
  lift reads as rested would depend on where in the frame the up
  arrived, and during a stall every arrival reads the same instant (two
  taps 0 ms apart, under `DOUBLE_TAP_MIN_MS`, a bounce). The time of an
  up should not depend on the app's frame rate.
- Living through a suspension. The animation timeline advances one period
  across a gap over `SUSPEND_MS`. Two taps on either side of a background
  stretch would read as close together, and an arrival time taken during a
  stall would be rewound by the next signal.

So `PacedClock` gains a third reading beside `now_ms` and `timer_now_ms`:

- At a frame signal it advances by the refresh count alloy reported, with
  no suspension skip; paused, stepped and scaled frames advance it as they
  advance the other two.
- Between signals it reads the arrival time: the latched reading plus
  the wall time since the signal, under the same pause and scale policy
  (it stands still while paused). Decided 2026-09-30: an event that lands
  between frames is stamped with when it arrived, not with the last
  frame's time, which is what `performance.now()` at handler time gives
  an app today. This exists in a running app only. In playback and under
  test no wall time passes between frames, so there is nothing to add
  and an arrival event reads its frame's time: a test stays fully
  stepped.
- A move is stamped with the latched reading of its frame's signal, the
  slot's time. An arrival event (down, up, key, wheel) is stamped with
  the arrival time.
- Stamps never go back: an arrival stamp can land a little past the next
  signal's reading (the signal's execution jitter), and a later stamp is
  then held at the earlier one. The reading itself is not moved, so the
  overshoot does not accumulate.
- In playback and under test it is frame / fps, as the other two are
  (frame-timing D6).

It stays within a period of the timer timeline outside scaled stretches,
so a recognizer that mixes stamps with `setTimeout` windows of hundreds of
milliseconds (double-tap, long-press) is consistent.

Rejected: the timer timeline for every event. It exists, has a reading
between signals and lives through suspensions, so it is the least work and needs
no device check; but a move would be stamped with the frame signal's wall
time, which keeps the jitter against slot positions that the item is
about. Rejected: the animation timeline with an arrival time added (the
suspension row: the skip rewinds an arrival stamp and folds a background
stretch to one period). Rejected: no arrival time, an event between
frames stamped with the last signal's reading. The clock would be simpler
and an app at the refresh rate would lose nothing, but below it the
stamp is late by up to a frame interval (50 ms at 20 fps, the size of
`VELOCITY_REST_MS`), a regression against today for exactly the slow
apps.

## What it involves

In lattice and flux:

- The third reading in `lattice/src/paced_clock.rs`, with tests beside the
  existing ones, the never-go-back rule among them (an arrival stamp past
  the next signal's reading holds the following stamp at the earlier
  value): tests under `srt test` cannot see the arrival time, so this
  part rests on the Rust tests and the device checks.
- The clock tick moves ahead of the move dispatch inside the frame verb
  (`lattice/src/runtime.rs`; moves dispatch before `pc.tick` today), so a
  move is stamped with its own frame's reading. An order change only: the
  frame signals, the refresh count, the cadence hold and the resampler
  stay as they are.
- The stamp is set where the runtime builds the event object
  (`build_pointer_obj` in `flux/src/alloy_plugins/input.rs`, `emit_key` in
  `flux/src/alloy_plugins/events.rs`). Lattice owns the clock: it passes
  the stamp with every event it hands flux, the slot reading for a move
  and the arrival time for the rest, taken where the event is sent to the
  UI loop (`EventSender`), not where the loop takes it out. The
  rendertree is not involved.
- `PointerEvent`, `WheelEvent`, `KeyEvent` and the global pointer event in
  `packages/core/src/types.d.ts` gain the field. So does the `gamepads`
  event (`flux/src/alloy_plugins/events.rs`): pad state reaches JS as an
  event on change, not as a poll per frame, so it is stamped like any
  other arrival.
- `okf/design/frame-timing.md`: the clocks table gains the input reading,
  with the table above as its reason.

Package code that moves off `performance.now()`:

| site | reads today | reads instead |
| --- | --- | --- |
| `packages/core/src/velocity.ts` (`push`, `velocity`) | handler time, as the default of `at` | the event's `timeStamp`, passed by the recognizer; the defaults go |
| `packages/core/src/pan.ts`, `swipe.ts`, `transform.ts`, `input-pointer.ts` (the tracker's callers) | nothing, they rely on the default | pass the event's `timeStamp` |
| `packages/core/src/double-tap.ts` | handler time | the event's `timeStamp` |
| `packages/2d/src/dispatch.ts`, `packages/3d/src/scene-pointer.ts` | an injected `now`, default `performance.now` | the event's `timeStamp`; the injection goes |
| `packages/core/src/input-processors.ts` (`tap`, `doubleTap` edges) | the wall, when the effect runs | the source's own time: see the next section |
| `packages/core/src/transform.ts` | nothing, it relies on the tracker's default | the `pointerFrame` terminator's `timeStamp`, which the event now carries |
| `packages/2d/src/animation.ts` | `setInterval` plus wall elapsed | the tick of a no-demand `onFrame` |
| `packages/2d/src/oversample.ts` (thrash sentinel) | wall time | `frameTime()` |

`VELOCITY_MIN_STEP_MS` stays: the transform pushes one sample per frame
and a pan one per move, but two pointers' moves of one frame share a
stamp, which is the same-instant pair it guards against.

## The two non-input sites (built 2026-09-30)

`packages/2d/src/animation.ts` and the thrash sentinel in
`packages/2d/src/oversample.ts` read `performance.now()` too. Neither is
input, and neither had a frame tick in hand: the clip stepped from a
`setInterval`, the sentinel is called from `setOversample`, wherever an
app or an effect calls that. Two additions to core carry them:

- `onFrame(fn, { demand: false })`: a frame callback that is no request
  for frames. What makes `onFrame` expensive is the `requestFrame()` it
  makes on every registration, a standing demand to present; the callback
  itself runs with every frame signal the runtime delivers, idle ticks
  included. Without the request it is called on the same ticks and the
  app presents only when the body writes. The clip registers one while it
  plays (under no owner: the clock is the clip's) and accumulates the
  tick's deltas, so it is on one clock, steps on the frame a boundary
  falls in (the timer landed up to half a clip frame late), freezes and
  resumes with the dev clock and plays at its own speed under `srt
  render`.
- `frameTime()`: the latest frame's tick, remembered by core, readable
  anywhere, not reactive, no demand. The sentinel counts its one-second
  window on it.

Rejected: the clip on a full `onFrame` (60 presents a second for an 8 fps
clip). Rejected, as proposed first: the clip on its timer plus
`frameTime()` (two clocks in one consumer, correct only because timers
fire after the clock's tick inside a frame, and the half-frame lateness
stays).

Measured on the desktop client, the Pixel 7 and the SM-T500:

- A no-demand callback on an idle screen is called 59, 54 and 56 times a
  second with 0 frames presented. The Pixel idles at 60 Hz, and the
  largest gap between two calls was 33 ms on the desktop and 50 ms on
  the devices: idle ticks skip a refresh now and then, so a step lands
  at most that late on a still screen.
- `packages/2d/examples/anim.tsx` logs ANIM-OK on all three, and its
  2 fps clip presents 7 or 8 frames in 4 s.
- With the dev clock paused the callback is not called (0 calls in 2 s);
  after the resume the largest tick step is 33 to 50 ms, so a clip
  continues where it stopped.

## Open details

- Below the refresh rate a rest before the lift reads as no rest
  (measured 2026-09-30, the app held near 15 fps, a drag that rests 80 ms
  and lifts: the last move to the up read 23 ms on the tablet and 6 ms on
  the desktop, under `VELOCITY_REST_MS`, and the lift flung). A move is
  stamped with the slot that delivered it, which is up to a frame
  interval after the finger was there, and the resampler's bridged step
  and its correction move the position one and two slots later still;
  the up carries its true arrival time, so the age of the last move comes
  out short by up to three frame intervals. At the refresh rate that is
  the two frames the rest window was sized for; at 15 fps it is 200 ms.
  It was wrong before as well, by accident in the other direction: the
  up's handler ran late behind the frame, which lengthened the rest
  (45 ms here, still a fling on the desktop). The cause is one gap: a
  move does not know when the finger was there. Closed by
  [Real sample times](#real-sample-times-decided-2026-09-30).
- Reading the timeline inside a move handler now gives the frame's own
  time, since the clock ticks before the moves: `flux::Timeline` (video
  sync) read from a handler is one frame fresher than it was. Transitions
  are stamped after the moves, as before.
- Whether a pointer session survives a suspension at all. With this
  reading the stamps are right either way.

## Real sample times (decided 2026-09-30)

Every input event is stamped with when it happened, not with its slot or
its arrival, and the resampler resamples by time. Chosen over a targeted
fix of the rest rule (a second time on the move, the rest judged
natively, or a documented limit) under the rule "no backwards
compatibility, the best solution only".

What SDL gives (SDL 3.4.10 source; upstream main is the same for
Android). Every SDL event has a `timestamp` in ns on SDL's tick base and
the sdl3 crate exposes it on every variant:

| platform | pointer and touch `timestamp` | sample time |
| --- | --- | --- |
| Wayland | the compositor's event time (ms; ns with `zwp_input_timestamps_v1`) | yes |
| macOS | `NSEvent.timestamp` | yes |
| iOS | `UIEvent.timestamp`; coalesced touches are dropped | yes |
| X11 | receipt in the pump (`X11_GetEventTimestamp` is a FIXME returning now) | as receipt |
| Windows | `msg.time`; touch ignores `TOUCHINPUT.dwTime` | no: moves in 15.6 ms tick steps (measured) |
| Android | 0, so the time of the JNI call; historical samples never read | no |

On a desktop the receipt time is a good sample time: input is not batched
to vsync and the pump thread blocks on the queue. Android is the one
platform that needs a path of our own, and the place exists:
`lattice/android/.../SDLSurface.java` is our patched copy and
`SolidRTActivity` has natives already. `MotionEvent` carries a time for
the current and every historical sample (ms, ns from API 34), on the
clock the Choreographer frame time is on, which is the frame signal's
reference under vsync-locked pacing.

The design:

1. Pointer, wheel and key events carry their time in alloy: SDL's
   `timestamp` on the desktop, the `MotionEvent` time on Android.
2. An Android touch path of our own: the finger branch of
   `SDLSurface.onTouch` hands every historical sample and the current
   one, each with its time, to a SolidRT native in place of
   `onNativeTouch`. SDL finger events stop on Android; mouse, pen, pinch
   and the touch device list stay with SDL.
3. The resampler resamples by time. A frame's position is the one at
   T = the frame's reference minus a latency constant: interpolated
   between the samples around T; extrapolated (touch only) when the
   newest sample is older than T, up to a bound; beyond the bound the
   newest real sample once, then silence. The slot assumption and the
   misses state machine go.
4. A move's `timeStamp` is the time of the position it carries: T when
   resampled, the sample's own time otherwise, never the frame's.
5. An up flushes: samples newer than the last dispatched move go out as
   one move ahead of the up, or a finger that moved until the lift would
   read as rested below the refresh rate.
6. The input reading maps event times: the frame signal carries its
   reference instant, the paced clock anchors the reading to it (not to
   the tick's execution time), and a stamp is the latched reading plus
   the event's distance from the reference, at the clock's rate. Pause,
   scale, step and the test clock are as before. `EventSender`'s send
   time stays for events with no time of their own (gamepad snapshots).

The rest rule in `packages/core/src/velocity.ts` does not change: the
up's stamp minus the last moved sample's stamp is the true rest at any
frame rate.

Built 2026-09-30, uncommitted:

- alloy: events travel in an envelope with their time (`Arrival`,
  `EventSender::send_at`); the pump takes SDL's event timestamp
  (`sdl_utils::event_instant`); frame signals carry `reference` and
  `grid` (`RefreshCounter::grid_ms`); `resample.rs` is the time-based
  resampler, with `RESAMPLE_LATENCY` (5 ms, Android's own),
  `RESAMPLE_MIN_AHEAD` and `PREDICT_MAX_PERIODS` as its tuning constants;
  `touch.rs` is the queue the Android native feeds.
- Android: `SolidRTSurface` (created by `SolidRTActivity`) overrides
  `onTouch` and hands every finger sample to `nativeTouch`; SDL's own
  Java is untouched by it.
- lattice: `PacedClock::input_at_ms` against the grid instant, the global
  never-go-back clamp gone (the reading of an instant no longer depends
  on the tick it is read after); the frame verb stamps each move with its
  own time and the terminator with the latest; the flush move ahead of an
  up dispatches on arrival with a terminator of its own.
- core: `predicted` on move events; the recognizers keep predicted moves
  out of the velocity tracker, which reads the rest from when the
  position was reached.
- Tests: the alloy resampler and grid tests, 64 lattice, 309 JS (a pan
  test among them: a predicted move moves the pan and stays out of its
  velocity).

Found while building, not in the design above:

- A predicted move is ahead of the finger, and when the finger had
  stopped the real position follows with an earlier time. That is the one
  case where a stamp goes back. Decided 2026-09-30: the move says so.
  `PointerEvent.predicted` (also on the global pointer event and the 2d
  and 3d layer and scene events) is true for it; pan, swipe, transform
  and the pointer feed follow a predicted position and keep it out of
  their velocity. The web keeps predictions apart in
  `getPredictedEvents()`; here the predicted position is the move, since
  bridging the frame is its purpose, and the flag is what separates it.
- A slow drag into a lift flung on the tablet (real finger, 2026-09-30):
  the finger pauses, then its contact shifts as it leaves the panel, two
  or three samples covering 0.9 to 2.7 px in the last 8 to 25 ms, the
  last one 9 ms before the up. The rest rule cannot see it and a fit
  through so short a span read 60 to 450 px/s. The tracker now does what
  Flutter's and Android's do: a gap over 40 ms between two samples cuts
  the history (`VELOCITY_STOP_GAP_MS`), and samples covering less than
  the pan slop are no motion (`VELOCITY_MIN_TRAVEL`, 8 px). Whether the
  tablet did this before real sample times is not known: the earlier
  finger tests were flings and a long rest.
- A cancelled touch (`ACTION_CANCEL`) is not passed on, as before: SDL's
  cancel event was never translated, so a pointer the system takes over
  stays down. Unchanged here, worth an item of its own.
- A synthetic touch drag (the control API) is resampled like a finger's,
  so a position read mid-drag can be an interpolated one; the up flushes
  the last position.

Measured 2026-09-30, release clients, `probes/timestamp-probe.tsx`. The
probe tracks each drag on the event stamps and on `performance.now()` in
the handler, the clock before this item.

Desktop (Wayland, 60 Hz), synthetic drags of 20 px per 16 ms through the
control API, so a true speed near 1166 px/s:

| drag | stamps | handler time |
| --- | --- | --- |
| mouse, lift 5 ms after the last move | rest 6.2 ms, 1156 px/s | rest 0.6 ms, 1347 px/s |
| mouse, 80 ms rest | rest 81.4 ms, zero | rest 70.5 ms, zero |
| touch, lift 5 ms after | rest 6.2 ms, 1167 px/s | rest 0.1 ms, 1189 px/s |
| touch, 80 ms rest | rest 81.3 ms, zero; resampled moves 16.67 ms apart | rest 64.0 ms, zero |
| mouse, app near 16 fps, lift 5 ms after | rest 6.1 ms, 1169 px/s | 666 px/s |
| mouse, near 16 fps, 80 ms rest | rest 80.1 ms, zero | rest 60.2 ms |
| touch, near 16 fps, lift 5 ms after | rest 6.1 ms, 1167 px/s | 423 px/s |
| touch, near 16 fps, 80 ms rest | rest 81.2 ms, zero | rest 60.1 ms |

The two rest rows near 16 fps are the case this section exists for: it
read 6 ms and flung before. Taps near 16 fps: 31.2 ms held, 101 ms apart,
a double tap.

Android, through the new touch path (`adb shell input swipe` of 600 px in
300 ms and `input tap`, which enter as real `MotionEvent`s; true speeds
762 and 1333 logical px/s):

| check | Pixel 7 (90 Hz) | SM-T500 (60 Hz) |
| --- | --- | --- |
| swipe, lift velocity on stamps | 762.6 px/s | 1310 px/s |
| the same on handler time | 848 px/s | 1390 px/s |
| swipe with the app near 13 fps, stamps | 762.6 px/s | 1334 px/s |
| the same on handler time | 817 px/s | 1480 px/s |
| two taps | down and up share a stamp (the injector's one time), a double tap | the same |

Windows (SDL's stamp against the receipt, `alloy/examples/
event_time_probe.rs`, cursor moved by script): the stamp trails the
receipt by 0 to 16.7 ms, 8 ms on average, and moves in steps of 15.6 ms
with a double step 6 percent of the time, which is a 60 Hz stream
quantized to the system tick. So Windows takes the pump's receipt
(`sdl_utils::event_instant`), as built. X11 needs no run: SDL's stamp
there is the receipt by its source.

Known limits and what is owed:

- Real finger, both devices, 2026-09-30 (the probe's list, swiped and
  flung): responsiveness judged good by eye, the constants left as they
  are. A slow drag into a lift no longer glides (see the tracker rules
  above). One tablet lift after a pause still read 1453 px/s: the panel
  reported 28 px in the last 34 ms, which position data cannot tell from
  a short flick; left as it is. Two Pixel lifts after a pause moved 9 to
  10 px and read 389 and 618 px/s, just over `VELOCITY_MIN_TRAVEL`; if
  lift-off glides come back, Flutter's 18 px is the next value to try,
  and contact pressure or size at the lift is the signal that could
  separate a roll-off from a flick (Android only, not built).
- Not done with a finger: a rest before the lift with the app held near
  15 fps (verified with synthetic input only).
- Below API 34 Android sample times are whole milliseconds.
- Far below the refresh rate the velocity window holds two or three
  samples (one move per frame plus the flush). All raw samples on the
  move (the web's `getCoalescedEvents()`) would fill it; additive, not
  in this item.

## Done looks like

- Every input event an app handler receives has `timeStamp`, documented
  on the types.
- No file under `packages/*/src` reads `performance.now()` except to
  measure work. Met: what is left under `packages/*/src` is comments.
- The velocity tests run on explicit stamps only (they already do), and
  the twelve parked tests (`packages/core/checks/gesture.test.ts`,
  `input-map-interactions.test.ts`) can run on stepped stamps with exact
  assertions instead of tolerances, which the test harness's app layer
  then does.
- `okf/design/frame-timing.md` lists the input reading among the clocks.

## Verification

The changed math is pure and tested headless (the paced clock's tests,
the velocity, gesture and interaction tests). The desktop client and two
Android devices were driven through the control API and by hand; the
numbers are under [Where this stands](#where-this-stands-2026-09-30) and
the section on the two non-input sites. Done there: the fling comparison
on touch devices, the taps and the rest-then-lift on clients held below
the refresh rate (the taps hold; the rest does not, see Open details),
and two taps across a background stretch on Android.
`probes/timestamp-probe.tsx` and `probes/passive-frame-probe.tsx` are
the probes.
