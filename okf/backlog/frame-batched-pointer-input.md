---
type: backlog-item
title: Frame-batched multi-pointer delivery (and frame-paced mouse)
description: Touch is already resampled per pointer per frame, but each pointer still dispatches as its own JS event, so multi-touch consumers measure one fresh + one stale position and every span/centroid reading oscillates; mouse is gated against queue growth but still dispatches at drain rate with a hit test per event. The last step of the platform recipe - all pointers of a frame delivered same-age in one batch - is missing.
status: done
timestamp: 2026-08-09T00:00:00Z
---

## Outcome (2026-08-09)

Implemented. Decisions against the open questions and shape below:

- **Event shape: neither array nor tag - a terminator.** Per-pointer events
  are unchanged; the runtime emits a `pointerFrame` bus event after all of a
  frame's moves have dispatched (before rAF/render, same synchronous job).
  The array shape cannot work here: each pointer bubbles its own frozen hit
  path, so a multi-pointer event has no single path to bubble. Recognizers
  update positions per event and measure once at the terminator.
- **Mouse/pen route through the resampler frame slots**; `pending_moves` is
  deleted. Extrapolation is touch-only (mouse would fake an overshoot on
  stop). `resampler.down` now re-seeds for all pointer types, so a buffered
  pre-down move collapses into the down instead of dispatching stale after
  it.
- **transform.ts kept the EMA (retuned x2 for per-frame cadence) and the
  creep hysteresis**; scissor compensation is obsolete by construction.
  Anchors (activation, join, lift) defer to the terminator too - a mid-batch
  anchor would bake one mixed-age jolt into the first delta. Raw-span A/B on
  device stays open as the EMA's retirement test.
- Candidate follow-up, deliberately NOT in: a minimum-span floor for the
  zoom gate (Android config_minScalingSpan precedent) against tiny-span
  ratio blowups and sensor contact merges. Was added once and rolled back
  to keep the baseline testable; re-add only against a reproduced fault,
  and if re-adding: crossing back above the floor must re-base the filter,
  or its catch-up replays the collapsed span as zoom.
- **Present-per-frame stays app discipline**; input now arrives at frame
  cadence, which removes the render-per-event failure mode by default.
- **No timestamps added.** If px/s thresholds are ever needed,
  `performance.now()` read at the terminator is batch-granular already.

## Follow-up: alloy owns the resampler (2026-08-10)

The resampler moved to `alloy/src/resample.rs` and alloy took over the
feeding. The rule is producer-side: whoever emits pointer events feeds the
histories at emission - the alloy pump for real input (`app.rs`; moves are
consumed there and NEVER travel as `AlloyEvent`s, downs seed and ups drop
the history before their events are sent), the dev connection for `input`
queries (`DevFlags::resampler`, same triple at the send site). The
consumer side shrank to two policy calls on `SharedResampler`: `sample()`
in lattice's frame verb, `clear()` on engine swap (the gate check moved
from `event()` into `frame()`, where the sampling is). Consequences:

- The batch-loop move coalescing in `lattice/src/lib.rs` is DELETED, touch
  exemption included - moves never cross the channel, so a stalled drain
  cannot replay stale positions by construction. Only the frame-signal
  collapse remains.
- InputState pointer positions are recorded from the frame verb's samples
  (frame granularity, exactly what the hover refresh reads); downs/ups
  keep updating it on arrival.
- A push design (alloy emitting resampled moves as events) was considered
  and rejected: piled-up move batches behind collapsed frame signals would
  reintroduce stale-position replay, forcing the coalescing walk back, and
  the engine-swap clear would need a cross-thread command. The pull design
  has neither problem.

# Frame-batched multi-pointer delivery (and frame-paced mouse)

Every mature input stack converges on the same layering: filter at the
driver, batch and resample all pointers to the frame clock, deliver them
together, paint after the batch. Android's view pipeline resamples every
pointer of a MotionEvent to a common frame timestamp; browsers coalesce
pointermove to the render loop and old-style TouchEvents carried the whole
touch list in one event; Flutter ships an explicit pointer-event resampler.
The consequence is that naive gesture code (three.js OrbitControls computes
raw per-event pinch ratios with zero filtering) works: by the time it runs,
all pointers are the same age and nothing paints mid-batch.

lattice has the per-pointer half of this but not the cross-pointer half,
and the gap is measurable. Filed 2026-08-09 after the unimog demo's orbit
camera shipped pinch/pan (see `packages/core/src/transform.ts`, whose
header documents the observed noise).

## What already exists

- **Touch is frame-slot resampled per pointer.** `alloy/src/resample.rs`:
  moves feed history on arrival, `frame()` drains one resampled move per
  pointer per frame signal (`lattice/src/runtime.rs:342`), with one-step
  velocity extrapolation to bridge Android's paired vsync deliveries. Per
  pointer, cadence is already clean.
