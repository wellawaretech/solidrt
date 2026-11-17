---
title: Keep boundary caches under a scale written every frame
description: Under an ancestor scale that JS writes every frame, a snapshot boundary re-rasterizes every frame into a new texture (2.5 fps for 40 static 1200x800 snapshots, against 52.7 fps for the same writes on x), because its key holds the box snapped to the device grid and that box has a new logical size at every scale; text-layer-motion and snapshot-raster-composite-scale decided the opposite, and performance.md rule 5 promises even fewer re-rasters. Fixing the key alone does little for this repro - at its raster cost, a frame that passes the motion bound lasts long enough for the next frame to pass it again, through about half of the sweep.
created: 2026-11-16
---

# Keep boundary caches under a scale written every frame

## Symptom

An external report, against runtime 0.0.88 (no relevant rendering change
since). No text, no components, and nothing inside the boundaries
changes. 40 views of 1200x800, each `repaintBoundary="snapshot"` and
holding 200 laid-out views with a `<rect detached>` each, are placed
once with `left`/`top` under ONE container. `onFrame` writes the
container's `scale` (or its `x`) every frame. There are no input events.

```tsx
let [t, setT] = createSignal(0)
onFrame(tick => setT(tick / 1000))
<view position="absolute" left={0} top={0} originX={0} originY={0}
      scale={0.2 * Math.pow(2, Math.sin(t() * 2))}>
  <For each={tiles}>{p =>
    <view position="absolute" left={p.x} top={p.y} width={1200} height={800}>
      <view width={1200} height={800} flexDirection="row" flexWrap="wrap" gap={8}
            repaintBoundary="snapshot">
        <For each={cells}>{() => <view width={110} height={60}><rect detached color="#888" /></view>}</For>
      </view>
    </view>}
  </For>
</view>
```

The report measured the frame cadence in the app from the `onFrame` tick
gaps, on a desktop NVIDIA GPU with a release runtime:

| boundary       | motion    | fps  | avg frame |
|----------------|-----------|------|-----------|
| snapshot       | translate | 52.7 | 19 ms     |
| snapshot       | scale     | 2.5  | ~410 ms   |
| plain (`true`) | scale     | 20.6 | 48 ms     |
| none           | scale     | 22.3 | 45 ms     |

At rest the boundaries are reused (`nodesPainted` 43 of 24123). During
the scale animation `nodesPainted` is 24123 of 24123, and the paint
draws every rect. `pixelSnap={false}` on the container does not change
it, and neither does an extra plain view around each boundary. The same
setup with text inside the boundaries (a designSize box per tile, a
wheel zoom eased in `onFrame`) painted 1501 nodes and 237 text layers in
one 170 ms frame, and presented 10 frames in 700 ms.

## Why it is a bug

[text-layer-motion](../done/text-layer-motion.md) and
[snapshot-raster-composite-scale](../done/snapshot-raster-composite-scale.md)
decided this case: while the scale moves, the existing raster is
composited stretched. It is re-made at once only past
`LAYER_SCALE_MOTION_BOUND` (a ratio of 1.5), and once when the scale has
held for `SCALE_REST_LOOKS`. A per-frame write has no known end, so it
takes the hysteresis. The snapshot row breaks that rule: it re-rasters
every frame. A pan or zoom of a canvas of live UI, the case boundaries
are for, then costs more with snapshots than without them.

`packages/core/agents/performance.md` rule 5 promises app authors more
than the hysteresis: no re-raster under a scale animation, except once
at rest for "a boundary holding text". It does not mention the motion
bound. And every snapshot re-rasters once at rest at a new scale, with
or without text (`text::raster_density` runs for every snapshot). Rule 5
changes with this item (Done looks like).

The plain row is not a cache failure (Findings 4). It shows a different
limit, which this item does not own.

## Findings

Verified 2026-11-16 with a throwaway GL example over `paint_phase` (not
kept): the repro's shape at a smaller size (2 tiles of 20 cells, the
tiles at fractional left/top), 10 frames at rest, then the repro's sine
written once per frame at 60 Hz. The counters were the same at display
scale 1 and 1.5.

