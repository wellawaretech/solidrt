use std::collections::HashMap;
use std::rc::Rc;

use crate::gpu::{
  resolve_draw_range, validate_draw_range, validate_order, validate_param_if_declared, validate_params,
  validate_texture_bindings, vertex_stride, BufferIds, DrawBounds, DrawSpec, DrawUpdate, ParamValue, PipelineSpec,
  SamplerState, TargetSpec, TextureBinding, TextureEntry, TextureFormat, UniformKind, UniformTable, WindowShader,
};
use crate::raster::RasterCmd;

use super::content::samples_transitively;
use super::mirror::{entry_mirror, DrawListMirror, EntryMirror, TargetMirror};
use super::Context;

impl Context {
  /// Compile a GLSL ES fragment shader into a new RGBA8 target texture and
  /// register the output in the texture registry. Returns the id the output
  /// is sampleable under (usable anywhere a normal texture id is); the first
  /// render happens at the raster thread's next dirty flush, before anything
  /// observes the pixels. The compiled program is retained so
  /// set_target_params can re-render the same texture without recompiling
  /// or re-adopting.
  #[allow(clippy::too_many_arguments)]
  pub fn create_shader_texture(
    &self,
    width: u32,
    height: u32,
    fragment_src: &str,
    params: &[(String, ParamValue)],
    textures: &[TextureBinding],
    sampler: SamplerState,
    label: Option<String>,
  ) -> Result<u64, String> {
    let limits = self.gpu_limits();
    limits.check_texture_size(width, height)?;
    limits.check_texture_units(textures.len())?;
    let id = self.textures.allocate_id();
    let (impeller, uniforms) = self.rpc(|reply| RasterCmd::CreateShaderTexture {
      id,
      width,
      height,
      fragment_src: fragment_src.to_string(),
      params: params.to_vec(),
      textures: textures.to_vec(),
      sampler,
      label,
      reply,
    })??;
    self.textures.insert(id, TextureEntry { impeller, width, height, sampler, format: TextureFormat::Rgba8 });
    self.targets.borrow_mut().insert(
      id,
      TargetMirror {
        uniforms: Rc::new(uniforms),
        draw: None,
        bounds: DrawBounds::default(),
        buffers: BufferIds::default(),
        entries: None,
      },
    );
    self.shader_sources.borrow_mut().insert(id, textures.iter().map(|b| ((0, b.name.clone()), b.id)).collect());
    Ok(id)
  }

  /// The binding checks shared by every path that adds sampler edges to
  /// target `id` (pass `entry` 0 for the fixed kinds' single pass): the
  /// merged per-pass count must fit the device's texture units, every source
  /// must exist, and no binding may close a flush-rendered sampling cycle.
  fn validate_new_bindings(&self, id: u64, entry: u64, textures: &[TextureBinding]) -> Result<(), String> {
    let limits = self.gpu_limits();
    let sources = self.shader_sources.borrow();
    let manual = self.manual_targets.borrow();
    // The rebind merges into the entry's existing bindings, so it is the
    // merged count that must fit the device's texture units - per entry,
    // because units rebind per draw.
    let current = sources.get(&id);
    let current_count = current.map_or(0, |c| c.keys().filter(|(e, _)| *e == entry).count());
    let added = textures.iter().filter(|b| current.is_none_or(|c| !c.contains_key(&(entry, b.name.clone())))).count();
    limits.check_texture_units(current_count + added)?;
    for TextureBinding { name, id: src_id, .. } in textures {
      if self.textures.get(*src_id).is_none() {
        return Err(format!("texture {src_id} (sampler '{name}') not found"));
      }
      if *src_id == id {
        return Err(format!("sampler '{name}' binds shader texture {id} to its own target (same-pass feedback)"));
      }
      // The flush-rendered subgraph is acyclic, and this call only changes
      // `id`'s own outgoing edges, so any new all-pure cycle runs through
      // one of the updated bindings: per binding, reject if the target can
      // already be reached from the new source without passing through a
      // manual target. A manual `id` needs no walk at all - every cycle
      // through it has a manual member (its direct self-bind was rejected
      // above). The walk never needs `id`'s own edges (it stops on reaching
      // `id`), so the pre-update graph is the right one.
      if !manual.contains(&id) && samples_transitively(&sources, &manual, *src_id, id) {
        return Err(format!("sampler '{name}' would create a sampling cycle back to shader texture {id}"));
      }
    }
    Ok(())
  }

