---
title: Animation layers - a rig in the core with layers, masks, additive deltas, inertial blends, sync groups, windows, markers and curves
description: The clip players grow into a rig per model (Unity's Animator, Godot's AnimationMixer, Unreal's AnimInstance, ozz's pose buffer) that composes ordered override and additive layers over the rest pose with shared bone masks, bakes additive deltas once, blends transitions by crossfade or inertialization, syncs cycles in groups, enforces subclip windows and fires markers, windows and curves, all evaluated in the spatial core so "aim while running", a flinch over a walk, a walk-to-run blend space, a trimmed clip and a footstep on frame 12 cost zero per-frame JS.
created: 2026-10-20
completed: 2026-10-20
---

# Animation layers

## Problem

The mixer plays, stops and crossfades: one flat weighted average over
every player on a node. That is the whole of `players.rs`'s blend
contract, and it cannot express what every character controller reaches
for next. "Aim while running" needs the upper body from one clip and the
legs from another (a bone mask); a flinch, a breath or a recoil over
whatever is playing needs an additive delta (Unity's additive layer,
Godot's `AnimationNodeAdd2`, Three's `AdditiveAnimationBlendMode`); a
footstep sound or a hit window needs a time marker fired by the player
(Unity's `AnimationEvent`, Unreal's notifies and notify states, Godot's
method tracks and markers); a walk-to-run blend needs the two cycles in
phase or the feet slide (Unity's blend trees warp time, Unreal's sync
groups, Three's `crossFadeTo(warp)`); a subclip (seconds 2 to 3.5 of a
long take) is a player window the core enforces.

None of these can be done outside the core, because the core samples
and blends and JS never sees the pose: a JS mask would re-read and
re-write every joint per frame (the spring-bone cost), a JS additive
would have to sample clips in JS (the cost the players removed), a JS
sync would set every player's time per frame, and a JS marker is a
per-frame poll of the player time.

The flat average also has two defects of its own. It has no notion of
"on top of": two full-weight players on the same joints average to a
half-and-half pose, where an engine composites the higher layer over
the lower. And it has no floor: `stop({ fadeMs })` does nothing visible
while the last player normalizes to full weight, then pops when it
drops, because nothing says what a pose fades TO. Three's mixer has
the same shape and the same defects; Godot, Unreal and ozz compose over
the rest pose.

## Done looks like

