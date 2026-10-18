---
title: The splat re-sort blocks the UI thread and its spike at 1M is unmeasured
description: A camera turn past the order gate radix-sorts the cloud's key mirror and republishes the id stream inside the flush, on the UI thread; at 1M splats that is a periodic stall the JS p50 hides and no probe has isolated (p95/max per re-sort on the Pixel 7), so the frame-time cost of the in-engine sort - the design's one unmeasured term - is unknown.
created: 2026-09-27
completed: 2026-10-17
---

# The splat re-sort blocks the UI thread and its spike at 1M is unmeasured

## Symptom

okf/notes/gaussian-splats-against-the-field.md: the in-engine sort is
what puts the viewer ahead of the worker-sorting viewers on latency, but
it runs synchronously in the flush (`rematerialize_retained_order`: an
LSD radix sort over 1M keys plus a 4 MB id materialization) every ~2
degrees of orbit. Every phone table so far reads GPU time and the JS
p50; the stall each re-sort puts on the UI thread has not been measured.

## Measured 2026-10-17

After okf/done/splat-key-copy.md (the sort reads a 12-byte key column).
Release builds, the train cloud (1,026,508 splats, SH 0) on
probes/splat-mesh-probe.tsx, orbit at 0.3 rad/s, cadence hold off on the
phone; a timer around `rematerialize_retained_order` logged every re-sort
(sort = the radix permutation, republish = the id materialization and
the lease send), /stats windows read the frames. The CPU numbers carry no
clock or skin readings: no cooling between rungs, and the sort shares the
memory bus with the draw.

| rung | Pixel 7 sort ms median / max | Pixel 7 frame p95 / max ms | desktop sort median / max | desktop frame p95 / max |
|---|---|---|---|---|
| 100k | 14.8 / 38 | 42.6 / 49.8 | 6.9 / 8.8 | 11.2 / 12.9 |
| 300k | 32.1 / 45.9 | 41.4 / 49.5 | 16.6 / 35.3 | 26.7 / 38.1 |
| 1M | 50.3 / 94.6 | 73.5 / 82.2 | 76.4 / 121 | 124 / 125 |

- The republish (4 MB of ids at 1M plus the send) is 1-3 ms everywhere;
  the sort is the stall.
- The sort is memory-bound and the draw contends for the bus: at 1M the
  quiet first sort at mount takes 58-72 ms on the phone and 23-28 ms on
  the desktop, the same sort mid-orbit 50-95 and 76-121.
- At 0.3 rad/s the ~2 degree gate re-sorts 6-8 times a second, so at
  every rung a third to a half of the frames are re-sort frames; the
  phone's p95 sits at 4x its 11.1 ms period from 100k up. The stall is
  visible at every size, not only at 1M.

## Options

- The sort off the UI thread: a worker owning the key mirror, publishing
  the ids when done - the order then lags one flush, still ahead of a
  worker sort's several frames, and the frame never waits for it. What
  every CPU-sorting viewer in the field does.
- A cheaper sort: 16-bit depth keys over the cloud's bounds (two 8-bit
  passes instead of four over a quantized f32, the field's precision for
  back-to-front blending) and a scatter that fits the phone's caches.
- A gate that adapts to the population size, so a large cloud re-sorts
  less often; this trades order staleness for frames and does not remove
  the stall.

## Plan (2026-10-17)

Option 1 with the exact key kept: the sort leaves the UI thread, the
order stays what it is today.

1. `gpu/order_worker.rs`: a sort thread (`sol-order`, the glyph worker's
   shape: jobs in over a channel, results back over another, the thread
   joined on drop). A job carries the entry, the mirror order and stride,
   the key column (an `Arc`, shared with the registry's mirror), a spare
   permutation vector and the id buffer's pooled block; the worker sorts,
   materializes the ids into the block and calls the job's `done` (latch
   a frame request, wake the loop).
2. The registry: an index-materialized entry above SYNC_SORT_MAX_KEYS
   submits instead of sorting in place, one job in flight per entry and
   the newest direction wins (a change during a sort marks the entry
   wanted; the result's landing resubmits). The UI thread drains results
   once per frame (before the spatial tick) and at every re-sort entry
   point: a changed permutation sends the block as the id write and notes
   content; an unchanged one recycles it. A fresh id buffer gets the
   sentinel written before its first asynchronous seed, so nothing draws
   record 0 while the seed sorts. Small populations and the gathered forms
   keep the synchronous path: exact per-frame order where a sort costs
   less than a frame of lag.
3. `settle` counts a sort in flight as unsettled; /stats carries the
   worker's counters (pending, sorts, last and max ms).
4. The radix digit is 11 bits (three passes over the exact 32-bit key
   instead of four); 8 against 11 measured on the Pixel, the winner kept.
5. Tests: the worker's round trip in the alloy order tests; a 3d app test
   with a cloud above the threshold, settled between the camera flip and
   the paint.

## Outcome

Built as planned. The same ladder (Pixel 7, release, cadence hold off,
the 1M train cloud, orbit at 0.3 rad/s), read through the new /stats
counters (`orderSortMs` is the last landed job's sort plus
materialization; the frame window is the UI thread):

| rung | sort ms on the thread, median / max, 11-bit | 8-bit | frame p95 / max ms |
|---|---|---|---|
| 100k | 31.7 / 39.0 | 34.4 / 46.1 | 3.0 / 4.0 |
| 300k | 28.8 / 41.9 | 34.1 / 35.0 | 3.2 / 4.2 |
| 1M | 53.3 / 67.7 | 47.4 / 61.1 | 3.5 / 5.3 |

- The stall is gone: the frame p95 was 42 / 41 / 74 ms before, with a
  third to a half of all frames slow; now no frame is slow at any rung
  and the UI thread's p50 is under 2 ms. The desktop reads the same:
  p95 1.1 ms at 1M where it was 124.
- 8 against 11 bits is a wash on the phone (within noise either way) and
  a clear win for 11 on the desktop (1M on the thread: median about 65 ms
  against 98). 11 stays.
- The sort on the thread barely scales from 100k to 300k on the phone
  (29-34 ms either way), and 100k sorts slower there than it did in the
  flush (15 ms): the new thread lands on a slow core at a low clock
  while the frame threads hold display priority. The sort's duration is
  now lag, not stall, so this is a latency item:
  okf/backlog/sort-thread-core-placement.md.
- The probe `view` command no longer observes the re-order before its
  reply above the in-place size; a `settle` does.
