//! Render targets: `ShaderTexture` (an FBO-backed target texture rendered as
//! a fullscreen fragment pass or as an ordered list of mesh draws) and the
//! retained layer target. This is where per-target state lives - the draw
//! entries with their VAOs, buffers, params and bindings, the target-owned
//! depth storage, and the clear color.

use glow::HasContext;
use std::cell::Cell;
use std::rc::Rc;

use super::entry::{
  build_vao, check_entry_buffers, merge_record, release_entry_buffers, DrawEntry, EntryBuffers, MeshState,
};
use super::pass::{run_pass, DrawGroup, PassDraw, PassInput, ResolvedDraw};
use super::storage::{
  attach_storage, create_depth_texture, create_mesh_storage, create_target_texture, DepthAttachment, MeshStorage, Msaa,
};
use super::program::{release_pipeline, release_program, RenderPipeline, ShaderProgram};
use crate::gpu::resources::GpuDrawInfo;
use crate::gpu::spec::DepthStorage;
use crate::gpu::vocab::{
  blend_name, cull_name, merge_bindings, validate_order, AttrFormat, DrawRange, ParamValue, PipelineDesc,
  TextureBinding,
};
use super::prev_framebuffer;

/// A sub-target's place in its parent: a draw target created with
/// `into` renders into a rectangle of `parent`'s storage instead of owning
/// any. `x`/`y` are the tile's top-left origin in the parent, in the
/// parent's image space (row 0 = top, like `srcX`/`srcY` on the texture
/// leaf). That is GL's viewport space unflipped: a target's memory row 0 is
/// its displayed top (see the readback contract), which is exactly why
/// meshes draw with y negated. `label` is the create's debug name (a tile
/// has no texture entry to hold one).
pub struct Region {
  pub parent: u64,
  pub x: i32,
  pub y: i32,
  pub label: Option<String>,
}

/// Which kind of pass renders this target.
pub(super) enum TargetKind {
  /// A fullscreen fragment pass: one program with target-level params and
  /// bindings. No clear, depth, or draw list - the covering triangle writes
  /// every pixel.
  Fragment { program: Rc<ShaderProgram>, params: Vec<(String, ParamValue)>, bindings: Vec<TextureBinding> },
  /// A vertex+fragment mesh target: clear + the ordered draw list.
  Mesh(MeshState),
}

/// An FBO-backed RGBA8 target texture rendered by shader passes: either a
/// fullscreen fragment pass or an ordered list of mesh draws. The target's GL
/// name is also adopted into Impeller (and held in the TextureRegistry); this
/// struct keeps the FBO and draw state so the same texture can be re-rendered
/// with new params. Like GpuTexture it never deletes the target name:
/// Impeller owns it once adopted, and deleting here would double-free.
pub struct ShaderTexture {
  kind: TargetKind,
  fbo: glow::Framebuffer,
  target: glow::Texture,
  width: u32,
  height: u32,
  /// Declared sampling for this target's output (how OTHER passes and the
  /// display draw sample it; the target's own inputs carry their own states).
  /// Survives resize; set via `with_sampler` after construction.
  sampler: crate::gpu::SamplerState,
  /// Manual render mode (see `TargetSpec::manual`): the dirty flush never
  /// renders this target, only an explicit RenderTarget command does. Set via
  /// `with_manual` after construction.
  manual: bool,
  /// Cumulative passes rendered into this target and their wall time in
  /// microseconds, recorded by the owner around each render (raster-thread
  /// occupancy, not GPU-side duration; see raster::RasterStats). Survives
  /// resize, dies with the target. Cell because renders take &self.
  passes: Cell<u64>,
  pass_issue_micros: Cell<u64>,
  /// GPU-side execution time of those passes, microseconds, credited by the
  /// owner as timer queries retire (see PassTimer).
  pass_exec_micros: Cell<u64>,
  /// Some = a sub-target (see `Region`): `fbo` and `target` are the
  /// parent's names, borrowed for bookkeeping only - never rendered through
  /// here (the parent renders the tile as a group of its own pass) and
  /// never deleted here. `width`/`height` are the tile's size.
  region: Option<Region>,
}

impl ShaderTexture {
  pub fn new(
    gl: &glow::Context,
    width: u32,
    height: u32,
    fragment_src: &str,
    sampler_bindings: Vec<TextureBinding>,
  ) -> Result<Self, String> {
    let program = Rc::new(ShaderProgram::new_fragment(gl, fragment_src)?);
    Self::from_fragment_program(gl, program, width, height, sampler_bindings).map_err(|(program, e)| {
      release_program(gl, program);
      e
    })
  }

  /// A fullscreen fragment target over an already-compiled program. On error
  /// the program Rc is handed back so the caller decides its fate.
  pub fn from_fragment_program(
    gl: &glow::Context,
    program: Rc<ShaderProgram>,
    width: u32,
    height: u32,
    sampler_bindings: Vec<TextureBinding>,
  ) -> Result<Self, (Rc<ShaderProgram>, String)> {
    if program.is_pipeline() {
      return Err((program, "program is a pipeline; the target needs a render pipeline".to_string()));
    }
    let MeshStorage { target, fbo, .. } = match create_mesh_storage(gl, width, height, DepthStorage::None, 1) {
      Ok(storage) => storage,
      Err(e) => return Err((program, e)),
    };
    {
      Ok(ShaderTexture {
        kind: TargetKind::Fragment { program, params: Vec::new(), bindings: sampler_bindings },
        fbo,
        target,
        width,
        height,
        sampler: crate::gpu::SamplerState::default(),
        manual: false,
        passes: Cell::new(0),
        pass_issue_micros: Cell::new(0),
        pass_exec_micros: Cell::new(0),
        region: None,
      })
    }
  }

