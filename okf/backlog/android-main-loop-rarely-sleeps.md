---
title: The main loop thread rarely sleeps on Android
description: In a 5 s scheduler trace of an animating app on the SM-T500, SDLThread (the alloy main loop) was switched out 259 times and only 15 of those were sleeps; the client's stats read 135% CPU against 7% for the same probe on Linux. The loop is meant to block on the SDL event queue between wakes.
created: 2026-09-12
---

# The main loop thread rarely sleeps on Android

Observed 2026-09-12 while tracing the release chain
([[android-vsync-release-chain]]); not investigated. The loop
(`alloy/src/app.rs`) blocks in `wait_event_timeout_ms` toward the next
deadline, and SDL's Android path does wait on a timed semaphore
(`Android_WaitLifecycleEvent`), so a plain spin in SDL is not the obvious
reading. Candidates: a zero `remaining` every iteration (a deadline that is
always already past), the wake events themselves arriving at a rate that
never lets the wait block, or the timed semaphore not blocking on this
device.

## Done looks like

The same probe shows SDLThread sleeping between wakes in the trace and
`cpuPct` in the tens, not above 100.

## What it involves

A trace with `sched` plus a log of `remaining` per iteration on the device;
then whichever of the candidates it names.
