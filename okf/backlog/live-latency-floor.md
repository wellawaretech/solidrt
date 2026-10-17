---
title: Give live sources their own buffering floor
description: A live player cannot play closer than 0.75 s behind the sender, whatever `latency` asks, because the live form inherits the file stream's low water (0.5 s) plus a 250 ms hysteresis as its resume floor; a call therefore runs about a second behind. A sender-paced source needs a floor of a few frames, so the three buffering constants should follow the source kind.
created: 2026-10-17
---

# Give live sources their own buffering floor

## Symptom

A call (`examples/call`) or a screencast opened with `latency: 0.3` plays
about a second behind the far camera, read by eye on the laptop and Pixel
call of 2026-10-17 and off the clocks on the screencast the day before
(0.8 s). The relay, QUIC and the encoder add tens of milliseconds between
them (no skipped or late frames at the current buffer): the second is the
player's own buffer.

## Cause

`forge/src/video/transport.rs` sizes buffering for a file fetched over
HTTP, a source that delivers in bursts:

- `STREAM_LOW_WATER_US` 0.5 s: a lead below it starts buffering.
- `STREAM_RESUME_BUFFER_US` 2 s: buffering ends once the lead is back here.

The live form (`reader.rs`, `Live::with_latency`) makes its resume buffer
the `latency`, clamped between `LIVE_MIN_RESUME_US` (the low water plus a
250 ms hysteresis, 0.75 s) and the file value. Playback starts once the
lead reaches the resume buffer and the catch-up rule
(`LIVE_CATCH_UP_MARGIN_US`, 0.5 s past it) holds the lead near it, so a
`latency` under the floor is silently 0.75 s.

## Shape

- The buffering constants become a property of the source kind: a live
  source paced by its sender (one frame per interval, a stall a real
  stall) gets a low water near zero, a resume floor of a few frames
  (100 to 150 ms to start with) and a catch-up margin to match; the file
  values stay for files and HTTP sources.
- `latency` then means what it says above the live floor, and the floor
  is documented in `VideoOpenOptions.latency`.
- The live reader knows its source (`source::from_stream`); the open
  passes the kind down, no new option.

## Done looks like

- The two-way call on the laptop and Pixel over the relay at `latency:
  0.15` plays with no buffering episodes over a minute, no skipped or
  late frames, and the delay read off the two clocks is well under half
  a second; the point where Wi-Fi stalls start to show is measured and
  stated.
- The file and HTTP playback tests are unchanged.
- The screencast and call examples drop their `LATENCY_S` to the new
  floor's range.
