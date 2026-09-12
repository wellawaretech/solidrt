---
title: Fullscreen video by surface punch-through on Android
description: Fullscreen playback decodes straight into its own SurfaceView, composited by SurfaceFlinger under a translucent UI, instead of through the texture pipeline. Deletes the whole per-frame chain (upload, YUV conversion, full-window repaint, our present cadence) that kept fullscreen 1080p from fitting the TV's 20 ms budget. Decided 2026-09-12, reversing the 2026-08-12 rejection; the texture pipeline keeps every non-fullscreen use.
created: 2026-09-12
---

# Fullscreen video by surface punch-through on Android

Decided 2026-09-12, reversing the rejection recorded in
[[video-playback]] (whose "Pipeline decision" section carries the original
reasoning and the reversal). This is a second path beside that pipeline,
not a replacement for it: video in a UI, video on a mesh, and anything
that wants a texture id keep going through the existing one.

## Why, in numbers

Fullscreen 1080p25 never became fluent on the Philips TPM171E through the
texture pipeline, and the 2026-09-12 measurements say why it cannot be
made to fit by shortening steps. Per loop iteration at 1080p, against a
20.0023 ms refresh period:

| | ms |
|---|---|
| plane upload (after staging; 27.5 before) | 10.4 |
| window draw | 11 - 13.5 |
| present | 4.5 |

The whole chain is on the critical path of one refresh period, and the
window draw does not shrink with the video's size: it is a full-window
repaint because a texture changing behind an unchanged id produces no
damage ([[live-texture-content-damage]]). 720p fits (49.2 presents a
second, 124 of 126 intervals on the grid, every frame delivered); 1080p
lands at 41.7 a second with a quarter of its intervals double-length.

Punch-through does not shorten that chain, it removes it. The decoder
renders into its own surface, SurfaceFlinger composites it, and none of
upload, conversion, repaint or our present cadence is involved at all.

## Mechanism

`AMediaCodec` is configured with a SurfaceView's `ANativeWindow` as its
output surface. Decoded buffers are never mapped: `releaseOutputBuffer`
hands the frame to the surface, and `releaseOutputBufferAtTime` hands it
over with a target presentation nanotime, which is where A/V sync happens.
The demuxer, the AAC decode and the audio clock are unchanged - the audio
position stays the master clock, and it feeds the release time instead of
feeding frame selection.

Layering consequences:

- This is a SECOND decoder mode, not a change to
  `forge/src/video/mediacodec.rs`. That file stays as it is for the
  texture path.
- `VideoDecoder`'s contract (feed an access unit, collect planar frames)
  does not describe it: there are no frames to collect. It needs its own
  shape rather than a bent version of that trait.
- Everything in the texture path's frame scheduling - the engine-timeline
  clock, the half-period lookahead, standing frame demand, the tail-frame
  release - is inert here. The codec and SurfaceFlinger own the timing.
- forge stays engine-free, but not window-free: a surface handle has to
  reach it. Where that handle is owned (alloy, which owns the Android
  window, versus forge, which owns the decoder) is the first structural
  question to settle.

## The part that is not free

The decode path is independent of the texture path. The Android glue is
not, and it is the only cost punch-through adds rather than removes:

- A second SurfaceView beneath SDL's, with z-order set, and SDL's GL
  surface made translucent so the video shows through it.
- A translucent main surface changes compositing for the WHOLE app, not
  just screens that play video. Measure that early and on the TV, before
  building on it: it is the one regression this design can cause, and it
  would hit every app.
- JNI, and meddling with SDL's Android view - one of the original
  objections, now accepted.

## Known device risk, from our own notes

The surface path is where this device class has documented trouble, and
that is worth verifying first rather than discovering late:

- The documented Philips colour corruption (pink/purple) was in Kodi's
  SURFACE mode, not its buffer mode.
- ExoPlayer carries MediaTek-specific surface workarounds
  (`codecNeedsSetOutputSurfaceWorkaround`).
- Our own 2026-08-12 probe found surface-attached taps per-device
  untrustworthy on exactly this chip: `AMediaCodec_start` failed outright
  with an AImageReader window as output. A SurfaceView is a different
  consumer (a real display surface, not a CPU-readable gralloc one), so it
  is expected to work where that did not - but "expected" is the word, and
  a first probe that just plays a clip into a bare SurfaceView answers it
  before any of the glue above is written.

## Open, for decision

- **API shape.** The texture pipeline's currency is a texture id displayed
  by `<texture>`, which is why there is no video primitive. A
  punch-through player has no texture: it is a hole. Fullscreen-only keeps
  this simple (the video is behind everything and the UI draws over it),
  but the app still needs a handle to say play/pause/seek and to know the
  video is there. Whether that is a distinct entry point, a mode on
  `createVideo`, or something a component owns is undecided.
- **Falling back.** What a fullscreen player does on a platform with no
  punch-through path (every non-Android target today), and whether
  choosing between the two paths is the app's business or the runtime's.
- **Where the surface is created and owned**, per the layering note above.

## Done looks like

Fullscreen 1080p25 on the TV, presenting every frame on an even grid,
read off the SurfaceFlinger census for the VIDEO layer rather than ours;
correct colours (the MediaTek risk above); UI composited over it without
a measurable cost to apps that play no video.

Related: [[video-playback]], [[live-texture-content-damage]],
[[texture-upload-leases]], [[frame-driver-pacing-contract]].
