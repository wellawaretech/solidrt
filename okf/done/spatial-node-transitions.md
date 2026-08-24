---
title: Native transitions on spatial node transforms
description: A spatial arena node moves only when something writes its local TRS every frame, so a mesh gliding somewhere or a sprite springing to its square costs per-frame JS plus an FFI write per node - the rendertree's shipped native transitions have no spatial analogue. Build the producer that animates node TRS in core (rendertree transition math reused, quaternion tracks the one new piece), making JS write targets once; the linchpin of 2d-spatial-citizenship and the smaller sibling of animation-core.
created: 2026-08-24
---

# Native transitions on spatial node transforms

## Symptom

A spatial arena node has no motion of its own: something must write its
local TRS every frame. Today that something is app JS in `onFrame` - an
interpreted loop plus a ~7 us `set_transform` crossing per moved node per
frame (spatial-core bench) - for every mesh gliding to a slot, camera rig
easing, light sweeping, or door swinging. The rendertree solved exactly
this shape with native transitions ([done](../done/native-transitions.md):
~10 us/element/frame of JS became one write per TARGET change, Rust
interpolating every frame, settled animations producing no frames), and the
spatial arena - the module that exists to take per-frame work below the
interpreter line - has no equivalent.

It is also the named linchpin of
[2d-spatial-citizenship](2d-spatial-citizenship.md): sprites as arena nodes
are a motion REGRESSION until retargeted motion costs zero JS and zero FFI
per frame, which is precisely this item. [spatial-core](spatial-core.md)
lists it as a future consumer ("the spatial analogue of the 2D tree's
shipped native transitions"); [animation-core](animation-core.md) calls it
its smaller sibling. Neither gives it a home; this file is it.

## Shape: a producer, and mostly existing math

The producer model animation-core defined, in miniature: per frame, before
`flush()`, advance the active tracks and write node-local TRS through the
same `set_transform` path JS uses. The spatial module needs ZERO changes -
dirty propagation, sinks, BVH refits and the transparent-order check just
see moved nodes. The sink admissibility rule is not in play (producers
write inputs, not outputs).

- **Spec and semantics: identical to the shipped element transitions.**
  Durations in ms; `{ duration }` / `{ duration, bounce }` is a spring
  (the default kind, chosen there because it is the retargeting-safe
  primitive - doubly true here, where drags and pursuit retarget
  constantly); `{ duration, curve }` is a tween; a non-numeric write
  cancels and snaps; natural settles fire a completion the JS side
  delivers like `onTransitionEnd`. One vocabulary across the tree and the
  arena, one mental model for app authors.
- **The math already exists**: tween curves and the perceptual spring
  oscillators live in alloy `rendertree/transitions.rs`, lane-vector
  tracks and all. Position and scale are plain lanes. Factor the
  oscillator/curve core out for both consumers rather than duplicating it
  - it is rendertree-independent already in substance.
- **Rotation is the one new piece.** Quaternion tracks do not reduce to
  independent lanes: a tween is a slerp along the geodesic; a spring is an
  angular-velocity spring on the geodesic (exponential-map form), which is
  the retargeting-safe rotational primitive. Small, well-trodden math, but
  it is the part that did not ship with the element transitions and wants
  its own tests (retarget mid-flight, near-antipodal targets).
- **Clock and demand**: the same animation clock the element transitions
  stamp and advance in lattice - paused/scaled time behaves identically,
  record/playback stays deterministic - and the same demand story: active
  tracks request frames, settled tracks produce nothing.
- **JS face**: a transition declaration per node plus target writes -
  `setTransform(node, targets)` animating instead of snapping once a
  declaration is set, mirroring how element writes animate under a
  `transition` prop. `@solidrt/3d` forwards it on its nodes; citizenship
  gives `@solidrt/2d` the same for free.

## Ordering with other producers

Transitions, clips (animation-core) and physics bodies all write node TRS
before flush. Multiple producers on ONE node is an authoring error more
than a design case; decide the cheap rule early (last write wins, dev-mode
warning when two producers claim a node) and leave blending to
animation-core, whose crossfades are the real mixing story.

## Not in this item

Clip sampling, blending and state machines (animation-core); skinning;
physics; the element transitions' color/paint lanes (nodes have no paint);
layout-affecting anything. Frustum-driven pausing of off-screen
transitions is a non-goal until someone shows a scene where it matters.

## What done looks like

A bench scene (the spatial-core one) where a few hundred meshes retarget
springs once a second shows JS cost proportional to target CHANGES, not
frames - the signal-bench "transition" MODE result, reproduced one level
down. The 2d citizenship item can then state its motion story without the
regression caveat.

## Findings

The core stage landed 2026-08-24 (uncommitted). The shared math (Curve,
TransitionSpec + spring mapping, the closed-form spring_step) moved from
`rendertree/transitions.rs` to a new `alloy/src/motion.rs`; the rendertree
keeps its lane tracks and re-exports the spec types, so nothing above it
changed. `alloy/src/spatial/transitions.rs` holds the node tracks:
position/scale are 3-lane tracks on the shared oscillator, rotation is the
quaternion track - tween slerps the geodesic (target pre-flipped to the
near hemisphere, so a 181-degree write animates backward through -179),
spring is an angular-velocity spring in the exponential map at the target
(state = orientation + rotation-vector velocity; retargets move the
equilibrium and keep the velocity, so momentum carries past the retarget
point exactly like the linear spring).

Spatial API: `set_node_transition(id, config|None)` (per-component specs +
`all`; clearing cancels in place, later writes snap),
`write_transform(id, p, q, s)` (the target-write sibling of
`set_transform`; undeclared components snap, components matching a running
track's target are left alone - the full-TRS write shape re-sends
unchanged components on every call and re-anchoring a tween on them would
restart it), `set_transition_now(ms)` / `advance_transitions()` /
`take_settled_transitions()` mirroring the tree's embedder surface.
Advance writes node TRS and queues; flush is untouched. Raw
`set_transform` never consults or cancels tracks (last write wins, the
producer rule). delay/from/exit/stagger deliberately absent - element
lifecycle conveniences, not node vocabulary.

Verified by 12 tests in `alloy/src/tests/spatial_transitions.rs` (exact
settles, no-restart on unchanged components, geodesic midpoints,
antipodal short-arc, retarget momentum, cancel/destroy/paused-clock/
hidden-node behavior); alloy 285/285, flux gui builds.

Stage 2 (the JS face and frame loop) landed 2026-08-24 (uncommitted):
flux:spatial gained `setTransition(node, config|null)` (components
position/rotation/scale plus `all`; values decoded by the shared
vocabulary - `decode_spec` in properties/transition.rs factors the
duration/curve/bounce core out of the element entry decode and rejects
delay/from/exit) and `writeTransform(node, transform)` (the target-write
sibling of setTransform; requests a frame when a track starts or a snap
moves the node - without that a fresh track would stall, since nothing
else demands the first frame). Settles emit one "spatialTransitionEnd"
engine event each (srt:events), payload `{ node, component }` - the same
bus the element "transitionEnd" rides, no Persistent handler storage.
Lattice: runtime.rs stamps the spatial clock beside the tree's
(`flux::gui::spatial::stamp_clock`), and the draw path runs
`flux::gui::spatial::tick` beside the element advance - advance, flush
when anything was written, emit settles; `wrote` joins the frame's
demand-gate bypass (sink writes must paint) and `active` keeps
requesting frames. flux-types gui/spatial.d.ts documents the surface.

Verified live (release go client, rebuilt - the engine is embedded):
probe with NO onFrame loop, one writeTransform (position spring 300ms +
rotation tween 200ms linear): mid-flight sample moved, both settles
fired, landing exact (x === 100, rotation matrix exact to 1e-5), clean
teardown. Two follow-ups went back to the backlog as their own items:
the startup clock anchor
([transition-clock-startup-anchor](../backlog/transition-clock-startup-anchor.md) -
a write before the first frame stamp starts its track at clock 0 and
fast-forwards the startup latency; shared with element transitions) and
settle routing to package handles
([spatial-settled-event-routing](../backlog/spatial-settled-event-routing.md) -
the engine event carries the core NodeId, packages should route to
sprite/SceneNode onTransitionEnd).

Stage 3 (adoption + bench) landed 2026-08-24 (uncommitted). @solidrt/2d:
`setSpriteTransition` / `setGroupTransition` (and a `transition` prop on
`<Sprite>`/`<Group>`); the layer's pose writes go through
`writeTransform`, so setSprite writes become targets under a
declaration - the JS pose mirror is then the TARGET mirror, which is
exactly what partial writes should compose from (documented as a trap:
getSprite reads targets, picking reads the actual mid-flight pose).
Record sprites throw (no node). The component declares the transition
AFTER the first pose sync so mount poses snap. @solidrt/3d:
`setTransition(node, ...)` stores the declaration on the SceneNode and
re-applies it at every scene enter (enter pose snaps); `pushTransform`
goes through `writeTransform`.

The done-looks-like bench is examples/springs.tsx (kept): 400 sprites
with position springs + rotation tweens, re-dealt to shuffled grid slots
every 1.2 s, NO onFrame. Measured on the release client: a retarget
burst of 400 setSprite calls is ~1.4-4.6 ms, once per shuffle - vs ~5 ms
per FRAME (~300 ms/s of JS) moving the same population imperatively. JS
cost is proportional to target changes, confirmed live: two control-API
snapshots 300 ms apart between bursts differ while JS idles. The 2d
citizenship item's motion story no longer carries the regression caveat
(packages/2d/AGENTS.md and packages/3d/AGENTS.md updated).

Producer-ordering rule as decided above: last write wins, no dev
warning until the dev/prod signal exists (the validation-policy item).