  /// Compile a vertex+fragment pipeline, render it once into a new RGBA8
  /// target texture, and register the output exactly like
  /// `create_shader_texture` (same id space; `set_target_params`,
  /// `destroy_texture`, and `<texture src>` all apply). The fused convenience
  /// over `create_render_pipeline` + `create_shader_target`; the anonymous
  /// program and pipeline die with the target.
  pub fn create_pipeline_texture(&self, mut spec: PipelineSpec) -> Result<u64, String> {
    let limits = self.gpu_limits();
    limits.check_texture_size(spec.target.width, spec.target.height)?;
    limits.check_texture_units(spec.entry.textures.len())?;
    limits.check_vertex_attribs(spec.pipeline.attributes.len() + spec.pipeline.instance_attributes.len())?;
    validate_load(&spec.target)?;
    let stride = vertex_stride(&spec.pipeline.attributes) as usize;
    let instance_stride = vertex_stride(&spec.pipeline.instance_attributes) as usize;
    let bounds = self.resolve_entry_range(&mut spec.entry, stride, instance_stride)?;
    let id = self.textures.allocate_id();
    let (width, height, sampler) = (spec.target.width, spec.target.height, spec.target.sampler);
    let manual = spec.target.manual;
    let draw = spec.entry.draw;
    let buffers = spec.entry.buffer_ids();
    let sources: HashMap<(u64, String), u64> =
      spec.entry.textures.iter().map(|b| ((0, b.name.clone()), b.id)).collect();
    let (impeller, uniforms) = self.rpc(|reply| RasterCmd::CreatePipelineTexture { id, spec, reply })??;
    self.textures.insert(id, TextureEntry { impeller, width, height, sampler, format: TextureFormat::Rgba8 });
    self
      .targets
      .borrow_mut()
      .insert(id, TargetMirror { uniforms: Rc::new(uniforms), draw: Some(draw), bounds, buffers, entries: None });
    self.shader_sources.borrow_mut().insert(id, sources);
    if manual {
      self.manual_targets.borrow_mut().insert(id);
    }
    Ok(id)
  }

  /// Check an entry's buffers against the pipeline's declared layouts and
  /// resolve its draw range in place, capturing the bounds for the entry's
  /// mirror: the fetch bound against the vertex buffer at the pipeline's
  /// stride for a plain entry, against the index buffer at the format's
  /// element size for an indexed one - whose vertex fetch runs through the
  /// index VALUES and so cannot be bounds-checked here (raw GL semantics;
  /// robust drivers clamp) - and the instance bound against the instance
  /// buffer at the per-instance record stride. Shared by the two split
  /// creates, the fused create, and add_draw.
  fn resolve_entry_range(
    &self,
    entry: &mut DrawSpec,
    stride: usize,
    instance_stride: usize,
  ) -> Result<DrawBounds, String> {
    let size = self.buffer_size(entry.buffer)?;
    if stride > 0 && size.is_none() {
      return Err("pipeline declares attributes but no vertex buffer".to_string());
    }
    let instance_size = match entry.instance_buffer {
      0 => None,
      id => {
        Some(self.buffer_sizes.borrow().get(&id).copied().ok_or_else(|| format!("instance buffer {id} not found"))?)
      }
    };
    if instance_stride > 0 && instance_size.is_none() {
      return Err("pipeline declares instanceAttributes but no instance buffer".to_string());
    }
    if instance_stride == 0 && instance_size.is_some() {
      return Err("pipeline declares no instanceAttributes; the instance buffer would never be read".to_string());
    }
    let (fetch, indexed) = match entry.index {
      Some((index_buffer, format)) => {
        let bytes = match index_buffer {
          0 => None,
          id => self.buffer_sizes.borrow().get(&id).copied(),
        }
        .ok_or_else(|| format!("index buffer {index_buffer} not found"))?;
        (Some((format.size() as usize, bytes)), true)
      }
      None => (size.filter(|_| stride > 0).map(|size| (stride, size)), false),
    };
    let bounds = DrawBounds { fetch, indexed, instance: instance_size.map(|size| (instance_stride, size)) };
    entry.draw = resolve_draw_range(entry.draw, bounds)?;
    Ok(bounds)
  }

