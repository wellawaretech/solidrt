---
type: backlog-item
title: GPU example gaps blocked on runtime work
description: Two core examples worth writing - a multi-pass shader chain and a points-topology particle field - each deliberately deferred until the runtime behaviour they would demonstrate is settled.
status: deferred
timestamp: 2026-07-29T00:00:00Z
---

# GPU example gaps blocked on runtime work

Both surfaced while acting on the 0.0.39 field reports
(projects/shadertoy and projects/second-reality). Three examples came out of
that round and shipped on 2026-07-29 - `text-import.tsx`,
`gpu-texture-blend.tsx`, and the complete-source dialect added to
`gpu-shader.tsx`. These two did not, for reasons that are about the runtime
rather than the examples.

## Multi-pass shader chain

A worked example of one target sampling another: a plasma pass feeding a cube
pipeline, the shape second-reality actually built.

Blocked on [[gpu-target-dependency-propagation]]. Written today the example
would have to teach the workaround - "drive one uniform per frame in every node
of a live chain, or the consumer silently keeps a stale frame" - which bakes a
bug into the example corpus and would then have to be un-taught. Write it once
propagation is decided, in whichever direction: if consumers get marked dirty
the example is simply a chain, and if the rule stays as-is the example becomes
the place the rule is demonstrated rather than merely documented.

## Points topology / particle field

`topology: "points"` has no example. Second-reality verified it works well:
`gl_PointSize` honored across 4..64px and `gl_PointCoord` available, so
shader-driven point clouds are viable.

Softly blocked on the blending toggle in [[gpu-pipeline-extensions]]. A
convincing particle field wants additive accumulation between overlapping
splats, and a target's own draw currently runs with GL blending disabled, so
overlapping points overwrite instead of accumulating - the organism project hit
exactly this ("`gl_PointSize > 1` draws opaque discs, so a point cloud can only
be thickened into a scaly overlap, never a smooth field"). An example is
possible today with non-overlapping points, but it would showcase the
limitation rather than the capability.

Neither is urgent. Filed so they are picked up with the runtime change that
unblocks them rather than rediscovered later.
