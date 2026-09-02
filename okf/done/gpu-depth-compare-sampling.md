---
title: Depth comparison sampling
description: Landed 2026-09-02, driven by the program's declared sampler type instead of new API - a pipeline program declaring `sampler2DShadow` for a depth texture gets the engine's comparison sampler (LINEAR + LEQUAL, hardware 2x2 PCF); SHADOW's 3x3 nine-tap loop became one comparison tap.
created: 2026-08-27
---

# Depth comparison sampling

Was: the depth id from `depth: "texture"` sampled nearest with the
compare done in GLSL, so `SHADOW` paid nine taps for a 3x3 PCF whose
softness was step-quantized (it can only average boolean compares).

Landed 2026-09-02, with LESS api than "done looks like" sketched: no
option on `depthTexture` and no sampler state at all. The declared
sampler TYPE is the switch, GL's own model:

- The engine reflects `sampler2DShadow` uniforms
  (`UniformKind::Sampler2DShadow`) and, at binding resolution, gives such
  a uniform the one comparison sampler object (LINEAR, clamp,
  `COMPARE_REF_TO_TEXTURE` + LEQUAL) instead of the texture's declared
  state - per ENTRY, so one shared depth binding serves a comparing
  receiver and a raw-reading `sampler2D` post effect in the same pass.
- Validation both ways: a color texture behind a `sampler2DShadow`
  uniform is refused at bind (UI-side eagerly where uniform kinds are
  known, raster-side warn-and-drop as backstop); a depth id on a plain
  `sampler2D` stays legal (raw reads, nearest). Window shaders refuse
  `sampler2DShadow` (their pass has no comparison path).
- `SHADOW` in `@solidrt/3d/glsl`: `uShadowAtlas` is a
  `sampler2DShadow` and `shadowSample` is ONE `texture(map, vec3(uv,
  ref))` tap - the hardware LEQUAL compare of `p.z - bias`,
  LINEAR-weighted over the 2x2 footprint (bilinear weighting of the
  COMPARE, the step the loop could not take). Signatures of
  `shadowAt`/`lightShadow` and the whole receiving contract unchanged;
  `lit` and every composing custom material just recompiled onto it. A
  hand-rolled fragment still declaring `sampler2D uShadowAtlas` keeps
  raw nearest reads and its own loop.
- The nothing-casts placeholder became a one-texel `depth: "texture"`
  target's depth (cleared to 1 = lit) - a comparison sampler needs a
  real depth texture behind it.
- The engine preambles (fragment and both pipeline stages) declare
  `precision highp sampler2DShadow;` - ES 3.0 gives the type NO default
  precision (unlike sampler2D's lowp), so without it every declaring
  source would need its own qualifier. A complete `#version` source
  still owns its precision statements, as it does for float.

Verified on the Linux client: shadows, cascades (handover band clean),
lamps (spot maps) and instanced (shadowVertex casters) examples all
render correctly, 61 fps, no GL or shader warnings. Windows/ANGLE not
yet exercised (ES 3.0 core feature; expected to translate).

## Not in this

Percentage-closer soft shadows, variance/exponential maps: different
algorithms, not sampler state.