  /// Create a render target over a pipeline from `create_render_pipeline` and
  /// register the output exactly like `create_shader_texture` (same texture
  /// id space: params updates, `resize_target`, `<texture src>` and
  /// `destroy_texture` all apply). Many targets may share one pipeline, and
  /// creating a target compiles nothing.
  pub fn create_shader_target(&self, pipeline: u64, spec: TargetSpec, mut entry: DrawSpec) -> Result<u64, String> {
    let limits = self.gpu_limits();
    limits.check_texture_size(spec.width, spec.height)?;
    limits.check_texture_units(entry.textures.len())?;
    let (uniforms, stride, instance_stride) = match self.pipeline_mirrors.borrow().get(&pipeline) {
      Some(mirror) => (mirror.uniforms.clone(), mirror.stride, mirror.instance_stride),
      None => return Err(format!("pipeline {pipeline} not found")),
    };
    validate_load(&spec)?;
    entry.pipeline = pipeline;
    let bounds = self.resolve_entry_range(&mut entry, stride, instance_stride)?;
    let id = self.textures.allocate_id();
    let (width, height, sampler) = (spec.width, spec.height, spec.sampler);
    let manual = spec.manual;
    let draw = entry.draw;
    let buffers = entry.buffer_ids();
    let sources: HashMap<(u64, String), u64> = entry.textures.iter().map(|b| ((0, b.name.clone()), b.id)).collect();
    let impeller = self.rpc(|reply| RasterCmd::CreateShaderTarget { id, spec, entry, reply })??;
    self.textures.insert(id, TextureEntry { impeller, width, height, sampler, format: TextureFormat::Rgba8 });
    self.targets.borrow_mut().insert(id, TargetMirror { uniforms, draw: Some(draw), bounds, buffers, entries: None });
    self.shader_sources.borrow_mut().insert(id, sources);
    if manual {
      self.manual_targets.borrow_mut().insert(id);
    }
    Ok(id)
  }

  /// Create a draw target: a render target whose contents are an ordered,
  /// mutable list of draws (see `add_draw`/`remove_draw`), over color storage
  /// plus optional target-owned `depth` storage shared by every entry. The
  /// output registers exactly like every shader target (same texture id
  /// space; `<texture src>`, resize, and destroy all apply). With no entries
  /// a render is the clear alone. Entry order is draw order; the purity
  /// contract is unchanged - the list is input data, so a flush-rendered
  /// draw target re-renders whenever its entries or their inputs change.
  pub fn create_draw_target(&self, spec: TargetSpec, depth: bool) -> Result<u64, String> {
    self.gpu_limits().check_texture_size(spec.width, spec.height)?;
    validate_load(&spec)?;
    let id = self.textures.allocate_id();
    let (width, height, sampler) = (spec.width, spec.height, spec.sampler);
    let manual = spec.manual;
    let impeller = self.rpc(|reply| RasterCmd::CreateDrawTarget { id, spec, depth, reply })??;
    self.textures.insert(id, TextureEntry { impeller, width, height, sampler, format: TextureFormat::Rgba8 });
    self.targets.borrow_mut().insert(
      id,
      TargetMirror {
        uniforms: Rc::new(UniformTable::default()),
        draw: None,
        bounds: DrawBounds::default(),
        buffers: BufferIds::default(),
        entries: Some(DrawListMirror { depth, next_draw: 1, entries: HashMap::new() }),
      },
    );
    self.shader_sources.borrow_mut().insert(id, HashMap::new());
    if manual {
      self.manual_targets.borrow_mut().insert(id);
    }
    Ok(id)
  }

