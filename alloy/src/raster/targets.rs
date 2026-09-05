//! Shader/pipeline/draw-target lifecycle and the dirty flush: creating the
//! target kinds (fragment, pipeline, draw list, sub-target), registering and
//! adopting their storage into Impeller, the fire-and-forget entry writes,
//! resize, the copy pass, and flush_dirty's dependency-ordered propagation
//! over the sampler graph (propagation_order, pure and unit-tested).

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::Ordering;

use impellers::{ISize, Texture};

use super::{cmd, ensure_copy_program, resolve_entry_buffers, RasterState, RasterStats};
use crate::gl;
use crate::gl::{GpuTexture, PassInput, PassTimer, SamplerCache, ShaderProgram, ShaderTexture, Timed};
use crate::gpu::{
  validate_binding_shapes, validate_params, validate_texture_bindings, BoundTexture, DepthStorage, DrawSpec,
  ParamValue, PipelineSpec, SamplerState, TargetSpec, TextureBinding, TextureFormat, TextureShape, UniformKind,
  UniformTable,
};

impl RasterState {
  /// What the shape rules see for a registered id (see
  /// `validate_binding_shapes`): this thread's answer for the fused creates,
  /// whose uniform kinds only exist post-compile.
  fn bound_texture(&self, id: u64) -> Option<BoundTexture> {
    self.textures.get(&id).map(|t| BoundTexture { shape: t.shape, format: t.format })
  }

  /// Render a manual target once, now (see `RasterCmd::RenderTarget`): fresh
  /// inputs first (the pixel-observer rule: this pass samples its sources),
  /// then the one pass, then seed the dirty set so targets sampling this one
  /// re-render at the next flush - the same shape as an uploadTexture
  /// content change.
  pub(super) fn render_target_now(&mut self, id: u64, face: Option<u32>, level: Option<u32>) {
    self.flush_sources_of(id);
    match self.shaders.get(&id) {
      Some(shader) => {
        timed_pass(&self.gl, &mut self.pass_timer, &self.stats, id, shader, || {
          let resolve = |bindings: &[TextureBinding], program: &ShaderProgram| {
            resolve_binding_list(&self.textures, &self.samplers, bindings, program)
          };
          match face {
            Some(face) => shader.render_face(&self.gl, &resolve, face, level),
            None => shader.render(&self.gl, &resolve),
          }
        });
        self.dirty.insert(id);
      }
      None => log::warn!("[alloy] render target failed: shader texture {id} not found"),
    }
  }

  /// Resolve every pending target write: render each shader/pipeline target
  /// whose own state changed, or whose sampled content (transitively) did,
  /// in dependency order - sources before the targets sampling them - then
  /// clear the dirty set. Called at the points target pixels become
  /// observable (a drawn frame, an offscreen rasterization, a readback), so
  /// a chain of targets propagates end to end with each target rendered at
  /// most once per flush. Manual targets are excluded from the graph: the
  /// flush never renders one (only RenderTarget does) and never propagates
  /// through one - a dirty manual id acts as a plain content source, exactly
  /// like an uploaded texture. That exclusion is what keeps the purity
  /// invariant honest: everything ordered here is a pure function of its
  /// inputs, so rendering it zero, one, or many times is indistinguishable.
  pub(super) fn flush_dirty(&mut self) {
    self.flush_dirty_within(None);
  }

  /// The flush a manual render owes its pass: only the pending writes among
  /// the targets `id` transitively samples (through non-manual targets; a
  /// manual one is a plain content source here as in `flush_dirty`). Any
  /// other dirty target - typically the auto target that samples the manual
  /// one being rendered, dirtied by its previous face or level pass - waits
  /// for the frame's flush instead of re-rendering before every pass of a
  /// six-face, nine-level sequence.
  pub(super) fn flush_sources_of(&mut self, id: u64) {
    if self.dirty.is_empty() {
      return;
    }
    let mut scope: HashSet<u64> = HashSet::new();
    let mut stack = vec![id];
    while let Some(t) = stack.pop() {
      let Some(shader) = self.shaders.get(&t) else { continue };
      for s in shader.binding_sources() {
        let s = self.depth_owners.get(&s).copied().unwrap_or(s);
        if self.shaders.get(&s).is_some_and(|src| !src.manual()) && scope.insert(s) {
          stack.push(s);
        }
      }
    }
    if !scope.is_empty() {
      self.flush_dirty_within(Some(&scope));
    }
  }

