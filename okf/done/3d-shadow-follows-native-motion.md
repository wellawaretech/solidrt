---
title: Node transitions advance before the frame's JS, so a casting light's shadow follows native motion
description: A castShadow light moved by a node transition lights the scene from its animated pose while its shadow map stays where the last JS write left it, and any JS reader of a transitioned node's pose (a late-pass follow, worldPosition, a 2d pin) is one frame stale, because node transitions are the one native mover that advances after the frame's JS.
created: 2026-10-10
---

# Node transitions advance before the frame's JS, so a casting light's shadow follows native motion

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

The same staleness, one frame instead of forever, hits every JS reader
of a transitioned node: a camera follow or a label pin in the late pass
of `onBeforeRender`, `worldPosition` in packages/3d (its doc promises
"exact before the pending sync has run", which holds for JS writes and
not for a running track), `worldPosition` in packages/2d. A node a clip
player or root motion moves has none of this.

## Cause

Native motion advances in two places (flux/src/alloy_plugins/frame.rs):

- Clip players and root motion in `frame::advance`, before the frame's
  JS, so `onFrame` reads and may overwrite the fresh pose, the late pass
  sees the final one, and the publish pass hands derived state to the
  engine.
- Node transitions in `frame::draw` (`spatial::tick`), after the frame's
  JS, including the publish pass of `onBeforeRender`
  (okf/done/before-render-phase.md). The publish pass runs a second time
  after the post-layout handlers, which is after the transition advance,
  but only on a frame that rebuilds and has `onLayout` handlers. So no JS
  entry reliably sees a transition-moved pose before the paint; the only
  JS out of `spatial::tick` is `spatialTransitionEnd`, on settles.

The light's `uLightPos`/`uLightDir` are core-driven slot sinks
(`bindPositionSlot`/`bindDirectionSlot` in `writeLights`,
packages/3d/src/scene.ts), so the lighting follows the world matrix on
every flush. The shadow cameras are placed in JS: `placeShadowCamera`
(packages/3d/src/scene-shadows.ts) reads the light's world matrix through
a core read (which resolves through dirty locals, so it would see the
stepped pose once the step has run), but it only runs from
`shadowSys.placeCameras` in the scene's sync, and the sync only runs
after a JS write. Every caster goes through it: spot, box, the six point
faces, and cascades.

The plain `onBeforeRender` pass is documented as Unity's LateUpdate ("a
follow or a fit reads the finished state of the frame",
packages/core/src/window.ts). That is true for clip players and false
for transitions. Shadows are the one reader inside the scene itself.

The scene camera is not affected: it is target state, not a tree node.

## Done looks like

- One rule for all native motion: it advances before the frame's JS, the
  frame's JS may read and overwrite it, the late pass sees the final
  pose, the publish pass hands derived state to the engine. Documented
  once, in the frame protocol and in the late-pass doc.
- A casting light moved only by native motion has its shadow follow in
  the same frame its lighting does, for every caster type and every
  native mover.
- A JS read of a transitioned node's pose in `onFrame` or the late pass
  returns the pose of this frame, in 3d and 2d.
- Tests: flux module tests for the ordering (an `onFrame` read sees the
  stepped pose; a node created with `from` paints `from` first; a raw
  write in the frame wins its frame; a settle fires before the frame's
  JS). A headless test in packages/3d/tests/ steps a transitioned caster
  frame by frame (paused clock) and reads the shadow following mid-flight
  (`uShadowMatrix` on a receiver, or the painted shadow). A 2d test reads
  `worldPosition` mid-flight.
- packages/3d/AGENTS.md "Retargeted motion" states the rule and that
  "one JS write per target change, zero per frame" holds for meshes,
  lighting and shadows alike; the Shadows section says the same. The
  `worldPosition` docs in 3d and 2d say the read is exact against native
  motion. The alloy `write_transform`/`set_transform` doc comments carry
  the new producer rule.

## What it involves

Move the transition step to `frame::advance` beside the clip players,
start what the frame's callbacks declared after them (the frame's start
pass, core's call before the late pass: Unity's animation update
between Update and LateUpdate), and keep the free gates and the flush in
the draw tick. Three things depend on the advance running after the
frame's JS today, and the split keeps each where it has to be:

- Enter transitions. `start_enter_transitions` runs first in the
  advance so a node created in this frame's JS has its first flushed
  transform at `from` (alloy/src/spatial/mod.rs). Stepped before the
  frame's JS, a created node would paint one frame at its created pose,
  then snap to `from`. So the enter starts run after the frame's
  callbacks: in the start pass core calls before the late pass, and
  again in the draw tick for nodes created outside a frame.
- Exits and frees. Staggered exits start with the enters, for nodes let
  go of in the frame; `spatialNodeFreed` relies on the zeroing writes
  landing in the flush before the event, and no node may vanish under
  the JS still running in the frame, so the free gates (`check_exits`),
  the unconditional flush and the freed events stay in the draw tick, in
  that order. The start pass frees nothing.