- **Mouse/pen moves are gated, not queued.** `runtime.rs` `pending_moves`
  (~line 105): at most one dispatch closure in flight per pointer,
  arrivals overwrite the pending position - the documented motivation is
  exactly the 1000Hz gaming mouse whose backlog would otherwise starve
  frame signals and replay stale positions. Hover is why they dispatch on
  arrival rather than from frame(): "hover must dispatch without any frame
  in flight" - though the resampler comment notes idle Ticks now keep
  frame signals coming at refresh cadence even when nothing paints, which
  weakens that constraint.
- **Downs/ups/wheel dispatch on arrival**, ordering-sensitive and
  delta-carrying; nothing here should change that.

## The gap, measured

Each pointer's move still crosses to JS as its own event
(`flux::gui::input::dispatch` per pointer; `packages/core/src/window.ts`
dispatches each along the hit path). With two fingers down, every event a
recognizer sees is one fresh position paired with the other finger's
one-frame-stale one. Captured on a tablet (unimog demo, ~130Hz combined):

- A mathematically smooth two-finger slide with constant separation
  measures a span that alternates 0.98x / 1.02x on consecutive events -
  pure scissor, no physical pinch.
- Replaying captured real gestures as raw per-event span ratios produced
  120-970% of accumulated back-and-forth zoom churn per gesture (against
  net changes of a few percent) - rendered as continuous jitter, since the
  demo also painted per event.

The recognizer now compensates (`packages/core/src/transform.ts`: span
EMA ~50ms, slop gate, rate hysteresis against fingertip-roll creep -
thresholds tuned from the same captures: creep ~9px/s vs deliberate
pinches 100-160px/s). That is the Flutter-era answer - correct but
per-recognizer; every future multi-pointer consumer (pinchable images, map
views) re-inherits the problem unless it uses createTransform.

Mouse-side, the gate bounds the queue but not the dispatch rate: moves
drain as fast as the JS thread runs them, each with a hit test and handler
walk, and apps that render per move event (the unimog demo's push-on-move)
paint at input rate rather than frame rate.

## Shape

Extend the existing machinery one step; no new subsystem:

1. **Batch the frame's touch samples into one delivery.** frame() already
   collects `resampler.sample()` into a Vec; dispatch that Vec as ONE
   InputEvent (all pointers, same frame slot) instead of N. JS-side,
   window.ts dispatches the batch's moves in one task before anything
   paints; recognizers get a natural "measure once per batch" point, and
   `createTransform` can drop its scissor compensation (keep the creep
   hysteresis - fingertip roll is real reported motion at ~9px/s that no
   amount of resampling removes, and keep the EMA as belt-and-braces or
   retire it after on-device A/B).
2. **Route mouse/pen through the same frame slots.** With idle Ticks
   already keeping frame signals at refresh cadence, the hover argument
   for arrival dispatch is mostly historical; frame-pacing mouse bounds
   hit tests and handler walks to refresh rate on any polling-rate device.
   If a latency-sensitive consumer emerges later (competitive-shooter
   aiming), that is what the relative-motion path is for (see
   [relative-mouse-input](relative-mouse-input.md)).
3. **Event shape.** Either one event object carrying a moves array
   (MotionEvent-style, honest about simultaneity) or per-pointer events
   tagged with a shared frame/batch id (less API churn; recognizers key on
   the id). The array is the platform-proven shape; the tag is the
   smaller diff. Decide against real consumers.

## Traps

- **Wheel and movement deltas must SUM under any coalescing, never
  overwrite** - the same trap already documented in
  [relative-mouse-input](relative-mouse-input.md) for future
  movementX/movementY; positions collapse correctly, deltas do not.
- **Ordering with downs/ups.** Down/up dispatch on arrival for good
  reasons; a frame batch of moves must not reorder around a down/up that
  arrived mid-frame (the pending_moves drop-on-up at `runtime.rs:299`
  shows the existing care here).
- **Per-event hover/hit semantics.** Batching moves means one hit test per
  pointer per frame, not per sample; enter/leave computation must consume
  the batch's final positions, not each sample.
- **Don't resample deltas into existence.** The resampler extrapolates one
  bridged step per gap; a batched event must mark extrapolated samples if
  recognizers ever need to distinguish them (today nothing does).

## Open questions

- Does `scale`-consumer simplification actually hold on device - i.e.
  with same-age batches, is the raw span clean enough to drop the EMA, or
  does sensor dither (+-1-3px/event on the captured tablet) still warrant
  it? Needs an on-device A/B with the transform.ts filter behind a flag.
- Should present-per-frame be enforced at the same time (apps currently
  choose when to push camera updates), or left as app discipline now that
  input arrives at frame cadence anyway?
- Where does position filtering ultimately belong if a noisier device
  shows dither that survives batching - runtime (One Euro on the samples,
  every consumer benefits) or recognizer (current answer, scoped to
  gestures)?
