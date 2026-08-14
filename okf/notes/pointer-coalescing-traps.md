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

Source: [frame-batched-pointer-input](../done/frame-batched-pointer-input.md).
The unsettled question of where position *filtering* belongs is
[pointer-position-filtering](../backlog/pointer-position-filtering.md).