  /// Add a draw entry to a draw target: `entry.pipeline` draws
  /// `entry.buffer` over the target's shared storage, with its own params
  /// and sampler inputs - appended (drawing last in list order), or
  /// inserted immediately before entry `before` when given. Returns the
  /// entry's stable draw id (target-scoped, never reused), the handle every
  /// per-entry update takes. Fire-and-forget after validation: everything is
  /// checked here against the mirrors - unknown ids, depth compatibility (a
  /// depth-testing pipeline needs a target created with depth), draw-range
  /// bounds, uniform names and arities, per-entry texture-unit count, and
  /// sampling cycles - so errors throw at the call site. The caller must
  /// request a frame.
  pub fn add_draw(&self, target: u64, mut entry: DrawSpec, before: Option<u64>) -> Result<u64, String> {
    let mut targets = self.targets.borrow_mut();
    let mirror = targets.get_mut(&target).ok_or_else(|| format!("shader texture {target} not found"))?;
    let Some(list) = mirror.entries.as_mut() else {
      return Err(format!("target {target} is not a draw target (create it with createDrawTarget)"));
    };
    if let Some(before_id) = before {
      if !list.entries.contains_key(&before_id) {
        return Err(format!("draw {before_id} (before) not found on target {target}"));
      }
    }
    let (uniforms, stride, instance_stride, depth) = match self.pipeline_mirrors.borrow().get(&entry.pipeline) {
      Some(pm) => (pm.uniforms.clone(), pm.stride, pm.instance_stride, pm.depth),
      None => return Err(format!("pipeline {} not found", entry.pipeline)),
    };
    if depth && !list.depth {
      return Err(format!(
        "pipeline {} tests depth but target {target} has no depth buffer (create the draw target with depth: true)",
        entry.pipeline
      ));
    }
    let bounds = self.resolve_entry_range(&mut entry, stride, instance_stride)?;
    validate_params(&uniforms, &entry.params)?;
    validate_texture_bindings(&uniforms, &entry.textures)?;
    let draw_id = list.next_draw;
    self.validate_new_bindings(target, draw_id, &entry.textures)?;
    // The entry's effective inputs include the shared names its program
    // declares and does not bind itself (shared bindings live under entry
    // key 0): the unit budget must hold for the combination, checked here so
    // an over-budget add throws at its call site instead of dropping inputs
    // raster-side. The one place existing shared state gates an add.
    {
      let sources = self.shader_sources.borrow();
      let shared_extra = sources.get(&target).map_or(0, |c| {
        c.keys()
          .filter(|(e, name)| {
            *e == 0
              && uniforms.get(name.as_str()).is_some_and(|s| s.kind == UniformKind::Sampler2D)
              && !entry.textures.iter().any(|b| b.name == *name)
          })
          .count()
      });
      self.gpu_limits().check_texture_units(entry.textures.len() + shared_extra)?;
    }
    list.next_draw += 1;
    list.entries.insert(draw_id, EntryMirror { uniforms, draw: entry.draw, bounds, buffers: entry.buffer_ids() });
    drop(targets);
    let mut sources = self.shader_sources.borrow_mut();
    let record = sources.entry(target).or_default();
    for b in &entry.textures {
      record.insert((draw_id, b.name.clone()), b.id);
    }
    drop(sources);
    self.send(RasterCmd::AddDraw { target, draw: draw_id, entry, before });
    self.note_target_content(target);
    Ok(draw_id)
  }

  /// Reorder a draw target's list: `order` must name every current entry
  /// exactly once (a full permutation, validated here against the mirror).
  /// List order is draw order - later entries land over earlier ones where
  /// depth does not decide - so this is the sorting verb: opaque
  /// front-to-back, transparent back-to-front. Fire-and-forget; the caller
  /// must request a frame.
  pub fn set_draw_order(&self, target: u64, order: &[u64]) -> Result<(), String> {
    {
      let targets = self.targets.borrow();
      let mirror = targets.get(&target).ok_or_else(|| format!("shader texture {target} not found"))?;
      let Some(list) = mirror.entries.as_ref() else {
        return Err(format!("target {target} is not a draw target (create it with createDrawTarget)"));
      };
      validate_order(order, list.entries.keys().copied())?;
    }
    self.send(RasterCmd::SetDrawOrder { target, order: order.to_vec() });
    self.note_target_content(target);
    Ok(())
  }

  /// Remove a draw entry from a draw target; the remaining entries keep
  /// their order and ids. The removed id errors from then on (never reused).
  /// Fire-and-forget; the caller must request a frame.
  pub fn remove_draw(&self, target: u64, draw: u64) -> Result<(), String> {
    let mut targets = self.targets.borrow_mut();
    let mirror = targets.get_mut(&target).ok_or_else(|| format!("shader texture {target} not found"))?;
    let Some(list) = mirror.entries.as_mut() else {
      return Err(format!("target {target} is not a draw target (create it with createDrawTarget)"));
    };
    if list.entries.remove(&draw).is_none() {
      return Err(format!("draw {draw} not found on target {target}"));
    }
    drop(targets);
    if let Some(record) = self.shader_sources.borrow_mut().get_mut(&target) {
      record.retain(|(d, _), _| *d != draw);
    }
    self.send(RasterCmd::RemoveDraw { target, draw });
    self.note_target_content(target);
    Ok(())
  }

