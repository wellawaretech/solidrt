---
title: A live player's stepped frame take can deadlock the headless host
description: Under the stepped clock (sol test, sol render) a playing texture player fed from the JS thread makes the raster thread wait in its frame take for a frame the UI thread has yet to pump, so any blocking raster call from the UI thread during that frame (a texture create, a compile, a readback) deadlocks the process silently; seen one run in four with two live p2p players in one test.
created: 2026-10-17
---

# A live player's stepped frame take can deadlock the headless host

## Symptom

A headless run (`sol test`, `sol render`) stops making progress with no
message: every thread idle in a futex wait, the test runner printing
nothing, the process ignoring SIGTERM. Seen 2026-10-17 in
`packages/core/tests/record.test.tsx` once in about four runs, with two
live players (each on a p2p stream carrying a live recording) created in
one component. The older note of the same family is the stepped take that
"waits forever with no message" when the producer is not pushing (the
first render of the latch hung on it).

## Mechanism

- Under a stepped clock the raster's take (`YuvLatchShared::take` with
  `wait`) waits until it is final: a frame due after the deadline is
  queued, or nothing more will come (`YuvLatch::settled`). That is what
  makes a capture deterministic: the frame shows the frame due, never the
  one that happened to have arrived.
- A live player's frames come from a `ReadableStream` the JS pump
  (`drive_async_iterable`) feeds into the reader's channel, on the UI
  thread. So the take's wait is, transitively, a wait on the UI thread.
- The UI thread's blocking raster RPCs (a texture create, a shader
  compile, a readback) queue behind the frame on the raster thread's one
  ordered channel. One issued while such a take waits never returns: the
  raster waits for the pump, the pump waits for the UI thread, the UI
  thread waits for the raster. The second player's `create_yuv_texture`
  in `build_player` was the RPC in the test; gdb as the process's parent
  showed `sol-raster` in the take and `sol-ui` in `create_texture_at`'s
  `recv`.
- A file source never trips it: the reader thread reads the file itself,
  so the take's wait ends without the UI thread. An http source is fed by
  tokio workers, which keeps the process moving (the capture is not
  deterministic, but nothing deadlocks).

## Shape

The take should wait for the producer's decoding to catch up, never for
input only the UI thread can deliver:

- The reader knows when its source is a pushed stream (`source::from_stream`)
  and when it is waiting on it (`Next::Waiting`). The worker, on a feed
  that waits on such a source, tells the presenter host the producer is
  starved; `YuvLatch::settled` then holds when the queue has nothing due
  after the deadline and the producer is starved (the latch learns it
  through the `PresenterHost` the texture player's worker pushes into,
  `flux/src/alloy_plugins/video.rs`). The capture shows the newest frame
  that had arrived, which for a live source is the only honest answer; a
  file source is never starved, so its captures stay exact.
- Clear the flag when the feed takes a packet again.
- The tiny.md item's log after a bounded wait stays worth having beside
  it, naming the texture and the deadline, so any remaining wedge is a
  log line.

## Done looks like

- `packages/core/tests/record.test.tsx` runs its two-way call test with the
  players created inside a component that mounts during the frame loop
  (the shape that deadlocked), a hundred times without a hang.
- A `sol render` of an app playing a live p2p stream completes.
- The window-clip and file-playback captures are unchanged (the existing
  record and video tests, frame for frame).
