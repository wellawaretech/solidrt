//! Render targets: `ShaderTexture` (an FBO-backed target texture rendered as
//! a fullscreen fragment pass or as an ordered list of mesh draws) and the
//! retained layer target. This is where per-target state lives - the draw
//! entries with their VAOs, buffers, params and bindings, the target-owned
//! depth storage, and the clear color.

use glow::HasContext;
use std::cell::Cell;
use std::num::NonZeroU32;
use std::rc::Rc;

use super::buffer::{release_buffer, GpuBuffer};
use super::pass::{run_pass, PassDraw, PassInput, ResolvedDraw};
use super::program::{release_pipeline, release_program, RenderPipeline, ShaderProgram};
use super::resources::GpuDrawInfo;
use super::spec::DepthStorage;
use super::vocab::{
  blend_name, cull_name, merge_bindings, validate_order, AttrFormat, DrawRange, IndexFormat, ParamValue, PipelineDesc, TextureBinding,
};
use super::{prev_buffer, prev_framebuffer, prev_texture, prev_vertex_array};

/// The buffers one draw entry fetches through, resolved from registry ids to
/// live Rc clones (the raster-side counterpart of `DrawSpec`'s id fields).
/// Each buffer rides in the entry for its VAO's lifetime - Rc-held like the
/// pipeline, so destroying the registry entry in either order is safe. The
/// registry ids stay alongside for `reads_buffer` and introspection.
#[derive(Default)]
pub struct EntryBuffers {
  /// The interleaved vertex buffer the pipeline's `attributes` describe;
  /// None when the pipeline is attributeless.
  pub vertex: Option<(Rc<GpuBuffer>, u64)>,
  /// Index binding: present = the entry draws indexed; the buffer's
  /// ELEMENT_ARRAY binding is captured in the entry's VAO at build time.
  pub index: Option<(Rc<GpuBuffer>, u64, IndexFormat)>,
  /// The per-instance buffers, one per instance slot the pipeline's
  /// `instance_attributes` declare (dense from 0), each captured in the
  /// VAO at divisor 1; empty when it declares none.
  pub instances: Vec<(Rc<GpuBuffer>, u64)>,
}

/// One draw of a mesh target's ordered list: the pipeline it draws with
/// (which owns the draw state), plus everything bound to THIS entry - the
/// VAO built against its concrete buffers, those buffers' registry ids, the
/// draw range, uniform values, and sampler inputs. Entries are addressed
/// by a UI-allocated id that stays stable across add/remove (never an index).
pub(super) struct DrawEntry {
  pub(super) id: u64,
  pub(super) pipeline: Rc<RenderPipeline>,
  /// Registry id of the shared pipeline this entry draws with; None for the
  /// fused create path, whose pipeline is anonymous and dies with the target.
  pipeline_id: Option<u64>,
  pub(super) vao: glow::VertexArray,
  /// The entry's resolved buffers (vertex, index, instance): what the VAO
  /// reads, and what buffer writes re-render through (see `reads_buffer`).
  buffers: EntryBuffers,
  /// Resolved and bounds-checked UI-side (see `resolve_draw_range`) before
  /// it ever reaches this field.
  pub(super) draw: DrawRange,
  /// This entry's current uniform values, folded in by the params merge (its
  /// only writer) and re-applied at every render - entries sharing a program
  /// overwrite each other's uniforms per pass, so re-application is
  /// mandatory, not redundancy.
  pub(super) params: Vec<(String, ParamValue)>,
  /// sampler2D uniform name -> source texture id. Resolved to a live GL
  /// texture at each render by the owner (which holds the texture registry),
  /// so an input whose contents or registry entry changed is picked up
  /// automatically.
  pub(super) bindings: Vec<TextureBinding>,
}