  /// Update one draw entry's params (the per-entry analog of
  /// `set_target_params`): validated against the entry's program, merged by
  /// name at the next render. The caller must request a frame.
  pub fn set_draw_params(&self, target: u64, draw: u64, params: &[(String, ParamValue)]) -> Result<(), String> {
    {
      let targets = self.targets.borrow();
      let entry = entry_mirror(&targets, target, draw)?;
      validate_params(&entry.uniforms, params)?;
    }
    self.send(RasterCmd::UpdateDrawParams { target, draw, params: params.to_vec() });
    self.note_target_content(target);
    Ok(())
  }

  /// Update a target's target-level params, routing by target kind. A
  /// single-program target (fragment texture, fixed pipeline target) has one
  /// pass, so target-level params ARE that pass's params: every name must be
  /// an active uniform with a matching component count (validated here,
  /// against the mirror, so the error lands at the call site; note a
  /// declared-but-optimized-out uniform reflects as absent and reports "no
  /// active uniform"), and the target re-renders (sampler inputs
  /// re-resolved) at the raster thread's next dirty flush, as do any targets
  /// sampling it, transitively. The output keeps its id and Impeller texture
  /// (no re-adoption); only the GL contents change.
  ///
  /// A draw target's target-level params are its SHARED params: values every
  /// entry reads - a camera's view-projection above all - folded by name like
  /// every params write and applied at render before each entry's own params,
  /// so an entry naming the same uniform overrides the shared value (specific
  /// beats general). Shared params are target state: they survive entry
  /// add/remove/rebuild. A target legitimately mixes material classes, so a
  /// name is applied where declared and skipped elsewhere (the iResolution
  /// rule), down to ZERO coverage: a name no current entry declares is
  /// stored and skips everywhere until a declaring entry arrives. That keeps
  /// shared state independent of write order - a value seeded before any
  /// entry exists and one written after entries attached are the same state -
  /// and lets a scene publish a standard set (camera position beside the
  /// view-projection) whatever materials are present. Validation is arity
  /// where declared: a name must match the declared component count in every
  /// entry program that declares it. The caller must request a frame.
  pub fn set_target_params(&self, target: u64, params: &[(String, ParamValue)]) -> Result<(), String> {
    {
      let targets = self.targets.borrow();
      let mirror = targets.get(&target).ok_or_else(|| format!("target {target} not found"))?;
      let Some(list) = mirror.entries.as_ref() else {
        validate_params(&mirror.uniforms, params)?;
        drop(targets);
        self.send(RasterCmd::UpdateShaderParams { id: target, params: params.to_vec() });
        self.note_target_content(target);
        return Ok(());
      };
      for (name, value) in params {
        for entry in list.entries.values() {
          validate_param_if_declared(&entry.uniforms, name, value)?;
        }
      }
    }
    self.send(RasterCmd::UpdateTargetParams { target, params: params.to_vec() });
    self.note_target_content(target);
    Ok(())
  }

