---
title: Switch and freeze a snapshot boundary
description: An app turns a snapshot boundary off while its subtree is live and freezes it for a gesture over live content (a zoomed-out overview of live tiles, a page transition, a dragged card); a repaintBoundary write reports Compose so a valid texture survives the switch, an off boundary releases its texture after a release age, a freeze prop keeps the texture while the subtree changes (a write inside stops at the frozen node as a stale mark and notes no damage), the unfreeze repaints only what changed, and snapshot-no-aa splits into its own antialias prop.
created: 2026-11-16
---

# Switch and freeze a snapshot boundary

## Problem

A snapshot boundary is for static content. Content that changes every
frame re-rasterizes every frame, and each raster is a blocking RPC from
sol-ui to sol-raster. Two things an app wants are missing:

- **Switch.** Turn the boundary off while the subtree is live, and on
  again when it is static. Today the switch costs a raster every time.
  The `repaintBoundary` write reports `Damage::Paint`
  (`flux/src/alloy_plugins/properties/mod.rs`), which marks the node's
  own snapshot stale: snapshot, off, snapshot re-rasterizes, also when
  nothing inside changed. A boundary turned off keeps its
  `SnapshotCache` until the node is removed or `drop_paint_caches` runs.
- **Freeze.** Some gestures want a fixed cost instead of a live picture:
  zoom out over live tiles, a page transition, a heavy card dragged. The
  app wants to say "show this as it is now, until I say otherwise". There
  is no way to say that today.

The validity is already correct while the mode is off: the
`invalidate_paint` walk passes every ancestor whatever its mode, and
marks an old snapshot stale when content below it changes. Only the mode
write throws it away.

The two are used together. A live tile is off while it is live, and
frozen for the gesture:

```tsx
<view repaintBoundary={live() ? false : "snapshot"} freeze={!live()}>
```

Stage 1 (the switch) is useful alone and can ship first. Stage 2 (the
freeze) depends on it.

## The props

One prop for each choice, so a new choice adds a prop and does not
multiply the values:

| prop              | values                        | says                                       |
|-------------------|-------------------------------|--------------------------------------------|
| `repaintBoundary` | `false`, `true`, `"snapshot"` | what is kept: nothing, a recording, pixels |
| `antialias`       | `true` (default), `false`     | how a snapshot rasterizes (4x MSAA or not) |
| `freeze`          | `false` (default), `true`     | keep the snapshot while the subtree changes |

`antialias` is the web canvas name. A frozen recording is not offered: a
recording refers to live textures (text layers, images) that can be made
again or released while it is frozen. A snapshot holds its own pixels.

**Off a snapshot boundary, `antialias` and `freeze` are inert.** They are
fields on `Element`, read only by the snapshot arm. They do not throw.
JSX writes props one at a time, so a throw would depend on the write
order (`freeze` before `repaintBoundary`), and the switch above turns
the mode off with the other props still set. A `freeze={true}` off a
snapshot warns once per write, at paint, as a `shader` declared without
a snapshot does (the `take_shader_dirty` warning in
`alloy/src/rendertree/composite.rs`). A value of the wrong type throws as
usual.

`repaintBoundary="snapshot-no-aa"` goes away for
`repaintBoundary="snapshot" antialias={false}`. One value packed two
choices, named one of them by what it lacks, and would have needed a
`"freeze-no-aa"` next. `BoundaryMode::SnapshotNoAa` goes away with it:
the mode says what is kept, and the `antialias` field says how. An
`antialias` write reports `Damage::Paint`. The raster changes and the
storage does not (an in-place re-render takes `aa`, `RasterizeDlInto`),
so the stale mark is enough and `SnapshotKey` does not take the setting.
With `SnapshotNoAa` as a mode and the mode write at Compose, the key
would have needed it.

The rename touches:

- `packages/core/src/types.d.ts` (the `repaintBoundary` and `shader`
  docs), `packages/core/docs/shaders.md`,
  `packages/core/examples/README.md`
- `flux/src/alloy_plugins/properties/mod.rs` and `read.rs`,
  `flux/src/tests/properties.rs`
- the `SnapshotNoAa` matches in `alloy/src/rendertree/cull.rs`
  (`is_backdrop_root`), `composite.rs` (the shader warning and the mode
  dispatch) and four in `tree/mod.rs` (`snapshot_texture`,
  `boundary_stand_in`, `texture_content_changed`, `content_reach`)