  /// `flush_dirty` over every non-manual target, or, with a `scope`, over
  /// the targets in it only: the rest of the dirty set is left for a later
  /// flush, with the rendered targets' out-of-scope samplers dirtied in
  /// their place (the rendered ones are fresh now).
  fn flush_dirty_within(&mut self, scope: Option<&HashSet<u64>>) {
    if self.dirty.is_empty() {
      return;
    }
    // A binding to a depth id is an edge to the target that renders it. A
    // sub-target is an edge from its parent: a dirty tile makes the parent
    // affected and orders it after the tile's own sources, and the parent
    // renders the one pass for all of them below.
    let own_sources: HashMap<u64, Vec<u64>> = self
      .shaders
      .iter()
      .filter(|(_, shader)| !shader.manual())
      .map(|(id, shader)| {
        let sources =
          shader.binding_sources().into_iter().map(|s| self.depth_owners.get(&s).copied().unwrap_or(s)).collect();
        (*id, sources)
      })
      .collect();
    let mut edges = own_sources.clone();
    for (parent, tiles) in &self.regions {
      if let Some(sources) = edges.get_mut(parent) {
        sources.extend(tiles.iter().copied());
      }
    }
    let (order, cyclic) = propagation_order(&self.dirty, &edges);
    if !cyclic.is_empty() {
      // The UI side rejects sampling cycles at bind time, so reaching this
      // means the mirrors diverged. Render each member once anyway: stale
      // inputs, but forward progress and no hang.
      let members: Vec<String> = cyclic.iter().map(|id| self.texture_desc(*id)).collect();
      log::warn!("[alloy] sampling cycle between shader targets [{}]; rendering each once", members.join(", "));
    }
    let affected: HashSet<u64> = order.iter().chain(cyclic.iter()).copied().collect();
    let changed = |id: &u64| self.dirty.contains(id) || affected.contains(id);
    let tile_clear =
      if self.regions.is_empty() { None } else { ensure_tile_clear_program(&self.gl, &mut self.tile_clear_program) };
    let mut rendered: Vec<u64> = Vec::new();
    for id in order.iter().chain(cyclic.iter()) {
      if scope.is_some_and(|s| !s.contains(id)) {
        continue;
      }
      let Some(shader) = self.shaders.get(id) else { continue };
      // A tile renders as a group of its parent's pass, never alone.
      if shader.region().is_some() {
        continue;
      }
      rendered.push(*id);
      let resolve = |bindings: &[TextureBinding], program: &ShaderProgram| {
        resolve_binding_list(&self.textures, &self.samplers, bindings, program)
      };
      timed_pass(&self.gl, &mut self.pass_timer, &self.stats, *id, shader, || match self.regions.get(id) {
        None => shader.render(&self.gl, &resolve),
        Some(tiles) => {
          // Full when the parent's own state or inputs changed (its clear
          // wipes every tile, so every tile redraws), or when every tile
          // changed anyway (a pass-level clear is cheaper on a tiler than
          // the full-surface load a no-clear pass implies); otherwise only
          // the changed tiles, each over its own rectangle.
          let full = self.dirty.contains(id)
            || own_sources.get(id).is_some_and(|s| s.iter().any(changed))
            || tiles.iter().all(changed);
          let tiles: Vec<&ShaderTexture> =
            tiles.iter().filter(|t| full || changed(t)).filter_map(|t| self.shaders.get(t)).collect();
          shader.render_groups(&self.gl, &resolve, full, &tiles, tile_clear.as_deref());
        }
      });
    }
    match scope {
      None => self.dirty.clear(),
      Some(scope) => {
        for r in &rendered {
          self.dirty.remove(r);
          if let Some(tiles) = self.regions.get(r) {
            for tile in tiles {
              self.dirty.remove(tile);
            }
          }
          // A fresh target's content changed for the samplers outside the
          // scope: they take its place in the dirty set.
          for (d, sources) in &edges {
            if !scope.contains(d) && sources.contains(r) {
              self.dirty.insert(*d);
            }
          }
        }
      }
    }
  }

