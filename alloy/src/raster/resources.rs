//! The plain resource registries and their inventory: pixel textures and
//! uploads, vertex buffer writes, program links and render pipelines, the
//! retired-timer harvest, and the Resources RPC's full inventory.

use std::sync::atomic::Ordering;

use impellers::{ISize, Texture};

use super::{RasterState};
use crate::gl;
use crate::gl::{GpuTexture, RenderPipeline, ShaderProgram, Timed};
use crate::gpu::{
  AttributeTable, GpuBufferInfo, GpuPipelineInfo, GpuProgramInfo, GpuRegionInfo, GpuRenderPipelineInfo, GpuResources,
  GpuTextureInfo, GpuWindowShaderInfo, PipelineDesc, SamplerState, TextureFormat, UniformTable,
};
use std::rc::Rc;

impl RasterState {
  pub(super) fn create_texture(
    &mut self,
    id: u64,
    width: u32,
    height: u32,
    pixels: &[u8],
    sampler: SamplerState,
    format: TextureFormat,
    label: Option<String>,
  ) -> Result<Texture, String> {
    let size = ISize::new(width as i64, height as i64);
    let mut gpu = GpuTexture::new(&self.gl, size, sampler, format);
    // A replace-at-id with no new label is an id-stable resize: labels are
    // create-time state and follow the id through it.
    gpu.label = label.or_else(|| self.textures.get(&id).and_then(|old| old.label.clone()));
    gpu.upload(&self.gl, pixels, size);
    match gl::adopt_texture(&gpu, &self.impeller_ctx, size) {
      Some(impeller) => {
        let replaced = self.textures.insert(id, gpu).is_some();
        // Replacing at an existing id (an id-stable resize): same contract as
        // UpdateTexture - shaders sampling this id re-render at the next
        // flush so they pick up the new texture without a params change.
        if replaced {
          self.dirty.insert(id);
        }
        Ok(impeller)
      }
      None => {
        // Adoption never took ownership, so the name is still ours to free.
        unsafe { glow::HasContext::delete_texture(&self.gl, gpu.gl_texture) };
        Err("adopt texture failed".to_string())
      }
    }
  }

  pub(super) fn update_texture(&mut self, id: u64, pixels: &[u8]) -> Result<(), String> {
    let gpu = self.textures.get(&id).ok_or_else(|| format!("texture {id} not found"))?;
    let expected = gpu.format.byte_len(gpu.width, gpu.height);
    if pixels.len() != expected {
      return Err(format!(
        "texture {} update is {} bytes, expected {expected} ({})",
        describe(id, &gpu.label),
        pixels.len(),
        gpu.format.name()
      ));
    }
    let size = ISize::new(gpu.width as i64, gpu.height as i64);
    gpu.upload(&self.gl, pixels, size);
    // Shader targets sampling this texture re-render at the next flush, so
    // data-texture changes are visible without a params change.
    self.dirty.insert(id);
    Ok(())
  }

  /// Link two compiled stages from the stage registry into a registered
  /// program, replying with the reflected uniform and attribute tables for
  /// the UI-side validation mirror. The UI side validated the ids and stage kinds against
  /// its mirror; a miss here means the mirrors diverged.
  pub(super) fn link_program(
    &mut self,
    id: u64,
    vertex: u64,
    fragment: u64,
    label: Option<String>,
  ) -> Result<(UniformTable, AttributeTable), String> {
    let vs = self.stages.get(&vertex).ok_or_else(|| format!("shader {vertex} not found"))?;
    let fs = self.stages.get(&fragment).ok_or_else(|| format!("shader {fragment} not found"))?;
    let program = ShaderProgram::from_stages(&self.gl, vs, fs)?.with_label(label);
    let tables = (program.uniform_table(), program.attribute_table());
    self.programs.insert(id, Rc::new(program));
    Ok(tables)
  }

  /// Pair a registered program with draw state under pipeline id `id`.
  pub(super) fn create_render_pipeline(
    &mut self,
    id: u64,
    program_id: u64,
    desc: PipelineDesc,
    label: Option<String>,
  ) -> Result<(), String> {
    let program = self.programs.get(&program_id).ok_or_else(|| format!("program {program_id} not found"))?.clone();
    let pipeline = RenderPipeline::new(program, Some(program_id), desc).map_err(|(_, e)| e)?.with_label(label);
    self.render_pipelines.insert(id, Rc::new(pipeline));
    Ok(())
  }

