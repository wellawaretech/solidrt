---
title: Video encode measurements
description: What recording, the window tap and the live form cost on the laptop and the Pixel 7 (recorder ms per frame, raster issue and map ms, bitrates, missed presents), the binary size of the encoder and the relay, and the external decoders the writer's files were checked against.
created: 2026-11-14
---

# Video encode measurements

Taken while building okf/done/video-encode.md. All on release clients.
The device facts behind the Android encoder (which phones and tablets
have one, and how it behaves) are in okf/notes/android-vp9-encoders.md.

## Camera and microphone to a file

- **Laptop webcam, 2026-10-16** (i5-1340P, 640x480 UVC camera at about 24
  fps): the recorder thread spends 2.8 ms per frame on the RGBA to I420
  conversion plus the VP9 encode (cpu-used 7, 4 threads, 2 tile columns).
  No camera frame dropped over 10 s clips, the process at about a fifth
  of a core more than idle, the UI thread unaffected (p95 0.5 ms, no slow
  frames in the `/stats` windows during three recordings, their starts
  included). A 12 s clip with Opus mono is 1.2 MB, 770 kbit/s against the
  920 kbit/s default budget.
- **Pixel 7, 2026-10-16** (Tensor G2, Android 17, `c2.exynos.vp9.encoder`
  in hardware): the 480x640 camera with the microphone costs 5.5 ms per
  frame on the recorder thread (the I420 convert, the codec's input copy,
  the output drain; the codec runs on its own threads). The clip carries
  Opus at 48 kHz.
- **The RGBA to I420 conversion is cheap enough** to stay on the recorder
  thread: it is inside the 2.8 ms at 640x480 and the 4.5 ms at 720p on the
  laptop, and on the Pixel the cost is mostly per call, not per pixel (see
  below). A camera tap exposing its native YUV is not needed for these
  sizes.

## The window tap (screencast to a file)

- **Laptop, 2026-10-16** (Mesa, 1280x720 window at scale 1, the camera
  viewfinder streaming so every refresh draws): a 19.5 s screencast at
  the window's size took 1172 frames, 60 per second. The raster thread
  spends 0.18 ms per frame issuing (the blit and the pack-buffer read) and
  0.71 ms completing (the fence check, the row-reversing copy out of the
  mapped buffer, the publish); the recorder 4.5 ms per 720p frame (RGBA to
  I420 plus VP9). Scaled to 640x360: 0.29 ms issue, 0.47 ms map, 2.3 ms in
  the recorder. No frame dropped, no missed present, no slow frame, GPU
  frame time 0.6 ms, UI p95 0.6 ms during the recording. 2.8 Mbit/s at
  720p, 0.7 at 360p.
- **Pixel 7, 2026-10-16**: the 411x914-point window at 720 lines (324x720
  pixels) ran 2268 frames in 37.7 s, 60 per second, at 6.5 ms per frame
  on the recorder thread; 162x360 at 5.1 ms. No frame dropped, no missed
  present, UI p95 3.2 ms, GPU frame time 5.3 ms. 240 kbit/s at 324x720,
  well under the budget (okf/backlog/mediacodec-vp9-bitrate-undershoot.md).

## The live form

The delays below were read at the old live resume floor (0.75 s);
okf/done/live-latency-floor.md lowered it to 0.15 s, and the call runs at
0.2 to 0.3 s glass to glass since.

- **Laptop to SM-T500 over a `flux:p2p` direct path, 2026-10-16**
  (1280x720 at 720 lines, libvpx, live mode; the tablet decoding with
  MediaCodec into the texture player): 59 fps, 12 missed presents per 10 s
  window, no buffering episode, about 0.8 s behind the caster's clock. The
  sender spends 37 KB/s.
- **Pixel 7 to laptop and to SM-T500 over TCP, 2026-10-16**: both play at
  60 fps, about 0.8 s behind. The tablet shows 1 missed present per 10 s
  and no buffering, the laptop 8.
- **The two-way call, laptop and Pixel 7 over the relay, 2026-10-17**:
  both sides live within 25 s. Over a 10 s window the laptop latched 2460
  frames, 0 skipped, 0 late, 0 missed presents; the phone 1527, 0
  skipped, 0 late, 4 missed presents.

## Size

- **The VP9 encoder** costs 1.28 MB of linked binary: the libvpx part is
  1732 KiB with it, 448 KiB without (okf/backlog/runtime-optimization.md).
  The Opus encoder was already linked with the decoder.
- **The relay server** (`p2p-relay`): the flux binary (release,
  unstripped, Linux x64) grows from 57.06 MB to 59.52 MB. iroh-relay's
  server half pulls clap, tokio-rustls-acme, rcgen and the certificate
  reloading crates on top of what iroh's client half already links.

## External decoders

The writer's files read in ffprobe and decode whole in ffmpeg (VP9, Opus,
Duration and Cues present), so the output is not only what our reader
accepts. A frame read back from a screencast is upright, the stats overlay
included. ffmpeg's VP9 decoder ignores trailing bytes in a block, so a
file check through it does not catch padding that libvpx refuses
(okf/notes/android-vp9-encoders.md).