  /// Resize an existing shader/pipeline target in place: a new target texture
  /// on the same FBO and program, re-rendered at the new size with the
  /// last-applied params (a manual target is cleared instead - the pass only
  /// runs on RenderTarget), then adopted into Impeller. Replies with the new
  /// handle so the UI side re-registers it under the same id; the old handle
  /// keeps the old GL name alive until in-flight display lists drop it.
  pub(super) fn resize_shader_texture(
    &mut self,
    id: u64,
    width: u32,
    height: u32,
  ) -> Result<cmd::TargetHandles, String> {
    let shader = self.shaders.get_mut(&id).ok_or_else(|| format!("shader texture {id} not found"))?;
    // An unadopted target's old name is this thread's to free once the
    // resize has allocated the new one (Impeller frees every adopted name).
    let unadopted = self.unadopted.contains(&id);
    let old_name = self.textures.get(&id).map(|old| old.gl_texture);
    shader.resize(&self.gl, width, height)?;
    let shader = self.shaders.get(&id).expect("shader present after resize");
    let size = ISize::new(width as i64, height as i64);
    let gpu = GpuTexture {
      gl_texture: shader.gl_texture(),
      width,
      height,
      shape: TextureShape::D2,
      sampler: shader.sampler(),
      format: shader.format(),
      // The id-stable resize keeps the create's label, like create_texture's
      // replace-at-id path.
      label: self.textures.get(&id).and_then(|old| old.label.clone()),
    };
    // The depth texture got a fresh name too (see ShaderTexture::resize):
    // re-adopt and re-register it at its own id.
    let depth = match (shader.depth_texture(), self.target_depths.get(&id).copied()) {
      (Some(gl_texture), Some(depth_id)) => {
        let label = self.textures.get(&depth_id).and_then(|old| old.label.clone());
        let depth_gpu = GpuTexture {
          gl_texture,
          width,
          height,
          shape: TextureShape::D2,
          sampler: SamplerState::DEPTH,
          format: TextureFormat::Depth24,
          label,
        };
        let impeller =
          gl::adopt_texture(&depth_gpu, &self.impeller_ctx, size).ok_or("adopt resized depth texture failed")?;
        self.textures.insert(depth_id, depth_gpu);
        Some(impeller)
      }
      _ => None,
    };
    if unadopted {
      if let Some(old) = old_name {
        unsafe { glow::HasContext::delete_texture(&self.gl, old) };
      }
      self.textures.insert(id, gpu);
      self.reseed_resized(id);
      return Ok(cmd::TargetHandles { color: None, depth });
    }
    match gl::adopt_texture(&gpu, &self.impeller_ctx, size) {
      Some(impeller) => {
        self.textures.insert(id, gpu);
        self.reseed_resized(id);
        Ok(cmd::TargetHandles { color: Some(impeller), depth })
      }
      // Should-not-happen path (adoption of a valid GL name): the shader keeps
      // rendering into the new target, but the registry entry still shows the
      // old one. The new name stays referenced by the shader, so nothing is
      // freed here; the error surfaces to the caller.
      None => Err("adopt resized shader texture failed".to_string()),
    }
  }

