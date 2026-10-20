---
title: Give live sources their own buffering floor
description: A live player could not play closer than 0.75 s behind the sender, whatever `latency` asked, because the live form inherited the file stream's low water plus a hysteresis as its resume floor; the buffering thresholds now follow the playback policy the `latency` option declares, the floor is 0.15 s, and a stall's backlog is skipped back to the latency through the keyframe before it instead of landing on a keyframe.
created: 2026-10-17
completed: 2026-10-20
---

# Give live sources their own buffering floor

## Symptom

A call (`examples/call`) or a screencast opened with `latency: 0.3` played
about a second behind the far camera, read by eye on the laptop and Pixel
call of 2026-10-17 and off the clocks on the screencast the day before
(0.8 s). The relay, QUIC and the encoder add tens of milliseconds between
them (no skipped or late frames at the current buffer): the second was the
player's own buffer.

## Cause

`forge/src/video/transport.rs` sized buffering for a file fetched over
HTTP, a source that delivers in bursts: a low water of 0.5 s and a resume
buffer of 2 s. The live form (`reader.rs`, `Live::with_latency`) made its
resume buffer the `latency`, clamped between the low water plus a 250 ms
hysteresis (0.75 s) and the file value, so a `latency` under the floor was
silently 0.75 s.

## What was done

- **The thresholds follow the policy the `latency` option declares, not
  the source kind.** The backlog shape keyed them on the opener, but a
  `ReadableStream` reaches the reader through `from_stream` whether it is
  a live recording piped over p2p or a fetch body of a two-hour file, so
  the opener cannot tell sender-paced from bursty. The app already says
  which it wants: `latency` present means "play at the edge, a stall is a
  real stall", absent means buffered playback. `transport::Thresholds`
  holds one set per policy (`FILE`: 0.5 s low water, 100 ms sink low
  water, 2 s resume; `live(latency)`: 50 ms low water for both, the
  latency as the resume), `Buffering` judges against a set, and `Live`
  carries the floored latency (`LIVE_MIN_LATENCY_US`, 150 ms, capped at
  the file resume buffer).
- **The lead the buffering rule judges is measured against the viewer's
  content time**, the anchor's reading, not the last released frame: a
  frame is released a release lead (50 ms) early, and at a latency of a
  few frames that read as a third of the budget gone, which made the
  floor flap. While buffering (the picture stands still, the anchor's
  reading runs on), before the anchor is set and against a stepped clock
  the reader's own lead stands in.
- **A live catch-up skips to the latency through the keyframe before it**
  instead of landing on a keyframe. With a 2 s live GOP the old cut left
  the player up to 2 s behind after every stall until the next keyframe
  came. The cut now resumes at the first frame at or past `newest -
  latency`, keeps the queue from the newest keyframe at or before it (the
  worker decodes those frames unshown through the seek-preroll path,
  `skip_until`), keeps audio from the track's seek preroll before it, and
  publishes the burn as `buffering`. While the burn is in flight and the
  edge moves on, the target moves within the epoch (the reader advances
  `resume_us`, the worker follows with its skip and audio discard) rather
  than starting a new one, which would flush the decoder and restart the
  burn; a newer keyframe that comes inside the window is cut to on the
  way. The presenter is no longer flushed on a catch-up: the queue may
  continue the group the decoder is in, and what the decoder holds is
  before the target anyway. The catch-up margin dropped from 500 ms to
  200 ms (above the live hysteresis plus a burst).
- **A release from before the epoch's resume position no longer moves
  the head back.** Found on the Pixel: the worker had a frame in hand
  (waiting for its release time) when a cut landed, released it after
  the wait, and `released()` set the head to that older frame, which
  grew the lead past the margin again and cut again, 150 times in 30 s.
  Any epoch's legitimate releases are at or past its resume position, so
  a release below it is ignored.
- The floor is documented on `VideoOpenOptions.latency`; the call and the
  screencast examples open at 0.15 s. The sink-start timeout log now
  says how much audio the sink held, and debug-level lines report each
  cut, each epoch's first shown frame, a release wait past a frame
  interval and a stale release, which is what diagnosed the above.

## Tests

`forge/src/tests/live.rs`: the paused-player cut resumes at the latency
through the keyframe before it in one epoch and moves within that epoch
when the edge moves on; play after a pause resumes at the moved target
without showing the keyframes it decoded through; a source paced at the
floor with up to 30 ms of arrival jitter plays every frame with no
buffering episode past the start (real clock); a one-second stall is
recovered at the latency with the backlog cut (no run of shown backlog
frames comes near the whole), the keyframe before the target shown once,
and every paced frame after it shown; a release from before a cut leaves
the head at the target and the epoch intact. `tests/video.rs`: the live
thresholds, and the ordering of the live constants. The file and HTTP
suites are unchanged.

## Measurements

The two-way call (`examples/call`) between the laptop (libvpx encode and
decode) and the Pixel 7 (MediaCodec) over the self-hosted relay at
`latency: 0.15`, 2026-10-20, one minute sampled every 200 ms on each
side with the laptop and Pixel both on this build:

| side | status samples reading "buffering" | cuts, re-anchors, audio restarts in the minute | videoSkipped / videoLateLatches | window missed presents |
|---|---|---|---|---|
| laptop (plays the Pixel) | 0 of 245 | 0 (one audio start at the call's start) | 0 / 0 | 0 of 395 frames |
| Pixel (plays the laptop) | 0 of 245 | 0 in the 36 s its log buffer holds | 0 / 0 | 1 of 400 frames |

Before the stale-release guard the Pixel side cut every 130 to 260 ms
(151 cuts in 30 s) and read "buffering" in 223 of 230 samples.

The glass to glass delay, read off one snapshot of the laptop showing its
own clock and, in the far picture, that clock as the Pixel's camera saw
it (the Pixel's selfie side pointed at the laptop screen; the path is
laptop display, Pixel camera, MediaCodec encode, relay, libvpx decode,
laptop display): 0.3, 0.2 and 0.2 s in three readings two seconds apart,
with the clocks at a tenth of a second. The Wi-Fi stall point was not
reached: no stall occurred over the relay on the local network in these
runs.

## Non-goals

- The floor is in time, not frames: the WebM header carries no frame
  rate. A source at a low frame rate (a screencast at 10 fps) passes a
  larger latency; the doc says so.
- The burn's convergence needs decode faster than real time; a decoder at
  or under real time on a stall's backlog never reaches the moving target
  and the picture stays held. Decode runs at 3x real time on the slowest
  target validated, and the next keyframe restarts the burn from closer
  in any case.
