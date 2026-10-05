---
title: Slate finch
description: Issues extracted from a feedback report on an agent-built app around record meshes written per frame from a worker, dated 2026-10-05, against 0.0.67.
created: 2026-10-06
---

# Slate finch

Issues extracted from a feedback report on an agent-built app (report dated
2026-10-05, ingested 2026-10-06, core/3d/cli 0.0.67). Background and
evidence: the raw report, pointer in the private feedback-sources mapping.
Numbering follows the report's own.

## Bugs

1. A record mesh's draw count reaches the engine the moment
   `setRecordCount` runs, but its record bytes only at the scene's sync, a
   microtask after the frame's JS. `runFrame` (core/src/window.ts) runs the
   frame callbacks, the flush and `renderFrame()` in one task, and the
   paint and submit happen inside that call, so a `setRecords` made in
   `onFrame` paints with the new count over the previous bytes. Three
   faces of it: after a shrink, a regrowth draws the old records in the
   slots past the old count for one frame (seen on screen); growth past
   capacity re-points the entry to the still-empty replacement buffers at
   once (`growInstances`, mesh.ts), so that frame draws the whole
   population as zero-size records; and every record write made in a
   frame shows one frame late. The engine's own spatial tick flushes
   transforms unconditionally before the paint, so transforms are not
   affected; the JS half of the sync (record bytes, lights, view params,
   cull groups) has no pre-paint pass. The 2d layers batch their publish
   to a microtask the same way (records.ts, tiles.ts, layer.ts).
   Workaround used: draw only the count the previous publish uploaded.
   [done 2026-10-06, okf/done/before-render-phase.md]

## Diagnostics

2. A snapshot (`/snapshot`, `get_snapshot`) captures a paint it requests
   itself, which comes after the request's own microtask checkpoint; under
   a paused clock the request forces that paint. So a glitch confined to
   the frame as its own code drew it (item 1) cannot be captured, even
   with `step_frames`. "Register the capture, then step" is not
   expressible: the request latches a frame first. Ask: a capture of the
   next stepped frame exactly as drawn.
   [done 2026-10-06, okf/done/snapshot-stepped-frame.md]

## Tooling

3. Moving a project from `file:` links to a checkout back to published
   versions fails `bun install` on the stale lockfile. [not extracted:
   judged a non-issue]
