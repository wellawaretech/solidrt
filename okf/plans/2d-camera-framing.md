---
title: 2d camera framing and lanes
description: createCamera2d's follow is a dead zone plus one damping rate, so a fast target can leave the screen and a platformer cannot follow lazily on one axis; bring it to the design's pipeline - soft zone, hard limits, lookahead, per-axis damping, the offset and shake lanes, damped bounds, rotation smoothing - with the framing math shared with the 3d orbit control.
created: 2026-09-22
---

# 2d camera framing and lanes

The 2d half of [camera-controls](../design/camera-controls.md); the
orbit half is [camera-and-controls-extensions](camera-and-controls-extensions.md).
The decisions that stay (pose at a pivot, contain bounds, exponential
easing, log-space zoom, input units) are in
[2d-camera-conventions](../notes/2d-camera-conventions.md).

## Symptom

`createCamera2d` (`packages/2d/src/camera2d.ts`) follows a point with a
dead zone and one damping rate for both axes (`deadZone`,
`followSpeed`). Against the reference (Cinemachine's Position Composer;
Godot's Camera2D for the minimum) that is missing:

- A **hard limit**: with pure exponential damping a fast target leaves
  the screen, and the lazier the damping the sooner. Nothing guarantees
  the target stays in view.
- A **soft zone**: today damping applies everywhere outside the dead
  zone; there is no band where it applies and an edge where it stops.
- **Per-axis damping**: a platformer wants a tight horizontal follow and
  a lazy vertical one (Godot and Cinemachine both split it).
- **Lookahead**: framing ahead of a moving target so the player sees
  where they are going.
- **Lanes**: a persistent screen offset (Godot `offset`, Phaser
  `followOffset`) and a shake, which today an app writes into the pose
  by hand and which then fights `set()` and the bounds clamp.
- **Damped bounds** (Godot `limit_smoothed`, Cinemachine's confiner
  damping): the contain clamp cuts an eased follow dead at the edge.
- **Rotation smoothing** (Godot `rotation_smoothing_*`).

## Done looks like

1. **Shared framing module in core** (`packages/core`, a `camera-control`
   entry beside `input`): the ease constants and `easeStep` (moved from
   `packages/3d/src/motion.ts`, which re-exports or goes), the zone
   block (dead, soft, hard limits, in viewport fractions around the
   pivot; returns the correction per axis), lookahead (velocity from
   successive points, smoothed), per-axis easing, and the lanes (offset,
   shake with decay and frequency). Pure, headless-checked once.
2. **`follow` option group**: `follow?: { damping: number | { x, y },
   deadZone, softZone, hardLimits, lookahead?: { time, smoothing } }`,
   replacing `deadZone` and `followSpeed` (no compatibility shim: the
   examples move). Zones are fractions of the viewport centred on the
   pivot; hard limits clamp at once, the soft zone eases, the dead zone
   ignores.
3. **Lanes**: `offset` (viewport fractions, live option or `setOffset`)
   and `shake(strength, duration, frequency?)`, summed at push, never in
   `pose()`; the bounds clamp sees pose plus lanes and moves the pose.
4. **Damped bounds**: `world` gains an optional damping so the contain
   clamp eases in instead of cutting; the follow's own ease and the
   bound's compose (the pose heads for the clamped goal).
5. **Rotation smoothing**: `rollSpeed`'s glide counterpart for `set({
   rotation })` through `glideTo`, and the roll axis damped like the
   zoom notch.
6. **Checks and examples**: `checks/camera2d-check.ts` covers the hard
   limit (a target moved faster than the damping can follow never
   leaves the limits), the per-axis split, lookahead landing, a shake
   that never enters `pose()` and never shows the outside of the world;
   `examples/camera.tsx` follows a moving sprite with the zones drawn.
   `packages/2d/AGENTS.md` and the conventions note updated.
