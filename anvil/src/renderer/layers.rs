//! The layer targets: where a draw renders a group (a `save_layer`, a nested
//! list below opacity 1) before its composite (walk.rs `Layer`). A pool of
//! offscreen targets by size bucket, each a color texture the composite
//! samples and a depth-stencil for the content's stencil clips, at the
//! sample count of the target being drawn: in tile memory through
//! `EXT_multisampled_render_to_texture` where the context has it, else a
//! multisampled color and depth-stencil resolved into the texture by a
//! blit, else single-sample with the texture attached directly. Anvil loads
//! the extension's entry points itself, through the proc address its
//! executor receives, as alloy's rig does for its own (alloy/src/gl/rig.rs);
//! anvil depends on nothing of alloy. The GL names die with the executor's
//! context, as its programs do: no Drop deletes them. Raster thread only,
//! with the context current.

use super::walk::Instance;
use glow::HasContext;
use std::os::raw::c_void;

/// The allocation granularity of a layer target, device pixels per axis:
/// a layer's size rounds up to a multiple of it, so slightly different
/// layer sizes (a fade over a card that grows) share storage.
pub const LAYER_BUCKET: i32 = 64;

/// The most layer targets the pool keeps across draws: a retention cap, not a
/// nesting limit. A layer of a new bucket takes the storage of the least
/// recently used free target once the pool holds this many, and grows the
/// pool past it when every target is in use (layers nested deeper); `begin`
/// trims the free targets back down to it.
pub const LAYER_POOL_MAX: usize = 16;

/// The draws a free layer target outlives unused before the pool deletes
/// it: two seconds at 60 Hz, as the text atlas evicts.
pub const LAYER_IDLE_DRAWS: u64 = 120;

/// glFramebufferTexture2DMultisampleEXT's signature.
type FramebufferTexture2DMultisample =
  unsafe extern "C" fn(target: u32, attachment: u32, textarget: u32, texture: u32, level: i32, samples: i32);

/// glRenderbufferStorageMultisampleEXT's signature.
type RenderbufferStorageMultisample =
  unsafe extern "C" fn(target: u32, samples: i32, internalformat: u32, width: i32, height: i32);

/// The two `GL_EXT_multisampled_render_to_texture` entry points: on a tiled
/// GPU the samples live in tile memory and only the resolved image is
/// written out, so a multisampled layer costs no sample-count multiple of
/// memory bandwidth.
struct MsrttFns {
  framebuffer_texture_2d_multisample: FramebufferTexture2DMultisample,
  renderbuffer_storage_multisample: RenderbufferStorageMultisample,
}

impl MsrttFns {
  // The entry points through `proc_address`; None when the context does not
  // advertise the extension or resolves either to null.
  unsafe fn load(gl: &glow::Context, proc_address: &mut dyn FnMut(&str) -> *mut c_void) -> Option<MsrttFns> {
    if !gl.supported_extensions().contains("GL_EXT_multisampled_render_to_texture") {
      return None;
    }
    let ftm = proc_address("glFramebufferTexture2DMultisampleEXT");
    let rsm = proc_address("glRenderbufferStorageMultisampleEXT");
    if ftm.is_null() || rsm.is_null() {
      return None;
    }
    // SAFETY: both are the non-null entry points of the advertised
    // extension, whose signatures are exactly the field types.
    Some(MsrttFns {
      framebuffer_texture_2d_multisample: std::mem::transmute::<*mut c_void, FramebufferTexture2DMultisample>(ftm),
      renderbuffer_storage_multisample: std::mem::transmute::<*mut c_void, RenderbufferStorageMultisample>(rsm),
    })
  }
}

/// glInvalidateFramebuffer is core in ES 3.0 (the platform minimum) but only
/// reached desktop GL at 4.3, so a desktop context below that must skip the
/// hint rather than call an unloaded function. (alloy/src/gl/rig.rs
/// `supports_invalidate` holds the same rule.)
fn supports_invalidate(gl: &glow::Context) -> bool {
  let v = gl.version();
  if v.is_embedded {
    v.major >= 3
  } else {
    v.major > 4 || (v.major == 4 && v.minor >= 3)
  }
}

