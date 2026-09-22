---
title: Camera controls
description: The one pipeline every SolidRT camera control runs - source, framing, lanes, constraints, push - and the decisions behind it, measured against Cinemachine (the reference for follow cameras in 2d and 3d), Godot's Camera2D and SpringArm3D, Three's controls, camera-controls and Babylon's ArcRotateCamera; which stage each control implements today, what is missing, and the open items. Read before touching createCamera2d, createOrbitCamera or createFirstPersonCamera.
created: 2026-09-22
---

# Camera controls

Camera questions have come back in every package: the 2d controller and
its conventions, the orbit damping and clamp item, glide and fit, the
first-person boost and reference frame, and now the fly demo's push
through a model. Each answered its own question against its own
references, and the answers started to diverge: the 2d control has a
follow with a dead zone, the 3d one has none; the 3d control has anchor
hooks, the 2d one anchors itself; both copy the same ease constants.
This document is the shared picture. It is measured against the
controls that do each thing best, not against what the other SolidRT
package happens to do.

The code this describes: `packages/2d/src/camera2d.ts` (the 2d control),
`packages/3d/src/orbit.ts` and `first-person.ts` (the 3d controls),
`packages/3d/src/motion.ts` (the shared ease), the `<Camera2d>`,
`<OrbitCamera>` and `<FirstPersonCamera>` components, and the bindings
presets in each package's `input.ts`. The decisions that predate this
document and still hold are in
[2d-camera-conventions](../notes/2d-camera-conventions.md) (pose, bounds,
easing, input units); they are restated here only where the pipeline
needs them.

## References

- **Cinemachine 3** (Unity) is the reference for follow cameras, 2d and
  3d alike: a virtual camera runs a fixed stage pipeline (position,
  rotation, noise, finalize with extensions such as the confiner and the
  deoccluder), the Position Composer frames a target with a dead zone, a
  soft zone, hard limits, lookahead and per-axis damping, Orbital Follow
  is an orbit whose centre follows a target, and Impulse is the shake.
  Godot's Phantom Camera reproduces it because Godot lacks it.
- **Godot Camera2D** is the minimal follow camera: drag margins (a dead
  zone), per-axis smoothing, limits with optional smoothing, an offset.
  **SpringArm3D** is the minimal occlusion constraint.
- **camera-controls** (yomotsu, over Three) is the reference for a model
  inspection camera: dolly to cursor, infinity dolly (the push), orbit
  point with a focal offset, fit to box and sphere, a boundary with
  friction, collider meshes.
- **Three's OrbitControls / MapControls / ArcballControls** and
  **Babylon's ArcRotateCamera** with its behaviors (auto-rotation,
  framing, bouncing) for defaults and naming; maps and whiteboards
  (Leaflet, MapLibre, tldraw) for the pan-zoom half of 2d.

The survey of what each ships, verified against current sources, is in
[camera-and-controls-extensions](../plans/camera-and-controls-extensions.md).

## Vocabulary

- **Pose.** The control's own state: 2d `{x, y, zoom, rotation}` at a
  pivot; orbit `{azimuth, elevation, distance, target}`; first-person
  `{position, yaw, pitch}`. `pose()` returns it, `set()` writes it. The
  pose never contains a lane.
- **Pivot** (2d) / **screen position** (Cinemachine). The viewport point,
  as a fraction of the viewport, the pose's world point is shown at. The
  2d control has it as an option; the orbit control's target is always
  at the view centre and the offset lane moves it.
- **Source.** Where a pose change comes from: an input nudge (a drag, a
  pinch, a wheel notch, a stick rate), a verb (`set`, `glideTo`, `fit`,
  `zoomBy`), or the **follow**, a world point the control chases.
- **Framing.** How the follow chases its point, in viewport fractions
  around the pivot: the **dead zone** (the point moves freely, the camera
  does not respond), the **hard limits** (the point is clamped inside at
  once, whatever the damping), the **soft zone** between the two (the
  camera eases the point back toward the dead edge, damped per axis) and
  **lookahead** (the point is projected ahead along its velocity,
  smoothed). Two sizes, `deadZone` and `hardLimits`, Cinemachine 3's
  names; the soft zone is the band they leave.
- **Lanes.** Additive offsets applied on top of the pose at push time,
  never written into it: the **offset** lane (a persistent screen-space
  offset: Babylon's `targetScreenOffset`, camera-controls' focal offset,
  Godot's `offset`, Phaser's `followOffset`) and the **shake** lane (a
  transient decaying noise: Cinemachine Impulse, Phaser `shake`). Both in
  view HEIGHTS on both axes, the pointer feed's unit, so a shake is
  round and an offset means the same on a phone and a wide window.
- **Constraints.** Applied after the lanes, every write: the range clamps
  and `clampPose` (3d), the contain bounds (2d), and **occlusion** (3d:
  pull the eye in when something stands between it and the target).
