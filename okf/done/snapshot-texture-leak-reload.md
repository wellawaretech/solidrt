---
title: Snapshot boundary textures leak across dev reloads
description: get_gpu_resources on the SM-T500 listed 51 window-sized rgba8 "snapshot" textures (2000x1092, ~8.7 MB each, ~440 MB) after a session of reloads; get_stats' `textures` grows by one per reload. The old app instance's snapshot boundary (the demo's backdrop) is never freed when the next bundle is pushed.
created: 2026-09-22
completed: 2026-09-24
---

# Snapshot boundary textures leak across dev reloads

## Symptom

After a day of reload-on-save on demo/notes (one `repaintBoundary="snapshot"`
d-view under the window, exposed through `snapshotTexture`), the tablet's
`get_gpu_resources` lists textures with ids 1, 4, 7, ... 152, every one
`label: "snapshot"`, 2000x1092 rgba8: 51 of them, ~440 MB of GPU memory
on a 3 GB device that had 78 MB free and 1.1 GB of swap in use at the
time. get_stats' `textures` goes 57 -> 58 -> 59 across three reloads with
no app change. Only the newest is referenced (the frost passes sample id
152).

## What done looks like

- A reload disposes the previous instance's boundary caches and their
  snapshot textures (and whatever else `textures` counts) before the new
  bundle mounts; `textures` returns to the same number after a reload.
- A dev-time check: log a warning when a reload leaves textures from the
  previous generation alive.

## Done (2026-09-24)

Cause: a vended snapshot id (`RenderTree::snapshot_texture`) is queued for
release only when its node is destroyed; a reload drops the whole tree, so
nothing queued them and the registry entry (plus the raster-side mirror)
kept each boundary's window-sized texture alive, one more per reload. The
runtime-owned ("borrowed") ids left once an engine has exited are exactly
those: camera sessions and video textures close with the engine. So the
engine loop, between engines, releases every borrowed id
(`Context::release_all_borrowed`) and reclaims the deferred destroys
against an empty reference set - nothing paints between engines - so the
next engine starts from the registry the finished one found. The dev
check compares that count with the previous handover's and warns
"reload left N textures from the previous app alive" on growth.

Verified: a vended snapshot boundary shown through a `<texture>`,
`/stats` textures 1 before and after three reloads (used to grow by one
each), no warning logged.
