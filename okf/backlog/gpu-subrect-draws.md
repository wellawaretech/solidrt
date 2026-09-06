---
title: A draw cannot name a sub-rectangle, so every render region costs a whole pass
description: run_pass sets one viewport for the whole pass, so N logical render regions means N targets and N passes. A pass costs 2.15 ms flat on an Adreno 610 regardless of size or content, which makes this a real budget item on mobile. Sub-targets (a draw target rendering into a rect of another's storage) unlock shadow atlases, cascades and multi-view rendering at one pass each.
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

**Cascaded shadow maps.** [3d-shadow-cascades.md](../done/3d-shadow-cascades.md)
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

## Shape: sub-targets, not per-entry viewports

The first proposal here was a viewport rectangle on `ResolvedDraw`. That is
the wrong granularity. A view is not "a draw with a rect": it is a rect plus
its own camera, its own clear, its own draw order and its own dirty flag.
The camera is the target's SHARED params today, and
[scene.ts](../../packages/3d/src/scene.ts) is explicit that this is
load-bearing ("one setTargetParams per camera move, not one write per
mesh"). A per-entry rect forces the camera per entry too, and every view
camera move becomes N mesh writes. The traps below (scissored clears,
per-tile dirty tracking) are group properties as well, not entry
properties.

So the unit is a **sub-target**: a draw target that renders into a rectangle
of another draw target's storage instead of owning storage of its own.

```ts
let atlas = createDrawTarget(1360, 1200, null, { depth: true })
let top = createDrawTarget(680, 600, null, { into: atlas, x: 0, y: 0 })
let bottom = createDrawTarget(680, 600, null, { into: atlas, x: 0, y: 600 })
```

- `top` and `bottom` are ordinary draw targets to every verb: `addDraw`,
  `setDrawParams`, `setTargetParams`, `setTargetTextures`, `setDrawOrder`,
  `setTargetSize`, `destroyTexture`. Each keeps its own shared params (its
  camera), clear color, entry list and dirty state. The JS view model does
  not change.
- `x`/`y` are top-left origin, the same space as the texture leaf's
  `srcX`/`srcY`, so the consumer is `<d-texture src={atlas} srcX={0}
  srcY={600} srcW={680} srcH={600} />` with no new prop.
- `setTargetRect(id, { x, y, width, height })` moves and resizes a tile;
  `setTargetSize` on a tile keeps its origin. A tile partly outside its
  parent is clipped (viewport and scissor do that for free), so a resize can
  update the parent and the tiles in any order.
- A sub-target id is not a texture: it cannot be sampled, displayed, read
  back or copied; those name the parent. `depthTexture(tile)` is the
  parent's too. One level only: `into` must name a target that owns
  storage.
- The parent is a flush-rendered (`render: "auto"`) draw target and so is
  every tile: no `render`, `loadOp` or `samples` on a tile (the parent's
  `samples` covers all of them), and `into` a manual parent is rejected. Not
  a permanent restriction, just nothing needs it.
- Destroying the parent destroys its tiles. Destroying a tile leaves its
  pixels until the parent next renders in full.

### In alloy

`PassDraw::Draws` becomes a list of **groups**, each `{ rect, clear, shared,
draws }`. `run_pass` sets the viewport and scissor per group, clears the
rect under the scissor (color and depth), and runs the group's entries with
the group's shared params and `iResolution`. Today's target is the one-group
case with no rect; the pass-level clear stays for the parent's own storage
and the scissor box joins the exhaustive save/restore set.

A tile is a `ShaderTexture` of its own in the raster registry (so every
per-target command routes unchanged) carrying a region marker: parent id and
origin. The parent renders the pass. `flush_dirty` treats a tile as a
source the parent samples: the edge graph gets `parent -> tile` so a dirty
tile makes the parent affected and orders after the tile's own sources; the
loop skips tile ids and, when it reaches a parent, renders one pass with:

- a **full** render (pass-level clear, the parent's own entries, then every
  tile) when the parent itself is dirty or one of its own sources is
  affected - a whole-target clear wipes clean tiles too, so they must all
  redraw;
- a **partial** render (no pass-level clear, only the affected tiles, each
  clearing its own rect) otherwise. Clean tiles keep their pixels. This is
  the per-tile dirty tracking; the purity invariant holds because a clean
  tile's pixels equal what re-rendering it would produce.

One pass either way: that is the whole point.

### Measured before stage 2 (2026-08-28, SM-T500)

A probe app (three 1024x1024 `depth: "texture"` targets, one cover
triangle each, depth animated so everything re-renders every frame; the
same three as tiles of one atlas) over a 17.4 ms empty frame, ms/frame:

| form | passes | ms/frame | over empty |
|---|---|---|---|
| three separate 1024x1024 | 3 | 24.3 | +6.9 (2.3 per pass) |
| one plain 2048x2048 target | 1 | 21.3 | +4.1 |
| one plain 3072x1024 target | 1 | 21.2 | +4.0 |
| atlas 2048x2048, 3 tiles, first cut | 1 | 29.8 | +12.3 |
| strip 3072x1024, 3 tiles, first cut | 1 | 27.9 | +10.5 |

So one large pass IS cheaper than the sum of small ones (the flat model
holds at these sizes), but the first-cut tile mechanics threw that away
twice over: a scissored `glClear` mid-pass ends the pass on the tiler and
restarts it with a full load/store of the whole surface, and a partial
(no-clear) render loads the whole surface once more. The fix in alloy:
tile wipes are a covering-triangle draw through a tile-clear program
(`gl_FragDepth = 1.0`), a parent with every tile changed renders in full
(fast clear instead of load), and a full render of a parent without
entries of its own skips tile wipes the pass clear already did.

After the fix, measured over a constant ballast load (a 2048x2048 target
with eight cover draws every frame, so the frame sits above vsync at ONE
GPU clock - without it the tablet's DVFS made the same probe read 2.3 ms
or 0.3 ms per pass depending on what ran before), ms/frame:

| form | passes | ms/frame | over ballast |
|---|---|---|---|
| ballast only | 1 | 20.6 | - |
| three separate 1024x1024 | 4 | 27.3 | +6.7 |
| atlas 2048x2048, 3 tiles | 2 | 23.4 | +2.8 |
| strip 3072x1024, 3 tiles | 2 | 23.5 | +2.9 |
| one plain 2048x2048 target | 2 | 23.75 | +3.2 |

The atlas costs exactly what one plain pass of its size costs; merging
three shadow passes into it saves ~3.9 ms on the Adreno 610 (about 2 ms
per pass removed). Real, and the precondition for cascades; on the demo's
70 ms frame it is a ~6% term, the shadow-map FILL is the bigger one.
Always measure with ballast on this device.

## Traps

- **Filtering across tile borders.** A PCF tap near a tile edge samples the
  neighbouring light's tile and shadows bleed between lights. Needs a gutter
  of unused texels and a clamp of the lookup to the tile rect. The existing
  out-of-frustum early-out in `SHADOW` happens in light-clip space before the
  map lookup, so it stays correct - it just has to map into atlas space
  after that check, not before.
- **Per-tile clearing needs scissor, not viewport.** Viewport transforms
  coordinates; it does not restrict `glClear`. Handled by the group clear
  above; the parent's pass-level clear must be skipped on a partial render
  or it wipes the clean tiles.
- **No Y flip.** Tile origins are top-left like `srcX`/`srcY`, and that IS
  the GL viewport origin for a target: a target's memory row 0 is its
  displayed top (the readback contract, and why meshes draw with y
  negated). The first cut flipped against the parent height and put every
  tile in the wrong half; the alloy example pins the placement.
- **Atlas size ceiling.** Three 1024 tiles need 2048x2048. More casters, or
  cascades, multiply that against `maxTextureSize`. Tile size has to become
  a function of the budget rather than a per-light constant.
- **All tiles share format, sampler state and MSAA.** Fine for shadow maps,
  which are already identical, but it constrains what else can share an
  atlas.

## Stages

1. **Sub-targets + multi-view.** Groups in `run_pass`, the region marker
   and parent-rendered pass in alloy, `into`/`x`/`y` on `createDrawTarget`
   and `setTargetRect` through flux, core and flux-types; `ViewOptions`
   takes `into`/`x`/`y` and a view gets `setRect`; the third-dimension
   demo's two side panels render into one atlas, sampled through
   `srcX`/`srcY`. Verified by an alloy example asserting pixels through
   readback, and by pass count on the tablet.
   Landed 2026-08-28 (uncommitted). On the demo (Linux, 3 casting lights)
   the frame went from 6 passes to 5: scene, 3 shadow maps, 1 atlas for
   both side views; `/gpu` shows the tiles with `into`, `x`, `y` and zero
   passes of their own. Tablet (SM-T500, Adreno 610, same client build,
   `/stats` deltas over 10 s, demo at 1320x1092 + three 1024 shadow maps):
   plain 6.0 passes/frame at 71.2 ms/frame, atlas 5.0 passes/frame at
   70.2 ms/frame (69.7-71.1 over three samples). About 1 ms, at the edge of
   the noise: this frame is fill-bound, and the 2.15 ms flat pass cost was
   measured on an otherwise empty frame with 128x128 targets. The saving is
   real but small next to the shadow-map fill, which is what stage 2's
   budget-sized tiles are for.
2. **Shadow atlas** in `@solidrt/3d`: tile allocation from a budget, one
   sampler plus `uShadowRect[N]`, gutter and clamp in `shadowAt`. Preceded
   by the 1024/2048 probe above.
   Landed 2026-08-28 (uncommitted): every casting light's map is a tile of
   one `<label>-shadow-atlas` target (grid of cells the largest `mapSize`
   wide, scaled uniformly against `maxTextureSize`); `SHADOW_SLOTS` is
   `uShadowAtlas` + `uShadowRect[N]`, `shadow(map, rect, coord, bias)`
   clamps every PCF tap to the tile inset by half a texel (no gutter
   needed), `shadowAt` lost its if-chain, `lightShadow` is unchanged for
   callers. Demo on the tablet: 5 -> 3 passes/frame, 68.6-69.5 -> 66.4-66.9
   ms/frame; the whole item took the demo from 6 passes at 71.2 ms to 3 at
   66.5 ms (~14.0 -> ~15.0 fps), the rest of that frame is fill.
3. Cascades ([3d-shadow-cascades.md](../done/3d-shadow-cascades.md)) on top.
   Landed 2026-08-28 (uncommitted, with the blend band and
   `shadow.distance`; split ratios stay open in that item): `shadow: { cascades: N }` makes a
   light N tiles of the same atlas, `SHADOW_SLOTS` went from light slots
   to MAX_SHADOW_MAPS map slots with `uShadowFirst`/`uShadowCount` per
   light, and `lightShadow` walks a light's maps tightest-first. Still
   one atlas pass; the example's frame is scene + atlas on the tablet at
   every cascade count (numbers in that item).

## What done looks like

A draw entry can name a sub-rectangle of its target; `@solidrt/3d` renders
all casting lights into one shadow atlas in one pass and samples it through
one sampler; and the pass count for a scene stops scaling with the number of
lights and views. Verified by pass count per frame and by wall-clock frame
time on the tablet, not by the GPU timers - see
[gpu-timer-attribution.md](gpu-timer-attribution.md) for why those cannot be
used here.