- **Push.** Writing the final camera to the driven target (`setCamera`).
  The final camera is pose plus lanes, constrained.
- **Dolly, zoom, push.** Dolly moves the eye toward the target (bounded by
  it), lens zoom changes the fov, push moves eye and target together
  (unbounded). The orbit control's `zoom` axis is a dolly; the push is the
  overflow past its floor.
- **Shot.** A camera producing a pose without pushing it, so several can
  be blended. Above this pipeline; see the open items.

## The pipeline

Every control runs the same stages, in this order, on every `update(dt)`;
a nudge or a verb enters at the source and runs the rest at once, so a
drag never waits for a frame.

1. **Source.** An input nudge or a verb writes the pose (a finger wins: it
   cancels a glide, and a follow yields to a drag until `follow` is
   called again). A follow chases its point through framing.
2. **Framing.** For a follow only. The point (with lookahead) is projected
   to viewport fractions relative to the pivot; the zones decide the
   correction; the correction is eased per axis and written to the pose.
   Non-follow sources skip this stage.
3. **Lanes.** The offset and shake lanes advance (a shake decays) and are
   summed. They read the pose; they do not write it.
4. **Constraints.** The range clamps and `clampPose`, the contain
   bounds, occlusion. A bound contains pose plus the PERSISTENT offset
   and moves the pose (so the next frame starts legal); a transient
   shake it clips at push, without touching the pose. Occlusion
   displaces the FINAL eye and leaves the pose's distance alone (see
   the decisions).
5. **Push.** One `setCamera` per driven target, only when something
   changed; `update` reports the change so per-frame dependents can
   follow.

What each control implements (the shared math is
`@solidrt/core/camera-control`: the ease, the framing block, the lanes;
`packages/core/checks/camera-control-check.ts` pins it):

| Stage | 2d (`createCamera2d`) | Orbit | First-person |
|---|---|---|---|
| Source: input | pan, zoom (anchored), roll; inertia | rotate, zoom (dolly; `push` past the floor; anchored), pan (screen or ground plane), focus; damping | look, move, rise, boost |
| Source: follow | `follow(x, y)` | `follow(point)` | none |
| Framing | dead zone, hard limits, damping per axis, lookahead | the same, in view space (right, up, forward) | n/a (the walker is the source) |
| Lanes | `offset`, `shake` | `offset`, `shake`, `setOrbitPoint`'s share | `shake` (a view kick in turns) |
| Constraints | zoom range, contain bounds (damped for a motion when `world.damping`) | ranges, `clampPose`, `occluder` (the component fills it from a raycast) | pitch clamps, `clampPosition` |
| Push | `camera()` is the final camera | `camera()` is the final camera | pose plus the shake |

Landed 2026-09-22 through
[camera-and-controls-extensions](../plans/camera-and-controls-extensions.md)
(orbit) and [2d-camera-framing](../done/2d-camera-framing.md) (2d); the
first-person reference frame is its own item
([3d-first-person-reference-frame](../backlog/3d-first-person-reference-frame.md)).

## Decisions

Each with the alternative rejected and why.

