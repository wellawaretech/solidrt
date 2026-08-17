---
title: Frame driver pacing contract
description: Pacing verdicts cost 90s on-device censuses because there is no way to run the frame driver against a synthetic vsync grid, and frames carry no deadline, so an overrunning critical path jitters between 1 and 2 vsyncs instead of degrading to a stable cadence. Harness first, then deadline-scheduled frames.
created: 2026-08-14
---

# Frame driver pacing contract

Two structural gaps that the video pacing trace kept running into, neither
of them video-specific. Split out of [[video-playback]] because they outlive
it: they are properties of the frame driver, and every future pacing
question pays the same toll.

## Symptoms

**Every pacing verdict costs a TV session.** Deciding whether a pacing
change helped currently means: build, adb install, run a 90s SF census with
MCP roundtrips at 5-8s each, and avoid touching the client during the census
because the debug calls run on the JS thread and cause the drops being
counted. That is a handful of hypotheses per day, for questions that are
mostly pure logic: when does the clock advance, what phase does the content
grid sit at, what happens when one stage overruns. The device is needed to
learn device facts (driver queueing, MediaCodec, actual scanout). It is not
needed to learn that a cadence rule is wrong.

**Frames are best-effort, so overrun turns into jitter.** Nothing in the
loop states when a frame was supposed to reach the screen. Each frame runs
as fast as it can and either fits in the slot or does not. On the TV that
shows up as an uneven 1-vs-2 vsync cadence, which the eye reads as
non-fluent even when the frame rate is nominally full ([[video-playback]],
pacing probe: a trivial 360p 50fps clip presents at ~41fps; 1080p adds
~28ms on top of a ~27ms base critical path). 25fps content on the 50Hz panel
needs the whole tick -> upload -> convert -> composite -> swap chain inside
two refresh periods; 1080p takes about three, and the result wobbles between
17 and 19 fps rather than settling anywhere.

The second symptom also explains why throughput work did not move it:
[[texture-upload-staging]] stages 1+2 cut raster busy by 25% and fps did not
change at all. The loop is bounded by tail latency against a deadline it
does not know about, not by mean cost.

## Stage 1: synthetic vsync harness

Run `FrameDriver` (alloy/src/rendertree/frame.rs) against an injectable
clock and vsync source instead of the real display, in
`alloy/src/tests/frame.rs` alongside the existing gate tests.

It has to be able to express, at minimum:

- a configured refresh (50Hz specifically, since that grid is where the
  content-rate mismatch lives) with content at a non-equal rate;
- a stage overrunning its slot once, and repeatedly;
- an extra loop iteration arriving between presents (the stray-idle-tick
  shape, see below);
- present-return jitter, which is what `PresentClock`'s GAIN smoothing
  exists to absorb.

Assertions run against the present ledger (filed as the first step of
[[video-playback]]'s Frame scheduling section): the ledger is what turns
"cadence" into something a test can state, and the same records serve the
on-device probe. Build it there, consume it here.

What this buys: cadence, phase and clock-advance bugs become millisecond
tests in the repo, and the TV goes back to answering only the questions
that genuinely need hardware.

## Stage 2: deadline-scheduled frames

Give each frame a target present time derived from the timeline, and let
the driver measure its own per-stage latency. Then a frame that cannot make
its slot becomes a decision rather than an accident: drop cleanly to a
stable divisor of the refresh (present every second or third vsync) and hold
there until the measured critical path fits again. A metronomic 12.5fps
reads as fluent; a wobble between 17 and 19 does not.

Layering: alloy owns the deadline, the
per-stage measurements and the mechanism, since those are facts about the
display and the loop. Which degradation is acceptable is policy and comes
from lattice, the same way `FramePacing` does today
([[frame-pacing-fluency]]). No device special cases inside alloy.

Every degradation decision is recorded, so a census can say why the cadence
changed instead of leaving it to be inferred.

## Stage 3: the stretched timeline is invisible to the app (2026-08-17)

A third property of the same contract, from the app side. Under a sustained
present stall - something else on the machine holding the GPU, the case a
reboot cures - the paced timeline advances one refresh period PER PRESENT,
not per second (`lattice/src/paced_clock.rs`), so:

- every frame is a uniform period step and the animation stays smooth BY
  CONSTRUCTION - the pacing is doing exactly its job;
- the whole simulation stretches against real time, so input handling,
  physics and audio all land late IN PROPORTION;
- to the person holding the device that is indistinguishable from input lag.

The three observations that read as contradictory when it was reported -
smooth, uniformly late, and audio late WITH the visuals rather than ahead of
them - are one fact. Worth stating explicitly: "is the sound late too?" is
the natural discriminator between an input-delivery problem and a
render-queue problem, and under this clock it does not discriminate at all.

The gap: an app cannot detect it. Every clock it has - the `tick` argument,
`performance.now()`, timers - rides the paced timeline, so from inside
nothing looks wrong; the only tell is diffing `Date.now()` against `tick`
yourself, which requires already knowing the pacing exists. An app that
wanted to react (drop a supersample factor, shed effects, tell the user
something else is eating the GPU) has no supported signal.

Wanted, smallest first:

1. Document the stall behaviour next to the pacing note on `onFrame`
   (`packages/core/src/window.ts`), which today presents pacing purely as a
   smoothness feature. Same place: the UPPER dt clamp apps are told to
   write is nearly inert on a GUI runtime, because `tick` is paced so `dt`
   sits near one period regardless of how long a frame really took -
   `Math.max(0, ...)` earns its place (the hot-reload negative delta,
   [onframe-tick-reset-on-reload](onframe-tick-reset-on-reload.md)),
   `Math.min(cap, ...)` mostly does not, and an author who assumes the
   opposite reasons wrongly about slow machines.
2. Expose the pacing error: an accessor (`clockDrift()`, ms the paced
   timeline is behind wall clock) or a fourth `onFrame` argument. It is a
   subtraction the runtime already performs to slow-correct; surfacing it
   turns an invisible failure mode into a two-line adaptive-quality check.
3. Surface it in `get_stats` too, so the MCP tools can answer "is this
   client's timeline behind?" without instrumenting the app - see
   [mcp-interaction-perf-visibility](mcp-interaction-perf-visibility.md).

Stage 2's deadline work is where the number naturally comes from: a frame
that knows when it was due knows how late the timeline is.

## Done looks like

- Stage 1: a pacing regression is caught by `cargo test -p alloy`, and a
  proposed pacing rule can be evaluated without an adb session.
- Stage 2: under a critical path that does not fit the slot, presents stay
  on a fixed grid and the driver reports which grid it chose and why.
- Stage 3: an app can read how far its timeline is behind wall clock, and
  the `onFrame` docs say what pacing does under a sustained stall.

## Not in scope

- The video timeline clock, frame selection and standing demand: those stay
  in [[video-playback]]'s Frame scheduling section, which is authoritative
  for that chain.
- The `VsyncLocked` / `SwapPaced` policy and the touch-fact correction:
  settled in [[frame-pacing-fluency]].
- Real presentation timestamps. `PresentClock` models the cadence precisely
  because the platform does not report it; if a real timestamp source ever
  lands, the deadline gets more accurate, but nothing here depends on it.

Related: [[video-playback]], [[frame-pacing-fluency]], [[frame-pacing]],
[[texture-upload-staging]], [[idle-tick-gpu-backlog-runaway]].
