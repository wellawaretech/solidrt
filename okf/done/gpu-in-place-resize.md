---
title: In-place GPU resize
description: Resize data textures and shader targets at a stable id so texture references, sampler bindings and owner-scoped auto-free survive; shipped, no GL-level test coverage.
created: 2026-07-23
completed: 2026-07-30
---

# In-place GPU resize

Shipped 2026-07-23. Motivated by the linux-terminal project's window-resize
feature: every GPU object baked its size at creation, so resizing the cell
grid meant rebuilding three data textures plus the shader, swapping the new
shader id into `<texture src>` via a signal, and hand-managing disposal of the
old set (which also forced the app off @solidrt/core's auto-freeing gpu
helpers onto raw flux:gpu, and opened a window where the render tree briefly
referenced a destroyed shader id).

The fix makes ids stable across a resize, so nothing downstream needs to
change or be re-registered:

- `resizeTexture(id, data, width, height)` (flux:gpu, re-exported by core) -
  replaces a pixel texture's storage at the same id. Built on the registry's
  existing replace-at-id path (`create_texture_at`, generation bump rebuilds
  retained display lists); shaders sampling the id re-render, same contract
  as uploadTexture. Rejects shader-target ids.
- `setShaderSize(id, width, height)` - the setDrawCount analog for output
  size: `ShaderTexture::resize` puts a new target texture on the existing
  FBO/program (depth renderbuffer re-allocated for pipelines), re-renders
  with last-applied params and re-resolved sampler bindings, re-adopts into
  Impeller, and the UI side re-registers under the same id. The old target's
  GL name stays owned by the old Impeller handle (adoption ownership), so
  in-flight display lists keep it alive until they drop - no destroy race.

Touched: alloy shader.rs (`ShaderTexture::resize`), raster.rs
(`ResizeShaderTexture` cmd, `resize_shader_texture`, dependent re-render on
replace-at-id via `rerender_samplers_of`), context.rs (`resize_texture`,
`resize_shader_texture`), flux gui/gpu.rs bindings, flux-types gpu.d.ts,
core gpu.ts re-exports, docs/core.md.

Verified 2026-07-23 against a locally built client: the linux-terminal
project (external, ~/solidrt/projects/linux) rewritten onto the new API -
resize effect collapsed to resizeTexture x3 + screen.resize + setShaderSize +
repaint, deleting the shaderId signal, manual destroys, and the raw flux:gpu
import split. Live drag-resize (shrink + widen) kept the same texture node
and shader id, preserved visible content, and produced zero runtime errors.

Remaining / follow-ups:

- No GL-level automated test exercises resize (the alloy suite has no GL
  context harness); coverage is the manual client run above.
- Related work, all shipped 2026-07-23: [[gpu-deferred-texture-destroy]]
  (frame-safe destroy for the cases where rebuild-and-swap is still the right
  shape), [[gpu-reactive-resource-helpers]] (owner-independent reactive
  lifetimes), [[gpu-sampler-rebinding]] (setShaderTextures).