| boundary                    | motion | per frame while moving                                        |
|-----------------------------|--------|---------------------------------------------------------------|
| snapshot                    | scale  | nodesPainted 86/86, snapshotsRasterized 2, reused 0           |
| snapshot, `pixelSnap=false` | scale  | 4/86, reused 2; rasterized 2 once, at the motion bound        |
| snapshot                    | x      | 4/86, reused 2                                                |
| plain (Recording)           | scale  | 4/86, boundariesReused 2, recorded 0                          |
| none                        | scale  | 86/86                                                         |

1. **The damage is not the cause.** `View::set_scale_x`/`_y` report
   `Damage::Compose`, and `RenderTree::apply_damage` invalidates from the
   parent up. Nothing below the container loses its cache. Lead 2 of the
   first draft is ruled out.

2. **The snapshot key misses at every new scale.** The walk sizes a
   laid-out child with `grid::snap_box`: whole device pixels divided by
   the grid's `m11`. Under an ancestor scale, `m11` is new at every
   frame, so the snapped box has a new logical size at every frame. It is
   not "slightly different at some scales". It is a different float
   every time. `snapshot_node_unculled` keeps the cached box only while
   `ctx.grid.unrounded()` (a running native track, or `pixelSnap={false}`).
   A JS write is neither, so `SnapshotKey::matches` fails on `width`.
   The raster density itself is correct: `text::raster_density` answers
   `Wait` and keeps the old scale. Only the box misses. A write to `x`
   keeps `m11`, and `moved_own` rounds the container's translate to
   whole device pixels (`grid::snap_move`), so the snapped size stays
   the same. That is why translate is fast.

3. **Each miss allocates a new texture.** The in-place re-render
   (`retained`) needs the same `SnapshotKey`, so it fails too. Every
   moving frame creates a new texture and drops the old one
   (`snapshotsRasterized`, never `snapshotsRerendered`).

4. **The plain boundary keeps its cache.** A fill-only `<rect>` does not
   count toward `scale_bound` (`Rectangle::painted_stroke_width` counts
   visible strokes only), so the recording never re-records for scale.
   It replays at every frame. The first draft's "a plain boundary does
   not re-record" is already true. Plain costing what none costs is the
   replay: a recording saves the record walk on sol-ui, not the draw.
   The raster thread still draws every rect at every frame. Plain equal
   to none says that this frame is bound on the raster side (timing not
   verified here).

5. **`pixelSnap={false}` works in alloy.** With the flag on the
   container, the box is the layout size, and the snapshot is reused
   while it moves. It re-rasters once, where the scale ratio passes 1.5
   (the 19th moving frame at 60 Hz), as the rule says. The report's
   "`pixelSnap={false}` does not change it" did not reproduce. A likely
   cause, not verified: the bound sustains itself at a low frame rate.
   The first frame rasters all 40 tiles (about 0.4 s at the report's
   cost). Over 0.4 s the sine moves up to 0.82 octaves, a ratio of 1.76,
   past the bound. So the next frame re-rasters all 40 tiles again, and
   this continues until the sine slows down near its turn.

   `pixelSnap={false}` already does what the key fix would do: the box
   holds, and only the bound re-rasters. So the report's measurement is
   also the prediction for the key fix alone (Findings 6). Measure it
   again on a release client before the fix. If it still has no effect,
   check that the prop reaches the node (`Element::pixel_snap_off`).
   Then check for the loop: `snapshotsRasterized` 40 at every frame in
   the middle of each sweep, and none near the turns.

