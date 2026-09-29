---
title: Input events carry a timeStamp on the animation timeline
description: PointerEvent carries no timestamp, so the velocity tracker, double-tap and the 2d and 3d dispatchers stamp events with performance.now() at handler time - a wall read that inherits the handler's execution jitter and cannot be stepped by a test or by srt render. Give every input event a timeStamp on the animation timeline, move package logic off performance.now(), and keep performance.now() for measuring work only. Prerequisite of the test harness's app layer.
created: 2026-09-29
---

# Input events carry a timeStamp on the animation timeline

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

## Decision (2026-09-29)

- Every input event carries `timeStamp`, the web name, in milliseconds on
  the animation timeline: the timeline of the `onFrame` tick, so an event
  time and a frame time compare directly and the dev clock's pause, scale
  and step apply to both.
- Under `srt render` and under a test the stamp comes from the stepped
  clock, with no test-mode special case in the recognizers.
- `performance.now()` stays real elapsed time, for measuring work. Package
  logic does not read it.

Rejected: stamping real hardware sample times and interpolating, as Android
and Flutter do. SDL's Android path carries no usable sample times (touch is
stamped at JNI receipt and historical batch samples are dropped), which is
why the resampler models slots. Given a slot position, the slot's frame
time is the truthful stamp. Going beneath SDL for sample times would be a
different item and a device campaign.

Rejected: virtualizing `performance.now()` under test. See decision D6 in
[test-harness](../plans/test-harness.md).

## What it involves

The stamp is read where the runtime hands an event to JS
(`flux/src/alloy_plugins/input.rs` builds the event object; the frame
timeline reaches flux as the `flux::Timeline` userdata). The rendertree is
not involved. `PointerEvent`, `WheelEvent`, `KeyEvent` and the global
pointer event in `packages/core/src/types.d.ts` gain the field.

Package code that moves off `performance.now()`:

| site | reads today | reads instead |
| --- | --- | --- |
| `packages/core/src/velocity.ts` (`push`, `velocity` defaults) | handler time | the event's `timeStamp` |
| `packages/core/src/double-tap.ts` | handler time | the event's `timeStamp` |
| `packages/core/src/transform.ts` | handler time | the event's `timeStamp` |
| `packages/2d/src/dispatch.ts`, `packages/3d/src/scene-pointer.ts` | an injected `now`, default `performance.now` | the event's `timeStamp`; the injection goes |
| `packages/core/src/input-processors.ts` (gamepad edge) | handler time | the frame tick, since the state is polled per frame |
| `packages/2d/src/animation.ts` | `setInterval` plus wall elapsed | the frame tick through `onFrame` |
| `packages/2d/src/oversample.ts` (thrash sentinel) | wall time | the frame tick |

`packages/2d/src/animation.ts` is expected to jump forward after a dev
clock pause instead of resuming, since its timer freezes and its elapsed
time does not. Expected from reading the code, not reproduced.

## Open details

- Down, up, key and wheel events dispatch on arrival, between frames, so
  their stamp is the timeline's reading at that moment: the last frame's
  time, or a live reading the way schedule-time timer deadlines take one
  (`timer_live_ms`). Frame quantization is harmless for double-tap and
  long-press windows of hundreds of milliseconds; how it meets the rest
  detection at lift (`VELOCITY_REST_MS`) has not been traced.
- The animation timeline skips a suspension and the timer timeline lives
  through it. Two taps on either side of a background stretch then read
  as close together; whether a pointer session survives a suspension at
  all decides if that matters.

## Done looks like

- Every input event an app handler receives has `timeStamp`, documented
  on the types.
- No file under `packages/*/src` reads `performance.now()` except to
  measure work.
- The velocity tests run on explicit stamps only (they already do), and
  the gesture tests run on the stepped clock with exact assertions instead
  of tolerances.
- `okf/design/frame-timing.md` lists input events among what runs on the
  animation timeline.

## Verification

Nothing in the frame chain moves (frame signals, the refresh count, the
cadence hold and the resampler stay as they are), so this does not reopen
the device timing work. The changed math is pure and testable headless.
The one device check is a fling on one touch device, comparing lift
velocities before and after, since the tracker's inputs become evenly
spaced instead of jittered.
