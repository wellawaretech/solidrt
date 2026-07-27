---
type: backlog-item
title: setShaderTextures - rebind sampler inputs
description: setShaderTextures rebinds sampler2D inputs on a live shader, enabling retargeting and ping-pong without recompiling; shipped ahead of a real use case.
status: done
timestamp: 2026-07-23T00:00:00Z
---

# setShaderTextures - rebind sampler inputs

Shipped 2026-07-23 (originally demand-gated after [[gpu-in-place-resize]]
solved the motivating resize case; implemented anyway while completing the
GPU-lifetime series, since it is small and completes the symmetry: float
uniforms had setShaderParams, sampler2D bindings were fixed at create time).

`setShaderTextures(id, { samplerName: textureId })` (flux:gpu, re-exported by
core) retargets a live shader/pipeline's sampler inputs without recompiling -
post-process source swap, ping-pong between two data textures. Shaped exactly
like UpdateShaderParams as predicted: mutate, then re-render with
last-applied params and re-resolved bindings.

- alloy shader.rs `ShaderTexture::set_sampler_bindings`: validates every name
  against the program's active uniforms before changing anything (failed call
  leaves all bindings intact); named bindings update, unnamed keep their
  source, and a declared-but-unbound sampler can be bound late (push).
- raster.rs `UpdateShaderTextures` cmd (fire-and-forget, warn on error, like
  UpdateShaderParams).
- context.rs `update_shader_textures`: UI-side validation that throws to JS -
  unknown shader id, unknown source texture id, and self-binding (a sampler
  sourcing the shader's own target: a GL feedback loop) are all rejected.
- flux gui/texture.rs binding + request_frame, flux-types gpu.d.ts, core
  gpu.ts re-export, docs/core.md.

Not live-verified: no app uses it yet (it shipped ahead of a real use case);
compile-checked and covered by the same manual-client caveat as the rest of
the series (no GL-level automated test harness).
