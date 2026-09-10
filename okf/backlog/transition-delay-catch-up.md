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

## Related

- transition-per-direction-curves.md, transition-layout-animations.md.
