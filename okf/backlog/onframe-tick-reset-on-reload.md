---
type: backlog-item
title: onFrame tick timebase resets across hot reload (one negative delta)
status: deferred
timestamp: 2026-07-15T00:00:00Z
---

# onFrame tick timebase resets across hot reload

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

Related: the frame demand gate means a freshly reloaded app whose onFrame
early-returns without side effects never gets a second JS frame (its clocks
stay frozen until the first input). Also noted in the scaffold AGENTS.md;
doom primes the loop with one redundant uploadTexture on its dt === 0 frame.
