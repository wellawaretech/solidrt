# @solidrt/2d - agent notes

An instanced sprite layer above `@solidrt/core/gpu`: one atlas texture, N
quads in ONE draw call, composited into the app as an ordinary `<texture>`
leaf. The live layer backs every sprite with a SPATIAL ARENA node (never a
rendertree element - `d-texture` sprites are the right tool up to the low
thousands; measured ~0.65us paint and ~15KB memory per NODE, and every
moved node is two setProperty FFI calls per frame). The node makes sprites
citizens of the spatial core: native producers (node transitions, animation
clips, physics) reach them through `sprite.node`, hierarchy recomputes
moved subtrees in Rust, and picking walks the core BVH.

## The model

- THREE faces, layered: the node-backed live layer (layer.ts:
  `createSpriteLayer`/`addSprite`/`setSprite`/`removeSprite` plus
  `addGroup`/`setGroup`/`setSpriteParent`/`setSpriteTransition`/
  `setGroupTransition` - plain objects, no signals, usable without
  components), the records layer (records.ts: `createRecordLayer` - the
  raw escape hatch, below), and the component face (components.tsx:
  `SpriteLayer`/`Sprite`/`Group` over context).
- Node layer ownership split, two instance-buffer slots on one pipeline:
  slot 0 is the POSE buffer `[x, y, angle, sx, sy]` written ONLY by the
  core (each sprite node's Pose2D record sink; one coalesced buffer write
  per flush however many nodes moved), slot 1 the STYLE buffer
  `[u0, v0, u1, v1, tint rgba]`, JS-owned, published through the zero-copy
  write lease. NEVER write the pose buffer from JS - the core's staging
  mirror owns it and will overwrite.
- Sprites hold FIXED instance slots: draw order is slot order, removal
  zeroes the pose (zero scale = nothing drawn) and recycles the slot to
  the next add. No painter's-insertion-order guarantee across removals;
  opaque-or-transparent pixel art never notices, z-ordered translucency is
  the sort-key backlog item (okf/backlog/2d-sprite-sort-key.md).
- Growth (past `capacity`, doubling): pose sinks move in ONE core
  `retargetRecords` call (full republish next flush), style re-uploads,
  `setDraw({ instanceBuffers })` swaps both, old buffers destroyed.
