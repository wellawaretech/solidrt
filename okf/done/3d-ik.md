---
title: Inverse kinematics - a solver stack in the spatial core's pose post-pass
description: Look-at, two-bone, chain (FABRIK and CCD) and foot-placement solvers declared once per model and stepped natively after the frame's JS, before the spring bones (Unity's Animator IK and Animation Rigging, Godot's SkeletonModifier3D stack, Unreal's Control Rig nodes), targets read from arena nodes or ground queries, weights ramped in core, so a character that aims, reaches, looks and plants its feet costs zero per-frame JS.
created: 2026-10-20
completed: 2026-10-20
---

# Inverse kinematics

## Problem

The pose pipeline is native from the clip players through the spring
bones, with one hole in the middle: anything that bends the primary pose
toward the world. A head that follows the camera, a hand on a door
handle, a weapon aimed at a cursor, feet planted on a slope - every one is
a per-frame JS read of world matrices (`getTransform`, `worldInto`, one
FFI round trip per ancestor), a solve, and a `setTransform` per joint. The
spring-bone plan named this class as the frame's JS primary producer and
placed the springs after it; it is the same cost that put the springs in
the core (a 34-joint chain was 0.8 ms per frame, nearly all of a
character's per-frame JS), paid per character per solver, and it scales
with the crowd. Unity (`Animator` IK and the Animation Rigging package),
Godot (`SkeletonModifier3D` and the IK modifiers that replace
`SkeletonIK3D`) and Unreal (Control Rig's TwoBoneIK, FABRIK, CCDIK, Aim
and foot placement) all run these natively after the animation pass; here
they are the last per-frame JS a rigged character pays.

## Done looks like

- `createIk(model, { solvers })` in `@solidrt/3d`: a stack of solvers
  over the model's joints, declared once, applied in order every frame.
  Four kinds, the engines' vocabulary:
  - `lookAt` (Unity `SetLookAtPosition` with body/head/eyes weights,
    Godot `LookAtModifier3D`, Rigging's `MultiAimConstraint`): a bone
    turns its forward axis to a target; optional distribution over a
    chain of bones with per-bone shares (spine 0.2, neck 0.3, head 0.5,
    the way the Unity body/head/eyes split reads); a cone limit with
    primary and secondary angle limits and the time-based ease Godot
    uses when a target leaves the cone (no snap at the limit); a world
    up or a node's up as the roll reference.
  - `twoBone` (Unity's hand and foot IK goals, Rigging's
    `TwoBoneIKConstraint`, Unreal's TwoBoneIK, Godot's two-bone
    modifier): root, mid and tip joints, an analytic solve; a pole (hint)
    target or a pole angle for the bend plane; target position weight
    and target rotation weight separately (the hand follows the handle's
    orientation only when asked); a soft reach band near full extension
    (Unreal's soft IK) instead of the pop at the limit; an optional
    stretch factor past reach (0 off).
  - `chain` (Godot's `SkeletonIK3D` FABRIK with a magnet, three's
    `CCDIKSolver`, Unreal's FABRIK and CCDIK, Rigging's
    `ChainIKConstraint`): a root-to-tip chain of any length with
    `method: "fabrik" | "ccd"`, max iterations and tolerance, a magnet
    (pole) for FABRIK, per-joint angle limits (hinge or cone, in the
    joint's rest frame) for CCD, and a tip rotation weight.
  - `feet` (Unity's foot IK pass, Unreal's foot placement, Godot's
    script-level standard): a group of two-bone legs whose targets come
    from GROUND QUERIES the core runs itself - a raycast down from each
    foot's animated position through the scene's index with a layer
    filter and rise/drop limits, the foot aligned to the hit normal with
    a per-foot weight that follows the clip's planted/lifted state via a
    height threshold, and a hips (pelvis) offset lowered so that both
    feet reach, eased per second. No JS raycast per foot per frame.
- Targets are nodes or points: `{ node }` (any `SceneNode` - a socket,
  a pickup, the camera, another character's joint - read from the arena
  at solve time, so a moving target costs nothing), `{ point, space? }`
  (world, or a node's frame), or for `feet` the ground. A target set from
  JS this frame is honoured this frame (the solve runs after the frame's
  JS).
- Every solver has a `weight` ramped in core (`setWeight(i, w, { fadeMs
  })`, the players' fade shape: frame-rate exact, no per-frame JS to
  blend an aim in and out), so the aim lifts off and the hand lets go
  smoothly; weight 0 costs no solve.
- The stack steps once per frame AFTER the frame's JS and its
  transitions, BEFORE the spring bones: the frame's JS sets targets and
  overwrites joints, the solvers bend the final primary pose toward the
  world, the springs lag behind the result. Unity's order is the same
  (animation, IK pass, LateUpdate secondary motion); Godot's modifier
  stack runs in child order after process.
- Frame-rate independence: the solves are stateless except the eases
  (weights, the look-at limit ease, the hips offset), all of which are
  per-second rates integrated over the frame delta.
- Scale and space: bone lengths are read from the pose as it stands each
  frame (an animated or ancestor scale is honoured); the solve runs in
  the chain root's parent space, so a model placed, turned or scaled in
  the world solves identically.
- A settled stack asks for no frames (target, chain and weights
  unchanged); a solver whose joint or target node dies drops, and
  `dispose()` returns the joints to the pose the players and the frame's
  JS wrote, as the springs' dispose does.
- `setSolver(i, update)` edits a solver's settings live (limits, pole,
  shares, method parameters); `setTarget(i, target)` moves its target;
  `reach(i)` reads the last solve's residual (distance left to the
  target) for gameplay that asks "can it reach" without a second solve.
- AGENTS.md "Inverse kinematics" section after "Spring bones" and a probe
  under `probes/` the way spring bones has one.

## Decisions

**Placement: after the frame's JS, before the springs.** IK is a
primary-pose refinement whose inputs (targets) the frame's JS sets, so it
runs after that JS; the springs are a function of the final primary
pose, so they run after IK. That makes the pose post-pass a two-stage
pass with a fixed order, not a list of equals: players (before JS), JS,
transitions, IK, springs, flush. The order is the engines': Unity's
`OnAnimatorIK` runs after the animation evaluation and before
`LateUpdate`, where SpringBone steps; Godot's modifiers run in child
order and its spring simulator is one of them, placed last by
convention. Consequence, documented as for the springs:
`getTransform` on a solved joint during the frame reads last frame's
solve, and a JS write to a solved joint is overwritten every frame while
the solver's weight is above 0 (Unity's rule: a script that writes a
bone the IK pass owns loses).

**Frames are matrices, not quaternions.** A quaternion composed down
the path cannot carry a mirroring ancestor (a negative scale, which
glTF exports of mirrored rigs do carry); the matrix does. So the solver
keeps only the world matrix per path node and reads every rotation it
needs off that matrix's orthonormal part: a bone's forward is the
matrix times the local axis, a world-space turn becomes the local
rotation `Pn^T D Pn` over the parent's orthonormal part (a proper
rotation even when that part is a reflection), and a tip's orientation
goal is read in Three's decompose convention (the x axis negated under
a negative determinant) so a target orientation under a mirrored arm
means what it means in Three. The mirrored-rig test pins every joint
of a reflected rig at the reflection of its plain twin.

