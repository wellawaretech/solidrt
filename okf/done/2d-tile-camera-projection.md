---
title: The tile camera's world-to-screen mapping is not exported, and its rotation convention is stated nowhere
description: Anything drawn in world space over a rotating tile world (shadows, parallax motes, sprites until the sprite camera rotates) re-implements TileCamera's projection by hand from the component source, and the tiles example rotates a quarter turn off from the "heading renders upward" it claims.
created: 2026-08-29
---

# The tile camera's world-to-screen mapping is not exported, and its rotation convention is stated nowhere

What it looks like when you hit it: a rotating tile world with things that
must sit on it but are not tiles - island shadows, drifting haze, a ship
drawn as a sprite. Until [2d-sprite-camera-rotation](2d-sprite-camera-rotation.md)
lands, each of those is projected in JS, which means hand-copying the
layer's own mapping:

```ts
let dx = (worldX - camX) * zoom
let dy = (worldY - camY) * zoom
sx = pivotX + dx * ca - dy * sa
sy = pivotY + dx * sa + dy * ca
```

Eight lines, written once per consumer, each a copy of the `<view>`
transform in [components.tsx](../../packages/2d/src/components.tsx) that
silently rots if the layer's convention ever changes.

## Cause

The mapping exists only as element props on the world view (`originX/Y`,
`rotate`, `scale`, `x/y`); nothing in the package states it as a function.
The `TileCamera` doc says "rotated by `rotation` (radians, clockwise) about
that pivot" and stops there: which way a heading must be turned to render
upward is left to experiment, and the answer is `rotation = -heading - pi/2`
(y-down: a heading `h` has direction `(cos h, sin h)`; `R(rotation)` must
carry it onto `(0, -1)`).

[examples/tiles.tsx](../../packages/2d/examples/tiles.tsx) gets this wrong.
The path is `C + r(cos t, sin t)`, whose heading is `t + pi/2`, and it sets
`rotation: -(t + pi/2)` under a comment saying "forward renders upward".
That is `-heading`, which renders the heading pointing screen-RIGHT; upward
is `-t - pi`. The example is a quarter turn off from its own comment.

## Shape

- Export the projection and its inverse from the package, named for the
  tile camera: `projectTileCamera(camera, worldX, worldY): [x, y]` and the
  screen-to-world partner, both pure functions of a `TileCamera`. The
  component's `<view>` props and these functions must agree by
  construction: derive one from the other, or a differential check in
  `checks/` (project a world point, read the same point back from the
  tree) so they cannot drift.
- One sentence on `TileCamera`: "rotation turns the world clockwise (y-down)
  about the pivot; to render a heading `h` upward use `-h - pi/2`".
- Fix the example to `-(t + Math.PI)` (or spell it `-heading - Math.PI / 2`
  with `heading = t + Math.PI / 2`, which documents itself).

When the sprite camera gains rotation the same function is the one both
layers agree on, so this is not throwaway: the export is the vocabulary,
the in-shader rotation is a consumer of it.

## Findings

Landed 2026-08-31 (uncommitted), together with
[2d-sprite-camera-rotation](2d-sprite-camera-rotation.md) - which is why
the export is NOT named for the tile camera: the sprite camera gained
rotation in the same change, `CameraUpdate` became the one camera type
both layers share (`TileCamera` stays as an alias), and the functions are
`projectCamera` / `unprojectCamera` in the new pure module
`packages/2d/src/camera.ts` (no GPU imports, headless-checkable like
pick.ts).

- The convention sentence lives on the `CameraUpdate` type: rotation
  turns the world clockwise (y-down) about the pivot; a heading `h`
  renders upward at `rotation = -h - pi/2`.
- The example now spells it self-documentingly (`heading = t + pi/2`,
  `rotation: -heading - pi/2`) and was verified by snapshot: the road
  band renders vertically through the pivot, where the old code drew it
  horizontally.
- Drift guard: `checks/camera-check.ts` runs projectCamera against an
  oracle spelling the `<view>` props exactly as `<TileLayer>` composes
  them (origin at the camera point, rotate + scale there, translate onto
  the pivot), the unproject round trip, and the conventions as hand
  cases - 20k seeded sweeps, headless on flux. The shader side cannot be
  oracle-checked headless; the AGENTS.md trap ("touch one rotation,
  touch all") covers it, and the live probe for the sprite-camera item
  verified shader agreement pixel-exactly.
