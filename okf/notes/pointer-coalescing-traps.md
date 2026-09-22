---
title: What must not be collapsed when pointer input is coalesced
description: Positions collapse to the latest sample safely; deltas do not, ordering around down/up does not, and hit testing must consume the batch's final position rather than each sample.
created: 2026-08-13
---

# What must not be collapsed when pointer input is coalesced

Batching pointer moves to frame cadence is safe for positions - the latest
sample is the truth - and unsafe for everything else. Four rules, each of which
has a wrong version that looks correct until it does not.

**Deltas SUM, positions collapse.** Wheel deltas and any future
`movementX`/`movementY` must be added together under coalescing, never
overwritten. Collapsing a delta to the last sample silently throws away
distance: a fast scroll becomes a slow one. This is the same trap recorded for
relative mouse input, and it applies to any value that is a difference rather
than a state.

**Do not reorder around a down or an up.** Downs and ups dispatch on arrival,
deliberately. A frame's batch of moves must not float across a down/up that
arrived mid-frame, or a drag starts from the wrong place.

**Hit test per pointer per frame, on the final position.** Batching means one
hit test per pointer per frame, not one per sample, and enter/leave must be
computed from the batch's final position. Running it per sample reintroduces
exactly the cost the batching removed.

**Do not resample deltas into existence.** The resampler extrapolates one
bridged step per gap. Nothing today needs to tell an extrapolated sample from a
real one, but a recognizer that starts caring will need them marked - inventing
motion is worse than missing it.

**Velocity at a lift is a fit over the window, never the last delta.** The
last sample before an up is often stationary, and under batching the last
delta is a whole frame old, so "last two samples" reads anything from zero
to a jolt. core's velocity tracker fits a line through the last 100 ms of
positions (Flutter's and Android's horizon) and reads it at the up, which is
not a sample. And "the finger rested" is measured from the last sample that
MOVED: the resampler delivers one extrapolated step and its correction up to
two frames after the raw stream stopped (a 10-step synthetic drag arrives as
eleven moves, the last one backwards), so the rest clock effectively starts
two frames late, and a 90 ms pause before the lift sits at the edge of the
50 ms rest rule (it read a 234 px/s fling once, zero the next time,
2026-09-22). A real finger's rest is longer; a synthetic probe should give
the up 120 ms or more when it means "rested". The same bridging makes a
SPARSE synthetic stream spiky: moves 100 ms apart arrive as a step forward
and a step back per frame, and a fit over such a tail can read 900 px/s for
a 150 px/s drag (a "slow" swipe that dismissed a row). A slow finger on
hardware streams every frame; a probe that means "slow" sends small steps
at 16 ms, never large steps far apart.

Source: [frame-batched-pointer-input](../done/frame-batched-pointer-input.md).
The unsettled question of where position *filtering* belongs is
[pointer-position-filtering](../backlog/pointer-position-filtering.md).
