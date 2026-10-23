# @solidrt/3d - picking, collision and pointer events

Picking, raycasts, collision and pointer events on meshes. Read
../AGENTS.md first: it carries the model this file builds on.

## Picking

Picking: `scene.pick(x, y)` is project()'s inverse - the camera ray
through a scene pixel, returning `Hit[]` (`{ mesh, distance, point }`,
world units, nearest first; every hit along the ray, not just the front
one). `scene.raycast(origin, direction, opts?)` is the world-space
primitive under it, and `scene.screenRay(x, y)` the ray itself
(`{ origin, direction }`; direction's camera-forward component is 1, so
`origin + w * direction` = unproject) for intersection work pick cannot
do - drag planes, ground grids, filtered raycasts. All three cast
exactly the same ray. raycast's `opts` filters the query:
`{ layers }` (Unity's layerMask) replaces the scene's mask for this ray,
and `{ meshes }` (Three's intersectObjects) is an include-list; a
per-frame ground query passes one or the other instead of skipping
skyboxes and actors by hand.
The index and the narrowphase live in the spatial core: every attached
mesh's local box is a leaf in a dynamic AABB tree the flush refits from
the fresh world matrices (O(moved) per frame, a query O(log meshes)), and
an ordinary mesh is then tested per triangle against its geometry's
shape (one CPU copy of the positions per distinct geometry, created with
its GPU buffers and rewritten by `updateVertices`; the mesh's box comes
from the same copy, so a deforming mesh is found where it is drawn),
so hits carry `face`, `uv` and a world-space `normal` facing the ray, and
a ray through a knot's hole misses. A large geometry's triangles are
BVH-indexed too - built by the first ray that reaches the shape, log-cost
after - so raycasting a merged static scene stays cheap (see the batching
advice). An instanced mesh picks per instance (each a leaf with the
geometry's shape; its own node is never a hit, its `bounds` cull
only). A record mesh is box-only (its
explicit population bounds; records are opaque), and so is a mesh whose
geometry is not a triangle list (lines, points, strips have no triangle
narrowphase): it is tested by the
box's twelve triangles, so its hits carry the struck face's `normal` and
no `face`/`uv` - and a ray from inside it meets the far side, the
surface contract overlap/sweep share. Both methods
flush pending writes first (the lookAt/project immediacy contract), and
both skip invisible meshes. A sprite under a screen-size clamp
(`sprite({ minScreenPx, maxScreenPx })`) draws at a size the index does
not know - the vertex stage scales it by the pixels per unit at its
depth under the camera drawing it - so `pick` (which has a camera; a
view's pick its own) replaces the ray's hits on clamped sprites with an
exact ray-quad test at the drawn size, the quad rebuilt as the stage
builds it (camera right/up, or a fixed-y sprite's yaw basis), hits
carrying the plane distance and a normal facing the ray. `raycast` is a
world query and keeps the index's unit-box answer.

## Collision

Collision: `scene.overlap(volume, opts?)` and `scene.sweep(volume,
motion, opts?)` are the same index's other two questions - what a volume
touches, and where a moving one first touches: Unity's
OverlapSphere/Box/Capsule and SphereCast/CapsuleCast/BoxCast, Godot's
intersect_shape and cast_motion (Three has the raycaster alone; every
Three game adds a physics library or three-mesh-bvh for this). A
`Volume` is a sphere (`{ center, radius }`), a capsule (`{ a, b,
radius }`, the radius swept along the segment - a character) or an
oriented box (`{ center, halfExtents, rotation? }`). overlap returns
`Overlap[]` - `{ mesh, point, normal, depth }`, the deepest contact per
mesh: the point on the mesh, the unit direction out of it and the depth
along it that clears the contact (Godot's get_rest_info, Unity's
ComputePenetration, per hit; unordered) - and sweep `Impact[]` -
`{ mesh, time, point, normal }`, per mesh its first touch with `time`
the fraction of the motion, earliest first. Both take raycast's `opts`
(`{ layers, meshes }`) and test per triangle in WORLD space against the
same shapes, so any transform holds, an instanced mesh or sprite counts
by its box, and the physics-collider pattern is one `{ layers }` away.
Two contracts to know: a mesh is a SURFACE (a volume wholly inside a
closed mesh with no triangle in reach touches nothing - the trimesh rule
in every engine) where a box - a sprite, a bounded group - is a SOLID (a
volume whose center lies inside it is pushed out of the nearest face; a
record mesh's population box is a ray's alone, `setCollide` off, so a
body is never walled by a whole population), and a sweep from a volume
already in contact reports
time 0 only while the motion closes in; leaving or sliding along the
contact is no hit, which is what lets a slide along a wall proceed. A
zero motion touches nothing. The filters run in the core: every query
hands `flux:spatial` a `{ root, layers, nodes }` filter (the scene root,
the mask, the `meshes` include-list as node ids - an instanced mesh
contributing every instance node), and a mesh's `layers` is written to
its core node (`setLayers`), so a query never sees another scene's or a
2d layer's nodes and JS only maps hits back to meshes.
`scene.moveAndSlide(volume, motion, opts?)` (and the free
`moveAndSlide(scene, ...)` spelling of it) is the controller over them:
Godot's CharacterBody3D.move_and_slide and Unity's
CharacterController.Move as one PURE call - no node, no velocity; the
first-person camera composes it in `clampPosition`, a node-driven body
applies `result.motion` with setTransform. The loop runs in the spatial
core (`alloy/src/spatial/mover.rs`, one FFI call per body per frame; the
2d layer's mover is the same loop), the JS side only packs the volume
and maps hits back to meshes. It pushes the body out
of anything it starts inside, sweeps, stops a skin short, slides the
rest along the contact plane up to `maxSlides` times, then snaps down
onto a floor within `floorSnap` unless the motion rises - and the
`floor` it reports is the one it ENDS on (within `floorSnap` below), not
one it touched on the way. `MoveOptions`
adds `up`, `floorMaxAngle` (45 degrees: flatter is floor, steeper a wall
the body slides down), `maxSlides` (6), `skin` (0.01) and `floorSnap`
(0.1, 0 off) to the query filters; the result is `{ motion, floor, wall,
ceiling, hits }`. Gravity is the caller's - fold it into `motion` and
zero it while `floor` is set (the snap keeps reporting the floor while
the body stands still) - and a walkable floor absorbs the vertical part,
so a body never creeps down a slope it can stand on.
`examples/collision.tsx` is the whole pattern: a capsule walker through
`clampPosition`, gravity as a frame loop that runs only while airborne,
the level on a collider layer bit beside its drawn one, pickups lit by
one overlap per move. Deliberately absent and additive when asked: a
step offset (Unity's stepOffset; Godot has none either) and a cylinder
volume (Godot only).

## Pointer events

Pointer events (scene-pointer.ts): the element event model one tree
deeper, with the SCENE as the root of the walk (under a `<View3d>` leaf:
the view). `onPointerDown/Move/Up/Enter/Leave/Wheel/Tap` are plain
fields on any node (and Mesh/Group/Instance props); the nearest hit is
the target - the struck instance of an instanced mesh, else the mesh -
and down/move/up/wheel dispatch there, bubble through the ancestors and
END AT THE SCENE's listeners (`scene.listen({...})`, the `<Scene>`
`onPointer*`/`onWheel`/`onTap` props); over empty space the walk is the
scene alone, with `event.mesh` null (a tap there is the deselect idiom).
`event.mesh` stays the hit (`NodePointerEvent` in the chain,
`ScenePointerEvent` at the root with `mesh` nullable), `currentTarget`
the node whose handler runs (the scene at the end), `stopPropagation()`
stops the walk, and a stopped DOWN claims the whole press: that
pointer's move, up and tap never reach the scene either (the chain still
bubbles). That one rule is how a mesh drags itself under an
`<OrbitCamera>` without the view turning: stop the down, own the
captured moves - the control's pointer feed listens at the root
(`<Scene pointer>` does it, `feedPointer(scene, feed)` by hand) and sees
only what the nodes let through. Capture is per pointerId to the press
target, the scene included (a drag from empty space keeps delivering to
the root as it crosses meshes; a drag from a mesh keeps naming it, with
`point`/`distance` null while the ray misses it). Enter/leave fire on
the struck node alone - a group never receives them - while the root
sees every move, so "hovering empty space" is a root move with `mesh`
null. Taps are synthesized by the dispatch, as DOM click (Unity's click
handler fires for every button; ours, like DOM's, for the primary alone):
`onTap` fires after the up when a PRIMARY-button press (a right click
walks its down and up with `button` 2 but never taps, like core's press;
read `button` on the down to act on it) released on the target it
pressed within the slop (8 window px, core's recognizer slop, so a press
is never both a tap and a drag), was the only pointer down for its whole
press (a pinch never taps), with `tapCount` counting
repeats within 300 ms and 20 px on the same target (a double tap is
`tapCount === 2`); a release on another instance of the same mesh is no
tap. Wheel walks like a move with `deltaX/deltaY` (a node stopping it
keeps the zoom out). Every event carries `native`, the leaf's element
event, for core's recognizers, and the element fields (pointerId,
pointerType, button, modifiers, `timeStamp`, and `predicted`: a move to
draw with, not to measure with - core's AGENTS.md has both) plus `x`/`y`
in scene pixels
(screenRay's input - a drag plane is one intersection away). Root
listeners all run, in registration order (the root is the last stop,
nothing is left to claim). Wiring: the built-in `<Scene>` leaf carries
`scene.handlers` automatically (opt out: `events={false}`, which throws
at mount together with a `pointer` feed - the feed would listen at a root
no event reaches; the same on `<View3d>`); an `output`
leaf spreads `{...leaf.handlers}` (scaled to the ref'd leaf's layout
box per event) and an imperative composition `{...scene.handlers}` onto
the element showing the texture. `scene.handlers` assumes that leaf is
LAID OUT at the target size - true for the built-in leaf and a d-texture
at natural size, under any ancestor transforms or design-size fits (the
hit test undoes them; localX/localY arrive in the leaf's layout frame).
A leaf laid out at a different size uses
`scene.handlersFor(() => ({ width, height }))` with its layout size.
tests/dispatch.test.ts pins the walk, claiming, capture, hover, wheel
and tap rules headless; examples/pick.tsx is the live guard.

## Traps

- Picking is triangle-accurate for ordinary meshes and for instances
  (`point` is a surface point, hits carry `face`/`uv`/`normal`; an
  instanced mesh's own node is never struck, its `bounds` only cull)
  but box-only for record meshes and for lines/points geometry: there
  `point` is where the ray meets the population `bounds` box (the
  geometry's box for lines), `normal` that face's, and `face`/`uv` are
  absent. Never present an
  instanced hit as a surface hit. Both tiers run in the spatial core (Rust); never
  add a per-triangle path in JS - rays at mesh scale are
  interpreter-hostile, and the core already does it.
- `scene.handlers` vs `handlersFor`: localX/localY arrive in the leaf's
  LAYOUT frame (every ancestor transform and design-size fit is already
  undone by the element hit test). `handlers` therefore assumes leaf
  layout == target pixels; scaling by `getBoundingBox` would be WRONG -
  the box composes transforms, and it would double-correct the built-in
  leaf under a design size. Only a leaf whose layout size differs from
  the target needs `handlersFor`, fed the layout size the app itself
  set - which `leaf.handlers` from `output` does per event from the
  ref'd leaf's box.
- Hover (enter/leave) reacts to pointer MOTION only: a mesh animating
  under a still pointer fires nothing until the next move - the same
  limit the element hit test has (hit-test-per-frame is an open platform
  item). Do not poll pick() per frame to fake it.
- overlap()/sweep() test a mesh as a SURFACE, the trimesh contract: a
  volume wholly inside a closed mesh with no triangle in reach touches
  nothing, and a body whose center has passed through a wall reports
  the push-out on the side its center is on (a box - a sprite, a bounded
  group - is a solid and pushes out of its nearest face; a record mesh's
  population box is out of the volume queries). Keep a skin (moveAndSlide
  does) and never teleport a body into geometry expecting it to come
  out the far side.
- moveAndSlide owns no gravity and no velocity: fold the fall into
  `motion` every frame and zero it while `floor` is set. A walkable
  floor absorbs the vertical part (a body never creeps down a slope it
  can stand on), and with `floorSnap: 0` a standing body reports no
  floor unless its motion presses into one.
