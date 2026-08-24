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
