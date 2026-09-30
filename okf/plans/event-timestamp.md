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

Built and verified on the desktop client and two Android devices,
uncommitted. Open: rest detection below the refresh rate (see
[Open details](#open-details)).

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

Rejected: stamping real hardware sample times and interpolating, as Android
and Flutter do. SDL's Android path carries no usable sample times (touch is
stamped at JNI receipt and historical batch samples are dropped), which is
why the resampler models slots. Given a slot position, the slot's time is
the truthful stamp. Going beneath SDL for sample times would be a
different item and a device campaign.

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
  (45 ms here, still a fling on the desktop). What the rule needs is when
  the finger last really moved: the arrival time of the resampler's last
  fresh sample, which the pump has and a move does not carry. To decide:
  a second time on the move event, the rest judged natively, or left as a
  limit of apps far below the refresh rate.

- How far an arrival stamp overshoots the next signal's reading is not
  measured. It is bounded by the signal's execution jitter; the
  never-go-back rule makes it harmless, the size says how often it
  applies.
- Reading the timeline inside a move handler now gives the frame's own
  time, since the clock ticks before the moves: `flux::Timeline` (video
  sync) read from a handler is one frame fresher than it was. Transitions
  are stamped after the moves, as before.
- Whether a pointer session survives a suspension at all. With this
  reading the stamps are right either way.

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
