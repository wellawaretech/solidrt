---
title: What the IK solver stack found
description: Facts that hold without the IK plan - Unity's two-bone turn sign under right-handed math, FABRIK's and CCD's collinear degeneracy and Godot's magnet seed (once, not per sweep), Godot's LookAtModifier3D limit and transition semantics, and the picking index answering a ground ray with the character's own meshes.
created: 2026-10-20
---

# What the IK solver stack found

Cut from [3d-ik](../done/3d-ik.md) at its close.

- Unity's two-bone solve (AnimationRuntimeUtils.SolveTwoBoneIK) turns
  the mid by `newAngle - oldAngle` about `cross(ab, bc)`; Unity is
  left-handed, and in right-handed math the same axis closes the
  interior angle with a POSITIVE turn, so the sign is `old - new`. A
  law-of-cosines test against the solved elbow angle catches it on the
  first run; a visual check does not, since the limb still reaches.
- FABRIK converges slowly toward a nearly straight solution from a
  chain pointing elsewhere: ten sweeps from a rest pose leave a 60 cm
  tail visibly bent for a target just inside full reach, and when the
  target steps outside, the out-of-reach branch snaps it straight - a
  5 cm jump on one frame in a sweep of the probe's orbit, and worse
  with a far-off seed (Godot's SkeletonIK3D seeds its middle chain item
  at the magnet; that seed per sweep held the tail 7 cm short of
  reachable targets, and once per solve it folded the tail into a
  hairpin on isolated frames, 0.8 m tip jumps in the running probe).
  A rigid turn of the whole chain onto the target's direction before
  the sweeps makes the straight case exact at once and starts a bent
  case near its answer. The one true degeneracy is a chain lying on
  the target's axis (every sweep lands back on the line; CCD is stuck
  there too, the shortest arc between parallel vectors being the
  identity), which a small nudge of the middle joint toward the magnet,
  else across the axis, breaks. The magnet's own turn of the interior
  must be an explicit axis-angle about the root-tip axis: a shortest arc
  between the chain's mean radial and the magnet's is a turn OFF the
  axis whenever that radial is float noise (a straight chain), which
  swung the probe's tail 90 degrees on isolated frames. A continuity test (every frame's solve
  from the same rest pose, the target stepped around an orbit, no joint
  jumping more than a few times the target's step) is what catches all
  of this; single-frame reads and short frame steps do not.
- Godot's LookAtModifier3D clamps the angles at the limit (with a
  damped approach past a threshold, `remap_damped`) and runs its
  time-based transition on a target switch and on a flip through the
  back, interpolating via rest when limited (`interpolate_via_rest`); a
  lost target holds the pose rotation; `duration` defaults to 0, a
  snap. A 300 ms transition reads as a head whip on a 137 degree swing
  between symmetric 1.2 rad limits; 600 ms reads as a turn.
- A ground ray through the arena's picking index answers with the
  character's own meshes unless filtered: a skinned mesh picks by its
  bind-pose box at the model root, so a foot ray that sees every layer
  lands on the character. Ground on a collider layer is the rule. The
  index is as of the last flush, so a ray at the post-pass sees a moving
  platform a frame late, Unity's physics raycast timing.
