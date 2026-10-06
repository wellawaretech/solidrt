---
title: A record-mesh caster that starts empty casts no shadow after a cold start on Android
description: On the SM-T500 the tower-toppling demo casts no shadows when the Player starts cold, while a reload of the same app, or any 3d scene run before it in the same process, shows them; the trigger is a shadow tile whose every caster had record count 0 at creation and got its records in the first frames, and nothing short of a new engine recovers it.
created: 2026-10-06
completed: 2026-10-06
---

# A record-mesh caster that starts empty casts no shadow after a cold start on Android

## Symptom

The tower-toppling demo on its own runtime, launched fresh on the SM-T500
tablet (Adreno 610, `am start -S` of the Player, the launcher engine then
the demo engine): the tower casts no shadow on the ground. The same app
reloaded in place (`/reload`, an engine swap) casts. The same app loaded
after any other 3d scene ran in that Player process casts. On Linux it
casts from a cold start. The GPU inventory is identical between the
broken and the working state apart from resource ids: the atlas, its
depth texture, the tile with its 288-instance caster draw, the receivers'
`uShadowAtlas` binding and light block.

## Reproduction

`target/scratch/shadow-probe3.tsx` in the checkout (gitignored; serve it
with `sol run target/scratch/shadow-probe3.tsx --project --lan` on the
derived Player, it imports `flux:physics3d` dynamically): one
`RecordMesh` caster of the demo's class with `count={0}` at mount, 24
bricks dropped by the physics world, poses published from an `onFrame`
without demand while bodies are awake. Cold start: no shadow. Judge by
picture, not by a floor-darkness metric: the bricks' shaded faces fool
the metric.

What was excluded on true cold starts, one at a time, in the demo's copy
or the probe: the HUD, the orbit camera and pointer feed, the balls
caster, fog, the ground size, the camera pose and lens, a fixed-size
scene, records present at mount (the demo's lagging count still makes
the first publish count 0), the physics tick as such (24 invisible
falling bricks with no record traffic cast fine), a 1024-record buffer
uploaded whole, publishing in tick-driven or callback-driven frames, the
world made outside the render root, group membership, a second caster of
the same class at count 0. In the broken state nothing in-process
recovers it: a camera move, a map re-layout, destroying and recreating
the shadow view, new receiver programs through fog and receiveShadow
toggles, a scene resize, remounting the caster under a freshly compiled
class.

What fixes it: a second `RecordMesh` caster, of any class, with records
and a non-zero count from its creation (the probe with a static ring of
12 cubes beside the dropped bricks casts on a cold start). So the failing
shape is a shadow tile whose every caster entry went 0 to N during the
first, slow frames of the first 3d engine in the process.

## Cause

A driver behaviour of the Adreno 610, written up in
[adreno-first-comparison-draw](../notes/adreno-first-comparison-draw.md):
the first program object in the process to draw through a
`sampler2DShadow` against a depth texture that was cleared and never drawn
into compares nothing for the rest of its life. With every caster at
count 0 during the first frames the atlas holds only its clear when the
scene's `phong()` program first samples it; that program is the first
comparison user in a cold process, so it is the one poisoned. A reload or
an earlier 3d scene makes another program take the hit, and a caster
with records from creation puts a draw into the atlas before the first
comparison. None of the engine's own state differed between the broken
and the working run: the raster inventory, the atlas depth, the
receivers' matrices and bindings were identical, and a new material in
the broken engine saw the shadow the ground's program could not.

## Fix

`gl::warm_compare_sampler` at raster start, on every platform: the
process's first comparison draw is a throwaway tap on a 1x1 cleared depth
texture. Verified by a cold start of the probe on the tablet with the
rebuilt derived Player; the demo needs no dummy caster. Setting the
comparison mode on the depth texture object was tried first and changed
nothing (the note has the reasoning).

## Trail

The two-hour bisect before the cause was found was confounded by
reload-on-save (every edited variant arrives as a hot reload, which hides
the bug: cold-start each variant) and by a floor-darkness metric that
cannot tell a shadow from a brick's shaded face. The repro
`target/scratch/shadow-probe3.tsx` stays out of the repo.
