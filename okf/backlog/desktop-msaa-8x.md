---
title: Desktop MSAA at 8x
description: Edges are quantised to 4 coverage levels (4x MSAA on every rig rasterization; Impeller GL has no analytic AA), which reads as slight jaggies on thick diagonal strokes at 1x; desktop could run the offscreen rig at 8x (one constant, already clamped to GL_MAX_SAMPLES) while Android must stay at 4x. Parked 2026-08-27.
created: 2026-08-27
---

# Desktop MSAA at 8x

Observed 2026-08-27 on a 1x desktop display: solid 8 px yellow polyline
strokes (line-points example) look "a little bit jagged". A scale-1
capture of the tile holds exactly five colours: background, the stroke, and
coverage at 25%, 50% and 75%. That is the 4x MSAA every rig rasterization
runs at (`MSAA_SAMPLES = 4`, alloy/src/gl/rig.rs); Impeller's GL backend has
no analytic path anti-aliasing and relies on the multisampled target, so
an edge pixel takes at most four intermediate values. It applies to fills,
solid strokes and dashes alike (the dash walker's curve pieces are not
involved, see [path-dashing](../done/path-dashing.md)).

Lever: the sample count. On desktop the window is single-sample and each
frame rasterizes into the multisampled offscreen rig, resolved into FBO 0
(alloy/src/gl/context.rs, `render_display_list_to_window`); the rig already
clamps to `GL_MAX_SAMPLES` (alloy/src/gl/draw.rs). Going to 8x is one
constant for desktop: eight coverage levels, at the price of doubling the
rig's multisample storage and fill bandwidth for a window-sized target
(the resolve stays one pass). Android must keep 4x: its backbuffer is
multisampled in-tile at swap and 4x is the count that class of GPU runs at
full rate ([android-surface-swap-latency](../done/android-surface-swap-latency.md)),
so the constant becomes per-platform.

Parked by the user on 2026-08-27 ("leave it"). If picked up: split the
constant (desktop 8, Android 4), rebuild, judge by eye on a 1x display and
read the paint cost from `/stats`; revert the line if not worth it.
Analytic coverage AA (Skia quality) would need a different rasterizer and
is out of scope.
