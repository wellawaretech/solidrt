---
title: Android VP9 encoders
description: Which Android devices have a VP9 encoder and how the one we use behaves; the SM-T500 has none (the platform's software encoder is excluded on slow-cpu devices), so `record` rejects there with `unsupported` by decision, and the Pixel 7's hardware encoder pads its output buffers and runs well under the bitrate asked.
created: 2026-11-14
---

# Android VP9 encoders

On Android `record` (okf/done/video-encode.md) encodes through
MediaCodec (`forge/src/video/mediacodec/encode.rs`); libvpx's encoder is
built off Android only.

## A device without one: `unsupported`, by decision

**The SM-T500 has no VP9 encoder** (Android 12, found 2026-10-16):
`AMediaCodec_createEncoderByType` returns none. The platform's
`c2.android.vp9.encoder` is declared `variant="!slow-cpu"` in the swcodec
apex, and the tablet is a slow-cpu device: its vendor list includes
`media_codecs_google_video_le.xml`, whose encoders are H.263, H.264,
MPEG-4 and VP8 only. Its hardware encoders are AVC and HEVC. A first
reading of the XML missed the variant.

Decided 2026-10-16: `record` rejects on such a device with `unsupported`,
to be revisited when a need arises. Such a device can still receive: in
a call it joins receiving only. The two ways to serve it, when wanted:

- **libvpx's own VP9 encoder built for Android too**: +1.3 MB, the same
  library at our realtime settings, MediaCodec staying first where it has
  VP9. The cost on a slow-cpu device is unmeasured.
- **VP8 as a second codec through the whole pipe**: the writer's and the
  reader's codec ids, the VP8 decoder configured in libvpx, the VP8 mime
  in the MediaCodec paths, codec selection in the players, the live form.
  The tablet's software VP8 (`OMX.google.vp8.encoder`, libvpx, declared
  21-30 fps at 720p) is the only royalty-free encoder it has, at roughly
  twice VP9's software speed.

## The Pixel 7's encoder

- `c2.exynos.vp9.encoder` is hardware, up to 3840x2176, declared 160-240
  fps at 720p. It took planar I420 input at the first try.
- Its output pts equal its input pts, one block per frame; no superframes
  seen.
- **Its output buffers carry padding after the frame** (found 2026-10-16
  on the first live cast): zeros mostly, a run of 0xff on some frames
  (sixty of a 223-byte frame). libvpx finds a frame's end where its
  arithmetic decoder stopped, skips zero padding, and takes a nonzero run
  for a second frame with an invalid marker, after which every frame
  until a keyframe is refused. ffmpeg's VP9 decoder ignores trailing
  bytes, which hid it from the file checks. Our decoder hands a block
  without a superframe index to libvpx as a one-frame superframe stating
  its length (VP9 Annex B, `forge/src/video/vpx/mod.rs`), and
  `forge/src/tests/data/pixel_live.webm` (the first second of such a
  stream) is the fixture.
- **It runs well under the bitrate asked**: 240 kbit/s at 324x720 against
  a 700 kbit/s budget, in a file (VBR) and live (CBR) alike, a soft
  picture: okf/backlog/mediacodec-vp9-bitrate-undershoot.md.
