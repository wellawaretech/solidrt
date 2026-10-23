# @solidrt/3d - cameras

Camera input, the orbit and first-person cameras, shots and blends, and
overlay projection. Read ../AGENTS.md first: it carries the model this
file builds on.

## Input

Input, the rule (ARCHITECTURE.md): a camera control consumes a
device-free abstraction and never handles events or reads a device
itself. The controls expose `axes` (core's createAxes contract - rates
sampled per frame, immediate deltas in device-free units) and pose verbs;
an input map (`createInputMap`, core AGENTS.md) drives the axes by action
name, and the APP binds devices to the map: the scene leaf's pointer feed
(`createPointerFeed()`, handed to `<Scene pointer>` / `<View3d pointer>`,
where it listens at the root of the pointer walk, so a mesh that claims
its press keeps the control out of that drag; an imperative scene or
view feeds it with `feedPointer(scene, feed)`), a pad (`gamepad(0)`, or `gamepad.next()`
for the next pad to press a button), the keyboard (`keyboard`, its key
events through `input.handlers` on the window).
Nothing binds by default: `<OrbitCamera />` without `input` moves only
through its handle. The standard wiring is a preset, plain bindings the
app applies and edits:

```tsx
let pointer = createPointerFeed()
let input = createInputMap(orbitActions)
input.bind(orbitBindings({ pointer, gamepad: gamepad() }))
<Scene pointer={pointer}>
  <OrbitCamera input={input} distance={7} />
```

The vocabulary is shared with @solidrt/2d, one kind and unit per word:
`rotate` vec2 (orbit turns), `zoom` axis (octaves, positive in), `pan`
vec2 (element heights), `focus` axis (a nudge with a focal: a double
tap's point), `look` vec2 (turns), `move` vec2 ([right, forward],
forward = -y in the screen convention), `rise` axis. Keys and
sticks move the CAMERA where a drag moves the content, so the presets
bind them to `rotate`/`pan` through `invert()`; `look` is the exception,
a drag and a stick both turn the eye.

## Orbit camera

The camera controls run one pipeline (okf/design/camera-controls.md):
a SOURCE (an input nudge, a verb, the follow) writes the pose, the
FRAMING eases a followed point into its zones, the LANES (a screen
offset, shakes) are summed on top of the pose, the CONSTRAINTS (the
clamps, `clampPose`, occlusion) apply, and the PUSH writes the final
camera. `pose()` is the pose alone; `camera()` the final camera as
pushed.

