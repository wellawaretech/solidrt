---
title: The main loop thread rarely sleeps on Android
description: In a 5 s scheduler trace of an animating app on the SM-T500, SDLThread (the alloy main loop) was switched out 259 times and only 15 of those were sleeps; the client's stats read 135% CPU against 7% for the same probe on Linux. The loop is meant to block on the SDL event queue between wakes.
created: 2026-09-12
completed: 2026-09-21
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

## Outcome

None of the three candidates. The loop's wait is correct; SDL's Android
`SDL_WaitEventTimeout` is what never blocks: every pump pushes the poll
sentinel, every pushed event sends the Android lifecycle WAKE, and the
wait blocks on that same semaphore, so it spins through its whole timeout.
Measured on the SM-T500 with the app idle: SDLThread 99% CPU, 0 voluntary
context switches per 5 s. The fix is the `SDL_POLL_SENTINEL=0` hint on
Android before SDL init (`alloy/src/app.rs`); after it, idle SDLThread 2%
CPU and 358 voluntary switches per 5 s, and the animating probe 5% CPU,
785 switches, 60 fps, 0 missed presents. Report and upstream status:
[[sdl-android-wait-event-poll-sentinel-spin]].

The probe's process cpuPct still reads 71, not tens: the raster thread's
GL driver work is about 56% of it (roughly 9 ms CPU per frame on a 0.1 ms
build). That is a separate question, not the loop.
