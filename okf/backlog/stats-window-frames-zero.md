---
title: get_stats' time window reports frames 0 for an animation that just ran
description: On the SM-T500, get_stats with window_ms 3500-8000 called right after a 2 s add/remove animation answered `window: { frames: 0 }` in most calls (a few answered 8-25 frames, never the ~120 presented), while SurfaceFlinger's latency history held all of them; only the frozen-clock window_frames path was reliable.
created: 2026-09-22
---

# get_stats' time window reports frames 0 for an animation that just ran

## Symptom

2026-09-21/22, demo/notes on the Galaxy Tab A7, release client. Flow:
`send_input` a tap plus a trailing 2.3 s delay (so the call returns after
the animation), then `get_stats` with `window_ms` 3500, 6000 or 8000. The
answer was `window: { frames: 0, windowMs: N }` in roughly two calls of
three; the others reported 8, 12, 16, 25 or 56 frames for an animation
that presented ~120 (per `dumpsys SurfaceFlinger --latency`, and per the
`cadenceHold` change the same call reported). `set_time_scale 0` +
`step_frames` + `window_frames` reported the stepped frames every time.

## What done looks like

- A time window covers the frames presented in the last N ms of wall
  time as the caller understands it; if the window is keyed to the app
  clock or to frames that "changed the picture" in a way a layout slide
  does not satisfy, say so in the tool description and offer the
  wall-clock one.

## Not reproduced (2026-09-23)

On the same tablet, with a probe sliding ten panes on a layout transition
for 2 s, `get_stats` with `window_ms` 4000 read right after the slide
answered 72-78 frames three times out of three, and 75 beside a
`srt android --census` count of 76 presents over the same span. Two
things changed since the report: the cadence hold resets after idle
(done/cadence-hold-sticky-android.md), and the GPU term no longer
stretches frames. The stats query now logs, at debug level, what the
ring held whenever a window comes back empty over a non-empty ring
(`[stats] window ... found no frames; the ring holds N (oldest, newest)`),
so a recurrence names the gap. Close this if the next Android session on
the original scene does not reproduce it.
