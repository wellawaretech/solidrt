---
title: Animation layers - masks, additive blending, subclips and markers in the core players
description: The clip players grow Unity's layer model (ordered layers with a weight, an override or additive mode and a bone mask; Godot's Blend2/Add2 with filters, Three's additive actions), subclip windows and time markers fired as events, all evaluated in the spatial core so "aim while running", a flinch over a walk, a trimmed clip and a footstep on frame 12 cost zero per-frame JS.
created: 2026-10-20
---

# Animation layers

## Problem

The mixer plays, stops and crossfades: one flat weighted average over
every player on a node. That is the whole of `players.rs`'s blend
contract, and it cannot express the three things every character
controller reaches for next. "Aim while running" needs the upper body
from one clip and the legs from another (a bone mask); a flinch, a
breath or a recoil over whatever is playing needs an additive delta
(Unity's additive layer, Godot's `AnimationNodeAdd2`, Three's
`AdditiveAnimationBlendMode`); a footstep sound or a hit frame needs a
time marker fired by the player (Unity's `AnimationEvent`, Godot's method
tracks and markers). A subclip (play seconds 2 to 3.5 of a long take) is
the same family: a player window the core enforces.

None of these can be done outside the core, because the core samples
and blends and JS never sees the pose: a JS mask would re-read and
re-write every joint per frame (the spring-bone cost), a JS additive
would have to sample clips in JS (the cost the players removed), and a
JS marker is a per-frame poll of the player time. The one shape
available today, a weighted average, also has no notion of "on top of":
two full-weight players on the same joints average to a half-and-half
pose, where an engine composites the higher layer over the lower.

## Done looks like

- **Layers on the mixer**, Unity's model, which Godot's `AnimationTree`
  blend nodes with filters reduce to and Three's actions approximate:
  `createMixer(model, { layers? })` or `mixer.layer(name, { weight?,
  mode?: "override" | "additive", mask? })`; the base layer exists
  without declaration. Layers are ordered; the base layer's players
  normalize among themselves as today, each higher `override` layer
  lerps its own blended pose over the result so far by its weight times
  the mask's per-joint share, each `additive` layer adds its weighted
  deltas. `play(name, { layer? })` plays on a layer (base when absent),
  `stop({ layer? })`, `setLayerWeight(name, w, { fadeMs })` ramped in
  core. A layer whose weight is 0 costs no sampling.
