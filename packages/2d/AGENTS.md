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
- `flipX`/`flipY` mirror on the UV SIDE: the style/record write swaps
  u0/u1 (v0/v1), so w/h stay the drawn size, a scale transition never sees
  a flip, and picking is unchanged (the vertex stage still carries no
  flip). The flags live on the Sprite and re-apply to every later frame
  write; `getSprite` returns the frame un-mirrored plus the flags. Raw
  `records` writers swap u0/u1 themselves.
- Mutations batch to a microtask: style lease publish + count setDraw +
  `spatial.flush()`. No mutation, no publish, no frame: a static layer
  costs zero, the same demand-gate story as the rest of the platform.
- The records layer (`createRecordLayer`) keeps the old model whole: 13
  JS-owned floats per sprite `[cx, cy, w, h, u0, v0, u1, v1, rot, tint
  rgba]` (`FLOATS_PER_SPRITE`), draw order = insertion order, remove
  shifts, `layer.records` + `touch()` raw writes, JS pick walk. It is the
  escape hatch for motion only JS can compute at scale (measured 30k
  sprites: 12.9ms raw records vs 30.8ms via setSprite; both figures are
  the WRITE path only - whatever computes the motion is excluded and is
  usually the dominant cost, e.g. a 24k-particle sim measured ~25ms with
  a near-free publish) - the axis is
  WHERE MOTION IS COMPUTED, not retained-vs-dynamic. The sprite functions
  (addSprite/setSprite/getSprite/removeSprite) work on both layer kinds;
  record sprites have `node: null` and no groups.
- Layer space is pixels, top-left origin, y-down - the render tree's frame.
  The pipeline's clip space is y-down too (core gpu.ts pixel contract), so
  the vertex stage carries NO flip anywhere. Do not add one.
- The camera (`setCamera`/the `camera` prop) is ONE `CameraUpdate` type
  across both layers: offset, zoom, and rotation about a pivot (camera.ts
  documents the semantics and the heading-upward convention,
  `rotation = -h - pi/2`). On the sprite layer it is a shared-params
  write (`uCamera` + `uCameraRot`, the rotation in-shader), one call
  however many sprites exist; `projectCamera`/`unprojectCamera` export
  the world <-> screen mapping as pure functions, and pointer dispatch
  undoes the camera with the latter, so events arrive in world (layer)
  pixels. Picking itself works in world space and never sees the camera.
- Retargeted motion is NATIVE: `setSpriteTransition(sprite, { position:
  { duration: 700, bounce: 0.3 }, ... })` (or the `transition` prop) makes
  setSprite writes TARGETS the core animates toward - position/scale
  (w/h) on the shared spring/tween math, rotation along the quaternion
  geodesic (always the short arc; a spring keeps its velocity through
  retargets). JS costs one write per target CHANGE, zero per frame; the
  running tracks drive frame demand themselves, and settled sprites cost
  nothing (bench: 400 retargets ~4ms, once a second - vs ~5ms per FRAME
  moving the same population imperatively). Retargeting every frame is
  also a legitimate pattern, not an abuse: rewriting a spring's TARGET
  each frame to chase a moving point (a follow-camera trailing a moving
  sprite) rides the spring's smoothing for free - a spring keeps its
  velocity through retargets, so the chase stays fluid. Mount poses always snap (the
  component declares the transition after the first pose sync; the
  function face sets it after addSprite). Each natural settle calls the
  handle's `onTransitionEnd` (plain field, or the `<Sprite>`/`<Group>` prop)
  with `{ component }` - target-only, never on a cancel, snap or removal;
  the raw "spatialTransitionEnd" engine event (srt:events, node =
  sprite.node) stays for flux:spatial consumers. See examples/springs.tsx.
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

Scrolling never re-bakes: the `<TileLayer>` camera prop (`TileCamera`, an
alias of the shared `CameraUpdate`) is a transform on the composited world
view - the world point (x, y) pinned to the viewport point (pivotX,
pivotY), scaled by zoom, ROTATED by rotation about the pivot. The type IS
the sprite layer's camera type, so one signal drives a whole rotating
scene across both layers (sprites ride the same rotation in-shader);
rotation is the ship-flies-over-the-map camera and costs the same
transform write. The
grid shape is creation-fixed (recreate to resize). Tiles are data, not
children: there is no `<Tile>` component on purpose - write cells through
`ref` with `setTile`. Not built yet: camera-driven residency (bake far
chunks on approach, evict) - okf/backlog/2d-baked-layers.md.

## Components

| Component | Props |
|---|---|
| `SpriteLayer` | width, height (layer pixels), atlas (TextureId), capacity?, clearColor?, camera?, oversample?, maxOversample?, label?, ref?, output?, events? |
| `Sprite` | x, y (center; local to the enclosing `<Group>`), w, h, frame?, rotation? (radians, clockwise), tint? ([r,g,b,a] 0..1), transition?, onPointer{Down,Move,Up,Enter,Leave}?, ref? |
| `Group` | x?, y?, rotation?, scale? (uniform, scales the subtree), transition?, ref? |
| `TileLayer` | cols, rows, tileW, tileH, atlas (TextureId), chunkClearColor?, filter?, chunkTiles?, oversample?, maxOversample?, camera? (TileCamera: x, y, zoom, rotation, pivotX, pivotY), label?, ref? |