6. **One re-raster is expensive, and they come together.** Each snapshot
   raster is a blocking RPC from sol-ui to sol-raster
   (`Context::render_display_list_to_texture`, then `Context::rpc`), at
   4x MSAA, one after another. From the report: (410 - 45) / 40, about
   9 ms per tile. That cost includes an allocation in every raster
   (Findings 3). A raster at the motion bound allocates too: the new
   density gives new texture dimensions, so the in-place path does not
   apply.

   All tiles share one composite scale, so all of them pass the motion
   bound in the same frame. The repro sweeps a factor of 4 per half
   period (pi/2 s). log(4) / log(1.5) is 3.4, but each turn of the sine
   loses the remainder, so a half period has 2 or 3 crossings: 1.3 to
   1.9 per second, at the pace of a fast client.

   At the report's cost, a crossing makes a frame of about 410 ms. That
   frame is long enough to cross again. The sine moves at up to 2
   octaves per second, and the bound is 0.58 octaves. Over 410 ms the
   next frame passes the bound again wherever the sine moves faster than
   1.43 octaves per second: where cos(2t) > 0.71, about half of the
   cycle. There every frame re-rasters all 40 tiles, at today's 2.5 fps.
   This is the loop of Findings 5. Near the turns the sine covers only
   0.3 octaves, so it passes no bound there.

   **With only the key fixed, the repro runs at about its translate cost
   near the turns of the sine, and at about today's 2.5 fps through the
   middle of each sweep.** The key fix is still necessary: without it,
   every frame re-rasters, also in a slow or a narrow sweep. But for
   this repro, the decision below is the fix. It needs one of these:
   - A per-frame budget for the re-rasters past the bound. The others
     stay stretched for one more frame. The budget breaks the loop: a
     frame then costs at most the base plus the budget (45 ms + 3 x 9 ms
     for a budget of 3). The bound becomes a soft cap. That budget gives
     about 42 tile rasters per second, and the middle of this sweep needs
     about 137 to hold 1.5 (40 tiles at 3.4 crossings per second). So
     most tiles go well past 1.5 there. The budget re-rasters the most
     stretched first; with one shared scale, that is the oldest raster.
   - Make one raster cheaper. Option D of
     [repaint-boundary-freeze](repaint-boundary-freeze.md) works this
     out: a raster into existing storage can be sent without a wait,
     but a raster into a new texture cannot, and a raster at the bound
     is a new texture. D frees sol-ui, but the frame that composites
     the rasters still waits behind them on sol-raster. So D alone does
     not break the loop on screen.
   - Accept it, and say in rule 5 that a sweep wider than the bound
     re-rasters at every frame where it moves fast.

   Measure first, as repaint-boundary-freeze recommends: where a
   raster's time goes, the wall time of the RPC on sol-ui against
   `cmd_micros` on sol-raster. The 9 ms per tile comes from the report,
   not from a measurement here.

   The freeze item rejects a budget (its option B) because a budget
   changes the moment at which each tile freezes. That reason does not
   apply here: a tile over the budget only stays stretched for one more
   frame.

## Prior art: Flutter

From the Flutter engine sources (`flow/raster_cache`). Not checked
again against the current release.

- **The engine raster cache (Skia backend only).** The key is the layer
  or display-list id and the transform matrix without its translation.
  A new scale is a miss. Flutter never keeps a raster stretched under
  another scale. Two guards keep the misses cheap:
  - An access threshold of 3. An entry is rasterized only after 3
    consecutive frames at the same matrix. Under a scale written every
    frame, nothing is cached and the display list draws directly. That
    is our plain row. The cache fills 3 frames after the scale stops.
  - A per-frame budget of 3 new entries
    (`kDefaultPictureAndDisplayListCacheLimitPerFrame`). The others draw
    directly this frame and are cached on a later frame.

  A cached image draws at a whole-pixel translation. Layout boxes are
  not snapped, and no box size is in the key. Findings 2 has no
  counterpart there.
- **Impeller (the default on iOS and Android) has no raster cache.** A
  `RepaintBoundary` is a layer boundary only, and its picture replays at
  every frame. That is our Recording mode, at the cost of our plain row.
- **`SnapshotWidget` (the framework, used by the zoom page transition).**
  It rasterizes the child to an image once, in the child's own space at
  `devicePixelRatio`, and ignores the ancestor transforms. Under an
  ancestor scale the image stretches. It never re-rasterizes for scale:
  there is no motion bound and no re-raster at rest. It re-snapshots
  only when the child repaints.

What this means here:

- While the raster is stretched, the proposed fix below uses the
  `SnapshotWidget` key: the box the raster was made for and its density.
  The rounding of this frame's ancestor transform is not in it.
- The per-frame budget is the precedent for the first option in
  Findings 6. Ours can do better than a direct draw for the entries over
  the budget: they have an older raster, which stays stretched one more
  frame.
