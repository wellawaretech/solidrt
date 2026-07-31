---
type: backlog-item
title: Sampler filter and wrap state
description: Every texture samples linear, with wrap fixed per creation path (clamp-to-edge for targets, repeat for createTexture); nearest magnification is unreachable, which rules out the whole retro/pixel-art category.
status: done
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

## Resolution (2026-07-29)

Implemented as proposed, plus two decisions and one architectural change:

- **API**: `filter?: "linear" | "nearest"` and `wrap?: "clamp" | "repeat"` on
  `createTexture`/`createMutableTexture`/`createShader`/`createPipeline`/
  `createShaderTarget`, applied at creation as a property of the texture id.
  Creation-time only; no `setSamplerState` (still demand-gated, and now a
  two-liner if demanded: update the registry entry, mark the id dirty).
- **Wrap default unified to clamp** for every creation path - the
  `createTexture` implicit repeat is gone; repeat is an explicit opt-in.
  Breaking, accepted deliberately (no backwards-compat requirement).
- **Not GL texture-object state.** Impeller GLES configures per-draw sampling
  by mutating the bound texture's parameters, so object state on any
  displayed texture would not survive a frame. Instead: four shared GL
  sampler objects (SamplerCache, alloy/src/texture.rs), bound per input unit
  in `run_pass` and restored on exit - alloy's passes are immune to
  Impeller's writes and vice versa. This also deleted the reapply-on-resize
  problem: the state lives in the registry entries (`GpuTexture`,
  `TextureEntry`, `ShaderTexture`) and follows the id through resizes.
- **Display path**: `<texture>` paint maps the id's filter to Impeller's
  per-draw `TextureSampling` (kinds/texture.rs), so nearest applies on screen
  too - the actual retro/pixel-art case. Internal textures (window-shader
  layers, MSAA resolve, snapshot rigs) are untouched and keep linear.

Depends on [[gpu-target-dependency-propagation]]: consumer re-renders after
resizes and any future sampler mutation ride the dirty-flush propagation.

Docs: docs/core.md, flux-types gui/gpu.d.ts (`SamplerOptions`), core gpu.ts
(`createShaderMemo` rebuilds on filter/wrap change).

Runtime-verified 2026-07-31 on Linux, Windows/ANGLE and the 2017 Android TV,
with pixel-identical results on all three - both decisive checks this note
named. A 4x4 checkerboard upscaled to 96px by `<texture>` displays hard-edged
under `filter: "nearest"` and smoothly interpolated under `"linear"` (the
Impeller display path), and a shader sampling the same source at `vUV * 3`
tiles it 3x3 under `wrap: "repeat"` while `"clamp"` smears the edge texels
(the GL sampler-object path). Impeller's cached state does not clobber the
shared sampler objects, which was the risk the SamplerCache design was
chosen to avoid.
