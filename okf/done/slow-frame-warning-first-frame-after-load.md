---
title: The slow-frame warning fires on the first frame after every load
description: Fixed 2026-09-23: the engine's first rebuild is tagged in its slow-frame line ("first frame after load: the first rebuild shapes, decodes and records everything, not steady-state jank"), kept rather than exempted so a load whose first frame takes 300 ms is still seen; the line covers the JS thread's phases, so a load whose cost is on the raster thread gets none.
created: 2026-09-07
completed: 2026-09-23
---

# The slow-frame warning fires on the first frame after every load

## Symptom

Every bundle push, the first frame of the fresh app logs `Slow frame:
27.3 ms (budget 16.7): js 0.8, layout 0.4, postLayout 0.1, paint 26.0,
...`. The app is not slow: that frame uploads the atlas, compiles what
needs compiling and rasters everything once. The line is correct and
useless, and after the third reload the reader has learned to skip past
"Slow frame", which is the one warning that should stay loud.

## Cause

`lattice/src/plugins/draw.rs` judges every frame against its refresh
period, throttled to one warning per `SLOW_WARN_INTERVAL`. Nothing there
knows a frame is the first of a fresh engine. The line does carry
`nodesAdded` so a mount frame's build cost can be told from steady-state
jank, but a texture upload adds no nodes: the reporter's case was 26
nodes and 26 ms of paint.

## Done looks like

The mount frame is either tagged or exempt:

- Tag: the same line with a `(first frame after load)` suffix, so the
  log still records the cost and the reader can dismiss it in one glance.
  Preferred - it keeps the number, and a load whose first frame takes 300
  ms is still worth seeing.
- Exempt: skip the warning for the first frame of an engine.

Either way the signal is already there: `RenderInner` (the draw loop's
per-engine state) is rebuilt by `store_state` on every engine build, so
its first frame is exactly "the first frame after a load"; a
`frames_seen` cell or an `is_none()` check on `last_node_count`-style
state is the whole detection. Bytecode one-shots and player launches go
through the same path and get the same tag.

## What it involves

`draw.rs` only: one cell on `RenderInner`, one branch in the warning
format. No wire or CLI change; `get_logs` shows the tagged line as is.

## Fixed (2026-09-23)

The tag, as preferred: a `first_frame_done` cell on `RenderInner`
(lattice/src/plugins/draw.rs) flips on the engine's first rebuild, and a
slow-frame line for that frame carries the "first frame after load"
cause the way a capture-stalled frame carries its own. `RenderInner` is
rebuilt per engine, so every load and reload gets exactly one tagged
frame; debugging.md says what the tag means.

## Seen end to end (2026-09-24)

The engine logger's lines do reach `get_logs`: a tablet frame stalled by
a window snapshot arrived as its tagged slow-frame line. The first-frame
tag itself was not seen on either client because their first rebuilds
were under budget on the JS thread, where the line measures: the load's
uploads and compiles are raster-thread work outside its phases. The
wording now names what that rebuild does on the JS thread (shaping,
decoding, recording) instead of the raster side, and debugging.md says a
first frame with no line is normal.
