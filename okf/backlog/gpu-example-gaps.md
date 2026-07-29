---
type: backlog-item
title: GPU example gaps blocked on runtime work
description: A multi-pass shader chain example, deferred until target dependency propagation is decided. The points-topology particle field shipped 2026-07-29 once the blend toggle landed.
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

DONE 2026-07-29: `packages/core/examples/gpu-particles.tsx`, written the day
the blend toggle landed in [[gpu-pipeline-extensions]]. An additive fibonacci-
sphere splat field: `topology: "points"` + `blend: "add"`, gl_PointSize from
the vertex stage, gaussian gl_PointCoord splats, premultiplied additive
output, typed vec3 tint uniforms. Deliberately no depth buffer (nothing
occludes in a pure additive pass); the header comment states when a scene
adds `depth: true` with `depthWrite: false`. Runtime-unverified as of filing.

The multi-pass gap above is not urgent. Filed so it is picked up with the
runtime change that unblocks it rather than rediscovered later.
