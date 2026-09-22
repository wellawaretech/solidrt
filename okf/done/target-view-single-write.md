---
title: One view write per target per camera move
description: The 3d scene wrote each target's frustum and LOD view as two crossings derived in JS from one camera; now one setView(target, view, proj) per target per move, the core deriving frustum, LOD view and sort view, with a per-target LOD bias and an LOD reference that makes shadow tiles measure by the scene camera.
created: 2026-09-22
---

# One view write per target per camera move

Split from the draw sort follow-ups (2026-09-22), whose other items
landed the same day: the cutout queue, the bind-time key, the pinned
background, and the state-change grouping question, closed by
measurement in [draw-state-change-grouping](../notes/draw-state-change-grouping.md).

## The problem

On every camera move the scene (packages/3d/src/scene.ts, `sync`) writes
to each target: the target params (`cameraParams`, a `setTargetParams`
call), the frustum (`spatial.setFrustum`, a view-proj matrix) and the
LOD view (`spatial.setLodView`: eye, forward, focal, ortho, bias). Three
crossings per target per move, and the same for every view with its own
camera, all derived from one camera object. The core's draw sort and LOD
pass read the LOD view; culling reads the frustum; the shaders read the
params.

## What landed (2026-09-22)

`Spatial::set_view(target, Option<(view, proj)>)` replaces `set_frustum`
and `set_lod_view`: the core derives the frustum (proj x view), the LOD
view (eye from the view's inverse translation, forward from its negated
z row, focal from proj[5], ortho from proj[15]) and the sort view from
the two matrices. `set_lod_bias(target, bias)` keeps the bias apart, so
it survives view writes; `set_lod_reference(target, source)` makes a
target measure LOD by another's view, only the measurement: culling and
the draw sort stay the target's own. `set_view(None)` forgets all three.
The plugin exports `setView`, `setLodBias`, `setLodReference`.

The scene's two helpers and scratch arrays collapsed into one `setView`
per camera move; a shadow view sets its LOD reference to the scene
target at creation and never writes an LOD view again (the `lodViewSet`
flag is gone); every other view writes its own bias at creation, and
`scene.setLodBias` writes it to each. Probe faces call `setView` per
face, so each face now culls its own frustum, where before a probe kept
the stale frustum of its creation-time camera. The cull pass processes
touched nodes first, in touch order, then the rest of the sinks on
targets whose frustum moved, so a LOD hand-over still writes its
outgoing level before the incoming one when the camera moved in the same
flush.

Crossings per camera move: two per target (camera params, view) instead
of three; one per shadow tile instead of two. The camera params the
shaders read stay a target param write on purpose: they are the 3d
package's uniform names, which alloy must not learn (the `iResolution`
auto-fill is a Shadertoy convention, not ours).

Verified with `probes/3d-view-write-probe.tsx` on a release client: a
hero LOD under the scene camera, a shadow tile, a minimap view and a
reflection probe, the drawn level of each read from `/gpu` after camera
parks at 5 and 30 units, a bias of 0.2, and a probe bake: the tile
follows the scene camera and its bias, the minimap picks by its own
camera, the probe's last face culls the hero below it.

What the design compared against: Three derives everything from the
camera object per render call, Unity from the Camera component with
shadow passes reusing the camera's cull result, Godot from a camera
resource attached to a viewport with shadow LOD measured from the main
camera. The per-target stored view is Godot's model; the LOD reference
is what all three do for shadows.
