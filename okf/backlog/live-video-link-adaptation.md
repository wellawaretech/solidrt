---
title: Let a live recording follow its link
description: The live form of `record` sends WebM over one reliable, ordered byte stream at a bitrate fixed at the start, with a keyframe only every 2 s; a lost packet stalls everything behind it until the live player's catch-up cuts, a receiver cannot ask for a keyframe, and a link that narrows is overrun; the encoders already take a forced keyframe and a new bitrate, nothing drives them.
created: 2026-11-14
---

# Let a live recording follow its link

## Symptom

A live recording (`record` without `to`, okf/done/video-encode.md)
writes WebM into one byte stream: a TCP connection or a `flux:p2p`
stream, reliable and ordered. On a link that loses packets:

- A lost packet stalls every packet behind it until it is resent. The
  live player then sees a backlog and skips it back to its `latency`
  (okf/done/live-latency-floor.md): the picture freezes, then jumps.
- A receiver that joins late, or loses its decoder state, waits for the
  next keyframe. `KEYFRAME_INTERVAL_US` (`forge/src/video/record.rs`) is
  2 s, and the receiver has no way to ask for one sooner.
- The bitrate is fixed when the recording starts. A link that narrows
  (a phone moving away from the access point, a relay under load) fills
  the live sink, and the recorder drops frames at the full channel
  instead of encoding fewer bits.

The call (`examples/call`) does not need any of this yet: on one local
network it ran a minute each way with no cut, and a dropped connection
is a fresh recording, which begins on a keyframe.

## What is already here

- Both encoders take `force_keyframe` per frame and `set_bitrate`
  (`forge/src/video/encode.rs`, the `VideoEncoder` contract; libvpx's
  `VPX_EFLAG_FORCE_KF`, MediaCodec's `request-sync` and `video-bitrate`).
- The live mode is CBR and error-resilient, so a receiver that loses a
  frame recovers at the next keyframe.
- The live player's catch-up already cuts a stall's backlog.

## Shape

1. A control channel back from the receiver: keyframe requests (the
   player asks when it joins or cannot decode) and the receiver's view of
   the link (delay, loss). One `flux:p2p` stream the other way, or a
   message on the same connection.
2. The sender acts on it: a forced keyframe on request, a bitrate stepped
   down on a growing delay and back up slowly (the shape of WebRTC's
   congestion control, much simpler).
3. Only if the stall-and-cut shows on a real link: QUIC datagrams for the
   video, with the keyframe requests covering the losses. That leaves
   WebM over a byte stream, so it needs its own framing and a live player
   that takes packets, not bytes.

## Done looks like

- A receiver that joins a running live recording shows a picture within
  a frame interval or two of joining, not up to 2 s.
- A link throttled mid-call (e.g. `tc` on the laptop) lowers the sender's
  bitrate within a few seconds, with no buffering episode at the
  receiver; the bitrate recovers when the throttle goes.

## Open

- Whether the control channel is part of `record`/`open` (a live handle
  that takes requests) or the app's (the app wires the player's request
  to the recorder's `keyframe()`).