  #[allow(clippy::too_many_arguments)]
  pub(super) fn create_shader_texture(
    &mut self,
    id: u64,
    width: u32,
    height: u32,
    fragment_src: &str,
    params: &[(String, ParamValue)],
    textures: Vec<TextureBinding>,
    sampler: SamplerState,
    label: Option<String>,
  ) -> Result<(Texture, UniformTable), String> {
    let mut shader = ShaderTexture::new(&self.gl, width, height, fragment_src, textures)?.with_sampler(sampler);
    let uniforms = shader.uniform_table();
    // Uniform names only exist after the compile, so create-time params and
    // bindings validate here, inside the blocking RPC - the error still
    // surfaces at the JS call site, and the half-built target rolls back.
    if let Err(e) = validate_params(&uniforms, params)
      .and_then(|()| validate_texture_bindings(&uniforms, shader.sampler_bindings()))
      .and_then(|()| validate_binding_shapes(&uniforms, shader.sampler_bindings(), |id| self.bound_texture(id)))
    {
      shader.destroy(&self.gl);
      return Err(e);
    }
    shader.merge_params(params);
    let texture = self.register_shader_target(id, shader, width, height, label, "adopt shader texture failed")?;
    Ok((texture, uniforms))
  }

  pub(super) fn create_pipeline_texture(
    &mut self,
    id: u64,
    spec: PipelineSpec,
  ) -> Result<(Texture, UniformTable), String> {
    let label = spec.target.label.clone();
    let buffers = resolve_entry_buffers(&self.buffers, spec.entry.buffer_ids())?;
    let shader = ShaderTexture::new_pipeline(
      &self.gl,
      spec.target.width,
      spec.target.height,
      &spec.vertex_src,
      &spec.fragment_src,
      spec.entry.textures.clone(),
      spec.pipeline,
      buffers,
      spec.entry.draw,
      spec.target.clear_color,
      spec.target.samples,
    )?;
    let mut shader =
      shader.with_sampler(spec.target.sampler).with_manual(spec.target.manual).with_load(spec.target.load);
    let uniforms = shader.uniform_table();
    // Same post-compile validation and rollback as create_shader_texture.
    if let Err(e) = validate_params(&uniforms, &spec.entry.params)
      .and_then(|()| validate_texture_bindings(&uniforms, shader.sampler_bindings()))
      .and_then(|()| validate_binding_shapes(&uniforms, shader.sampler_bindings(), |id| self.bound_texture(id)))
    {
      shader.destroy(&self.gl);
      return Err(e);
    }
    shader.merge_params(&spec.entry.params);
    let texture = self.register_shader_target(
      id,
      shader,
      spec.target.width,
      spec.target.height,
      label,
      "adopt pipeline texture failed",
    )?;
    Ok((texture, uniforms))
  }

  /// Create a fixed single-entry target over a registered pipeline
  /// (`entry.pipeline`) and adopt it under texture id `id`; the first render
  /// happens at the next dirty flush.
  pub(super) fn create_shader_target(&mut self, id: u64, spec: TargetSpec, entry: DrawSpec) -> Result<Texture, String> {
    let pipeline_id = entry.pipeline;
    let pipeline =
      self.render_pipelines.get(&pipeline_id).ok_or_else(|| format!("pipeline {pipeline_id} not found"))?.clone();
    // The program already exists, so params and bindings validate before
    // anything is built - no rollback needed on this path.
    let uniforms = pipeline.uniform_table();
    validate_params(&uniforms, &entry.params)?;
    validate_texture_bindings(&uniforms, &entry.textures)?;
    validate_binding_shapes(&uniforms, &entry.textures, |id| self.bound_texture(id))?;
    let buffers = resolve_entry_buffers(&self.buffers, entry.buffer_ids())?;
    let mut shader = ShaderTexture::from_pipeline(
      &self.gl,
      pipeline,
      Some(pipeline_id),
      spec.width,
      spec.height,
      entry.textures.clone(),
      buffers,
      entry.draw,
      spec.clear_color,
      spec.samples,
    )
    .map_err(|(_, e)| e)?
    .with_sampler(spec.sampler)
    .with_manual(spec.manual)
    .with_load(spec.load);
    shader.merge_params(&entry.params);
    self.register_shader_target(id, shader, spec.width, spec.height, spec.label, "adopt shader target failed")
  }