/// `size` rounded up to the allocation bucket on each axis.
pub(super) fn bucket(size: (i32, i32)) -> (i32, i32) {
  let up = |n: i32| (n + LAYER_BUCKET - 1) / LAYER_BUCKET * LAYER_BUCKET;
  (up(size.0), up(size.1))
}

/// The composite instance `composite`, whose texel rows the walk wrote over
/// a unit square (u and v from 0 to 1 across the layer's rect), rescaled to
/// a layer of `content` pixels drawn into storage of `alloc` pixels: the
/// content occupies the storage's bottom-left `content` texels (the viewport
/// puts the layer's bottom edge at window row 0, exec.rs `Frame`), so u
/// scales by content.0 / alloc.0 and v by content.1 / alloc.1. The corner
/// texels in `tex` scale with them.
pub(crate) fn rescale_composite(composite: &Instance, content: (i32, i32), alloc: (i32, i32)) -> Instance {
  let sx = content.0 as f32 / alloc.0 as f32;
  let sy = content.1 as f32 / alloc.1 as f32;
  let mut out = *composite;
  for k in 0..3 {
    out.grad_x[k] *= sx;
    out.grad_y[k] *= sy;
  }
  let [u0, v0, u1, v1] = composite.tex;
  out.tex = [u0 * sx, v0 * sy, u1 * sx, v1 * sy];
  out
}

/// A layer target the pool handed out: its entry, the framebuffer the
/// content draws into, the texture the composite samples, the resolve
/// framebuffer of an explicitly resolved one, and the storage's size.
#[derive(Clone, Copy)]
pub(super) struct LayerTarget {
  pub index: usize,
  pub fbo: glow::Framebuffer,
  pub color: glow::Texture,
  pub resolve: Option<glow::Framebuffer>,
  pub size: (i32, i32),
}

/// One target of the pool.
struct Entry {
  fbo: glow::Framebuffer,
  color: glow::Texture,
  depth_stencil: glow::Renderbuffer,
  /// The explicit resolve's multisampled color renderbuffer (on `fbo`) and
  /// the framebuffer the texture is attached to; None in tile memory and
  /// single-sample, where the texture is `fbo`'s color.
  msaa: Option<(glow::Renderbuffer, glow::Framebuffer)>,
  size: (i32, i32),
  /// 0 for single-sample.
  samples: i32,
  in_use: bool,
  last_used: u64,
}

impl Entry {
  unsafe fn delete(&self, gl: &glow::Context) {
    gl.delete_framebuffer(self.fbo);
    gl.delete_texture(self.color);
    gl.delete_renderbuffer(self.depth_stencil);
    if let Some((color, resolve)) = self.msaa {
      gl.delete_renderbuffer(color);
      gl.delete_framebuffer(resolve);
    }
  }

  fn target(&self, index: usize) -> LayerTarget {
    LayerTarget {
      index,
      fbo: self.fbo,
      color: self.color,
      resolve: self.msaa.map(|(_, resolve)| resolve),
      size: self.size,
    }
  }
}

/// Why a target could not be made.
enum Failure {
  /// GL made no name.
  Names,
  /// The multisampled framebuffer is incomplete.
  Incomplete,
}

/// The layer targets, kept across draws.
pub(super) struct LayerPool {
  entries: Vec<Entry>,
  msrtt: Option<MsrttFns>,
  /// Whether `finish` hints the discarded attachments away.
  invalidate: bool,
  /// The draws begun so far: the clock `last_used` reads.
  draw: u64,
  /// Latched on the first incomplete multisampled framebuffer: a driver may
  /// report a sample count yet reject the configuration, and one rejection
  /// means every later attempt fails too, so the pool stays single-sample
  /// for the process, as alloy's rig does.
  single_sample: bool,
}

impl LayerPool {
  /// Over the context current on the calling thread, its extension entry
  /// points resolved through `proc_address`.
  pub(super) unsafe fn new(gl: &glow::Context, proc_address: &mut dyn FnMut(&str) -> *mut c_void) -> LayerPool {
    LayerPool {
      entries: Vec::new(),
      msrtt: MsrttFns::load(gl, proc_address),
      invalidate: supports_invalidate(gl),
      draw: 0,
      single_sample: false,
    }
  }