  /// Rebind a target's target-level sampler2D inputs by uniform name,
  /// routing by target kind like `set_target_params`; bindings not named
  /// keep their current source, and the caller must request a frame. On a
  /// single-program target the bindings are the one pass's inputs, validated
  /// strictly against its uniform table. Every path errors if the target or
  /// any source texture id is unknown, or a binding would create a sampling
  /// cycle whose members are all flush-rendered targets (such a cycle is a
  /// feedback loop the flush cannot order). A cycle through a manual target
  /// is legal: the flush never renders one, so the loop is only ever stepped
  /// by explicit renders - ping-pong feedback is two manual targets bound to
  /// each other. Self-binding stays rejected for every target, manual
  /// included: a pass sampling the very texture it writes is a same-pass GL
  /// feedback loop (undefined pixels), not a scheduling problem.
  ///
  /// A draw target's target-level bindings are its SHARED bindings: sources
  /// every entry reads (an environment map, a shadow map, a LUT), written
  /// once per target. At render each entry gets the shared names its program
  /// declares and its own bindings do not override - an entry's own binding
  /// wins, and coverage may be partial, exactly like `set_target_params`.
  /// Shared bindings are target state: entry add/remove/rebuild cannot lose
  /// them. Validation: each name must be a sampler2D everywhere it is
  /// declared, and coverage may be ZERO exactly like `set_target_params` -
  /// an undeclared name is stored, joins the sampler graph, and binds when
  /// a declaring entry arrives. Every entry's effective input count (its
  /// own bindings plus the applicable merged shared set) must fit the
  /// device's texture units, and a shared edge counts for propagation and
  /// cycles even before any entry declares its name.
  pub fn set_target_textures(&self, target: u64, textures: &[TextureBinding]) -> Result<(), String> {
    {
      let targets = self.targets.borrow();
      let mirror = targets.get(&target).ok_or_else(|| format!("target {target} not found"))?;
      let Some(list) = mirror.entries.as_ref() else {
        validate_texture_bindings(&mirror.uniforms, textures)?;
        drop(targets);
        self.validate_new_bindings(target, 0, textures)?;
        let mut sources = self.shader_sources.borrow_mut();
        let record = sources.entry(target).or_default();
        for b in textures {
          record.insert((0, b.name.clone()), b.id);
        }
        drop(sources);
        self.send(RasterCmd::UpdateShaderTextures { id: target, textures: textures.to_vec() });
        self.note_target_content(target);
        return Ok(());
      };
      for TextureBinding { name, .. } in textures {
        for entry in list.entries.values() {
          if let Some(slot) = entry.uniforms.get(name) {
            if slot.kind == UniformKind::Inactive {
              continue;
            }
            if slot.kind != UniformKind::Sampler2D || slot.count > 1 {
              return Err(format!("uniform '{name}' is {}, not a sampler2D", slot.glsl_name()));
            }
          }
        }
      }
      // Per-entry unit budget against the MERGED shared set: an entry's
      // effective inputs are its own bindings plus the shared names its
      // program declares and does not bind itself.
      let sources = self.shader_sources.borrow();
      let record = sources.get(&target);
      let mut shared: Vec<&str> =
        record.map(|c| c.keys().filter(|(e, _)| *e == 0).map(|(_, n)| n.as_str()).collect()).unwrap_or_default();
      for TextureBinding { name, .. } in textures {
        if !shared.contains(&name.as_str()) {
          shared.push(name);
        }
      }
      let limits = self.gpu_limits();
      for (draw_id, entry) in list.entries.iter() {
        let own_count = record.map_or(0, |c| c.keys().filter(|(e, _)| *e == *draw_id).count());
        let extra = shared
          .iter()
          .filter(|n| {
            entry.uniforms.get(**n).is_some_and(|s| s.kind == UniformKind::Sampler2D)
              && record.is_none_or(|c| !c.contains_key(&(*draw_id, (**n).to_string())))
          })
          .count();
        limits.check_texture_units(own_count + extra).map_err(|e| format!("draw {draw_id}: {e}"))?;
      }
    }
    self.validate_new_bindings(target, 0, textures)?;
    let mut sources = self.shader_sources.borrow_mut();
    let record = sources.entry(target).or_default();
    for b in textures {
      record.insert((0, b.name.clone()), b.id);
    }
    drop(sources);
    self.send(RasterCmd::UpdateTargetTextures { target, textures: textures.to_vec() });
    self.note_target_content(target);
    Ok(())
  }

  /// Rebind one draw entry's sampler2D inputs by uniform name (the per-entry
  /// analog of `set_target_textures`); bindings not named keep their current
  /// source. Same checks as every bind path: names against the entry's
  /// program, per-entry unit count, source existence, cycles. The caller
  /// must request a frame.
  pub fn set_draw_textures(&self, target: u64, draw: u64, textures: &[TextureBinding]) -> Result<(), String> {
    let entry_uniforms = {
      let targets = self.targets.borrow();
      let entry = entry_mirror(&targets, target, draw)?;
      validate_texture_bindings(&entry.uniforms, textures)?;
      entry.uniforms.clone()
    };
    self.validate_new_bindings(target, draw, textures)?;
    // Combined with the target's shared bindings (entry key 0), the entry's
    // merged inputs must still fit the unit budget - the add_draw rule,
    // re-checked because a rebind can add names.
    {
      let sources = self.shader_sources.borrow();
      let record = sources.get(&target);
      let own = record.map_or(0, |c| c.keys().filter(|(e, _)| *e == draw).count())
        + textures.iter().filter(|b| record.is_none_or(|c| !c.contains_key(&(draw, b.name.clone())))).count();
      let shared_extra = record.map_or(0, |c| {
        c.keys()
          .filter(|(e, name)| {
            *e == 0
              && entry_uniforms.get(name.as_str()).is_some_and(|s| s.kind == UniformKind::Sampler2D)
              && !c.contains_key(&(draw, name.clone()))
              && !textures.iter().any(|b| b.name == *name)
          })
          .count()
      });
      self.gpu_limits().check_texture_units(own + shared_extra)?;
    }
    let mut sources = self.shader_sources.borrow_mut();
    let record = sources.entry(target).or_default();
    for b in textures {
      record.insert((draw, b.name.clone()), b.id);
    }
    drop(sources);
    self.send(RasterCmd::UpdateDrawTextures { target, draw, textures: textures.to_vec() });
    self.note_target_content(target);
    Ok(())
  }

