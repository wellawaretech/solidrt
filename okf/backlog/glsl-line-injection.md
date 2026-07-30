---
type: backlog-item
title: Shader compile errors on .tsx lines via #line injection
description: A shader compile error reports the line inside the string plus the injected preamble (offset 19 in the trails example), leaving the author to hand-count; a bundler pass that injects a #line directive into glsl-tagged template literals would make the driver report the .tsx line itself, closing the last unmapped diagnostic in the dev loop.
status: open
timestamp: 2026-07-30T00:00:00Z
---

# Shader compile errors on .tsx lines via #line injection

The `glsl` tag (landed 2026-07-30 in `@solidrt/core/gpu`, all inline example
shaders tagged) marks GLSL template literals for editor highlighting. The
same tag is a structural marker a bundler transform can key on: inject a
`#line` directive carrying the literal's source line, and the driver's own
info log reports .tsx line numbers instead of string-relative ones.

Today the offset is unrecoverable by inspection: string line, plus the
injected standard header for `{ header: true }` sources (4-5 lines, see
`alloy/src/gpu/program.rs` preambles), plus the literal's position in the
file. In the trails example a fragment error at .tsx line 42 reports as
roughly line 9. Compile errors throw at the compileShader call site already;
this fixes the number inside the message.

## Why no platform gate

`#line` is core GLSL ES 3.00 preprocessor syntax: a conformant compiler must
accept it, and shader toolchains (glslify, shaderc) emit it routinely. The
only per-driver freedom is whether the info log honors it in diagnostics. A
driver that ignores it degrades to exactly today's behavior, so nothing can
break on other platforms; value on the dev machine (Mesa) is the whole bar.
No ANGLE or device testing needed.

## Plan

1. Calibration probe first: an alloy example (release build, one run) that
   compiles a deliberately broken shader in four variants - with/without
   `#line`, raw `#version` source and `header: true` - and prints the
   reported line for each. This answers whether Mesa honors the directive in
   its log (if not, the feature has no value here and dies) and measures the
   off-by-one convention (whether the line after `#line 25` reports as 25 or
   26 - the ES and desktop specs have worded this differently, do not trust
   memory).
2. Babel plugin in `packages/cli/src/bundler.ts` next to `inlineImport`:
   visitor on TaggedTemplateExpression with tag identifier `glsl`, editing
   the first quasi (both `raw` and `cooked`) using `node.loc`. Babel runs
   per-file before bundling, so `loc` is true .tsx lines with no sourcemap
   involvement.

## Decisions and caveats

- The `#version` asymmetry: nothing may precede `#version`, so for a source
  that declares its own version line the directive goes after it with the
  number bumped; for header-injected sources it can lead. The plugin encodes
  both.
- Wrong numbers are worse than none: if the driver honors `#line` and our
  arithmetic is off, errors point at wrong .tsx lines, worse than the known
  constant offset. That is what the probe's calibration is for.
- Numbering is only guaranteed up to the first interpolation that splices in
  newlines. No current shader interpolates at all; document, do not solve.
- Untagged sources get no injection: tag adoption is the opt-in.
