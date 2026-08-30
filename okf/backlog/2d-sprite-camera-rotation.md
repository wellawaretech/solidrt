---
title: The sprite layer's camera cannot rotate, so sprites cannot ride a rotating world
description: TileCamera rotates the baked world about a pivot, but the sprite layer's uCamera is offset + zoom only, so the ship and enemies drawn as sprites over a rotating tile map cannot follow - rotation must go in-shader with the inverse in pick and the handlers, keeping one camera vocabulary across both layers.
created: 2026-08-24
---

# The sprite layer's camera cannot rotate, so sprites cannot ride a rotating world

What it looks like when you hit it: a tile world rotating under a ship (the
`TileCamera` rotation + pivot that landed with baked-layers stage B1), with
the ship and everything moving drawn as sprites - and no way to point the
sprite layer's camera the same way. `setCamera` takes offset + zoom;
`uCamera` is a vec4; the vertex stage does `(world - cam) * zoom` and
nothing else.

Rotating the sprite layer's OUTPUT leaf instead is wrong: the layer renders
into a viewport-sized texture with the camera already applied, so rotating
the composited leaf spins the cropped viewport and the corners cut. The
tile layer gets away with the transform-on-the-leaf trick only because its
output is the WORLD, not a viewport of it.

## Shape

- `CameraUpdate` grows `rotation?` (radians, clockwise) and the pivot pair,
  one vocabulary across both layers so one signal drives a whole rotating
  scene; the tile layer's `TileCamera` becomes the same type.
- The shared vertex stage (packages/2d/src/shaders.ts) applies the rotation
  in-shader: a cos/sin uniform pair alongside `uCamera`, screen =
  pivot + R * zoom * (world - cam). The tile layer's bake targets pin
  rotation to identity and pivot to the chunk origin - the chunk-rect
  mapping is unchanged.
- Every inverse mapping follows the shader: `handlersFor`'s screen-to-world
  (`toLayer`), so events keep arriving in world pixels, and anything else
  that undoes the camera. `pick` itself already works in world space and
  needs nothing, but the shader/pick rotation-agreement trap applies to the
  camera rotation direction too - one differential check should cover the
  round trip (screen -> world -> pick under a rotated camera).

Small and well-contained, but it touches the shader + inverse pair, which
is why it did not ride along with stage B1 silently.

Until this lands, the interim for world-space things over a rotating tile
world is projecting in JS - which today means copying the tile camera's
mapping by hand; [2d-tile-camera-projection](2d-tile-camera-projection.md)
exports it so the copy goes away, and is the vocabulary this item's
in-shader rotation then consumes.
