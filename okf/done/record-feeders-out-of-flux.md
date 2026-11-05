---
title: Move the recording feeder loops out of the flux plugin
description: The video and audio feeder threads of `record` (flux:video) are pump logic in a plugin; move them to forge's recorder over a plain value the alloy taps hand over.
created: 2026-11-05
completed: 2026-11-05
---

# Move the recording feeder loops out of the flux plugin

## Problem

`video_feeder` and `audio_feeder` in `flux/src/alloy_plugins/record.rs` are
thread loops in a plugin: they wait on alloy's frame tap and microphone tap,
cut the PCM into Opus-frame blocks, stamp each block's time from the
microphone's open time and position, count dropped frames, and drain one
last frame at the stop. That is recording logic, not marshalling (AUDIT.md
"Flux plugins marshal only").

They sit in flux because flux is the one place alloy (the taps) and forge
(the recorder) meet, and the layering rule forbids a trait one crate
exposes for the other to implement.

## Done

The plugin opens the taps and the recorder and connects them; the loops,
the block sizing, the timestamping and the drop accounting live in
`forge::video::record`.

## Involves

- Decide the plain value the halves meet on: what forge's recorder takes
  from a frame source and a sample source (a channel of frames and PCM
  blocks with their timestamps, or a closure per source), without forge
  naming alloy's tap types.
- Move the loops and their constants (`FEEDER_WAIT`, `STOP_DRAIN`,
  `AUDIO_BLOCK_MS`) to forge; keep the stop and join order of engine
  teardown unchanged.

## Outcome

The halves meet on forge's input contracts, not on a channel or a closure:
`forge::video::record` defines `VideoInput` and `AudioInput` (each yields
`Next<VideoFrame>` / `Next<AudioBlock>`, plain values with their
timestamps), and `Recorder::attach(Inputs)` runs a pump thread per input.
The plugin adapts the taps in one line each (`TapInput`, `MicInput`), the
pattern playback already uses for `FrameSink` and `AudioSink`. The traits
are passed by value at the attach, nothing sits behind a global.

- The pumps own the cursor, the drop counts, the stop drain and the Opus
  block, sized by the encoder's own `frame_samples()` (reported by the
  recorder thread at open), so the 20 ms lives once. `stop` ends and joins
  the pumps before it finishes the file.
- The block timestamp is alloy's: `MicRead::timestamp_ns`.
- The device-pixel box (`node_rect`, the tiny.md item folded in here) is
  the rendertree's: `RenderTree::device_box(id, scale) -> IRect`, which
  lands an axis-aligned box by the paint's own snap (`grid::snap_box`,
  each edge to the nearest pixel) and outsets only a rotated or 3D one.
  `node_rect` outset every box, so a fractional one took a row or column
  of its neighbour into the clip; the cut-out is now the pixels the node
  paints. The rendertree returns a plain whole-pixel rect (`IRect`, beside
  `ISize` in the vocabulary) and names no tap type; `TapRect::within_window`
  only cuts it to the window's unsigned quadrant. `track_rects` stays in
  the plugin as the per-frame hook, since only flux holds both the tree and
  the alloy context.
- Teardown order changed in one place: the stop now closes the window tap
  just before the pumps see the stop flag (before, the flag came first).
  The pump still takes the frame the tap delivers before it closes; the
  record app tests cover it, a phone run does not yet.
