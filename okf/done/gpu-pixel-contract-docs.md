---
title: Document the GPU pixel contract
description: Three facts every pipeline author eventually discovers the hard way - clip-space y points down, targets are premultiplied alpha, values are non-linear RGBA8 - are stated nowhere or only in example comments; declaring them is documentation-only and converts silent per-app discoveries into named contracts.
created: 2026-07-30
completed: 2026-08-11
---

# Document the GPU pixel contract

## Done 2026-07-30

All three facts are now stated as one named contract, "the pixel contract",
in the same wording at every site:

- `packages/flux-types/gui/gpu.d.ts` - a paragraph in the file-header block
  after the compositing paragraph, plus a self-contained y-down sentence in
  the `compileShader` and `createPipeline` doc comments (the header block is
  invisible on hover, and those two are where a vertex source gets written).
- `packages/core/src/gpu.ts` - the same paragraph in the top comment block
  and the same y-down sentence on `createPipeline`. The raw-layer calls are
  bare re-exports there, so they carry no second copy.
- `docs/core.md` - a `### Pixel contract` subsection under `## GPU`,
  immediately before `### Raw shading layer` so it sits next to the `vUV`
  top-left-origin sentence that already stated the coordinate half.
- `packages/cli/scaffold/AGENTS.md` - the y-down clause only, in the
  shaders-for-continuous-effects rule. Premultiplied output was already
  stated there (`vec4(vec3(a), a)`), which is why app authors met the alpha
  rule but not the coordinate one.

Verified against the runtime rather than restated from the review: the
y-down derivation is `alloy/src/gpu/program.rs` (the built-in fullscreen
vertex stage documents clip y = -1 landing in memory row 0, which Impeller
samples as the top); premultiplied is `PixelFormat::RGBA8888` at
`adopt_opengl_texture` plus `gpu-particles.tsx` writing `vec4(tint * a, a)`;
non-linear RGBA8 is `glow::RGBA8` in `alloy/src/gpu/target.rs` (no sRGB
internal format anywhere) with `blend: "add"` as `glBlendFunc(ONE, ONE)`.

Left alone deliberately: the `gpu-pipeline.tsx` "Clip y is negated" comment
(it explains that example's projection, and the contract now exists above
it), and the `<texture blendMode>` premultiplied clause in `docs/core.md`,
which is not cross-referenced.

## Original

From [gpu-review](../notes/gpu-review.md) (lessons 12 and 14), shortlist
item 2. Documentation only - no engine change - which is why it is near the
top of the do-order.

Three facts to state, and where they live today:

- **Clip-space y points down** (row 0 is the top). The fragment path
  absorbed the flip (vUV is top-left origin, documented); the pipeline path
  leaks it - every vertex author must negate y in `gl_Position` or render
  upside down, and the contract lives in one example comment
  (`gpu-pipeline.tsx`, "Clip y is negated..."). Vulkan ships the same
  y-down clip space and simply declares it; WebGPU's convention cleanup is
  the reason half of WebGL's all-time questions (Y-flip) do not exist
  there. The runtime cannot absorb this one (gl_Position belongs to the
  app's shader), so declare it.
- **Targets are premultiplied alpha.** Implied by the particles example's
  output and by how Impeller composites, stated nowhere. Declaring it also
  decides the factor pair for the deferred alpha-over blend mode
  ([[gpu-pipeline-blend-modes]]).
- **Values are non-linear RGBA8** (no colour-space concept): linear
  filtering and additive blending operate on non-linear values. Not fixable
  cheaply and not worth fixing now - but stating it keeps shaders written
  against the real contract correct if a format vocabulary ever arrives
  ([[gpu-float-texture-formats]]).

Sites: the pipeline/preamble section of `packages/flux-types/gui/gpu.d.ts`
and `packages/core/src/gpu.ts` doc comments, `docs/core.md`'s GPU section -
next to the existing vUV-origin sentence, which is the half of the
coordinate contract already written down.
