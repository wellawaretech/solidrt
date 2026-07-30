---
type: backlog-item
title: Call-site validation for uniforms and draw bounds
description: A typo'd param name is silently dropped at render and an arity mismatch warns on the raster thread where no app can see it, while a draw count past the buffer end is undefined-behaviour vertex fetch; both are validatable synchronously at the JS call site from state the UI thread already mirrors.
status: done
timestamp: 2026-07-30T00:00:00Z
---

Done 2026-07-30. Creates validate raster-side inside their blocking RPCs
(uniform names/arity after compile, draw bounds in `resolve_target_mesh`);
fire-and-forget updates validate UI-side in `Context` against mirrors the
create/link replies populate (`TargetMirror`, `PipelineMirror`,
`program_uniforms`; validators in `gpu/vocab.rs`). Also covered:
`set_window_shader` params/textures, `setShaderTextures` names must be active
sampler2Ds, `setDrawCount` rejects negative and out-of-bounds counts, and the
flux marshal layer throws on non-numeric param/texture values. Decision: an
absent uniform name always throws (strict); the two-tier
declared-but-inactive distinction was deliberately not built - see
[gpu-inactive-uniform-two-tier](gpu-inactive-uniform-two-tier.md). The
`params` prop stays warn-at-build (deferred apply; a call-site throw would
break prop-order independence).

# Call-site validation for uniforms and draw bounds

From [gpu-review](../analysis/gpu-review.md) (lesson 2), shortlist item 5.
Two halves, one principle: everything here is checkable synchronously where
the app made the mistake, which is the design rule the review asks to
protect (errors throw at the call site, not into a log).

## Uniforms: the inherited WebGL flaw

WebGL's worst ergonomic property is that `getUniformLocation` returns null
for a typo and setting it is a silent no-op; WebGPU eliminated the class
structurally. solidrt inherited and softened it: an unknown param name is
dropped silently at render, and a component-count mismatch logs a raster-
thread warning (`apply_uniform`, alloy/src/shader.rs) - neither reaches the
app. The program's reflected uniform table (name -> type) exists at link
time; mirroring it UI-side (the sampler-cycle mirror is precedent) lets
`setShaderParams` / the `params` prop / create-time `params` throw on the
line that wrote the typo. Matches the dev/prod validation policy: sites
throw today.

One wrinkle: an *inactive* uniform (declared but optimized out) reflects as
absent, so "unknown name" must stay a dev-level error, not a hard runtime
guarantee - the same reason WebGL made it silent. Warn-by-default with the
name in the message may be the right strength for that sub-case; a name
that was never declared at all is always a throw.

## Draw bounds: an actual UB hole

WebGL mandates draw-time bounds validation (`drawArrays` past an attribute
buffer's end is INVALID_OPERATION - its one security-driven check that is
also pure ergonomics); raw ES 3.0 leaves out-of-bounds vertex fetch
undefined. solidrt validates `writeBuffer` against the mirrored
`buffer_sizes` but not the draw: an explicit `vertexCount` is taken
verbatim (`resolve_target_mesh`, alloy/src/raster.rs) and `setDrawCount`
checks only that the id is a pipeline texture. The stride is known from the
pipeline desc and the buffer size is mirrored, so `count * stride <= size`
is one more call-site check in the existing style, at `createShaderTarget`
/ `createPipeline` / `setDrawCount`.
