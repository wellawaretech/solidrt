---
title: Coalesce the per-event syncs of writes made outside a frame
description: A pointer move handler that moves a sprite or a mesh flushes its layer or scene once per dispatched move, because every event is its own JS entry with its own microtask checkpoint; the frame's publish pass coalesces only what the frame's JS writes, so a drag at a high event rate publishes many times per frame.
created: 2026-10-06
---

# Coalesce the per-event syncs of writes made outside a frame

## Symptom

Since [before-render-phase], a write made in the frame's JS is published
once, in the publish pass ahead of the paint. A write made outside it is
published at the microtask after the JS entry that made it: a pointer
move handler, a timer, a debug command. Batched pointer moves are
dispatched one entry per move before the render event, so a drag that
moves a sprite on every move flushes the layer on every move - N buffer
writes and N `spatial.flush()` calls per frame instead of one - and a
scene written from a move handler syncs the same way.

## What done looks like

A drag that writes a layer or a scene per move publishes once per frame,
with no change to what the handlers observe (write-then-query in one
handler stays coherent: the queries flush pending writes themselves).

## What it involves

- Measure first: syncs per frame under a synthetic drag (`send_input`
  drag at the default move rate) in an app whose move handler writes a
  sprite or a mesh transform; the cost of the extra publishes at a few
  thousand sprites. The stats overlay's setProperty count and the raster
  command rate say what the publishes cost.
- Then either dispatch the batched moves inside the frame's JS task
  (between the timers and the frame callbacks), so the publish pass
  covers them and nothing is scheduled per move, or give the schedulers a
  "frame imminent" signal from core that `_schedule` reads to skip the
  microtask when a frame will publish anyway. The first changes when move
  handlers run relative to timers and rAF; the second keeps the event
  timing and adds one flag.
- The microtask path must stay for writes made when no frame is coming
  (a paused clock paints through `render_now` without JS).

[before-render-phase]: ../done/before-render-phase.md