- Our hysteresis (stretch while moving, re-raster past 1.5, once at
  rest) comes from Chrome, not Flutter. Flutter either does not cache
  under a moving scale (raster cache) or stretches with no limit
  (`SnapshotWidget`). Our rule is between the two, which is why the
  crossings of the bound are a problem at all.

Recommendation: fix the key, then re-raster at most a budget of
snapshots per frame past the bound, the most stretched first. The others
stay stretched for one more frame.

## Proposed fix

Three parts. Parts 1 and 2 are decided and small, and they can land
before part 3 is decided.

### 1. The key keeps its box while the raster is stretched

Treat the snapped box as content made for one scale, like the density:
while the raster is composited stretched, the key keeps the box the
raster was made for. In `snapshot_node_unculled`, keep the cached box
when `(ctx.grid.unrounded() || wait) && snap.layout == layout`, where
`wait` is the second result of `text::raster_density`. This needs
`scale_observed` and `raster_density` before the box override. That is
possible: `snapped_own` reads `ctx.size`, not the box.

The guard is `wait`, not `!at_rest`. `at_rest` is also false for the 3
looks after a frame where the node was not visited (culled, for
example). There the density can be `Keep` (the same scale), and then
nothing requests a frame. If the snapped box changed in the meantime (a
reflow moved the boundary by a fraction while it was off screen), a
guard on `!at_rest` keeps the stale box until an unrelated paint. For a
JS write (`factor` 1), `wait` is true exactly while the raster is
stretched, and it already requests the next frame, so the look at rest
always comes. A native track stays on `unrounded()`.

When the density re-rasters (past the bound, or at rest), the key takes
this frame's box. If that is a new size at rest, the snapshot re-rasters
once, on the same look as the density re-raster.

### 2. The quad draws at this frame's box

The key's box and the quad's box become two boxes. The key's box is
what the texture holds: the cached box while the raster is stretched or
a track runs. The quad's box is this frame's: the snapped box on a
rounded grid, the layout box under a running track or
`pixelSnap={false}`. Today `BoundaryComposite::new` takes the quad from
the key (`dst` is `key.canvas()`). It takes this frame's box for `dst`
instead, grown by the outset, and keeps `src` from the key.

The reason: the cached box's far edge can be up to about one device
pixel of the old scale, times the scale ratio, away from where this
frame's box ends. That is up to 1.5 px at the bound. Boundaries that
touch (map tiles, cards with no gap) then show a seam or an overlap
while they move. At this frame's box, each edge meets its neighbour's
edge. The texture is stretched by less than one more device pixel, and
it is resampled anyway.

The native track path gets the same change. Today it keeps the cached
box under a track, and its quad draws there. With this change it draws
at the layout box.

### 3. The crossings (not decided)

As decided in Findings 6. Recommended: the per-frame budget, the most
stretched first.

`grid::snap_box` stays as it is. The rounding is correct at rest, and
[pixel-snap-scroll-and-js-motion](../backlog/pixel-snap-scroll-and-js-motion.md)
owns whether a JS-written value rounds at all.

## Review 2026-11-17

Findings 2 and 3 were checked against the code and hold. Parts 1 and 2
stand as written. Three additions, which are part of this item:

1. **The re-raster at rest takes the budget too.** When the scale holds,
   every snapshot re-rasters in the same frame (40 tiles, one hitch at
   the end of every zoom). Part 3 as first written covered only the
   crossings of the bound. The budget covers every re-raster for density
   of a snapshot that has a valid raster to stretch: past the bound, and
   at rest. A first raster and a raster for changed content are not
   budgeted (nothing can stand in for them).
2. **"Most stretched first" needs two frames.** The walk meets the
   candidates in tree order, so a plain counter gives the budget to the
   first tiles in the tree, and in a fast sweep the later tiles never
   get a turn. A node over the budget keeps its raster, notes its
   stretch ratio and requests the next frame. The next frame grants the
   most stretched of the noted nodes first. What is left of the budget
   goes in walk order.
3. **Known limit: a detached boundary sized by its inherited frame.** A
   detached boundary without a size of its own takes its box from the
   frame it inherits, which is the parent's snapped size. That size is
   new at every scale, so `snap.layout == layout` fails and part 1 does
   not keep its box. Not fixed here. A detached boundary with its own
   `w`/`h`, or a laid-out one, is not affected.

