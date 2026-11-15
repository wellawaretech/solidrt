---
title: The Pixel's hardware VP9 encoder gives a soft picture
description: On the Pixel 7 `c2.exynos.vp9.encoder` produces about 240 kbit/s at 324x720 against a 700 kbit/s budget, in a file (VBR) and live (CBR) alike, so a screencast or a call from the phone looks soft; find out whether the codec takes the bitrate mode and the bitrate at all, and raise the budget or the quality setting until it does.
created: 2026-11-14
---

# The Pixel's hardware VP9 encoder gives a soft picture

## Symptom

A recording on the Pixel 7 (okf/done/video-encode.md, MediaCodec in
`forge/src/video/mediacodec/encode.rs`) comes out at about a third of the
bitrate asked. A screencast of the window at 324x720 gives about
240 kbit/s where `default_video_bitrate` asks about 700 kbit/s (0.1 bit
per pixel per frame at the nominal 30 fps). The file form (VBR) and the
live form (CBR) give the same figure. The picture is visibly soft on the
receiving laptop. libvpx on the laptop gives 770 kbit/s against a
920 kbit/s budget at 640x480, so the undershoot is the hardware codec's.

## Likely causes, to check in this order

1. **The codec ignores `bitrate-mode`.** A codec that does not list the
   mode in its capabilities (`MediaCodecInfo.EncoderCapabilities
   .isBitrateModeSupported`) falls back to its own default silently. Read
   the output format and the capabilities with `dumpsys media.player` or
   a probe.
2. **The codec's rate control reaches its quality target first.** Many
   hardware encoders stop spending bits once a QP target is met, on
   simple content (a mostly flat UI screencast) especially. A camera
   clip with motion would then come closer to the budget; measure one.
3. **The budget is right but the nominal rate is not.** The window draws
   at 60 fps while the budget assumes 30, so each frame gets half.

## Done looks like

- A screencast and a camera clip from the Pixel each read within, say,
  20% of the bitrate asked, or the reason they cannot is stated.
- The live default is raised if that is the fix, and the receiving
  laptop's picture of the phone is sharp by eye.
