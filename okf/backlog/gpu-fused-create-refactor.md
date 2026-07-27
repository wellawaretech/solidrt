---
type: backlog-item
title: Refactor createShader/createPipeline over the raw shading layer
description: The fused conveniences predate compileShader/linkProgram/createShaderTarget and still compile+link internally with conditional preamble sniffing; decide whether they become thin compositions of the raw layer, and whether a mid-level program shorthand is wanted.
status: open
timestamp: 2026-07-27T00:00:00Z
---

# Refactor createShader/createPipeline over the raw shading layer

The raw layer landed 2026-07-27: `compileShader(stage, source, { header? })`,
`linkProgram(vs, fs)`, `createShaderTarget(program, w, h, opts)`,
`destroyShader`, `destroyProgram`. The fused `createShader` and
`createPipeline` were deliberately left as-is (decided 2026-07-27): they
predate the split, compile+link inside `ShaderTexture::new`/`new_pipeline`,
and carry the conditional preamble ("inject unless the source starts with
#version") that the raw layer replaced with an explicit `header` opt-in.

## To look into

- **Make the fused paths compositions of the raw layer.** Engine-side this is
  mostly done (`new`/`new_pipeline` are already thin wrappers over
  `from_fragment_program`/`from_pipeline_program`); the JS-visible question
  is whether `createShader`/`createPipeline` should literally be
  compile + link + target over registered ids (visible in gpu resources,
  uniform destruction semantics) instead of an anonymous program dying with
  its target.
- **The conditional preamble.** The raw layer made the header explicit; the
  fused paths still sniff for `#version`. Aligning them (an explicit
  `header`/`raw` option, defaulting to the current behavior) would leave one
  preamble story instead of two.
- **A mid-level program shorthand.** The window effect
  (okf/plans/root-layer-effects.md stage 2) wants a one-liner for "fragment
  program over the standard fullscreen vertex stage". Raw spelling is ~5
  lines incl. writing the covering triangle. Whether that deserves a
  shorthand (a `compileProgram(fragmentSrc)`-style convenience, or a core
  helper) and what it is named was deferred when the raw layer was chosen
  over the earlier mid-level `compileShader(fragmentSrc)` draft.
- **`createShaderMemo`** composes over `createShader` today; if specs get a
  program handle field, the memo's rebuild rule ("new source = new id")
  changes shape.

The fragment-kind program (fullscreen triangle draw path, no mesh state) is
currently only reachable through fused `createShader`; every raw-linked
program is pipeline-kind. A shorthand that produces fragment-kind handles
would make the two kinds symmetric across layers.
