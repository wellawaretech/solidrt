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

Contents:

- [The model](#the-model)
  - [Three faces](#three-faces)
  - [A layer renders through its views](#a-layer-renders-through-its-views)
  - [Pose and style slots](#pose-and-style-slots)
  - [Fixed instance slots](#fixed-instance-slots)
  - [Growth](#growth)
  - [Picking](#picking)
  - [Spatial queries](#spatial-queries)
  - [Groups](#groups)
  - [Visibility](#visibility)
  - [Flips](#flips)
  - [Mutations batch to a microtask](#mutations-batch-to-a-microtask)
  - [The records layer](#the-records-layer)
  - [Layer space](#layer-space)
  - [The camera](#the-camera)
  - [Camera control](#camera-control)
  - [Layer tint](#layer-tint)
  - [Views](#views)
  - [Retargeted motion](#retargeted-motion)
  - [Frame-rate motion](#frame-rate-motion)
  - [Frame animation](#frame-animation)
  - [Pure pieces](#pure-pieces)
- [The baked tile layer (tiles.ts)](#the-baked-tile-layer-tilests)
  - [Chunks](#chunks)
  - [Scrolling and the camera](#scrolling-and-the-camera)
  - [Bulk writes](#bulk-writes)
  - [Tinting](#tinting)
- [Components](#components)
  - [Component props](#component-props)
  - [SpriteLayer and useSpriteLayer](#spritelayer-and-usespritelayer)
  - [Pointer events](#pointer-events)
- [Traps](#traps)
  - [Atlases and sampling](#atlases-and-sampling)
  - [Slots, capacity and order](#slots-capacity-and-order)
  - [Nodes, transitions and queries](#nodes-transitions-and-queries)
  - [Cameras and rotation](#cameras-and-rotation)
  - [Tile layer](#tile-layer)

## The model

### Three faces

THREE faces, layered: the node-backed live layer (layer.ts:
`createSpriteLayer`/`addSprite`/`setSprite`/`destroySprite` plus
`addGroup`/`setGroup`/`setSpriteParent`/`setSpriteTransition`/
`setGroupTransition` - plain objects, no signals, usable without
components), the records layer (records.ts: `createRecordLayer` - the
raw escape hatch, below), and the component face (components/:
`SpriteLayer`/`Sprite`/`Group`/`View2d`/`Camera2d` over context).

### A layer renders through its views

A layer holds sprites and renders NOTHING by itself: it shows through
its VIEWS (`layer.createView({ width, height, camera?, oversample?,
clearColor?, label? })`, `<View2d>`, or the `<SpriteLayer>`'s own leaf,
which is one), each a draw target with a camera, a size and pointer
dispatch of its own - a Unity scene renders only through Cameras, a
Godot World2D only through Viewports, and there is no privileged first
one. The window view, the minimap, two split-screen panes are the same
`ViewHandle`. Details under Views below.

### Pose and style slots

Node layer ownership split, two instance-buffer slots on one pipeline:
slot 0 is the POSE buffer `[x, y, angle, sx, sy]` written ONLY by the
core (each sprite node's Pose2D record sink; one coalesced buffer write
per flush however many nodes moved), slot 1 the STYLE buffer
`[u0, v0, u1, v1, tint rgba, renderOrder]`, JS-owned, published through the
zero-copy write lease. NEVER write the pose buffer from JS - the core's
staging mirror owns it and will overwrite.

### Fixed instance slots

Sprites hold FIXED instance slots: draw order is slot order, removal
zeroes the pose (zero scale = nothing drawn) and recycles the slot to
the next add. No painter's-insertion-order guarantee across removals;
opaque-or-transparent pixel art never notices, and a scene that needs a
real draw order says so with `orderBy` ("y" or "renderOrder" - see below).

### Growth

Growth (past `capacity`, doubling): pose sinks move in ONE core
`retargetRecords` call (full republish next flush), style re-uploads,
`setDraw({ instanceBuffers })` swaps both, old buffers destroyed.

### Picking

Picking is the core index: `pick` raycasts [x, y, -1] along +z (exact
rotated-rect via the node's local box) and returns ALL hits topmost
first (highest slot) - the all-hits shape of @solidrt/3d's pick, so
`pick(x, y)[0]` is the topmost; `pickRect` is `overlap` over an
unrotated rect, sprites only (exact for rotated sprites, the marquee),
unordered. Every query passes the core a filter with the layer's root
node (sprites and groups without a parent hang off it), so the arena
being shared with e.g. a 3d scene costs nothing in JS.

### Spatial queries

Spatial queries (`checks/collision-check.tsx` pins the contract): the
3d scene's trio one dimension down, Godot's PhysicsDirectSpaceState2D
and CharacterBody2D in their names. `layer.raycast(x, y, dx, dy,
opts?)` is every shown sprite the ray strikes, nearest first, with
distance, edge point and edge normal; `layer.overlap(volume, opts?)`
every sprite a volume touches with its deepest contact `{ sprite,
point, normal, depth }`, unordered; `layer.sweep(volume, dx, dy,
opts?)` every sprite the moving volume first touches `{ sprite, time,
point, normal }`, earliest first; `layer.moveAndSlide(volume, dx, dy,
opts?)` the character mover over them, one core call per body per
frame (the depenetration, slide and floor-snap loop runs in the spatial
core, the same one @solidrt/3d's mover uses), returning `{ motion,
floor, wall, ceiling, hits }` with `up` defaulting to [0, -1] (y-down).
A `Volume` is a `Circle` `{ x, y, radius }`, a `Capsule` `{ ax, ay,
bx, by, radius }` or a `Rect` `{ x, y, width, height, rotation? }`;
`QueryOptions.sprites` is an include-list. Results name sprites, never
node ids, and every query runs the pending batch first, like pick.
The trap behind the design: a sprite is a COLUMN in the index
(`SPRITE_DEPTH` half-depth along z), not a flat quad. Against a flat
triangle the core's contact for a circle whose center lies over the
sprite is the plane normal (z) with depth = radius; against a column
every volume at z = 0 meets only side faces, so contacts, sweeps and
the mover stay in the plane with no 2d narrowphase of their own.

### Groups

Groups (`addGroup`/`<Group>`) are plain arena nodes (x, y, rotation,
UNIFORM scale - a group is a frame, never a sprite size; sprite w/h
lives in the sprite node's scale, which is why sprites cannot parent
sprites). Child sprite pose fields are local to the group. `destroyGroup`
destroys the SUBTREE: child sprites and groups die with it (re-parent a
child out first to keep it), children first so each plays its own
`exit` and the group stays their frame until the last has settled. One
removal verb, the same word with the same meaning as @solidrt/3d's
`destroy`: a sprite cannot exist outside its layer, so there is no
detach here (3d's `remove`), and a sprite that should come back is
hidden (`visible: false`).
A handle's x/y are local to its group; `worldPosition(sprite | group)`
reads the layer-pixel position composed through every enclosing group
from the core's world matrix (@solidrt/3d's worldPosition).

### Visibility

`visible` (@solidrt/3d's setVisible, in the setSprite/setGroup bag;
default true) is core node visibility: hidden, the pose slot zeroes
(nothing drawn) and pick/pickRect skip the node; a group's flag hides
its WHOLE subtree while each child keeps its own. Handle, slot, style
records and transition state stay, so showing restores the sprite as
it was. Node layer only - on a record sprite it throws (hide those by
zeroing w or h).

### Flips

`flipX`/`flipY` mirror on the UV SIDE: the style/record write swaps
u0/u1 (v0/v1), so w/h stay the drawn size, a scale transition never sees
a flip, and picking is unchanged (the vertex stage still carries no
flip). The flags live on the Sprite and re-apply to every later frame
write; `getSprite` returns the frame un-mirrored plus the flags. Raw
`records` writers swap u0/u1 themselves.

### Mutations batch to a microtask

Mutations batch to a microtask: style lease publish + count setDraw +
`spatial.flush()`. No mutation, no publish, no frame: a static layer
costs zero, the same demand-gate story as the rest of the platform.

### The records layer

The records layer (`createRecordLayer`) keeps the old model whole: 13
JS-owned floats per sprite `[cx, cy, w, h, u0, v0, u1, v1, rot, tint
rgba]` (`FLOATS_PER_SPRITE`), draw order = insertion order (or key
order with `orderBy` - see below), remove
shifts, `layer.records` + `touch()` raw writes, JS pick walk. It is the
escape hatch for motion only JS can compute at scale (measured 30k
sprites: 12.9ms raw records vs 30.8ms via setSprite; both figures are
the WRITE path only - whatever computes the motion is excluded and is
usually the dominant cost, e.g. a 24k-particle sim measured ~25ms with
a near-free publish) - the axis is
WHERE MOTION IS COMPUTED, not retained-vs-dynamic. The sprite functions
(addSprite/setSprite/getSprite/destroySprite) work on both layer kinds;
record sprites have `node: null` and no groups.

### Layer space

Layer space is pixels, top-left origin, y-down - the render tree's frame.
The pipeline's clip space is y-down too (core gpu.ts pixel contract), so
the vertex stage carries NO flip anywhere. Do not add one.

### The camera

The camera (`view.setCamera`/the `camera` prop) is ONE `CameraUpdate`
type across both layer kinds' views: offset, zoom, and rotation about a
pivot (camera.ts documents the semantics and the heading-upward
convention, `rotation = -h - pi/2`). It is a shared-params write on
the view's target (`uCamera` + `uCameraRot`, the rotation in-shader),
one call however many sprites exist; `projectCamera`/`unprojectCamera`
export the world <-> screen mapping as pure functions, and pointer
dispatch undoes the view's camera with the latter, so events arrive in
world (layer) pixels. Picking itself works in world space and never
sees a camera. `view.camera()` reads the camera back (every field
present - the `CameraState` snapshot, @solidrt/3d's scene.camera())
and `view.project(x, y)` / `view.unproject(x, y)` are the two pure
functions over it.

### Camera control

Camera control: `createCamera2d(view | views, { viewport: () =>
({ width, height }), world?: { width, height }, min/maxZoom?, pivot?,
deadZone?: { width, height }, panSpeed?, zoomSpeed?, rollSpeed?,
followSpeed?, inertia?, x?, y?, zoom?, rotation? })` - Godot's Camera2D
and Three's MapControls in one control over the shared CameraUpdate:
pan with inertia on release, zoom about a point, eased `glideTo`/`fit`,
`follow(x, y)` through a dead zone with damping, roll about the pivot,
and world bounds that CONTAIN the view (an axis whose view is wider
than the world centers - so at the fit zoom a pan is a no-op, there is
nothing to pan; limits ignore rotation, Godot's rule). The pose is
"world point at the pivot", the pivot a viewport fraction defaulting to
the center, so `camera().x/y` is the view center and glideTo/follow
land there (without a `world`, the default pose puts world 0,0 at the
pivot: a fill layer that wants world = screen at rest takes `pivot: {
x: 0, y: 0 }`). The first argument is anything with a view's
`setCamera` (a view of a sprite layer, of a record layer, several at
once - one camera over a scene's layers - or a signal setter feeding
`<TileLayer camera>`).

#### Input and bindings

Input, the rule (ARCHITECTURE.md): the control consumes a device-free
abstraction and never handles events itself. Its `axes` (core's
createAxes contract) are `pan` (vec2: a delta is finger travel in
viewport heights, applied as content travel - the world follows the
finger; a rate slides at one viewport height per second), `zoom` (axis,
octaves, positive in: a delta bracketed by a gesture - a pinch - applies
at once about its focal point, an unbracketed one - a wheel notch -
retargets an eased glide, which is how the control tells a finger from
an impulse; notches compound on the pending target) and `roll` (axis,
turns); a pan gesture's begin stops any glide (a finger landing on a
gliding view holds it) and its end flings with the drag's velocity. The
verbs, each pushing the pose at once: `panBy(dx, dy)` screen pixels,
`zoomAt(sx, sy, factor, { glide? })`, `rollBy(angle)`, `set(pose)`,
`glideTo`, `fit`, `follow`/`unfollow`, `interrupt`, `release`. An input
map (core AGENTS.md) drives the axes by name and the APP binds devices
to it: the view's pointer feed (`createPointerFeed()`, handed to
`<SpriteLayer pointer>` / `<View2d pointer>`, or bridged with
`feedPointer(view, feed)` for an imperative view - fed from the view's
ROOT, so a sprite that claims its press keeps the camera out of that
drag and a wheel anywhere zooms), a pad, the keyboard. Nothing binds
by default; `camera2dBindings({ pointer, gamepad?, keyboard? })` is the
standard set (drag and two fingers pan, pinch and wheel zoom, twist
rolls; the left stick and the arrows scroll the VIEW - bound through
invert(), since keys and sticks move the camera where a drag moves the
content - the triggers zoom, the right stick's x rolls) and
`camera2dActions` the declarations:

```tsx
let pointer = createPointerFeed()
feedPointer(view, pointer)
let input = createInputMap(camera2dActions)
input.bind(camera2dBindings({ pointer, gamepad: gamepad() }))
input.drive(cam.axes)
```

Call `cam.update(dt)` from a frame loop (it advances glides, follow,
inertia and the axis rates, pushes one setCamera per driven view when
the pose changed and reports that), gated on the reactive
`cam.active()` (true while a glide, fling, fit, follow or pan gesture
needs frames, or a rate drives; false at rest), read `cam.camera()` for
projectCamera. The camera has NO tap of its own: taps are the
dispatch's (`onTap` on the root with `e.sprite` null is "tap on empty
space"). `<Camera2d input={input}>` inside `<SpriteLayer>` is all of
that wired through context, the 3d `<OrbitCamera>` shape: options read
at mount, driving the nearest view (the `<SpriteLayer>`'s own or the
enclosing `<View2d>`), `viewport` defaulting to that view's size, the
map's `pan`/`zoom`/`roll` actions (or the names in `actions`) driving
the axes, frames only while `active()`. A view's feed normalizes a drag
by the leaf's own box, so a leaf under a designSize fit pans and zooms
correctly. camera2d.ts imports `@solidrt/core/input` only, so
checks/camera2d-check.ts pins the clamp, anchoring, glides, follow,
inertia and the axes headless; examples/camera.tsx (function face) and
examples/pick.tsx (`<Camera2d>`) are the live guards. The shared
vocabulary with @solidrt/3d, one kind and unit per word: `pan` vec2,
`zoom` axis (octaves), `roll` axis (turns) here; `rotate`, `look`,
`move`, `rise` there. The pose options are initial values; every other
option (world, zoom range, pivot, dead zone, the speeds, inertia) is
read where it applies, never snapshotted, so `<Camera2d>` props are
live - forwarded as getters, a bounds or pivot change re-clamping and
pushing the pose at once through `set({})` - and a function-face
caller mutates the options object it passed (the `<OrbitCamera>` rule,
pinned in the check's live-options case). Not yet, all additive:
rotation glides, a contain origin other than center.

### Layer tint

Layer tint (`setTint(rgba)`/the `tint` option and prop, both layer
kinds): one `uTint` shared-params write multiplied over every sprite's
own tint - day/night, a dimmed parallax plane, a fade-in. Cheap to
animate (no record touches), unlike TileLayer's, which re-renders
resident chunks. The same contract as TileLayer.setTint, so one signal
drives a whole scene across layer kinds.

### Views

Views (`layer.createView({ width, height, camera?, oversample?,
clearColor?, label? })`, both layer kinds): the ONLY output a layer
has - the window-filling main view, a minimap, a radar strip, a zoomed
inset, two split-screen panes - @solidrt/3d's scene.createView one
dimension down, as Unity renders a scene only through Cameras and
Godot a World2D only through Viewports. Each view is one draw target
holding one entry over the layer's pipeline and instance buffers
(views.ts): no sprite is mirrored, no record is written twice, and key
order (`orderBy`) comes along, since the core gathers the buffers
themselves at publish - ONE live entry declares the order (the first
view's; when that view is disposed the next live view re-adds its
entry ordered), every other entry reads them sorted. Growth, the
instance count and the layer tint fan out to every view; the camera
and the viewport are the view's own params, so a view costs no
per-frame JS beyond its camera writes. The `ViewHandle` is the
viewport contract (texture, width/height/setSize,
oversample/setOversample, setCamera/camera/project/unproject,
listen/handlers/handlersFor, dispose - views also die with the layer);
the sprites stay the layer's (`addSprite`, `pick`, `setTint`,
`createView`, `dispose`). Pointer events on a view leaf run the
dispatch with the VIEW's camera undone over the layer's pick, the view
as the root of the walk: a sprite under a minimap gets its ordinary
handlers, `view.listen` is the last stop (a tap there with `e.x`/`e.y`
in world pixels is "glide the main camera here"), and
a camera bound to a feed `feedPointer(view, feed)` bridges drives any
view. `<View2d>` is the
component form; the `<SpriteLayer>`'s built-in leaf is a view too, and
`<SpriteLayer output={false}>` has none, showing only through its
`<View2d>` children (examples/split-screen.tsx). examples/views.tsx is
the live guard. Not yet, all additive: tile-layer views, a `layers`
bitmask (a markers-only minimap), `into` tiling, per-view tint, and a
viewport over SEVERAL layers under one camera (their scene-level
camera) - okf/backlog/2d-layer-views-additive.md.

### Retargeted motion

Retargeted motion is NATIVE: `setSpriteTransition(sprite, { position:
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
velocity through retargets, so the chase stays fluid. A mount pose snaps
unless a component's `from` (2d units: `position: { from: [x, y] }`,
`rotation: { from: angle }`, `scale: { from: [w, h] }`, a group's
`from: s`) animates the sprite in from there - once, and only when the
declaration lands in the tick that added the sprite (the `<Sprite>` prop
or a setSpriteTransition right after addSprite; a later one animates
writes only). A component's `exit` (same units) is where it animates to
when destroySprite/destroyGroup lets go of it: the sprite stays drawn
while it leaves and is a GHOST meanwhile - no pick, raycast, overlap,
sweep or pointer event sees it - then frees when the last exit settles
(a bullet never hits a corpse; an entering sprite is live from its
first frame, whatever `from` it passes through). Either endpoint takes
the object form `{ value, duration?, curve?, bounce?, delay? }` to own
its direction's motion (an ease-out enter, an ease-in exit), and `delay`
on an entry holds its writes that long on the animation clock (a late
frame catches up, so a hitch never shifts a held start). No stagger:
sprites have no tree order to cascade in; space a burst with `delay`.
Each natural settle calls the handle's `onTransitionEnd` (plain field,
or the `<Sprite>`/`<Group>` prop) with `{ component }` - target-only,
never on a cancel, snap or exit; the raw "spatialTransitionEnd" engine
event (srt:events, node = sprite.node) stays for flux:spatial consumers.
See examples/springs.tsx (tap a sprite: it leaves through its `exit`).

### Frame-rate motion

Frame-rate motion only JS can compute (physics, flocking) bypasses the
declarative layer: `ref` the sprite, call `setSprite` from `onFrame` (a
~7us core transform write per moved sprite - fine to a few thousand;
past that use the records layer). Signals carry structure and slow
state - a `<Sprite x={sig()}>` re-running 60 times a second works but
re-runs an effect per sprite per frame for nothing.

### Frame animation

Frame animation (animation.ts): `createAnimation(frames, fps, { loop })`
is a clip with a shared wall-clock timer stepping every attached sprite
(`anim.add(sprite)` / `remove`; `play`/`pause`; `loop: false` holds the
last frame and fires `onFinish`, the mixer's name for it in
@solidrt/3d). One timer per playing clip, one setSprite
per sprite per STEP - an 8fps cycle is 8 publishes a second regardless of
display rate, and a paused clip costs nothing. A sprite belongs to at
most one animation (add detaches the previous); removed sprites prune
lazily on the next step. Works on both layer kinds; with `<Sprite>`,
attach via `ref` and leave the `frame` prop off - the clip owns that
field (the prop effect passes absent props as undefined, which setSprite
keeps, so other props stay reactive).

### Pure pieces

frames.ts, pick.ts, camera.ts, camera-motion.ts and dispatch.ts are pure
(no GPU imports) BY DESIGN so they can be checked headless; keep them
that way.

## The baked tile layer (tiles.ts)

### Chunks

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

### Scrolling and the camera

Scrolling never re-bakes: the `<TileLayer>` camera prop (`TileCamera`, an
alias of the shared `CameraUpdate`) is a transform on the composited world
view - the world point (x, y) pinned to the viewport point (pivotX,
pivotY), scaled by zoom, ROTATED by rotation about the pivot. The type IS
the sprite layer's camera type, so one signal drives a whole rotating
scene across both layers (sprites ride the same rotation in-shader);
rotation is the ship-flies-over-the-map camera and costs the same
transform write. A tile
world is FINITE, sized at creation - the contract, not a provisional
limit (recreate to resize; a huge sparse world just picks big numbers,
empty chunks cost nothing). Tiles are data, not
children: there is no `<Tile>` component on purpose - write cells through
`ref` with `setTile`.

### Bulk writes

Bulk writes: `setTiles(col, row, cols, rows, cells)` writes a rect at
once - `cells` row-major, `cols * rows` long, either frames (null clears)
or, with a `frames` table given at creation (`createTileLayer(..., {
frames })`, the `<TileLayer frames>` prop: a tileset, `grid()`'s array,
up to 65534 entries), indices into it (-1 clears; a Uint16Array holds -1
as 0xffff, the same bits) - Unity's SetTilesBlock, Phaser's putTilesAt
over the map's tileset. One locate and one dirty mark per chunk the rect
touches, the per-cell loop inside the layer, the same flush; a chunk the
rect only clears never allocates, so a whole-world write of a sparse
world allocates exactly the chunks the per-cell seed would. A generated
or worker-built world is a Uint16Array of indices, transferable, never an
array of frame objects; a fill is `new Uint16Array(n).fill(i)`. `tint`
in the options applies to every cell set. Every entry is validated before
the first write, so a bad one changes nothing; the table is copied at
creation. The rect form is the shape for worker transfer and chunk
re-fill, not a large speedup: the per-cell floor is the record write
itself, so a 16k-cell solid rect re-writes in ~14 ms against ~25 ms as a
setTile loop and a sparse rect is at parity with the loop, while a first
seed is dominated by chunk allocation at 1-2 ms per chunk.

### Tinting

Tinting, two levels: `setTile(col, row, frame, { tint })` writes the
cell's record tint (same `[r, g, b, a]` 0..1 semantics as a sprite's
tint; absent keys keep their values, a cell set from empty starts at
`[1, 1, 1, 1]`), and `setTint(rgba)` / the `tint` option and prop tints
the whole layer through the shared `uTint` uniform, multiplied over the
per-cell tints - day/night, a dimmed parallax plane. A layer-tint write
is not a record write (chunks re-render GPU-side, nothing re-uploads),
but it does re-render every resident chunk: drive it from slow state,
not per frame. Not built yet: camera-driven residency (bake far chunks
on approach, evict) - okf/backlog/2d-baked-layers.md.

## Components

### Component props

| Component | Props |
|---|---|
| `SpriteLayer` | atlas (TextureId), capacity?, tint? ([r,g,b,a] 0..1, over the whole layer in every view), orderBy?, label?, ref?(layer) - the layer; plus its OWN VIEW, unless `output={false}`: width?, height? (view pixels - both, or neither = FILL: the leaf lays out at 100% of its sized parent and the view follows its box, so view pixels are the leaf's own coordinates; mount-fixed, a function `output` requires explicit sizes, matching `<Scene>` in @solidrt/3d), clearColor?, camera?, oversample?, maxOversample?, viewRef?(view), output? (a function composes the leaf from the texture id; `false` = no own view, the layer shows only through `<View2d>` children and the view props throw), events?, pointer? (a createPointerFeed fed from the view's root, what a map over this view binds; `useSpriteLayer().pointer` inside), onPointer{Down,Move,Up}?, onWheel?, onTap? (the view's root: `event.sprite` is the hit sprite or null over empty space) |
| `Sprite` | x, y (center; local to the enclosing `<Group>`), w, h, frame?, rotation? (radians, clockwise), tint? ([r,g,b,a] 0..1), visible?, transition?, onPointer{Down,Move,Up,Enter,Leave}?, onWheel?, onTap?, ref? |
| `Group` | x?, y?, rotation?, scale? (uniform, scales the subtree), visible? (the whole subtree), transition?, onPointer{Down,Move,Up}?, onWheel?, onTap? (bubbled from hit child sprites), ref? |
| `Camera2d` | createCamera2d's options minus `viewport` (world?, min/maxZoom?, pivot?, deadZone?, panSpeed?, zoomSpeed?, rollSpeed?, followSpeed?, inertia?, x?, y?, zoom?, rotation?), viewport? (`() => { width, height }`, default: the driven viewport's size), input? (the input map driving its `pan`/`zoom`/`roll` axes; live), actions? (action names per axis when the map's differ), ref? - a `<SpriteLayer>` child driving the nearest view's camera (the `<SpriteLayer>`'s own, or inside a `<View2d>` that view) from the map, nothing else; read at mount; throws under `output={false}` outside a `<View2d>` |
| `View2d` | a `<SpriteLayer>` child: one more view of the layer from a camera of its own (layer.createView as a component): width, height (view pixels, live; fixed-size only for now), camera? (partial CameraUpdate on the view's camera, live; the same state a `<Camera2d>` child writes), oversample?, maxOversample? (the auto-pick, as SpriteLayer's), clearColor?, label? (createView's, fixed), ref?(view), output?(texture) (else a built-in `<texture>` leaf at the view size carrying the view's handlers), events?, pointer? (this view's feed, fed from its root), onPointer{Down,Move,Up}?, onWheel?, onTap? (the view's root: `event.sprite` null over empty space); a `<Camera2d>` child drives the VIEW from its map (inside, `useSpriteLayer()` reports the view as `viewport` and the feed as `pointer`); `<Sprite>`/`<Group>` children mount to the layer as outside |
| `TileLayer` | cols, rows, tileW, tileH, atlas (TextureId), frames? (the tileset `setTiles` indices name), chunkClearColor?, filter?, chunkTiles?, tint? ([r,g,b,a] 0..1, over the whole layer), oversample?, maxOversample?, camera? (TileCamera: x, y, zoom, rotation, pivotX, pivotY), label?, ref? |

### SpriteLayer and useSpriteLayer

`SpriteLayer` owns the layer and its own view, rendered as the built-in
`<texture>` leaf carrying the view's pointer handlers (opt out with
`events={false}`; compose yourself with `output`, then spread
`useSpriteLayer().viewport.handlers` onto your leaf; `output={false}` for
no own view at all). `useSpriteLayer()` returns `{ layer, parent,
viewport, pointer }` - the same shape as `useScene()` in `@solidrt/3d` -
where `parent` is the enclosing `<Group>`'s handle (null at the layer
root), so imperative `addSprite(layer, { parent })` mounts where the JSX
sits, `viewport` is the nearest view (the `<SpriteLayer>`'s own, or
inside a `<View2d>` that view), what a `<Camera2d>` drives, and
`pointer` that view's feed (the owner's `pointer` prop, null without
one); read under `output={false}` outside a `<View2d>` `viewport` throws. A FILL-mode
view is 1x1 until the first layout: `ref` fires at mount, before it, so
`viewport.width`/`height` read 1 there. Sprites added and positioned from
`ref` (the pool, the opening scene) are fine - the first layout resizes
the view before the first paint - but anything that needs the real size
(centering on the view) waits for `onLayout`, or works in a `designSize`
that is known up front. `Sprite` renders
nothing - it allocates a record through context and syncs props into it.
`GroupContext` is `createContext<SpriteGroup | null>(null)` on purpose: an
optional parent needs a non-undefined default, since Solid 2 throws on a
resolved `undefined` even when one was passed as the default.

### Pointer events

Pointer events (dispatch.ts, both layer kinds): the DOM event model one
tree deeper, with the VIEW as the root of the walk. Exact rotated-rect
containment, topmost sprite first; down/move/up/wheel dispatch on the hit
sprite, bubble through its enclosing groups and END AT THE VIEW's
listeners (`view.listen({...})`, the `<SpriteLayer>`/`<View2d>`
`onPointer*/onWheel/onTap` props) - over empty space the walk is the view
alone, with `event.sprite` null. `event.sprite` stays the hit,
`currentTarget` the handle whose handler runs (the view at the end),
`stopPropagation()` stops the walk, and a stopped DOWN claims the whole
press: that pointer's move, up and tap never reach the view either (the
chain still bubbles).
That one rule is how a sprite drags itself under a `<Camera2d>` without
the view panning: stop the down, own the captured moves. Capture is per
pointerId to the press target, the view included (a drag from empty
space keeps delivering to the root as it crosses sprites; a drag from a
sprite keeps naming it with live coordinates). Enter/leave fire on the
sprite alone - a group never receives them - while the root sees every
move, so "hovering empty space" is a root move with `sprite` null. Taps
are synthesized by the dispatch (DOM click, Unity's click handler):
`onTap` fires after the up when the press released on the target it
pressed within the slop (8 window px, core's recognizer slop, so a press
is never both a tap and a pan), was the only pointer down for its whole
press (a pinch never taps), with `tapCount` counting repeats within 300
ms and 20 px on the same target (a double tap is `tapCount === 2`).
Wheel walks like a move with `deltaX/deltaY`. Every event carries
`native`, the leaf's element event, for core's recognizers (`createPan`
to drag a sprite with slop, `createTransform` under the camera); a plain
`onPointerMove` also fires on hover, so a sprite drag gates on its own
pressed flag from down to up (`native.button` is set on down/up only).
Root listeners all run, in registration order (the root is the last
stop, nothing is left to claim); a record layer has no groups, so the
chain is the sprite then the root. Event x/y are layer pixels with the
camera undone. checks/dispatch-check.ts pins the walk, claiming, capture,
hover, wheel and tap rules headless.

## Traps

### Atlases and sampling

- The atlas is NOT owned by the layer: layers come and go, atlases usually
  live app-long. Dispose atlases yourself (or let the reactive owner do it -
  createAtlas registers with the owning scope like every core texture).
- `createImage` is the wrong loader for pixel-art atlases: it never forwards
  sampler options, so it is always `filter: "linear"`. `createAtlas` takes
  decoded pixels (`createAtlas(decodeImage(bytes), { filter: "nearest" })`,
  or pixels built in code) and passes the full sampler through (`filter`,
  `wrap`, `mipmap`, `anisotropy`); the record it returns is what `grid` and
  `namedFrames` slice.
- Frames that share an edge do not bleed: the fragment stage clamps every
  sample into its frame, so a sprite at a fractional position (a scrolling
  backdrop, an easing camera) never paints a line of the neighbouring cell,
  and an edge-to-edge sheet needs no gutter and no shaved rect table. The
  one case left is a `mipmap` atlas, whose mip texels average across cell
  edges before sampling: pass `inset` to `grid`/`namedFrames` (2^k texels
  keeps mip level k clean, losing that border) or pack a gutter as
  `spacing`, until extrusion lands (okf/backlog/2d-atlas-extrude.md).
- Tint multiplies the sampled texel (`texture * tint`) and the pipeline
  blends with `blend: "alpha"` in record order, the premultiplied composite.
  The atlas is premultiplied because `decodeImage` premultiplies by default;
  an atlas uploaded from straight-alpha pixels (`decodeImage(bytes, { alpha:
  "straight" })` into `createAtlas`) draws color under transparent texels
  as opaque - the classic "keyed-out backdrop becomes a wash" symptom.
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

### Slots, capacity and order

- `capacity` is a reservation, not a limit, on both layer kinds; reserve
  realistically to skip the growth copies. The pool idiom that fits a
  game: reserve `capacity`, add every sprite at mount with `visible:
  false`, then show/hide - a hidden sprite keeps its slot, so draw order
  stays stable and a spawn is one setSprite, never an add. Keep a free
  list of hidden handles; scanning the pool for one is the cost that
  shows up first at a few hundred entities. On the records layer, do not
  cache `layer.records` across addSprite - growth replaces the array and
  a hoisted reference becomes a dead copy whose writes publish nothing.
  `layer.withRecords(fn)` is the hoist-proof read; a bare `layer.records`
  at use time is equally live.
- Records layer: record order is draw order: `destroySprite` shifts every
  later sprite down one slot (copyWithin + index fixup, O(later
  sprites)). Its flush publishes the WHOLE live prefix, not a dirty
  range: one moved sprite re-publishes count x 52 bytes - a single
  memcpy, microseconds at 10k; the node layer's style publish is the same
  whole-prefix shape. Dirty ranges were deliberately not built until a
  measurement asks.
- `orderBy` on BOTH layers: the core gathers publishes into key order
  (gpu `instanceOrder`, radix sort + one extra memcpy, no per-record JS),
  so a y-sorted crowd costs the same flush as an unsorted one. Records
  and slots stay stably addressed; ties keep slot order; `pick()` still
  resolves overlap by slot/record order. Records layer keys: "y" or a raw
  `{ field, descending? }` offset into the 13-float record. Node layer
  keys: `"y"` - WORLD y from the core-written pose buffer, so sprites
  moved by native transitions (or any core producer) re-sort with zero JS
  per frame - or `"renderOrder"` - the app-owned per-sprite `renderOrder`
  field (style record float 8, default 0; @solidrt/3d's name for the same
  knob), the explicit-layering key for painter-order scenes: raise a
  dragged piece with `setSprite(hit, { renderOrder: ++top })`, back to 0
  to restore. Either way the core gathers
  pose AND style under ONE permutation and republishes the sibling buffer
  itself when the key buffer re-orders (the multi-buffer stage of
  okf/backlog/gpu-instance-order.md). `renderOrder` on a record-layer
  sprite throws - its 13-float record has no key field.
- The node layer's STYLE slots are not compacted: a removed sprite leaves
  its style floats in place (invisible - the pose is zeroed) until the
  slot recycles. Do not read style truth from the buffer; getSprite reads
  the JS mirror.
- Sprite handles go inert on removal (`sprite.layer === null`); setSprite on
  an inert handle is a silent no-op (matching the throw-in-dev policy would
  mean throwing, but removal racing a queued pointer event is routine, not
  a bug).
- The `<Sprite>` effect syncs ALL seven fields when ANY prop changes (one
  effect, one tuple). Fine at component scale; if a profile ever blames it,
  split the effects before inventing anything cleverer.

### Nodes, transitions and queries

- Node layer: `sprite.node` is public FOR BINDING PRODUCERS, not for
  lifecycle - never destroyNode it yourself (destroySprite owns that), and
  a transform written through flux:spatial directly bypasses the sprite's
  pose mirror, so a later setSprite with the old x wins (its compare sees
  no change to skip, but partial writes compose from the mirror).
- With a transition set, the sprite's fields (and getSprite) read the
  TARGET, not the mid-flight pose - the JS mirror is what setSprite
  composes partial writes from, and targets are the right thing to
  compose. Picking, the pose buffer and `worldPosition` see the actual
  mid-flight pose (what is on screen). Clearing the transition (null) keeps the
  mid-flight pose on the node while the mirror still holds the old
  target: the next setSprite write snaps to whatever it says.
- Node layer picking reads the index as of the last core flush; every
  query runs the layer's pending batch first, so write-then-query in one
  tick is coherent. Producers moving nodes between flushes are one frame
  stale to picking, like every query.
- The queries are per-body answers - a hit test, a marquee, a blast
  radius, one mover call per character - not a collision broadphase. A
  bullets-vs-crowd test at frame rate is hundreds of core queries per
  frame, and it is simulation logic anyway, which lives in plain
  TypeScript with no layer in sight (core's "keep the simulation out of
  the renderer"): the app's own arrays, a uniform grid over them,
  runnable headless. Tile-layer collision (tiles are baked records, not
  nodes) is a solid-cell grid query of its own, not the sprite index.

### Cameras and rotation

- Every rotation must agree on direction (clockwise, y-down):
  `pointInSprite` in pick.ts with the vertex stage's `iRot`, and
  `projectCamera` in camera.ts with `uCameraRot` and `<TileLayer>`'s view
  transform. The differential checks (pick-check.ts, camera-check.ts)
  guard the JS math against oracles but NOT against the shader - if you
  touch one rotation, touch all, then run examples/camera-probe.tsx (the
  live guard: shader vs projectCamera, node/record parity, the pointer
  round trip) and watch for CAMERA-OK.
- Retro scrolling habits are the app's, not the layer's: keep the camera
  fractional (rounding it to design pixels makes motion step at the
  rounding rate, not the frame rate), and a game that wants whole-pixel
  scrolling snaps its own camera.
- The sprite layer's camera rotation is IN-SHADER (uCameraRot) on
  purpose: rotating the layer's OUTPUT leaf instead is wrong - the output
  is viewport-sized with the camera already applied, so a leaf transform
  spins the cropped viewport and the corners cut. The tile layer gets
  away with the transform-on-the-leaf camera only because its composited
  view is the WORLD, not a viewport of it.

### Tile layer

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
- A dirty chunk flush publishes and re-bakes that chunk in full. Fine on
  change-only cadence; per-frame setTile churn re-bakes chunks per frame -
  that is sprite-layer work, not tile work.
- Chunk allocation is MONOTONIC: nothing evicts, so texture memory is
  proportional to the touched area (~920KB per resident chunk at the
  default size) and every resident chunk keeps a composited leaf. Bounded
  worlds only; streaming/infinite is stage B2 in
  okf/backlog/2d-baked-layers.md.
