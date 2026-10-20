---
title: Run the live player timing tests on a virtual clock
description: Two live floor tests in forge/src/tests/live.rs feed and play frames on the wall clock with ~15 ms of slack for thread wakeups, so the macos-14 CI runner fails them and CI skips them there; a clock that owns the player's waits would make them deterministic.
created: 2026-10-21
---

Low priority: CI skips the two tests on darwin only, and they still run on
the linux, linux-arm and windows runners and locally.

## Problem

`a_source_paced_at_the_floor_plays_without_buffering` and
`a_stall_is_recovered_at_the_latency_through_the_keyframe_before_it`
(forge/src/tests/live.rs) play a live stream against `Clock::monotonic()`.
A feeder thread sleeps to each frame's arrival time, and the player's
worker sleeps to each frame's release (`wait_for_release` and the
`IDLE_POLL` wait in forge/src/video/worker.rs). The assertions are about the
buffering policy (every frame shown, at most one buffering episode), but
the margins under them are wall-clock margins. At the 150 ms floor
(`LIVE_MIN_LATENCY_US`) the lead may swing 100 ms (the floor less
`LIVE_LOW_WATER_US`), the test's `JITTER_MS` pattern plus one frame
interval takes about 83 ms of that, and about 15 ms is left for every
thread wakeup on the way.

The macos-14 GitHub runner (a 3 vCPU VM) misses that slack. Two runs of
the same commit lost 21 and 29 frames in 5 and 7 buffering episodes, mostly
in runs of five frames (a buffering episode, then a catch-up to the floor),
and the stall test once lost a frame after its recovery. The same commit
passed both tests 10 times out of 10 on an M1 (8 cores, load average up to
17), so macOS itself holds the floor. `.github/workflows/ci.yml` skips the
two tests on the darwin runner (`matrix.skip` in test-forge).

The stepped clock does not fit as it stands: against it the worker drops
nothing as late and does not judge buffering on the lead (the
`clock.stepped` branches in worker.rs), and that is what these tests check.

## Done looks like

The live floor tests run on a virtual clock with the free-running
semantics (lateness, buffering on the lead), on which the feeder's
arrivals and the worker's waits are deadlines, so the outcome is the same
on any machine. The darwin skip is gone from ci.yml.

## Involves

- The clock owns waiting, not only reading: `Clock` gains a wait (until a
  deadline, or until a command arrives) that the worker uses in place of
  `recv_timeout` against `Instant` (`wait_for_release`, the `IDLE_POLL`
  and audio start waits). The monotonic clock keeps today's behavior.
- A test clock that advances only when the threads taking part (feeder,
  worker, reader) are all blocked on it. Knowing when that is, without a
  registry every thread has to join, is the open design question. Decode
  costs no virtual time on it, which is right for a test of the policy.
- The other real-time tests in live.rs (the `IDLE_POLL` sleeps while
  paused) would move to the same clock and get faster.