  /// Create a draw target - empty ordered draw list over color plus optional
  /// target-owned depth storage - and adopt it under texture id `id`. A
  /// flush-rendered draw target starts dirty (its first render is the clear);
  /// a manual one is cleared at registration like every manual target.
  ///
  /// With `DepthStorage::Texture` the depth texture is adopted and
  /// registered under `depth_id` beside the color: the same ownership as
  /// the color (Impeller deletes the name when the handle drops), and the
  /// same fixed sampling its id declares (`SamplerState::DEPTH`).
  pub(super) fn create_draw_target(
    &mut self,
    id: u64,
    depth_id: Option<u64>,
    spec: TargetSpec,
    depth: DepthStorage,
    format: TextureFormat,
  ) -> Result<cmd::TargetHandles, String> {
    let (width, height) = (spec.width, spec.height);
    let shader =
      ShaderTexture::new_draw_target(&self.gl, width, height, depth, spec.clear_color, spec.samples, format)?
        .with_sampler(spec.sampler)
        .with_manual(spec.manual)
        .with_load(spec.load);
    let depth_texture = shader.depth_texture();
    let depth_label = spec.label.as_ref().map(|l| format!("{l}.depth"));
    // Impeller adopts rgba8 alone (what it displays and reads back); any
    // other format is a sampler-only target, registered without a handle.
    let color = if format == TextureFormat::Rgba8 {
      match self.register_shader_target(id, shader, width, height, spec.label, "adopt draw target failed") {
        Ok(color) => Some(color),
        Err(e) => {
          // register deleted the color name; the depth texture is ours until
          // adopted.
          if let Some(tex) = depth_texture {
            unsafe { glow::HasContext::delete_texture(&self.gl, tex) };
          }
          return Err(e);
        }
      }
    } else {
      self.register_unadopted_target(id, shader, width, height, format, spec.label);
      None
    };
    let depth = match (depth_texture, depth_id) {
      (Some(gl_texture), Some(depth_id)) => {
        let gpu = GpuTexture {
          gl_texture,
          width,
          height,
          shape: TextureShape::D2,
          sampler: SamplerState::DEPTH,
          format: TextureFormat::Depth24,
          label: depth_label,
        };
        match gl::adopt_texture(&gpu, &self.impeller_ctx, ISize::new(width as i64, height as i64)) {
          Some(impeller) => {
            self.textures.insert(depth_id, gpu);
            self.target_depths.insert(id, depth_id);
            self.depth_owners.insert(depth_id, id);
            Some(impeller)
          }
          None => {
            // Roll the color registration back: dropping `color` lets
            // Impeller delete that name; the depth name is still ours.
            self.textures.remove(&id);
            self.dirty.remove(&id);
            if let Some(shader) = self.shaders.remove(&id) {
              shader.destroy(&self.gl);
            }
            unsafe { glow::HasContext::delete_texture(&self.gl, gl_texture) };
            return Err("adopt depth texture failed (depth \"texture\" is unavailable on this device)".to_string());
          }
        }
      }
      _ => None,
    };
    Ok(cmd::TargetHandles { color, depth })
  }

  /// Create a cube draw target (see `RasterCmd::CreateCubeDrawTarget`):
  /// registered like an uploaded cube map - a cube-shaped texture entry
  /// with no Impeller adoption, the name deleted by `release_texture` on
  /// destroy - plus its shader, cleared now (a manual target's defined
  /// initial contents, all six faces).
  pub(super) fn create_cube_draw_target(
    &mut self,
    id: u64,
    size: u32,
    spec: TargetSpec,
    depth: DepthStorage,
    format: TextureFormat,
  ) -> Result<(), String> {
    let shader =
      ShaderTexture::new_cube_draw_target(&self.gl, size, depth, spec.clear_color, spec.sampler.mipmap, format)?
        .with_sampler(spec.sampler)
        .with_manual(true)
        .with_load(spec.load);
    let gpu = GpuTexture {
      gl_texture: shader.gl_texture(),
      width: size,
      height: size,
      shape: TextureShape::Cube,
      sampler: spec.sampler,
      format,
      label: spec.label,
    };
    shader.clear(&self.gl);
    self.textures.insert(id, gpu);
    self.shaders.insert(id, shader);
    Ok(())
  }