/// The mesh half of a target: the ordered draw list sharing this target's
/// color (and optional depth) storage, rendered as one pass - clear once,
/// then entries in list order.
pub(super) struct MeshState {
  /// The ordered draw list. The single-draw creates hold exactly one entry
  /// (id 0, `fixed`); draw targets start empty and mutate via
  /// `add_entry`/`remove_entry`.
  pub(super) entries: Vec<DrawEntry>,
  /// Target-level params every entry shares (a camera's view-projection),
  /// applied at render before each entry's own params so an entry naming the
  /// same uniform overrides the shared value. Target state, not entry state:
  /// it survives entry add/remove/rebuild. Only draw targets write it (via
  /// `merge_shared_params`); the fixed kinds' target-level params ARE entry
  /// 0's params.
  pub(super) shared_params: Vec<(String, ParamValue)>,
  /// Target-level sampler bindings every entry shares (an environment map, a
  /// shadow map, a LUT): sampler2D uniform name -> source texture id, same
  /// shape as an entry's `bindings`. At render each entry gets the shared
  /// names its program declares and its own bindings do not override - so
  /// coverage may be partial and an entry's own binding wins, mirroring
  /// `shared_params`. Target state; only draw targets write it (via
  /// `merge_shared_bindings`).
  pub(super) shared_bindings: Vec<TextureBinding>,
  /// Present when the target owns depth storage: explicit on a draw target
  /// (`create_draw_target`'s depth option), derived from the pipeline on the
  /// single-draw creates. A renderbuffer stays private to the FBO; a depth
  /// texture is registered under its own id by the owner (see
  /// `DepthAttachment`).
  depth: Option<DepthAttachment>,
  /// Multisampled storage when the target was created with `samples >= 2`
  /// and the device granted it; None = single-sample. See `Msaa`.
  msaa: Option<Msaa>,
  pub(super) clear_color: [f32; 4],
  /// Color load op (see `TargetSpec::load`): true = draw over the previous
  /// contents instead of clearing. Only ever true on manual targets.
  pub(super) load: bool,
  /// The single-draw creates: the entry set is fixed at creation. The
  /// per-target verbs address entry 0; add/remove are rejected (gated
  /// UI-side, backstopped here).
  fixed: bool,
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
  /// owner as timer queries retire (see gpu::PassTimer).
  pass_exec_micros: Cell<u64>,
}

/// How a mesh target multisamples. Both flavors keep the target texture
/// single-sample - it stays the id everything else samples, displays, reads
/// back and copies - and differ only in where the samples live:
///
/// - `InTile` (EXT_multisampled_render_to_texture): the texture itself is
///   attached with a sample count and the driver resolves at tile writeback.
///   No extra color storage, no resolve pass; the right answer on tiled
///   mobile GPUs (see `gl::MsrttFns`).
/// - `Explicit` (ES 3.0 core): a multisampled color renderbuffer in its own
///   FBO, resolved into the texture with glBlitFramebuffer after every pass.
///
/// Depth, when the target owns it, is allocated multisampled to match
/// (through the extension's or the core storage call respectively).
pub(super) enum Msaa {
  InTile { fns: &'static crate::gl::MsrttFns, samples: i32 },
  Explicit { fbo: glow::Framebuffer, color: glow::Renderbuffer, samples: i32 },
}

impl Msaa {
  fn samples(&self) -> i32 {
    match self {
      Msaa::InTile { samples, .. } | Msaa::Explicit { samples, .. } => *samples,
    }
  }
}

/// Target texture + FBO shared by every target kind: allocation only, nothing
/// attached and no binding left behind. `attach_storage` wires and checks
/// it; `create_mesh_storage` is the one-call form every create uses.
fn create_target(gl: &glow::Context, width: u32, height: u32) -> Result<(glow::Texture, glow::Framebuffer), String> {
  unsafe {
    let target = create_target_texture(gl, width, height)?;
    let fbo = match gl.create_framebuffer() {
      Ok(fbo) => fbo,
      Err(e) => {
        gl.delete_texture(target);
        return Err(format!("glGenFramebuffers failed: {e}"));
      }
    };
    Ok((target, fbo))
  }
}

/// The target texture alone (creation and resize share it): LINEAR, clamp,
/// no mips - the default MIN_FILTER references mipmaps, which would make the
/// texture sampling-incomplete (reads as black) when Impeller samples it.
/// Restores the texture binding it touches.
unsafe fn create_target_texture(gl: &glow::Context, width: u32, height: u32) -> Result<glow::Texture, String> {
  let prev_tex = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
  let target = gl.create_texture().map_err(|e| format!("glGenTextures failed: {e}"))?;
  gl.bind_texture(glow::TEXTURE_2D, Some(target));
  gl.tex_image_2d(
    glow::TEXTURE_2D,
    0,
    glow::RGBA8 as i32,
    width as i32,
    height as i32,
    0,
    glow::RGBA,
    glow::UNSIGNED_BYTE,
    glow::PixelUnpackData::Slice(None),
  );
  gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
  gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
  gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
  gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
  gl.bind_texture(glow::TEXTURE_2D, prev_texture(prev_tex));
  Ok(target)
}

/// A mesh target's depth storage (see `DepthStorage`). `Buffer` is the
/// private renderbuffer, deleted with the target. `Texture` is a
/// `DEPTH_COMPONENT24` texture that the owner adopts into Impeller under its
/// own registry id exactly like the color target, so it follows the color
/// target's ownership rule: never deleted here once registered (Impeller
/// deletes the name when the adopted handle drops), and replaced by a fresh
/// name on resize so in-flight users of the old one stay valid.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum DepthAttachment {
  Buffer(glow::Renderbuffer),
  Texture(glow::Texture),
}

