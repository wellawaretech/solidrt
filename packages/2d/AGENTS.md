# @solidrt/2d - agent notes

An instanced sprite layer above `@solidrt/core/gpu`: one atlas texture, one
instance buffer, N quads in ONE draw call, composited into the app as an
ordinary `<texture>` leaf. Sprite records publish through the zero-copy
buffer write lease (`beginBufferWrite`/`endBufferWrite`), so per-frame motion
costs float stores plus one bulk publish - never per-sprite property writes,
which is the whole reason this package exists (rendertree `d-texture` sprites
are the right tool up to the low thousands; measured ~0.65us paint and ~15KB
memory per NODE, and every moved node is two setProperty FFI calls per
frame).

## The model

- Two layers, the @solidrt/3d split verbatim: the imperative core (layer.ts:
  `createSpriteLayer`/`addSprite`/`setSprite`/`removeSprite` - plain objects,
  dirty flags, no signals, usable without components) and the component face
  (components.tsx: `SpriteLayer`/`Sprite` over context, effects syncing props
  into the retained records, no new intrinsic elements).
- A sprite is 13 floats in the layer's canonical Float32Array:
  `[cx, cy, w, h, u0, v0, u1, v1, rot, tintR, tintG, tintB, tintA]`
  (`FLOATS_PER_SPRITE`). Draw order IS record order IS insertion order -
  painter's algorithm, later over earlier. There is no z field in v1;
  reorder by remove/re-add, or wait for the z pass (see Traps).
- Mutations batch to a microtask. The flush does
  `beginBufferWrite` -> bulk `.set` of the live prefix -> `endBufferWrite`
  -> `setDraw({ instanceCount })` when the count changed. No mutation, no
  publish, no frame: a static layer costs zero, the same demand-gate story
  as the rest of the platform.
- Layer space is pixels, top-left origin, y-down - the render tree's frame.
  The pipeline's clip space is y-down too (core gpu.ts pixel contract), so
  the vertex stage carries NO flip anywhere. Do not add one.
- The camera (`setCamera`/the `camera` prop) is a shared-params write
  (`uCamera`: offset + zoom), one call however many sprites exist. Picking
  undoes it, so events arrive in world (layer) pixels.
- Frame-rate motion bypasses the declarative layer: `ref` the sprite, call
  `setSprite` from `onFrame`. Signals carry structure and slow state - a
  `<Sprite x={sig()}>` re-running 60 times a second works but re-runs an
  effect per sprite per frame for nothing.
- Above ~10k moving sprites, setSprite's call overhead dominates (measured
  30k sprites fullscreen on a desktop RTX machine: 30.8ms via setSprite,
  12.9ms writing `layer.records` directly + one `layer.touch()`). The raw
  path is public for exactly this; the record layout is documented on the
  type and `FLOATS_PER_SPRITE` is exported.
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
| `Sprite` | x, y (center), w, h, frame?, rotation? (radians, clockwise), tint? ([r,g,b,a] 0..1), onPointer{Down,Move,Up,Enter,Leave}?, ref? |
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
- `capacity` is a reservation, not a limit: `addSprite` past it doubles the
  canonical array, and the next publish creates a larger GPU buffer, writes
  it, swaps it in with `setDraw({ instanceBuffer })` and destroys the old
  one. Records are 52 bytes each; reserve realistically to skip the copies.
  Do not cache `layer.records` across addSprite - growth replaces the array.
- Record order is draw order: `removeSprite` shifts every later sprite down
  one slot (copyWithin + index fixup, O(later sprites)). Cheap in practice;
  do not remove thousands per frame and expect it free.
- The flush publishes the WHOLE live prefix, not a dirty range: one moved
  sprite re-publishes count x 52 bytes. That is a single memcpy plus the
  lease message - at 10k sprites ~520KB, microseconds - and keeps the write
  path one code path. A dirty-range optimization is possible (writeBuffer
  takes byteOffset) but was deliberately not built until a measurement asks
  for it.
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