- the test `snapshot_no_aa_counts_as_bake` in `alloy/src/tests/tree.rs`
- a comment in `alloy/src/raster/cmd.rs`
- `okf/notes/offscreen-msaa-provenance.md`, which says what the value
  exists for

## Stage 1: the switch

- The mode write reports `Damage::Compose`: the ancestors repaint, the
  node is noted for the screen damage, and the node's own cache is kept.
  Snapshot, off, snapshot with nothing changed inside reuses the texture.
  A change inside while off re-rasterizes once at the switch back, into
  the old storage when the box and density match.
- Recording to snapshot and back keeps working: each arm already ignores
  a cache of the other kind and makes its own.
- An off boundary releases its snapshot storage after a release age,
  valid or stale, as a text layer is released (`TEXT_LAYER_RELEASE_FRAMES`).
  Stale storage is worth keeping: at the same box and density, the next
  raster re-renders into it and does not allocate. A switch that comes
  right back finds it.
- The release is a sweep after the paint walk, over a registry of the
  off boundaries that hold a cache, as `release_stale_text_layers` does
  for text layers. It is not a check in the `BoundaryMode::None` arm of
  `composite.rs`: after the walk that the mode write causes, an off
  boundary under a replayed recording, or culled, is not walked again.
- A freeze after a live period longer than the release age allocates its
  storage again. The open question below measures that case.

## Stage 2: the freeze

### When the picture is taken

`freeze={true}` takes effect at the node's first snapshot paint after
the write. That paint applies the snapshot rules of today: it reuses the
snapshot when it is valid and its key matches the box and density of
this frame, and rasterizes once otherwise. From then on the picture is
held (a `held` bit that the paint sets, and that a `freeze` or a mode
write clears).

- The picture is of the state at the end of the batch that froze it,
  whatever the order of the writes in that batch. The freeze write
  reports `Damage::Compose`, so the node is painted in that frame.
- A node that is not painted when it freezes (culled, offscreen) is
  rasterized when it is first painted. Its picture is of that moment, at
  the density of that moment. In a zoom-out, the tiles that the zoom
  shows freeze at the zoomed-out density.
- A snapshot whose key no longer matches is not reused, so "unfreeze and
  freeze again" makes a texture sharp again.
- Many freezes in one frame: open, see below.

### The stale mark

The stale mark is `SnapshotCache::valid`. The walk already clears it
(`invalidate_paint_step`). The freeze changes who reads it, not what it
means. There is no second bit.

### Writes inside

- The `invalidate_paint` walk runs as today from the written node up.
  Every cache between the written node and the frozen node is cleared,
  so nothing below is wrongly left marked valid.
- A walk that reaches the frozen node from below clears `valid` and
  stops. The ancestors keep their caches. The walk leaves the node's
  envelope as it is, because the frozen quad still covers what it
  covered. The walk does not put the node in the visited set of
  `invalidate_paint_batched` (see the box change below).
- The walk stops at any node with `freeze` set, held or not. Before the
  first paint, a stop costs nothing: the freeze write's own damage makes
  that paint happen.

### The frozen node's own writes

- **Compose** (transform, opacity, a drag by `x`/`y`): as today. The quad
  moves, and the walk starts at the parent.
- **Paint and Scroll** (its background, its scroll offset, `antialias`):
  the stale mark only. The raster holds them.
- **A change of its own box** (a sibling that grows, a written `width`,
  a written `left`/`top`): the layout pass notes the node and clears
  from it up (`set_unrounded_layout` in
  `alloy/src/rendertree/layout/context.rs`). That clear goes past the
  frozen node to the ancestors, because the quad moves or stretches and
  an ancestor recording still holds the old place. Taffy writes the
  children's boxes before the node's own box. A stop that put the frozen
  node in `paint_cleared` would end this clear at once. That is why the
  stop leaves the visited set alone.
- **Its own boundary shader and its own backdrop** stay live. They are
  composited over the frozen source, as on a valid snapshot today. A
  shader input change re-runs the pass (`shader_hits` in
  `texture_content_changed`). A shader write that needs other storage
  rasterizes the live subtree once, and the picture moves to that
  moment. That is an outset change, a `previous` toggle, or a shader
  withdrawn with an outset (which drops the cache in
  `snapshot_node_unculled`). Documented, not avoided.