- **A rig per mixer in the core** (Unity's Animator, Godot's
  AnimationMixer, Unreal's AnimInstance, ozz's local pose buffer):
  `createRig(nodes, clock, rest)` owns the joint table (a node per slot),
  the rest TRS per joint, a dense pose buffer (slot x path, plus the
  weights register and a curves register), the layers, the players and
  the sync groups. Clips bind to a rig once (channel to slot indices
  resolved at bind, no hashing per frame). `createMixer(model)` makes
  the rig over the model's nodes with the glTF rest pose; `createMixer({
  root, nodes })` makes one over any scene nodes, so clips authored in
  code (a `ModelClip` built from channels) drive any node's TRS, the
  "author keyframe tracks in code" parity row for the arena's state.
- **Layers**, Unity's model, which Godot's `AnimationTree` blend nodes
  with filters and Unreal's layered blend per bone reduce to and Three's
  actions approximate: `mixer.layer(name, { weight?, mode?: "override"
  | "additive", mask? })`; the base layer exists without declaration.
  Layers are ordered. The base layer's players blend over the REST pose
  with threshold normalization (below); each higher `override` layer
  normalizes its own players the same way and lerps the result over the
  pose so far by its weight times the mask's per-joint share; each
  `additive` layer adds its weighted deltas. `play(name, { layer? })`
  plays on a layer (base when absent), `stop({ layer? })`,
  `setLayerWeight(name, w, { fadeMs })` ramped in core. A layer at
  weight 0 costs no sampling.
- **Bone masks** (Unity's AvatarMask as a transform mask, Godot's
  filters, Unreal's blend masks and blend profiles) are shared core
  objects: `createMask({ include?, exclude?, soft? })` where an entry is
  a joint name or node with `subtree: true` (the spine-up mask is one
  entry) and `soft` names per-joint shares under 1 (a 0.5 on the
  shoulder so the mask does not crease, Unreal's per-bone blend weight).
  JS expands subtrees over `model.nodes` once; the core holds `{
  default, shares: node -> float }` and caches the per-channel shares on
  each binding, refreshed when the mask's version bumps. A layer
  references a mask; a player may reference one too (`play(name, {
  mask })` for the one-off case); the effective share is the product.
  Swapping a layer's mask is one write.
- **Additive clips** (Three's `makeClipAdditive`, Unity's additive
  reference pose, Godot's `AnimationNodeSub2` feeding an `Add2`): a clip
  played on an additive layer is turned into deltas against a reference
  once, in core, cached per (clip, reference) like the root-motion
  variants: the clip's own first frame by default (Three's
  `referenceFrame: 0`, Unity's default), a named clip at a time
  (`additiveReference: { clip, time }`), or the rig's rest pose
  (Godot's). Position deltas add, rotation deltas multiply (slerped
  from identity by the weight), scale deltas multiply, weights and
  curves add; the delta applies after every override layer below it, so
  a flinch rides whatever pose the base and the masks produced.
- **Transitions blend two ways**: `play(name, { fadeMs, blend?:
  "crossfade" | "inertial" })`. Crossfade is today's: the new player
  fades in, the others on its layer fade out, both sampled during the
  fade. Inertial is Unreal's inertialization: the new player plays at
  full weight at once and the rig keeps, per slot, the offset from the
  pose it last wrote (with the velocity from the two previous outputs)
  and decays it over `fadeMs` on a quintic that lands at zero with zero
  velocity, its duration capped so the curve cannot overshoot. One
  sample per frame instead of two, no averaging between two cycles, an
  interrupted transition re-captures from wherever the pose is, and any
  discontinuity is smoothed whatever its source (a layer switched on, a
  stop, a seek, a window jump). Godot's `play_with_capture` is the
  half-step and is subsumed.
- **Sync groups** (Unreal's sync groups, Unity's blend-tree time warp,
  Godot's `sync`): `mixer.group(name, [clips])` makes the named players
  share one phase: the heaviest member leads, the others play at the
  leader's normalized time (Unreal's rule), a member's speed scales the
  group, markers and windows fire at each member's own clip time. With
  `sync: "markers"` and markers common to every member, the phase is
  matched between markers (Unreal's marker-based sync), so a walk whose
  foot plants sit at other fractions than the run's still lands them
  together. The package gets `createBlendSpace(mixer, { clips, axes })`
  (1D and 2D, Godot's triangulated blend space) as the consumer: a
  speed-and-direction change is O(weights) writes to one group.
- **Subclips**: `play(name, { start?, end? })` in seconds (Three's
  `subclip`, Godot's `play_section`); the player's time stays within
  the window, loops within it, finishes at `end`, `seek` clamps to it,
  root motion under a window is the window's travel (the net drift rule
  re-evaluated over the window), markers outside it never fire.
- **One-shots end in core**: `play(name, { loop: false, fadeOutMs })`
  starts the fade `fadeOutMs` before the clip's end (Godot's OneShot
  `fadeout_time`, Unreal's montage blend-out), so a flinch on a layer
  returns to the base without a JS timer and without the one-frame pop
  of fading on `onFinish`.
- **Markers and windows** (Unity's AnimationEvent, Unreal's notifies and
  notify states, Godot's markers and method tracks):
  `mixer.markers(clipName, [{ time, name, duration?, data? }])` or
  markers carried on `ModelClip.markers` from the bake (glTF has none;
  the model tool reads them from the clip's `extras` so an exporter can
  author them). The core fires `spatialClipMarker` when a player's time
  crosses a marker, loop-wrap aware (a marker at 0 fires on every wrap),
  speed-sign aware (reverse play fires in reverse), once per crossing
  however long the frame, with the player's effective weight and layer
  in the payload so the app thresholds itself. A marker with `duration`
  is a window: `begin` and `end` phases, and `end` also fires when the
  player stops, drops or finishes inside the window (Unreal's NotifyEnd
  on a montage stop). `seek` never fires. `mixer.onMarker` delivers them
  in the frame's JS after the advance, like `onFinish`; `mixer.inside(
  clip, marker)` reads a window's state without an event. `onLoop`
  (Three's `loop` event) fires on the wrap itself.
- **Curves** (Unreal's animation curves, ozz's float tracks): named
  float tracks on a clip, `ModelClip.curves` from the clip's `extras`
  or set in code, sampled into the rig's curves register and blended
  like weights (override lerps, additive adds, masks do not apply).
  `mixer.curve(name)` reads a value per frame; an IK solver's weight
  can be bound to one in core (`setWeight(i, { curve })`), which is the
  foot-plant authoring the IK plan deferred. KHR_animation_pointer and
  properties outside the arena (materials, lights) stay out.
- **Time**: `mixer.time(name)` reads a player's time, `seek(name, t)`
  writes it, `mixer.setSpeed(name, s)` (the tiny.md item) rides along.
- **Root motion composes with layers**: a player's reported travel is
  weighted by its effective weight (player times layer times the root's
  mask share), so a legs-only base keeps the travel and an upper-body
  override contributes none; additive players never report travel
  (Unity ignores root motion on additive layers by design).
- AGENTS.md "Animation" grows a layered model section; the mixer's
  "Not yet" list shrinks to model-space blends and culling (below).

## Decisions

**A rig, not free-floating players.** Every feature here wants state at
the level of "this model's animation": the rest pose, the per-slot
output history the inertial blend decays from, the groups, the curves
register, the layers. Four half-objects hung on players would each
re-derive the joint table. The rig is what every engine has under its
own name, and ozz's layout (a dense local pose buffer the layers blend
into) is the fast one: slot indices resolved at bind, a Vec per path,
no per-frame hashing. The mixer maps 1:1 to it.

**Layers, not a blend graph.** Godot, Unreal and Bevy ship graphs;
Unity ships ordered layers; ozz, the runtime primitive beneath several
custom engines, ships ordered layers with per-joint weights and an
additive kind. A normalized blend of normalized blends is one
normalized blend of the leaves, an additive inside a blend is an
additive layer at the blend's weight, an override inside an override is
two ordered layers: the graph's nesting flattens into layers up to the
usual rotation approximations every tree engine also makes. What a
graph adds is an editor. The layer weight is the one knob a controller
drives, and a state machine or blend space is a weight policy in JS.

**The base layer composes over rest with threshold normalization.**
Godot's deterministic mode, ozz's blending job and Unreal all blend
toward the rest pose when the weights sum below one and treat a track a
clip lacks as the rest value; Three normalizes and writes sparsely,
which is where the stop-fade pop and the crossfade snap on a joint only
one clip animates come from. Rule: on a layer, sum at or above one
normalizes (crossfades are unchanged), below one the remainder is rest;
within a layer a player lacking a channel another player on that layer
animates contributes rest for it; a slot no player on any layer
animates is left alone, so a wave-only clip alone still holds the body.
Below the base is rest; below a higher layer is the pose so far, so a
layer fading out reveals what is under it. The rest pose is the rig's,
given once (the model's glTF node TRS). Springs and IK capture their
own rest today and may read the rig's later; not in scope.

**Inertialization is in scope because rest-fill alone snaps.** A fade
that starts from a held pose that is not rest snaps to the rest-mix on
its first frame, Unity's write-defaults trap exactly. The engine-grade
answer is not a third fill mode but a blend that starts from the pose
as it stands: the inertial transition captures the offset at the switch
and decays it, so the fill rule can be the clean one and the snap never
shows. It also replaces the double sampling and the cycle averaging of
a crossfade. Crossfade stays the default because every existing probe
and demo verifies it; AGENTS.md recommends inertial for locomotion.
Unreal's exact polynomial is not published; the one here matches
offset and velocity at zero and reaches zero offset, velocity and
acceleration at the duration, with the duration capped when the
velocity points away from zero so the curve cannot overshoot. The
history is two outputs per slot, kept on the rig's pose buffer, which
is why that buffer persists across frames instead of clearing.

**Sync groups now, not later.** The player's time model (window, speed,
wrap, marker crossings, root motion) is rewritten by this plan; a
player that takes its time either from its own clock or from a group's
phase is a day-one shape, and bolting a shared phase on later reopens
every one of those. Without it the blend space, the thing every
locomotion controller builds, foot-slides, the one artifact every
engine's sync feature exists to remove. Leader by weight is Unreal's
rule and needs no authoring; marker sync is the additive upgrade the
markers already in the core clip make cheap.

**Masks are shared objects keyed by node.** A mask is per joint, not per
channel; keying it on the node lets one mask serve every clip of a rig
and every rig over a shared skeleton. Per-channel shares are a cache on
the binding, so the blend reads a float, never a map. A float per joint
makes the soft edge a value, not a mechanism.

**Additive deltas are baked once in core.** Three bakes in JS at load,
Unity at import, Godot subtracts per frame (`Sub2`). Baking once per
(clip, reference) is the root-motion variant mechanism, costs nothing
per frame and keeps the authored clip intact for override use. Delta
algebra: position add; rotation `delta = inverse(ref) * pose` applied
as `pose = base * delta` in the joint's local frame, slerped from
identity by the weight; scale multiply; weights and curves add.

**Markers fire regardless of weight.** Unity fires events on any
playing clip; Godot's method tracks fire while the animation runs. The
payload's effective weight lets the app ignore a footstep at 0.01; a
threshold in the core would be a magic number the app could not tune per
marker. Per crossing, not per frame: a long frame that passes two
markers fires both in time order; a wrap fires those between the old
time and the end, then those from the start to the new time; the core
walks the sorted list from the last fired index.

**Subclips are player windows, not derived clips.** Three numbers on
the player; a derived clip per window would multiply registered data
for a scrub.

**Model-space blends are deferred, additive.** Unreal's mesh-space
additive and mesh-space rotation blend (its aim-offset recipe) keep an
aim's absolute orientation under a spine the base bends differently;
Unity, Godot and Three blend in local space only. It needs ancestor
composition inside the blend, a different mechanism, and lands as
`space: "model"` on a layer without touching the rest.

**Also deferred, each additive**: culling of off-screen rigs (one
policy for players, IK and springs together, per the IK plan),
humanoid naming and mirroring (presuppose a humanoid layer the package
does not have), property animation outside the arena.

**No backwards compatibility.** The mixer's flat average becomes the
base layer's rule over rest; `play` and `stop` keep their names and
grow options; `flux:spatial`'s player calls take a rig. Nothing is kept
for the old shape's sake.

## Work

1. Core, `players.rs` (split into `rig.rs` for the rig, layers, masks
   and the blend, `players.rs` for clips, sampling, windows, markers,
   groups and root motion if it reads better): the rig with its rest
   pose and persistent pose buffer; layers (ordered, weight with fade,
   mode, mask); masks (shared, versioned, per-binding share cache); the
   layered blend with threshold normalization over rest and the
   union-of-channels fill; additive delta baking per (clip, reference);
   windows; one-shot end fades; markers and windows with the crossing
   walk, `ClipEvent::Marker { phase }` and `ClipEvent::Loop`; curves
   register; sync groups (leader by weight, normalized time, marker
   phase); the inertial transition over the slot history; root motion
   through layers and masks. Tests in `alloy/src/tests/spatial_rig.rs`
   and `spatial_players.rs`: an override layer at weight 1 replaces
   exactly on masked joints and nothing elsewhere; a soft share lerps; a
   base player fading out alone fades to rest; a joint one clip lacks
   blends from rest during a crossfade; an additive layer over a still
   base reproduces the authored clip minus its reference; an inertial
   switch is continuous in position and velocity and ends on the new
   clip; a group's followers sit at the leader's phase across a wrap and
   a leader change, and on markers when asked; marker crossings over
   wraps and reverse play, a window's end on a stop mid-window; a
   subclip loops and finishes at its end; root travel through a legs
   mask.
2. Plugin: `flux:spatial` `createRig/destroyRig`, `createLayer/setLayer/
   destroyLayer`, `createMask/setMask/destroyMask`, `createPlayer` over
   a rig with layer, mask, window, blend and end-fade options,
   `setPlayerWindow`, `setClipMarkers`, `setClipCurves`, `createGroup/
   setGroup/destroyGroup`, `readCurve`, the `spatialClipMarker` and
   `spatialClipLoop` events; the IK weight-from-curve binding;
   `packages/flux-types/gui/spatial.d.ts`.
3. Package: `mixer.ts` over the rig (`createMixer(model | { root,
   nodes })`, layers, masks with subtree expansion, `play` options,
   `group`, `markers`, `curves`, `onMarker`, `onLoop`, `inside`, `time`,
   `seek`, `setSpeed`), `blend-space.ts`; `gltf.ts`/`model-file.ts`
   carry `ModelClip.markers` and `ModelClip.curves` from the clip's
   `extras` (container version bump); `ik.ts` `setWeight` from a
   curve; AGENTS.md "Animation" rewritten around the layered model.
4. Probe `probes/animation-layers-probe.tsx`: a walk-run blend space in
   a sync group on the base, an aim on an upper-body override layer, a
   flinch one-shot on an additive layer, an inertial switch to a crouch,
   a subclip of the walk, markers on its foot plants, a curve driving
   an IK weight, all read through the control API with frame stepping.
5. Example `packages/3d/examples/animation-layers.tsx`; the parity rows
   for additive blending, masks, subclips, events and code-authored
   tracks re-linked and recounted; the mixer speed line leaves tiny.md.

## Findings

Cut to [3d-animation-layers-findings](../notes/3d-animation-layers-findings.md).