**One control per species carries follow and free navigation.** The 2d
control is Godot's Camera2D and Three's MapControls in one; the orbit
control becomes Cinemachine's Orbital Follow when `follow` is engaged:
the orbit's centre chases the point, the orbit's own input keeps
working around it. Rejected: a separate follow camera per package
(Babylon's FollowCamera, a "ThirdPersonCamera" component). It would
duplicate the pose, the clamps, the glide and the input wiring, and a
game switches between "follow the player" and "let the player orbit"
constantly; that switch is `follow`/`unfollow` on one control, not a
remount. The roadmap's earlier deferral of a follow rig "until a
third-person game asks" is withdrawn: the reference set treats it as
table stakes, and the framing block is shared with 2d anyway.

**Framing math is shared and pure, in core.** Zones, lookahead and
per-axis easing work on "the point's offset from the pivot in viewport
fractions" and return a correction in the same units; each control
projects its point in and applies the correction in its own space (2d:
the pose's x/y; orbit: `slide` along right/up and a forward move for
depth). The module lives in `packages/core` (`@solidrt/core/camera` is
taken by capture, so a `camera-control` entry: the ease constants and
`easeStep` from `motion.ts`, the framing block, the lanes). Rejected:
a copy per package, which is what the ease constants are today (the 2d
control restates `GLIDE_EASE`, `GLIDE_EPSILON` and the log-space rule
from `motion.ts`), and which is how the two controls drifted. Headless
checks run the shared module once.

**Zones: dead inside soft inside hard, hard limits clamp at once.**
Cinemachine's model, complete. Godot has only the margins (a dead zone)
and Phaser a dead zone plus a lerp; both let a fast target leave the
screen when the smoothing is lazy, which is the bug every platformer
camera tutorial then works around. The hard limits are the guarantee;
the soft zone is where damping applies. Damping is per axis (Cinemachine
and Godot both split it: a platformer wants a tight horizontal and a
lazy vertical), given as one number for both or an object per axis.
Lookahead projects the point ahead by its velocity times `time`, the
velocity estimated from successive `follow` calls and smoothed by
`smoothing` (Cinemachine's two knobs; the smoothing is what keeps a noisy
input from jittering the camera). Zones are fractions of the viewport
centred on the pivot; a missing zone is zero-sized (dead) or the whole
viewport (hard), so today's `deadZone` option is the model with two
defaults.

**Lanes are additive and never enter the pose.** `pose()` stays what the
app set or the follow reached; `set()` and `glideTo` are unaffected by a
running shake; a lane cannot accumulate into the pose through the
constraints. Cinemachine applies noise after the body and aim stages and
before the finalize extensions, so the shake still runs into the
confiner and cannot show the outside of the world; ours does the same by
CLIPPING the shaken camera at push, not by moving the pose (the first
cut contained pose plus shake by moving the pose, and live at the fit
zoom, where the bounds pin the camera, that made the pose wobble while
the picture stood still - the exact inversion). The persistent offset is
contained by moving the pose, since a follow or a pivot means it to
hold. The offset lane doubles as the orbit point off the view axis:
`setOrbitPoint(point)` puts the target at the point and sets the offset
so the picture does not move (camera-controls' `setOrbitPoint` plus
focal offset). Rejected: a shake that writes the pose and restores it
(Phaser's approach): a `set()` during the shake then lands on a shaken
pose, and a bounds clamp during the shake eats the restore.

**Shake is a control primitive with one shape.** `shake(strength,
duration, frequency?)`: a decaying noise offset in viewport fractions,
one call per impulse, impulses summing. Rejected: Cinemachine's spatial
propagation (sources with a position and a listener with a radius). It is
a game-side concept over one primitive; the 2d package or the app can
add distance falloff when a game asks, and the primitive stays the same.

**Constraints run last, every write, and occlusion is instant in, damped
out, as a displacement of the final eye.** An obstacle between target
and eye pulls the eye in at once (a wall through the camera is the one
thing a player always notices) and the return when the obstacle clears
is damped (Cinemachine's deoccluder damping). Rejected: Godot's
SpringArm3D rule, instant both ways, whose snap back is the second thing
a player notices. The pose's distance is untouched (Cinemachine keeps
the deoccluder's displacement as its own state): a zoom out from behind
a wall still goes where the pose says once the wall is gone, and a
damped return has a fixed goal to ease to. Rejected: shortening the
pose's distance, which a zoom would then fight. The pure control still
knows no level: it takes a hook, `occluder(target, eye) => free distance
| null`, and the `<OrbitCamera>` component fills it from the scene
(`raycast` from the target toward the eye, `radius`, `layers` and
`meshes` to mask the followed character out), the same split as the
anchor hooks. This replaces the roadmap's "collision stays outside every
control": the control's hook is outside, the component's wiring is
inside, and an app with its own level format supplies the hook itself.

**Dolly is bounded, the push is the overflow.** The orbit `zoom` axis
stays a dolly (multiplicative, in octaves, ending at `minDistance`), and
with `push` on, a step that would cross the floor moves eye and target
together by the overflow instead, along the view axis or the anchor ray
when anchored (camera-controls' `infinityDolly`, verified: "will keep
the distance and pushes the target position instead"). The eye's speed
is continuous across the floor, so a pinch through a model never stalls
and never lurches. Rejected: always-push (no reference does it; a dolly
that ends at the subject is the right default for a viewer), and the
fly demo's fixed world step per octave (speed does not scale with the
subject, so it is either a crawl outside the model or a leap inside).

**Zoom to cursor is built in where the projection is; the pivot
re-seat is explicit.** `<OrbitCamera>` builds the anchor mapping itself
from its owner, the scene or a view (`pick` first, the target-depth
plane through `unproject` as the fallback), one option, `anchor: "pick"
| "plane" | false`, on by default (the touch-first viewer's default;
Three and Babylon default theirs off, but a viewer that zooms at the
target under a finger aimed elsewhere is the complaint every model
viewer gets). The pivot re-seat on every press (`repivot`, Blender's
auto-depth) is OFF by default: it changes `distance` on each tap, which
a viewer reading the pose does not expect, and a double tap (`focus`)
re-seats explicitly, which is what Three's Arcball and Sketchfab do.
`zoomAnchor`/`rotateAnchor` stay as overrides. Three's
`zoomToCursor` applies to the wheel and the pinch (verified in source);
Babylon's `zoomToMouseLocation` is wheel-only. Ours applies to both,
and to `focus`. A `ViewHandle` gained `project`, `unproject`,
`screenRay` and `raycast` for this, so a view is a full owner.

**A discrete gesture carries its focal.** A double tap has to say where
it landed for a focus to mean anything, and the input map's button
channel carries no position. Rejected: the component listening to the
scene's taps itself (device handling in a component, against the
architecture) and a new recognizer. Instead the pointer feed's pulses
(`doubleTap`, `longPress`) bound to an AXIS action nudge 1 with the
gesture's focal, the same channel a wheel notch uses, and contribute no
RATE (a pulse's one-task press integrated as a rate zoomed the 2d
camera by 2^(1/60) and interrupted the octave glide, live); the orbit
control declares `focus: "axis"` and `orbitBindings` binds the double
tap to it, `camera2dBindings` binds it to `zoom` (one octave in at the
point, the map convention). A pad button on the same action focuses
the view centre.

**Easing stays exponential, zoom in log space, glides land exactly.**
Unchanged from the 2d conventions and `motion.ts`; the shared module
makes it one copy. A per-axis damping value is an e-folding rate
multiplier like today's `damping`, never a lerp factor per frame.

**Shots and blends live above the controls, and need no producer
mode.** Cinemachine's headline feature (several virtual cameras, a
priority, a blend between the active two) is the architecture of a game
camera. A control already drives "anything with setCamera", so a shot
is a RECORDING target handed to a control, and the blender
(`createShotBlend` in core; `createShots` in each package with its mix)
owns the one push to the scene: the live shot's pushes go straight
through at rest, a blend runs on `update(dt)` and eases in and out over
a fixed time from the output of the moment it started (a quick
back-and-forth never jumps), the live shot is the enabled one with the
highest priority. Rejected: a producer mode on every control (a second
API on each, for the blender's benefit alone) and Cinemachine's blend
curves per pair (a fixed smoothstep and a time per switch cover a game;
curves are additive). The 2d mix blends zoom in log space; the 3d mix
cuts a perspective-to-ortho switch at the midpoint since no
in-between projection exists.

**A followed heading recentres the azimuth after the input rests.**
`follow(point, heading)` takes the followed thing's yaw in the
first-person convention, so a walker's pose feeds a chase camera with
no conversion, and the azimuth eases onto it by the shortest turn once
the rotate input has rested for `wait` (Cinemachine's Recentering: a
drag looks around, the camera settles back behind the walker).
Rejected: binding the orbit's frame to the heading outright
(Cinemachine's LockToTarget modes), which makes a drag fight the
walker's every turn; the recentre with a wait is the mode every
third-person game ships.

**Shots have a component form that redirects the context.** `<Shot>`
provides the enclosing context with the shot's recording target as the
viewport (2d: the view behind a Proxy with its camera redirected, so
size and handlers stay live), so the camera components inside it need
no shot awareness at all. Rejected: a `shot` prop on each camera
component (every control component would carry it).

**The first-person shake is a view kick in turns.** A positional shake
on a walker reads as the world jolting; first-person games kick the
view (recoil, a hit). Yaw and pitch offsets in turns, the look axis's
unit, so the control needs no fov and every lane stays device-free.
Rejected: the positional shake in world units of the first cut.

## Input conventions (unchanged)

The controls are pure axis consumers over `createAxes`, driven by an
input map; the bindings presets (`camera2dBindings`, `orbitBindings`,
`firstPersonBindings`) are the standard wiring and bind nothing unless
the app says so. A gesture-bracketed delta applies at once and an
unbracketed one (a wheel notch, a key step) glides in; a gesture's end
brings its release velocity. Keys and sticks move the camera where a
drag moves the content. See the packages' `AGENTS.md` and the 2d
conventions note for the units.

## Known limits and open items

- The fly demo (`~/solidrt/demoes/fly`) still carries its hand-rolled
  push and turn; it moves to `push` and `setOrbitPoint` with the user
  ([camera-and-controls-extensions](../plans/camera-and-controls-extensions.md),
  item 9).
- A `<Shot>`'s picks and raycasts (`anchor`, `occlusion`) resolve
  through the owner's actual camera, which is the shot's own only while
  it is live and at rest; a non-live shot's anchored zoom aims through
  the wrong camera. A per-shot projection would need the scene to
  project through an arbitrary camera. Additive.
- The heading recentre turns the azimuth only; a followed thing that
  pitches (a plane) would want the elevation too. Additive.
- Trackball/arcball (a control without a fixed up vector) and object
  manipulation (Three's TransformControls/DragControls) are not camera
  pipeline work; both are `ideas.md` lines.
- A signal-read trap every control met: `notify` must compare against a
  plain flag, not the signal's own read, because between flushes the
  read reports the value before the queued writes and a `false` is then
  never written (found by the 2d check's damped-bound case; the orbit
  checks passed only because they flush mid-sequence).