### Screen damage

A change inside notes no damage. Every damage path ends in
`RenderTree::note_damage`, but the sites pass different nodes, so the
check cannot be "the noted node has a frozen ancestor":

- `apply_damage` and `apply_damage_batch` note the written node, before
  the walk.
- The structural sites (insert, detach, destroy) note the parent. For a
  direct child of the frozen node, that is the frozen node itself.
- The layout pass notes each node whose box changed.
- `texture_content_changed` notes `reach.node`, the outermost boundary
  above the texture (`content_reach`), which is the frozen node or one
  above it. Or it notes an area (`note_area`). A video or camera feed in
  a frozen live tile would damage the whole tile at every video frame.

The walk decides. A site notes its damage only when its walk did not
stop at a frozen node. The visited set of `invalidate_paint_batched`
keeps that outcome for each node, so a walk that ends early at a visited
node takes the answer from there. `content_reach` reports a frozen node
on its way, and then the content hit is dropped. The bake walk sets the
stale mark as today.

The damage is not noted at all. It is not noted and then dropped at the
resolve: the ledger turns a frame of more than `MAX_DAMAGED_NODES`
noted nodes into full damage (`damage.rs`). `boundary_stand_in` is not
enough either. Standing the frozen node in would still damage its area
at every write and repaint it, with nothing new to show. The frozen node
is a leaf for the partial repaint.

### Scale and size

A held texture never re-rasterizes: no motion bound, no re-raster at
rest, none when its own box resizes, and none at a `pixelSnap` change
above it. It stretches to the box. In `snapshot_node_unculled` the hold
skips three checks: `valid` with `box_key.matches` on the plain path, the
same pair on the shaded path, and the `pixel_snap_off` stale mark. The
quad is built from the current box, and the texture comes from the
cache.

### Unfreeze

The write reports:

- `Damage::Paint` on the frozen node when the snapshot is stale. The
  partial repaint takes its old extent cell (the frozen quad) and its
  new one. The new cell holds the overflow of the live subtree, because
  `cull::envelope` does not crop a snapshot: a safe superset of the
  raster, which crops at the box. Only that subtree is walked and
  rasterized again.
- `Damage::Compose` otherwise. The walk then applies the snapshot rules
  of today: the texture is reused when its key matches. When the box or
  density moved during the freeze, it re-rasterizes as those rules say
  (at once past the motion bound, else once at rest). Without a damage,
  the stretched texture would stay until something else walks the node,
  and would then re-rasterize at a moment that looks random.

The decision reads the node's paint cache, so it is an `Element` method
in alloy that returns the damage. The props arm forwards to it, as a
thin plugin layer does.

### Other paths

- **Layout and hit testing** keep running inside the frozen subtree. The
  extents are then current at the unfreeze. Taps go to the live targets,
  which need not match the picture. That is the app's choice, with the
  prop it already has: `pointerEvents="all"` on the frozen view catches
  them as one target, `pointerEvents="none"` passes them through.
- **Captures** (`captureSnapshot`) of a node inside a frozen subtree show
  the live subtree, not the frozen picture: they are isolated walks
  (`service_captures_under_cache`).
- **Context loss.** `drop_paint_caches` drops the frozen texture with
  every other cache. The next paint rasterizes the current content, and
  holds that picture.

## Open: many freezes in one frame

N boundaries freeze in one frame. Each one without a valid snapshot
needs a raster, and a raster is a blocking RPC from sol-ui to
sol-raster. At the cost the scale report implies (about 9 ms per tile),
40 tiles are a hitch of about 365 ms. Live tiles that were off for
longer than the release age (stage 1) have no storage left, so each one
also allocates.

- **A. All in that frame.** One hitch, and all tiles show the same
  moment. Simple, and the app sees what it asked for.
- **B. A per-frame budget, the others live until their turn.** No
  hitch, but the tiles freeze at moments a few frames apart. That bends
  what "freeze" means.
- **C. Record all in that frame, rasterize over the next frames.** The
  record walk is cheap (sol-ui, about the cost of no boundary). A tile
  replays its recording until its raster lands, within a per-frame
  budget. One moment, no hitch. For those frames the recording refers
  to live textures (text layers, images), so a text that changes then
  shows through.
