use std::rc::Rc;

use crate::gpu::{vertex_stride, AttributeTable, PipelineDesc, ShaderStage};
use crate::raster::RasterCmd;

use super::mirror::PipelineMirror;
use super::Context;

impl Context {
  /// Compile a single raw shader stage, returning its stage id (its own id
  /// space). The source is complete GLSL ES unless `header` explicitly asks
  /// for the standard header (see `gpu::compile_stage`). Compile errors
  /// surface here, synchronously, at a call site the app chose. Free with
  /// `destroy_shader_stage` - safe right after linking.
  pub fn compile_shader_stage(&self, stage: ShaderStage, source: &str, header: bool) -> Result<u64, String> {
    let id = self.next_stage_id.get();
    self.rpc(|reply| RasterCmd::CompileStage { id, stage, source: source.to_string(), header, reply })??;
    self.next_stage_id.set(id + 1);
    self.stage_kinds.borrow_mut().insert(id, stage);
    Ok(id)
  }

  /// Link a compiled vertex and fragment stage into a shared program,
  /// returning its program id (a separate id space from textures, like
  /// buffers). The program backs any number of targets via
  /// `create_shader_target`, is freed with `destroy_shader_program`, and link
  /// errors surface here, synchronously. The stages remain usable for further
  /// links.
  pub fn link_shader_program(&self, vertex: u64, fragment: u64, label: Option<String>) -> Result<u64, String> {
    let kinds = self.stage_kinds.borrow();
    match kinds.get(&vertex) {
      None => return Err(format!("shader {vertex} not found")),
      Some(ShaderStage::Vertex) => {}
      Some(s) => return Err(format!("shader {vertex} is a {} stage, expected vertex", s.name())),
    }
    match kinds.get(&fragment) {
      None => return Err(format!("shader {fragment} not found")),
      Some(ShaderStage::Fragment) => {}
      Some(s) => return Err(format!("shader {fragment} is a {} stage, expected fragment", s.name())),
    }
    drop(kinds);
    let id = self.next_program_id.get();
    let (uniforms, attributes) = self.rpc(|reply| RasterCmd::LinkProgram { id, vertex, fragment, label, reply })??;
    self.next_program_id.set(id + 1);
    self.program_uniforms.borrow_mut().insert(id, Rc::new(uniforms));
    self.program_attributes.borrow_mut().insert(id, Rc::new(attributes));
    Ok(id)
  }

  /// The active vertex attributes (name, format) of a program from
  /// `link_shader_program`, as the compiler left them: an `in` the vertex
  /// stage never reads is not listed. A pipeline over the program must
  /// declare every one of these (attributes or instanceAttributes).
  pub fn program_attributes(&self, id: u64) -> Result<Rc<AttributeTable>, String> {
    self.program_attributes.borrow().get(&id).cloned().ok_or_else(|| format!("program {id} not found"))
  }

  /// Delete a compiled stage and retire its id. Programs linked from it are
  /// unaffected: a linked program keeps its own compiled copies.
  pub fn destroy_shader_stage(&self, id: u64) {
    self.stage_kinds.borrow_mut().remove(&id);
    self.send(RasterCmd::DestroyStage { id });
  }

  /// Pair a program from `link_shader_program` with draw state, returning the
  /// pipeline id (its own id space, like programs and buffers). The pipeline
  /// is the draw-state object every target created from it shares; creating
  /// one compiles nothing. Free with `destroy_render_pipeline`.
  pub fn create_render_pipeline(&self, program: u64, desc: PipelineDesc, label: Option<String>) -> Result<u64, String> {
    self.gpu_limits().check_vertex_attribs(desc.attributes.len() + desc.instance_attributes.len())?;
    let uniforms = match self.program_uniforms.borrow().get(&program) {
      Some(uniforms) => uniforms.clone(),
      None => return Err(format!("program {program} not found")),
    };
    let stride = vertex_stride(&desc.attributes) as usize;
    let instance_stride = vertex_stride(&desc.instance_attributes) as usize;
    let depth = desc.depth.is_some();
    let id = self.next_pipeline_id.get();
    self.rpc(|reply| RasterCmd::CreateRenderPipeline { id, program, desc, label, reply })??;
    self.next_pipeline_id.set(id + 1);
    self.pipeline_mirrors.borrow_mut().insert(id, PipelineMirror { uniforms, stride, instance_stride, depth });
    Ok(id)
  }

  /// Drop a shared pipeline's registry entry and retire its id. Targets
  /// created from it keep rendering - they hold the pipeline until they are
  /// destroyed - so either destruction order is safe. The program it was
  /// created from is yours and unaffected.
  pub fn destroy_render_pipeline(&self, id: u64) {
    self.pipeline_mirrors.borrow_mut().remove(&id);
    self.send(RasterCmd::DestroyRenderPipeline { id });
  }

  /// Drop a shared program's registry entry and retire its id. Pipelines
  /// created from it keep rendering - they hold the program until they are
  /// destroyed - and the GL program is deleted once the last user is gone, so
  /// either destruction order is safe.
  pub fn destroy_shader_program(&self, id: u64) {
    self.program_uniforms.borrow_mut().remove(&id);
    self.program_attributes.borrow_mut().remove(&id);
    self.send(RasterCmd::DestroyProgram { id });
  }
}