  /// Update one draw entry's range and/or buffers (the per-entry `set_draw`):
  /// see `update_draw`. The caller must request a frame.
  pub fn set_draw_range(&self, target: u64, draw: u64, update: DrawUpdate) -> Result<(), String> {
    self.update_draw(target, Some(draw), update)
  }

  /// Apply a `DrawUpdate` to one entry - `draw` None addresses the
  /// single-draw kinds' one entry (the setDraw side), Some a draw target's
  /// entry (the setDrawRange side). One transaction: the buffer swap
  /// (replace-only, see `BufferIds::merged`) and the range merge are both
  /// validated against the resulting state - the merged range against the
  /// swapped buffers' sizes - before either commits, so an error leaves the
  /// entry exactly as it was. A swap alone keeps the current range, which
  /// must still fit the new buffers (a too-small buffer errors here; a
  /// larger one never does); a swap plus a range extends into the new
  /// buffer in one call. The growth primitive: a population outgrowing its
  /// instance buffer allocates a bigger one, writes it, swaps, and destroys
  /// the old (the entry holds the old buffer alive until the swap lands).
  pub(super) fn update_draw(&self, target: u64, draw: Option<u64>, update: DrawUpdate) -> Result<(), String> {
    let mut targets = self.targets.borrow_mut();
    let mirror = targets.get_mut(&target).ok_or_else(|| format!("shader texture {target} not found"))?;
    let (ids, bounds, range) = match (draw, mirror.entries.as_mut()) {
      (None, Some(_)) => {
        return Err(format!("target {target} is a draw target; update draws per entry with setDrawRange"));
      }
      (Some(_), None) => {
        return Err(format!("target {target} is not a draw target (create it with createDrawTarget)"));
      }
      (None, None) => {
        let Some(range) = mirror.draw.as_mut() else {
          return Err("not a pipeline texture".to_string());
        };
        (&mut mirror.buffers, &mut mirror.bounds, range)
      }
      (Some(id), Some(list)) => {
        let entry = list.entries.get_mut(&id).ok_or_else(|| format!("draw {id} not found on target {target}"))?;
        (&mut entry.buffers, &mut entry.bounds, &mut entry.draw)
      }
    };
    let next_ids = ids.merged(update.buffers)?;
    let swapped = next_ids != *ids;
    let next_bounds = if swapped { self.rebound(*bounds, next_ids)? } else { *bounds };
    let next_range = range.merged(update, next_bounds.indexed)?;
    validate_draw_range(next_range, next_bounds)?;
    *ids = next_ids;
    *bounds = next_bounds;
    *range = next_range;
    drop(targets);
    if swapped {
      self.send(RasterCmd::SetDrawBuffers { target, draw, ids: next_ids });
    }
    match draw {
      None => self.send(RasterCmd::SetDraw { id: target, range: next_range }),
      Some(draw) => self.send(RasterCmd::SetDrawRange { target, draw, range: next_range }),
    }
    self.note_target_content(target);
    Ok(())
  }

  /// `bounds` re-sized for `ids`: the fetch bounds keep their strides (the
  /// pipeline layout and vocabulary are unchanged by a swap; only an index
  /// format change moves the element size) and take the named buffers'
  /// sizes. Errs on an id the buffer registry does not know.
  fn rebound(&self, bounds: DrawBounds, ids: BufferIds) -> Result<DrawBounds, String> {
    let sizes = self.buffer_sizes.borrow();
    let size_of = |id: u64, role: &str| sizes.get(&id).copied().ok_or_else(|| format!("{role} {id} not found"));
    let fetch = match bounds.fetch {
      None => None,
      Some((stride, _)) => Some(match ids.index {
        Some((id, format)) => (format.size() as usize, size_of(id, "index buffer")?),
        None => (stride, size_of(ids.buffer, "buffer")?),
      }),
    };
    let instance = match bounds.instance {
      None => None,
      Some((stride, _)) => Some((stride, size_of(ids.instance_buffer, "instance buffer")?)),
    };
    Ok(DrawBounds { fetch, indexed: bounds.indexed, instance })
  }