- **D. Do not wait on sol-raster.** Possible for half of the rasters:
  - Into existing storage: yes. `RasterizeDlInto` returns only a
    `Result<(), String>`, so it can be sent without a wait, as
    `RerunNodeShader` already is (a failure warns and the old pixels
    stay). The raster loop applies the commands between frames in
    order, also when it drops a superseded frame, so the raster runs
    before the frame that composites it. No `glFinish` is needed: one
    context (`alloy/src/gl/draw.rs`). This covers a freeze within the
    release age of a live period, and every unfreeze of a stale node.
  - Into a new texture: no. The display list takes an Impeller
    `Texture` when sol-ui builds it (`draw_texture_rect`), and that
    object is made on sol-raster (`adopt_texture`). A vended texture id
    does not help: the registry maps it to that same object on the UI
    side. A first raster, a freeze after a long live period, and a
    raster that grows the texture need one. Two ways to make that
    smaller: one batched allocation RPC per frame for all new textures
    (one wait, not N), and storage that is kept when a raster is
    smaller (the quad already samples only the top-left
    `canvas * scale` region of a padded texture), so that only growth
    allocates.
  - What D does not do: the frame that composites the rasters still
    waits behind them on sol-raster. The GPU work is already
    asynchronous, so what is left there is Impeller's and the driver's
    CPU time per raster. D frees sol-ui (input, JS), and makes the
    hitch on screen shorter only by the part of the cost that was the
    wait.

  This is the "one raster cheaper" option of
  [boundary-cache-under-js-written-scale](boundary-cache-under-js-written-scale.md),
  and it helps every snapshot.

Recommended: A for the meaning, then measure before any machinery. The
9 ms comes from the scale bug, where every raster also allocated a new
texture. Measure where a raster's time goes: the wall time of the RPC on
sol-ui against `cmd_micros` (the raster stats' time for non-frame
commands on sol-raster). The difference is the wait, which D removes.
The rest is work, which only C hides. B is not recommended.

## Review 2026-11-17

The design was checked against the code and holds. Five additions, which
are part of this item:

Stage 1:

1. **A texture content change while off.** "The validity is already
   correct while the mode is off" holds for tree writes, not for texture
   content. `content_reach` decides `baked` by the boundary mode, so a
   video, a camera feed or a draw target inside an off boundary leaves
   the kept snapshot valid, and the switch back shows the old pixels. A
   node that is not a snapshot boundary but holds a valid snapshot
   counts as baked. The hit marks it stale, so it counts once and not at
   every content frame.
2. **A shader while off.** The "shader without a snapshot" warning takes
   the shader's dirty flag, and the shader input hits of
   `texture_content_changed` look at the mode. A params write or an
   input change while off is then lost, and the switch back composites
   the old pass output. The switch to snapshot marks a declared shader
   dirty, so the pass runs once over the kept source.
3. **The release sweep runs after a paint walk only**, as the text layer
   sweep does. An app at rest keeps an off boundary's texture until its
   next paint. Accepted.

Stage 2:

4. **A write inside bumps no revision.** `apply_damage` bumps the
   revision before the walk, and a bake hit bumps it too. Without a
   change, every write inside a frozen node makes a full paint walk and
   a submit with no damage. A write whose walk stops at a frozen node
   bumps nothing, so a frozen tile with a video in it costs no frame.
5. **A node mounted while frozen is not painted** until the unfreeze, so
   a transition written on it snaps (`lifecycle.painted`). Accepted, and
   documented with the prop.

Many freezes in one frame: option A is decided.

Layout props are not for animation: a node that moves every frame moves
by `x`/`y`. The frozen node's own box change is here for correctness
(the quad follows the box), not as a way to drag it.

Stage 1, the write: `repaintBoundary` forwards to a rendertree method
that returns the damage, as a thin plugin does. A write of the mode the
node has reports no damage. Leaving Recording drops the recording (it
costs nothing to make again), and a write to Recording drops a snapshot
(the Recording arm would replace it). So only one state is new: a node
that is off and holds a snapshot. The release registry and the "counts
as baked" rule of addition 1 are about that state alone.

## Stage 1 as built (2026-11-17)

