---
title: Screen-space sizing for sprites (min-px floors, constant-size markers)
description: Markers that must stay legible at any zoom (selection rings, map pins, traffic dots) have no shader-side answer, so apps rewrite w/h from JS on every camera change - per frame for record sprites - even though the camera is already a uniform in the vertex stage.
created: 2026-09-02
completed: 2026-10-05
---

# Screen-space sizing for sprites (min-px floors, constant-size markers)

## Symptom

A zoomable world always carries a few sprites whose size is about the
SCREEN, not the world: a selection ring that must stay findable at the
zoomed-out overview, a map pin, a drag handle, a traffic dot that should
never fall under two device pixels. Today size is world pixels scaled by
the camera, full stop, so the app claws the floor back from JS:

- A canvas app resizes its selection and hover rings on every camera
  apply (`minPx / zoom`): two setSprite calls, but easy to forget and
  wrong for one frame if applied out of order with the camera write.
- Its pulses are record sprites, so the floor means rewriting w/h for
  every live pulse every frame, turning a positions-only write loop into
  a positions-plus-size loop.

The vertex stage already has the camera (`uCamera`, zoom in .z/.w), so
the divide the app performs per sprite per frame is one shader
instruction away from free.

## Shape

Per-sprite, opt-in, both layer kinds share the vertex stage so one
mechanism covers both:

- Minimal: a min-screen-px field - drawn size =
  `max(iSize * zoom, minPx)` (per axis, preserving aspect). Covers every
  case met so far; zero cost when 0.
- Fuller: a size-space flag (world | screen), where screen-space sprites
  ignore zoom entirely - the constant-size gizmo. The min-px field is
  the blend of the two and may be the better single knob.

Record layout note: the 13-float record has no free slot, so the record
layer either grows the record (breaking FLOATS_PER_SPRITE consumers) or
takes a per-layer uniform floor (`layer.setMinScreenPx(n)`) - the
per-layer spelling would have served both uses above and avoids the
layout change. The node layer's style record has room for a per-sprite
field if wanted (style floats are not slot-constrained the same way).

Picking must agree with drawing: a min-px-floored sprite that draws 15px
should hit-test at 15px, so pointInSprite/pick need the same clamp -
that is the real cost of the feature, and the reason it belongs in the
package rather than in app code (rings that pick at their world size
today, subtly wrong at overview zoom).

Comparison: Three ships this as `Sprite` (screen-facing) plus
`sizeAttenuation: false` on sprites/points - the exact world/screen
split; Unity's gizmos and canvas-space UI are constant-size by
definition and world markers use billboards with constant-screen-size
scripts; Godot has no built-in and its forums carry the same
divide-by-zoom workaround this item removes.

## Open questions

- Per-sprite field vs per-layer uniform floor - or both, uniform first
  (cheap, no layout change) and per-sprite when a use case demands
  mixing floors in one layer?
- Does a floored sprite's rotation stay world-space (yes, presumably) and
  does the floor apply before or after group scale?

## Landed

`minScreenPx` on every sprite (SpriteOptions, `<Sprite>`), both layer
kinds: the vertex stages scale the quad by `max(1, minScreenPx / (min(w,
h) * zoom))` over the core-written pose, so the floor comes after group
scale and a size transition, rotation stays world-space, and a collapsed
sprite stays collapsed. Per sprite, not per layer: a canvas mixes rings
and nodes in one layer, and the record pulses need it too, so the record
grew to 14 floats and the style record to 10 (the new float appended;
demos writing records by offset keep their offsets and get a zero floor).

Picking agrees: `layer.pick(x, y, zoom)` takes the zoom the floors are
measured at (default 1), `view.pick(x, y)` and the pointer walk pass the
view camera's. The live layer keeps the raycast while no sprite has a
floor; with one, the candidates are the index columns within the widest
floor's reach of the point (`floorReach` over the floored sprites, exact,
recomputed lazily after one leaves or shrinks) tested exactly against the
drawn rect from the node's world matrix. `pickRect`, overlap,
sweep and the mover stay world-space body queries by design - the floor
is a view-space drawing rule, not a body.

Open questions answered: per-sprite field (the uniform floor would not
have served a layer mixing floored markers and plain sprites); rotation
world-space; floor after group scale. Pinned by tests/screen-floor.test.tsx
(pixels of a view at three zooms, pick on both layers through a scaled
group, view.pick) and tests/pick.test.ts (floorReach as a bound against
pointInSprite over random floored sprites). Landed together with stage 1
of [2d-materials-and-blend](../backlog/2d-materials-and-blend.md), the
`blend` option, since both touch the one pipeline builder.

## The clamp, later the same day

The floor became a clamp, after the survey of what the engines do
(Three, Godot 3D, Unreal and Cesium ship a world-or-screen mode flag;
deck.gl and Cesium's distance curves ship pixel bounds; the 2d engines
ship nothing): `maxScreenPx` beside `minScreenPx`, both bounds on the
smaller axis as one uniform scale. No bounds is world size, a floor
alone the overview marker, a ceiling alone the label that must not
balloon, equal bounds the constant size (Three's `sizeAttenuation:
false`, `w`/`h` then the aspect alone). The clamp model subsumes the
mode flag and adds the one-sided cases the map and canvas apps need.
The records grew once more (15 floats, the style record 11).

The math moved to core (`packages/core/src/shaders.ts`, the pure shader
helpers beside the `glsl` tag, exported from `@solidrt/core/gpu`):
`SCREEN_SIZE_GLSL`, its JS twin
`screenSizeScale`, and `checkScreenSize` for the pair - one definition
both sprite renderers paste and both pickers apply, with one
differential test. The 2d pick's candidate search learned that the
core's box overlap is a SURFACE contact (a box inside a big column
finds nothing), so the ray's hits stay in the union with the box's.

@solidrt/3d's sprite material took the same vocabulary:
`sprite({ minScreenPx, maxScreenPx })`, the per-entry `uScreenPx`,
the vertex stage deriving the pixels per unit at the quad's depth from
the shared uniforms alone (the center and the center one camera-up unit
away, projected: right under perspective and orthographic cameras), and
`scene.pick`/`view.pick` replacing the index's unit-box hits on clamped
sprites with an exact ray-quad test at the drawn size. A floored sprite
mesh switches its frustum culling off, one way. Pinned by
packages/3d/tests/sprite-clamp.test.tsx (painted width at three camera
distances, perspective and orthographic, pick hits and misses, the hit's
distance and normal, fixed-y, the culling switch, validation).