  /// Update a pipeline texture's draw range - which vertices are drawn
  /// (`first_vertex`, `vertex_count`) and how many instances - and re-render
  /// it with its last-applied params. Fields absent from `update` keep their
  /// current value (the params merge rule), so the common case stays one
  /// field. The caller must request a frame. Errs on a negative field, or a
  /// vertex range whose fetch would run past the end of the target's buffer
  /// (undefined behaviour in raw GLES; validated against the bound captured
  /// at create, see `TargetMirror::bounds` - attributeless targets fetch
  /// nothing, so any non-negative range is safe there).
  pub fn set_draw(&self, id: u64, update: DrawUpdate) -> Result<(), String> {
    self.update_draw(id, None, update)
  }

  /// Render a manual target (`TargetSpec::manual`) once, now. Fire-and-forget
  /// on the ordered raster channel, so renders land in call order relative to
  /// every other GPU command - two renders of one target run twice, in order,
  /// and a readback issued after one observes its pass. Pending pure-target
  /// writes flush first, so the pass samples fresh inputs; targets sampling
  /// this one re-render at the next flush. The caller must request a frame
  /// for displayed output. Errs on an unknown id or a target the flush owns
  /// (a non-manual one, whose pass must stay a pure function of its inputs).
  pub fn render_target(&self, id: u64) -> Result<(), String> {
    if !self.targets.borrow().contains_key(&id) {
      return Err(format!("shader texture {id} not found"));
    }
    if !self.manual_targets.borrow().contains(&id) {
      return Err(format!("target {id} is not manual (the runtime renders it; create with render: \"manual\")"));
    }
    self.send(RasterCmd::RenderTarget { id });
    // A manual target's pixels change exactly here (and at copy_texture), so
    // this notes directly - note_target_content's manual skip is for the
    // writes that only stage state for a later render.
    self.note_content(id);
    Ok(())
  }

  /// Declare (or clear, with None) the window shader: the frame then resolves
  /// into the runtime-owned layer and `shader.program` draws over it into the
  /// window as the last step before present (see `WindowShader`). Fire-and-
  /// forget on the ordered frame channel, so a change lands cleanly between
  /// two frames; the raster thread holds the program while declared, so
  /// destroying its handle keeps the effect running until it is re-declared
  /// or cleared. The caller must request a frame. Errs on an unknown program
  /// handle, or on params/textures naming anything but the program's active
  /// uniforms (same call-site validation as the target paths; `uSource`,
  /// `uPrevious` and `iResolution` are runtime-filled and need no entry
  /// here - anything else the shader declares, a time uniform included, is
  /// app-driven through `params` like any other uniform).
  pub fn set_window_shader(&self, shader: Option<WindowShader>) -> Result<(), String> {
    if let Some(ws) = &shader {
      // The runtime-filled layers occupy units ahead of the declared inputs:
      // uSource always, uPrevious while declared.
      self.gpu_limits().check_texture_units(1 + usize::from(ws.previous) + ws.textures.len())?;
      let programs = self.program_uniforms.borrow();
      let uniforms = programs.get(&ws.program).ok_or_else(|| format!("program {} not found", ws.program))?;
      validate_params(uniforms, &ws.params)?;
      validate_texture_bindings(uniforms, &ws.textures)?;
    }
    self.send(RasterCmd::SetWindowShader { shader });
    Ok(())
  }
}

/// The loadOp invariant behind both target create paths: loading the
/// previous contents makes render count observable, which only the app may
/// count - on a flush-rendered target the output would silently depend on
/// how often the flush happened to run.
fn validate_load(spec: &TargetSpec) -> Result<(), String> {
  if spec.load && !spec.manual {
    return Err(
      "loadOp \"load\" requires render: \"manual\" (a runtime-rendered target must stay a pure function of its inputs)"
        .to_string(),
    );
  }
  if spec.load && spec.samples > 1 {
    return Err(
      "loadOp \"load\" cannot combine with samples > 1 (multisampled storage cannot load the previous contents)"
        .to_string(),
    );
  }
  Ok(())
}