Camera control: `createOrbitCamera(scene, { target?, azimuth?, elevation?,
distance?, min/maxDistance?, min/maxElevation?, orbitSpeed?, rotateSpeed?,
zoomSpeed?, panSpeed?, damping?, push?, panPlane?, follow?, offset?,
clampPose?, zoomAnchor?, rotateAnchor?, occluder? })`
- azimuth/elevation/distance around a target with optional auto-orbit. The
first argument is anything with the scene's `setCamera`, `camera()` (the
fov maps pan travel to world) and `size()` (the aspect `fit` frames
against): a Scene, or a View to drive one view's camera independently.
Its `axes` are `rotate` (a delta of one element height sweeps one full
turn, Three's OrbitControls convention, so a drag feels the same on a
phone and a 4k window; a rate turns at 0.5 turn/s at full deflection),
`zoom` (octaves: a delta of 1 halves the distance, a rate of 1 halves it
per second - a DOLLY, bounded by the target), `pan` (the target slides so
the scene tracks the fingers 1:1 at the target's depth, three.js
DOLLY_PAN, weighted by `panSpeed`; `panPlane: "ground"` slides in the
plane orthogonal to world up instead, Three's screenSpacePanning off,
the map's choice) and `focus` (an axis: a nudge with a focal - a double
tap - glides the target to the point under it through `zoomAnchor`;
without a focal, to the point under the view centre). A delta bracketed
by a gesture (a drag, a pinch, two fingers) applies at once, the content
staying under the fingers; an unbracketed one (a wheel notch, a key
step) is an impulse and glides in - `damping` scales that settle time, 0
applies it at once - notches compounding on the pending value so a fast
scroll is one push, the 2d camera's rule and the same knob there. The
verbs: `rotateBy(azimuth, elevation)` radians, `zoomBy(factor, anchor?)`
(factor > 1 in, as the 2d camera's zoomAt), `panBy(right, up)` world
units, `setPivot(point)`, `setOrbitPoint(point)`, `set(pose)`,
`shake(strength, duration, { frequency?, direction? })`; every one
pushes the pose at once. `interrupt()` stops a glide or a damped tail
where it is (a press landing, a shot taking over) and leaves the
follow, the lanes and the auto-orbit running - the same verb on every
camera control. Two commanded moves ease instead, inside
update(dt): `glideTo({ azimuth?, elevation?, distance?, target? })`, and
`fit(bounds, { glide? })`, which frames a `[minX, minY, minZ, maxX,
maxY, maxZ]` box (geometryBounds, a model's `bounds`) - target to its
centre, distance to where the bounding sphere fills the tighter of the
vertical and horizontal fov at the target's size, azimuth and elevation
kept, clamps applied; a snap unless `glide`, so a park-then-snapshot
repeats (under ortho only the target moves). Any input drops a glide (a
finger landing holds the view), and a `set()` that writes a pose field
snaps and drops any motion; `set({})` and `set({ orbiting })` leave it
running. Every write - input, verb, glide frame, set() - goes through
the range clamps and then `clampPose(pose)`, which sees the whole pose
and returns the fields to change: bound where a pan may put the target,
or hold the eye above a floor (an elevation floor that depends on the
distance, which a fixed `minElevation` cannot say); a glide's goal is
clamped when set and its frames as they land, so it never shows an
illegal pose.

**Push.** `push: true` lets a zoom step past `minDistance` (or
`maxDistance`) move eye and target together by the overflow - along the
view axis, or the anchor's ray when the step is anchored, so the point
under the fingers keeps its pixel - instead of stopping at the floor
(camera-controls' infinityDolly): the eye's speed is continuous across
the floor, so a pinch moves THROUGH a model at a speed that scales with
the distance. Off by default: a dolly that ends at the subject is the
viewer's default.

**Anchors.** Zoom aims at the target unless `zoomAnchor(focal, {eye,
target})` maps the gesture's focal point - a FRACTION of the element,
[0..1, 0..1] - to a world point: then that point stays pinned under the
pointer and the target slides toward it. A pinch holds one anchor for
its whole gesture (the fingers' interleaved events make the span
oscillate, and re-anchoring per delta turns that into a crawl); the
wheel, arriving unbracketed, anchors per notch. `rotateAnchor({eye,
target})`, called when a rotate gesture begins, names the point its
drag should pivot about: projected onto the view axis, it re-seats the
pivot without moving the picture (`setPivot`), so a drag after an
anchored zoom orbits what the camera looks at, not wherever the zoom
left the target. `<OrbitCamera>` builds both from its scene (`anchor`
below); a function-face caller supplies them (`scene.pick` at the
focal, `scene.unproject` at the target's depth as the fallback).

**Orbit point and the lanes.** `setOrbitPoint(point)` re-seats the
pivot EXACTLY: the target becomes the point, the distance its depth, and
the offset lane takes up the point's sideways offset, so the camera's
position and orientation do not change and the next drag orbits that
point (camera-controls' setOrbitPoint with a focal offset) - a viewer
pushed through a model keeps turning about the model's centre. The
`offset` option is the lane's persistent half, in view HEIGHTS from the
centre (x right, y down: `[0.25, 0]` shows the target a quarter of the
height right of centre - Babylon's targetScreenOffset); `shake` adds
decaying oscillations to it. Lanes are added on top of the pose at push
and never enter `pose()`; a verb that writes the target (`set`,
`glideTo`, `fit`, `focus`) clears setOrbitPoint's share.

**Follow.** `follow(point)` makes the target chase a world point
through the framing in `follow: { damping?, deadZone?, hardLimits?,
lookahead?, heading? }` (core's camera-control, shared with the 2d
camera): damping per view axis (`{ x, y, z }`: right, up, forward - a
lazy vertical, a tight horizontal), the dead zone and hard limits as
fractions of the view centred on the target (inside the dead zone the
point roams free; in the band up to the hard limits it is eased back;
beyond them it is clamped inside at once, whatever the damping - a fast
point never leaves the view), lookahead by `time` seconds of the
point's velocity smoothed over `smoothing` seconds. Cinemachine's
Orbital Follow: the orbit's own input keeps working around the followed
point - a rotate orbits it, a zoom dollies toward it, a pan moves the
target and the follow eases it back. `follow(point, heading)` also
recentres the azimuth on `heading`, the followed thing's yaw in the
first-person convention (0 faces -z, positive turns left: a walker's
`pose().yaw` feeds a chase camera directly), by the shortest turn,
after `follow.heading.wait` seconds (default 1) without rotate input at
`follow.heading.damping` - Cinemachine's recentering: a drag looks
around, the camera settles back behind the walker. Call `follow` every
frame for a moving point; the control settles once the point rests
inside the zones and the heading is reached. `unfollow()`, or a verb
that writes the target, ends it.

**Occlusion.** `occluder(target, eye)` reports the free distance from
the target toward the eye when something stands between them (null
otherwise); a free distance under the pose's pulls the FINAL eye in to
it at once, and the return eases as the obstacle clears (Cinemachine's
deoccluder damping; Godot's SpringArm3D snaps both ways). The pose's
distance is untouched: a zoom out from behind a wall still goes where
the pose says once the wall is gone. `<OrbitCamera occlusion>` builds
the hook from a scene raycast.

Call `orbit.update(dt)` from your onFrame to integrate the rates, the
auto-orbit, any glide, the follow, the lanes and the occlusion return
(no frame loop of its own), and use its return - true when the camera
changed since the previous update, nudges included - to gate per-frame
dependents like reprojecting HUD overlays. `orbiting()` (the auto-orbit
switch) and `active()` (the frame-loop gate: orbiting with a non-zero
rate, a glide, follow, shake or occlusion return in flight, or any rate
driving - the predicate every camera control shares) are reactive
(HUD-safe); the pose is plain state via `pose()`/`set()` (also the
debug-command shape). It drives position and target only; fov/near/far
stay on scene.setCamera (or the Scene `camera` prop). The auto-orbit
pauses while a gesture is open. tests/orbit.test.ts pins every stage
headless.

In a component tree, skip the wiring: `<OrbitCamera input={input}
azimuth={1.2} distance={7} />` as a Scene child reaches the scene through
context (as a `<View3d>` child, that view: the context's `viewport` is
the nearest owner's), drives from the map in `input` (its `rotate`,
`zoom`, `pan`, `focus` actions, or the names in `actions`; live - a new
map reconnects) and nothing else, and runs a frame loop only while
`active()`, so a camera moved by drags alone keeps the app demand-driven
idle. The projection-dependent hooks are built here, since the
component sits where the projection is: `anchor` ("pick", the default:
the scene under the focal, else the plane at the target's depth;
"plane": that plane only; false: zoom toward the target, no pivot
re-seat) fills `zoomAnchor` and `rotateAnchor`, `occlusion={{ radius?,
layers?, meshes? }}` fills `occluder` from a raycast between target and
eye (mask the followed character out with `layers`, or name the level's
meshes); a hook prop overrides its half. The pose props are initial
values: runtime pose changes (and the debug-command hookup) go through
`ref`'s handle, whose set() and verbs push the pose and whose
glideTo/fit ease it. Every other prop is live - forwarded to the
control as a getter and read where it applies, never snapshotted - so
clamps, rates, `damping`, the follow's zones, `offset` and anchors
follow their props without a remount, and a clamp or offset change
re-clamps the pose at once. A `fit` from `ref` at mount reads the
scene's size of that moment: a fill-mode `<Scene>` has its creation
size until its first layout, so fit after the first frame (or hand a
fixed-size scene). `orbitBindings` is the viewer's wiring (a drag
rotates, Ctrl-drag, right-drag and two fingers pan, pinch and wheel
zoom, a double tap focuses); `mapBindings` the map's (a drag pans,
Ctrl-drag, right-drag and two fingers rotate), paired with `panPlane:
"ground"`.

## Shots and blends

`createShots(scene | view, { blend? })` (core's blender over the
scene's CameraState, Cinemachine's virtual cameras): `shot(name, {
priority? })` returns a target an orbit or first-person control drives
instead of the scene (its `camera()` is the shot's own, its `size()` the
owner's) - a chase camera and a cinematic one, each with its own
control - and `activate(name, { blend? })` makes the highest-priority
enabled shot live, blending the scene from the current output over
`blend` seconds (default 0.5; 0 cuts; position, target, up, fov, near
and far linear, two ortho extents linear, a perspective-to-ortho switch
cuts at the midpoint; `mixCamera`); `deactivate` falls back by
priority. A live shot's pushes reach the scene at once; a blend needs
`update(dt)` while `active()`, the controls' rule. `live()` and
`camera()` read the state. A switch mid-blend starts from the output of
that moment, so a quick back-and-forth never jumps. Every shot starts
from the owner's camera at creation. The component form: `<Shots
blend? ref?>` as a Scene (or View3d) child with `<Shot name priority?
active? blend?>` children, each providing a context whose `viewport` is
the shot's recording target, so an `<OrbitCamera>` or
`<FirstPersonCamera>` inside a `<Shot>` drives that shot; `active`
(default true, live) withdraws and restores a shot, an unmount removes
it, the container runs the blend on frames while one is in flight. A
shot's picks and raycasts (`anchor`, `occlusion`) go through the
owner's actual camera, which is the shot's own only while it is live
and at rest. Pinned in tests/orbit.test.ts.

## First-person camera

First-person control: `createFirstPersonCamera(scene, { position?, yaw?,
pitch?, min/maxPitch?, moveSpeed?, boostSpeed?, lookSpeed?, fly?,
clampPosition? })` -
a position plus yaw/pitch (yaw 0 faces -z, positive turns left; pitch
positive looks up), Unity's FirstPersonController shape (look AND move in
one control) where Three splits PointerLockControls from a hand-written
key loop. Its `axes`: `look` (a delta of one element height sweeps half a
turn - a drag across the screen turns the walker around - and the
pointer feed's `mouseDelta` delivers mouse motion under pointer lock in
the same unit; a rate turns at 0.4 turn/s), `move` ([right, forward],
forward = -y: a stick pushed up or W reads [0, -1]; a rate walks at
`moveSpeed`, a delta is a step in world units, diagonals clamped to unit
length so they walk no faster), `rise` (world up at `moveSpeed`, fly
mode only) and `boost` (an axis, since the control has no button kind:
a bound button reads 1 while held, and any non-zero read multiplies the
move and rise RATES by `boostSpeed`, default 2 - the sprint; a delta
stays a one-unit step; `firstPersonBindings` puts it on Shift and the
left stick's press). Walking (the default) flattens the heading onto the ground
plane at fixed height; `fly` moves along the view. The verbs:
`lookBy(yaw, pitch)` radians, `moveBy(right, forward, up?)` in the
walker's frame, `set(pose)`, and `glideTo({ position?, yaw?, pitch? })`,
which eases there inside update(dt) (yaw to the number given, not the
shortest turn), dropped by any input and by a set() of a pose field -
the orbit camera's glide, without its damping: this control's unbracketed
delta is mouse motion under pointer lock, and that is never eased.
`interrupt()` stops a glide where it is, the same verb as the other
controls'. `shake(strength, duration, { frequency?, direction? })` is the lane
every control has (okf/design/camera-controls.md), here a view KICK in
TURNS of yaw and pitch (the look axis's unit; 0.01 is 3.6 degrees), the
first-person convention: a hit, a recoil, a footstep; the pose and the
eye are untouched. Every
option but the initial pose is read
where it applies (`fly` per step, `clampPosition` per move, the rates and
pitch clamps per input), so a field changed on the options object takes
effect on the next move: walk and fly are one control. The control NEVER
calls `lockPointer` - click-to-lock and Escape-to-release are the app's
window-level decisions (see `examples/first-person.tsx`) - and has no
collision of its own: `clampPosition(next, current)` is the whole hook -
bounds, a floor height, or `moveAndSlide` over `next - current` against
the collision layer (see `examples/collision.tsx`, whose `jump` is one
more action on the same map: Space or the pad's south button, read by
name); a glide consults it every frame as a walk does every step. Call
`update(dt)` from onFrame; `active()` is reactive - a rate driving (a
held key, a deflected stick), or a glide in flight - and gates the loop.
The map
tracks held keys from the down/up pair through `input.handlers`; spread
its `onBlur` too, since the up never arrives once focus has left.

`<FirstPersonCamera input={input}>` as a Scene child wires the control
to the scene and the map, nothing else: the app spreads
`input.handlers` on the window (keys bubble there from wherever focus
is, so a click on the scene needs no focus dance) and hands the scene
its `pointer` feed. The frame loop runs only while `active()`. Every prop
but the initial pose is live: `fly={flying()}` toggles walk/fly on the
running control (pose carries over, no remount, and `clampPosition` may
swap with it), `moveSpeed`/`lookSpeed` follow their props, and a pitch
clamp change re-clamps at once. Like the orbit control it drives
position and yaw/pitch only: fov/near/far stay on scene.setCamera (or
the Scene `camera` prop), so a walker in a scene bigger than the default
`far` of 100 sets one there. tests/orbit.test.ts and
tests/first-person.test.ts pin both controls headless (they import
`@solidrt/core/input` only).

## Overlay projection

Overlay projection: `scene.project(point)` maps a world point to scene
pixels (top-left origin, y down - the output texture's own space; `w` is
the camera-forward distance in world units under either projection) and
returns null for a point at or behind a PERSPECTIVE camera's plane; an
ortho camera places every point (`w` may be <= 0 there - negative near
is legal ortho). It reflects a pending `setCamera` immediately, so
set-then-project in one tick is exact. `scene.unproject(x, y, w, out?)`
is its exact inverse: the world point at that pixel and camera-forward
distance `w` - project()'s `w` round-trips in both modes (the
drag-at-depth recipe: project the grabbed point once, keep its `w`,
unproject each move). `scene.viewProj(out?)` copies the view-projection
matrix for batch work. Never rebuild the camera matrices by hand for a
HUD.
