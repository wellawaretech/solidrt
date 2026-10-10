---
title: Transition time lives on the track, not on a clock
description: Why both transition modules (the render tree's elements and the spatial arena's nodes) keep time per track - a tween's elapsed, a hold's remaining, a spring's step - with the frame stamp only saying what app time it is, and what that buys (a per-node rate with no rebasing, a one-assignment startup anchor, exact catch-up of held writes).
created: 2026-10-10
---

# Transition time lives on the track, not on a clock

Shared by `alloy/src/rendertree/transitions.rs` (elements) and
`alloy/src/spatial/transitions.rs` (nodes) through `alloy/src/motion.rs`
(`TrackStart`, `scaled_step`). Established by
[native-motion-time-scale](../done/native-motion-time-scale.md).

- A tween carries how far it has run (`elapsed_ms`), a spring integrates a
  step, a held write counts its hold down (`remaining_ms`). Every track
  and hold remembers the stamp it was last advanced to (`since_ms`) and
  takes `(now - since) * rate` as its step at an advance. Nothing holds an
  absolute start or activation time, so nothing is ever rebased: a node
  changing rate or parent just takes a different step next frame.
- With absolute track times, "the write and the advance agree on time"
  held only because both read the same stamp; the per-track `since_ms`
  makes the agreement explicit and survives a rate that differs per
  node. A fresh write's track is created with `since = now`, so the
  advance of the same frame steps it by zero, exactly as the absolute
  model did (p = 0 at the frame's paint).
- The first stamp is time zero for everything that exists: it sets
  `since` on every track and hold (the startup anchor of
  [transition-clock-startup-anchor](../done/transition-clock-startup-anchor.md)
  in one assignment, where the absolute model shifted every time by a
  delta).
- A held write that comes due between frames hands its overshoot to the
  new track as a lead (`TrackStart::lead_ms`): the track starts that far
  in, so a late frame finds the motion where it would have been. The
  element advance's "every track started at this very clock" early return
  must not skip such a track: it is advanced to the stamp yet has a value
  to write.
- The staggered exits keep the stamp they were let go of at
  (`(id, at_ms)`): the exit track starts with `since = at_ms`, so the
  "runs as of the clock it was let go of at" contract of
  [spatial-node-exit-transitions](../done/spatial-node-exit-transitions.md)
  is one field, not a special case.
- `motion_of` / `describeNode` report `heldFor` (ms of the hold left on
  the node's time) in place of the absolute `heldUntil`: the absolute
  value had no meaning under a rate.
- The spatial arena's rate is per node (`time_scale` declared, `rate`
  effective, nearest declaring ancestor wins, propagated eagerly on
  declare, reparent and orphaning free); the element tree runs every
  track at 1 and has no rate of its own.