**A stack, not a set.** Solvers run in declared order and each sees the
previous one's result, the Godot modifier-stack and Unity rig-layer
model. A look-at on the spine before a two-bone on the arm is the normal
case, and the only way a hand reaches a point that the turned torso moved
it toward. Unordered solvers would be simpler to schedule and wrong.

**Solvers in the core, not a generic constraint graph.** Unity's
Animation Rigging is a constraint graph (any transform drives any
transform); Unreal's Control Rig is a visual program. Both are editor
products over a runtime that ships the handful of solvers above. We ship
the solvers as named kinds with the engines' parameter vocabulary and
no graph: a constraint graph is a second scripting language, the
spatial core stays engine-independent and data-driven, and a solver kind
is additive (a `copyTransform` or `twist` kind lands without changing
the others).

**Two chain methods, both.** FABRIK is the natural-looking, fast solver
for long chains with a magnet (Godot's `SkeletonIK3D`, Unreal, Rigging's
`ChainIKConstraint`); CCD takes per-joint angle limits cleanly (three's
`CCDIKSolver`, Unreal's CCDIK, Godot's CCD modifier) where FABRIK's limit
handling is a known weak point. Shipping one and emulating the other's
strength is the least-effort path this plan does not take; the two share
the chain spec and differ in `method`, and limits are honoured by CCD
exactly and by FABRIK through a post-projection per iteration (the Godot
and Unreal approach), documented as the softer of the two.

**Two-bone is analytic.** Unity, Unreal and Godot all solve the two-bone
case in closed form (law of cosines in the bend plane), not by iteration:
exact, zero iterations, stable at full extension with the soft band.
The bend plane comes from a pole target when given, else from a pole
angle about the root-to-target axis seeded from the rest pose's bend
direction (Unity's hint-less default), so a leg bends forward without a
pole node per leg.

**Foot placement belongs in the core.** Every engine leaves it to a
script over raycasts, and every game writes the same script: two rays,
two IK goals, a pelvis drop, three weights. Here that script is per-frame
JS over the core's own raycast, exactly the cost class this plan removes,
and the core already owns the index the rays run through. So `feet` is
a solver kind whose targets are ground hits the core takes itself, with
the layer filter the physics-collider pattern uses. The planted/lifted
weight follows the foot's animated height over a threshold (Unity's
common recipe through animation curves, done without authoring), and a
clip that authors foot-plant curves can override it later through the
markers the animation-layers plan adds (additive). Both feet reaching
is the hips offset: the pelvis lowers by the larger of the two drops and
eases back, the Unreal foot-placement rule.

