---
type: analysis
title: GPU stack maturity — readiness for 3D games
timestamp: 2026-07-15T00:00:00Z
---

# GPU stack maturity — readiness for 3D games

Assessment of solidrt-open's GPU stack as of 2026-07-15, the day
vertex-shader pipelines shipped. Written from the Doom port
(`~/solidrt/projects/doom`), which is the stack's most demanding consumer and
its de-facto acceptance test.

## Summary

The GPU stack just crossed the line from "shader toy" to minimal but genuinely
usable 3D pipeline: custom vertex + fragment GLSL, interleaved vertex buffers
with offset writes, depth buffering, and sampler inputs — enough that the Doom
port retired its fragment-only raycaster for a real mesh renderer the same day
(walls, flats, sky, animated doors, depth-occluded sprites, 60fps, jsMs ~0.3).
Games in the Doom/PS1/stylized-retro class are feasible **now**. A
modern-style 3D game (translucency, large meshes, post-processing, shadows)
immediately hits the deferred-feature list in
[backlog/gpu-pipeline-extensions.md](../backlog/gpu-pipeline-extensions.md),
and any first-person game hits the lack of relative mouse input before it hits
any GPU gap.

## What exists

- **Fragment shaders** (`createShader`): GLSL ES 3.00 fullscreen passes
  rendered into FBO-backed RGBA8 textures adopted into Impeller — a shader
  output is just a texture id usable anywhere in the UI tree
  (`<texture src={id} params={{...}}/>`). Uniforms reflected by name at link
  time; `iResolution`/`iTime` free; `sampler2D` inputs bind other textures
  (including live camera feeds) and re-sample on every update.
- **Vertex pipelines** (`createPipeline`, `packages/core/src/gpu.ts` backed by
  `alloy/src/shader.rs`): custom vertex + fragment GLSL, one interleaved float
  vertex buffer per pipeline with name-resolved attributes
  (`f32|vec2|vec3|vec4`), five topologies, optional private
  `DEPTH_COMPONENT24` depth buffer, clear color, mutable draw count.
- **Buffers**: `createBuffer` / `writeBuffer(id, data, byteOffset)` /
  `destroyBuffer`, DYNAMIC_DRAW. A buffer write auto-re-renders every pipeline
  drawing from it; `uploadTexture` has the same contract for mutable textures.
  That trio (offset writes + draw count + auto re-render) covers dynamic
  geometry: Doom's sprites live in a dynamic tail region of the single
  level-mesh buffer.
- **Engineering quality**: the GL state save/restore in
  `ShaderTexture::render` is exhaustive (color mask, stencil, rasterizer
  discard, sample coverage, depth range, polygon offset...) because shader
  passes share a context with Impeller's cached state — battle-hardened by a
  real bug (Impeller's depth-clear-to-0.0 convention inverting depth tests
  after snapshot captures). `glGetError` after every pass logs future state
  leaks instead of drawing black. The reactive layer auto-frees GPU resources
  on owner disposal. Each feature shipped with a real-client acceptance test.
- **Surrounding game infrastructure**: `onFrame` with refresh rate, keyboard
  events, gamepad, audio (WAV/Ogg), camera/microphone, MCP dev tooling
  (snapshots, logs, stats).

## What's missing

Items 1–5 match [backlog/gpu-pipeline-extensions.md](../backlog/gpu-pipeline-extensions.md);
the rest are gaps that file doesn't cover.

1. **Typed uniforms (vecN/mat4)** — params are float scalars end to end. Doom
   rebuilds its camera matrix *from scalars inside the vertex shader*. The
   most annoying daily-driver gap; first on the backlog's own list.
2. **Index buffers** — everything is unindexed `drawArrays`. Fine at E1M1
   scale (~8.6k verts), wasteful beyond it.
3. **Blending control** — blend state is force-disabled during pipeline
   passes. Alpha-tested cutouts work via `discard` (Doom's window grates), but
   true translucency (spectres, glass, particles) is impossible.
4. **Multi-pass / shared render targets** — one program, one buffer, one draw
   call per target. No world-pipeline + sprite-pipeline sharing a depth
   buffer; Doom's single-buffer-with-dynamic-tail trick covers it but doesn't
   generalize.
5. **Texture formats and sampling** — RGBA8 only; LINEAR + CLAMP_TO_EDGE
   hardcoded; no mipmaps, REPEAT, or NEAREST. Doom fixed-point-encodes 16-bit
   sector heights across two RGBA8 channels and tiles textures manually in the
   shader. No mipmaps means distant surfaces alias.
6. **No instancing, compute, MRT, stencil access, cube maps, or MSAA on
   targets** — rules out most modern techniques (shadow maps only via painful
   depth-encoding color passes; GPU particles, post-processing chains, and
   skyboxes all need workarounds).
7. **Backend: GL only.** Vulkan and Metal are `unimplemented!()` panics in
   `alloy/src/backend.rs`. GLES 3.0 is a sensible lowest common denominator,
   but platform reach for games currently equals the GL backend's reach.
8. **No mouse look.** Pointer events are UI-style absolute coordinates; no
   pointer-lock / relative-motion API exists in core or flux-types. The Doom
   port turns with arrow keys. For first-person games this is arguably the
   biggest gap in the whole stack, and it's an SDL capability away.
9. **No 3D scaffolding above the pipeline** — no math library (mat4 by hand),
   no model loading, no culling, no 3D scene graph. Defensible scope for a UI
   framework exposing GPU primitives, but every game rebuilds this layer
   (Doom's `mesh.ts` is 551 lines of geometry code).
10. **QuickJS has no JIT**, so the architecture requires keeping per-pixel and
    per-vertex work on the GPU. Respected beautifully by the Doom port
    (jsMs ~0.3/frame), but CPU-heavy games (physics, large AI populations)
    would feel it.

## Verdict

**Early but disciplined** — staged minimalism: the implemented surface is
small, correct, and verified on real clients; the unimplemented surface is
explicitly catalogued rather than half-built. Next-tier 3D needs the backlog's
top four items (typed uniforms, index buffers, blending, multi-pass); any
first-person game needs relative mouse input before it needs any of them.