  /// The start of a draw: every target is free again, one unused for
  /// LAYER_IDLE_DRAWS draws is deleted, and the least recently used ones go
  /// while the pool holds more than LAYER_POOL_MAX. No target is handed out
  /// between draws, so no index a draw holds moves.
  pub(super) unsafe fn begin(&mut self, gl: &glow::Context) {
    self.draw += 1;
    let draw = self.draw;
    self.entries.retain_mut(|entry| {
      entry.in_use = false;
      let keep = draw - entry.last_used < LAYER_IDLE_DRAWS;
      if !keep {
        entry.delete(gl);
      }
      keep
    });
    while self.entries.len() > LAYER_POOL_MAX {
      let oldest = (0..self.entries.len()).min_by_key(|&i| self.entries[i].last_used).expect("the pool is not empty");
      self.entries.remove(oldest).delete(gl);
    }
  }

  /// A free target of storage `size` (a bucket's, `bucket`) at the target's
  /// `samples` (0 or 1 for single-sample), marked in use: a free one of that
  /// size and sample count, else a new one, in place of the least recently
  /// used free one when the pool holds LAYER_POOL_MAX, past the cap when none
  /// is free. A replacement takes the old one's slot, so no index a live
  /// `LayerTarget` holds moves during a draw. None only when GL cannot make
  /// one (warned once): the caller skips the layer. Leaves the framebuffer
  /// binding dirty; the caller binds its frame.
  pub(super) unsafe fn acquire(&mut self, gl: &glow::Context, size: (i32, i32), samples: i32) -> Option<LayerTarget> {
    let samples = if samples >= 2 && !self.single_sample { samples } else { 0 };
    let draw = self.draw;
    let found = self.entries.iter().position(|e| !e.in_use && e.size == size && e.samples == samples);
    let index = match found {
      Some(index) => index,
      None => {
        let entry = match self.create(gl, size, samples) {
          Ok(entry) => entry,
          Err(Failure::Incomplete) if samples >= 2 => {
            log::warn!(
              "[anvil] Renderer: a {samples}x multisampled layer target is incomplete; layers are single-sample from here on"
            );
            self.single_sample = true;
            match self.create(gl, size, 0) {
              Ok(entry) => entry,
              Err(_) => return self.unavailable(),
            }
          }
          Err(_) => return self.unavailable(),
        };
        let free = if self.entries.len() >= LAYER_POOL_MAX {
          self.entries.iter().enumerate().filter(|(_, e)| !e.in_use).min_by_key(|(_, e)| e.last_used).map(|(i, _)| i)
        } else {
          None
        };
        match free {
          Some(oldest) => {
            let old = std::mem::replace(&mut self.entries[oldest], entry);
            old.delete(gl);
            oldest
          }
          None => {
            self.entries.push(entry);
            self.entries.len() - 1
          }
        }
      }
    };
    let entry = &mut self.entries[index];
    entry.in_use = true;
    entry.last_used = draw;
    Some(entry.target(index))
  }

  // No target could be made: warned once, and the layer is skipped.
  fn unavailable(&self) -> Option<LayerTarget> {
    warn_once!("[anvil] Renderer: a layer target could not be created; the layer does not draw");
    None
  }