  /// Create a sub-target of draw target `parent` under `id` (see
  /// `RasterCmd::CreateSubTarget`): in the shader map and the parent's
  /// group list, never in the texture map. Starts dirty like every
  /// flush-rendered target; the parent renders it at the next flush.
  pub(super) fn create_sub_target(
    &mut self,
    id: u64,
    parent: u64,
    x: i32,
    y: i32,
    spec: TargetSpec,
  ) -> Result<(), String> {
    let parent_shader = self.shaders.get(&parent).ok_or_else(|| format!("target {parent} not found"))?;
    let shader = ShaderTexture::new_sub_target(
      parent_shader,
      parent,
      x,
      y,
      spec.width,
      spec.height,
      spec.clear_color,
      spec.label,
    )?;
    self.shaders.insert(id, shader);
    self.regions.entry(parent).or_default().push(id);
    self.dirty.insert(id);
    Ok(())
  }

  /// Add a draw entry to a draw target (see `RasterCmd::AddDraw`). The UI
  /// side validated everything against its mirrors; a failure here means the
  /// mirrors diverged.
  pub(super) fn add_draw(
    &mut self,
    target: u64,
    draw: u64,
    entry: DrawSpec,
    before: Option<u64>,
  ) -> Result<(), String> {
    let pipeline = self
      .render_pipelines
      .get(&entry.pipeline)
      .ok_or_else(|| format!("pipeline {} not found", entry.pipeline))?
      .clone();
    let buffers = resolve_entry_buffers(&self.buffers, entry.buffer_ids())?;
    let shader = self.shaders.get_mut(&target).ok_or_else(|| format!("shader texture {target} not found"))?;
    shader.add_entry(
      &self.gl,
      draw,
      pipeline,
      Some(entry.pipeline),
      buffers,
      entry.draw,
      entry.params,
      entry.textures,
      before,
    )?;
    if !shader.manual() {
      self.dirty.insert(target);
    }
    Ok(())
  }

  /// Apply a write to a shader target's record and mark it dirty (a manual
  /// target only folds - its pixels change on its next explicit render, not
  /// here), warning on failure like every fire-and-forget write. The one
  /// doorway for every fire-and-forget target mutation, per-entry or
  /// target-level.
  pub(super) fn entry_write(
    &mut self,
    target: u64,
    what: &str,
    write: impl FnOnce(&glow::Context, &mut ShaderTexture) -> Result<(), String>,
  ) {
    let result = self
      .shaders
      .get_mut(&target)
      .ok_or_else(|| format!("shader texture {target} not found"))
      .and_then(|shader| write(&self.gl, shader).map(|()| shader.manual()));
    match result {
      Ok(manual) => {
        if !manual {
          self.dirty.insert(target);
        }
      }
      Err(e) => log::warn!("[alloy] {what} failed: {e}"),
    }
  }

  /// Adopt a new shader/pipeline target into Impeller and record it under
  /// `id` in both the texture and shader maps. The target starts dirty: its
  /// first render happens at the next flush, before anything observes its
  /// pixels, so the blocking create RPC never pays for a draw. A manual
  /// target is cleared instead: its pass runs only on RenderTarget, and the
  /// clear is what keeps undefined storage from ever being observable.
  /// After a resize: a manual target cannot preserve accumulated history
  /// (new storage), so clear it and the app re-seeds from defined pixels -
  /// the flush will not render it, so the clear happens here. Either way
  /// the new storage renders (and its samplers re-resolve) at the next
  /// flush, before anything observes it; for a manual target the dirty
  /// seed only re-renders its samplers against the new (cleared) name.
  fn reseed_resized(&mut self, id: u64) {
    if let Some(shader) = self.shaders.get(&id) {
      if shader.manual() {
        shader.clear(&self.gl);
      }
    }
    self.dirty.insert(id);
  }

