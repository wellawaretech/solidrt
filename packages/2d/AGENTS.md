# @solidrt/2d - agent notes

An instanced sprite layer above `@solidrt/core/gpu`: a layer's atlases
bound together, N quads from any of them in ONE draw call, composited into
the app as an ordinary `<texture>`
leaf. The live layer backs every sprite with a SPATIAL ARENA node (never a
rendertree element - `d-texture` sprites are the right tool up to the low
thousands; measured ~0.65us paint and ~15KB memory per NODE, and every
moved node is two setProperty FFI calls per frame). The node makes sprites
citizens of the spatial core: native producers (node transitions, animation
clips, physics) reach them through `sprite.node`, hierarchy recomputes
moved subtrees in Rust, and picking walks the core BVH.

The files in agents/ carry the depth this one leaves out, each topic with
its own traps; read the one that matches the work before starting it:
- agents/materials.md - Sprite materials and their shaders. Read before
  writing or tuning a sprite material.
- agents/records.md - The records layer: JS-owned float records per
  sprite, raw writes and the JS pick walk. Read before choosing
  createRecordLayer over the node-backed layer.
- agents/views.md - Camera control and input bindings, multiple views, and
  streaming regions. Read before moving the camera from input, showing a
  layer in more than one view, or streaming a large world.
- agents/motion.md - Retargeted (native) motion, frame-rate motion and
  frame animation. Read before animating a sprite.
- agents/text.md - Text runs drawn as sprites. Read before drawing text
  inside a sprite layer.
- agents/tiles.md - The baked tile layer (tiles.ts): chunks, residency,
  cells, views, wrap-around worlds, bulk writes and tinting. Read before
  building a tile map.

Contents:

- [The model](#the-model)
  - [Three faces](#three-faces)
  - [A layer renders through its views](#a-layer-renders-through-its-views)
  - [Atlases and frames](#atlases-and-frames)
  - [Pose, sprite and style slots](#pose-sprite-and-style-slots)
  - [Fixed instance slots](#fixed-instance-slots)
  - [Growth](#growth)
  - [Picking](#picking)
  - [Spatial queries](#spatial-queries)
  - [Groups](#groups)
  - [Visibility](#visibility)
  - [Flips](#flips)
  - [Mutations batch to the frame](#mutations-batch-to-the-frame)
  - [Layer space](#layer-space)
  - [The camera](#the-camera)
  - [Layer tint](#layer-tint)
  - [Pure pieces](#pure-pieces)
- [Components](#components)
  - [Component props](#component-props)
  - [SpriteLayer and useSpriteLayer](#spritelayer-and-usespritelayer)
  - [Pointer events](#pointer-events)
- [Traps](#traps)
  - [Atlases and sampling](#atlases-and-sampling)
  - [Slots, capacity and order](#slots-capacity-and-order)
  - [Nodes, transitions and queries](#nodes-transitions-and-queries)

## The model

### Three faces

THREE faces, layered: the node-backed live layer (layer.ts:
`createSpriteLayer`/`addSprite`/`setSprite`/`destroySprite` plus
`addGroup`/`setGroup`/`setSpriteParent`/`setSpriteTransition`/
`setGroupTransition` - plain objects, no signals, usable without
components), the records layer (records.ts: `createRecordLayer` - the
raw escape hatch, agents/records.md), and the component face (components/:
`SpriteLayer`/`Sprite`/`Group`/`View2d`/`Camera2d` over context).

### A layer renders through its views

A layer holds sprites and renders NOTHING by itself: it shows through
its VIEWS (`layer.createView({ width, height, camera?, oversample?,
clearColor?, label? })`, `<View2d>`, or the `<SpriteLayer>`'s own leaf,
which is one), each a draw target with a camera, a size and pointer
dispatch of its own - a Unity scene renders only through Cameras, a
Godot World2D only through Viewports, and there is no privileged first
one. The window view, the minimap, two split-screen panes are the same
`ViewHandle`. Details under Views in agents/views.md.

### Atlases and frames

A layer draws from the ATLASES it declares at creation
(`createSpriteLayer([a, b], ...)`, `<SpriteLayer atlases={[a, b]}>`, the
record and tile layers alike): `createAtlas` records, or
`{ texture, width, height }` literals for a render target or a camera
frame, up to `limits.maxTextureUnits` of them (16 on every GLES 3.0
device), of any sizes and sampler states. They bind together as ONE
draw per view - the multi-texture batch of PixiJS and Phaser, where
Unity and Godot break the batch on every texture change - so sprites
from several sheets interleave freely in draw order, key order
included, and a tile world bakes from several tilesets in one pass. A
`Frame` carries its texture (`{ texture, u0, v0, u1, v1 }`, stamped by
`grid`/`namedFrames`/`fullFrame`; PixiJS's Texture, Phaser's Frame,
Unity's Sprite and Godot's AtlasTexture pair the texture with the rect
the same way), the record stores that texture's index in the layer's
list (`layer.atlases`, what a raw record writer stores), and the
fragment SET is GENERATED per layer with one sampler per atlas, the
flat per-instance index picking it (`spriteSource` in `@solidrt/2d/glsl`,
prepended to whatever fragment the layer's material carries):
derivatives are taken from the unclamped uv before the branch and the
tap is `textureGrad`, so mip selection is the quad's own and well
defined inside the branch. A frame
whose texture the layer did not declare THROWS at the write (the
wrong-sheet bug, caught instead of drawn); the frame left out of
`addSprite` is the whole FIRST atlas. An atlas declared `sdf: { range }`
(a sprite font's msdf atlas, or any distance field you upload) is
decoded in its branch instead of sampled as colour: the median of rgb is
the edge, anti-aliased over one screen pixel at whatever zoom, the true
field in alpha grows a per-sprite outline (text runs set it; a colour
atlas ignores the field). Fixed at creation like the list itself.

### Pose, sprite and style slots

Node layer ownership split, instance-buffer slots on one pipeline (the
material's; two materials are two layers): slot 0 is the POSE buffer
`[x, y, angle, sx, sy]` written ONLY by the core (each sprite node's
Pose2D record sink; one coalesced buffer write per flush however many
nodes moved), slot 1 the SPRITE record `[u0, v0, u1, v1, tint rgba,
renderOrder, minScreenPx, maxScreenPx, atlas, outline rgb,
outlineWidth]` (`SPRITE_FLOATS`, 16), JS-owned through the sprite verbs
and published through the zero-copy write lease, and after them the
material's STYLE streams, one per instance buffer it declares (app-owned
per-sprite data, agents/materials.md). NEVER write the pose buffer from JS -
the core's staging mirror owns it and will overwrite.

### Fixed instance slots

Sprites hold FIXED instance slots: draw order is slot order, removal
zeroes the pose (zero scale = nothing drawn) and recycles the slot to
the next add. No painter's-insertion-order guarantee across removals;
opaque-or-transparent pixel art never notices, and a scene that needs a
real draw order says so with `orderBy` ("y" or "renderOrder" - see below).

### Growth

Growth (past `capacity`, doubling): pose sinks move in ONE core
`retargetRecords` call (full republish next flush), style re-uploads,
`setDrawBuffers({ buffers })` swaps both, old buffers destroyed.

### Picking

Picking is the core index: `pick` raycasts [x, y, -1] along +z (exact
rotated-rect via the node's local box) and returns ALL hits topmost
first (highest slot) - the all-hits shape of @solidrt/3d's pick, so
`pick(x, y)[0]` is the topmost; `pickRect` is `overlap` over an
unrotated rect, sprites only (exact for rotated sprites, the marquee),
unordered. Every query passes the core a filter with the layer's root
node (sprites and groups without a parent hang off it), so the arena
being shared with e.g. a 3d scene costs nothing in JS.

The screen-size clamp bends this one way. A sprite with `minScreenPx`
and/or `maxScreenPx` draws its smaller axis within those VIEW pixels
(scaled uniformly, in the vertex stage over the core-written pose, so
after group scale and mid-transition; core's `screenSizeScale`, the
same function @solidrt/3d's sprite material uses), which differs from
its index column once the zoom carries it past a bound - larger under
a floor, smaller under a ceiling; equal bounds hold a constant screen
size. So `pick(x, y, zoom)` takes the camera zoom the clamps are
measured at (default 1): while no sprite on the layer is clamped it is
the raycast above; once one is, the candidates are the columns
containing the point (the ray) plus, when a floor is on, those within
the widest floor's reach of it (`floorReach` in pick.ts over the
clamped sprites, recomputed on the next pick after one leaves or
shrinks; the box overlap is a SURFACE contact in the core, so a box
inside a big column finds nothing, which is why the ray stays in the
union), and each is tested exactly against its drawn rect from the
node's world matrix (`screenSizeScale` + `pointInSprite`). The pointer
walk and `view.pick(x, y)` pass the view camera's zoom; a bare
`layer.pick(x, y)` is the unzoomed world. `pickRect`, overlap, sweep and
the mover stay world-space body queries and never see a clamp. The
record layer's JS walk applies the same scale per record. The pair is
validated against the STORED bounds before any write (a ceiling set
alone may not drop below the floor already there).

### Spatial queries

Spatial queries (`tests/collision.test.tsx` pins the contract): the
3d scene's trio one dimension down, Godot's PhysicsDirectSpaceState2D
and CharacterBody2D in their names. `layer.raycast(x, y, dx, dy,
opts?)` is every shown sprite the ray strikes, nearest first, with
distance, edge point and edge normal; `layer.overlap(volume, opts?)`
every sprite a volume touches with its deepest contact `{ sprite,
point, normal, depth }`, unordered; `layer.sweep(volume, dx, dy,
opts?)` every sprite the moving volume first touches `{ sprite, time,
point, normal }`, earliest first; `layer.moveAndSlide(volume, dx, dy,
opts?)` (and the free `moveAndSlide(layer, ...)` spelling of it, as
@solidrt/3d's) the character mover over them, one core call per body per
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
hidden (`visible: false`). `destroySprite(sprite, { snap: true })` /
`destroyGroup(group, { snap: true })` free NOW, playing no declared
exit (the declaration is cleared first, so the core frees at once and
the slot recycles this flush): a region's teardown, a level reset,
anything torn down off screen - Unity's deferred Destroy under an exit
animation is how streamed objects outlive their cell.
A handle's x/y are local to its group; `worldPosition(sprite | group)`
reads the layer-pixel position composed through every enclosing group
from the core's world matrix (@solidrt/3d's worldPosition) - under a
transition the pose of this frame, since native motion steps before the
frame's JS. A re-parent (`setSpriteParent`, `setGroup`'s `parent`)
keeps the LOCAL fields by default, so the handle moves with the frame
change; `{ keepWorld: true }` keeps the WORLD pose instead (Unity's
SetParent with worldPositionStays), rewriting x, y, rotation and w/h
(a group's scale) in the new frame from the core's world matrices as
the tree stands now - a passenger stepping off a moving platform, a
sprite re-homed out of a leaving region. The kept pose is this frame's:
under a transition in flight the mid-flight pose is kept and the rest
of the motion dropped (its target was spelled in the old frame; the
declaration stays), so write the target again if it still applies.

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

### Mutations batch to the frame

Mutations batch: style lease publish + count setDraw + `spatial.flush()`
go out once, in the publish pass of core's `onBeforeRender` ahead of the
frame's paint, so a write made anywhere in the frame's JS (onFrame, an
effect, an onLayout handler) is in that frame's picture, never the next;
a write made outside a frame (an event handler, a timer) publishes at
the microtask. No mutation, no publish, no frame: a static layer costs
zero, the same demand-gate story as the rest of the platform.

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
world (layer) pixels. The camera point is written RELATIVE to the
layer's RENDER ORIGIN, and the pose records are relative to the same
point (flux:spatial setInstanceOrigin): the arena keeps sprite
positions in f64, the pose buffer is f32, so a world ten million
pixels across would step in whole pixels without it. The origin snaps
near the PRIMARY view's camera (the first live view, the one that
declares the key order) on a power-of-two cell of 65536 of its screen
pixels (views.ts); a camera crossing a cell restages the records in one
coalesced write and rewrites every view's camera. One buffer, one
origin: the other views subtract the same point, exact while they stay
within about 10^5 px of it - two views further apart than that over one
layer (a split screen across a continent) want a layer each. The
records layer keeps its origin at zero: its records are the app's,
written as world f32, so it steps past a few million pixels.
Picking itself works in world space, in f64, and never
sees a camera. `view.camera()` reads the camera back (every field
present - the `CameraState` snapshot, @solidrt/3d's scene.camera())
and `view.project(x, y)` / `view.unproject(x, y)` are the two pure
functions over it.

### Layer tint

Layer tint (`setTint(rgba)`/the `tint` option and prop, both layer
kinds): one `uTint` shared-params write multiplied over every sprite's
own tint - day/night, a dimmed parallax plane, a fade-in. Cheap to
animate (no record touches), unlike TileLayer's, which re-renders
resident chunks. The same contract as TileLayer.setTint, so one signal
drives a whole scene across layer kinds. The general form is
`setParams` (agents/materials.md): the tint is the one param every material
takes.

### Pure pieces

frames.ts, extrude.ts (the mip-gutter repack, tests/extrude.test.ts),
pack.ts (the shelf packer, tests/pack.test.ts), pick.ts, camera.ts,
dispatch.ts, oversample-math.ts, tiles-math.ts (the
tile grid's chunk and slot math, tests/tiles.test.ts) and text-layout.ts
(glyph placement over prepared units, tests/text-layout.test.ts) are pure
(no GPU imports) BY DESIGN so they can be checked headless; keep them
that way.

## Components

### Component props

| Component | Props |
|---|---|
| `SpriteLayer` | atlases (Atlas[], the sheets bound as one draw; mount-fixed) or layer (an existing createSpriteLayer handle to adopt: the component shows and populates it, keeps only the view and the leaf as its own and leaves the layer's lifetime to its maker, so `<Sprite>`/`<Text2d>` sit over an imperative layer; the creation props atlases/capacity/material/orderBy then throw), capacity?, material? (the layer's material, `unlit()` default, `unlit({ blend: "add" })` for glows; mount-fixed), params? (layer.setParams as a prop, over the material's own; live, merge semantics), tint? ([r,g,b,a] 0..1, over the whole layer in every view), orderBy?, stagger? (ms, the layer root's enter/exit spacing), label?, ref?(layer) - the layer; plus its OWN VIEW, unless `output={false}`: width?, height? (view pixels - both, or neither = FILL: the leaf lays out at 100% of its sized parent and the view follows its box, so view pixels are the leaf's own coordinates; mount-fixed, matching `<Scene>` in @solidrt/3d), clearColor?, camera?, oversample?, maxOversample?, maxPixels? (a cap on the view target's texel count under the auto pick, the budget form of maxOversample: one cost on every display; the same name on `<TileLayer>` and the 3d `<Scene>`), viewRef?(view), output? (a function `(texture, leaf)` composes the leaf from the texture id: `ref={leaf.ref}` makes a fill view follow your leaf, `{...leaf.handlers}` routes sprite events through it scaled to its layout, `leaf.size()` (the target's texels, view pixels times oversample) and `leaf.deviceSize()` are reactive reads a post chain sizes from, applied before the paint - the same contract as the 3d `<Scene>`'s; `events` then throws; `false` = no own view, the layer shows only through `<View2d>` children and the view props throw), events?, pointer? (a createPointerFeed fed from the view's root, what a map over this view binds; `useSpriteLayer().pointer` inside), onPointer{Down,Move,Up}?, onWheel?, onTap? (the view's root: `event.sprite` is the hit sprite or null over empty space) |
| `Sprite` | x, y (center; local to the enclosing `<Group>`), w, h, frame?, rotation? (radians, clockwise), tint? ([r,g,b,a] 0..1), minScreenPx?, maxScreenPx? (the screen-size clamp on the smaller axis, view pixels; 0 = off; equal = constant size), style? (the sprite's style record under the layer's material, one value per component of its first instance buffer; live), visible?, transition?, onPointer{Down,Move,Up,Enter,Leave}?, onWheel?, onTap?, ref? |
| `Group` | x?, y?, rotation?, scale? (uniform, scales the subtree), visible? (the whole subtree), transition?, onPointer{Down,Move,Up}?, onWheel?, onTap? (bubbled from hit child sprites), ref? |
| `Camera2d` | createCamera2d's options (world?, min/maxZoom?, pivot?, follow?, offset?, panSpeed?, zoomSpeed?, rollSpeed?, damping?, inertia?, x?, y?, zoom?, rotation?), input? (the input map driving its `pan`/`zoom`/`roll` axes; live), actions? (action names per axis when the map's differ), ref? - a `<SpriteLayer>` child driving the nearest view's camera (the `<SpriteLayer>`'s own, or inside a `<View2d>` that view; its `size()` the viewport) from the map, nothing else; read at mount; throws under `output={false}` outside a `<View2d>` |
| `View2d` | a `<SpriteLayer>` child: one more view of the layer from a camera of its own (layer.createView as a component): width?, height? (view pixels, live - both, or neither = FILL exactly as SpriteLayer's own view: the leaf fills its sized parent and the view sizes to that box), camera? (partial CameraUpdate on the view's camera, live; the same state a `<Camera2d>` child writes), oversample?, maxOversample?, maxPixels? (the auto-pick, as SpriteLayer's), clearColor?, label? (createView's, fixed), ref?(view), output?(texture, leaf) (as SpriteLayer's; else a built-in `<texture>` leaf at the view size carrying the view's handlers), events?, pointer? (this view's feed, fed from its root), onPointer{Down,Move,Up}?, onWheel?, onTap? (the view's root: `event.sprite` null over empty space); a `<Camera2d>` child drives the VIEW from its map (inside, `useSpriteLayer()` reports the view as `viewport` and the feed as `pointer`); `<Sprite>`/`<Group>` children mount to the layer as outside |
| `Region` | x, y, width, height (the box, in the enclosing `<Group>`'s frame), enter, leave, weight?, priority?, retain?, label? (createRegion's, mount-fixed), load?, build (the content JSX, mounted under the region's content group with the loaded data), unload?, proxy? (JSX under the region's group while not shown), onStateChange?, onError?, ref?(region) - an explicit region (Regions in agents/views.md); move it through `ref` with setGroup on `region.group` |
| `RegionGrid` | cell, origin?, enter?, leave?, bounds?, weight?, limit?, priority?, retain?, label? (createRegionGrid's, mount-fixed), load?, build (per cell, mounted under the cell's content group), unload?, proxy?, onStateChange?, onError?, ref?(grid) - a region grid at the layer root (throws under a `<Group>`) |
| `TileLayer` | tileW, tileH, atlases (Atlas[], the tilesets bound as one bake; mount-fixed) or layer (an existing createTileLayer handle to show again under a camera of its own, a minimap: the component adds a view and its leaf, the layer's lifetime, cells and hooks stay its maker's, so the creation props and onEnter/onLeave throw), frames? (the tileset `setTiles` indices name), chunkClearColor?, material? (the bake's material, `unlit()` default; mount-fixed), params? (layer.setParams as a prop; live, each change re-bakes), chunkTiles?, tint? ([r,g,b,a] 0..1, over the whole layer), bounds?, wrap?, overhang?, enter?, leave?, priority?, retain?, fill?, pool?, maxTexels?, density? (pin the bake density; live), maxDensity?, mipmap? (TileLayerOptions, creation-fixed; with `layer` they are its maker's), onEnter?, onLeave? (the layer's residency hooks), label?, ref?(layer) - the layer; plus its OWN VIEW, exactly `<SpriteLayer>`'s: width?, height? (view pixels - both, or neither = FILL the parent's box; mount-fixed), camera? (TileCamera: x, y, zoom, rotation, pivotX, pivotY; the view's camera and the gate's viewer; live, partial), clearColor? (the ground every chunk draws over), oversample?, maxOversample?, maxPixels? (the view's auto pick), viewRef?(view), output?(texture, leaf), events?, pointer?, onPointer{Down,Move,Up}?, onWheel?, onTap? (the view's root, every event over the world: `event.sprite` is always null); a `<Camera2d>` child drives the view |

### SpriteLayer and useSpriteLayer

`SpriteLayer` owns the layer (or adopts an existing one through `layer`,
owning only what follows) and its own view, rendered as the built-in
`<texture>` leaf carrying the view's pointer handlers (opt out with
`events={false}`, which throws at mount together with a `pointer` feed -
the feed would listen at a root no event reaches; the same on `<View2d>`;
compose yourself with `output={(texture, leaf) => ...}`, `ref={leaf.ref}`
on your leaf so a fill view follows it and `{...leaf.handlers}` so sprite
events route through it scaled to its layout, the view's size and
oversample following the ref'd leaf as they follow the built-in one
(components/fill.ts, shared with `<View2d>`, which fills by the same
rule); `output={false}` for no own view at all). `useSpriteLayer()` returns `{ layer, parent,
viewport, pointer, viewportSize }` - the same shape as `useScene()` in
`@solidrt/3d` - where `parent` is the enclosing `<Group>`'s handle (null at the layer
root), so imperative `addSprite(layer, { parent })` mounts where the JSX
sits, `viewport` is the nearest view (the `<SpriteLayer>`'s own, or
inside a `<View2d>` that view), what a `<Camera2d>` drives, and
`pointer` that view's feed (the owner's `pointer` prop, null without
one), and `viewportSize` that view's size in view pixels as a signal
(`viewport.size()` reactive: a fill view's follows its leaf); read under
`output={false}` outside a `<View2d>` `viewport` and `viewportSize`
throw. `events` wires the built-in leaf only and throws with a function
`output`. A FILL-mode
leaf is size-contained (`contain="size"`, the CSS layout prop every
element takes): it adds nothing to its ancestors' sizes, exactly as an
empty `<view>` at 100% would, so a column of header, layer pane
(`flexGrow: 1`) and footer needs no `minHeight={0}`, and a pane with no
definite height does not come out square. The leaf then has no size of
its own at all: a parent with no size leaves it empty, the view is
invisible, and the component warns once with the stack of the component
that built it, naming the fix (a size, or flexGrow in a sized column). A
FILL-mode view is 1x1 until the first layout: `ref` fires at mount,
before it, so `viewport.size()` reads 1x1 there. Sprites added and positioned from
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
are synthesized by the dispatch, as DOM click (Unity's click handler
fires for every button; ours, like DOM's, for the primary alone): `onTap`
fires after the up when a PRIMARY-button press (a right click walks its
down and up with `button` 2 but never taps, like core's press; read
`button` on the down to act on it) released on the target it pressed
within the slop (8 window px, core's recognizer slop, so a press is never
both a tap and a pan), was the only pointer down for its whole press (a
pinch never taps), with `tapCount` counting repeats within 300 ms and 20
px on the same target (a double tap is `tapCount === 2`).
Wheel walks like a move with `deltaX/deltaY`. Every event carries
`timeStamp` and `predicted` as the element event has them (time input
with the first; a `predicted` move is one to draw with, not to measure
with - core's AGENTS.md has both), and
`native`, the leaf's element event, for core's recognizers (`createPan`
to drag a sprite with slop, `createTransform` under the camera); a plain
`onPointerMove` also fires on hover, so a sprite drag gates on its own
pressed flag from down to up (`native.button` is set on down/up only).
Root listeners all run, in registration order (the root is the last
stop, nothing is left to claim); a record layer has no groups, so the
chain is the sprite then the root. Event x/y are layer pixels with the
camera undone. tests/dispatch.test.ts pins the walk, claiming, capture,
hover, wheel and tap rules headless.

## Traps

### Atlases and sampling

- The atlases are NOT owned by the layer: layers come and go, atlases
  usually live app-long. Dispose atlases yourself (or let the reactive
  owner do it - createAtlas registers with the owning scope like every
  core texture).
- `createImage` is the wrong loader for pixel-art atlases: it never forwards
  sampler options, so it is always `filter: "linear"`. `createAtlas` takes
  decoded pixels (`createAtlas(decodeImage(bytes), { filter: "nearest" })`,
  or pixels built in code) and passes the full sampler through (`filter`,
  `wrap`, `mipmap`, `anisotropy`); the record it returns is what `grid`,
  `namedFrames` and `fullFrame` slice, and what a layer's `atlases` list
  holds.
- A frame from a sheet the layer did not declare THROWS at the write
  (addSprite, setSprite, setTile, setTiles, a tile layer's `frames`
  table), naming the layer's atlases: a sprite never silently draws from
  the wrong sheet. The same frames serve every layer that declares their
  atlas (a scene layer and its additive glow layer over the same art).
  Raw record writers store the texture's index in `layer.atlases`.
- The sampler cap: a layer binds at most `limits.maxTextureUnits` atlases
  (16 on every GLES 3.0 device, the whole pass's budget - a layer's
  targets bind nothing else). Past it, pack sheets together; runtime
  images need the packer (okf/backlog/2d-atlas-runtime-packing.md).
- Frames that share an edge do not bleed: the fragment stage clamps every
  sample into its frame, so a sprite at a fractional position (a scrolling
  backdrop, an easing camera) never paints a line of the neighbouring cell,
  and an edge-to-edge sheet needs no gutter and no shaved rect table. A
  `mipmap` atlas is different: its mip texels average across cell edges
  BEFORE sampling, and no UV-side trick undoes that. Extrude the sheet
  before `createAtlas` - `extrudeGrid(image, cols, rows, gutter)` for a
  uniform sheet, `extrudeRects(image, rects, gutter)` for a hand-packed
  one: every cell is copied into a gutter of its own replicated edge
  pixels, the result slices with the returned `options`/`rects`, and mip
  levels 0 through log2(gutter) sample clean (TexturePacker's "Extrude",
  Unity's "Extrude Edges"). The rules: `gutter` a power of two and every
  cell size a multiple of it, so cell edges land on mip texel edges (the
  calls throw otherwise, naming the fix); the sheet grows by 2 x gutter
  per cell per axis; deeper levels mix again, so size the gutter to the
  smallest scale the sheet is drawn at (gutter 4 covers down to a
  quarter). Load-time CPU work, a few milliseconds for a 1024-square
  sheet.
- The colour contract: tint multiplies the atlas texel as stored -
  premultiplied sRGB, no decode - and the layer tint multiplies over
  that, so a tint of `[0.5, 0.5, 0.5, 1]` halves the ENCODED value (about
  a fifth of the light, not half). Right for an unlit pipeline (the
  texel is the author's colour and comes out as it went in), but it is
  not @solidrt/3d's contract, whose color.ts documents sRGB in, linear
  shading: the same tint triple is a different brightness in the two
  packages, so a palette shared across them needs its own conversion.
- Tint multiplies the sampled texel (`texture * tint`) and the pipeline
  blends with the material's `blend` in record order - "alpha" by
  default, the premultiplied composite; "add" accumulates (glows,
  sparks, light cones: two half-alpha whites make opaque white),
  "multiply" darkens, "none" overwrites. Pipeline state, so it is per
  layer (`material: unlit({ blend: "add" })`) and fixed at creation; an
  additive effect layer draws over the scene layer.
  The atlas is premultiplied because `decodeImage` premultiplies by default;
  an atlas uploaded from straight-alpha pixels (`decodeImage(bytes, { alpha:
  "straight" })` into `createAtlas`) draws color under transparent texels
  as opaque - the classic "keyed-out backdrop becomes a wash" symptom.
- Two samplers, two jobs. The ATLAS sampler (createAtlas `filter`) decides
  whether texels are hard blocks ("nearest", pixel art) or smooth
  ("linear"). A VIEW's target sampler (and the tile view's sampling of
  the chunk bakes) does the composite resample to the box, and stays
  linear: "nearest" there snaps texels to uneven widths at any
  fractional scale (a 3.6x designSize fit draws source pixels 3 or 4
  device pixels wide - shimmer standing still, boil when scrolling).
  Proper resampling at a fractional or HiDPI scale is the `oversample`
  factor: the view renders at n texels per view pixel (nearest inside
  keeps blocks square), the linear composite spreads the fraction over
  one device pixel at block edges. The components pick n every layout
  from their leaf's on-screen box (which composes designSize fits and
  layout scaling), shrinking only past a margin so an oscillating
  measurement cannot re-render in a loop, within a budget of the
  window's own device pixel count; the primitives default to 1 and take
  `{ oversample }` / `setOversample(n)` - with `output` there is no
  built-in leaf, so set it yourself with the exported `fitOversample`.
  The tile layer's bake has the same pick one level down: its `density`
  follows the views' oversample times zoom, and `maxDensity` is the cap
  that bounds a world's texture memory (resident chunks x n squared; a
  2x display otherwise picks 4x the memory of n = 1, silently). A layer
  whose oversample or density changes more than a few times in a second
  warns in the console (thrash: every change resizes and redraws the
  targets, a tile layer re-bakes every resident chunk) - pin the knob or
  cap it when an animated transform or camera legitimately sweeps the
  scale. Never fix a shimmer by snapping the fit to an integer: the
  scene should fill its box at any ratio.

### Slots, capacity and order

- `capacity` is a reservation, not a limit, on both layer kinds; reserve
  realistically to skip the growth copies. The pool idiom that fits a
  game: reserve `capacity`, add every sprite at mount with `visible:
  false`, then show/hide - a hidden sprite keeps its slot, so draw order
  stays stable and a spawn is one setSprite, never an add. Keep a free
  list of hidden handles; scanning the pool for one is the cost that
  shows up first at a few hundred entities. On the records layer, do not
  cache `records(layer)` across addSprite - growth replaces the array and
  a hoisted reference becomes a dead copy whose writes publish nothing;
  read it at use time (the top of the frame's step is the place).
- Records layer: record order is draw order: `destroySprite` shifts every
  later sprite down one slot (copyWithin + index fixup, O(later
  sprites)) and marks the shifted records dirty. The flush publishes the
  DIRTY record range (one buffer write at its byte offset, clipped to
  the live sprites), or under `orderBy` the drawn prefix whole through
  the lease - so a moved sprite costs its 80 bytes on a plain layer and
  count x 80 bytes on an ordered one. The node layer's style publish is
  still the whole-prefix shape (a boolean dirty flag over the high-water
  mark), the one place a range is not yet tracked.
- `orderBy` on BOTH layers: the core gathers publishes into key order
  (gpu `instanceOrder`, radix sort + one extra memcpy, no per-record JS),
  so a y-sorted crowd costs the same flush as an unsorted one. Records
  and slots stay stably addressed; ties keep slot order; `pick()` still
  resolves overlap by slot/record order. Records layer keys: "y" or a raw
  `{ field, descending? }` offset into the 16-float record. Node layer
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
  sprite throws - its 16-float record has no key field.
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
- A sprite is a SOLID to overlap, sweep and moveAndSlide: a volume whose
  center lies inside it is pushed out of the nearest edge (its column
  is deep, so never along z), where a mesh in @solidrt/3d is a surface.
  The push-out depth carries the whole volume clear; a body teleported
  deep into a wide sprite comes out of the nearest side, which may not
  be the side it came from.
- `destroySprite`/`destroyGroup` play a declared `exit` unless told
  `{ snap: true }`: a leaving node keeps its slot until the exit
  settles, so a subtree torn down off screen (a region, a level reset)
  without `snap` holds its slots for the exit's duration.
- `keepWorld` keeps THIS frame's pose: a transition in flight is cut
  there (the declaration stays), since its target was spelled in the old
  frame. Re-home at rest where you can, or write the target again.
- The queries are per-body answers - a hit test, a marquee, a blast
  radius, one mover call per character - not a collision broadphase. A
  bullets-vs-crowd test at frame rate is hundreds of core queries per
  frame, and it is simulation logic anyway, which lives in plain
  TypeScript with no layer in sight (core's "keep the simulation out of
  the renderer"): the app's own arrays, a uniform grid over them,
  runnable headless. Tile-layer collision (tiles are baked records, not
  nodes) is a solid-cell grid query of its own, not the sprite index.