`SpriteLayer` owns the layer and renders the built-in `<texture>` leaf
carrying the layer's pointer handlers (opt out with `events={false}`; compose
yourself with `output`, then spread `useSpriteLayer().handlers` onto your
leaf). `Sprite` renders nothing - it allocates a record through context and
syncs props into it.
`GroupContext` is `createContext<SpriteGroup | null>(null)` on purpose: an
optional parent needs a non-undefined default, since Solid 2 throws on a
resolved `undefined` even when one was passed as the default.

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
  cache `layer.records` across addSprite - growth replaces the array and
  a hoisted reference becomes a dead copy whose writes publish nothing.
  `layer.withRecords(fn)` is the hoist-proof read; a bare `layer.records`
  at use time is equally live.
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
  blends with `blend: "alpha"` in record order, the premultiplied composite.
  The atlas is premultiplied because `decodeImage` premultiplies by default;
  an atlas uploaded from straight-alpha pixels (`decodeImage(bytes, { alpha:
  "straight" })` + `createTexture`) draws color under transparent texels
  as opaque - the classic "keyed-out backdrop becomes a wash" symptom.
- Every rotation must agree on direction (clockwise, y-down):
  `pointInSprite` in pick.ts with the vertex stage's `iRot`, and
  `projectCamera` in camera.ts with `uCameraRot` and `<TileLayer>`'s view
  transform. The differential checks (pick-check.ts, camera-check.ts)
  guard the JS math against oracles but NOT against the shader - if you
  touch one rotation, touch all, then run examples/camera-probe.tsx (the
  live guard: shader vs projectCamera, node/record parity, the pointer
  round trip) and watch for CAMERA-OK.
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
  the box. Both layers render LAID-OUT elements (`<TileLayer>` a `<view>`,
  `<SpriteLayer>` a `<texture>` leaf) and take no position props, so to
  overlay sprites on tiles wrap each in a `<view position="absolute">`
  inside that container - as plain flex siblings they stack side by side
  instead.
  Both are LAYOUT components: neither can live inside a d-* subtree (the
  insert throws); for a detached parent, `<SpriteLayer output>` hands out
  the texture id for a `<d-texture>` of your own.
- `chunkClearColor` is PER CHUNK, as named: never-written regions have no
  chunk and render nothing, so a full-bleed ground color belongs on the
  container behind the layer (a `d-rect` under it), not here. The flip side is FREE
  TRANSPARENCY: with the default `[0,0,0,0]` clear, a mostly-unwritten
  tile layer stacked over another composites with no mask, no alpha pass,
  no shader - a sparse upper world (floating clouds over a sea) just
  works, and per-cell alpha in the written cells carries through the
  chunk (transparent clear + alpha blend).
- Two samplers, two jobs. The ATLAS sampler (createAtlas `filter`) decides
  whether texels are hard blocks ("nearest", pixel art) or smooth
  ("linear"). The LAYER's output sampler (the sprite layer's target, the
  tile layer's `filter` option, default "linear") does the composite
  resample to the box, and stays linear: "nearest" there snaps texels to
  uneven widths at any fractional scale (a 3.6x designSize fit draws
  source pixels 3 or 4 device pixels wide - shimmer standing still, boil
  when scrolling). Proper resampling at a fractional or HiDPI scale is the
  `oversample` factor: the layer renders at n texels per layer pixel
  (nearest inside keeps blocks square), the linear composite spreads the
  fraction over one device pixel at block edges. The components pick n
  every layout from their leaf's window box (`getBoundingBoxViewport` x
  `displayScale()`, which composes designSize fits and camera zoom; the
  tile layer divides its camera rotation's AABB swell back out - rotation
  is not a resolution factor - and shrinks only past a margin so an
  oscillating measurement cannot re-bake in a loop), within
  a budget of the window's own device pixel count; the
  primitives default to 1 and take `{ oversample }` / `setOversample(n)`
  - with `output` on `<SpriteLayer>` there is no built-in leaf, so set it
  yourself with the exported `fitOversample`. The window budget bounds one
  TARGET, which never binds for chunk-sized tile targets: a tile world's
  texture memory is resident chunks x n squared, and `maxOversample` is
  the cap that bounds it (auto-pick only; a 2x display otherwise picks 16x
  the memory of n = 1, silently). A layer whose oversample changes more
  than a few times in a second warns in the console (thrash: every change
  resizes and redraws the targets, a tile layer re-bakes every resident
  chunk) - pin `oversample` or set `maxOversample` when an animated
  transform or camera legitimately sweeps the scale. Never fix a shimmer by
  snapping the fit to an integer: the scene should fill its box at any
  ratio.
- Retro scrolling habits are the app's, not the layer's: keep the camera
  fractional (rounding it to design pixels makes motion step at the
  rounding rate, not the frame rate), and a game that wants whole-pixel
  scrolling snaps its own camera.
- A dirty chunk flush publishes and re-bakes that chunk in full. Fine on
  change-only cadence; per-frame setTile churn re-bakes chunks per frame -
  that is sprite-layer work, not tile work.
- Chunk allocation is MONOTONIC: nothing evicts, so texture memory is
  proportional to the touched area (~920KB per resident chunk at the
  default size) and every resident chunk keeps a composited leaf. Bounded
  worlds only; streaming/infinite is stage B2 in
  okf/backlog/2d-baked-layers.md.
- The sprite layer's camera rotation is IN-SHADER (uCameraRot) on
  purpose: rotating the layer's OUTPUT leaf instead is wrong - the output
  is viewport-sized with the camera already applied, so a leaf transform
  spins the cropped viewport and the corners cut. The tile layer gets
  away with the transform-on-the-leaf camera only because its composited
  view is the WORLD, not a viewport of it.
