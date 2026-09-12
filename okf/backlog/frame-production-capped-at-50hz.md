---
title: Frame production is capped near 50 a second whatever the display rate
description: An animating app on a 60 Hz Android tablet presents in an exact 1,1,1,1,2 pattern over refresh periods, five presents per six vsyncs, which is exactly 50 fps. The build costs 0.11 ms, so nothing is over budget; the producer's period is simply 20 ms and the display quantises it.
created: 2026-09-12
---

# Frame production is capped near 50 a second whatever the display rate

An app with a transition running and nothing else on screen presents at
50.75 frames a second on a 60 Hz panel (Samsung SM-T500, Android 12). That was
known loosely as "about five skipped frames a second" and written off in the
`PacingBudget` comment in `alloy/src/vsync.rs` as the platform pairing input
deliveries across vsync boundaries. It is not that, and it is not noise.

The SurfaceFlinger census of the app's own layer shows a perfectly regular
pattern of present intervals in refresh periods:

    1 1 1 1 2  1 1 1 1 2  1 1 1 1 2  1 1 1 1 2  ...

104 single and 21 double intervals over 126 presents. Five presents per six
vsyncs is exactly 50 a second out of 60. The runtime's own frame period
(`frameMs`) reads 20.98 to 21.22 ms across runs, against a `periodMs` of
16.67 that it reports correctly. So the producer runs on a 20 ms period and
the 60 Hz display beats it into the 1,1,1,1,2 quantisation. There is no input
involved at all: the transition is driven by the animation lane.

That also explains why this went unnoticed. The Philips TV's panel is 50 Hz,
so the same 20 ms producer lands on every one of its vsyncs and looks
flawless. The bug is invisible on exactly the device that gets the most
testing.

## What is already ruled out

- **Frame cost.** The build is 0.11 ms at p50 and 0.36 ms at worst, with zero
  slow frames and zero fence timeouts, against a 16.67 ms period.
- **A wrong refresh rate.** SDL reports 60 on the tablet and 50 on the TV, the
  stats window reports `periodMs` 16.67, and `FALLBACK_REFRESH_HZ` is 60. The
  loop derives its tick period from the observed rate every iteration.
- **A hardcoded 20 ms or 50 Hz anywhere in the frame path.** There is none.
  The 20 ms is emergent, which is what makes it worth chasing.
- **The re-arm margin.** `PacingBudget::MARGIN_MS` raised from 2.0 to 6.0,
  which moves the armed signal delay from about 14.4 ms after the vsync to
  about 10.3 ms and so triples the time available to re-arm the next
  Choreographer callback, changed nothing: 204 frames per four seconds either
  way, same 36 missed presents, same pattern.

- **The present fence depth.** `PRESENT_FENCE_DEPTH` raised from 2 to 3
  changed the pattern from the clean 1,1,1,1,2 to an irregular mix of the
  same two intervals, and left the rate at 51 a second. That the pattern
  moves while the rate does not is itself evidence: the limit is upstream of
  the present path, not in it.

## The leading suspect

`alloy/src/vsync.rs`, the Android backend's `run` loop, sleeps the pacing
delay ON the vsync thread between the Choreographer callback firing and the
signal being sent:

    post_frame_callback(); while !fired { poll_once() }   // wait for vsync
    if fired && !delay.is_zero() { sleep(delay) }         // 14.3 ms
    signal_tx.send(generation); wake()                    // then signal

The next callback is only posted when the main loop, woken by that signal,
emits its frame and arms a fresh request. So the post happens at roughly
`vsync + delay`, leaving `period - delay` to register for the next vsync -
about 2.4 ms at 60 Hz with the delay the budget picks. Android wakes an app
1.94 periods ahead of the vsync it is serving (see
[[choreographer-vsync-phase-offset]] for where that number comes from), so a
callback posted 2.4 ms before a vsync plausibly cannot be served for it and
lands on the one after, costing a period.