- Picking is the core index: `pick` raycasts [x, y, -1] along +z (exact
  rotated-rect via the node's local box), topmost = highest slot;
  `pickRect` is the BVH overlap query (exact for rotated sprites, the
  marquee). Both filter to the layer's own nodes - the arena is shared
  with e.g. a 3d scene.
- Groups (`addGroup`/`<Group>`) are plain arena nodes (x, y, rotation,
  UNIFORM scale - a group is a frame, never a sprite size; sprite w/h
  lives in the sprite node's scale, which is why sprites cannot parent
  sprites). Child sprite pose fields are local to the group.
- Mutations batch to a microtask: style lease publish + count setDraw +
  `spatial.flush()`. No mutation, no publish, no frame: a static layer
  costs zero, the same demand-gate story as the rest of the platform.
- The records layer (`createRecordLayer`) keeps the old model whole: 13
  JS-owned floats per sprite `[cx, cy, w, h, u0, v0, u1, v1, rot, tint
  rgba]` (`FLOATS_PER_SPRITE`), draw order = insertion order, remove
  shifts, `layer.records` + `touch()` raw writes, JS pick walk. It is the
  escape hatch for motion only JS can compute at scale (measured 30k
  sprites: 12.9ms raw records vs 30.8ms via setSprite) - the axis is
  WHERE MOTION IS COMPUTED, not retained-vs-dynamic. The sprite functions
  (addSprite/setSprite/getSprite/removeSprite) work on both layer kinds;
  record sprites have `node: null` and no groups.
- Layer space is pixels, top-left origin, y-down - the render tree's frame.
  The pipeline's clip space is y-down too (core gpu.ts pixel contract), so
  the vertex stage carries NO flip anywhere. Do not add one.
- The camera (`setCamera`/the `camera` prop) is a shared-params write
  (`uCamera`: offset + zoom), one call however many sprites exist. Picking
  undoes it, so events arrive in world (layer) pixels.
- Retargeted motion is NATIVE: `setSpriteTransition(sprite, { position:
  { duration: 700, bounce: 0.3 }, ... })` (or the `transition` prop) makes
  setSprite writes TARGETS the core animates toward - position/scale
  (w/h) on the shared spring/tween math, rotation along the quaternion
  geodesic (always the short arc; a spring keeps its velocity through
  retargets). JS costs one write per target CHANGE, zero per frame; the
  running tracks drive frame demand themselves, and settled sprites cost
  nothing (bench: 400 retargets ~4ms, once a second - vs ~5ms per FRAME
  moving the same population imperatively). Mount poses always snap (the
  component declares the transition after the first pose sync; the
  function face sets it after addSprite). Each settled component fires a
  "spatialTransitionEnd" engine event (srt:events), node = sprite.node.
  See examples/springs.tsx.
- Frame-rate motion only JS can compute (physics, flocking) bypasses the
  declarative layer: `ref` the sprite, call `setSprite` from `onFrame` (a
  ~7us core transform write per moved sprite - fine to a few thousand;
  past that use the records layer). Signals carry structure and slow
  state - a `<Sprite x={sig()}>` re-running 60 times a second works but
  re-runs an effect per sprite per frame for nothing.
- frames.ts and pick.ts are pure (no GPU imports) BY DESIGN so they can be
  checked headless; keep them that way.

## The baked tile layer (tiles.ts)

Static 2D bulk as a few quads: on tiled GPUs the budget is primitive count
(core agents/performance.md), so a 100x100 tile world must not be 10,000
quads per frame. `createTileLayer(cols, rows, tileW, tileH, atlas)` bakes
the world into CHUNKED `render: "manual"` targets (default ~512px of tiles
per chunk, `chunkTiles` to tune), each chunk a small copy of the sprite
pipeline (shaders.ts) with FIXED record slots - an empty tile is a
zero-size quad, instance count is constant per chunk. Records hold WORLD
pixel coordinates; each chunk target's `uCamera` is its pixel origin, so
the shared vertex stage does the chunk-local mapping. Chunks allocate on
the first `setTile` that reaches them - an empty chunk costs nothing, a
sparse world is bounded by its content, and world size is bounded by
memory, not `maxTextureSize`. `setTile` batches to a microtask whose flush
publishes and re-bakes ONLY dirty chunks. After that the layer is static
textures: zero per-frame cost however many tiles exist.

Scrolling never re-bakes: the `<TileLayer>` camera prop (`TileCamera`) is
a transform on the composited world view - the world point (x, y) pinned
to the viewport point (pivotX, pivotY), scaled by zoom, ROTATED by
rotation about the pivot. Pivot (0,0) makes `{x, y, zoom}` mean what the
sprite layer's camera means, so one signal drives both; rotation is the
ship-flies-over-the-map camera and costs the same transform write. The
grid shape is creation-fixed (recreate to resize). Tiles are data, not
children: there is no `<Tile>` component on purpose - write cells through
`ref` with `setTile`. Not built yet: camera-driven residency (bake far
chunks on approach, evict) - okf/backlog/2d-baked-layers.md.

## Components

| Component | Props |
|---|---|
| `SpriteLayer` | width, height (layer pixels), atlas (TextureId), capacity?, clearColor?, camera?, label?, ref?, output?, events? |
| `Sprite` | x, y (center; local to the enclosing `<Group>`), w, h, frame?, rotation? (radians, clockwise), tint? ([r,g,b,a] 0..1), transition?, onPointer{Down,Move,Up,Enter,Leave}?, ref? |
| `Group` | x?, y?, rotation?, scale? (uniform, scales the subtree), transition?, ref? |
| `TileLayer` | cols, rows, tileW, tileH, atlas (TextureId), clearColor?, filter?, chunkTiles?, camera? (TileCamera: x, y, zoom, rotation, pivotX, pivotY), label?, ref? |

`SpriteLayer` owns the layer and renders the built-in `<texture>` leaf
carrying the layer's pointer handlers (opt out with `events={false}`; compose
yourself with `output`, then spread `useSpriteLayer().handlers` onto your
leaf). `Sprite` renders nothing - it allocates a record through context and
syncs props into it.

Pointer events: exact rotated-rect containment, topmost sprite first, capture
per pointerId (a drag keeps delivering to the grabbed sprite with live
coordinates), enter/leave paired per pointer. No bubbling - the sprite list
is flat. Event x/y are layer pixels with the camera undone.

## Traps

- The atlas is NOT owned by the layer: layers come and go, atlases usually
  live app-long. Dispose atlases yourself (or let the reactive owner do it -
  createAtlas registers with the owning scope like every core texture).
- `capacity` is a reservation, not a limit, on both layer kinds; reserve
  realistically to skip the growth copies. On the records layer, do not
  cache `layer.records` across addSprite - growth replaces the array.
