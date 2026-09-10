---
title: A delayed transition starts late when the frame that activates it lands late
description: A held write (a `delay`, a stagger slot) applies at the first advance that finds it due and its track starts at that frame's clock, so a frame hitch shifts the whole motion instead of being absorbed; done means the track behaves as if it started at its scheduled time, as every timeline-based peer does.
created: 2026-09-10
---

# A delayed transition starts late when the frame that activates it lands late

## Symptom

A stagger cascade spaces items by `index * stagger`. If a frame lands late
over one item's slot, that item's track starts at the late frame's clock
and runs its full duration from there, so it drifts behind its neighbours
by the size of the hitch and stays behind. Two items whose slots fall in
the same late frame start together. The cascade's spacing is then a
function of frame timing, which is the thing a declarative delay exists to
hide. A plain `delay` on a write has the same behaviour, one item at a
time.

Every timeline-based peer catches up instead: Framer Motion, CSS animations
and transitions, Svelte and GSAP treat a delay as an offset from a start
timestamp, so a late frame finds the animation already part way in.

## Cause

`take_due` (alloy/src/rendertree/tree/transitions.rs) hands each due
`PendingWrite` to `retarget`, which builds the track with `start_ms = now`
for a tween and a fresh spring state for a spring. The scheduled `at_ms`
is consulted only to decide that the write is due; the overshoot
`now - at_ms` is dropped. The doc comment on `Transitions::schedule`
(alloy/src/rendertree/transitions.rs) records the behaviour, and the exit
subtree tests step through activation frames to avoid it.

## Done looks like

A track started from a held write is indistinguishable from one started at
`at_ms` on a frame that landed exactly then: a tween's `start_ms` is
`at_ms`, and a spring has already advanced by the overshoot at the end of
the activating frame. Stagger spacing is then exact under any frame
timing, and the `schedule` doc comment loses its caveat.

## What it involves

`retarget` takes a start time (default `now`), and `take_due` passes each
write's `at_ms`. For a tween that is the whole change. For a spring the
activating advance integrates `now - at_ms` on top of the frame's `dt`, so
the first-frame step for that track is the overshoot plus `dt` (capped the
way the spring integrator already caps a large `dt`, so a long stall still
lands the spring instead of blowing it up). A test that skips from before a
slot to well past it must then see the track mid-flight, which inverts the
stepping trap the exit tests currently work around.

Observable from the MCP, as part of done: with the clock frozen, a
`step=<n>` that jumps past a stagger slot in one go must read the slotted
item mid-flight in the tree dump with `props`, at the value it would hold
had every frame landed; today it reads at its start value. That is the
whole check, and it doubles as the regression test for the probe trap.

## Findings

The cause was two-sided, not one: a late tween started at the frame's
clock (the drift described above), while a late spring was created fresh
and then integrated the whole frame's `dt` measured from the previous
advance, so it ran ahead by the part of the frame before its slot. Both
came from one global "time of the previous advance" serving every track.
The fix gives each `Track` its own clock (`since_ms` in
alloy/src/rendertree/transitions.rs): the scheduled time when a held write
starts, the previous advance otherwise; `retarget` takes the start time
and a spring integrates from its own clock to the frame's. The global
clock went away with it, including its "reset when the list becomes
non-empty" rule, since a fresh track starts at its own time and an idle
gap cannot enter it. No `dt` cap was needed: the oscillator step is
closed-form, so a long stall lands the spring exactly.

The advance pass's early-out ("nothing moved since the last advance")
now derives from the tracks and fires only when tracks exist and all
started at this clock; with no tracks it must fall through to report
idle, which the held-write-of-a-dead-node test pinned.

Read from the stagger example over the control API with the clock frozen,
one `step=12` after the tap: the rows' opacities inverted through the
ease-in gave elapsed times 176.1, 106.3 and 35.5 ms, gaps of 69.8 and
70.8 ms for a 70 ms stagger, where the old start-at-the-frame rule would
have read 83.3 and 66.7 ms.

## Related

- transition-per-direction-curves.md, transition-layout-animations.md.