- **Bone masks** (Unity's AvatarMask as a transform mask, Godot's
  filters, Unreal's blend profiles): `mask: { include?: NodeSpec[],
  exclude?: NodeSpec[] }` where a NodeSpec is a joint name or node with
  `subtree: true` to take its descendants (the spine-up mask is one
  entry). Resolved to per-target shares in JS once (O(changes)); the
  core carries a share per target slot as a float, so a soft edge
  (Unreal's per-bone blend weight, a 0.5 on the shoulder so the mask
  does not crease) is a value, not a new mechanism. Masks are per layer;
  a per-player mask is the same share table on one player and is
  exposed as `play(name, { mask })` for the one-off case.
- **Additive clips** (Three's `makeClipAdditive`, Unity's additive
  reference pose, Godot's `AnimationNodeSub2` feeding an `Add2`): a clip
  played on an additive layer is turned into deltas against a reference
  pose once, in core, cached per (clip, reference) like the root-motion
  variants: the reference is the clip's own first frame by default
  (Three's `referenceFrame: 0`, Unity's default), or a named clip at a
  time (`additiveReference: { clip, time }`), or the model's rest pose.
  Position deltas add, rotation deltas multiply (slerped from identity
  by the weight), scale deltas multiply, morph weights add; the delta is
  applied after every override layer below it, so an additive flinch
  rides whatever pose the base and the masks produced.
- **Subclips**: `play(name, { start?, end? })` in seconds (Three's
  `subclip`, Godot's `play_section`); the player's time stays within
  the window, loops within it, finishes at `end`, and `seek` clamps to
  it. Root motion under a window is the window's travel (the net drift
  rule re-evaluated over the window).
- **Markers** (Unity's AnimationEvent, Godot's markers and method
  tracks): `mixer.markers(clipName, [{ time, name, data? }])` or
  markers carried on `ModelClip.markers` from the bake (glTF has none;
  the model tool reads them from the clip's `extras` so an exporter
  can author them); the core fires `spatialClipMarker` when a player's
  time crosses a marker, loop-wrap aware (a marker at 0 fires on every
  wrap), speed-sign aware (reverse play fires in reverse), once per
  crossing however long the frame, with the player's current weight and
  layer in the payload so the app thresholds itself. `mixer.onMarker`
  delivers them in the frame's JS after the advance, like `onFinish`.
  Added beside it: `onLoop` (Three's `loop` event; Unity has none) for
  the clip-wrap itself.
- **Time**: `mixer.time(name)` reads a player's time, `seek(name, t)`
  writes it (the `time` field of `PlayerUpdate` already exists), so a
  scrub or a sync is one call.
- Root motion composes with layers: a player's reported travel is
  weighted by its effective weight (player times layer times the root's
  mask share), so a legs-only mask on the base layer keeps the travel
  and an upper-body override contributes none; additive players never
  report travel (a delta clip has no root travel by construction).
- AGENTS.md "Animation" grows layers, masks, additive, subclips and
  markers; the mixer's "Not yet" list shrinks to sync groups (below).

## Decisions

**Layers, not per-player masks alone.** A per-player mask would let two
players share a skeleton by region, but it leaves the composition as a
weighted average: a masked aim at weight 1 and a run at weight 1 still
average on the shared joints at the mask edge, and there is no way to
say "this one is on top". Unity's ordered layers with override and
additive modes are the model every engine's tree reduces to (Godot's
Blend2 with a filter IS an override layer; Add2 IS an additive layer),
and the layer weight is the one knob a controller actually drives. The
core evaluates layers bottom-up per (node, path): base average, then per
layer either lerp-over or add. The cost is one extra accumulator per
layer per written slot, native.

**Additive deltas are baked once in core.** Three bakes in JS at load
(`makeClipAdditive`); Unity bakes at import; Godot subtracts at runtime
per frame (`Sub2`). Baking once per (clip, reference) in the core is the
root-motion variant mechanism already in `players.rs` (a second core
clip derived on first use), costs nothing per frame and keeps the
authored clip intact for override use. The reference choice follows
Three's three forms exactly.

**Delta algebra.** Position: add. Rotation: `delta = inverse(ref) *
pose` applied as `pose = base * delta` in the joint's local frame,
slerped from identity by the weight (Three's additive action does the
same; Unity's additive layers likewise). Scale: multiply (a delta of 1
is no change). Morph weights: add. The weight is the player's times the
layer's times the mask share, so an additive layer under a mask affects
only the masked joints.

**Markers fire regardless of weight.** Unity fires events on any
playing clip; Godot's method tracks fire while the animation runs. A
faded-out clip that still plays is still a clip the app started, and
the payload's weight lets the app ignore a footstep at weight 0.01. The
alternative (a threshold in the core) is a magic number the app could
not tune per marker.

**Markers are per crossing, not per frame.** A long frame that passes
two markers fires both, in time order; a loop wrap fires the markers
between the old time and the end, then those from the start to the new
time. The core walks the sorted marker list from the last fired index,
O(markers crossed), with the same cursor discipline the channel sampler
uses.

**Subclips are player windows, not derived clips.** A window is three
numbers on the player (start, end, and the time inside); a derived clip
per window would multiply registered data for a scrub. Loop wraps
inside the window are the same wrap logic over `end - start`; markers
outside the window never fire.

**Sync groups are additive, later.** Unity's sync layers and Unreal's
sync groups (a walk and a run that share a normalized phase so a blend
between them does not foot-slide) are the next layer up and need a
normalized-time blend between two players, which the layer model here
leaves room for (a layer's players already share an advance). Filed as
a follow-up when this lands; nothing here closes it off.

**Blend spaces are not in this plan.** A 1D or 2D blend space (speed
times direction over four clips) is a weight policy over the players a
layer already has; it is JS at O(changes) today (write four weights when
the speed signal changes) and a library helper, not core work.

**No backwards compatibility.** The mixer's flat average becomes the
base layer's rule; `play` and `stop` keep their names and grow options.
Nothing is kept for the old shape's sake.

## Work

1. Core: `players.rs` - layers (ordered, weight with fade, mode, per-slot
   mask shares), the layered blend in `advance_players`, additive delta
   baking against a reference (clip first frame, clip at time, rest
   pose) cached per (clip, reference), player windows, markers per clip
   with the crossing walk and `ClipEvent::Marker`, `ClipEvent::Loop`;
   root-motion weighting through layer and mask; tests in
   `alloy/src/tests/spatial_players.rs`: an override layer at weight 1
   replaces exactly on masked joints and nothing elsewhere, a soft share
   lerps, an additive layer over a still base reproduces the authored
   clip minus its reference, marker crossings over wraps and reverse
   play, a window loops and finishes at its end.
2. Plugin: `flux:spatial` `createLayer/setLayer/destroyLayer`,
   `setPlayerMask`, `setPlayerWindow`, `setClipMarkers`, the
   `spatialClipMarker` and `spatialClipLoop` events;
   `packages/flux-types/gui/spatial.d.ts`.
3. Package: `mixer.ts` - layers, mask resolution from names and subtrees
   to slot shares, `play` options (`layer`, `mask`, `start`, `end`,
   `additiveReference`), `markers`, `onMarker`, `onLoop`, `time`,
   `seek`; `gltf.ts`/`model-file.ts` carry `ModelClip.markers` from the
   clip's `extras` (container version bump); AGENTS.md "Animation".
4. Probe `probes/animation-layers-probe.tsx`: a walk on the base, an
   aim on an upper-body override layer, a flinch on an additive layer, a
   subclip of the walk, markers on its foot plants read through the
   control API with frame stepping.
5. Example `packages/3d/examples/animation-layers.tsx`; the parity rows
   for additive blending and subclips re-linked and recounted.

## Findings