Not owned here: text layers outside a snapshot also pass the motion
bound in the same frame, with no budget.

The budget is built after the measurement of Findings 6, on a release
client with parts 1 and 2 in place.

## Corrections found while building parts 1 and 2 (2026-11-17)

Parts 1 and 2 as written were not enough. They left a seam under a
native scale track and after a content change while the raster is
stretched, an independent review found three more cases that still
re-rasterized, and a snapshot holding text paid for every raster made
in motion four times. These rules were added. The GL example
`alloy/examples/boundary_scale.rs` and `alloy/src/tests/boundary.rs`
hold them.

1. **The raster is laid out at the key's box.** The record walk laid the
   content out at this frame's box (`ctx.size`), not at the key's box. A
   raster made while the key keeps the cached box (a content change
   while stretched, the first frame of a scale track) then held content
   for one box in a canvas made for another, and its far edge came up
   short. For a laid-out node the record walk now gets the key's box as
   its frame. The quad side keeps this frame's box.
2. **A new raster under a running track takes a box of whole texels.**
   Under a track the box is the layout box, and `box * scale` is not
   whole texels: the last texel is partly covered, and the stretch shows
   it. The key keeps the cached box only while the cached density is
   kept too. A raster for a new density is new storage anyway, and under
   a track it takes this frame's box rounded to whole texels of its
   density. Not under `pixelSnap={false}`: there the box stays the
   layout box, mapped 1:1 as before. A whole-texel box would resample
   the interior for good, and a seam between fractional boxes is what
   `pixelSnap={false}` means.
3. **The key keeps its box when this frame's box needs the same
   texture.** With the layout and the density kept, the cached box
   stands when the grid is unrounded, when the raster is stretched
   (`wait`), or when this frame's box has the same whole-texel dims.
   Before, a scale that moved by less than `SCALE_HELD_EPSILON` per
   frame (the tail of an eased zoom, a slow drift) answered Keep, the
   key took this frame's `N / m11`, which differs from the cached box in
   the last bits, and every snapshot re-rasterized at every frame. The
   same compare made a raster for a track's end again on the frame the
   track settles.
4. **The texture dims tolerate float noise.** A box of 60.000004 texels
   was rounded up to 61, and the stretch sampled the empty column.
   `canvas_texels` takes a count within a tolerance above a whole number
   as that number (absolute `TEXEL_CEIL_EPSILON`, relative
   `TEXEL_CEIL_RELATIVE`, which must stay above `SCALE_HELD_EPSILON`),
   `src` never reaches past the texture, and
   `RenderTree::snapshot_texture` uses the same derivation.
5. **Inside a snapshot raster, the snapshot owns the look at rest.** A
   text made in a raster that is not for rest is unhinted. It used to
   ask for its own next look, and that look marked the snapshot stale:
   one raster and three re-renders per raster made in motion. Now a text
   or a nested recording inside a snapshot raster asks for nothing and
   counts as content for rest to re-make (`rest_pending`, on the
   `BuildContext` and on both caches). A snapshot that holds such
   content re-renders once, for rest, when its scale has rested, and a
   nested recording is recorded again there. A one-shot scale write
   costs one raster and one re-render.
6. **The stretch reaches the shaded outset and the inline fallback.**
   The outset in the quad is scaled by the ratio of the two boxes, so
   the content box lands on this frame's box, and the failed-raster
   fallback fills this frame's box.

Found on the way and fixed apart: `text::raster_density` returned a
zoom's end recomputed as `now * factor` past the motion bound. Callers
compare scales exactly, so a raster already made for the end was made
again on the next frames of the track (two more rasters per zoom at a
display scale of 1.5). A target that is the raster's own scale now keeps
the raster.

Known limits, not fixed:

- **Rest under a creeping scale.** A scale that moves by less than
  `SCALE_HELD_EPSILON` per frame is at rest at every frame, so the rest
  rule re-rasterizes each time it has moved one held step from the
  raster: three rasters at the end of an eased zoom (k = 0.2), one per
  five frames under a slow drift. The fix would be a second, wider
  tolerance for "the same scale" at rest, in the rule that text layers
  and recordings share.