  /// The end of `target`'s content, of `content` pixels, before its
  /// composite samples it: an explicitly multisampled target resolves its
  /// content corner into the texture by a blit (the scissor is off inside a
  /// layer, so the blit is whole); then the attachments nothing reads again
  /// are invalidated where the context can (the depth-stencil always, the
  /// multisampled color after its resolve). In tile memory the texture is
  /// complete once the framebuffer is unbound, which the caller's next frame
  /// does. Leaves the framebuffer bindings dirty; the caller binds its frame.
  pub(super) unsafe fn finish(&self, gl: &glow::Context, target: &LayerTarget, content: (i32, i32)) {
    let (w, h) = content;
    if let Some(resolve) = target.resolve {
      gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(target.fbo));
      gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(resolve));
      gl.blit_framebuffer(0, 0, w, h, 0, 0, w, h, glow::COLOR_BUFFER_BIT, glow::NEAREST);
    }
    if self.invalidate {
      gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.fbo));
      if target.resolve.is_some() {
        gl.invalidate_framebuffer(glow::FRAMEBUFFER, &[glow::COLOR_ATTACHMENT0, glow::DEPTH_STENCIL_ATTACHMENT]);
      } else {
        gl.invalidate_framebuffer(glow::FRAMEBUFFER, &[glow::DEPTH_STENCIL_ATTACHMENT]);
      }
    }
  }

  /// `target` is free again. Reuse within the same draw is safe: GL orders
  /// the composite's sampling before the next layer's clear of it.
  pub(super) fn release(&mut self, target: &LayerTarget) {
    if let Some(entry) = self.entries.get_mut(target.index) {
      entry.in_use = false;
    }
  }

  // A new target of storage `size` at `samples` (0: single-sample): the
  // RGBA8 color texture (nearest, clamped, sampled 1:1 by the composite),
  // a DEPTH24_STENCIL8 depth-stencil, and the framebuffers, in tile memory
  // through the extension, explicitly resolved, or single-sample. Leaves the
  // framebuffer binding dirty and nothing bound on TEXTURE_2D or
  // RENDERBUFFER.
  unsafe fn create(&self, gl: &glow::Context, size: (i32, i32), samples: i32) -> Result<Entry, Failure> {
    let (w, h) = size;
    let explicit = samples >= 2 && self.msrtt.is_none();
    let color = gl.create_texture().map_err(|_| Failure::Names)?;
    let names = (
      gl.create_framebuffer(),
      gl.create_renderbuffer(),
      if explicit { gl.create_renderbuffer().map(Some) } else { Ok(None) },
      if explicit { gl.create_framebuffer().map(Some) } else { Ok(None) },
    );
    let (Ok(fbo), Ok(depth_stencil), Ok(msaa_color), Ok(resolve)) = names else {
      gl.delete_texture(color);
      let (fbo, depth_stencil, msaa_color, resolve) = names;
      if let Ok(f) = fbo {
        gl.delete_framebuffer(f);
      }
      if let Ok(r) = depth_stencil {
        gl.delete_renderbuffer(r);
      }
      if let Ok(Some(r)) = msaa_color {
        gl.delete_renderbuffer(r);
      }
      if let Ok(Some(f)) = resolve {
        gl.delete_framebuffer(f);
      }
      return Err(Failure::Names);
    };
    gl.bind_texture(glow::TEXTURE_2D, Some(color));
    gl.tex_image_2d(
      glow::TEXTURE_2D,
      0,
      glow::RGBA8 as i32,
      w,
      h,
      0,
      glow::RGBA,
      glow::UNSIGNED_BYTE,
      glow::PixelUnpackData::Slice(None),
    );
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
    gl.bind_texture(glow::TEXTURE_2D, None);
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
    gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth_stencil));
    match (&self.msrtt, msaa_color) {
      (Some(fns), _) if samples >= 2 => {
        (fns.renderbuffer_storage_multisample)(glow::RENDERBUFFER, samples, glow::DEPTH24_STENCIL8, w, h);
        (fns.framebuffer_texture_2d_multisample)(
          glow::FRAMEBUFFER,
          glow::COLOR_ATTACHMENT0,
          glow::TEXTURE_2D,
          color.0.get(),
          0,
          samples,
        );
      }
      (_, Some(msaa_color)) => {
        gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, samples, glow::DEPTH24_STENCIL8, w, h);
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(msaa_color));
        gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, samples, glow::RGBA8, w, h);
        gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(msaa_color));
      }
      _ => {
        gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH24_STENCIL8, w, h);
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(color), 0);
      }
    }
    gl.framebuffer_renderbuffer(
      glow::FRAMEBUFFER,
      glow::DEPTH_STENCIL_ATTACHMENT,
      glow::RENDERBUFFER,
      Some(depth_stencil),
    );
    gl.bind_renderbuffer(glow::RENDERBUFFER, None);
    let mut complete = gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
    if let Some(resolve) = resolve {
      gl.bind_framebuffer(glow::FRAMEBUFFER, Some(resolve));
      gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(color), 0);
      complete &= gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
    }
    let entry = Entry {
      fbo,
      color,
      depth_stencil,
      msaa: msaa_color.zip(resolve),
      size,
      samples,
      in_use: false,
      last_used: self.draw,
    };
    if !complete {
      entry.delete(gl);
      return Err(Failure::Incomplete);
    }
    Ok(entry)
  }
}
