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
[camera-and-controls-extensions](../backlog/camera-and-controls-extensions.md).

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
  does not respond), the **soft zone** around it (the camera eases the
  point back toward the dead zone, damped per axis), the **hard limits**
  around that (the point is clamped inside at once, whatever the
  damping), and **lookahead** (the point is projected ahead along its
  velocity, smoothed).
- **Lanes.** Additive offsets applied on top of the pose at push time,
  never written into it: the **offset** lane (a persistent screen-space
  offset: Babylon's `targetScreenOffset`, camera-controls' focal offset,
  Godot's `offset`, Phaser's `followOffset`) and the **shake** lane (a
  transient decaying noise: Cinemachine Impulse, Phaser `shake`).
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
4. **Constraints.** Pose plus lanes are clamped: the range clamps and
   `clampPose`, the contain bounds, occlusion. A constraint that has to
   move the camera moves the POSE (so the next frame starts legal), and
   the lanes stay as they are.
5. **Push.** One `setCamera` per driven target, only when something
   changed; `update` reports the change so per-frame dependents can
   follow.

What each control implements today, and what the pipeline adds (the
open items link the backlog files):

| Stage | 2d (`createCamera2d`) | Orbit | First-person |
|---|---|---|---|
| Source: input | pan, zoom (anchored), roll; inertia | rotate, zoom (dolly, anchor hook), pan; damping | look, move, rise, boost |
| Source: follow | `follow(x, y)`, one damping rate | none | none |
| Framing | dead zone only | none | n/a (the walker is the source) |
| Lanes | none | none | none |
| Constraints | contain bounds, zoom range | ranges, `clampPose` | pitch clamps, `clampPosition` |
| Push | yes, change-gated | yes, change-gated | yes, change-gated |

Adds: 2d gets the soft zone, hard limits, lookahead and per-axis damping
in framing, both lanes, and damped bounds
([2d-camera-framing](../backlog/2d-camera-framing.md)). Orbit gets a
follow source with the same framing, both lanes, the occlusion
constraint, the push, and the built-in anchor and pivot
([camera-and-controls-extensions](../backlog/camera-and-controls-extensions.md)).
First-person gets the shake lane and stays otherwise as it is; a
reference frame is its own item
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
before the finalize extensions, which is exactly this order: the shake
still runs into the confiner, so it cannot show the outside of the
world. The offset lane doubles as the orbit point off the view axis:
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
out.** An obstacle between target and eye pulls the eye in at once (a
wall through the camera is the one thing a player always notices) and
the return when the obstacle clears is damped (Cinemachine's deoccluder
damping). Rejected: Godot's SpringArm3D rule, instant both ways, whose
snap back is the second thing a player notices. The pure control still
knows no level: it takes a hook, `occluder(target, eye) => distance |
null`, and the `<OrbitCamera>` component fills it from the scene
(`scene.raycast` from the target toward the eye with a radius), the same
split as the anchor hooks. This replaces the roadmap's "collision stays
outside every control": the control's hook is outside, the component's
wiring is inside, and an app with its own level format supplies the
hook itself.

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

**Zoom to cursor and the dynamic pivot are built in where the projection
is.** `<OrbitCamera>` in a `<Scene>` builds the anchor and pivot mapping
itself (`scene.pick` first, the target-depth plane through
`scene.unproject` as the fallback), one option, `anchor: "pick" |
"plane" | false`; `zoomAnchor`/`rotateAnchor` stay as overrides. Three's
`zoomToCursor` applies to the wheel and the pinch (verified in source);
Babylon's `zoomToMouseLocation` is wheel-only. Ours applies to both.
Known limit: a `ViewHandle` has `pick` but no `unproject`, `project` or
`screenRay`, so inside a `<View3d>` only the pick mode works until the
view gains them.

**Easing stays exponential, zoom in log space, glides land exactly.**
Unchanged from the 2d conventions and `motion.ts`; the shared module
makes it one copy. A per-axis damping value is an e-folding rate
multiplier like today's `damping`, never a lerp factor per frame.

**Shots and blends live above the controls.** Cinemachine's headline
feature (several virtual cameras, a priority, a blend between the active
two) is the architecture of a game camera and belongs on the roadmap,
but it needs the controls to produce a final camera without pushing it,
so a blender can own the one `setCamera`. That is a producer mode on
each control (`camera()` already exists on 2d; the 3d controls push
only) and a small blender over `CameraUpdate`. Not designed yet; it is
in `ideas.md` and is shaped after the pipeline above exists.

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

- Orbit: push, built-in anchor and pivot, orbit point off the view axis,
  follow with framing, occlusion, shake, the map preset and pan plane,
  double-tap to focus:
  [camera-and-controls-extensions](../backlog/camera-and-controls-extensions.md).
- 2d: soft zone, hard limits, lookahead, per-axis damping, both lanes,
  damped bounds, rotation smoothing:
  [2d-camera-framing](../backlog/2d-camera-framing.md).
- `ViewHandle` lacks `unproject`/`project`/`screenRay`, so the built-in
  anchor inside a `<View3d>` is pick-only (noted in the 3d item).
- Double-tap to focus wants a tap recognizer in core (`ideas.md`: a
  `createTap` beside `createPan`); until then the component derives it.
- Shots and blends: `ideas.md`, shaped after the pipeline lands.
- Trackball/arcball (a control without a fixed up vector) and object
  manipulation (Three's TransformControls/DragControls) are not camera
  pipeline work; both are `ideas.md` lines.