/// A depth texture at `width` x `height`: `DEPTH_COMPONENT24`, NEAREST and
/// clamped - a depth texture without a comparison mode is only
/// sampling-complete at NEAREST (ES 3.0), and its registry entry declares
/// the same, so the sampler object a pass binds agrees with these. Restores
/// the texture binding it touches.
unsafe fn create_depth_texture(gl: &glow::Context, width: u32, height: u32) -> Result<glow::Texture, String> {
  let prev_tex = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
  let tex = gl.create_texture().map_err(|e| format!("glGenTextures (depth) failed: {e}"))?;
  gl.bind_texture(glow::TEXTURE_2D, Some(tex));
  gl.tex_image_2d(
    glow::TEXTURE_2D,
    0,
    glow::DEPTH_COMPONENT24 as i32,
    width as i32,
    height as i32,
    0,
    glow::DEPTH_COMPONENT,
    glow::UNSIGNED_INT,
    glow::PixelUnpackData::Slice(None),
  );
  gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
  gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
  gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
  gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
  gl.bind_texture(glow::TEXTURE_2D, prev_texture(prev_tex));
  Ok(tex)
}

/// Everything a mesh target draws into: the texture-owning FBO, optional
/// depth storage, optional multisampling.
struct MeshStorage {
  target: glow::Texture,
  fbo: glow::Framebuffer,
  depth: Option<DepthAttachment>,
  msaa: Option<Msaa>,
}

impl MeshStorage {
  /// Delete every GL name this storage owns (the create-path rollback; a
  /// live target frees through `ShaderTexture::destroy` instead). The depth
  /// texture is not yet adopted on this path, so it is ours to delete.
  unsafe fn delete(self, gl: &glow::Context) {
    match self.depth {
      Some(DepthAttachment::Buffer(rb)) => gl.delete_renderbuffer(rb),
      Some(DepthAttachment::Texture(tex)) => gl.delete_texture(tex),
      None => {}
    }
    if let Some(Msaa::Explicit { fbo, color, .. }) = self.msaa {
      gl.delete_framebuffer(fbo);
      gl.delete_renderbuffer(color);
    }
    gl.delete_framebuffer(self.fbo);
    gl.delete_texture(self.target);
  }
}

