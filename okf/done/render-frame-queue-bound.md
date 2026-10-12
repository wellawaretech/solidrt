---
title: Bound the frames sol render holds for its PNG writer
description: sol render queued every read-back frame for a single PNG writer thread with no bound, so at 4K, where rendering outpaces encoding, memory grew by one raw frame per frame of backlog for the whole render; now a few writers take frames from a bounded queue the driver waits on.
created: 2026-10-12
completed: 2026-10-12
---

# Bound the frames sol render holds for its PNG writer

## Problem

At 3840x2160, `sol render` renders frames faster than it writes them, and
every frame still waiting to be written stays in memory as raw RGBA. The
client grows by one frame (31.6 MiB at 4K) for each frame it gets ahead,
for the whole render; the memory comes back only after the last frame,
while the writer drains. The growth is linear in the render's length, so
a long 4K render, or a high `--fps`, runs the machine out of memory. At
1920x1080 the writer keeps up and memory stays flat.

## Analysis

`drive` in [render_host.rs](../../lattice/src/render_host.rs) read each
frame back (`read_window`, a fresh `Vec` per frame) and sent it on an
unbounded `std::sync::mpsc::channel` to one writer thread, which encoded
with `forge::image::encode_png` and wrote the file. The driver never
waited on the writer until it joined it after the last frame, so nothing
held rendering back when encoding fell behind.

The encoder is already on the image crate's fast compression (the
`PngEncoder::new` default); the remaining cost is the adaptive filter and
deflate on one thread, which is slower than the stepped render at 4K.

A render runs on app time (stepped mode), so making the driver wait for
the writer costs wall-clock time only; the frames are the same.

## Done

`FrameWriters` in render_host.rs:

- **A bounded queue the driver awaits.** `tokio::sync::mpsc::channel`
  of `QUEUED_FRAMES`; the driver's `send().await` yields the engine task
  while the writers are behind, the writers `blocking_recv` on their own
  threads.
- **Parallel writers on one queue.** `PNG_WRITERS` threads, fewer on a
  machine with fewer cores than that plus the render's own two (engine and
  raster), take frames from the shared receiver in any order (the lock is
  held for the wait only, as in `forge::workers`). The file name carries
  the index. Peak memory is `QUEUED_FRAMES + writers + 1` raw frames.
- **A failed write stops the render.** The writers share a `OnceLock` with
  the first failure; the driver checks it before each frame, so a render
  stops one frame after a write fails, and `finish` reports the failure
  over the frame count. This closed item 3 of
  [render-data-and-output-safety](../backlog/render-data-and-output-safety.md).
- **The output directory is created before the first frame.** An output
  path is a request to write there, whoever passes it (`sol render -o`, a
  direct `--out`); a directory that cannot be created fails the render
  before anything is rendered.
- **`forge::image::write_png`** streams the encode into a `BufWriter` on
  the file instead of building the compressed image in memory first: the
  same bytes, no compressed buffer beside the pixels, and the file is
  opened before the encode, so an unwritable path fails in milliseconds.
  The test host's snapshot writes through it too.
- A unit test of the writers without an engine
  (`lattice/src/tests/render_writer.rs`).

## Not done, on purpose

- **The shared `forge::workers` pool** instead of render-private threads:
  about the same code, with the file write and the bookkeeping moved into
  engine tasks, for a process-wide bound on encode threads that a render
  process has no other user of. `Workers` is built around awaiting one
  job's result; the render's parallelism is the queue itself.
- **Buffer reuse for the read-back.** `read_window` allocates a fresh
  31.6 MiB `Vec` per 4K frame, mmap/munmap churn with first-touch faults
  at seven frames in flight; it would touch alloy's capture RPC, and deflate
  dominates. Worth a note only if a 4K profile says so.
- **A faster PNG filter or compression level.** Roughly doubles throughput
  but changes the bytes. The real answer for video at 4K is raw or y4m
  frame output to ffmpeg, filed in ideas.md.