- **Whole texels on a rounded grid.** A raster on a rounded grid whose
  density is not the grid its box was snapped on (a boundary with a
  scale of its own, a layout size change while stretched, a detached
  boundary under a track) has a partly covered last texel, which a
  stretch can show as a faint line at the far edge until the next raster
  for density.

Measured on a release client, desktop, the repro at 40 tiles, windows of
5 s: snapshot under the written scale went from 15.4 fps (avg 65 ms) to
58.4 fps, with one dropped frame at each crossing of the bound. The
`pixelSnap={false}` row before the fix was 59.2 fps: the prediction of
Findings 5 held, and the loop of Findings 6 does not form on this
machine, where one tile rasters in about 1 ms.

## Related

- [repaint-boundary-freeze](repaint-boundary-freeze.md): today a
  `repaintBoundary` write re-rasterizes a snapshot with nothing changed
  inside. With that fixed, an app can turn a boundary off while its
  content changes every frame, and on again for free. The same item adds
  a freeze that keeps the texture while the subtree changes, for a
  gesture over live content. Its option D and its measurement of a
  raster's time are the "one raster cheaper" option of Findings 6. Its
  budget (option B) is a different question: it is rejected there
  because it changes the moment at which each tile freezes.

## Done looks like

- Under a scale written every frame on a rounded grid, a snapshot
  boundary is reused while the scale moves. It re-rasters only where the
  motion bound says so, and once at rest. It never re-rasters per frame,
  and it never allocates a new texture per frame.
- The quad draws at this frame's box: the snapped box on a rounded grid,
  the layout box under a running track or `pixelSnap={false}`. Two
  snapshot boundaries that touch show no seam and no overlap while they
  move.
- The crossings of the motion bound are handled as decided (Findings 6).
  With the repro on a release client, the snapshot row runs at about its
  translate cost near the turns of the sine, and the middle of each
  sweep costs what was decided.
- `pixelSnap={false}` on the container is measured on a release client
  before the key fix, as the prediction for the key fix alone
  (Findings 5). After the fix, it keeps the snapshot cached there too.
- `packages/core/agents/performance.md` rule 5 states the motion bound,
  that every snapshot re-rasters once at rest at a new scale (with or
  without text), and what a sweep wider than the bound costs, as
  decided.
- Tests:
  - The snapshot half is a GL example beside
    `alloy/examples/boundary_transform.rs`. A lib test cannot hold it: its
    stand-in raster thread refuses every raster
    (`alloy/src/tests/yuv.rs`), so no snapshot cache forms. Under an
    ancestor scale written every frame: `snapshotsReused` while moving,
    one raster at the bound, one when the scale holds. Two snapshot
    boundaries that touch: a readback across their shared edge shows no
    seam while the scale moves, under a JS write and under a native
    scale track.
  - The recording half is a lib test in `alloy/src/tests/text_motion.rs`.
    A fill-only recording under an ancestor scale written every frame is
    replayed at every frame, and is not re-recorded when the scale
    holds. This is already true. The test pins it.
- The repro is kept as a probe under `probes/`, with `mode` and `report`
  debug commands to switch the boundary, the motion and the text.

## Involves

`alloy/src/rendertree/boundary.rs`: `snapshot_node_unculled` (the box
override, after `scale_observed` and `raster_density`, guarded on
`wait`) and `BoundaryComposite::new` (the quad at this frame's box,
apart from the key). Also `grid::snap_box` as the cause (not to
change), `text::raster_density` and `LAYER_SCALE_MOTION_BOUND` for the
crossings, the snapshot raster RPC in `alloy/src/context/capture.rs` if
one raster is made cheaper, and rule 5 in
`packages/core/agents/performance.md`. The damage (`kinds/view.rs`) and
the `BoundaryMode::Recording` arm in `composite.rs` are not involved.
The fix must not wait for
[pixel-snap-scroll-and-js-motion](../backlog/pixel-snap-scroll-and-js-motion.md):
whatever the rounding, a static boundary under a moving ancestor scale
keeps its cache.