The fix direction that follows is to decouple the callback cadence from the
delay: post the next frame callback as soon as the current one fires, before
the sleep, so a callback is always registered a full period ahead, and keep
the delay only as the signal's own timing. Note against this theory that
raising the margin to give six times the registration window changed
nothing, so either the required lead is far larger than the margin can
provide or something else is also in play. Read the loop before trusting
either reading.

## Done looks like

An animating app with a sub-millisecond frame presents on every vsync of a
60 Hz panel, and the census of its own layer is all single intervals.

## Bisected: it regressed between v0.0.55 and v0.0.56

Measured with a matched probe - the same marker sliding on the position lane
in a `<window fullscreen>`, built each time with that release's OWN toolchain
so the bundle and the client agree, and served from its own project on its
own port. The census is of the app's own layer.

| release | frames per 4 s | census, single / double intervals |
|---|---|---|
| 0.0.55 | 223, 218, 220 | 114/12, 116/8, 111/13 |
| 0.0.56 | 203, 204, 202 | 103/22 |
| 0.0.58 | 204, 204, 204 | 102/23 |
| 0.0.59 | 203, 203, 202 | 101/24 |

0.0.55 delivers about 55 a second where the three later releases deliver 51,
the doubled intervals roughly halve, and 0.0.55 produces long unbroken runs
of single intervals (27 in a row in one sample) that none of the later
releases produce. The spread within the later three was 202 to 204, so the
gap is far outside the noise.

The only pacing commit in that window is **eb00118b, "FrameRelease state
machine, boundary hygiene"**, which first shipped in v0.0.56. That is the
same file and the same mechanism the suspect below names, arrived at
independently.

Two things to keep in mind about that result. 0.0.55 is at 55 a second, not
60, so this commit accounts for roughly half the shortfall and something
older accounts for the rest - the remaining candidates are the two pacing
commits that shipped in v0.0.49 and v0.0.38. And 0.0.55 predates the
`missedPresents` counter, so its stats report that field as absent; the
per-layer census is the instrument that compares across all four.

Two traps met while bisecting, both of which produce wrong numbers silently.
A client one release behind the CLI runs the bundle at 1 frame a second with
zero frames in the stats window, which reads like a catastrophic cap rather
than a page that never started; build each bundle with the matching
toolchain. And window mode changes the answer: the first 0.0.58 reading was
213 frames against 0.0.59's 203, which was entirely a plain `<window>`
composing the status and navigation bars alongside the app. Matched to
fullscreen it fell to 204 and landed on top of 0.0.59.

## Where to look next

The chain is `VsyncSource` (one AChoreographer callback armed per request,
answered after a delay), `FrameRelease` (which present's signal defers to
which vsync signal, and when the one outstanding request is armed), and the
main loop's `wait_event_timeout_ms` plus the wake the vsync thread pushes
into the SDL event queue. Since the margin is not the lever, the question is
whether one vsync signal in five is never delivered, or delivered too late to
be acted on, or acted on but its frame not produced. The instrument that
settled the pattern is the per-layer census, and the missing measurement is a
trace of the signal chain against the vsyncs themselves: an `atrace` of
`gfx view sched` gives the Choreographer callbacks, the app's queueBuffer and
SurfaceFlinger's cycles in one clock, and the recipe is in
[[android-video-punch-through]].

And there is one concrete gap to close first, which is why this item and
[[choreographer-vsync-phase-offset]] should be picked up together. The
backend receives a Choreographer frame time on every callback and throws it
away: `frame_callback` in `alloy/src/vsync.rs` takes it as
`_frame_time_nanos`, with a comment saying the timestamp is deliberately
unused because the paced clock models time as frame counts. So the state
machine has no way to know WHICH vsync it was woken for, and therefore cannot
tell a wake that is on cadence from one that has already slipped a period.
It is blind to exactly the event this item is about. Reading that timestamp
is both the fix for the sibling item, whose whole content is that the same
value is being misread elsewhere, and the missing instrument here.

Related: this is the same territory as the parked
[[frame-pacing-fluency-hunt]], but with a sharper symptom than that hunt had.
The video plane work is a separate matter and its fence wait is not involved:
these numbers are from a page with no video in it at all.
