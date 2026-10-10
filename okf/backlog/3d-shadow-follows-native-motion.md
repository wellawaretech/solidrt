---
title: Re-place a casting light's shadow when native motion moves it
description: A castShadow light moved by the spatial core (a node transition, a clip player, root motion) lights the scene from its animated pose while its shadow map stays where the last JS write left it, because the shadow cameras are placed in the scene's JS sync, which native motion never schedules.
created: 2026-10-10
---

# Re-place a casting light's shadow when native motion moves it

## Symptom

A spot light that casts, on a group whose turn is a declared transition:

```tsx
<Group
  rotation={[0, right() ? 0.75 : -0.95, 0]}
  transition={{ rotation: { duration: 5900, curve: "ease-in-out" } }}
>
  <SpotLight position={[0, 2.4, 2.4]} direction={[0, -2.4, -2.4]} castShadow />
</Group>
```

Flip `right()`: the light swings and the lit side sweeps with it, but the
shadow on the floor stays put. It catches up only when something writes
the light or an ancestor from JS again.

The only workaround is to take the motion away from the core: write the
group's transform from `onFrame` every frame it moves, so each write
schedules the scene's sync.

## Cause

The light's `uLightPos`/`uLightDir` are core-driven slot sinks
(`bindPositionSlot`/`bindDirectionSlot` in `writeLights`,
packages/3d/src/scene.ts), so the lighting follows the world matrix on
every flush. The shadow cameras are placed in JS: `placeShadowCamera`
(packages/3d/src/scene-shadows.ts) reads the light's world matrix through
a core read, so the animated pose would be there, but it only runs from
`shadowSys.placeCameras` in the scene's sync, and the sync only runs
after a JS write.

It is wider than one light type or one mover:

- Every caster goes through `placeShadowCamera`: spot, box, the six
  point faces, and cascades (a cascaded sun turned by a transition goes
  stale the same way).
- Every native mover: node transitions, and a light parented under a
  node that a clip player or root motion moves.

Where the frame's JS sits relative to the native advance matters for any
fix (flux/src/alloy_plugins/frame.rs):

- Clip players advance in `frame::advance`, before the frame's JS.
- Node transitions advance in `frame::draw` (`spatial::tick`), after the
  frame's JS, including the publish pass of `onBeforeRender`
  (okf/done/before-render-phase.md) the scene syncs in.
- The publish pass runs a second time after the post-layout handlers,
  which is after the transition advance, but only on a frame that
  rebuilds and has `onLayout` handlers. So no JS entry reliably sees a
  transition-moved pose before the paint today; the only one from
  `spatial::tick` is `spatialTransitionEnd`, on settles.

The scene camera is not affected: it is target state, not a tree node.

## Done looks like

- A casting light moved only by native motion has its shadow follow in
  the same frame its lighting does, for every caster type and every
  native mover.
- A headless test in packages/3d/tests/ steps a transitioned caster
  frame by frame (paused clock) and reads the shadow following mid-flight
  (`uShadowMatrix` on a receiver, or the painted shadow).
- packages/3d/AGENTS.md "Retargeted motion" says what the motion costs
  today ("one JS write per target change, zero per frame" holds for
  meshes and lighting, not for a caster's shadow); the Shadows section
  says the same once fixed.

## What it involves

Two shapes:

1. **Re-place in JS when the core moved a caster.** The scene re-runs
   `placeCameras` and `flushMatrices` for the casters whose world matrix
   moved, ahead of the paint. Needs (a) a JS entry after the native
   advance: move the transition advance into `frame::advance` beside the
   clip players, so the publish pass already sees it, or an event out of
   `spatial::tick` the way `spatialTransitionEnd` is; and (b) knowing
   which casters moved: compare each caster per frame (`placeShadowCamera`
   already compares against `lastWorld`; one core read per caster), or
   the core reports moved nodes it was asked to watch (the flush knows
   which subtrees it recomputed). Covers every caster type and every
   mover with the existing placement code. Costs a few matrix writes per
   frame, only while a caster moves.
2. **Core-driven shadow cameras.** A sink that turns a node's world pose
   and a constant projection into the tile target's camera params
   (`cameraParams`: uViewProj, uInvViewProj, uCamPos, uCamRight, uCamUp),
   its view (`setView`, which the tile culls and picks LOD by) and its
   slot of `uShadowMatrix` on every receiving target. Zero JS per frame
   for spot, box and point. Cascades do not fit: their fit follows the
   scene camera and snaps to the texel grid, so either all of that moves
   to Rust or cascaded lights stay stale.

Lean: shape 1. It is complete across casters and movers, small, and
"tell me this node's world moved" is useful on its own (a screen label
pinned to a mesh, a sound following a node). Shape 2 stays an
optimization if a profile ever asks for it.

Before moving the transition advance: check what depends on it running
after the frame's JS (a `writeTransform` in `onFrame` retargets at the
stamped time either way; the render tree's own transitions stay in
`draw`).

Related: [native-motion-time-scale] (both come from native motion
running outside the app's frame JS; independent fixes, and shape 1
respects a pause for free since it follows actual movement).

[native-motion-time-scale]: ../done/native-motion-time-scale.md