- The producer rule. A raw `set_transform` on a transitioning node is
  documented as overwritten by the track at the next advance. Stepped
  before the frame's JS, the frame's write wins the frame, which is the
  clip players' rule (`onFrame` overwrites the fresh pose). That is the
  rule to keep: one for all native motion. No compatibility concern.

The pieces:

1. **alloy spatial.** Split `advance_transitions` into the step (due
   writes, track steps, settles, exit checks) and the starts (enter
   starts, staggered exit starts). The step returns `active`; the
   settled list is taken after the step. Doc comments on
   `write_transform`/`set_transform` for the producer rule.
2. **flux frame protocol.** `frame::advance` stamps the clock (already),
   advances the players, then steps the transitions; what the motion
   has to tell JS waits for `deliver`, which emits a "frameStart" event
   (the frame's clock, core stamps `frameTime()` from it), then the
   motion events (`spatialClipEnd`, `spatialRootMotion`,
   `spatialTransitionEnd`: handlers see the frame's poses and its
   clock), then rAF and "render". The advance's running motion is
   carried to `draw` for the standing re-request past the gate, the way
   `ticking` is stored on the gui state, and the advance seeds the
   frame's demand reasons (`draw` appends its own). `flux:spatial`
   exports `startTransitions`, the start pass. The draw tick runs the
   starts again, the free gates, the unconditional flush and the freed
   events, and reports `wrote` as now. The render tree's own transitions
   stay in `draw`: nothing in JS derives state from them.
3. **core window.ts.** Stamps the tick on "frameStart", calls
   `startTransitions` after the frame callbacks and their flush, before
   the late pass; the late-pass doc says it sees every native mover.
4. **packages/3d.** The scene's publish handler runs the sync when a
   write scheduled it and a `follow` pass otherwise (`placeCameras`, the
   views' pending cameras, `flushMatrices`); the existing `lastWorld`
   compare makes that one 16-float read per caster per frame and zero
   writes when nothing moved. No watch list and no moved-node report from the core: a reader
   that wants a fresh pose reads it in the late pass, the Three and
   Unity model. AGENTS.md and the `worldPosition` doc.
5. **packages/2d.** Doc only; the fix comes with the frame protocol.

While in `frame::advance`: `players.active` only requests a frame there,
and the gate consumes that request; nothing in `draw` re-requests for a
playing clip the way it does for `spatial.active`. Confirm what keeps a
playing clip's loop alive; the carried flag above may fix a latent gap.

Rejected: a JS entry out of `spatial::tick` (an event when the flush
wrote, or a third publish pass after the tick) that re-places the
shadows. It fixes the symptom and leaves every app-side reader one frame
stale, adds a JS entry to the draw, and keeps the late pass running
before native motion.

Deferred: core-driven shadow cameras, a sink that turns a node's world
pose and a constant projection into the tile target's camera params, its
cull view and its `uShadowMatrix` slot on every receiving target. Zero
JS per frame for spot, box and point; cascades do not fit (their fit
follows the scene camera and snaps to the texel grid). An optimization
if a profile ever asks for it, additive over the above.

Related: [native-motion-time-scale] (both come from native motion
running outside the app's frame JS; independent fixes, and the shadow
follows actual movement, so a paused or slowed node is respected for
free).

[native-motion-time-scale]: ../done/native-motion-time-scale.md

## Findings

- A playing clip kept its loop alive by accident: `advance_players`
  requested the frame in `frame::advance`, the draw gate consumed that
  request, and nothing in `draw` re-requested for a player the way it
  did for a spatial transition's `active`. `Gui.motion_active` now
  carries the advance's active motion (players and transitions) to the
  draw's standing re-request.
- First cut emitted the motion events from `frame::advance`, where
  core's `frameTime()` was still the previous frame's tick (core stamped
  it from the "render" event). Hence the "frameStart" event: the clock
  reaches JS first, then the motion's reports, then the callbacks.
- First cut started the enters in the draw tick, after the whole frame's
  JS, so the late pass of a node's creation frame read a pose the frame
  never drew. The start pass before the late pass is Unity's order
  (animation update between Update and LateUpdate); the free gates stay
  in the draw tick so nothing is freed under running JS.
- The scene's unscheduled pass costs, at rest, flag checks plus one
  `worldMatrix` read per caster per frame; the `lastWorld` compare keeps
  the writes at zero when nothing moved.
- The alloy unit tests drive time through a `tick` in the protocol's
  order (step, start, gates at one stamp); a test acts as a frame's JS
  and ticks at that stamp before moving time on. The halves are pinned
  by three `split_*` tests.
- The test host's mount frame stamps no animation clock (the startup
  anchor applies), so a track written right after `mount` starts at the
  first frame that runs; now in the `mount` doc.
