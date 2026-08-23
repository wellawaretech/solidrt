---
title: GPU example gaps
description: A multi-pass shader chain example, formerly blocked on target dependency propagation - which landed 2026-07-29, so the example is now unblocked and simply unwritten. The points-topology particle field shipped 2026-07-29 once the blend toggle landed.
created: 2026-07-29
---

# GPU example gaps

Both surfaced while acting on the 0.0.39 field reports
(projects/shadertoy and projects/second-reality). Three examples came out of
that round and shipped on 2026-07-29 - `text-import.tsx`,
`gpu-texture-blend.tsx`, and the complete-source dialect added to
`gpu-shader.tsx`. These two did not, for reasons that are about the runtime
rather than the examples.

## Multi-pass shader chain

A worked example of one target sampling another: a plasma pass feeding a cube
pipeline, the shape second-reality actually built.

Was blocked on [[gpu-target-dependency-propagation]]; that landed 2026-07-29
in the consumers-get-marked-dirty direction (pull-based flush, chains render
in topological order), so the example is now simply a chain - bind the plasma
target as the cube pipeline's sampler input and drive only the plasma's
uniforms. Unblocked and unwritten as of 2026-07-30 (no gpu-chain example in
packages/core/examples). Worth writing as the demonstration that sampler
bindings are live dependencies, the contract documented in flux-types
gui/gpu.d.ts and in the core GPU page (packages/core/docs/reference/gpu.md,
"The model").

## Points topology / particle field

DONE 2026-07-29: `packages/core/examples/gpu-particles.tsx`, written the day
the blend toggle landed in [[gpu-pipeline-extensions]]. An additive fibonacci-
sphere splat field: `topology: "points"` + `blend: "add"`, gl_PointSize from
the vertex stage, gaussian gl_PointCoord splats, premultiplied additive
output, typed vec3 tint uniforms. Deliberately no depth buffer (nothing
occludes in a pure additive pass); the header comment states when a scene
adds `depth: true` with `depthWrite: false`. Runtime-verified 2026-07-31 on
Linux and the 2017 Android TV: the field renders as intended (point sprites
sized from the vertex stage, gaussian splats, additive accumulation), at
0.62 ms per pass on the TV for 1500 points into a 512x512 target.

The multi-pass gap above is not urgent. Filed so it is picked up with the
runtime change that unblocks it rather than rediscovered later.