- `Element::set_repaint_boundary` owns the write and returns the damage;
  the flux arm parses the value and forwards. The tree keeps the
  registry of off nodes that hold a snapshot through a reconcile hook of
  `edit`, and `release_off_snapshots` sweeps it after the paint walk.
  The release age is `SNAPSHOT_RELEASE_FRAMES`, the text layer's value:
  the storage is held through 121 paint walks after the switch.
- A switch between `snapshot` and `snapshot-no-aa` reports
  `Damage::Paint`: the storage stays and the raster changes.
- Known until the `antialias` prop lands: snapshot, off, `snapshot-no-aa`
  reuses the antialiased raster, because the cache does not hold how it
  was rasterized.
- A vended snapshot texture id (`snapshotTexture`) stays readable while
  the node is off and after the release, and shows the last raster. The
  id holds the texture, so with a vended id the release frees the
  cache's handle only; the storage goes when the next raster publishes
  under the id or the node is destroyed.
- A switch back under another scale takes the snapshot rules: the node's
  scale was not observed while off, so the frame of the switch back is
  its first look (stretched, then one raster at rest; at once past the
  bound).
- Tests: `alloy/examples/boundary_switch.rs` (GL),
  `alloy/src/tests/boundary_switch.rs`, and a property test in flux.

## Done looks like

Stage 1:

- Snapshot, off, snapshot with nothing changed inside reuses the texture
  (`snapshotsReused`, no raster). A change inside while off re-renders
  once at the switch back (`snapshotsRerendered`).
- An off boundary past the release age releases its texture, also under
  a replayed recording.

Stage 2:

- With the repro of
  [boundary-cache-under-js-written-scale](boundary-cache-under-js-written-scale.md),
  but with a color written inside every tile at every frame: frozen, the
  frame costs the translate row (`snapshotsReused` 40, no raster, no
  screen damage from the writes). After the unfreeze, one repaint of
  each tile.
- A texture whose content changes at every frame inside a frozen tile
  damages nothing.
- A frozen node whose box a relayout moves is drawn at its new place.
- An unfreeze with nothing changed inside rasterizes nothing
  (`snapshotsReused` 1). When the box or density moved during the
  freeze, the snapshot rules re-rasterize it as they say.
- `snapshot-no-aa` is gone, `antialias={false}` gives the same raster,
  and an `antialias` write re-renders in place.
- The tree read shows `freeze`, `antialias` and the stale mark while
  frozen (`/tree`, `flux/src/alloy_plugins/properties/read.rs`), so a
  test or an agent can see what the runtime holds.

Tests:

- A lib test for the damage half, which needs no snapshot cache: a write
  inside stops the walk (an ancestor recording survives), and no damage
  is noted (`frame_damage`) for a property write, an insert under the
  frozen node, a relayout inside and a texture content change inside.
  The frozen node's own box change does clear the ancestors.
- A GL example beside `alloy/examples/boundary_transform.rs` for the
  texture half: the switch (reused, one raster after a change while off,
  released past the age), the hold, the stale mark and the unfreeze
  damage. A lib test cannot hold these: its stand-in raster thread
  refuses every raster (`alloy/src/tests/yuv.rs`), so no snapshot cache
  forms.

Docs:

- `packages/core/agents/performance.md` rule 5 says that a switch is
  cheap and when a boundary is worth turning off, documents the freeze,
  and points at `pointerEvents` for the taps. Rule 6 (the first-raster
  hitch) says that a freeze over content that was live pays it.
- The view props in the core AGENTS.md and the typings document both.

## Involves

- `BoundaryMode` (`alloy/src/rendertree/mod.rs`) and two new `Element`
  fields.
- The `repaintBoundary` arm and two new props in
  `flux/src/alloy_plugins/properties/mod.rs` and `read.rs`.
- In `alloy/src/rendertree/tree/mod.rs`: `invalidate_paint`,
  `invalidate_paint_step`, `invalidate_paint_batched`, `apply_damage`,
  `apply_damage_batch`, the structural sites, `texture_content_changed`
  and `content_reach`, and a release registry beside `text_layers`.
- `set_unrounded_layout` in `alloy/src/rendertree/layout/context.rs`.
- The damage ledger (`note_damage`, `damage.rs`).
- `snapshot_node_unculled` in `alloy/src/rendertree/boundary.rs` (the
  three checks the hold skips, the `held` bit).
- The core typings.

Found while reviewing
[boundary-cache-under-js-written-scale](boundary-cache-under-js-written-scale.md).
