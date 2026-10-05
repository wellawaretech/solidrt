---
title: Publish extension writes before the frame paints, not at the microtask after it
description: A record mesh written from onFrame painted its new count over its old bytes, and every record, light, style and tile write made in a frame showed a frame late, because extensions published at a microtask that only runs after the frame's paint; core now has a before-render phase whose publish pass the scene and the 2d layers sync in.
created: 2026-10-06
completed: 2026-10-06
---

# Publish extension writes before the frame paints, not at the microtask after it

## Symptom

A record mesh rewritten from `onFrame` after its population had shrunk
drew the old records in the slots past the old count for one frame
([slate finch] 1). Reading the code gave two more faces of the same
thing: a write past capacity re-points the entry to its still-empty
replacement buffers at once, so that frame draws the whole population at
zero bytes; and every record write made in a frame shows one frame late.

## Cause

`setRecordCount` writes the draw count to the engine at once, while the
record bytes are marked dirty and published by the scene's sync, which
`_schedule` defers to a microtask. The frame's JS is one task in core's
`runFrame`: the `onFrame` callbacks, the reactive flush and
`renderFrame()`, whose paint and submit happen inside the call; flux's
microtask checkpoint runs after the whole task. Buffer writes and draw
updates are ordered raster commands, so the skew is deterministic: the
count is in frame N, the bytes in N+1. The engine's spatial tick flushes
transforms unconditionally before the paint (precisely to catch writes
whose microtask has not landed), so transforms were never affected; the
JS half of the sync - record bytes, lights, view params, cull groups -
had no such pass, and neither had the 2d layers' flushes (sprite layer
style and count, record layer records, tile bakes).

## Shape

- core `onBeforeRender(fn, options?)` in `packages/core/src/window.ts`:
  the plain handlers are the frame's late update, run once per frame
  after every `onFrame` callback and the flush (skipped on the bootstrap
  frame, like the callbacks), their own reactive writes flushed once they
  have all run, the way the post-layout handlers' are; `{ publish: true
  }` handlers run after that and again after the post-layout handlers'
  flush, the one other JS entry ahead of the paint. Each handler is its
  own boundary, like `onFrame`. The late pass exists so that an app's follow-the-pose work
  has a place after every callback, and so that the publish pass has
  something to be ordered after: an extension registered before the app
  would otherwise publish before the app's last write.
- The scene (`packages/3d/src/scene.ts`) and the three 2d layers
  (`layer.ts`, `records.ts`, `tiles.ts`) register a publish handler that
  runs their pending sync or flush, under no owner, unhooked by their
  own dispose. The microtask path stays for writes made outside a frame
  (an event handler, a timer, a debug command under a paused clock).
- Tests: `packages/3d/tests/before-render.test.tsx` (the stale-records
  and the growth cases) and `packages/2d/tests/before-render.test.tsx`
  (a sprite, a record sprite and a tile written from `onFrame`). All
  five failed on the previous code, on the predicted pixels, and pass
  now. Their probe, `app.painted(locator)` of `@solidrt/test`, came out
  of writing them ([frame-as-drawn-probe]).
- Follow-ups taken the same day: the camera controls' docs point a
  follow at the late pass; `sol check` bundles the CLI's own commands
  for bun and covers them from the repo root (the MCP command's check
  failed on the browser target before); the per-event syncs of writes
  made outside a frame are [outside-frame-sync-coalescing].

## Non-goals

- Coalescing the per-event syncs of writes made outside a frame (a
  pointer move handler moving a sprite still flushes per event). Unchanged.
- The count and buffer swap of a record mesh still reach the engine at
  once; with the publish pass they land in the same frame as the bytes,
  so moving them into the sync buys nothing.

## Findings

Moved to [frame-as-drawn-probe].

[slate finch]: ../feedback/slate-finch.md
[frame-as-drawn-probe]: ../notes/frame-as-drawn-probe.md
[outside-frame-sync-coalescing]: ../backlog/outside-frame-sync-coalescing.md