  /// Drain retired timer queries into the cumulative GPU-side counters
  /// (see gpu::PassTimer). A target destroyed before its result retired is
  /// simply not credited; the total still is.
  pub(super) fn harvest_pass_timings(&mut self) {
    for exec in self.pass_timer.poll(&self.gl) {
      match exec.what {
        Timed::Pass { target } => {
          self.stats.pass_exec_micros.fetch_add(exec.micros, Ordering::Relaxed);
          if let Some(shader) = self.shaders.get(&target) {
            shader.record_exec(exec.micros);
          }
        }
        Timed::Frame => {
          self.stats.frame_exec_micros.fetch_add(exec.micros, Ordering::Relaxed);
        }
      }
    }
  }

  pub(super) fn write_buffer(&mut self, id: u64, data: &[u8], byte_offset: usize) -> Result<(), String> {
    let buffer = self.buffers.get(&id).ok_or_else(|| format!("buffer {id} not found"))?;
    buffer.write(&self.gl, data, byte_offset).map_err(|e| format!("buffer {}: {e}", describe(id, &buffer.label)))?;
    // Every pipeline drawing from this buffer re-renders at the next flush,
    // so geometry-only changes reach the screen even when no new params
    // arrive. (Marked by target id: buffer ids are their own space.) Manual
    // targets pick the new geometry up at their next explicit render.
    let drawing: Vec<u64> =
      self.shaders.iter().filter(|(_, s)| !s.manual() && s.reads_buffer(id)).map(|(tid, _)| *tid).collect();
    self.dirty.extend(drawing);
    Ok(())
  }

  /// `7 (bloom-h)` when texture id 7 carries a label, else `7`: how raster
  /// messages name a texture - the id stays the cross-reference key, the
  /// label the human name.
  pub(super) fn texture_desc(&self, id: u64) -> String {
    describe(id, &self.textures.get(&id).and_then(|t| t.label.clone()))
  }

