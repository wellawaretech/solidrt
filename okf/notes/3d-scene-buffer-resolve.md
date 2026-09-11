---
title: The scene buffer and its resolve - what the measurements said
description: Findings from making the linear buffer plus resolve pass the only 3d pipeline - why the resolve is a draw target, the pass cost on the Intel/Mesa laptop, and the bytes that prove linear-space blending, the round trip and the orientation.
created: 2026-09-11
---

# The scene buffer and its resolve

Cut from [3d-hdr-scene-buffer](../done/3d-hdr-scene-buffer.md) on
completion; the pipeline itself is documented in `packages/3d/AGENTS.md`
(Color).

- The resolve is a DRAW target with one covering-triangle entry, not a
  shader target: `set_target_params` on a shader target validates every
  name strictly against the one program, while a draw target tolerates
  zero coverage. The scene's `setParams` fan-out (uExposure,
  uToneMapping, app names) can therefore reach the resolve unchanged,
  and a custom resolve reads any scene param like the background does.
- Cost on the Intel/Mesa laptop (GPU timer queries are unavailable
  there: the attribution self-test fails, so measured by subtraction
  with `probes/3d-hdr-resolve-bench.tsx`): 8 full-HD views plus the
  fill scene hold 60 fps with and without the resolve; at 20 views the
  frame is ~27 ms against ~22.5 ms, about 0.2 ms per 1080p target for
  the pass plus the half-float bandwidth. The tablet, the TV and the Pi
  are not measured; the low-end answer if one cannot afford it is a
  smaller buffer, never a second pipeline.
- Linear-space blending is measurable: two 50% white quads over black
  read 225 (0.75 linear) where encoded-space blending read 191.
- An opaque sRGB 0.5 gray round-trips to 128 exactly through rgba16f,
  the resolve and its dither; a custom resolve summing a param-scaled
  scene texel and a bound rgba8 texture lands within 1 of the computed
  byte.
- The resolve samples the buffer at the covering triangle's vUV and the
  picture is upright: a draw target read by a pass and the display read
  of the pass agree on orientation, no flip anywhere.
- `probes/scene-set-probe.tsx` still carried the pre-WebGPU `"vec2"`
  attribute format name and failed at withAttribute before this work;
  fixed in passing to `"float32x2"`.
