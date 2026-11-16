---
title: Keep boundary caches under a scale written every frame
description: Under an ancestor scale that JS writes every frame, a snapshot boundary re-rasterizes every frame into a new texture (2.5 fps for 40 static 1200x800 snapshots, against 52.7 fps for the same writes on x), because its key holds the box snapped to the device grid and that box has a new logical size at every scale; text-layer-motion and snapshot-raster-composite-scale decided the opposite and performance.md rule 5 promises it. Fixing the key alone is not enough for a wide sweep - every crossing of the motion bound re-rasters every snapshot in one frame.
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
takes the hysteresis. `packages/core/agents/performance.md` rule 5
promises the same to app authors. The snapshot row breaks that rule: it
re-rasters every frame. A pan or zoom of a canvas of live UI, the case
boundaries are for, then costs more with snapshots than without them.

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
   keeps `m11`, and so the snapped size, which is why translate is fast.

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
   this continues until the sine turns. Measure it again in the runtime
   after the fix. If it still has no effect, check that the prop reaches
   the node (`Element::pixel_snap_off`).

6. **One re-raster is expensive, and they come together.** Each snapshot
   raster is a blocking RPC from sol-ui to sol-raster
   (`Context::render_display_list_to_texture`, then `Context::rpc`), at
   4x MSAA, one after another. From the report: (410 - 45) / 40, about
   9 ms per tile. All tiles share one composite scale, so all of them
   pass the motion bound in the same frame. The repro sweeps a factor
   of 4: log(4) / log(1.5) = 3.4 crossings per half period (pi/2 s),
   about 4.4 per second. At the report's cost each crossing is a hitch of
   about 365 ms. **With the key fixed, the snapshot row still does not
   reach its translate cost for this sweep.** That needs one of these
   decisions:
   - Spread the re-rasters past the bound over several frames, with a
     per-frame budget. The others stay stretched for one more frame.
   - Make one raster cheaper: no blocking round trip per boundary.
   - Accept the hitches, and tell the app author in rule 5 that a
     sweep wider than the bound re-rasters.

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

- The proposed fix below is the `SnapshotWidget` key: the box in the
  node's own space and the raster density. The rounding of the ancestor
  transform is not in it.
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
snapshots per frame past the bound, the oldest raster first. The others
stay stretched for one more frame.

## Proposed fix (not decided)

Treat the snapped box as content made for one scale, like the density:
while the composite scale moves, the box follows the layout size. In
`snapshot_node_unculled`, keep the cached box when
`(ctx.grid.unrounded() || !at_rest) && snap.layout == layout`. This
needs `scale_observed` and `at_rest` before the box override. That is
possible: `snapped_own` reads `ctx.size`, not the box. At rest the box
snaps again. If it is a new size, the snapshot re-rasters once, on the
same look as the density re-raster. While it moves, the quad draws at
the cached box, up to half a device pixel off the grid. It is resampled
anyway.

`grid::snap_box` stays as it is. The rounding is correct at rest, and
[pixel-snap-scroll-and-js-motion](pixel-snap-scroll-and-js-motion.md)
owns whether a JS-written value rounds at all.

## Remaining leads

- Seen during the same runs: the stats endpoint returned frozen values
  while this ran. File it separately if it reproduces.

## Related

- [repaint-boundary-freeze](repaint-boundary-freeze.md): today a
  `repaintBoundary` write re-rasterizes a snapshot with nothing changed
  inside. With that fixed, an app can turn a boundary off while its
  content changes every frame, and on again for free. The same item adds
  a freeze that keeps the texture while the subtree changes, for a
  gesture over live content. How many freezes in one frame are rastered
  is open there; its option D is the "one raster cheaper" option of
  Findings 6.

## Done looks like

- Under a scale written every frame on a rounded grid, a snapshot
  boundary is reused while the scale moves. It re-rasters only where the
  motion bound says so, and once at rest. It never re-rasters per frame,
  and it never allocates a new texture per frame.
- The crossings of the motion bound are handled as decided (Findings 6).
  With the repro on a release client, the snapshot row runs at about its
  translate cost between crossings, and the crossings cost what was
  decided.
- `pixelSnap={false}` on the container is measured again in the runtime
  and keeps the snapshot cached there too.
- Tests:
  - The snapshot half is a GL example beside
    `alloy/examples/boundary_transform.rs`. A lib test cannot hold it: its
    stand-in raster thread refuses every raster (`tests/yuv.rs`), so no
    snapshot cache forms. Under an ancestor scale written every frame:
    `snapshotsReused` while moving, one raster at the bound, one when the
    scale holds.
  - The recording half is a lib test in `alloy/src/tests/text_motion.rs`.
    A fill-only recording under an ancestor scale written every frame is
    replayed at every frame, and is not re-recorded when the scale
    holds. This is already true. The test pins it.
- The repro is kept as a probe under `probes/`, with `mode` and `report`
  debug commands to switch the boundary, the motion and the text.

## Involves

`alloy/src/rendertree/boundary.rs` (`snapshot_node_unculled`: the box
override and its order against `scale_observed`), `grid::snap_box` as
the cause (not to change), `text::raster_density` and
`LAYER_SCALE_MOTION_BOUND` for the crossings, and the snapshot raster
RPC in `alloy/src/context/capture.rs` if one raster is made cheaper. The
damage (`kinds/view.rs`) and the `BoundaryMode::Recording` arm in
`composite.rs` are not involved. The fix must not wait for
[pixel-snap-scroll-and-js-motion](pixel-snap-scroll-and-js-motion.md):
whatever the rounding, a static boundary under a moving ancestor scale
keeps its cache.