/// Create a target's storage at `samples`x (1 = single-sample; `depth` and
/// `samples` are the mesh-only extras, the fragment and layer targets ask
/// for neither and get the bare color FBO). A count
/// above the device maximum is clamped; the in-tile flavor is tried first
/// where the extension exists, then the explicit one, and a multisampled
/// configuration the driver refuses (incomplete FBO) falls back to
/// single-sample with a warning rather than failing the create - the app
/// asked for quality, not for a hard requirement. Restores the framebuffer
/// binding.
fn create_mesh_storage(
  gl: &glow::Context,
  width: u32,
  height: u32,
  depth: DepthStorage,
  samples: u32,
) -> Result<MeshStorage, String> {
  if depth == DepthStorage::Texture && samples >= 2 {
    // Gated UI-side; backstopped here because a multisampled depth texture
    // would silently be unsampleable.
    return Err("a depth texture cannot be multisampled (samples must be 1 with depth \"texture\")".to_string());
  }
  unsafe {
    let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
    let (target, fbo) = create_target(gl, width, height)?;
    let depth = match depth {
      DepthStorage::None => None,
      DepthStorage::Buffer => match gl.create_renderbuffer() {
        Ok(rb) => Some(DepthAttachment::Buffer(rb)),
        Err(e) => {
          gl.delete_framebuffer(fbo);
          gl.delete_texture(target);
          return Err(format!("glGenRenderbuffers failed: {e}"));
        }
      },
      DepthStorage::Texture => match create_depth_texture(gl, width, height) {
        Ok(tex) => Some(DepthAttachment::Texture(tex)),
        Err(e) => {
          gl.delete_framebuffer(fbo);
          gl.delete_texture(target);
          return Err(e);
        }
      },
    };
    let mut storage = MeshStorage { target, fbo, depth, msaa: None };

    let max_samples = gl.get_parameter_i32(glow::MAX_SAMPLES).max(1);
    let samples = (samples as i32).min(max_samples);
    if samples >= 2 {
      storage.msaa = match crate::gl::msrtt() {
        Some(fns) => Some(Msaa::InTile { fns, samples }),
        None => match (gl.create_framebuffer(), gl.create_renderbuffer()) {
          (Ok(msaa_fbo), Ok(color)) => Some(Msaa::Explicit { fbo: msaa_fbo, color, samples }),
          (Ok(msaa_fbo), Err(e)) => {
            gl.delete_framebuffer(msaa_fbo);
            storage.delete(gl);
            return Err(format!("glGenRenderbuffers failed: {e}"));
          }
          (Err(e), _) => {
            storage.delete(gl);
            return Err(format!("glGenFramebuffers failed: {e}"));
          }
        },
      };
      match attach_storage(gl, &storage, width, height) {
        Ok(()) => {
          gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
          return Ok(storage);
        }
        Err(e) => {
          log::warn!("[shader] {samples}x multisampling unavailable ({e}); target renders single-sample");
          if let Some(Msaa::Explicit { fbo: msaa_fbo, color, .. }) = storage.msaa.take() {
            gl.delete_framebuffer(msaa_fbo);
            gl.delete_renderbuffer(color);
          }
        }
      }
    }
    let result = attach_storage(gl, &storage, width, height);
    gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
    match result {
      Ok(()) => Ok(storage),
      Err(e) => {
        storage.delete(gl);
        Err(e)
      }
    }
  }
}