  /// Inventory the GPU resources this thread tracks: registered textures,
  /// vertex buffers, and shader/pipeline targets with their bookkeeping.
  /// Sorted by id for stable output.
  pub(super) fn resources(&self) -> GpuResources {
    let mut textures: Vec<GpuTextureInfo> = self
      .textures
      .iter()
      .map(|(id, gpu)| GpuTextureInfo {
        id: *id,
        width: gpu.width,
        height: gpu.height,
        target: self.shaders.contains_key(id),
        format: gpu.format.name(),
        sampler: gpu.sampler,
        label: gpu.label.clone(),
      })
      .collect();
    textures.sort_by_key(|t| t.id);

    let mut buffers: Vec<GpuBufferInfo> = self
      .buffers
      .iter()
      .map(|(id, b)| GpuBufferInfo { id: *id, byte_length: b.size, label: b.label.clone() })
      .collect();
    buffers.sort_by_key(|b| b.id);

    let mut pipelines: Vec<GpuPipelineInfo> = self
      .shaders
      .iter()
      .map(|(texture_id, shader)| {
        let (passes, pass_issue_micros, pass_exec_micros) = shader.pass_stats();
        // A draw target reports its entries in the `draws` list; the flat
        // single-pass fields stay for the fixed kinds, where they describe
        // the one pass - read off its first (only) entry's record.
        let flat = !shader.is_draw_list();
        let entry0 = if flat { shader.entry0_info() } else { None };
        let (width, height) = shader.size();
        let region = shader.region().map(|r| GpuRegionInfo { parent: r.parent, x: r.x, y: r.y, width, height });
        GpuPipelineInfo {
          texture_id: *texture_id,
          label: self
            .textures
            .get(texture_id)
            .and_then(|t| t.label.clone())
            .or_else(|| shader.region().and_then(|r| r.label.clone())),
          region,
          kind: if shader.is_draw_list() {
            "draws"
          } else if shader.is_pipeline() {
            "pipeline"
          } else {
            "fragment"
          },
          program_id: if flat { shader.program_id() } else { None },
          pipeline_id: entry0.as_ref().and_then(|e| e.pipeline_id),
          buffer_id: entry0.as_ref().and_then(|e| e.buffer_id),
          index_buffer_id: entry0.as_ref().and_then(|e| e.index_buffer_id),
          index_format: entry0.as_ref().and_then(|e| e.index_format),
          instance_buffer_ids: entry0.as_ref().map(|e| e.instance_buffer_ids.clone()).unwrap_or_default(),
          topology: entry0.as_ref().map(|e| e.topology),
          draw_count: entry0.as_ref().map(|e| e.vertex_count),
          first_vertex: entry0.as_ref().map(|e| e.first_vertex),
          instance_count: entry0.as_ref().map(|e| e.instance_count),
          depth: shader.has_depth(),
          samples: shader.samples(),
          depth_write: entry0.as_ref().map(|e| e.depth_write),
          blend: entry0.as_ref().map(|e| e.blend),
          cull: entry0.as_ref().map(|e| e.cull),
          attributes: if flat {
            shader.attributes().iter().map(|(name, fmt)| (name.clone(), fmt.name().to_string())).collect()
          } else {
            Vec::new()
          },
          instance_attributes: if flat {
            shader
              .instance_attributes()
              .iter()
              .map(|(name, fmt, slot)| (name.clone(), fmt.name().to_string(), *slot))
              .collect()
          } else {
            Vec::new()
          },
          // For a draw target the flat textures/params fields carry its
          // shared (target-level) bindings and params; the per-entry ones
          // live in `draws`.
          textures: if flat { shader.sampler_bindings().to_vec() } else { shader.shared_bindings().to_vec() },
          params: if flat { shader.last_params() } else { shader.shared_params().to_vec() },
          draws: if flat { Vec::new() } else { shader.draw_infos() },
          manual: shader.manual(),
          load: shader.load(),
          passes,
          pass_issue_micros,
          pass_exec_micros,
        }
      })
      .collect();
    pipelines.sort_by_key(|p| p.texture_id);

    let mut render_pipelines: Vec<GpuRenderPipelineInfo> = self
      .render_pipelines
      .iter()
      .map(|(id, pipeline)| {
        let desc = pipeline.desc();
        GpuRenderPipelineInfo {
          id: *id,
          program_id: pipeline.program_id().unwrap_or(0),
          label: pipeline.label().map(str::to_string),
          topology: desc.topology.name(),
          blend: crate::gpu::blend_name(desc.blend),
          cull: crate::gpu::cull_name(desc.cull),
          depth: desc.depth.is_some(),
          depth_write: desc.depth.map_or(true, |d| d.write),
          attributes: desc.attributes.iter().map(|(name, fmt)| (name.clone(), fmt.name().to_string())).collect(),
          instance_attributes: desc
            .instance_attributes
            .iter()
            .map(|(name, fmt, slot)| (name.clone(), fmt.name().to_string(), *slot))
            .collect(),
        }
      })
      .collect();
    render_pipelines.sort_by_key(|p| p.id);

    let mut programs: Vec<GpuProgramInfo> =
      self.programs.iter().map(|(id, p)| GpuProgramInfo { id: *id, label: p.label().map(str::to_string) }).collect();
    programs.sort_by_key(|p| p.id);

    let window_shader = self.window_shader.as_ref().map(|state| GpuWindowShaderInfo {
      program_id: state.spec.program,
      width: state.layer.as_ref().map_or(0, |l| l.width),
      height: state.layer.as_ref().map_or(0, |l| l.height),
      previous: state.spec.previous && state.prev_layer.is_some(),
      pass_only_frames: self.pass_only_frames,
    });

    GpuResources { textures, buffers, pipelines, render_pipelines, programs, window_shader }
  }
}

/// `7 (bloom-h)` with a label, `7` without: the one spelling for a labeled id
/// in raster-side messages.
fn describe(id: u64, label: &Option<String>) -> String {
  match label {
    Some(label) => format!("{id} ({label})"),
    None => id.to_string(),
  }
}
