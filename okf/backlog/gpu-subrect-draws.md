---
title: A draw cannot name a sub-rectangle, so every render region costs a whole pass
description: run_pass sets one viewport for the whole pass, so N logical render regions means N targets and N passes. A pass costs 2.15 ms flat on an Adreno 610 regardless of size or content, which makes this a real budget item on mobile. An optional per-entry viewport unlocks shadow atlases, cascades and multi-view rendering at one pass each.
created: 2026-08-27
---

# A draw cannot name a sub-rectangle, so every render region costs a whole pass

[alloy/src/gpu/pass.rs](../../alloy/src/gpu/pass.rs) sets the viewport once,
in `run_pass`, and every entry in a `PassDraw::Draws` list inherits it.
`ResolvedDraw` carries its own program, pipeline descriptor, VAO, draw range,
params and inputs - everything except where on the target it lands. So two
things that want to render into different parts of one texture cannot share
a pass, and each becomes its own target with its own pass.

## Why it matters now

A render pass is not free on a tiled mobile GPU. Measured on an Adreno 610
(Samsung SM-T500) by arming N 128x128 targets whose fragment shader writes a
constant, with nothing else in the frame:

| live passes | 0 | 1 | 2 | 3 | 4 | 5 | 6 |
|---|---|---|---|---|---|---|---|
| frame ms | 17.51 | 20.74 | 22.72 | 25.20 | 27.89 | 28.40 | 30.41 |

**~2.15 ms per pass, flat** - independent of target size and of what the
shader does. That is a tile load/store cycle, and it is spent before a pass
draws anything. Full measurement context in
[3d-low-end-gpu-performance.md](3d-low-end-gpu-performance.md).

Two existing backlog items already hit this wall from different directions
without being able to price it. [2d-atlas-limits.md](2d-atlas-limits.md) puts
it plainly: "a layer is not a draw, it is a full-size offscreen texture, its
own render pass, and its own composited `<texture>` leaf in the tree. What
should cost one extra draw call costs a second canvas." That item's own fix
is a texture-binding problem rather than a viewport one, so this does not
subsume it - but both are the same complaint about the same missing
capability, and now there is a number attached.

## Consumers

**Shadow atlas.** `createShadow` in
[packages/3d/src/scene.ts](../../packages/3d/src/scene.ts) builds one square
depth target per casting light. Three casters is three targets and three
passes. With sub-rect draws they become tiles of one atlas texture rendered
in one pass. The second win is larger than the pass saving:
[packages/3d/src/glsl.ts](../../packages/3d/src/glsl.ts) declares
`uShadowMap0..N-1`, one `sampler2D` per light slot whether it casts or not,
and `shadowAt` picks between them with an if-chain because GLSL ES 3.00
cannot index a sampler array with a non-constant. An atlas collapses that to
one sampler plus a `uniform vec4 uShadowRect[N]` of tile offsets and scales -
no branch, no per-light binding, fewer bound samplers and better occupancy.

**Cascaded shadow maps.** [3d-shadow-cascades.md](3d-shadow-cascades.md)
splits each light's frustum into N maps. Per-light targets make that N x
lights passes. Over an atlas it stays one pass and one sampler, so this is
closer to a prerequisite for cascades than an optimisation of them.

**Multi-view.** A `scene.createView` panel is its own target and its own
pass. The third-dimension demo's two side panels are two 680x600 targets
costing two passes to produce one screen. As two viewports into one texture
that is one pass. This half is reachable end to end: the texture leaf
already takes `srcX`/`srcY`/`srcW`/`srcH`
([packages/core/src/types.d.ts](../../packages/core/src/types.d.ts)), so the
consuming `<d-texture>` can sample a tile without any new prop.

For the demo that is six passes down to three - scene, one atlas shadow
pass, one combined view pass - about 6.5 ms, at no cost in visual quality.

## Proposed shape

An optional viewport rectangle on `ResolvedDraw`: `None` keeps exactly
today's behaviour (inherit the pass viewport), `Some(rect)` sets
`gl.viewport` for that entry. The entry loop in `run_pass` already switches
program, VAO, uniforms and pipeline state per entry, so this is one more
piece of per-entry state, and the existing exhaustive save/restore around
the pass keeps Impeller safe on the shared context.

Then the target layer needs a way to express "this view renders into a tile
of that texture" rather than "this view owns a texture", and `@solidrt/3d`
needs to allocate tiles instead of targets.

## Traps

- **Filtering across tile borders.** A PCF tap near a tile edge samples the
  neighbouring light's tile and shadows bleed between lights. Needs a gutter
  of unused texels and a clamp of the lookup to the tile rect. The existing
  out-of-frustum early-out in `SHADOW` happens in light-clip space before the
  map lookup, so it stays correct - it just has to map into atlas space
  after that check, not before.
- **Per-tile clearing needs scissor, not viewport.** Viewport transforms
  coordinates; it does not restrict `glClear`. Today each shadow view clears
  its own target and can be skipped independently when its light has not
  moved (`shadow.dirty`). One atlas pass with one clear loses that: either
  every tile is redrawn every frame, or a tile is cleared under a scissor
  rect before its draws. The per-entry state probably wants both a viewport
  and an optional scissored clear, and the dirty-tracking wants designing
  alongside it rather than after.
- **Atlas size ceiling.** Three 1024 tiles need 2048x2048. More casters, or
  cascades, multiply that against `maxTextureSize`. Tile size has to become
  a function of the budget rather than a per-light constant.
- **All tiles share format and sampler state.** Fine for shadow maps, which
  are already identical, but it constrains what else can share an atlas.

## What done looks like

A draw entry can name a sub-rectangle of its target; `@solidrt/3d` renders
all casting lights into one shadow atlas in one pass and samples it through
one sampler; and the pass count for a scene stops scaling with the number of
lights and views. Verified by pass count per frame and by wall-clock frame
time on the tablet, not by the GPU timers - see
[gpu-timer-attribution.md](gpu-timer-attribution.md) for why those cannot be
used here.
