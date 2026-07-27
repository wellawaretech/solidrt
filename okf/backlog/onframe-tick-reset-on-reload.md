---
type: backlog-item
title: onFrame tick reset on reload
description: The tick timebase resets across hot reload after the new instance's first frame, handing apps one enormous negative delta; apps clamp dt as a workaround.
status: deferred
timestamp: 2026-07-27T00:00:00Z
---

# onFrame tick reset on reload

Observed (doom, 2026-07-15): after `reload`, the new app instance's FIRST
onFrame call still receives a tick on the old timebase (ms since client
launch, ~6.8e6 after two hours), and from the SECOND call on the counter has
reset to ~0. Any app that computes dt = (tick - lastTick) sees one enormous
negative delta right after every hot reload.

In doom this teleported the player ~41k units into the void (dy = speed * dt
with a movement key held during reload - dt was -6839s, and the app's
Math.min(dt, cap) only capped the positive side) and drove a 35Hz tic
accumulator to -239k tics, silently freezing all tic-clocked logic (light
effects, damage floors) for what would have been two hours of game time. Cost
a long debugging arc because the visible symptom ("light thinkers never run")
was far from the cause.

Fix candidates, in preference order:
- Make the tick timebase continuous across reloads (never reset the counter
  for a connected client), or
- Reset it BEFORE the new instance's first onFrame call so all deltas within
  one instance share a timebase.

App-side workaround (now in the scaffold AGENTS.md): clamp deltas to
[0, cap] with Math.max(0, Math.min(dt, cap)).

Status 2026-07-27: the described reset is no longer traceable in current
code - both clocks (PacedClock, flux Clock) are constructed outside the
engine reload loop in lattice/src/lib.rs and PacedClock is Arc-shared with
no reset path; lib.rs even documents "persists across reloads for
continuous time". Before picking this up, re-verify empirically that the
reset still reproduces; it may have been fixed as a side effect of the
frame-pacing work.

Superseded note: an earlier version of this file claimed the frame demand
gate freezes a reloaded app whose onFrame early-returns. That is no longer
true - a pending onFrame callback is a standing request for the next frame
(packages/core/src/window.ts), and the scaffold AGENTS.md now documents
that no startup "prime" write is needed.