/// (Re)attach and (re)size a mesh target's storage for `width` x `height`:
/// the texture onto its FBO (multisampled through the extension for the
/// in-tile flavor), the explicit flavor's color renderbuffer onto the draw
/// FBO, and the depth renderbuffer onto whichever FBO draws. Creation and
/// resize share it. Ends with the draw FBO's completeness check and leaves
/// the framebuffer binding on it; the renderbuffer binding is restored.
unsafe fn attach_storage(gl: &glow::Context, storage: &MeshStorage, width: u32, height: u32) -> Result<(), String> {
  let (w, h) = (width as i32, height as i32);
  let prev_rb = gl.get_parameter_i32(glow::RENDERBUFFER_BINDING);
  gl.bind_framebuffer(glow::FRAMEBUFFER, Some(storage.fbo));
  let explicit = match &storage.msaa {
    None => {
      gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(storage.target), 0);
      if let Some(DepthAttachment::Buffer(rb)) = storage.depth {
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
        gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, w, h);
      }
      false
    }
    Some(Msaa::InTile { fns, samples }) => {
      (fns.framebuffer_texture_2d_multisample)(
        glow::FRAMEBUFFER,
        glow::COLOR_ATTACHMENT0,
        glow::TEXTURE_2D,
        storage.target.0.get(),
        0,
        *samples,
      );
      if let Some(DepthAttachment::Buffer(rb)) = storage.depth {
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
        (fns.renderbuffer_storage_multisample)(glow::RENDERBUFFER, *samples, glow::DEPTH_COMPONENT24, w, h);
      }
      false
    }
    Some(Msaa::Explicit { fbo, color, samples }) => {
      // The texture FBO is the resolve destination and must be complete on
      // its own.
      gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(storage.target), 0);
      let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
      if status != glow::FRAMEBUFFER_COMPLETE {
        gl.bind_renderbuffer(glow::RENDERBUFFER, prev_renderbuffer(prev_rb));
        return Err(format!("target framebuffer incomplete: {status:#x}"));
      }
      gl.bind_framebuffer(glow::FRAMEBUFFER, Some(*fbo));
      gl.bind_renderbuffer(glow::RENDERBUFFER, Some(*color));
      gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, *samples, glow::RGBA8, w, h);
      gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(*color));
      if let Some(DepthAttachment::Buffer(rb)) = storage.depth {
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
        gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, *samples, glow::DEPTH_COMPONENT24, w, h);
      }
      true
    }
  };
  match storage.depth {
    Some(DepthAttachment::Buffer(rb)) => {
      gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(rb));
    }
    // Sized at creation (never respecified: a resize brings a new name), so
    // attaching is all there is to do.
    Some(DepthAttachment::Texture(tex)) => {
      gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, Some(tex), 0);
    }
    None => {}
  }
  gl.bind_renderbuffer(glow::RENDERBUFFER, prev_renderbuffer(prev_rb));
  let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
  if status != glow::FRAMEBUFFER_COMPLETE {
    let what = if explicit { "multisample" } else { "target" };
    return Err(format!("{what} framebuffer incomplete: {status:#x}"));
  }
  Ok(())
}

fn prev_renderbuffer(prev: i32) -> Option<glow::Renderbuffer> {
  NonZeroU32::new(prev as u32).map(glow::NativeRenderbuffer)
}

/// Record one interleaved attribute layout against the currently bound
/// ARRAY_BUFFER into the current VAO. Attribute locations are looked up by
/// name, so an attribute the shader does not use is skipped - its bytes
/// still occupy the stride. `divisor` 0 advances per vertex, 1 per instance.
unsafe fn record_layout(
  gl: &glow::Context,
  program: &ShaderProgram,
  attributes: &[(String, AttrFormat)],
  divisor: u32,
) {
  let stride = super::vocab::vertex_stride(attributes);
  let mut offset = 0i32;
  for (name, fmt) in attributes {
    // None means the shader does not (actively) use the attribute; that
    // is fine, the bytes are simply skipped over via the stride.
    if let Some(loc) = gl.get_attrib_location(program.program, name) {
      gl.enable_vertex_attrib_array(loc);
      gl.vertex_attrib_pointer_f32(loc, fmt.components(), glow::FLOAT, false, stride, offset);
      if divisor != 0 {
        gl.vertex_attrib_divisor(loc, divisor);
      }
    }
    offset += fmt.components() * 4;
  }
}