  /// Register a 2D target whose format Impeller does not adopt (see
  /// `create_draw_target`): in the texture map for sampling, in the shader
  /// map for rendering, and in `unadopted` so `release_texture` deletes
  /// the name.
  fn register_unadopted_target(
    &mut self,
    id: u64,
    shader: ShaderTexture,
    width: u32,
    height: u32,
    format: TextureFormat,
    label: Option<String>,
  ) {
    let gpu = GpuTexture {
      gl_texture: shader.gl_texture(),
      width,
      height,
      shape: TextureShape::D2,
      sampler: shader.sampler(),
      format,
      label,
    };
    self.textures.insert(id, gpu);
    self.unadopted.insert(id);
    if shader.manual() {
      shader.clear(&self.gl);
    } else {
      self.dirty.insert(id);
    }
    self.shaders.insert(id, shader);
  }

  fn register_shader_target(
    &mut self,
    id: u64,
    shader: ShaderTexture,
    width: u32,
    height: u32,
    label: Option<String>,
    adopt_err: &str,
  ) -> Result<Texture, String> {
    let size = ISize::new(width as i64, height as i64);
    let gpu = GpuTexture {
      gl_texture: shader.gl_texture(),
      width,
      height,
      shape: TextureShape::D2,
      sampler: shader.sampler(),
      format: TextureFormat::Rgba8,
      label,
    };
    match gl::adopt_texture(&gpu, &self.impeller_ctx, size) {
      Some(impeller) => {
        self.textures.insert(id, gpu);
        if shader.manual() {
          shader.clear(&self.gl);
        } else {
          self.dirty.insert(id);
        }
        self.shaders.insert(id, shader);
        Ok(impeller)
      }
      None => {
        shader.destroy(&self.gl);
        unsafe { glow::HasContext::delete_texture(&self.gl, gpu.gl_texture) };
        Err(adopt_err.to_string())
      }
    }
  }

  /// Overwrite manual target `dst` with texture `src`'s current pixels via
  /// the shared copy program - a fullscreen sampling draw into dst's FBO,
  /// never a blit. The UI side validated ids, sizes, and dst's manual mode;
  /// a miss here means the mirrors diverged. Counts as a pass into dst (it
  /// occupies the thread like one) and seeds dst into the dirty set so
  /// targets sampling it re-render at the next flush.
  pub(super) fn copy_texture(&mut self, src: u64, dst: u64) -> Result<(), String> {
    let program = ensure_copy_program(&self.gl, &mut self.copy_program)?;
    let gpu = self.textures.get(&src).ok_or_else(|| format!("texture {src} not found"))?;
    let shader = self.shaders.get(&dst).ok_or_else(|| format!("shader texture {dst} not found"))?;
    let input = PassInput::d2("uSrc", gpu.gl_texture, Some(self.samplers.get(gpu.sampler)));
    timed_pass(&self.gl, &mut self.pass_timer, &self.stats, dst, shader, || {
      shader.overwrite_with(&self.gl, &program, &[input]);
    });
    self.dirty.insert(dst);
    Ok(())
  }
}

/// Get (compiling on first use) the tile-clear program (see
/// `pass::TILE_CLEAR_FRAGMENT`); a compile failure is logged once and the
/// slot stays empty, so parents render their tiles without the wipe.
fn ensure_tile_clear_program(gl: &glow::Context, slot: &mut Option<Rc<ShaderProgram>>) -> Option<Rc<ShaderProgram>> {
  if slot.is_none() {
    match ShaderProgram::new_fragment(gl, crate::gl::TILE_CLEAR_FRAGMENT) {
      Ok(program) => *slot = Some(Rc::new(program)),
      Err(e) => log::error!("[alloy] tile clear program failed to compile: {e}"),
    }
  }
  slot.clone()
}

/// The pass-accounting policy around one GPU pass into `target`: the timer
/// query pair, issue-time measurement, the per-target record, and the
/// cumulative stats - one owner, so every pass producer counts identically.
/// A free function over the disjoint fields (not a method) because callers
/// hold shared borrows of the registries while the timer needs `&mut`.
fn timed_pass(
  gl: &glow::Context,
  timer: &mut PassTimer,
  stats: &RasterStats,
  target: u64,
  shader: &ShaderTexture,
  pass: impl FnOnce(),
) {
  let start = std::time::Instant::now();
  timer.begin(gl);
  pass();
  timer.end(gl, Timed::Pass { target });
  let micros = start.elapsed().as_micros() as u64;
  shader.record_pass(micros);
  stats.passes.fetch_add(1, Ordering::Relaxed);
  stats.pass_issue_micros.fetch_add(micros, Ordering::Relaxed);
}

