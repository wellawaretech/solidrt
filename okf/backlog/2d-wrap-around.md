---
title: Wrap-around worlds in 2d - recipes exist but live nowhere, and a wrapping tile world has no seam answer
description: A toroidal world (Asteroids screen wrap, an endlessly repeating tile map) is expressible today only by app-side recipes nothing documents - position modulo that must SNAP (native transitions animate the wrap jump across the world), ghost copies at the seams - and the chunked tile layer has no way to draw the seam at all.
created: 2026-08-31
---

# Wrap-around worlds in 2d

What it looks like when you hit it: an Asteroids-like where the ship exits
right and enters left, or a world map that scrolls east forever and comes
back to where it started. Nothing supports or documents the topology; every
piece is an app-side recipe, one of them with a real trap, and one case has
no good recipe at all.

Engine comparison: Unity, Godot and Three all leave ENTITY wrap to the
game too - that part staying app-side is parity, not a gap. What they also
all have is repeat-addressed texture sampling for backgrounds, which we
have on the gpu face and lack on the rendertree face
([texture-tile-mode](texture-tile-mode.md)).

## The three cases

**Repeating background - solved, modulo one backlog item.** A quad
sampling the background with `wrap: "repeat"` (SamplerOptions, or the
per-binding `{ id, wrap }` override) and a camera-driven UV offset: the
GPU does the wrap, no seam exists. Already shipped on the gpu face; the
`<texture>` element form is [texture-tile-mode](texture-tile-mode.md).

**Entity wrap (sprites) - app recipes, worth documenting.**

- Position modulo `((x % W) + W) % W` per step. The TRAP: this composes
  with per-frame JS motion and the records layer, but NOT with native
  transitions - a declared position transition treats the wrap write as a
  target and animates the jump clear across the world. There is no
  "teleport" write on a transitioned sprite today. A wrapping entity is
  therefore inherently a JS-motion (or later physics) workload; if demand
  shows for wrap + springs, the primitive to consider is a snap write
  that bypasses a declared transition (spatial core vocabulary, not 2d).
- Seam continuity: a sprite straddling an edge must be DRAWN twice (up to
  four times in a corner) - ghost copies, the classic answer. Fully
  expressible: park permanent ghosts off-camera and place them when the
  primary is within half a sprite of an edge. An in-shader modulo could
  never replace this (a quad crossing the seam needs a second draw, not a
  moved one), so ghosts are the right mechanism at any layer; at most a
  package helper could own the bookkeeping if apps keep rewriting it.

**Wrapping tile world - the unsupported case.** The tile layer composites
world-positioned chunk leaves inside one camera-transformed view, so a
camera modulo leaves a hole at the seam. A world that fits one texture can
be drawn twice (two leaves offset by the world width); a CHUNKED world has
no recipe - it wants either duplicated seam-adjacent chunk leaves or
modular chunk addressing in a camera-anchored composite. That composite is
exactly the shape the unbounded-world extension already sketches
([2d-tile-world-bounds](../done/2d-tile-world-bounds.md) kept it additive,
landing with [2d-baked-layers](2d-baked-layers.md) stage B2's residency) -
a torus is "unbounded with modular coordinates", so wrap should be decided
WITH that work, not bolted on after.

## Shape

Staged, smallest first:

1. **Docs only**: the three recipes above (repeat background, modulo +
   ghost copies, the transition-snap trap) in packages/2d/AGENTS.md or a
   docs page - so the topology is at least stated, with its costs.
2. **If demand shows**: a ghost-copy helper on the sprite layer
   (bookkeeping only, no new rendering), and the snap-write question for
   transitioned sprites raised against the spatial core.
3. **Tile wrap**: folded into the unbounded-world/B2 design as a modular
   coordinate mode of the camera-anchored composite - not designed here.