/// Record a pipeline's vertex and instance layouts against an entry's
/// concrete buffers in a fresh VAO: `desc.attributes` over the vertex buffer
/// (divisor 0), `desc.instance_attributes` over the instance buffer (divisor
/// 1 - the divisor is VAO state in ES 3.0, like the attribute pointers). The
/// index buffer, when given, is bound as ELEMENT_ARRAY while the VAO is
/// current: that binding is VAO state too, so it is captured here once and
/// needs no per-draw rebinding (and no explicit save - restoring the
/// previous VAO restores its own element binding). Restores the VAO and
/// array-buffer bindings it touches.
fn build_vao(
  gl: &glow::Context,
  program: &ShaderProgram,
  desc: &PipelineDesc,
  buffers: &EntryBuffers,
) -> Result<glow::VertexArray, String> {
  unsafe {
    let prev_vao = gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING);
    let prev_ab = gl.get_parameter_i32(glow::ARRAY_BUFFER_BINDING);
    let vao = gl.create_vertex_array().map_err(|e| format!("glGenVertexArrays failed: {e}"))?;
    gl.bind_vertex_array(Some(vao));
    if let Some((buffer, _)) = &buffers.vertex {
      gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer.vbo));
      record_layout(gl, program, &desc.attributes, 0);
    }
    for (slot, (buffer, _)) in buffers.instances.iter().enumerate() {
      let layout: Vec<(String, AttrFormat)> = desc
        .instance_attributes
        .iter()
        .filter(|(_, _, s)| *s as usize == slot)
        .map(|(n, f, _)| (n.clone(), *f))
        .collect();
      gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer.vbo));
      record_layout(gl, program, &layout, 1);
    }
    if let Some((index, _, _)) = &buffers.index {
      gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(index.vbo));
    }
    gl.bind_vertex_array(prev_vertex_array(prev_vao));
    gl.bind_buffer(glow::ARRAY_BUFFER, prev_buffer(prev_ab));
    Ok(vao)
  }
}

/// The buffer-presence contract between a pipeline and an entry: a declared
/// layout needs its buffer, and an instance buffer without a declared layout
/// would never be read. Validated at the call site against the UI mirrors;
/// this copy is the raster-side backstop for the create paths.
fn check_entry_buffers(desc: &PipelineDesc, buffers: &EntryBuffers) -> Result<(), String> {
  if !desc.attributes.is_empty() && buffers.vertex.is_none() {
    return Err("pipeline declares attributes but no vertex buffer".to_string());
  }
  let slots = desc.instance_attributes.iter().map(|(_, _, s)| *s as usize + 1).max().unwrap_or(0);
  if slots > buffers.instances.len() {
    return Err("pipeline declares instanceAttributes but no instance buffer".to_string());
  }
  if slots < buffers.instances.len() {
    return Err("pipeline declares no instanceAttributes; the instance buffer would never be read".to_string());
  }
  Ok(())
}

/// Drop an entry's uses of its buffers, deleting each GL buffer when the
/// entry held the last reference (see `release_buffer`).
fn release_entry_buffers(gl: &glow::Context, buffers: EntryBuffers) {
  if let Some((buffer, _)) = buffers.vertex {
    release_buffer(gl, buffer);
  }
  if let Some((buffer, _, _)) = buffers.index {
    release_buffer(gl, buffer);
  }
  for (buffer, _) in buffers.instances {
    release_buffer(gl, buffer);
  }
}

/// Fold a params update into a record by name (new names append, existing
/// names overwrite): the merge rule shared by target-level and per-entry
/// params writes.
fn merge_record(record: &mut Vec<(String, ParamValue)>, params: &[(String, ParamValue)]) {
  for (name, value) in params {
    match record.iter_mut().find(|(n, _)| n == name) {
      Some(entry) => entry.1 = value.clone(),
      None => record.push((name.clone(), value.clone())),
    }
  }
}

