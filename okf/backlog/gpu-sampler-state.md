---
type: backlog-item
title: Sampler filter and wrap state
description: Every texture samples linear, with wrap fixed per creation path (clamp-to-edge for targets, repeat for createTexture); nearest magnification is unreachable, which rules out the whole retro/pixel-art category.
status: open
timestamp: 2026-07-29T00:00:00Z
---

# Sampler filter and wrap state

Raised by the second-reality field report
(projects/second-reality/SOLIDRT-FEEDBACK.md #4), measured against 0.0.39.

Sampler state is fixed at creation and not exposed anywhere:

- **Filter is always linear.** `GpuTexture::new` (alloy/src/texture.rs) and the
  shader/pipeline target paths (alloy/src/shader.rs) all set `LINEAR` min and
  mag. No mips exist, so the default `MIN_FILTER` would make the texture
  sampling-incomplete (reads black) - hence the explicit set, but nothing lets
  an app choose `NEAREST`.
- **Wrap differs by creation path**, which the report did not catch and the
  docs did not state:
  - shader/pipeline render targets set `CLAMP_TO_EDGE` explicitly
  - `createTexture`/`createMutableTexture` set no wrap at all, so GL's default
    `REPEAT` stands

The defaults are now documented (flux-types gui/gpu.d.ts, core/src/gpu.ts,
docs/core.md, 2026-07-29). Making them controllable is the open work.

## Why it matters

Repeat + linear happens to be right for a rotozoomer, so it cost that project
nothing. But nearest magnification has no path at all, and that rules out an
entire app category: render at 320x200 and upscale with hard pixels is the
defining move of retro and pixel-art apps. Linear smoothing is not a stylistic
preference there, it is wrong output.

The per-path wrap split is also a trap in its own right: the same shader
sampling a data texture and sampling another target behaves differently at the
edges, with nothing in the API hinting at it.

## Proposed shape

`filter?: "linear" | "nearest"` and `wrap?: "repeat" | "clamp"` on
`createTexture`/`createMutableTexture`/`createShader`/`createPipeline`, applied
at creation. Whether these also need to be mutable later (a `setSamplerState`
analog to [[gpu-sampler-rebinding]]) is demand-gated - no reported case wants
to change filtering on a live texture.

Worth deciding at the same time whether the per-path wrap default should be
unified rather than merely documented; the split looks like history rather than
intent.