/// Which shader targets need re-rendering after the contents of the `dirty`
/// ids changed, given the sampler graph `edges` (target id -> the ids it
/// samples, with multiplicity): every target that is itself dirty or samples
/// a dirty/affected id, in dependency order - sources before the targets
/// sampling them - so one pass over the result renders a chain end to end.
/// Targets on a sampling cycle cannot be ordered and come back in the second
/// list. Both lists are deterministic (ascending id per Kahn layer) for a
/// given input. Pure over the id graph, so it unit-tests without GL.
pub(crate) fn propagation_order(dirty: &HashSet<u64>, edges: &HashMap<u64, Vec<u64>>) -> (Vec<u64>, Vec<u64>) {
  use std::collections::BTreeSet;
  // Affected = fixpoint of "dirty target, or samples a dirty/affected id".
  let mut affected: BTreeSet<u64> = BTreeSet::new();
  loop {
    let before = affected.len();
    for (id, sources) in edges {
      if !affected.contains(id)
        && (dirty.contains(id) || sources.iter().any(|s| dirty.contains(s) || affected.contains(s)))
      {
        affected.insert(*id);
      }
    }
    if affected.len() == before {
      break;
    }
  }
  // Kahn's algorithm over the affected subgraph: a target is ready once none
  // of its sources are still waiting (sources outside `remaining` are either
  // unaffected or already ordered).
  let mut order = Vec::with_capacity(affected.len());
  let mut remaining = affected;
  loop {
    let ready: Vec<u64> =
      remaining.iter().copied().filter(|id| edges[id].iter().all(|s| !remaining.contains(s))).collect();
    if ready.is_empty() {
      break;
    }
    for id in ready {
      remaining.remove(&id);
      order.push(id);
    }
  }
  (order, remaining.into_iter().collect())
}

/// Map a binding list to live GL textures, each with the sampler object for
/// the source's declared state under the binding's override, dropping any
/// id no longer registered (it samples as unbound/black). The resolver a
/// target's render calls per pass - once for a fragment target, once per
/// entry for a mesh target. A uniform the program declares as
/// `sampler2DShadow` gets the comparison sampler instead (hardware LEQUAL
/// compare, 2x2 PCF); it demands a depth texture, and a cube map may only
/// back a `samplerCube` (and vice versa). Both shape rules are rejected at
/// bind on every path, so a mismatch here means the mirrors diverged - drop
/// the input (samples as unbound) and say which.
fn resolve_binding_list(
  textures: &HashMap<u64, GpuTexture>,
  samplers: &SamplerCache,
  bindings: &[TextureBinding],
  program: &ShaderProgram,
) -> Vec<PassInput> {
  bindings
    .iter()
    .filter_map(|b| {
      let gpu = textures.get(&b.id)?;
      let kind = program.uniform_kind(&b.name);
      let compare = kind == Some(UniformKind::Sampler2DShadow);
      if compare && gpu.format != TextureFormat::Depth24 {
        log::warn!("[alloy] sampler2DShadow input '{}': texture {} is not a depth texture; skipped", b.name, b.id);
        return None;
      }
      if kind.and_then(UniformKind::sampler_shape).is_some_and(|wanted| wanted != gpu.shape) {
        log::warn!(
          "[alloy] input '{}': texture {} is a {} texture, the sampler is not; skipped",
          b.name,
          b.id,
          gpu.shape.name()
        );
        return None;
      }
      let sampler = if compare { samplers.compare() } else { samplers.get(gpu.sampler.overridden(&b.sampler)) };
      Some(PassInput { name: b.name.clone(), texture: gpu.gl_texture, sampler: Some(sampler), shape: gpu.shape })
    })
    .collect()
}
