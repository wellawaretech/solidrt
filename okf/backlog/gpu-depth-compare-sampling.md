---
title: Depth comparison sampling
description: A depth id samples NEAREST with the compare done in GLSL, so a shadow lookup pays nine taps for a 3x3 PCF; ES 3.0's sampler2DShadow compares in hardware at one LINEAR tap (2x2 PCF). Additive sampler state on the depth id; changes SHADOW's body, not its signature.
created: 2026-08-27
---

# Depth comparison sampling

Symptom: shadow edges cost nine texture taps per fragment per casting
light. The depth id from `depth: "texture"` (see
[3d-shadow-maps](../done/3d-shadow-maps.md) stage 1) is sampler-only with a
fixed nearest/clamp state, because LINEAR on a plain depth texture would
blend depth VALUES (meaningless at an edge). So `SHADOW` in
`@solidrt/3d/glsl` reads `.r` and compares by hand, and softness is a
3x3 loop.

ES 3.0 has the answer in core: `TEXTURE_COMPARE_MODE =
COMPARE_REF_TO_TEXTURE` on the depth texture turns it into a
`sampler2DShadow`, whose `texture(map, vec3(uv, ref))` returns the
compare result and, with LINEAR, the hardware's 2x2 PCF - one tap for
what the loop does in four, and a quality step the loop cannot match
(bilinear weighting of the compare, not of the depth).

## Done looks like

`depthTexture(target)` accepts an option (or the depth id takes a sampler
state) `compare: "less"` that sets the compare mode and allows LINEAR;
a receiving program then declares `sampler2DShadow` for that id. The
engine's reflection knows the sampler kind, so binding a comparison
sampler to a plain `sampler2D` uniform (or the reverse) is refused at
bind, not left to a black frame. `SHADOW`'s body becomes one
comparison tap (or a 3x3 of comparison taps for a wider kernel);
`shadowAt`/`lightShadow` in `SHADOW_LOOKUP` keep their signatures, so
`lit` and custom receivers do not change. Pure quality, no API change
for apps using `lit`.

## Not in this

Percentage-closer soft shadows, variance/exponential maps: different
algorithms, not sampler state.