  /// The fused create path: compile a vertex+fragment pair, wrap it in an
  /// anonymous pipeline, and build a one-entry target over it in one step.
  #[allow(clippy::too_many_arguments)]
  pub fn new_pipeline(
    gl: &glow::Context,
    width: u32,
    height: u32,
    vertex_src: &str,
    fragment_src: &str,
    sampler_bindings: Vec<TextureBinding>,
    desc: PipelineDesc,
    buffers: EntryBuffers,
    draw: DrawRange,
    clear_color: [f32; 4],
    samples: u32,
  ) -> Result<Self, String> {
    let program = Rc::new(ShaderProgram::new_pipeline(gl, vertex_src, fragment_src)?);
    let pipeline = match RenderPipeline::new(program, None, desc) {
      Ok(p) => Rc::new(p),
      Err((program, e)) => {
        release_program(gl, program);
        return Err(e);
      }
    };
    Self::from_pipeline(gl, pipeline, None, width, height, sampler_bindings, buffers, draw, clear_color, samples)
      .map_err(|(pipeline, e)| {
        release_pipeline(gl, pipeline);
        e
      })
  }

  /// A fixed single-entry target over a render pipeline: the pipeline's
  /// vertex and instance layouts are bound to this target's concrete buffers
  /// in a fresh VAO, and the pipeline's depth state gives the target its
  /// private depth storage. On error the pipeline Rc is handed back so the
  /// caller decides its fate (a fused create releases it, a shared pipeline
  /// stays registered).
  #[allow(clippy::too_many_arguments)]
  pub fn from_pipeline(
    gl: &glow::Context,
    pipeline: Rc<RenderPipeline>,
    pipeline_id: Option<u64>,
    width: u32,
    height: u32,
    sampler_bindings: Vec<TextureBinding>,
    buffers: EntryBuffers,
    draw: DrawRange,
    clear_color: [f32; 4],
    samples: u32,
  ) -> Result<Self, (Rc<RenderPipeline>, String)> {
    if let Err(e) = check_entry_buffers(&pipeline.desc, &buffers) {
      return Err((pipeline, e));
    }
    let depth = if pipeline.desc.depth.is_some() { DepthStorage::Buffer } else { DepthStorage::None };
    let storage = match create_mesh_storage(gl, width, height, depth, samples) {
      Ok(storage) => storage,
      Err(e) => return Err((pipeline, e)),
    };

    unsafe {
      let vao = match build_vao(gl, &pipeline.program, &pipeline.desc, &buffers) {
        Ok(vao) => vao,
        Err(e) => {
          storage.delete(gl);
          return Err((pipeline, e));
        }
      };
      let MeshStorage { target, fbo, depth, msaa } = storage;

      let entry =
        DrawEntry { id: 0, pipeline, pipeline_id, vao, buffers, draw, params: Vec::new(), bindings: sampler_bindings };
      Ok(ShaderTexture {
        kind: TargetKind::Mesh(MeshState {
          entries: vec![entry],
          shared_params: Vec::new(),
          shared_bindings: Vec::new(),
          depth,
          msaa,
          clear_color,
          load: false,
          fixed: true,
        }),
        fbo,
        target,
        width,
        height,
        sampler: crate::gpu::SamplerState::default(),
        manual: false,
        passes: Cell::new(0),
        pass_issue_micros: Cell::new(0),
        pass_exec_micros: Cell::new(0),
        region: None,
      })
    }
  }

  /// A mesh target with an empty, mutable draw list (`create_draw_target`):
  /// color storage plus optional target-owned depth storage, rendered as
  /// clear + entries in list order. Entries arrive via `add_entry`; with none
  /// the render is the clear alone.
  pub fn new_draw_target(
    gl: &glow::Context,
    width: u32,
    height: u32,
    depth: DepthStorage,
    clear_color: [f32; 4],
    samples: u32,
  ) -> Result<Self, String> {
    let MeshStorage { target, fbo, depth, msaa } = create_mesh_storage(gl, width, height, depth, samples)?;
    Ok(ShaderTexture {
      kind: TargetKind::Mesh(MeshState {
        entries: Vec::new(),
        shared_params: Vec::new(),
        shared_bindings: Vec::new(),
        depth,
        msaa,
        clear_color,
        load: false,
        fixed: false,
      }),
      fbo,
      target,
      width,
      height,
      sampler: crate::gpu::SamplerState::default(),
      manual: false,
      passes: Cell::new(0),
      pass_issue_micros: Cell::new(0),
      pass_exec_micros: Cell::new(0),
      region: None,
    })
  }

  /// A sub-target: a draw target with an empty, mutable draw list that
  /// renders into the `width` x `height` rectangle at `(x, y)` (top-left
  /// origin) of `parent`'s storage. It shares the parent's depth storage
  /// (entries test against it, and the tile's clear wipes its rectangle of
  /// it) and multisampling (the parent's resolve covers the whole storage),
  /// and allocates nothing of its own: the parent's GL names are copied for
  /// bookkeeping only (see `region`). Errs when the parent is not a mesh
  /// target or is itself a sub-target.
  pub fn new_sub_target(
    parent: &ShaderTexture,
    parent_id: u64,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    clear_color: [f32; 4],
    label: Option<String>,
  ) -> Result<Self, String> {
    if parent.region.is_some() {
      return Err(format!("target {parent_id} is itself a sub-target; tiles do not nest"));
    }
    let Some(parent_mesh) = parent.mesh() else {
      return Err(format!("target {parent_id} is not a draw target"));
    };
    Ok(ShaderTexture {
      kind: TargetKind::Mesh(MeshState {
        entries: Vec::new(),
        shared_params: Vec::new(),
        shared_bindings: Vec::new(),
        depth: parent_mesh.depth,
        msaa: None,
        clear_color,
        load: false,
        fixed: false,
      }),
      fbo: parent.fbo,
      target: parent.target,
      width,
      height,
      sampler: parent.sampler,
      manual: false,
      passes: Cell::new(0),
      pass_issue_micros: Cell::new(0),
      pass_exec_micros: Cell::new(0),
      region: Some(Region { parent: parent_id, x, y, label }),
    })
  }

