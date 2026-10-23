# @solidrt/3d - animation

Retargeted motion, animation clips and the mixer, root motion, spring
bones and inverse kinematics. Read ../AGENTS.md first: it carries the
model this file builds on.

## Retargeted motion

RETARGETED motion is native: `setTransition(node, { position:
{ duration: 400 }, ... })` makes setTransform writes TARGETS the core
animates toward every frame (position/scale per lane, rotation along
the quaternion geodesic - a spring keeps its velocity through
retargets), so a mesh gliding to a slot or a camera rig easing costs
one JS write per target change, zero per frame - for the mesh, its
lighting and its shadow alike (a casting light on a transitioned group
re-places its shadow cameras in the scene's sync, which runs every
frame). One rule for every native mover, transitions and clip players
both: the core steps running motion BEFORE the frame's callbacks and
starts what they declared (an enter from its `from`, an exit) AFTER
them and their flush - Unity's animation update between Update and
LateUpdate - so `worldPosition`, `getTransform` and a late-pass follow
read the pose the frame draws (a node created in a frame's callback
reads its created pose there and its `from` from the late pass on),
and a raw `setTransform` made in the frame wins that frame (the next
step overwrites it again - the producer rule, last write wins). The declaration lives
on the SceneNode and re-applies on every scene enter; the pose a node
enters with snaps, unless a component's `from` (`position: { duration,
from: [x, y, z] }`, a quaternion for `rotation`) animates it in from
there - at every scene enter, since each enter creates the core node
anew. A component's `exit` is where it animates to when `destroy` lets
go of the node: it stays drawn while it leaves and is a GHOST meanwhile -
no pick, raycast, overlap, sweep or pointer event sees it - then frees
when the last exit settles; children go first and the parent stays their
frame until the last of them has settled, so a dying character's parts
leave in its frame (a mixer driving its joints keeps driving them). Either
endpoint takes `{ value, duration?, curve?, bounce?, delay? }` to own its
direction's motion (an ease-out enter, an ease-in exit), and `delay` on
an entry holds its writes on the animation clock (a late frame catches
up). `stagger` (ms) goes on an ANCESTOR's declaration (a `<Group
transition={{ stagger: 40 }}>`), never on the animating nodes: it spaces
the enters and exits of its descendants that begin in one frame by
`index * stagger` (add order in, tree order out whatever order the
unmount lets them go in; nearest declaring ancestor wins) and cascades
nothing unless they declare `from`/`exit` -
a squad of instances spawning in is one `stagger` on their group, and
for the nodes straight under the scene root it is `<Scene stagger>` /
`createScene({ stagger })` / `scene.setStagger(ms)` (a declaration on
`scene.root`). A populated
mesh unmounts as `destroy(mesh)` then `disposeInstances(mesh)`, and the
dispose waits for a mesh still animating out, so an `<Instance>`'s exit
plays through its `<InstancedMesh>`'s unmount (`model.dispose` waits the
same way); the scene's and a layer's `dispose` cut a corpse short - the
whole target is going. `examples/exits.tsx` is the live example: crates
that pop in and shrink away on tap, the shelf cascading out and back
through `<Show>`. Each natural settle calls the node's `onTransitionEnd`
(plain field like the pointer handlers) with `{ component }` - never on
a cancel, snap, leave or exit; the raw "spatialTransitionEnd" engine
event (sol:events, carrying the CORE node id `_node`) stays for
flux:spatial consumers.

PAUSE and time scale are a node declaration too: `setTimeScale(node,
scale)` / the `timeScale` prop sets the rate the native motion under the
node runs at - transitions with their delays, enters, exits and stagger
slots, morph weight tracks, and the clip players of the models under it
(a mixer's players are clocked on their model's node, so this is Three's
`mixer.timeScale` as well). 0 freezes, 1 is app time, 0.5 is slow
motion; `scene.setTimeScale` / `<Scene timeScale>` declares it on
`scene.root`, the whole-world pause. The NEAREST declaring ancestor
wins, never a product (Godot's process_mode shape): a world at 0 with a
`<Group timeScale={1}>` inside keeps that group running - the spinning
preview on a pause screen, which a product could not express. A frozen
scene asks for no frames (the loop idles; the write that resumes it
brings the frame), writes while frozen behave as always (undeclared
components snap, declared ones retarget and wait), a `destroy` with an
`exit` stays a ghost until its time moves again (a scene `dispose`
still cuts it short), and a node created frozen sits at its `from`. The
scale covers the CORE's motion only: your own `onFrame` logic, timers,
video and audio stay on app time - gate them yourself, multiplying your
step by `timeRate(node)` / `scene.timeRate()`, the effective rate read.
The dev clock (`/clock?scale=`) scales app time underneath all of this.

## Animation

Animation: `createMixer(model)` plays `model.clips` by name through
the spatial core's RIG - the core's animation state for one model
(Unity's Animator, Godot's AnimationMixer, Unreal's AnimInstance): the
joint table, the rest pose, ordered layers, the players on them and
the sync groups. `mixer.play(name, { loop?, speed?, fadeMs?, blend?,
layer?, mask?, start?, end?, time?, weight?, exclusive?, fadeOutMs?,
additiveReference?, inPlace? })`, `mixer.stop({ fadeMs?, layer? })`,
`mixer.playing(layer?)`, `mixer.onFinish(name, layer)` for `loop: false`
clips (the pose holds at the end), `onLoop(name, layer)` on every wrap.
Playback is CORE-DRIVEN: there is no update() and no frame loop to
register - clips are registered with the spatial core once and the rig
samples, blends and writes joint TRS natively each frame, so a playing
character costs zero JS per frame (gate other work on
`mixer.playing()`). play() requires the model to be IN a scene (the rig
binds live arena nodes; removing the model drops it - a re-added model
plays again from play()). The rig runs on the model node's time:
`setTimeScale(model, 0)` (or on any ancestor, the scene root included)
pauses every clip, fade and transition of the model where it stands
and resumes it from there - Three's `mixer.timeScale`, Unity's
`Animator.speed` under `Time.timeScale` (see Retargeted motion).

THE BLEND, per joint and per layer in order (what every engine's tree
flattens to; `okf/done/3d-animation-layers.md` has the reasoning):
the BASE layer's players blend over the REST pose - their weights
normalized when they sum to one or more, and below one the rest shows
through for the remainder, so `stop({ fadeMs })` fades to rest instead
of popping, and a player lacking a channel another player on its layer
animates contributes rest for it (no snap when clips of different
coverage crossfade); each higher "override" layer composes its own
blend OVER the result by its weight times its mask's share per joint;
each "additive" layer adds its players' deltas. A joint no player on
any layer animates keeps its pose. LAYERS: `createMixer(model, {
layers: { aim: { mode: "override", mask: { include: [{ node: "spine",
subtree: true }] } }, hit: { mode: "additive" } } })` or
`mixer.layer(name, opts)` (ordered by first declaration; the base is
"base"), `play(name, { layer })`, `setLayerWeight(name, w, { fadeMs })`
ramped in core (an aim switched on and off with no per-frame JS; a
layer at 0 costs no sampling), `removeLayer(name)`. MASKS (Unity's
AvatarMask, Godot's filters, Unreal's blend masks): `{ include?,
exclude?, soft? }` of joints by node or name, `subtree: true` taking
every joint under one (the spine-up mask is ONE entry), `soft: [{
node, share }]` for a partial share (a 0.5 on the shoulder so the mask
does not crease); per layer, or per play (`play(name, { mask })`, the
two multiply). ADDITIVE clips are turned into deltas against a
reference once, in core, and cached: `additiveReference` "first" (the
clip's own first frame, the default - author the flinch from the
pose it leaves), "rest", or `{ clip, time }`; positions add, rotations
compose, scales multiply, morph weights and curves add, the weight
slerping the delta from identity. A ONE-SHOT on a layer: `play(name, {
layer: "hit", loop: false, fadeOutMs: 150 })` fades out before its end
in core and returns to what is under it (Godot's OneShot, Unreal's
montage blend-out), onFinish still firing at the end.

TRANSITIONS blend two ways. `fadeMs` with the default `blend:
"crossfade"` fades the clip in and the others on its layer out (both
sampled meanwhile - Unity's CrossFade, Godot's play-with-blend). `blend:
"inertial"` is Unreal's inertialization: the clip starts at full weight
at once and the core decays the pose's offset from where it was, with
its velocity, on a quintic that lands with zero velocity and never
overshoots - one sample per frame, no averaging of two cycles, so it
is THE transition for locomotion (walk to run, run to crouch) and the
one that hides any jump (`mixer.inertialize(ms)` before a seek, a
group change or a layer switched on). SYNC GROUPS (Unreal's; Unity's
blend-tree time warp): `mixer.group(name, ["walk", "run"], { sync?:
"time" | "markers" })` keeps the named clips in phase whenever they
play together - the heaviest leads, the others follow its normalized
time, or the markers common to every clip (left and right foot plants)
with `sync: "markers"` - so a walk-run blend does not foot-slide.
`createBlendSpace(mixer, { samples: [{ clip: "walk", position: 1.5 },
{ clip: "run", position: 4 }], layer?, sync?, fadeMs? })` is the
weight policy over such a group: 1D between neighbours, 2D by
Delaunay triangles (Godot's blend spaces, Unity's blend trees), `set(
position)` a few weight writes, nothing per frame; `play(name, {
weight, exclusive: false })` and `setWeight(name, w, { fadeMs })` are
the pieces it is made of. SUBCLIPS: `play(name, { start, end })` in
seconds (Three's subclip, Godot's play_section) - the time stays
within the window, loops within it, finishes at `end`; `time(name)`
reads a playing clip's time, `seek(name, t)` scrubs (clamped),
`setSpeed(name, s)` retimes. MARKERS (Unity's AnimationEvent, Unreal's
notifies and notify states, Godot's markers): `ModelClip.markers`
from the file (a glTF animation's `extras.markers: [{ time, name,
duration?, data? }]`, kept by the `sol tool 3d/model` bake) or
`mixer.markers(clip, list)`; the core fires `mixer.onMarker({ clip,
name, phase, time, data?, weight, layer })` when a player crosses one -
once per crossing however long the frame, loop wraps and reverse play
included, with the player's effective weight so you threshold a
footstep at 0.01 yourself; a marker with `duration` is a WINDOW
(`phase` "begin"/"end", the end also fired on a stop, a drop or a
finish inside it; `mixer.inside(clip, name)` reads it), a seek fires
none of the instants it jumps over but ends the windows it leaves and
begins those it lands in. CURVES (Unreal's animation curves): a clip's
named float tracks (`ModelClip.curves`, a glTF animation's
`extras.curves: [{ name, times, values, interpolation? }]`) blend like
the pose into `mixer.curve(name)`, drive an IK solver's weight in
core (`ik.setWeight(i, { mixer, curve: "footPlant" })`) and drive a
SHADER PARAM in core: `mixer.bindCurve("glow", mesh, "uGlow")` routes
the curve into that scalar param of every entry the mesh draws with
its material, `mixer.bindCurve("exposure", scene, "uExposure")` into a
scene-wide param, written at the flush whenever the value changed (the
way uModel reaches an entry), kept across re-enters; `unbindCurve`
removes it. The rest pose the layers compose over is each joint's
DECLARED rest (a model's file transform, published to the core at
every scene enter; the spring chains settle to the same), else the
pose when the rig is made. A rig over any nodes, for clips authored in
code: `createMixer({ root, nodes, clips, rest? })` with channels
indexing `nodes` (`rest` declares the nodes' rest); `mixer.dispose()`
frees it.

Three traps that follow from core ownership: (1) the rig advances
BEFORE your onFrame, which is therefore the post-animation hook - read
a freshly posed joint and overwrite it (skeleton copies, a procedural
tweak) in plain setTransform, last write wins, all published by the
same flush; (2) the JS `position`/`quaternion` fields of rig-animated
joints (and of the model node under root motion "apply") go STALE
(they hold the last JS write) - read poses with `getTransform(node)`,
which reads the core; writes to such a node always go through
(setTransform skips its usual equal-value short-circuit there, so a
teleport back to the last JS-written spot is not lost); (3) a channel
nothing plays leaves the node's pose alone, and an additive delta on a
joint no override layer animates applies over the REST pose; the draw
sort follows native pose writes like any move (the core re-keys what
it recomputed), so an animated transparent sorts as it animates. Not
here, additive when wanted: model-space blends (Unreal's mesh-space
additive for aim offsets under a bent spine), culling of off-screen
rigs, humanoid joint naming and mirroring.

## Root motion

Root motion: `play(name, { inPlace? })` strips a clip's root travel
(Unity's applyRootMotion off, Godot's root_motion_track): the root's
x/z hold at the clip's first key and its height rebases onto the root's
rest position, the vertical bob intact. `createMixer(model, {
rootHeight })` rebases EVERY clip onto that height, pinned or not, for
an export whose clips ride above its rest pose. Unset, it is decided per clip by NET DRIFT of the
root's position track - last key minus first, past a fraction of the
model's height - so run cycles play in place and a taunt that roams and
returns does not (pinning THOSE pushes the slide into the feet);
`mixer.travels(name)` reads the verdict. The root is the topmost node
any position channel of the clip targets, provided the clip animates
something beneath it too: a clip translating one lone joint (a sliding
prop, a test track) has no root and is never pinned, and an additive
clip is never pinned either (its deltas carry no travel). The strip is baked into a
second core clip on first in-place play, so it costs no per-frame JS and
crossfades like any other clip. A GAME wants the travel kept and moved
onto the character: `createMixer(model, { rootMotion: "apply" |
"report" })` plays every clip fully pinned (all three root axes held)
and its yaw held too (the turn about +y stripped key by key, so the
lean and pitch of the pose survive), while the core samples the
authored root tracks at each player's time and hands the per-frame
delta on - a translation in the model's local frame and a yaw in
radians. "apply" adds both to the model node itself (Unity's
applyRootMotion: the character walks and turns where its clip says),
"report" accumulates them until `mixer.rootDelta()` takes them
(`{ position, yaw }`), for a controller to spend through its own
movement (Godot's get_root_motion_position). Zero per-frame JS in
"apply"; one read per frame in "report". Loop wraps are continuous
(the clip's net drift is added across the wrap), the deltas are
weighted by the player's EFFECTIVE weight (its own times its layer's
times the root's mask share: a legs-only base keeps the travel, an
upper-body override contributes none, an additive clip never does), and
a clip's travel is given in the root's
CURRENT facing (its own turn so far undone), so a turn that wanders
out and back ends where the clip says and two blending clips agree on
the frame. The object form `{ mode, up?, vertical? }` names the root's
parent-space up axis (default +y; the turn and height axis) and
`vertical: "pose"` keeps the height in the pose, delivering only the
horizontal travel (Unity's bake-into-pose Y - for a controller that
owns gravity). `inPlace` is ignored while rootMotion is set. Yaw is
the swing-twist about up (exact under any lean); a cubic rotation
track is linearized at 60 keys/s before its yaw is held. Verified on
Mixamo's standing turns (external/mixamo-turn).

## Spring bones

Secondary motion - hair, a hood, a skirt, a tail, a cable: chains that
lag behind the bones they hang from - is `createSpringBones(root, {
colliders?, springs })` (`root` a model, or any node whose subtree holds
the joints: a group of plain nodes, as createMixer's rig over nodes),
solved in the spatial core (Unity's SpringBone, Godot's
SpringBoneSimulator3D, three-vrm's VRMSpringBoneManager) with
VRM's vocabulary, so a VRM's `secondaryAnimation`/`VRMC_springBone`
values go straight in: each spring is `{ joints, colliders?, center? }`,
its joints PARENTS FIRST, each `{ node, tail?, stiffness?, drag?,
gravityPower?, gravityDir?, hitRadius? }` (defaults three-vrm's: 1,
0.4, 0, -y, 0; `tail` is where the chain continues in the bone's own
space, defaulting to the next joint's rest position or VRM's 7 cm
virtual tail for a leaf), `colliders` indices into the options' list of
`{ node, shape: "sphere" | "capsule" | "plane", ... , inside? }` (offsets
in the collider node's space; `inside` keeps the tails WITHIN a sphere
or capsule, a skirt around the legs), and `center` the node whose space
the chain is solved in (its own motion then does not swing the chain -
VRM's center; default world space). A bone turns from its DECLARED
rest rotation (a model's file transform, published at every scene
enter: the same rest a mixer's layers compose over), else from its
rotation at the call. The handle has `reset()` (every
tail back at rest under the current pose, what a teleport wants),
`setJoint(chain, joint, { stiffness?, drag?, gravityPower?,
gravityDir?, hitRadius? })` (the AUTHORED settings, for tuning and
states - wet hair raises drag; the chain goes on from where it is, and
a re-enter keeps the values), `setForce(force, chain?)` (the RUNTIME
force on every tail, world space, the units of gravityPower along a
direction, added to each joint's authored gravity: the wind - Godot's
external_force, UniVRM's ExternalForce - on the whole model or one
chain; write it per frame for gusts, a constant one settles) and
`dispose()` (the chains dropped and the joints put back at rest, so a
"hair physics off" switch is dispose, and on is a new declaration). Zero
per-frame JS: the core steps every chain once per
frame AFTER your onFrame and its flush - the POSE POST-PASS, where
Unity's LateUpdate and Godot's skeleton modifiers run - so the chains
react to the final primary pose: the players, the transitions and the
frame's own writes (a look-at, an IK, a controller's move). Three
consequences: (1) the chain owns its joints' rotations - a setTransform
on one is overwritten at the step; (2) `getTransform`/`worldPosition`
on a joint during the frame read the PREVIOUS frame's solve (anything
attached under a joint follows through the hierarchy and needs no
read); (3) the model's time scale freezes its chains with its clips.
The solve is frame-rate exact: the free motion is the closed-form flow
of three-vrm's 60 Hz step (authored values mean what they mean there)
and the length and collider projections run at 240 Hz substeps, so a
50, 60 or 120 Hz display draws the same motion with no added latency.
Lengths follow the matrices (a scaled ancestor scales bone lengths, hit
radii and collider radii alike). The declaration lives on the model
node and re-applies on every scene enter, like setTransition; a settled
chain asks for no frames. The VRM extension itself is the app's to
parse (no reader here yet); `okf/plans/3d-spring-bones.md` holds the
design.

## Inverse kinematics

Anything that bends the animated pose toward the world - a head on the
camera, a hand on a handle, a weapon on a cursor, a tail on a point,
feet on a slope - is `createIk(root, { solvers })` (`root` a model, or
any node whose subtree holds the joints, as createMixer's rig over
nodes), a stack of solvers solved in the spatial core (Unity's Animator
IK and Animation Rigging, Godot's SkeletonModifier3D stack, Unreal's
Control Rig nodes) in their vocabulary. Four kinds, `{ kind, ..., weight? }` each: `lookAt` (`bones`
root first - spine, neck, head - each a node or `{ node, share }`; the
shares distribute the aim, the last bone reaching the target exactly;
`forward`/`upAxis` the bones' own axes, default +z/+y; `up` a roll
reference; `limit: { yaw, pitch, damp? }` about the last bone's rest
forward, a number symmetric or `[min, max]`, radians, Godot's
LookAtModifier3D with its damped approach; `easeMs`, default 600, the
transition through rest when the target switches, is lost or flips
behind the head - no snap across the limit), `twoBone` (`root`, `mid`,
`tip` - shoulder, elbow, wrist - closed form; `pole` the bend plane,
else the animated bend holds; `poleAngle`; `positionWeight` the reach
and `rotationWeight` the grip on the target's rotation; `soft` a reach
band near full extension instead of the pop, Unreal's soft IK;
`stretch` past it), `chain` (`root` to `tip`, every node between a
joint; `method` "fabrik" with a `magnet` or "ccd" for exact per-joint
`limits` - `{ hinge, min, max }` or `{ cone, twist? }` in the joint's own
frame - `iterations`, `tolerance`, `rotationWeight`) and `feet` (`hips`
plus `legs: [{ root, mid, tip, height? }]` whose targets are GROUND HITS
the core takes itself: a ray `rise` above each animated foot down to
`drop` below it on `layers` - REQUIRED, a collider layer the model's own
meshes are not on, as `scene.raycast` takes it - the sole `height` below
the ankle (measured from the root's ground plane at the declaration,
so declare at rest), a foot planted within `plantHeight` of its rest
height and lifting over `plantBand` with the clip, the hips moved along
`up` by the planted foot that needs it most so both feet reach, eased by
`hipsEaseMs`, each planted foot aligned to its ground normal by
`rotationWeight`; no JS raycast per foot per frame). Targets are
`{ node }` (ANY scene node - a socket, a pickup, the camera's node,
another character's joint - read in the core at solve time, so a moving
target costs nothing) or `{ point, rotation?, space? }` (world, or a
node's frame); poles and magnets likewise without the rotation. The
handle has `setWeight(i, w, { fadeMs? })` (ramped in core: the aim
lifts off and the hand lets go smoothly; 0 costs no solve),
`setTarget(i, target | null)` (a point moved to another point tracks,
feed a cursor every frame; a lost look-at eases back), `setSolver(i,
update)` (limits, pole, shares, method parameters, the joints
themselves, live), `reach(i)` (the last solve's residual - distance
left, or the look-at's angle - for "can it reach" without a second
solve) and `dispose()` (the joints back to the pose the players and
your frame wrote). Zero per-frame JS: the core steps every stack once
per frame AFTER your onFrame and its transitions and BEFORE the spring
bones - the POSE POST-PASS, Unity's order (animation, OnAnimatorIK,
LateUpdate) and Godot's (process, modifiers in child order) - so your
frame sets targets and overwrites joints, the solvers bend the FINAL
primary pose toward the world, and the springs lag behind the result.
The solvers run in declared order, each over the previous one's result:
a lookAt on the spine before a twoBone on the arm is the normal case,
and the only way a hand reaches a point the turned torso moved it
toward; `feet` goes FIRST, so the limbs and chains solve under the
moved hips. Four consequences: (1) every frame each solver solves from the
PRIMARY pose - what the players, the transitions and your frame wrote -
not its own last result, so a static pose under a half-weight aim holds
instead of creeping toward the full one, and a setTransform on a solved
joint in the frame is solved over, not lost; (2) `getTransform` /
`worldPosition` on a solved joint during the frame read the PREVIOUS
frame's solve (a node attached under it follows through the hierarchy
and needs no read); (3) the model's time scale paces the eases with its
clips; (4) solves run in world space over matrices composed on the fly,
so a placed, turned or scaled model solves identically and bone lengths
follow the pose (an animated scale is honoured). The declaration lives
on the model node and re-applies on every scene enter, like
setTransition; a settled stack (targets, joints and weights unchanged)
asks for no frames. Not here, additive when wanted: a humanoid layer
that names joints by body part (Unity's goals), whole-body IK, culling
of off-screen solves. `okf/done/3d-ik.md` holds the design.
