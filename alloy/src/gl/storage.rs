//! GL storage for render targets: the target texture + FBO pair every kind
//! shares, depth attachments (renderbuffer or adoptable texture),
//! multisampled color (in-tile or explicit resolve), the one-call mesh
//! storage create with its attachment checks, and the retained layer target.

use glow::HasContext;
use std::num::NonZeroU32;

use super::{prev_framebuffer, prev_texture};
use crate::gpu::spec::DepthStorage;

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
  pub(super) fn samples(&self) -> i32 {
    match self {
      Msaa::InTile { samples, .. } | Msaa::Explicit { samples, .. } => *samples,
    }
  }
}

/// Target texture + FBO shared by every target kind: allocation only, nothing
/// attached and no binding left behind. `attach_storage` wires and checks
/// it; `create_mesh_storage` is the one-call form every create uses.
pub(super) fn create_target(gl: &glow::Context, width: u32, height: u32) -> Result<(glow::Texture, glow::Framebuffer), String> {
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
pub(super) unsafe fn create_target_texture(gl: &glow::Context, width: u32, height: u32) -> Result<glow::Texture, String> {
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
pub(super) unsafe fn create_depth_texture(gl: &glow::Context, width: u32, height: u32) -> Result<glow::Texture, String> {
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
pub(super) struct MeshStorage {
  pub(super) target: glow::Texture,
  pub(super) fbo: glow::Framebuffer,
  pub(super) depth: Option<DepthAttachment>,
  pub(super) msaa: Option<Msaa>,
}

impl MeshStorage {
  /// Delete every GL name this storage owns (the create-path rollback; a
  /// live target frees through `ShaderTexture::destroy` instead). The depth
  /// texture is not yet adopted on this path, so it is ours to delete.
  pub(super) unsafe fn delete(self, gl: &glow::Context) {
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
pub(super) fn create_mesh_storage(
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
pub(super) unsafe fn attach_storage(gl: &glow::Context, storage: &MeshStorage, width: u32, height: u32) -> Result<(), String> {
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

pub(super) fn prev_renderbuffer(prev: i32) -> Option<glow::Renderbuffer> {
  NonZeroU32::new(prev as u32).map(glow::NativeRenderbuffer)
}

/// Record one interleaved attribute layout against the currently bound
/// ARRAY_BUFFER into the current VAO. Attribute locations are looked up by
/// name, so an attribute the shader does not use is skipped - its bytes

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