  /// The sub-target marker: Some when this target renders into a rectangle
  /// of another's storage.
  pub fn region(&self) -> Option<&Region> {
    self.region.as_ref()
  }

  /// Move and resize a sub-target's rectangle (top-left origin). Errs on a
  /// target that owns its storage - those resize through `resize`.
  pub fn set_region_rect(&mut self, x: i32, y: i32, width: u32, height: u32) -> Result<(), String> {
    let Some(region) = self.region.as_mut() else {
      return Err("target is not a sub-target".to_string());
    };
    region.x = x;
    region.y = y;
    self.width = width;
    self.height = height;
    Ok(())
  }

  /// Set the declared sampling for this target's output (builder-style, right
  /// after construction).
  pub fn with_sampler(mut self, sampler: crate::gpu::SamplerState) -> Self {
    self.sampler = sampler;
    self
  }

  /// Set the render mode (builder-style, right after construction); see the
  /// `manual` field.
  pub fn with_manual(mut self, manual: bool) -> Self {
    self.manual = manual;
    self
  }

  /// Set the color load op (builder-style, right after construction); see
  /// `TargetSpec::load`. A no-op on fragment targets, which have no mesh
  /// state (and cannot be manual anyway).
  pub fn with_load(mut self, load: bool) -> Self {
    if let TargetKind::Mesh(mesh) = &mut self.kind {
      mesh.load = load;
    }
    self
  }

  fn mesh(&self) -> Option<&MeshState> {
    match &self.kind {
      TargetKind::Mesh(mesh) => Some(mesh),
      TargetKind::Fragment { .. } => None,
    }
  }

  fn entry0(&self) -> Option<&DrawEntry> {
    self.mesh().and_then(|m| m.entries.first())
  }

  /// The first entry's full draw record (see `GpuDrawInfo`) - the one
  /// accessor behind every "first entry" question the inventory asks of the
  /// fixed single-entry kinds; None on a fragment-only shader. Same data
  /// `draw_infos` reports per entry.
  pub fn entry0_info(&self) -> Option<GpuDrawInfo> {
    self.draw_infos().into_iter().next()
  }

  /// Whether the target draws over its previous contents (loadOp "load").
  pub fn load(&self) -> bool {
    self.mesh().is_some_and(|m| m.load)
  }

  pub fn sampler(&self) -> crate::gpu::SamplerState {
    self.sampler
  }

  /// Whether the target renders only on an explicit RenderTarget command.
  pub fn manual(&self) -> bool {
    self.manual
  }

  pub fn gl_texture(&self) -> glow::Texture {
    self.target
  }

  /// The target's output size (a sub-target's: its rectangle's).
  pub fn size(&self) -> (u32, u32) {
    (self.width, self.height)
  }

  /// Registry id of the shared program behind the first entry's pipeline;
  /// None for fragment targets and for the fused create path, whose program
  /// is anonymous.
  pub fn program_id(&self) -> Option<u64> {
    self.entry0().and_then(|e| e.pipeline.program_id)
  }

  /// Whether this is a mesh target (vs a fullscreen fragment pass).
  pub fn is_pipeline(&self) -> bool {
    self.mesh().is_some()
  }

  /// Whether this is a draw target: a mesh target whose entry list mutates
  /// via add/remove (vs the fixed single-entry creates).
  pub fn is_draw_list(&self) -> bool {
    self.mesh().is_some_and(|m| !m.fixed)
  }

  /// Whether any draw entry fetches from buffer `id` - as its vertex, index,
  /// or instance buffer: buffer writes re-render the targets this returns
  /// true for.
  pub fn reads_buffer(&self, id: u64) -> bool {
    self.mesh().is_some_and(|m| {
      m.entries.iter().any(|e| {
        e.buffers.vertex.as_ref().is_some_and(|(_, bid)| *bid == id)
          || e.buffers.index.as_ref().is_some_and(|(_, iid, _)| *iid == id)
          || e.buffers.instances.iter().any(|(_, iid)| *iid == id)
      })
    })
  }

  /// The first entry's declared interleaved attribute layout; empty for
  /// fragment-only shaders and attributeless pipelines.
  pub fn attributes(&self) -> &[(String, AttrFormat)] {
    self.entry0().map(|e| e.pipeline.desc.attributes.as_slice()).unwrap_or(&[])
  }

  /// The first entry's declared per-instance layout; empty when its
  /// pipeline declares none (and for fragment-only shaders).
  pub fn instance_attributes(&self) -> &[(String, AttrFormat, u32)] {
    self.entry0().map(|e| e.pipeline.desc.instance_attributes.as_slice()).unwrap_or(&[])
  }

  /// Whether the target owns depth storage.
  pub fn has_depth(&self) -> bool {
    self.mesh().is_some_and(|m| m.depth.is_some())
  }

  /// The GL name of the target's depth TEXTURE (`DepthStorage::Texture`);
  /// None for renderbuffer depth and depthless targets. What the owner
  /// registers under the depth id, and re-registers after every resize (a
  /// resize allocates a fresh name, see `resize`).
  pub fn depth_texture(&self) -> Option<glow::Texture> {
    match self.mesh().and_then(|m| m.depth) {
      Some(DepthAttachment::Texture(tex)) => Some(tex),
      _ => None,
    }
  }

