# @solidrt/2d - views, camera control and regions

Camera control and input bindings, multiple views, and streaming regions.
Read ../AGENTS.md first: it carries the model this file builds on.

## Camera control

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

### Input and bindings

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

## Views

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
image over the chunk-quad pipeline, sampling the bake (the baked tile
layer, tiles.md). Either way the camera and the viewport are the view's own
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

## Regions

A sprite layer STREAMS by the spatial core's residency gate the way the
tile layer does (Residency in tiles.md; the contract in
the Residency section of @solidrt/3d's agents/large-worlds.md, which this mirrors
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

## Traps

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
  (The camera in ../AGENTS.md): a second view more than ~10^5 px from the primary sees
  f32 steps in the records. Give it a layer of its own; the tile layer
  has no such limit (its records are chunk-local).