/// Create a retained layer target: an exactly-sized RGBA8 texture + FBO
/// (the window-shader layer, a boundary shader's output or history). Exact
/// on purpose - shaders sample it with 0..1 coordinates, so padding would
/// leak into the sampling contract. Completeness-checked here (unlike shader
/// targets, nothing later would catch it); restores the FBO binding it
/// touches. The new layer starts cleared to `clear`: a history layer
/// (`uPrevious`) is sampled before anything renders into it, and undefined
/// storage must not reach a program - the window path clears opaque black
/// (its frames are opaque), boundary layers clear transparent (a snapshot's
/// empty regions are).
pub fn create_layer_target(
  gl: &glow::Context,
  width: u32,
  height: u32,
  clear: [f32; 4],
) -> Result<(glow::Texture, glow::Framebuffer), String> {
  let MeshStorage { target, fbo, .. } = create_mesh_storage(gl, width, height, DepthStorage::None, 1)?;
  unsafe {
    // Scissor, color mask, and clear color are Impeller-cached state on this
    // shared context: force a full clear and put all three back.
    let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
    let scissor = gl.is_enabled(glow::SCISSOR_TEST);
    let mut prev_mask = [0i32; 4];
    gl.get_parameter_i32_slice(glow::COLOR_WRITEMASK, &mut prev_mask);
    let mut prev_clear = [0f32; 4];
    gl.get_parameter_f32_slice(glow::COLOR_CLEAR_VALUE, &mut prev_clear);
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
    gl.disable(glow::SCISSOR_TEST);
    gl.color_mask(true, true, true, true);
    gl.clear_color(clear[0], clear[1], clear[2], clear[3]);
    gl.clear(glow::COLOR_BUFFER_BIT);
    gl.clear_color(prev_clear[0], prev_clear[1], prev_clear[2], prev_clear[3]);
    gl.color_mask(prev_mask[0] != 0, prev_mask[1] != 0, prev_mask[2] != 0, prev_mask[3] != 0);
    if scissor {
      gl.enable(glow::SCISSOR_TEST);
    }
    gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
  }
  Ok((target, fbo))
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
    })
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

  /// Registry id of the shared program behind the first entry's pipeline;
  /// None for fragment targets and for the fused create path, whose program
  /// is anonymous.
  pub fn program_id(&self) -> Option<u64> {
    self.entry0().and_then(|e| e.pipeline.program_id)
  }

  /// Registry id of the shared pipeline the first entry draws with; None
  /// for fragment targets and the fused create path.
  pub fn pipeline_id(&self) -> Option<u64> {
    self.entry0().and_then(|e| e.pipeline_id)
  }

  /// Registry id of the vertex buffer the first entry draws from, if any.
  pub fn buffer_id(&self) -> Option<u64> {
    self.entry0().and_then(|e| e.buffers.vertex.as_ref().map(|(_, id)| *id))
  }

  /// Registry id of the first entry's index buffer, if it draws indexed.
  pub fn index_buffer_id(&self) -> Option<u64> {
    self.entry0().and_then(|e| e.buffers.index.as_ref().map(|(_, iid, _)| *iid))
  }

  /// The first entry's index format as the string `IndexFormat::parse`
  /// accepts; None when it draws plain.
  pub fn index_format_name(&self) -> Option<&'static str> {
    self.entry0().and_then(|e| e.buffers.index.as_ref().map(|(_, _, fmt)| fmt.name()))
  }

  /// Registry ids of the first entry's per-instance buffers, in slot
  /// order; empty when it binds none.
  pub fn instance_buffer_ids(&self) -> Vec<u64> {
    self.entry0().map(|e| e.buffers.instances.iter().map(|(_, id)| *id).collect()).unwrap_or_default()
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

  /// The draw range of the first entry; None on a fragment-only shader.
  pub fn draw_range(&self) -> Option<DrawRange> {
    self.entry0().map(|e| e.draw)
  }

  /// The first entry's topology as the string `Topology::parse` accepts;
  /// None on a fragment-only shader.
  pub fn topology_name(&self) -> Option<&'static str> {
    self.entry0().map(|e| e.pipeline.desc.topology.name())
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
      crate::gpu::generate_mipmap(gl, self.target);
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

  /// Whether the first entry's draw writes depth; None on a fragment-only
  /// shader.
  pub fn depth_write(&self) -> Option<bool> {
    self.entry0().map(|e| e.pipeline.desc.depth.map_or(true, |d| d.write))
  }

  /// The first entry's blend mode as the string `parse_blend` accepts; None
  /// on a fragment-only shader.
  pub fn blend_name(&self) -> Option<&'static str> {
    self.entry0().map(|e| blend_name(e.pipeline.desc.blend))
  }

  /// The first entry's cull mode as the string `parse_cull` accepts; None on
  /// a fragment-only shader.
  pub fn cull_name(&self) -> Option<&'static str> {
    self.entry0().map(|e| cull_name(e.pipeline.desc.cull))
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
    match self.kind {
      TargetKind::Fragment { program, .. } => release_program(gl, program),
      TargetKind::Mesh(mesh) => {
        for entry in mesh.entries {
          unsafe { gl.delete_vertex_array(entry.vao) };
          release_pipeline(gl, entry.pipeline);
          release_entry_buffers(gl, entry.buffers);
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
  pub fn uniform_table(&self) -> super::vocab::UniformTable {
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
  pub fn render(&self, gl: &glow::Context, resolve: &dyn Fn(&[TextureBinding]) -> Vec<PassInput>) {
    match &self.kind {
      TargetKind::Fragment { program, params, bindings } => {
        let inputs = resolve(bindings);
        let draw =
          PassDraw::Fullscreen { program, params, textures: &inputs, vertex_count: 3, clear: None, blend: false };
        run_pass(gl, Some(self.fbo), (0, 0), self.width, self.height, draw);
        self.resolve(gl);
      }
      TargetKind::Mesh(mesh) => {
        let draws: Vec<ResolvedDraw> = mesh
          .entries
          .iter()
          .map(|e| {
            // An entry's inputs are its own bindings plus the shared ones its
            // program declares and does not bind itself (entry overrides
            // shared, and an undeclared shared name must not eat a texture
            // unit on this entry).
            let inputs = if mesh.shared_bindings.is_empty() {
              resolve(&e.bindings)
            } else {
              let mut combined = e.bindings.clone();
              for b in &mesh.shared_bindings {
                if e.pipeline.program.is_active(&b.name) && !combined.iter().any(|c| c.name == b.name) {
                  combined.push(b.clone());
                }
              }
              resolve(&combined)
            };
            ResolvedDraw {
              program: &e.pipeline.program,
              desc: &e.pipeline.desc,
              vao: e.vao,
              range: e.draw,
              index: e.buffers.index.as_ref().map(|(_, _, fmt)| *fmt),
              params: &e.params,
              inputs,
            }
          })
          .collect();
        let draw = PassDraw::Draws {
          clear: (!mesh.load).then_some(mesh.clear_color),
          depth: mesh.depth.is_some(),
          shared: &mesh.shared_params,
          draws: &draws,
        };
        run_pass(gl, Some(self.draw_fbo()), (0, 0), self.width, self.height, draw);
        self.resolve(gl);
      }
    }
  }

  /// Draw the resolved inputs over this target's full contents via `program`
  /// (the shared copy program), no clear - the covering triangle writes every
  /// pixel: the copyTexture write. A sampling draw, never a blit (see
  /// `gl::draw::draw_and_resolve` for why blits are not an option on this stack).
  pub fn overwrite_with(&self, gl: &glow::Context, program: &ShaderProgram, textures: &[PassInput]) {
    super::pass::render_program_to_fbo(gl, program, Some(self.fbo), self.width, self.height, &[], textures);
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