**Look-at limits ease in time.** Godot's `LookAtModifier3D` is the model:
a primary and a secondary angle limit about the bone's rest forward, a
symmetric or asymmetric cone, and when the target leaves the cone the
bone eases toward the limit (and back to rest when the target is lost)
over a duration rather than snapping. A hard clamp alone (Unity's
`clampWeight`) is the minimum every engine has outgrown.

**Targets are arena nodes first.** A node target is read from the arena
during the solve (pending writes included, as the springs read their
anchors), so a hand on a moving handle or a head on a walking NPC costs
no JS. A point target is a convenience over the same path (a world point
is a node-less endpoint). Nothing in the solver knows what a camera is;
the app passes the camera's node where there is one or feeds a point.

**Culling policy.** Solvers run whether or not the character is drawn
this frame, as the players and springs do; Unity's
`cullingMode = CullUpdateTransforms` is the additive follow-up once a
crowd measures it, and it should then apply to players, IK and springs
together as one per-model policy rather than three switches.

**No humanoid retarget, no full-body IK.** Unity's humanoid rig gives
IK goals by body part (left hand, right foot) over a normalized skeleton;
Unreal's Power IK solves the whole body at once. Both presuppose a
humanoid abstraction this package does not have (joints are named nodes;
`bindSkeleton` shares skeletons, it does not retarget). Joints are named
by the app, as Rigging's constraints and Godot's modifiers name them,
and a humanoid layer that maps standard names to joints is a separate
item above this one.

**Scope beyond the parity row, on purpose.** The parity row says "IK";
the four kinds, the limits, the ease, the soft reach and the ground
queries are what a port actually reaches for in the three engines, and
each would otherwise come back as a per-frame JS workaround. Everything
deferred (culling mode, humanoid naming, foot-plant markers, more solver
kinds) is additive.

## Work

1. Core: `alloy/src/spatial/ik.rs` - the solver stack per model root
   (`create_ik/destroy_ik/set_ik_solver/set_ik_target/set_ik_weight/
   advance_ik`), the four kinds, the eases, the ground query through the
   existing BVH and pick path with the `QueryFilter`, residuals; tests in
   `alloy/src/tests/spatial_ik.rs`: analytic two-bone against a brute
   solve, FABRIK and CCD convergence and limit honouring, look-at cone
   and ease over substeps, foot placement on a slope, order against the
   springs (a spring chain hanging from an IK'd bone lags the solved
   bone).
2. Plugin: `flux:spatial` `createIk/destroyIk/setIkSolver/setIkTarget/
   setIkWeight/readIkReach`, the `advance_ik` step in the pose post-pass
   of `flux/src/alloy_plugins/spatial.rs` `tick` ahead of
   `advance_springs`; `packages/flux-types/gui/spatial.d.ts`.
3. Package: `packages/3d/src/ik.ts` (`createIk`, the specs, the enter
   hook on the model node the springs use), exports, AGENTS.md section
   after "Spring bones", the trap list (last frame's solve on read, the
   solver owns its joints, feet want a collider layer).
4. Probe `probes/ik-probe.tsx`: a rigged model with a look-at on the
   head toward an orbiting node, a two-bone arm on a moving handle, a
   FABRIK tail, feet on a sloped floor; settle demand and residuals read
   through the control API; a frame-stepped snapshot per solver.
5. Example `packages/3d/examples/ik.tsx` from the probe, and the parity
   note's IK row re-linked and recounted.

## Findings

The engine facts this work surfaced are cut to
[3d-ik-findings](../notes/3d-ik-findings.md). What stays is the one
mechanism that only makes sense with this plan in place: solving from
the primary pose needs no pose-reset protocol with the players. A stack
keeps, per joint, the rotation it last wrote and the input it last
solved from; at the frame's post-pass a joint whose rotation differs
from the stack's last write was written by something else (a player, a
transition, the frame's JS) and that write is the new input, otherwise
last frame's input stands. So a static pose under a half-weight aim
holds (the test pins it), a player's frame-by-frame writes are the
inputs, and a JS write to a solved joint is solved over rather than
lost. Unity gets the same by re-evaluating the animation every frame;
Godot's modifiers run over a pose the skeleton re-takes.