  /// The effective multisample count (1 = single-sample), after clamping
  /// and any fallback at creation.
  pub fn samples(&self) -> u32 {
    self.mesh().and_then(|m| m.msaa.as_ref()).map_or(1, |m| m.samples() as u32)
  }

  /// The FBO a mesh pass draws into: the explicit multisample FBO when the
  /// target has one, else the texture's own.
  fn draw_fbo(&self) -> glow::Framebuffer {
    match self.mesh().and_then(|m| m.msaa.as_ref()) {
      Some(Msaa::Explicit { fbo, .. }) => *fbo,
      _ => self.fbo,
    }
  }

  /// The tail of every content write (render, overwrite, clear): the MSAA
  /// resolve when the target has one, then the mip regeneration when the id
  /// declares a chain.
  fn resolve(&self, gl: &glow::Context) {
    self.resolve_msaa(gl);
    // The chain serves the NEXT consumer of this target (another pass
    // sampling it minified), so it follows every content write: the
    // automatic regeneration the dirty flush makes possible.
    if self.sampler.mipmap {
      super::texture::generate_mipmap(gl, self.target);
    }
  }

  /// After a pass on an `Msaa::Explicit` target: blit the multisampled color
  /// into the texture (the resolve), then drop the samples - they are dead
  /// once resolved, and the invalidate keeps tilers from writing them back.
  /// A no-op for the other flavors. Restores the framebuffer bindings.
  fn resolve_msaa(&self, gl: &glow::Context) {
    let Some(Msaa::Explicit { fbo: msaa_fbo, .. }) = self.mesh().and_then(|m| m.msaa.as_ref()) else {
      return;
    };
    unsafe {
      let prev_read = gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING);
      let prev_draw = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
      let scissor = gl.is_enabled(glow::SCISSOR_TEST);
      let (w, h) = (self.width as i32, self.height as i32);
      // The blit honours the scissor, which Impeller may have left enabled.
      gl.disable(glow::SCISSOR_TEST);
      gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(*msaa_fbo));
      gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(self.fbo));
      gl.blit_framebuffer(0, 0, w, h, 0, 0, w, h, glow::COLOR_BUFFER_BIT, glow::NEAREST);
      if crate::gl::supports_invalidate(gl) {
        let attachments: &[u32] = if self.has_depth() {
          &[glow::COLOR_ATTACHMENT0, glow::DEPTH_ATTACHMENT]
        } else {
          &[glow::COLOR_ATTACHMENT0]
        };
        gl.invalidate_framebuffer(glow::READ_FRAMEBUFFER, attachments);
      }
      if scissor {
        gl.enable(glow::SCISSOR_TEST);
      }
      gl.bind_framebuffer(glow::READ_FRAMEBUFFER, prev_framebuffer(prev_read));
      gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, prev_framebuffer(prev_draw));
    }
  }

  /// Set the first entry's draw range (resolved and validated UI-side, see
  /// `Context::set_draw`): the single-draw targets' setDraw. Errors on a
  /// fragment-only shader (its fullscreen triangle is fixed).
  pub fn set_draw(&mut self, range: DrawRange) -> Result<(), String> {
    match &mut self.kind {
      TargetKind::Fragment { .. } => Err("not a pipeline texture".to_string()),
      TargetKind::Mesh(mesh) => match mesh.entries.first_mut() {
        Some(entry) => {
          entry.draw = range;
          Ok(())
        }
        None => Err("target has no draw entries".to_string()),
      },
    }
  }

  /// Swap an entry's buffers (validated UI-side, backstopped here; `draw`
  /// None = entry 0, the single-draw kinds' one entry): the VAO is rebuilt
  /// against the new buffers - a VAO captures its buffers at build time, so
  /// a swap is a rebuild - and the replaced buffers released (deleted when
  /// this was their last use). The entry's draw range is untouched; the UI
  /// side has already checked it against the new buffers.
  pub fn set_entry_buffers(
    &mut self,
    gl: &glow::Context,
    draw: Option<u64>,
    buffers: EntryBuffers,
  ) -> Result<(), String> {
    let entry = match draw {
      Some(id) => self.entry_mut(id)?,
      None => match &mut self.kind {
        TargetKind::Fragment { .. } => return Err("not a pipeline texture".to_string()),
        TargetKind::Mesh(mesh) => mesh.entries.first_mut().ok_or_else(|| "target has no draw entries".to_string())?,
      },
    };
    check_entry_buffers(&entry.pipeline.desc, &buffers)?;
    let vao = build_vao(gl, &entry.pipeline.program, &entry.pipeline.desc, &buffers)?;
    unsafe { gl.delete_vertex_array(entry.vao) };
    entry.vao = vao;
    let previous = std::mem::replace(&mut entry.buffers, buffers);
    release_entry_buffers(gl, previous);
    Ok(())
  }

  /// Add a draw entry to a draw target's list (see `DrawEntry`; validated
  /// UI-side, backstopped here): appended - drawing last in list order - or
  /// inserted immediately before entry `before` when given.
  #[allow(clippy::too_many_arguments)]
  pub fn add_entry(
    &mut self,
    gl: &glow::Context,
    id: u64,
    pipeline: Rc<RenderPipeline>,
    pipeline_id: Option<u64>,
    buffers: EntryBuffers,
    draw: DrawRange,
    params: Vec<(String, ParamValue)>,
    bindings: Vec<TextureBinding>,
    before: Option<u64>,
  ) -> Result<(), String> {
    let TargetKind::Mesh(mesh) = &mut self.kind else {
      return Err("not a draw target".to_string());
    };
    if mesh.fixed {
      return Err("target's draw list is fixed (created single-draw)".to_string());
    }
    if pipeline.desc.depth.is_some() && mesh.depth.is_none() {
      return Err(
        "pipeline tests depth but the target has no depth buffer (create the draw target with depth: true)".to_string(),
      );
    }
    check_entry_buffers(&pipeline.desc, &buffers)?;
    let position = match before {
      Some(before_id) => Some(
        mesh
          .entries
          .iter()
          .position(|e| e.id == before_id)
          .ok_or_else(|| format!("draw {before_id} (before) not found"))?,
      ),
      None => None,
    };
    let vao = build_vao(gl, &pipeline.program, &pipeline.desc, &buffers)?;
    let entry = DrawEntry { id, pipeline, pipeline_id, vao, buffers, draw, params, bindings };
    match position {
      Some(pos) => mesh.entries.insert(pos, entry),
      None => mesh.entries.push(entry),
    }
    Ok(())
  }

  /// Reorder the draw list to `order`, which must be a full permutation of
  /// the current entry ids (validated UI-side, backstopped here): every
  /// entry named exactly once. List order is draw order.
  pub fn set_entry_order(&mut self, order: &[u64]) -> Result<(), String> {
    let TargetKind::Mesh(mesh) = &mut self.kind else {
      return Err("not a draw target".to_string());
    };
    if mesh.fixed {
      return Err("target's draw list is fixed (created single-draw)".to_string());
    }
    validate_order(order, mesh.entries.iter().map(|e| e.id))?;
    let index: std::collections::HashMap<u64, usize> = order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    mesh.entries.sort_by_key(|e| index[&e.id]);
    Ok(())
  }

  /// Remove a draw entry by id, releasing its VAO and its uses of the
  /// pipeline and buffer (deleted only when nothing else holds them).
  pub fn remove_entry(&mut self, gl: &glow::Context, id: u64) -> Result<(), String> {
    let TargetKind::Mesh(mesh) = &mut self.kind else {
      return Err("not a draw target".to_string());
    };
    if mesh.fixed {
      return Err("target's draw list is fixed (created single-draw)".to_string());
    }
    let pos = mesh.entries.iter().position(|e| e.id == id).ok_or_else(|| format!("draw {id} not found"))?;
    let entry = mesh.entries.remove(pos);
    unsafe { gl.delete_vertex_array(entry.vao) };
    release_pipeline(gl, entry.pipeline);
    release_entry_buffers(gl, entry.buffers);
    Ok(())
  }

  fn entry_mut(&mut self, id: u64) -> Result<&mut DrawEntry, String> {
    let TargetKind::Mesh(mesh) = &mut self.kind else {
      return Err("not a draw target".to_string());
    };
    mesh.entries.iter_mut().find(|e| e.id == id).ok_or_else(|| format!("draw {id} not found"))
  }

  /// Fold a params update into one entry's record by name (validated UI-side
  /// against the entry's program).
  pub fn merge_entry_params(&mut self, id: u64, params: &[(String, ParamValue)]) -> Result<(), String> {
    merge_record(&mut self.entry_mut(id)?.params, params);
    Ok(())
  }

  /// Fold a params update into a draw target's shared record (see
  /// `MeshState::shared_params`; validated UI-side against the union of the
  /// entries' programs). The fixed single-draw kinds error: their target-level
  /// params are entry 0's, written via `merge_params`.
  pub fn merge_shared_params(&mut self, params: &[(String, ParamValue)]) -> Result<(), String> {
    let TargetKind::Mesh(mesh) = &mut self.kind else {
      return Err("not a draw target".to_string());
    };
    if mesh.fixed {
      return Err("target's draw list is fixed (created single-draw)".to_string());
    }
    merge_record(&mut mesh.shared_params, params);
    Ok(())
  }

  /// A draw target's current shared params (empty for every other kind), for
  /// resource introspection.
  pub fn shared_params(&self) -> &[(String, ParamValue)] {
    match &self.kind {
      TargetKind::Mesh(mesh) => &mesh.shared_params,
      TargetKind::Fragment { .. } => &[],
    }
  }

  /// Fold a sampler-binding update into a draw target's shared record (see
  /// `MeshState::shared_bindings`; validated UI-side - names, sources, unit
  /// budget, cycles). Bindings not named keep their current source. Same
  /// gating as `merge_shared_params`.
  pub fn merge_shared_bindings(&mut self, updates: &[TextureBinding]) -> Result<(), String> {
    let TargetKind::Mesh(mesh) = &mut self.kind else {
      return Err("not a draw target".to_string());
    };
    if mesh.fixed {
      return Err("target's draw list is fixed (created single-draw)".to_string());
    }
    merge_bindings(&mut mesh.shared_bindings, updates);
    Ok(())
  }

  /// A draw target's current shared sampler bindings (empty for every other
  /// kind), for resource introspection.
  pub fn shared_bindings(&self) -> &[TextureBinding] {
    match &self.kind {
      TargetKind::Mesh(mesh) => &mesh.shared_bindings,
      TargetKind::Fragment { .. } => &[],
    }
  }

  /// Set one entry's draw range (resolved and validated UI-side).
  pub fn set_entry_draw(&mut self, id: u64, range: DrawRange) -> Result<(), String> {
    self.entry_mut(id)?.draw = range;
    Ok(())
  }

  /// Rebind one entry's sampler2D inputs by uniform name; bindings not named
  /// keep their current source. Names are validated against the entry's
  /// program before anything changes.
  pub fn set_entry_bindings(&mut self, id: u64, updates: &[TextureBinding]) -> Result<(), String> {
    let entry = self.entry_mut(id)?;
    for b in updates {
      if !entry.pipeline.program.accepts_uniform(&b.name) {
        return Err(format!("no active uniform named '{}'", b.name));
      }
    }
    merge_bindings(&mut entry.bindings, updates);
    Ok(())
  }

  /// A copy of the first pass's current uniform values (fragment, or entry
  /// 0), for the flat resource introspection fields.
  pub fn last_params(&self) -> Vec<(String, ParamValue)> {
    match &self.kind {
      TargetKind::Fragment { params, .. } => params.clone(),
      TargetKind::Mesh(mesh) => mesh.entries.first().map(|e| e.params.clone()).unwrap_or_default(),
    }
  }

  /// Record one executed pass into this target (see the `passes` field).
  pub fn record_pass(&self, micros: u64) {
    self.passes.set(self.passes.get() + 1);
    self.pass_issue_micros.set(self.pass_issue_micros.get() + micros);
  }

  /// Credit GPU-side execution time for a retired pass into this target.
  pub fn record_exec(&self, micros: u64) {
    self.pass_exec_micros.set(self.pass_exec_micros.get() + micros);
  }

  /// (cumulative passes, issue microseconds, GPU execution microseconds)
  /// rendered into this target, for resource introspection.
  pub fn pass_stats(&self) -> (u64, u64, u64) {
    (self.passes.get(), self.pass_issue_micros.get(), self.pass_exec_micros.get())
  }

  /// Recreate the render target at a new size, keeping the compiled programs,
  /// FBO, entries, and draw state; the caller re-renders afterwards. The old
  /// target texture is NOT deleted here: Impeller owns its GL name via the
  /// adopted Texture handle (see register_shader_target), which dies with the
  /// UI side's last reference once the registry entry is replaced. On error
  /// the old target is left attached and the shader stays usable at its
  /// previous size.
  pub fn resize(&mut self, gl: &glow::Context, width: u32, height: u32) -> Result<(), String> {
    if self.region.is_some() {
      return Err("sub-targets resize through set_region_rect".to_string());
    }
    unsafe {
      let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
      let target = create_target_texture(gl, width, height)?;
      // Color, depth and multisample storage must all match or the FBO goes
      // incomplete; on failure size everything back to the old target so the
      // shader keeps rendering at its previous size. The storage view borrows
      // this target's GL names (no ownership: nothing is deleted through it).
      let mesh = self.mesh();
      let old_depth = mesh.and_then(|m| m.depth);
      // A depth texture follows the color target's rule: a fresh name at the
      // new size (the old one is Impeller-owned once adopted, so it is
      // neither respecified nor deleted here). Renderbuffer depth is resized
      // in place by attach_storage.
      let depth = match old_depth {
        Some(DepthAttachment::Texture(_)) => match create_depth_texture(gl, width, height) {
          Ok(tex) => Some(DepthAttachment::Texture(tex)),
          Err(e) => {
            gl.delete_texture(target);
            return Err(e);
          }
        },
        other => other,
      };
      let mut storage = MeshStorage {
        target,
        fbo: self.fbo,
        depth,
        msaa: mesh.and_then(|m| m.msaa.as_ref()).map(|m| match m {
          Msaa::InTile { fns, samples } => Msaa::InTile { fns, samples: *samples },
          Msaa::Explicit { fbo, color, samples } => Msaa::Explicit { fbo: *fbo, color: *color, samples: *samples },
        }),
      };
      let result = attach_storage(gl, &storage, width, height);
      if let Err(e) = &result {
        storage.target = self.target;
        storage.depth = old_depth;
        if let Err(rollback) = attach_storage(gl, &storage, self.width, self.height) {
          log::error!("[shader] resize rollback failed ({rollback}) after: {e}");
        }
      }
      let result = result.map_err(|e| format!("shader framebuffer incomplete after resize: {e}"));
      gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
      if let Err(e) = result {
        gl.delete_texture(target);
        if let (Some(DepthAttachment::Texture(tex)), true) = (depth, depth != old_depth) {
          gl.delete_texture(tex);
        }
        return Err(e);
      }
      self.target = target;
      self.width = width;
      self.height = height;
      if let TargetKind::Mesh(m) = &mut self.kind {
        m.depth = depth;
      }
      Ok(())
    }
  }

  /// Release GL resources owned by this target (FBO, depth renderbuffer, and
  /// every entry's VAO), and drop its uses of pipelines, programs, and vertex
  /// buffers - which delete the underlying GL objects only when nothing else
  /// (a registry, another target) still holds them. The target texture is NOT
  /// deleted here, and neither is a depth texture: Impeller owns both via
  /// the adopted Texture handles in the TextureRegistry, and those handles
  /// are responsible for deletion (a registration that never adopted the
  /// depth texture deletes it itself, see the raster owner).
  pub fn destroy(self, gl: &glow::Context) {
    let tile = self.region.is_some();
    match self.kind {
      TargetKind::Fragment { program, .. } => release_program(gl, program),
      TargetKind::Mesh(mesh) => {
        for entry in mesh.entries {
          unsafe { gl.delete_vertex_array(entry.vao) };
          release_pipeline(gl, entry.pipeline);
          release_entry_buffers(gl, entry.buffers);
        }
        // A sub-target's storage names are the parent's (see `region`).
        if tile {
          return;
        }
        if let Some(DepthAttachment::Buffer(rb)) = mesh.depth {
          unsafe { gl.delete_renderbuffer(rb) };
        }
        if let Some(Msaa::Explicit { fbo, color, .. }) = mesh.msaa {
          unsafe {
            gl.delete_framebuffer(fbo);
            gl.delete_renderbuffer(color);
          }
        }
      }
    }
    unsafe { gl.delete_framebuffer(self.fbo) };
  }

  /// The active uniforms of this target's program (fragment, or entry 0 -
  /// the two fused creates are single-entry by construction), for the create
  /// replies that seed the UI-side validation mirror.
  pub fn uniform_table(&self) -> crate::gpu::vocab::UniformTable {
    match &self.kind {
      TargetKind::Fragment { program, .. } => program.uniform_table(),
      TargetKind::Mesh(mesh) => mesh.entries.first().map(|e| e.pipeline.program.uniform_table()).unwrap_or_default(),
    }
  }

  /// The sampler2D inputs of the first pass (fragment, or entry 0), as
  /// (uniform name, source texture id): the flat introspection view and the
  /// create-time validation input.
  pub fn sampler_bindings(&self) -> &[TextureBinding] {
    match &self.kind {
      TargetKind::Fragment { bindings, .. } => bindings,
      TargetKind::Mesh(mesh) => mesh.entries.first().map(|e| e.bindings.as_slice()).unwrap_or(&[]),
    }
  }

  /// Every source texture id any pass of this target samples: the fragment
  /// bindings, or the union over all draw entries plus the shared bindings.
  /// What the flush graph and the propagation walk read as this target's
  /// incoming edges. A shared binding counts even while no entry's program
  /// declares its name - conservative (at worst an extra re-render), and it
  /// matches the UI-side sampler-graph mirror.
  pub fn binding_sources(&self) -> Vec<u64> {
    match &self.kind {
      TargetKind::Fragment { bindings, .. } => bindings.iter().map(|b| b.id).collect(),
      TargetKind::Mesh(mesh) => mesh
        .entries
        .iter()
        .flat_map(|e| e.bindings.iter().map(|b| b.id))
        .chain(mesh.shared_bindings.iter().map(|b| b.id))
        .collect(),
    }
  }

  /// Per-entry introspection for the resource inventory's `draws` list;
  /// empty for fragment targets.
  pub fn draw_infos(&self) -> Vec<GpuDrawInfo> {
    self
      .mesh()
      .map(|m| {
        m.entries
          .iter()
          .map(|e| GpuDrawInfo {
            id: e.id,
            pipeline_id: e.pipeline_id,
            buffer_id: e.buffers.vertex.as_ref().map(|(_, id)| *id),
            index_buffer_id: e.buffers.index.as_ref().map(|(_, iid, _)| *iid),
            index_format: e.buffers.index.as_ref().map(|(_, _, fmt)| fmt.name()),
            instance_buffer_ids: e.buffers.instances.iter().map(|(_, id)| *id).collect(),
            topology: e.pipeline.desc.topology.name(),
            blend: blend_name(e.pipeline.desc.blend),
            cull: cull_name(e.pipeline.desc.cull),
            depth_write: e.pipeline.desc.depth.map_or(true, |d| d.write),
            first_vertex: e.draw.first_vertex,
            vertex_count: e.draw.vertex_count,
            instance_count: e.draw.instance_count,
            params: e.params.clone(),
            textures: e.bindings.clone(),
          })
          .collect()
      })
      .unwrap_or_default()
  }

  /// Rebind the first pass's sampler2D inputs by uniform name (fragment, or
  /// entry 0: the single-draw update path); bindings not named keep their
  /// current source, and a name without an existing binding is added (a
  /// declared sampler left unbound at creation). Every name is validated
  /// against the program's active uniforms before anything changes, so a
  /// failed call leaves all bindings intact. The caller re-renders afterwards.
  pub fn set_sampler_bindings(&mut self, updates: &[TextureBinding]) -> Result<(), String> {
    {
      let program = match &self.kind {
        TargetKind::Fragment { program, .. } => program,
        TargetKind::Mesh(mesh) => match mesh.entries.first() {
          Some(e) => &e.pipeline.program,
          None => return Err("target has no draw entries".to_string()),
        },
      };
      for b in updates {
        if !program.accepts_uniform(&b.name) {
          return Err(format!("no active uniform named '{}'", b.name));
        }
      }
    }
    let bindings = match &mut self.kind {
      TargetKind::Fragment { bindings, .. } => bindings,
      TargetKind::Mesh(mesh) => &mut mesh.entries.first_mut().expect("entry checked above").bindings,
    };
    merge_bindings(bindings, updates);
    Ok(())
  }

  /// Fold a params update into the first pass's record by name (fragment, or
  /// entry 0: the single-draw update path). Uniforms are program state in GL,
  /// so rendering once with the merged record is equivalent to rendering
  /// after each partial params list; the owner defers that render to its
  /// dirty flush.
  pub fn merge_params(&mut self, params: &[(String, ParamValue)]) {
    match &mut self.kind {
      TargetKind::Fragment { params: record, .. } => merge_record(record, params),
      TargetKind::Mesh(mesh) => {
        if let Some(entry) = mesh.entries.first_mut() {
          merge_record(&mut entry.params, params);
        }
      }
    }
  }

  /// Render the target's pass into its texture: the fullscreen fragment
  /// draw, or clear + the ordered entry list. `resolve` maps a binding list
  /// to live GL textures + sampler objects (the owner holds the registries).
  /// See `run_pass` for the GL state contract; Context::submit's per-frame
  /// fence orders the work ahead of the render thread sampling the target
  /// from its shared GL context, so no glFinish is needed here.
  pub fn render(&self, gl: &glow::Context, resolve: &dyn Fn(&[TextureBinding], &ShaderProgram) -> Vec<PassInput>) {
    match &self.kind {
      TargetKind::Fragment { program, params, bindings } => {
        let inputs = resolve(bindings, program);
        let draw =
          PassDraw::Fullscreen { program, params, textures: &inputs, vertex_count: 3, clear: None, blend: false };
        run_pass(gl, Some(self.fbo), (0, 0), self.width, self.height, None, draw);
        self.resolve(gl);
      }
      TargetKind::Mesh(_) => self.render_groups(gl, resolve, true, &[], None),
    }
  }

  /// Render a mesh target's pass with its sub-targets as groups (see
  /// `DrawGroup`): with `full`, the pass-level clear, the target's own
  /// entries over the whole storage, then every tile in `tiles`; without
  /// it, only the tiles in `tiles`, each clearing its own rectangle, and
  /// nothing else touched - the partial render that keeps clean tiles'
  /// pixels. One pass either way. `tiles` are this target's sub-targets
  /// (`region().parent` = this target); the owner picks which.
  pub fn render_groups(
    &self,
    gl: &glow::Context,
    resolve: &dyn Fn(&[TextureBinding], &ShaderProgram) -> Vec<PassInput>,
    full: bool,
    tiles: &[&ShaderTexture],
    tile_clear: Option<&ShaderProgram>,
  ) {
    let Some(mesh) = self.mesh() else { return };
    let own: Vec<ResolvedDraw> = if full { mesh.resolved_draws(resolve) } else { Vec::new() };
    let tile_draws: Vec<Vec<ResolvedDraw>> =
      tiles.iter().map(|t| t.mesh().map(|m| m.resolved_draws(resolve)).unwrap_or_default()).collect();
    let mut groups: Vec<DrawGroup> = Vec::with_capacity(tiles.len() + 1);
    if full {
      groups.push(DrawGroup { rect: None, clear: None, clear_depth: false, shared: &mesh.shared_params, draws: &own });
    }
    for (tile, draws) in tiles.iter().zip(tile_draws.iter()) {
      let (Some(region), Some(tile_mesh)) = (tile.region(), tile.mesh()) else { continue };
      // Image space is viewport space here (row 0 = top, see `Region`): no
      // flip. In a full render of a parent with no entries of its own the
      // pass-level clear already wiped the rectangle (depth included), so
      // the tile only re-clears when its color differs from the parent's;
      // otherwise (a partial render, or own entries that may have painted
      // and depth-written the rectangle) the tile wipes both.
      let covered = full && own.is_empty();
      let same_color = mesh.clear_color == tile_mesh.clear_color;
      groups.push(DrawGroup {
        rect: Some((region.x, region.y, tile.width, tile.height)),
        clear: (!covered || !same_color).then_some(tile_mesh.clear_color),
        clear_depth: !covered,
        shared: &tile_mesh.shared_params,
        draws,
      });
    }
    let draw = PassDraw::Draws {
      clear: (full && !mesh.load).then_some(mesh.clear_color),
      clear_depth: full,
      depth: mesh.depth.is_some(),
      groups: &groups,
      tile_clear,
    };
    run_pass(gl, Some(self.draw_fbo()), (0, 0), self.width, self.height, None, draw);
    self.resolve(gl);
  }

  /// Draw the resolved inputs over this target's full contents via `program`
  /// (the shared copy program), no clear - the covering triangle writes every
  /// pixel: the copyTexture write. A sampling draw, never a blit (see
  /// `gl::draw::draw_and_resolve` for why blits are not an option on this stack).
  pub fn overwrite_with(&self, gl: &glow::Context, program: &ShaderProgram, textures: &[PassInput]) {
    super::pass::render_program_to_fbo(gl, program, Some(self.fbo), self.width, self.height, &[], textures, None);
    self.resolve(gl);
  }

  /// Clear the target to its clear color (and its depth buffer, when
  /// attached) without running any program: the defined initial contents of a
  /// manual target, whose pass may be non-idempotent and therefore must not
  /// run outside an explicit render. Creation and resize would otherwise
  /// leave undefined storage. Scissor, color/depth masks, clear values and
  /// the FBO binding are Impeller-cached state on this shared context: force,
  /// clear, and put everything back (same contract as `run_pass`).
  pub fn clear(&self, gl: &glow::Context) {
    let [r, g, b, a] = self.mesh().map(|m| m.clear_color).unwrap_or([0.0; 4]);
    unsafe {
      let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
      let scissor = gl.is_enabled(glow::SCISSOR_TEST);
      let mut prev_mask = [0i32; 4];
      gl.get_parameter_i32_slice(glow::COLOR_WRITEMASK, &mut prev_mask);
      let mut prev_clear = [0f32; 4];
      gl.get_parameter_f32_slice(glow::COLOR_CLEAR_VALUE, &mut prev_clear);

      gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.draw_fbo()));
      gl.disable(glow::SCISSOR_TEST);
      gl.color_mask(true, true, true, true);
      gl.clear_color(r, g, b, a);
      if self.mesh().is_some_and(|m| m.depth.is_some()) {
        let prev_depth_mask = gl.get_parameter_i32(glow::DEPTH_WRITEMASK) != 0;
        let prev_clear_depth = gl.get_parameter_f32(glow::DEPTH_CLEAR_VALUE);
        gl.depth_mask(true);
        // Always the far plane; Impeller's clip passes leave 0.0 behind (see
        // run_pass).
        gl.clear_depth_f32(1.0);
        gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        gl.depth_mask(prev_depth_mask);
        gl.clear_depth_f32(prev_clear_depth);
      } else {
        gl.clear(glow::COLOR_BUFFER_BIT);
      }

      gl.clear_color(prev_clear[0], prev_clear[1], prev_clear[2], prev_clear[3]);
      gl.color_mask(prev_mask[0] != 0, prev_mask[1] != 0, prev_mask[2] != 0, prev_mask[3] != 0);
      if scissor {
        gl.enable(glow::SCISSOR_TEST);
      }
      gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
    }
    self.resolve(gl);
  }
}