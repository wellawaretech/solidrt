---
type: backlog-item
title: GPU file reorganization
description: shader.rs holds six unrelated concerns at 1466 lines and flux's gpu module lives in a file named texture.rs; split shader.rs into an alloy gpu/ folder (vocab, program, buffer, target, pass), rename the flux plugin file, and lift the RasterCmd enum, capture path and context DTOs.
status: done
timestamp: 2026-07-30T00:00:00Z
---

# GPU file reorganization

The file-split half of [gpu-review](../analysis/gpu-review.md) (final
section), shortlist item 8. Mechanical, no behaviour change; the test
applied there is whether each deferred feature lands in one file or smears
across several.

- **Split `alloy/src/shader.rs`** (1466 lines, six concerns) into a folder
  mirroring `rendertree/`:

      alloy/src/gpu/mod.rs      re-exports
      alloy/src/gpu/vocab.rs    AttrFormat, Topology, BlendMode, DepthState,
                                PipelineDesc, ShaderStage
      alloy/src/gpu/program.rs  compile_stage, link, ShaderProgram, RenderPipeline
      alloy/src/gpu/buffer.rs   GpuBuffer (misfiled today - not a shader)
      alloy/src/gpu/target.rs   ShaderTexture, create_target, create_layer_target
      alloy/src/gpu/pass.rs     run_pass, render_program_to_window/_fbo

  Payoff: vocab.rs is where every extension adds a word (cull, depth-func,
  index format), pass.rs is the file whose exhaustive GL save/restore needs
  review on every change, target.rs is where a draw list or loadOp lands.
  Moving SamplerState/SamplerCache from texture.rs into gpu/sampler.rs is
  optional.
- **Rename `flux/src/plugins/gui/texture.rs` -> `gpu.rs`**: it registers
  the flux:gpu module across five id spaces; the module and file should
  agree. No further split - it is a thin marshalling layer best read whole.
- **`alloy/src/raster.rs`: two narrow lifts only** - the ~250-line
  RasterCmd enum to `raster/cmd.rs` (a protocol definition, what every new
  GPU feature extends first) and the capture/readback path (`rasterize`,
  `rasterize_into`, `flip_for_fbo`) to `raster/capture.rs`. Leave
  frame/present/flush_dirty together: the methods share RasterState fields
  and scattering them hides the state machine.
- **`alloy/src/context.rs`: lift the DTOs** - `GpuResources` + `Gpu*Info`
  to a resources module, `TargetSpec`/`PipelineSpec`/`WindowShader` to a
  spec module. A move, not a redesign.

Do opportunistically or as a standalone mechanical pass; per the
no-formatting-during-implementation rule, not in the middle of a feature.

## Done 2026-07-30

Landed as a standalone mechanical pass, no behaviour change:

- `alloy/src/gpu/` as planned (vocab, program, buffer, target, pass), plus
  `spec.rs` (TargetSpec/PipelineSpec/WindowShader) and `resources.rs`
  (GpuResources + Gpu*Info) for the context DTO lift - the "resources/spec
  modules" live inside gpu/, not beside context.rs. `mod.rs` holds the
  re-exports and the shared prev_* binding-restore helpers. `ParamValue`
  sits in vocab.rs, `PassInput` in pass.rs.
- `raster/` with `cmd.rs` (RasterCmd) and `capture.rs` (rasterize,
  rasterize_into, flip_for_fbo); frame/present/flush_dirty stay in mod.rs.
- flux `gui/texture.rs` renamed to `gui/gpu.rs` (mod.rs + lattice call
  sites updated); forward-looking backlog pointers updated to the new path.
- The optional SamplerState/SamplerCache move to `gpu/sampler.rs` was
  skipped (texture.rs is not big enough to force it, per the analysis).
