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

Contents:

- [The model](#the-model)
  - [Three faces](#three-faces)
  - [A layer renders through its views](#a-layer-renders-through-its-views)
  - [Atlases and frames](#atlases-and-frames)
  - [Materials](#materials)
  - [Pose, sprite and style slots](#pose-sprite-and-style-slots)
  - [Fixed instance slots](#fixed-instance-slots)
  - [Growth](#growth)
  - [Picking](#picking)
  - [Spatial queries](#spatial-queries)
  - [Groups](#groups)
  - [Visibility](#visibility)
  - [Flips](#flips)
  - [Mutations batch to the frame](#mutations-batch-to-the-frame)
  - [The records layer](#the-records-layer)
  - [Layer space](#layer-space)
  - [The camera](#the-camera)
  - [Camera control](#camera-control)
  - [Layer tint](#layer-tint)
  - [Views](#views)
  - [Regions](#regions)
  - [Retargeted motion](#retargeted-motion)
  - [Frame-rate motion](#frame-rate-motion)
  - [Frame animation](#frame-animation)
  - [Text runs](#text-runs)
  - [Pure pieces](#pure-pieces)
- [The baked tile layer (tiles.ts)](#the-baked-tile-layer-tilests)
  - [Chunks](#chunks)
  - [Residency](#residency)
  - [Cells: frames, symmetry, overhang](#cells-frames-symmetry-overhang)
  - [Scrolling and the camera](#scrolling-and-the-camera)
  - [Bulk writes](#bulk-writes)
  - [Tinting](#tinting)
- [Components](#components)
  - [Component props](#component-props)
  - [SpriteLayer and useSpriteLayer](#spritelayer-and-usespritelayer)
  - [Pointer events](#pointer-events)
- [Traps](#traps)
  - [Atlases and sampling](#atlases-and-sampling)
  - [Materials](#materials-1)
  - [Slots, capacity and order](#slots-capacity-and-order)
  - [Nodes, transitions and queries](#nodes-transitions-and-queries)
  - [Cameras and rotation](#cameras-and-rotation)
  - [Text runs](#text-runs-1)
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

### Materials

What a layer's pixels look like is its MATERIAL (material.ts), one per
layer, fixed at creation (`createSpriteLayer(atlases, { material })`,
the records and tile layers alike, `<SpriteLayer material>` /
`<TileLayer material>`): the material sits on the thing that is one
draw entry - the layer is the batch - as @solidrt/3d's sits on the
mesh, so two looks are two layers, which is also how you order them
(the additive glow layer draws over the scene layer). The same model
and the same words as 3d, deliberately: `unlit()` is the stock
material, `prelude`/`surface` its tier-2 slot, `shaderMaterialClass`
/ `shaderMaterial` the class form over the layer's GLSL set,
`instanceBuffers` / `instanceStyle` / `setInstanceStyle` /
`instanceAttribute` the per-sprite style record (the comparison and
the decisions: okf/plans/materials-one-model.md; Godot's
`CanvasItem.material` with a `canvas_item` shader is the nearest
engine shape, PixiJS and Phaser the batch-per-look precedent). The one
deliberate asymmetry: a 3d `blend` implies `transparent` and the
scene's back-to-front sort; a layer has no depth and draws in record
order, so nothing sorts.

- `unlit({ blend?, prelude?, surface?, params?, textures?,
  instanceBuffers?, instanceStyle?, label? })` - the atlas texel times
  the sprite's tint (a distance-field atlas decoded to its fill and
  outline), times the layer tint, blended with `blend` ("alpha" by
  default, "add" for glows and additive particles, "multiply" to
  darken, "none" to overwrite; pipeline state, so per layer). Every
  layer draws with `unlit()` unless told otherwise. The TIER-2 slot:
  `surface` declares `void surface(inout Sprite s)`, called with the
  struct filled and before the layer tint - `color` (the stock result,
  premultiplied: texel times tint, or the decoded field), `uv`,
  `frame`, `tint`, `atlas` - rewrite `s.color` or `discard`;
  `prelude` is file scope after the set (uniforms, each a LAYER PARAM;
  helpers); `spriteSample(uv)` resamples the fragment's own atlas at
  another uv (the pick, the frame clamp, the field decode, the tint),
  so a scroll is `spriteSample(mix(s.frame.xy, s.frame.zw,
  fract(t)))` - the clamp into the frame still applies, so a wrap is
  `fract` in FRAME space. One program per distinct source, cached for
  the app's lifetime like 3d's.
- PER-SPRITE DATA: `instanceBuffers: [{ attributes: [{ name, format }]
  }]` on either form declares the material's STYLE record(s), one
  stream per buffer bound after the layer's own records, any vertex
  format (a palette index is a `uint8x4` at four bytes, a dissolve
  amount a `float16x2`), every slot starting as `instanceStyle` (the
  identity of what the look does with it). On `unlit` the stock
  vertex stage forwards each attribute unchanged as a flat varying -
  `iPalette` reaches the prelude and the surface as `vPalette`
  (`forwardedName`; names follow the layer's own `i` + capital
  convention) - so a palette swap is one attribute, one surface line
  and a `setInstanceStyle(layer, sprite, [row])` per sprite. Writes:
  `setInstanceStyle(layer, sprite, values)` (the first buffer, one
  value per component as the shader sees them; marks the slot like
  setSprite), `instanceAttribute(layer, name)` (the accessor: slot in,
  component values out and in through the format's codec; valid
  across growth) plus `updateRecords(layer, { stream, first, count
  })`, or `records(layer, stream)` in bulk (stream 1.. the material's
  buffers in order; a Float32Array over an all-float layout, bytes
  otherwise), `<Sprite style>` in the component face, `setTile(...,
  { style })` on a tile layer. A recycled slot (and a records layer's
  shifted one) starts blank again.
- LAYER PARAMS: `layer.setParams({ uFlash: 0.5 })` writes a prelude's
  uniforms to every view (and every resident chunk of a tile layer,
  which RE-BAKES on each write like its tint) over the material's own
  `params`; the `params` option and prop seed them, a view created
  later inherits the merged record. @solidrt/3d's setMeshParams one
  dimension down. Material `textures` (a palette LUT, a noise texture)
  bind beside the atlases and count against the same
  `limits.maxTextureUnits` budget, checked at layer creation.
- The class form, `shaderMaterialClass({ vertex, fragment,
  instanceBuffers?, instanceStyle?, blend?, label? })` with
  `instance({ params?, textures? })` and `dispose()` on the class
  (`shaderMaterial(opts)` is a class with one instance): TIER 1 pairs
  a vertex stage of your own with `unlitFragment(options)` from
  `@solidrt/2d/glsl` - compose `SPRITE_VARYINGS`, core's
  `SCREEN_SIZE_GLSL`, your placement into `corner`/`center`/`rot`/
  `clampScale`, then `SPRITE_VERTEX_BODY`; `SPRITE_VERTEX` is the
  stock stage, `unlitVertex(forwards)` the stock stage forwarding
  attributes - and TIER 3 writes a `main` of its own over the layer's
  SET, which the layer prepends (the varyings, `uAtlasN`, `uTint`, the
  `Sprite` struct, `spriteOf()`, `spriteSample(uv)`: declare none of
  them, start with `Sprite s = spriteOf();` since the derivatives
  every tap uses are taken there, and write premultiplied). One
  program per ATLAS LIST met (the set is generated per list), one
  pipeline per instance-layout list met on top - the records and tile
  layers bind one record buffer, the node layer a pose and a sprite
  buffer, with the same attribute NAMES on both (iPos, iRot, iScale,
  iUv, iTint, iScreenPx, iAtlas, iOutline), so one vertex stage
  serves every layer kind. A vertex stage must declare and use
  `uCamera`, `uCameraRot` and `uViewport` (checked at creation), an
  instance attribute may not reuse a layer name
  (`LAYER_ATTRIBUTE_NAMES`), a texture may not take an atlas sampler's
  (`uAtlasN`). `tests/materials.test.tsx` pins all three tiers, the
  style streams through growth and shift, the params, the resample,
  the tile bake and the throws.

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
per-sprite data, Materials above). NEVER write the pose buffer from JS -
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

### The records layer

The records layer (`createRecordLayer`) keeps the old model whole: 20
JS-owned floats per sprite `[cx, cy, w, h, u0, v0, u1, v1, rot, tint
rgba, minScreenPx, maxScreenPx, atlas, outline rgb, outlineWidth]`
(`INSTANCE_FLOATS`; `atlas` is the frame's texture as its index in
`layer.atlases`; the outline is read by a distance-field atlas only), draw order =
insertion order (or key order with `orderBy` - see below), remove
shifts, raw writes, JS pick walk. The raw path speaks @solidrt/3d's
record-mesh verbs, one dimension down: `records(layer)` is the mirror
(read it at use time - growth replaces it), `updateRecords(layer, {
first?, count? })` publishes a RANGE of it at the flush (the whole
mirror by default; ten moved records of ten thousand cost ten - one
`writeBuffer` at the range's byte offset, clipped to the live sprites;
under `orderBy` the drawn prefix whole through the lease instead, since
the core gathers it into key order during the copy), and
`setRecordCount(layer, n)` dials the drawn prefix - the first n live
sprites draw and pick, the rest stay live but undrawn and unpicked,
and the dial stays in force as sprites come and go (write ahead, dial
after; `layer.count` is the live total regardless). The sprite verbs
mark their own record, so a `setSprite` here publishes one record, not
the prefix. The POPULATION verbs differ by kind and stay so: a record
mesh's records are opaque data (`setRecords` copies in and grows), a
record layer's are sprites (`addSprite`/`destroySprite`), so the copy-in
here is `records(layer).set(src)` then `updateRecords(layer)`. The
publish itself is the core's record stream (`createRecordStream` in
`@solidrt/core/gpu`: the GPU buffer, the byte mirror, the dirty range,
the range-or-whole publish, growth into a replacement), the one model
the 3d instance streams run on too; this file only decides what to
mark and when to flush. The node layer speaks the same two verbs over
its SPRITE records: `records(spriteLayer)` is the sprite mirror
(`SPRITE_FLOATS` floats per slot: `[u0, v0, u1, v1, tint rgba,
renderOrder, minScreenPx, maxScreenPx, atlas, outline rgb,
outlineWidth]`, slot-indexed, poses stay the core's) and
`updateRecords(spriteLayer, { first?, count? })` publishes the slot
range, so a bulk restyle (a palette cycle over thousands of sprites) is
one loop and one range write instead of a setSprite each - the 3d
`records(instancedMesh)` pairing, one dimension down. Both layer
kinds take a `stream` beyond 0 on the same verbs for the material's
style streams (`records(layer, 1)`, `updateRecords(layer, { stream: 1
})`), which grow, publish and (on a records layer) shift with the
layer's own record. It is the
escape hatch for motion only JS can compute at scale (measured 30k
sprites: 12.9ms raw records vs 30.8ms via setSprite; both figures are
the WRITE path only - whatever computes the motion is excluded and is
usually the dominant cost, e.g. a 24k-particle sim measured ~25ms with
a near-free publish) - the axis is
WHERE MOTION IS COMPUTED, not retained-vs-dynamic. The sprite functions
(addSprite/setSprite/getSprite/destroySprite) work on both layer kinds;
record sprites have `node: null` and no groups. `tests/records.test.tsx`
pins the range, the dial, growth and the ordered publish on painted
frames.

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

### Camera control

The camera controls run one pipeline (okf/design/camera-controls.md):
a SOURCE (an input nudge, a verb, the follow) writes the pose, the
FRAMING eases a followed point into its zones, the LANES (a screen
offset, shakes) are summed on top of the pose, the CONSTRAINTS (the zoom
range, the contain clamp) apply to pose plus lanes and move the pose,
and the PUSH writes the final camera. `pose()` is the pose alone;
`camera()` the final camera as the layers received it.

Camera control: `createCamera2d(view | views, { world?: { width,
height, damping? }, min/maxZoom?,
pivot?, follow?: { damping?, deadZone?, hardLimits?, lookahead? },
offset?, panSpeed?, zoomSpeed?, rollSpeed?, damping?, inertia?, x?, y?,
zoom?, rotation? })` - Godot's Camera2D and Three's MapControls in one
control over the shared CameraUpdate: pan with inertia on release, zoom
about a point, eased `glideTo`/`fit`, `follow(x, y)` through
Cinemachine's framing, roll about the pivot, and world bounds that
CONTAIN the view (an axis whose view is wider than the world centers -
so at the fit zoom a pan is a no-op, there is nothing to pan; limits
ignore rotation, Godot's rule; a `world.damping` eases a MOTION into
the bounds instead of cutting it at the edge, Godot's limit_smoothed,
while a direct write always clamps at once). The pose is "world point
at the pivot", the pivot a viewport fraction defaulting to the center,
so `camera().x/y` is the view center and glideTo/follow land there
(without a `world`, the default pose puts world 0,0 at the pivot: a
fill layer that wants world = screen at rest takes `pivot: { x: 0, y:
0 }`). The pivot is the point the control zooms and rolls about - the
3d orbit control's target, which that control holds at the view centre
and re-seats in world space (`setPivot`); this one places it on screen.
The first argument is anything with a view's `setCamera` and `size()`
(a view of a sprite layer, of a record layer, several at once - one
camera over a scene's layers, the FIRST one's size the viewport - or a
`{ setCamera, size }` pair over a signal feeding `<TileLayer camera>`).

**Follow.** `follow(x, y)` chases a world point through the framing in
`follow` (core's camera-control, shared with the 3d orbit camera):
damping per axis (`{ x, y }`: a platformer's tight horizontal and lazy
vertical; a number for both; 0 snaps), the dead zone and hard limits as
fractions of the viewport centred on the pivot (inside the dead zone
the point roams free; in the band up to the hard limits it is eased
back, Godot's drag margins; beyond them it is clamped inside at once,
whatever the damping - the guarantee a fast target never leaves the
screen; Cinemachine's game default is 0.8 x 0.8), and lookahead by
`time` seconds of the point's velocity smoothed over `smoothing`
seconds. Call it every frame for a moving point; the control settles
once the point rests inside the zones. A pose glide yields to it, an
anchor glide (a wheel zoom) keeps running under it; `unfollow()`, or a
verb that writes x/y/zoom, ends it.

**Lanes.** `offset` is where the pose's world point is shown, in
viewport HEIGHTS from the pivot (x right, y down: `[0, -0.25]` shows it
a quarter of the height above the pivot - Godot's `offset`, Phaser's
followOffset); `shake(strength, duration, { frequency?, direction? })`
adds decaying oscillations (viewport heights of peak displacement) that
sum. Both are added on top of the pose at push and never enter
`pose()`; the bounds contain pose plus offset by moving the pose, and
clip a shake at push, so a shake at the world's edge never shows the
outside and never disturbs the pose.

**Shots.** `createShots(view | views, { blend? })` (core's blender over
the layers' CameraState, Cinemachine's virtual cameras): `shot(name, {
priority? })` returns a target a `createCamera2d` drives instead of the
view - a wide camera and a close one, each with its own control - and
`activate(name, { blend? })` makes the highest-priority enabled shot
live, blending the view from the current output over `blend` seconds
(default 0.5; 0 cuts; x/y/rotation/pivot linear, the zoom in log
space, `mixCamera`); `deactivate` falls back by priority. A live
shot's pushes reach the view at once; a blend needs `update(dt)` while
`active()`, the controls' rule. `live()` and `camera()` read the state.
A switch mid-blend starts from the output of that moment, so a quick
back-and-forth never jumps. The component form: `<Shots blend? ref?>`
under `<SpriteLayer>` (or `<View2d>`) with `<Shot name priority?
active? blend?>` children, each providing the view with its camera
redirected to the shot, so a `<Camera2d>` inside a `<Shot>` drives that
shot while its size and handlers stay the view's; `active` (default
true, live) withdraws and restores a shot, an unmount removes it, the
container runs the blend on frames while one is in flight. Pinned in
tests/camera2d.test.ts.

#### Input and bindings

Input, the rule (ARCHITECTURE.md): the control consumes a device-free
abstraction and never handles events itself. Its `axes` (core's
createAxes contract) are `pan` (vec2: a delta is finger travel in
viewport heights, applied as content travel - the world follows the
finger; a rate slides at one viewport height per second), `zoom` (axis,
octaves, positive in: a delta bracketed by a gesture - a pinch - applies
at once about its focal point, an unbracketed one - a wheel notch -
retargets an eased glide, which is how the control tells a finger from
an impulse; notches compound on the pending target, and `damping` scales
the settle time - 0 applies a notch at once - the 3d controls' knob) and
`roll` (axis,
turns); a pan gesture's begin stops any glide (a finger landing on a
gliding view holds it) and its end flings with the release velocity the
gesture measured (viewport heights per second from the pointer feed;
the control has no estimator of its own). The
verbs, each pushing the pose at once: `panBy(dx, dy)` screen pixels,
`zoomAt(sx, sy, factor, { glide? })`, `rollBy(angle)`, `set(pose)`,
`glideTo({ x?, y?, zoom?, rotation? })` (eased, the fields given, a
rotation glide included - the 3d controls' glideTo(pose)),
`fit`, `follow`/`unfollow`, `shake`, `interrupt`, `release([vx, vy])`
(screen pixels per second of content travel). An input
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
inertia, the lanes and the axis rates, pushes one setCamera per driven
view when the camera changed and reports that), gated on the reactive
`cam.active()` (true while a glide, fling, fit, follow, shake or damped
bound needs frames, or a rate drives; false at rest), read
`cam.camera()` for projectCamera and `cam.pose()` for the control's own
four values (the shape `set()` takes, the 3d controls' `pose()`). The
camera has NO tap of its own: taps are the dispatch's (`onTap` on the
root with `e.sprite` null is "tap on empty space"). `<Camera2d
input={input}>` inside `<SpriteLayer>` is all of that wired through
context, the 3d `<OrbitCamera>` shape: options read at mount, driving
the nearest view (the `<SpriteLayer>`'s own or the enclosing
`<View2d>`), that view's `size()` the viewport, the map's
`pan`/`zoom`/`roll` actions (or the names in `actions`) driving the
axes, frames only while `active()`. A view's feed normalizes a drag by
the leaf's own box, so a leaf under a designSize fit pans and zooms
correctly. camera2d.ts imports `@solidrt/core/input` and
`@solidrt/core/camera-control` only, so tests/camera2d.test.ts pins
the clamp, anchoring, glides, the framing, the lanes, damped bounds,
inertia and the axes headless; examples/camera.tsx (function face) and
examples/pick.tsx (`<Camera2d>`) are the live guards. The shared
vocabulary with @solidrt/3d, one kind and unit per word: `pan` vec2,
`zoom` axis (octaves), `roll` axis (turns) here; `rotate`, `look`,
`move`, `rise`, `focus` there. The pose options are initial values;
every other option (world, zoom range, pivot, the follow's zones, the
offset, the speeds, inertia) is read where it applies, never
snapshotted, so `<Camera2d>` props are live - forwarded as getters, a
bounds, pivot or offset change re-clamping and pushing the pose at once
through `set({})` - and a function-face caller mutates the options
object it passed (the `<OrbitCamera>` rule, pinned in the check's
live-options case). Not yet, additive: a contain origin other than
center.

### Layer tint

Layer tint (`setTint(rgba)`/the `tint` option and prop, both layer
kinds): one `uTint` shared-params write multiplied over every sprite's
own tint - day/night, a dimmed parallax plane, a fade-in. Cheap to
animate (no record touches), unlike TileLayer's, which re-renders
resident chunks. The same contract as TileLayer.setTint, so one signal
drives a whole scene across layer kinds. The general form is
`setParams` (Materials above): the tint is the one param every material
takes.

### Views

Views (`layer.createView({ width, height, camera?, oversample?,
clearColor?, label? })`, every layer kind, the tile layer included):
the ONLY output a layer has - the window-filling main view, a minimap,
a radar strip, a zoomed inset, two split-screen panes - @solidrt/3d's
scene.createView one dimension down, as Unity renders a scene only
through Cameras and Godot a World2D only through Viewports. A view is
one draw target with a camera and a viewport of its own (views.ts holds
the shell every kind shares: the target, its size and oversample, the
camera and its params write, projection, the pointer dispatch). What it
draws is the kind's: a sprite or record layer's view holds one entry
over the layer's pipeline and instance buffers - no sprite is mirrored,
no record is written twice, and key order (`orderBy`) comes along,
since the core gathers the buffers themselves at publish (ONE live
entry declares the order, the first view's; when that view is disposed
the next live view re-adds its entry ordered, every other entry reads
them sorted); growth, the instance count and the layer tint fan out to
every view. A tile layer's view holds one entry per resident chunk
image over the chunk-quad pipeline, sampling the bake (The baked tile
layer below). Either way the camera and the viewport are the view's own
params, so a view costs no per-frame JS beyond its camera writes. The
`ViewHandle` is the viewport contract (texture, size/setSize,
oversample/setOversample, setCamera/camera/project/unproject, pick (the
layer's, through this camera; empty on a tile view),
listen/handlers/handlersFor, dispose - views also die with the layer);
the sprites stay the layer's (`addSprite`, `pick`, `setTint`,
`createView`, `dispose`). Pointer events on a view leaf run the
dispatch with the VIEW's camera undone over the layer's pick, the view
as the root of the walk: a sprite under a minimap gets its ordinary
handlers, `view.listen` is the last stop (a tap there with `e.x`/`e.y`
in world pixels is "glide the main camera here"; on a tile view every
event lands there), and a camera bound to a feed
(`feedPointer(view, feed)`) drives any view, `createCamera2d(view)` and
`<Camera2d>` alike. `<View2d>` is the component form, filling its box
like the `<SpriteLayer>`'s own view when given no size; the
`<SpriteLayer>`'s built-in leaf is a view too, and `<SpriteLayer
output={false}>` has none, showing only through its `<View2d>` children
(examples/split-screen.tsx). examples/views.tsx is the live guard.
`<TileLayer>` owns one view the same way and `<TileLayer layer>` adds
another of an existing layer (a minimap). A view is also what a
POSITIONAL SOUND is heard from: `view.listener({ speedOfSound? })` is
the view's flux:audio listener over its camera (made on first call,
destroyed with the view), and a core Sound placed with `emitter: {
node: sprite.node, listener: view.listener(), refDistance, maxDistance,
doppler }` (@solidrt/core/audio) pans and fades as the camera moves,
with no per-frame JS - the same emitter pass @solidrt/3d's attachSound
rides. Layer units are PIXELS: give refDistance and maxDistance in
pixels and `speedOfSound` in pixels per second, or `doppler: 0`. A
record layer's sprites have no nodes, so its views have no listener.
Not yet, all additive: a
`layers` bitmask (a markers-only minimap), `into` tiling, per-view
tint, and a viewport over SEVERAL layers under one camera (their
scene-level camera) - okf/backlog/2d-layer-views-additive.md.

### Regions

A sprite layer STREAMS by the spatial core's residency gate the way the
tile layer does (Residency under the tile layer below; the contract in
the Residency section of @solidrt/3d's AGENTS.md, which this mirrors
one dimension down): `createRegionGrid(layer, { cell: [w, h], enter?,
leave?, origin?, bounds?, weight?, limit?, priority?, retain?,
...content })` / `<RegionGrid>` is a root group of the layer whose
every cell a viewer wants within `enter` of its view (kept until beyond
`leave`; defaults half a cell and one and a half, the tile layer's)
becomes a child Region at the cell's corner - Unreal's World Partition,
Minecraft's chunks, for POPULATIONS: a city's cars by block, a
bullet-hell's enemies by zone, where the tile layer streams the ground
- and `createRegion(layer, { x, y, width, height, enter, leave,
parent?, ...content })` / `<Region>` is a bounded group placed like any
group (a level section, a boss room; Unreal's level streaming volume)
that moves with its group. VIEWERS are every view of every layer (the
view shell pushes its camera over its viewport to the gate on creation
and on every camera or size write: the main view, a minimap, a
split-screen pane - scoped to the layer's WORLD, its root node, so a
3d scene's camera or another layer's view never wants this layer's
cells) and `createStreamingSource(layer, { x, y, radius })` beyond them
(a cut-scene's look-ahead, a second player on a server, a teleport
destination loaded before the cut; moved with `setPosition`).
Cells are `[w, h]` in layer pixels, 0 unbounded along an axis (`[512,
0]` streams a side-scroller by column); `bounds` is an inclusive
cell-index box; a sprite grid has no `wrap` (the layer draws no seam
copies of a cell; okf/done/2d-wrap-around.md has the recipe); a
`<RegionGrid>` under a `<Group>` throws, since the gate's grid is layer
space.

What a region HOLDS is the app's, through core's lifecycle: `load(region,
signal)` prepares the data off the thread (a file read, a decode, a
`"use isolate"` generator; async, abortable; absent, `build` gets
undefined); `build(region, data)` adds sprites under `region.content`
(`addSprite(layer, { parent: region.content, ... })`, positions
cell-local; as a generator, a `yield` after every batch of a few
hundred runs the creates one step per slice of the frame budget,
core's `setSliceBudget`; a `<RegionGrid build>` returns JSX mounted
there instead, one step); `unload(region, data)` runs at leave BEFORE
the content goes, with the data (an editable world reads its cell back
here); `proxy(region)` makes the sprite or group drawn under
`region.group` while the content is not shown (a far-ring stand-in; most
layers pass none - a 2d far ring is the mip chain's job). The content
is HIDDEN until the build completes (pick, pickRect and the queries
skip it), shown in the frame the last step ran in, and MEASURED then:
the record bytes of every sprite under it (pose, sprite and style
records; `region.measure()` after content added later) are what the
gate's limits weigh the cell by in place of the estimate, so a grid
`limit` or the scene-wide `setResidencyLimit` counts what is there.
`priority` orders grids and layers when the enter budget is short
(`setResidencyBudget`): the ground tile layer before its decoration
layer before the populations. A leave aborts a load in flight, cancels
a build under way (the generator's `finally` runs), runs `unload`, and
SNAPS the teardown (`destroyGroup` with `snap`: no exit plays, the
slots recycle this flush - a region leaves off screen); a failed load
or build is fail-soft (`state` "failed", `onError`, the next enter
retries). `region.state` is the gate's "pending"/"stalled" until the
enter, then "loading", "building", "shown" or "failed";
`grid.cells()`, `cellAt(i, j)`, `cellOrigin`, `setLimit`, `setWeight`,
`forget` (drop the retained data), `onStateChange`/`onError`, `dispose`.
`/stats` reports the gate (`regionsWanted`, `regionsResident`,
`regionsStalled`, `residentWeight`) and the handlers (`app.regionsLoading`,
`regionsBuilding`, `regionsShown`, `regionsFailed`).

OWNERSHIP, the rule every streamed world needs: a region owns what sits
under its content and destroys it at leave, and a handle kept to one of
those sprites is inert from then on (`layer` null). A sprite that must
outlive its cell - an enemy chasing the player across the border, a
projectile - is re-homed by the app, `setSpriteParent(sprite, elsewhere,
{ keepWorld: true })` as it crosses or in `unload` at the latest, the
way Minecraft moves an entity between chunks. Two more rules from the
engines that got them wrong: draw order across cells is slot order (a
re-entered cell takes whatever slots are free), so a streamed world
wants `orderBy`; and `enter` must cover half the largest sprite's
extent, or a sprite near a cell edge pops in late.

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
frame catches up, so a hitch never shifts a held start). `stagger` (ms)
goes on a GROUP's declaration (`setGroupTransition` / `<Group
transition>`), never on the sprites: it spaces the enters and exits of
everything under the group that begin in one frame by `index * stagger`
(add order in, tree order out whatever the destroy order; nearest
declaring group wins), and
cascades nothing unless the sprites declare `from`/`exit` - a hand of
cards fanning in, a menu popping in, is one `stagger` on the group. For
the sprites straight under the layer (no group) the layer's root carries
it: `createSpriteLayer(texture, { stagger })` / `layer.setStagger(ms)`.
Each natural settle calls the handle's `onTransitionEnd` (plain field,
or the `<Sprite>`/`<Group>` prop) with `{ component }` - target-only,
never on a cancel, snap or exit; the raw "spatialTransitionEnd" engine
event (sol:events, node = sprite.node) stays for flux:spatial consumers.
See examples/springs.tsx (tap a sprite: it leaves through its `exit`).

PAUSE and time scale are declarations on the same nodes:
`layer.setTimeScale(scale)` / `<SpriteLayer timeScale>` sets the rate
every sprite and group transition in the layer runs at - with their
delays, enters, exits and stagger slots - 0 a pause that leaves the
app's UI moving, 1 app time, 0.5 slow motion; `setGroupTimeScale` /
`<Group timeScale>` and `setSpriteTimeScale` / `<Sprite timeScale>`
declare it for a subtree or one sprite. The NEAREST declaring node wins,
never a product (Godot's process_mode shape): a frozen layer with a
`<Group timeScale={1}>` inside keeps that group running. A frozen layer
asks for no frames (the write that resumes it brings the frame), writes
while frozen retarget and wait, a destroyed sprite with an `exit` stays
a ghost until its time moves again (the layer's `dispose` still cuts it
short), and a sprite added frozen sits at its `from`. The scale covers
the CORE's motion: frame animations follow it only when given a `clock`
(see Frame animation), and your own `onFrame` logic, timers and audio
stay on app time - multiply your step by `layer.timeRate()` /
`timeRate(sprite)`, the effective rate read. The dev clock
(`/clock?scale=`) scales app time underneath all of this.

### Frame-rate motion

Frame-rate motion only JS can compute (physics, flocking) bypasses the
declarative layer: `ref` the sprite, call `setSprite` from `onFrame` (a
~7us core transform write per moved sprite - fine to a few thousand;
past that use the records layer). Signals carry structure and slow
state - a `<Sprite x={sig()}>` re-running 60 times a second works but
re-runs an effect per sprite per frame for nothing.

### Frame animation

Frame animation (animation.ts): `createAnimation(frames, fps, { loop })`
is a clip with a shared clock on the app's frame time (it freezes with
the dev clock and plays at its own speed under `sol render`) stepping
every attached sprite
(`anim.add(sprite)` / `remove`; `play`/`pause`; `frame()`/`playing()`
read the clock, methods like the 3d mixer's `playing()`; `loop: false`
holds the last frame and fires `onFinish`, the mixer's name for it in
@solidrt/3d). One no-demand frame callback per playing clip, one
setSprite per sprite per STEP - an 8fps cycle is 8 publishes and 8
presents a second regardless of display rate, and a paused clip costs
nothing. A sprite belongs to at
most one animation (add detaches the previous); removed sprites prune
lazily on the next step. Works on both layer kinds; with `<Sprite>`,
attach via `ref` and leave the `frame` prop off - the clip owns that
field (the prop effect passes absent props as undefined, which setSprite
keeps, so other props stay reactive). A clip is a JS clock, so a layer's
`setTimeScale` does not reach it by itself: `createAnimation(frames, fps,
{ clock: layer })` (a group or a sprite works too) runs the clip at that
handle's `timeRate`, read every frame, so the one pause that freezes the
world's transitions holds its walk cycles as well, and a slow-motion
group slows the flipbooks in it.

### Text runs

Text IN the layer's world - a label under a sprite, a name on a map, a
damage number - is a text run: a group of glyph sprites drawn from a
SPRITE FONT's atlas (`createSpriteFont`, `addText`/`setText`/
`destroyText`, `<Text2d>`; `examples/text.tsx`). The font is the
runtime's own glyph engine (`flux:font`): it shapes the text (kerning is
the engine's, carets are one pass), makes glyph cells on a worker thread
and packs them into one atlas texture that grows as glyphs arrive. The
atlas is one more entry in the layer's list - `createSpriteLayer([art,
font.atlas])` - so labels draw in the one draw with the sprites they
belong to and pan, zoom, sort (`orderBy`), pick and transition as
sprites; a run's pose is its group's (`run.group`), where glyph pointer
events bubble. Two cell kinds: "msdf" (the default: one atlas sharp at
every zoom, the fragment stage decoding `Atlas.sdf`; the per-run
`outline` draws only here) and "mask" (coverage at the face's exact size,
for 1:1 drawing). Text
as elements is still the right tool up to hundreds of labels: `d-text`
under a `<view>` carrying the camera transform (what `<TileLayer>` does
for its chunks) costs no JS per label on a camera move and is rasterized
at the real scale; runs are for thousands, for in-layer draw order, and
for a zoom animation that would re-rasterize every label per frame.

Layout is `layoutText` (text-layout.ts, pure): `\n` breaks, `maxWidth`
wraps with core's greedy rule, `align` per line, `anchor`
(start/middle/end, as d-text) and `anchorY` (top/middle/baseline/bottom)
place the run's point, `lineHeight` as named; `letterSpacing` is shaped
into the units by the engine. The run reports `width`, `height`, `ascent`
and `lines`. A run's `fontSize` defaults to the face's; glyphs scale their
cells by `fontSize / font.size`.

### Pure pieces

frames.ts, extrude.ts (the mip-gutter repack, tests/extrude.test.ts),
pack.ts (the shelf packer, tests/pack.test.ts), pick.ts, camera.ts,
dispatch.ts, oversample-math.ts, tiles-math.ts (the
tile grid's chunk and slot math, tests/tiles.test.ts) and text-layout.ts
(glyph placement over prepared units, tests/text-layout.test.ts) are pure
(no GPU imports) BY DESIGN so they can be checked headless; keep them
that way.

## The baked tile layer (tiles.ts)

### Chunks

Static 2D bulk as a few quads: on tiled GPUs the budget is primitive count
(core agents/performance.md), so a 100x100 tile world must not be 10,000
quads per frame. `createTileLayer(tileW, tileH, atlases, opts)` bakes
the world into CHUNKED `render: "manual"` targets (default ~512px of
tiles per chunk, `chunkTiles` to tune) that its views draw as one quad
each, each chunk a manual target over
the layer's ONE pipeline (its material's, resolved once per layer for its
atlases; a cell's style record is `setTile`'s `style`, its params re-bake
every resident chunk on each write) with FIXED record slots - an empty
cell is a zero-size quad, instance count is constant per chunk. Records
hold CHUNK-LOCAL pixel coordinates and every chunk target's `uCamera`
sits at minus the overhang margin, so float32 precision is per chunk and
a world 10^7 px from the origin bakes as crisp as one at the origin.
Cells are UNBOUNDED: any integer `(col, row)`, negative included, in a
sparse map of chunks keyed by chunk coordinates (Unity's growing
`cellBounds`, Godot's `get_used_rect`, Tiled's infinite maps); a world
finite by design passes `bounds` (a cell rect; writes outside throw), a
torus `wrap` (periods in cells, modular coordinates; the two exclude
each other), and `usedRect()` reports the written extent. `setTile`
batches to the frame's publish pass (the microtask, outside a frame),
whose flush publishes and re-bakes ONLY dirty resident chunks, paced
under the bake budget (Residency below). After that the layer is static
textures: zero per-frame cost however many tiles exist.

### Residency

The layer is a HANDLER over the core's residency gate (the Residency
section of @solidrt/3d's AGENTS.md; flux:spatial `createGrid`): its
chunk grid is a grid of regions (`cell: [chunkW, chunkH, 0]`), every
VIEW (`layer.createView`; `<TileLayer>` makes one) is a viewer - the
view shell every layer kind shares pushes an orthographic view of its
viewport to the gate on creation and on every camera or size write,
scoped to the layer's own world (a root node the layer makes for it) -
and the gate's `spatialRegionEnter` / `spatialRegionLeave` events drive
the GPU side.
A chunk is wanted while within `enter` of any view (default half a
chunk) and kept until
beyond `leave` (default one and a half chunks; both in world pixels, the
hysteresis that keeps a camera on a chunk border from thrashing; the test
is strict, so a camera aligned to the grid never loads the ring beyond
its edge). On ENTER a chunk with cells takes a records buffer and a
target from the resource pool (`createResourcePool` in
@solidrt/core/gpu: the layer's own, 16 chunks deep, or `opts.pool`
shared with other layers and handlers; the layer registers the
destroyers of its two kinds on whichever pool it uses, so a shared pool
needs nothing from its maker), uploads its records and bakes in the same
frame's publish pass, so a chunk shows complete or not at all. The pool
hands out only what the GPU is done with: an id given back is stamped
with `submittedFrames()` and comes out again once `retiredFrames()` has
reached that count (the raster thread publishes it from its present
fences; both in @solidrt/core/gpu and `/stats`), because a frame in flight
still samples the targets and reads the buffers it was drawn with, and a
write into one of those makes a tiled driver drain its queue first (27
ms on an Adreno 610). The same rule makes a RE-BAKE of a resident chunk
(a `setTile` into it, a tint, a density change) bake into a retired set
from the pool and swap - the views rebind their entry - never into the
target on screen. And bakes are PACED, the way the gate paces enters: a
bake is a render pass the frame's picture then samples, paid beside the
frame, and a batch of them overruns a phone GPU (an Adreno 610 at a 15
ms frame pays 0.7 to 1.7 ms a bake: one a frame runs clean, two or four
miss presents as an unpaced ten-chunk batch does, 50 to 67 ms gaps), so
`setBakeBudget(n)` (default 1) is ONE
budget of bakes per frame every tile layer in the app spends together,
the residency budget one level up. A chunk's first bake, its
appearance, is never held back (the gate paced it; a hole is worse than
a late refresh) but spends the budget; the re-bakes of drawn chunks take
what is left, nearest a view's eye first across the layers (the gate's
urgency: eye distance in `leave`s), and the rest wait a frame showing
their last bake, the layer demanding the frames they need. So a
`setTint`, `setParams` or density change over forty resident chunks
rolls outward from the view over forty frames instead of landing in one
heavy frame, and an edit batch costs one frame of latency per budget of
chunks. `/stats` reports `app.tileBakes` (cumulative) and
`app.tileBakesWaiting`. The default holds on the low-end baseline; a
desktop app raises it (`Infinity` = unpaced), with a deeper `pool` past
sixteen chunks, since every re-bake parks a set until it retires. On
LEAVE the target and buffers go back to
the pool and the records stay on the CPU (~5 KB a chunk, ~180x cheaper
than its texture; `retain: "records"`, the default), or go too under
`retain: "none"`, where `opts.fill(chunk)` regenerates the chunk's cells
on the next enter with one `setTiles` over `chunk.col, chunk.row,
chunk.cols, chunk.rows` - the procedural world, Minecraft's generate
(`fill` also runs on the first approach of a never-written chunk under
`"records"`). A chunk written while not resident is records only; a
resident chunk emptied gives its target back; a leave that arrives before
a bake cancels it. `resident()` is the wanted set (a resident chunk with
no cell holds no texture), `onEnter(chunk)` / `onLeave(chunk)` bracket
it, and `TileChunk` is one object for the chunk's whole life (its cell
rect, its world rect, `texture` while resident). The gate decides WHEN
(hysteresis, the union over views, enters paced nearest first under the
scene-wide `setResidencyBudget`); the layer never does, and a layer with
no view bakes nothing. Unity and Godot cull chunks but never evict
them; the bake moves memory from bytes per cell to texels, which is why
residency is the default here, not an option.

Memory is legible, then bounded. Every chunk target and buffer carries
the LAYER's label, so `/gpu?label=<layer>` lists the layer's resources
and the reply's `groups.<layer>` is its footprint in one number
(textures, buffers, bytes: what is allocated under the label, pooled
idle targets included); `/stats` reports the gate (`regionsWanted`,
`regionsResident`, `regionsPending`, `regionsStalled`, cumulative
`regionEnters`/`regionLeaves` and the window's pair, `residentWeight`)
and the pools' traffic (`app.poolHits`, `app.poolMisses`,
`app.poolHeld`, every pool summed). `maxTexels` is the layer's texture
budget: a chunk is admitted by the grid's ESTIMATE (one target's texels
at the current density, its mip chain included, re-estimated on
`setDensity`) and weighed by its MEASURED bytes once it holds one (the
target plus its record and style buffers, `setCellWeight`; nothing
while it holds no cell), the budget is the grid's limit (the texels'
bytes plus the buffers of as many chunks as they hold, so it holds
exactly the chunks it promises), and past it the gate evicts the chunk
least needed - furthest from any view in `leave`s - for one more
needed, the eviction's leave before the enter (the evicted set reaches
the pool first; the enter takes a retired set, which that one becomes a
frame later), and a chunk nothing can make room for waits, wanted and
stalled, until a view moves or the limit changes (Unreal's streaming
pool, one grid at a time; `setResidencyLimit` is the scene-wide form
over every grid and region, bytes everywhere). The budget bounds the
resident COUNT, never the quality: a world that fills it shows holes at
its far edge rather than blur everywhere, and a resident chunk with no
cells holds no texture and weighs nothing, so a sparse world sits below
its budget. It must hold at least one chunk at the density in force
(throws otherwise, at creation and at `setDensity`). `priority` (an
integer, default 0) orders the layer among the app's grids and regions
when the enter budget is short (`setResidencyBudget`): the ground
layer before its decoration layer before the sprite populations
(Regions above), and evicted last.

### Cells: frames, symmetry, overhang

A cell is a frame plus its symmetry, a tint and a style record. A frame
draws at its OWN pixel size anchored to the cell's bottom-left (Tiled's
rule for tiles larger than the grid, Unity's and Godot's own-size
sprites): a frame the cell's size fills it, a taller one reaches up, a
wider one right, so oversized, isometric and hex tiles are expressible.
The three symmetry flags `flipX`, `flipY`, `transpose` (`setTile`'s
options, or the top three bits of a `setTiles` index cell: `TILE_FLIP_X`,
`TILE_FLIP_Y`, `TILE_TRANSPOSE`, Tiled's gid layout, so a Tiled layer's
cells are these minus its firstgid) compose in Tiled's order (transpose,
then the flips) into the eight symmetries of a square with no record
layout change: the flips are UV swaps and the transpose a quarter-turn
rotation with the flips remapped (tiles-math.ts `cellSymmetry`; a
transposed frame covers `fh x fw`). `getTile` reads the canonical frame
back. Overhang is baked into the OWNING chunk: every chunk target is
chunk size plus the layer's `overhang` on every side (default the
largest overhang in the `frames` table as its frames lie, else 0; a
frame exceeding the cell plus the margin throws at the write, a
transposed non-square frame included, so give `overhang` to transpose
one), a view's chunk quads overlap by that margin and draw row-major,
so a lower row's overhang draws over the row above - Tiled's and
Godot's y-order for overhanging tiles.
A cell dirties exactly one chunk. Animated cells are a documented limit:
a bake re-renders a chunk per frame change, so animated tiles live in a
sprite layer.

### Views and the camera

The tile layer renders only through its views (`layer.createView`, the
Views section above; `<TileLayer>` owns one, `<TileLayer layer>` adds
one). A tile view is a draw target with one entry per resident chunk
image over the chunk-quad pipeline (tile-pipeline.ts): the entry binds
the chunk's baked target and carries its chunk indices, the view's
shared params carry the camera, so scrolling never re-bakes and a
camera move is one params write with no entry change - the world point
(x, y) pinned to the viewport point (pivotX, pivotY), scaled by zoom,
ROTATED by rotation about the pivot, the same `CameraUpdate` the sprite
layer takes, so one signal drives a whole rotating scene across both
layers; rotation is the ship-flies-over-the-map camera. The stage takes
the camera SPLIT by chunk (`uOrigin`, the chunk the camera point lies
in, and the remainder inside it) and subtracts chunk indices before
scaling to pixels, so a world 10^7 px out renders as crisp as one at
the origin. Entries are kept in row-major world order (a lower row's
overhang over the row above) and follow residency: an entry comes when
a chunk acquires its target and goes when it releases it, synced at the
layer's flush. The view is the gate's viewer too (Residency above).
One layer has ONE bake density (`density`, texels per world pixel):
every view asks for its oversample times its camera zoom and the layer
bakes at the largest wish, rounded up with the shrink hysteresis and
bounded by `maxDensity` and `maxTexels` (a chunk must fit the budget,
so the pick is clamped, never thrown; only a pin past the budget
throws). `density` pins it, and `setDensity(null)` hands it back to the
views. A view below the density minifies the bake through the chunks'
mip chain (`mipmap`, default true; the core regenerates it after every
bake, and it counts a third more against `maxTexels`), so a minimap at
a tenth of the main zoom is as clean as the main view; `mipmap: false`
for a world no view ever minifies. The VIEW's own `oversample` (texels
per view pixel, the auto pick from the leaf's on-screen box in the
component) is the sprite view's knob, unchanged. Tiles are data, not
children: there is no `<Tile>` component on purpose - write cells
through `ref` with `setTile`; a `<Camera2d>` child drives the view.

### Wrap-around worlds

A toroidal world - the Asteroids screen wrap, a world map that scrolls
east forever - is three cases, two of them app recipes (Unity, Godot
and Three leave entity wrap to the game too):

- **A repeating background** is a quad sampling its texture with `wrap:
  "repeat"` (SamplerOptions on the gpu face, or a per-binding `{ id,
  wrap }` override) and a camera-driven UV offset: the GPU wraps, no
  seam exists. The `<texture>` element form is
  okf/backlog/texture-tile-mode.md.
- **Entity wrap** is position modulo per step, `((x % W) + W) % W`, on a
  JS-moved sprite (records layer or `setSprite` per frame). The TRAP: a
  sprite with a declared position `transition` treats the wrap write as
  a target and animates the jump clear across the world - there is no
  teleport write on a transitioned sprite, so a wrapping entity is a
  JS-motion workload. Seam continuity is GHOST COPIES: a sprite
  straddling an edge is drawn twice (four times in a corner), so park
  permanent ghosts off-camera and place them when the primary is within
  half a sprite of an edge; an in-shader modulo could never replace this
  (a quad crossing the seam needs a second draw, not a moved one).
- **A wrapping tile world** is `wrap: { cols, rows }` on the tile layer
  (periods in cells, whole chunks each; `bounds` and `wrap` exclude each
  other): cells are modular, and a view draws every IMAGE of a resident
  chunk inside its world box, so the seam is two entries over one
  texture and a view wider than the world shows it repeated, in exact
  painter's order. The image set depends on the camera only through the
  chunk-index range the view's box covers - four integers compared on
  every camera write - so a scroll costs no JS until the view crosses an
  image boundary.

### Bulk writes

Bulk writes: `setTiles(col, row, cols, rows, cells)` writes a rect at
once - `cells` row-major, `cols * rows` long, either frames (null clears)
or, with a `frames` table given at creation (`createTileLayer(..., {
frames })`, the `<TileLayer frames>` prop: a tileset, `grid()`'s array),
indices into it with the symmetry flags in the top three bits (-1
clears; a Uint16Array holds -1 as 0xffff and a Uint32Array as
0xffffffff, the same bits) - Unity's SetTilesBlock, Phaser's putTilesAt
over the map's tileset. One locate and one dirty mark per chunk the rect
touches, the per-cell loop inside the layer, the same flush; a chunk the
rect only clears never allocates, so a whole-world write of a sparse
world allocates exactly the chunks the per-cell seed would. A generated
or worker-built world is a Uint16Array or Uint32Array of indices,
transferable, never an array of frame objects; a fill is `new
Uint16Array(n).fill(i)`. `tint`, `style` and the frame-form flags in the
options apply to every cell set. Every entry is validated before the
first write, so a bad one changes nothing; the table is copied at
creation. Under `wrap` a rect may cross the seam but not exceed a period.
The rect form is the shape for worker transfer and chunk fill, not a
large speedup: the per-cell floor is the record write itself.

### Tinting

Tinting, two levels: `setTile(col, row, frame, { tint })` writes the
cell's record tint (same `[r, g, b, a]` 0..1 semantics as a sprite's
tint; absent keys keep their values, a cell set from empty starts at
`[1, 1, 1, 1]`), and `setTint(rgba)` / the `tint` option and prop tints
the whole layer through the shared `uTint` uniform, multiplied over the
per-cell tints - day/night, a dimmed parallax plane. A layer-tint write
is not a record write (chunks re-render GPU-side, nothing re-uploads),
but it does re-render every resident chunk: drive it from slow state,
not per frame. `setParams` (a material prelude's uniforms) and a cell's
`style` (the material's per-cell record, `setTile`/`setTiles` `{ style
}`) are the same two levels for a custom look, Materials above. Not
built: record compaction for the live sprite layer (a gather restricted
to the records the views see; okf/backlog/2d-sprite-record-compaction.md
holds the design and the condition it waits on).

## Components

### Component props

| Component | Props |
|---|---|
| `SpriteLayer` | atlases (Atlas[], the sheets bound as one draw; mount-fixed) or layer (an existing createSpriteLayer handle to adopt: the component shows and populates it, keeps only the view and the leaf as its own and leaves the layer's lifetime to its maker, so `<Sprite>`/`<Text2d>` sit over an imperative layer; the creation props atlases/capacity/material/orderBy then throw), capacity?, material? (the layer's material, `unlit()` default, `unlit({ blend: "add" })` for glows; mount-fixed), params? (layer.setParams as a prop, over the material's own; live, merge semantics), tint? ([r,g,b,a] 0..1, over the whole layer in every view), orderBy?, stagger? (ms, the layer root's enter/exit spacing), label?, ref?(layer) - the layer; plus its OWN VIEW, unless `output={false}`: width?, height? (view pixels - both, or neither = FILL: the leaf lays out at 100% of its sized parent and the view follows its box, so view pixels are the leaf's own coordinates; mount-fixed, matching `<Scene>` in @solidrt/3d), clearColor?, camera?, oversample?, maxOversample?, maxPixels? (a cap on the view target's texel count under the auto pick, the budget form of maxOversample: one cost on every display; the same name on `<TileLayer>` and the 3d `<Scene>`), viewRef?(view), output? (a function `(texture, leaf)` composes the leaf from the texture id: `ref={leaf.ref}` makes a fill view follow your leaf, `{...leaf.handlers}` routes sprite events through it scaled to its layout, `leaf.size()` (the target's texels, view pixels times oversample) and `leaf.deviceSize()` are reactive reads a post chain sizes from, applied before the paint - the same contract as the 3d `<Scene>`'s; `events` then throws; `false` = no own view, the layer shows only through `<View2d>` children and the view props throw), events?, pointer? (a createPointerFeed fed from the view's root, what a map over this view binds; `useSpriteLayer().pointer` inside), onPointer{Down,Move,Up}?, onWheel?, onTap? (the view's root: `event.sprite` is the hit sprite or null over empty space) |
| `Sprite` | x, y (center; local to the enclosing `<Group>`), w, h, frame?, rotation? (radians, clockwise), tint? ([r,g,b,a] 0..1), minScreenPx?, maxScreenPx? (the screen-size clamp on the smaller axis, view pixels; 0 = off; equal = constant size), style? (the sprite's style record under the layer's material, one value per component of its first instance buffer; live), visible?, transition?, onPointer{Down,Move,Up,Enter,Leave}?, onWheel?, onTap?, ref? |
| `Group` | x?, y?, rotation?, scale? (uniform, scales the subtree), visible? (the whole subtree), transition?, onPointer{Down,Move,Up}?, onWheel?, onTap? (bubbled from hit child sprites), ref? |
| `Camera2d` | createCamera2d's options (world?, min/maxZoom?, pivot?, follow?, offset?, panSpeed?, zoomSpeed?, rollSpeed?, damping?, inertia?, x?, y?, zoom?, rotation?), input? (the input map driving its `pan`/`zoom`/`roll` axes; live), actions? (action names per axis when the map's differ), ref? - a `<SpriteLayer>` child driving the nearest view's camera (the `<SpriteLayer>`'s own, or inside a `<View2d>` that view; its `size()` the viewport) from the map, nothing else; read at mount; throws under `output={false}` outside a `<View2d>` |
| `View2d` | a `<SpriteLayer>` child: one more view of the layer from a camera of its own (layer.createView as a component): width?, height? (view pixels, live - both, or neither = FILL exactly as SpriteLayer's own view: the leaf fills its sized parent and the view sizes to that box), camera? (partial CameraUpdate on the view's camera, live; the same state a `<Camera2d>` child writes), oversample?, maxOversample?, maxPixels? (the auto-pick, as SpriteLayer's), clearColor?, label? (createView's, fixed), ref?(view), output?(texture, leaf) (as SpriteLayer's; else a built-in `<texture>` leaf at the view size carrying the view's handlers), events?, pointer? (this view's feed, fed from its root), onPointer{Down,Move,Up}?, onWheel?, onTap? (the view's root: `event.sprite` null over empty space); a `<Camera2d>` child drives the VIEW from its map (inside, `useSpriteLayer()` reports the view as `viewport` and the feed as `pointer`); `<Sprite>`/`<Group>` children mount to the layer as outside |
| `Region` | x, y, width, height (the box, in the enclosing `<Group>`'s frame), enter, leave, weight?, priority?, retain?, label? (createRegion's, mount-fixed), load?, build (the content JSX, mounted under the region's content group with the loaded data), unload?, proxy? (JSX under the region's group while not shown), onStateChange?, onError?, ref?(region) - an explicit region (Regions above); move it through `ref` with setGroup on `region.group` |
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

### Materials

- A `surface` reads the struct, never a local of the generated program:
  `s.color` is premultiplied and BEFORE the layer tint (`uTint`
  multiplies after the slot), so a rewrite to an opaque colour is
  `vec4(rgb, 1.0) * s.color.a` to keep the sprite's coverage.
- A custom `main` (tier 3) starts with `Sprite s = spriteOf();`: the uv
  derivatives every tap uses are taken there, in uniform control flow.
  A `spriteSample` before it reads zero derivatives and picks the
  sharpest mip everywhere.
- `spriteSample(uv)` clamps into the FRAME like the stock tap: a uv
  past the frame's edge lands on the edge texel, never on the next
  cell. Wrap in frame space (`fract`) before mixing into the frame rect.
- The stock stage forwards a material's instance attributes as FLAT
  varyings named `v` + the name without its `i` - `iPalette` is
  `vPalette` in the surface; a name not spelled `i` + capital throws at
  `unlit()`. A custom vertex stage forwards what it likes.
- A material's `textures` and the atlases share one sampler budget
  (`limits.maxTextureUnits`, 16): a LUT and a noise texture leave room
  for fourteen sheets, and the layer creation throws past it.
- A tile layer BAKES: `setParams` on it re-renders every resident chunk,
  like `setTint`. Animated uniforms belong on a sprite layer, whose
  params are one shared write per view.
- The stock material classes are cached for the app's lifetime (one per
  blend x prelude x surface x layouts, like 3d's unlit); a
  `shaderMaterialClass` is yours to `dispose()`, and its programs are
  compiled per atlas LIST met - two layers over the same atlases share
  the program, two over different lists compile twice.

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

### Cameras and rotation

- Every rotation must agree on direction (clockwise, y-down):
  `pointInSprite` in pick.ts with the vertex stage's `iRot`, and
  `projectCamera` in camera.ts with `uCameraRot` and `<TileLayer>`'s view
  transform. The differential tests (tests/pick.test.ts, tests/camera.test.ts)
  guard the JS math against oracles but NOT against the shader - if you
  touch one rotation, touch all, then run examples/camera-probe.tsx (the
  live guard: shader vs projectCamera, node/record parity, the pointer
  round trip) and watch for CAMERA-OK.
- Retro scrolling habits are the app's, not the layer's: keep the camera
  fractional (rounding it to design pixels makes motion step at the
  rounding rate, not the frame rate), and a game that wants whole-pixel
  scrolling snaps its own camera.
- Every view's camera rotation is IN-SHADER (uCameraRot, the tile view's
  stage included) on purpose: rotating a view's OUTPUT leaf instead is
  wrong - the output is viewport-sized with the camera already applied,
  so a leaf transform spins the cropped viewport and the corners cut.
- Views of one sprite layer share its render origin, the primary view's
  (The camera): a second view more than ~10^5 px from the primary sees
  f32 steps in the records. Give it a layer of its own; the tile layer
  has no such limit (its records are chunk-local).

### Text runs

- A run draws nothing for a glyph whose cell is not in the atlas yet:
  the sprite is there, hidden, at its place, and the font re-frames the
  run when the cell lands a frame or two later (`font.ready()` awaits
  that; `settle` does in a test). Printable ASCII is made at font
  creation so a run of it never waits; `chars: false` skips that.
- The atlas grows by doubling and EVERY cell moves when it does:
  `font.atlas.width`/`height` change in place and every run over the
  font is re-framed. Never cache a glyph's frame outside the font.
- A sprite font over "msdf" cells (the default) is sharp at every zoom;
  one over "mask" cells is exact at the face's size, resamples under zoom
  like any texture, and silently ignores `outline`.
- A run is node-layer only (it is a group); `addText` on a layer that
  does not declare `font.atlas` throws. Dispose runs before the font
  (`font.dispose`, or the owner's cleanup).
- The style record is 16 floats and the raw record 20: a raw record
  writer (`records(layer)`) that assumed 16 is off by the outline field
  per sprite.

### Tile layer

- `<TileLayer>` is a `<SpriteLayer>` in shape: a `<texture>` leaf
  showing its view, filling its parent when unsized (give it a sized
  parent) or `width`/`height` in view pixels. Both layers render
  LAID-OUT leaves and take no position props, so to overlay sprites on
  tiles wrap each in a `<view position="absolute">` inside one container
  - as plain flex siblings they stack side by side instead. Neither can
  live inside a d-* subtree (the insert throws; `output` with a
  `<d-texture>` is the detached form).
- A layer with NO VIEW bakes nothing: imperative use (no `<TileLayer>`)
  makes one with `layer.createView({ width, height, camera })` and shows
  `view.texture`, or every chunk stays records on the CPU. `resident()`
  is empty, and a chunk's `texture` null, until the gate's enter arrives
  with a frame - a test settles or steps a frame before reading a
  chunk's target or the view's.
- `setCamera` is a PARTIAL update on every view (absent keys keep their
  values, the params rule): `view.setCamera({})` moves nothing; a reset
  is `{ x: 0, y: 0 }`.
- A frame bakes at its OWN size, anchored to the cell's bottom-left: a
  one-texel atlas no longer fills an 8 px cell (the sprite layers stretch
  a frame to the sprite's size; the tile layer does not). A frame larger
  than the cell plus `overhang` throws at the write; the `frames` table's
  largest frame sets the default margin, so a Tiled tileset with
  oversized tiles works unconfigured.
- `chunkClearColor` is PER CHUNK, as named: regions with no cell have no
  texture and render nothing, so a full-bleed ground color is the view's
  `clearColor` (or the container behind the leaf), not this. The flip
  side is FREE TRANSPARENCY: with the default `[0,0,0,0]` clear, a
  mostly-empty tile layer stacked over another composites with no mask,
  no alpha pass, no shader - a sparse upper world (floating clouds over
  a sea) just works, and per-cell alpha in the written cells carries
  through the chunk (transparent clear + alpha blend).
- Two densities, two words: the LAYER's `density` is texels per WORLD
  pixel (the bake), a VIEW's `oversample` is texels per VIEW pixel (the
  target); the view's wish for the bake is their product with the zoom.
  Hard pixels belong to the ATLAS sampler (`createAtlas` `filter:
  "nearest"`): the view samples the chunks linear through their mip
  chain, which at a 1:1 density-to-view mapping is exact and everywhere
  else the proper resample.
- A dirty chunk flush publishes and re-bakes that chunk in full, into a
  retired target and buffers from the pool (the ones on screen stay with
  the frames in flight; writing them stalls a phone GPU for tens of
  milliseconds), at most the bake budget of chunks a frame
  (`setBakeBudget`, default 1, nearest a view first; the rest show their
  last bake a frame longer). Fine on change-only cadence; per-frame
  setTile churn over more chunks than the budget falls behind by design,
  and each re-bake parks a set until it retires - that is sprite-layer
  work, not tile work. An animated tile is a sprite.
- Memory is the RESIDENT set times the chunk texture (~1.3 MB at the
  default size and density 1 with its mip chain, more with the overhang
  margin and the density squared), not the touched area: a leave gives
  the texture to the pool, the pool's cap bounds what waits there, and
  `maxTexels` bounds the resident set (`groups.<label>` in `/gpu` is the
  footprint).
- Under `wrap` the periods must be whole chunks: the default `chunkTiles`
  is picked to divide them, an explicit one that does not throws. A rect
  write wider than a period throws too.
