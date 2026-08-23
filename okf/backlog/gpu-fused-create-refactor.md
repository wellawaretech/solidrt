---
title: Refactor the fused creates over the raw shading layer
description: The naming collision and the iTime trap were resolved 2026-07-31 (createShaderTexture/createPipelineTexture/createShaderTextureMemo, hard rename; iTime dropped from all preambles - the preamble now declares exactly what the runtime fills). Still open - the composition questions - whether the fused paths become thin compositions of the raw layer, whether a mid-level program shorthand is wanted, and the two-dialect preamble story.
created: 2026-07-27
---

# Refactor the fused creates over the raw shading layer

The raw layer landed 2026-07-27: `compileShader(stage, source, { header? })`,
`linkProgram(vs, fs)`, `createShaderTarget(program, w, h, opts)`,
`destroyShader`, `destroyProgram`. The fused creates were deliberately left
as-is (decided 2026-07-27): they predate the split, compile+link inside
`ShaderTexture::new`/`new_pipeline`, and carry the conditional preamble
("inject unless the source starts with #version") that the raw layer replaced
with an explicit `header` opt-in.

## Resolved 2026-07-31: the names and iTime

Both naming findings from [gpu-review](../notes/gpu-review.md) landed as
one hard rename (no aliases, no compat, matching the setDrawCount -> setDraw
precedent):

- `createShader` -> **`createShaderTexture`**, `createPipeline` ->
  **`createPipelineTexture`** - the names the engine already used internally
  (`Context::create_shader_texture`, `create_pipeline_texture`), saying what
  comes back. `createShaderMemo` -> **`createShaderTextureMemo`** for the
  same reason. `createShaderTarget` keeps its name: a target IS the retained
  object, and gpu-review praised that collapse.
- **`iTime` is gone from all three preambles**, not filled and not declared.
  The rule that replaced it: **the preamble declares exactly what the runtime
  provides** (version, precision, `vUV` on the fragment path, `fragColor`,
  `iResolution`); anything app-driven is the source's own declaration, driven
  through params like any other uniform, so forgetting to drive it is a
  compile error instead of a silent t=0. An auto-filled iTime was considered
  and rejected: under the pull-based dirty model it would make every
  iTime-reading target a hidden per-frame pass - 0.5-0.7 ms each on TV-class
  hardware (see the gpu-review verification section). An opt-in declare flag
  was rejected too: it buys one GLSL line and keeps the trap. The Shadertoy
  complete-source dialect is unaffected (nothing was ever injected there).
  Examples now drive `uTime`, their own declaration.

Touched: alloy gpu/program.rs preambles, flux plugin exports + error strings,
flux-types, core gpu.ts, packages/core/docs/reference/gpu.md,
both AGENTS.md files, all GPU
examples, gpu_split.rs.

## Still to look into

- **Make the fused paths compositions of the raw layer.** Engine-side this is
  mostly done (`new`/`new_pipeline` are already thin wrappers over
  `from_fragment_program`/`from_pipeline_program`); the JS-visible question
  is whether `createShaderTexture`/`createPipelineTexture` should literally
  be compile + link + target over registered ids (visible in gpu resources,
  uniform destruction semantics) instead of an anonymous program dying with
  its target.
- **The conditional preamble.** The raw layer made the header explicit; the
  fused paths still sniff for `#version`. Aligning them (an explicit
  `header`/`raw` option, defaulting to the current behavior) would leave one
  preamble story instead of two. (What the preamble contains is now settled -
  see above - this is only about how a source opts in or out.)
- **A mid-level program shorthand.** The window effect
  (okf/plans/root-layer-effects.md stage 2) wants a one-liner for "fragment
  program over the standard fullscreen vertex stage". Raw spelling is ~5
  lines incl. writing the covering triangle. Whether that deserves a
  shorthand (a `compileProgram(fragmentSrc)`-style convenience, or a core
  helper) and what it is named was deferred when the raw layer was chosen
  over the earlier mid-level `compileShader(fragmentSrc)` draft.
- **`createShaderTextureMemo`** composes over `createShaderTexture` today; if
  specs get a program handle field, the memo's rebuild rule ("new source =
  new id") changes shape.

The fragment-kind program (fullscreen triangle draw path, no mesh state) is
currently only reachable through fused `createShaderTexture`; every
raw-linked program is pipeline-kind. A shorthand that produces fragment-kind
handles would make the two kinds symmetric across layers.