- Node layer: `sprite.node` is public FOR BINDING PRODUCERS, not for
  lifecycle - never destroyNode it yourself (removeSprite owns that), and
  a transform written through flux:spatial directly bypasses the sprite's
  pose mirror, so a later setSprite with the old x wins (its compare sees
  no change to skip, but partial writes compose from the mirror).
- With a transition set, the sprite's fields (and getSprite) read the
  TARGET, not the mid-flight pose - the JS mirror is what setSprite
  composes partial writes from, and targets are the right thing to
  compose. Picking and the pose buffer see the actual mid-flight pose
  (what is on screen). Clearing the transition (null) keeps the
  mid-flight pose on the node while the mirror still holds the old
  target: the next setSprite write snaps to whatever it says.
- Node layer picking reads the index as of the last core flush; `pick`/
  `pickRect` run the layer's pending batch first, so write-then-pick in
  one tick is coherent. Producers moving nodes between flushes are one
  frame stale to picking, like every query.
- Records layer: record order is draw order: `removeSprite` shifts every
  later sprite down one slot (copyWithin + index fixup, O(later
  sprites)). Its flush publishes the WHOLE live prefix, not a dirty
  range: one moved sprite re-publishes count x 52 bytes - a single
  memcpy, microseconds at 10k; the node layer's style publish is the same
  whole-prefix shape. Dirty ranges were deliberately not built until a
  measurement asks.
- The node layer's STYLE slots are not compacted: a removed sprite leaves
  its style floats in place (invisible - the pose is zeroed) until the
  slot recycles. Do not read style truth from the buffer; getSprite reads
  the JS mirror.
- `createImage` is the wrong loader for pixel-art atlases: it never forwards
  sampler options, so it is always `filter: "linear"`. `createAtlas` decodes
  bytes and passes `filter: "nearest"` through - use it, or `decodeImage` +
  `createTexture` directly.
- Tint multiplies the sampled texel (`texture * tint`) and the pipeline
  blends with `blend: "alpha"` in record order. The layer's OUTPUT obeys the
  premultiplied-alpha contract to the extent the atlas does: PNG decode
  yields straight alpha, and a translucent texel tinted translucent can
  composite slightly wrong at the edges. Opaque-or-transparent pixel art
  (the overwhelming case) is exact. A premultiply-on-decode option is the
  fix if it ever matters; note it, do not silently add it.
- `pointInSprite` in pick.ts and the vertex stage's rotation must agree on
  direction (clockwise, y-down). The differential check (pick-check.ts)
  guards the math against an oracle but NOT against the shader - if you
  touch one rotation, touch both.
- Sprite handles go inert on removal (`sprite.layer === null`); setSprite on
  an inert handle is a silent no-op (matching the throw-in-dev policy would
  mean throwing, but removal racing a queued pointer event is routine, not
  a bug).
- The `<Sprite>` effect syncs ALL seven fields when ANY prop changes (one
  effect, one tuple). Fine at component scale; if a profile ever blames it,
  split the effects before inventing anything cleverer.
- `<TileLayer>`'s world view is WORLD sized (cols * tileW) and takes that
  much layout space: put it inside a clipping container (`overflow="clip"`)
  sized to the viewport, or camera panning shows the world hanging out of
  the box.
- `clearColor` is PER CHUNK: never-written regions have no chunk and render
  nothing, so a full-bleed ground color belongs on the container behind the
  layer (a `d-rect` under it), not on clearColor.
- Tile layer zoom/rotation scale the BAKED chunk textures at composite time
  (the sprite layer's zoom re-samples the atlas in-shader), so the tile
  layer's `filter` option is what pixel art must set to "nearest" - on top
  of the atlas's own nearest from createAtlas; they are different samplers.
- A dirty chunk flush publishes and re-bakes that chunk in full. Fine on
  change-only cadence; per-frame setTile churn re-bakes chunks per frame -
  that is sprite-layer work, not tile work.
- Chunk allocation is MONOTONIC: nothing evicts, so texture memory is
  proportional to the touched area (~920KB per resident chunk at the
  default size) and every resident chunk keeps a composited leaf. Bounded
  worlds only; streaming/infinite is stage B2 in
  okf/backlog/2d-baked-layers.md.
- The sprite layer's camera cannot rotate (uCamera is offset + zoom); a
  sprite layer riding a rotating TileCamera needs
  okf/backlog/2d-sprite-camera-rotation.md first. Rotating the sprite
  layer's OUTPUT leaf instead is wrong - it is viewport-sized, the corners
  cut.
