---
title: How to observe a frame exactly as it was drawn
description: A texture readback renders a dirty target before reading and a test's pixels() paints afresh, so neither sees a frame as its own paint drew it; a capture requested from inside the frame does, and so does /snapshot?step=1 under a paused clock.
created: 2026-10-06
---

# How to observe a frame exactly as it was drawn

Found while pinning [before-render-phase] with a test: the first probe,
`readTexture(scene.texture)` right after `app.frame()`, passed on the
broken code. The readback is an ordered raster command behind the late
buffer write, and the write marks the scene target dirty, so the readback
renders the target again before reading and returns the write wherever
it landed. A locator's `pixels()` (the test host's `capture()`) paints
the tree afresh through `render_now`, after the microtasks, for the same
reason.

What does observe the frame: a capture requested from inside the frame.
`captureSnapshot(node)` called from an `onFrame` callback after the write
is serviced by that frame's own paint, which samples every target as it
stands when the frame renders; the capture's promise settles with the
frame after, so a test runs two frames and then reads the pixels. The
dev tools' form is `/snapshot?node=<id>&step=1` (`get_snapshot` with
`step: true`) under a paused clock, which queues the capture and one step
together on the JS thread, so the stepped frame's paint services it
([snapshot-stepped-frame]).

A tile chunk is the exception that proves the rule: its bake is an
explicit `renderTarget` command, not a dirty mark, so a readback after
the frame sees the bake wherever it was queued, while the in-frame
capture sees the chunk the paint sampled.

[before-render-phase]: ../done/before-render-phase.md
[snapshot-stepped-frame]: ../done/snapshot-stepped-frame.md
