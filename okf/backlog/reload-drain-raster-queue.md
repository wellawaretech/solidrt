---
type: backlog-item
title: Reload does not drain the raster queue
description: A client whose raster thread is backed up stays backed up through load/reload, because the backlog lives in the raster command channel rather than in the app; the dev has no way out short of restarting the process, and no reason to suspect the runtime.
status: deferred
timestamp: 2026-07-27T00:00:00Z
---

# Reload does not drain the raster queue

Split out of idle-tick-gpu-backlog-runaway.md.

Observed repeatedly during that session: once the raster thread was far enough
behind, `load` and `reload` did not recover it. A freshly loaded app inherited
the previous one's frame period - at one point a newly loaded probe's first
frame arrived **2 m 16 s** after the load, and a restored app inherited a 50 s
period. The only reliable recovery was `am force-stop` plus relaunch.

The cause is structural rather than a bug in reload: replacing the app swaps out
the JS side, but the queued `RasterCmd`s are already in the raster channel and
nothing drops them. So the new app's first frames queue behind the old app's
work.

Why this is worse than it sounds for a dev: the symptom is "my app is slow", the
instinct is to edit and reload, and reloading is exactly what does not help.
During the original investigation this contaminated several measurements before
it was understood - configurations that measure fine from a fresh process
measured as catastrophic when they followed a collapsed one, which sent the
diagnosis down a wrong path for a while.

## Why deferred rather than open

The runaway that made clients wedge in the first place is fixed (see the parent
item: queue-depth-gated idle ticks plus per-shader params load-shedding), and it
was the only known way to build a backlog deep enough for this to bite. So this
is now defensive: worth doing so the failure mode cannot silently reappear
behind some future expensive-pass workload, but no longer blocking anyone.

## Shape of the fix

Drop or drain pending raster commands on engine teardown, so a reload starts
against an empty channel. The queue-depth counter added for the idle-tick gate
(`RasterSender` in alloy/src/raster.rs) already tracks exactly what would need
to be cleared, and would make the drain observable. Care needed around commands
with a `reply` channel - a dropped command must not leave a caller waiting
forever.

A cheaper interim: log a warning on reload when queue depth is non-trivial, so
the dev at least learns that restarting the client is the move.

Related: dev-state-across-reloads.md, gpu-pass-timing.md.
