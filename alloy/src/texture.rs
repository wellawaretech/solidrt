use glow::HasContext;
use impellers::{ISize, Texture};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;

/// How a texture is sampled, everywhere it is sampled: filter, wrap mode and
/// whether a mip chain exists, declared at creation as a property of the
/// texture id. One
/// state for both consumers - shader passes (applied via a bound GL sampler
/// object) and `<texture>` display (the filter maps to Impeller's per-draw
/// sampling). Never stored as GL texture-object state: Impeller configures
/// sampling by mutating the parameters of whatever texture it draws, so
/// object state on a displayed texture does not survive a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct SamplerState {
  pub filter: SamplerFilter,
  pub wrap: SamplerWrap,
  /// The id keeps a mip chain: regenerated after every upload (data
  /// textures) or render (targets), and shader passes minify through it
  /// (trilinear for `Linear`, NEAREST_MIPMAP_LINEAR for `Nearest`). The
  /// `<texture>` display draw samples level 0 only (Impeller per-draw
  /// sampling). Id state, not overridable per binding: a sampler asking for
  /// mip levels on a texture without a chain is sampling-incomplete.
  pub mipmap: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum SamplerFilter {
  #[default]
  Linear,
  Nearest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum SamplerWrap {
  #[default]
  Clamp,
  Repeat,
}

/// Pixel format of an uploaded texture, declared at creation as a property of
/// the id (like SamplerState). Rgba8 is the default. R8 is the single-channel
/// path for palette-indexed or grayscale content: one byte per pixel, sampled
/// in GLSL as `(v, 0, 0, 1)` (read `.r`), and uploads set unpack alignment 1
/// so any width works with no 4-byte row padding - the point of the format
/// (packing indices into RGBA texels is only free when the width divides by
/// four). Shader/pipeline targets are always RGBA8; this only applies to
/// pixel uploads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum TextureFormat {
  #[default]
  Rgba8,
  R8,
  /// Two channels, one byte each, sampled as `(r, g, 0, 1)`. Exists for the
  /// interleaved UV plane of NV12 YUV textures (see `yuv`); not offered in
  /// the app-facing `parse` until an app-level consumer exists.
  Rg8,
}

impl TextureFormat {
  /// Parse the app-facing option string (optional, defaulting to rgba8).
  pub fn parse(format: Option<&str>) -> Result<Self, String> {
    match format {
      None | Some("rgba8") => Ok(TextureFormat::Rgba8),
      Some("r8") => Ok(TextureFormat::R8),
      Some(other) => Err(format!("unknown format '{other}' (expected \"rgba8\" or \"r8\")")),
    }
  }

  pub fn bytes_per_pixel(self) -> usize {
    match self {
      TextureFormat::Rgba8 => 4,
      TextureFormat::R8 => 1,
      TextureFormat::Rg8 => 2,
    }
  }

  /// The app-facing name, for the resource inventory and error messages.
  pub fn name(self) -> &'static str {
    match self {
      TextureFormat::Rgba8 => "rgba8",
      TextureFormat::R8 => "r8",
      TextureFormat::Rg8 => "rg8",
    }
  }
}

impl SamplerState {
  /// Parse the app-facing options (each optional, defaulting to
  /// linear/clamp/no mips). Filter: "linear" | "nearest"; wrap: "clamp" |
  /// "repeat"; mipmap: bool.
  pub fn parse(filter: Option<&str>, wrap: Option<&str>, mipmap: Option<bool>) -> Result<Self, String> {
    let filter = match filter {
      None | Some("linear") => SamplerFilter::Linear,
      Some("nearest") => SamplerFilter::Nearest,
      Some(other) => return Err(format!("unknown filter '{other}' (expected \"linear\" or \"nearest\")")),
    };
    let wrap = match wrap {
      None | Some("clamp") => SamplerWrap::Clamp,
      Some("repeat") => SamplerWrap::Repeat,
      Some(other) => return Err(format!("unknown wrap '{other}' (expected \"clamp\" or \"repeat\")")),
    };
    Ok(SamplerState { filter, wrap, mipmap: mipmap.unwrap_or(false) })
  }
}

/// The eight GL sampler objects covering every SamplerState combination,
/// created once on the raster thread and never freed (process-lifetime).
/// Alloy's own passes bind one of these alongside each sampled texture unit:
/// a bound sampler object overrides texture-object parameters, so per-texture
/// sampling state holds regardless of what Impeller writes into the texture
/// objects it draws, and nothing alloy sets leaks into Impeller's own draws
/// (the pass unbinds on exit).
pub struct SamplerCache {
  samplers: [glow::Sampler; 8],
}

impl SamplerCache {
  pub fn new(gl: &glow::Context) -> Self {
    let mut samplers = [None; 8];
    for filter in [SamplerFilter::Linear, SamplerFilter::Nearest] {
      for wrap in [SamplerWrap::Clamp, SamplerWrap::Repeat] {
        for mipmap in [false, true] {
          let state = SamplerState { filter, wrap, mipmap };
          let mag = match filter {
            SamplerFilter::Linear => glow::LINEAR,
            SamplerFilter::Nearest => glow::NEAREST,
          };
          // Minification through the chain picks the nearer two levels and
          // blends them; within a level the declared filter applies.
          let min = match (filter, mipmap) {
            (_, false) => mag,
            (SamplerFilter::Linear, true) => glow::LINEAR_MIPMAP_LINEAR,
            (SamplerFilter::Nearest, true) => glow::NEAREST_MIPMAP_LINEAR,
          };
          let wrap_st = match wrap {
            SamplerWrap::Clamp => glow::CLAMP_TO_EDGE,
            SamplerWrap::Repeat => glow::REPEAT,
          };
          unsafe {
            let sampler = gl.create_sampler().expect("glGenSamplers failed");
            gl.sampler_parameter_i32(sampler, glow::TEXTURE_MIN_FILTER, min as i32);
            gl.sampler_parameter_i32(sampler, glow::TEXTURE_MAG_FILTER, mag as i32);
            gl.sampler_parameter_i32(sampler, glow::TEXTURE_WRAP_S, wrap_st as i32);
            gl.sampler_parameter_i32(sampler, glow::TEXTURE_WRAP_T, wrap_st as i32);
            samplers[Self::index(state)] = Some(sampler);
          }
        }
      }
    }
    SamplerCache { samplers: samplers.map(|s| s.expect("all eight sampler states populated")) }
  }

  pub fn get(&self, state: SamplerState) -> glow::Sampler {
    self.samplers[Self::index(state)]
  }

  fn index(state: SamplerState) -> usize {
    (state.filter as usize) * 4 + (state.wrap as usize) * 2 + (state.mipmap as usize)
  }
}

/// The UI thread's view of a registered texture: the adopted Impeller handle
/// (all a display list needs) plus dimensions for layout measure and update
/// validation, and the sampler state (the paint walk picks the display
/// sampling from its filter). The GL name behind it lives in the raster
/// thread's map.
pub struct TextureEntry {
  pub impeller: Texture,
  pub width: u32,
  pub height: u32,
  pub sampler: SamplerState,
  /// Pixel format of the id (rgba8 unless created as r8); sizes update and
  /// resize validation. Display of an r8 texture shows the red channel only
  /// (Impeller samples it as `(v, 0, 0, 1)` like any shader would).
  pub format: TextureFormat,
}

impl TextureEntry {
  pub fn width(&self) -> u32 {
    self.width
  }
  pub fn height(&self) -> u32 {
    self.height
  }
  pub fn sampler(&self) -> SamplerState {
    self.sampler
  }
}

impl std::ops::Deref for TextureEntry {
  type Target = Texture;
  fn deref(&self) -> &Texture {
    &self.impeller
  }
}

pub struct TextureRegistry {
  entries: RefCell<HashMap<u64, Rc<TextureEntry>>>,
  next_id: RefCell<u64>,
  // Bumped on insert (create or replace at an id). Content uploads into an
  // existing texture do not count. Lets a painter detect that a retained
  // display list may reference replaced textures and must be rebuilt.
  generation: Cell<u64>,
}

impl TextureRegistry {
  pub(crate) fn new() -> Self {
    TextureRegistry { entries: RefCell::new(HashMap::new()), next_id: RefCell::new(1), generation: Cell::new(0) }
  }

  pub fn get(&self, id: u64) -> Option<Rc<TextureEntry>> {
    self.entries.borrow().get(&id).map(Rc::clone)
  }

  /// Number of textures currently held in the registry.
  pub fn len(&self) -> usize {
    self.entries.borrow().len()
  }

  /// (id, width, height) of every registered texture, unordered.
  pub fn list(&self) -> Vec<(u64, u32, u32)> {
    self.entries.borrow().iter().map(|(id, e)| (*id, e.width(), e.height())).collect()
  }

  pub fn is_empty(&self) -> bool {
    self.entries.borrow().is_empty()
  }

  pub fn insert(&self, id: u64, entry: TextureEntry) {
    self.entries.borrow_mut().insert(id, Rc::new(entry));
    self.generation.set(self.generation.get().wrapping_add(1));
  }

  pub fn generation(&self) -> u64 {
    self.generation.get()
  }

  pub fn allocate_id(&self) -> u64 {
    let mut id = self.next_id.borrow_mut();
    let result = *id;
    *id += 1;
    result
  }

  pub fn remove(&self, id: u64) -> Option<Rc<TextureEntry>> {
    self.entries.borrow_mut().remove(&id)
  }
}

/// A GL texture that is adopted into Impeller right after creation. Impeller
/// takes ownership of the GL name and deletes it when its Texture drops, so
/// GpuTexture deliberately does NOT delete the name (no Drop impl) - doing so
/// would double-free the name and corrupt whatever live texture reuses it.
/// Raster-thread-only: creation and uploads are GL work.
pub struct GpuTexture {
  pub gl_texture: glow::Texture,
  pub width: u32,
  pub height: u32,
  /// Declared sampling for this id; shader passes resolve it to a sampler
  /// object at bind time. The texture-object parameters set below are only a
  /// completeness fallback, not this state's storage.
  pub sampler: SamplerState,
  /// Pixel format, fixed at creation; uploads size and unpack against it.
  pub format: TextureFormat,
  /// Free-form debug name from the create (WebGPU's label), surfaced in the
  /// resource inventory and raster-side messages. Not unique; survives
  /// id-stable resizes (the raster side inherits it on replace-at-id).
  pub label: Option<String>,
}

/// Rebuild the mip chain of `texture` from its level 0 (glGenerateMipmap
/// allocates the levels on first use). Raster-thread GL; restores the
/// texture binding it touches.
pub fn generate_mipmap(gl: &glow::Context, texture: glow::Texture) {
  unsafe {
    let prev = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
    gl.bind_texture(glow::TEXTURE_2D, Some(texture));
    gl.generate_mipmap(glow::TEXTURE_2D);
    gl.bind_texture(glow::TEXTURE_2D, NonZeroU32::new(prev as u32).map(glow::NativeTexture));
  }
}

impl GpuTexture {
  pub fn new(gl: &glow::Context, size: ISize, sampler: SamplerState, format: TextureFormat) -> Self {
    let (width, height) = (size.width as u32, size.height as u32);
    let internal = match format {
      TextureFormat::Rgba8 => glow::RGBA8,
      TextureFormat::R8 => glow::R8,
      TextureFormat::Rg8 => glow::RG8,
    };
    unsafe {
      let prev = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
      let gl_texture = gl.create_texture().expect("glGenTextures failed");
      gl.bind_texture(glow::TEXTURE_2D, Some(gl_texture));
      gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        internal as i32,
        width as i32,
        height as i32,
        0,
        match format {
          TextureFormat::Rgba8 => glow::RGBA,
          TextureFormat::R8 => glow::RED,
          TextureFormat::Rg8 => glow::RG,
        },
        glow::UNSIGNED_BYTE,
        glow::PixelUnpackData::Slice(None),
      );
      // No mips exist: the default MIN_FILTER references mipmaps, which would
      // make the texture sampling-incomplete (reads as black) when Impeller
      // samples it. Completeness fallback only - the declared SamplerState is
      // applied via sampler objects in alloy's passes and via per-draw
      // sampling in Impeller, never through these parameters (Impeller
      // rewrites them on every draw of the texture).
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
      gl.bind_texture(glow::TEXTURE_2D, NonZeroU32::new(prev as u32).map(glow::NativeTexture));
      GpuTexture { gl_texture, width, height, sampler, format, label: None }
    }
  }

  pub fn upload(&self, gl: &glow::Context, data: &[u8], size: ISize) {
    let (width, height) = (size.width as i32, size.height as i32);
    // RGBA8 rows are width*4, always a multiple of 4, so the default unpack
    // alignment holds. R8 rows are width*1 and must unpack at alignment 1 or
    // any width not divisible by 4 reads rows off by their padding - the
    // whole reason the format exists is to avoid that per-frame repacking.
    // RG8 rows are width*2; alignment 1 is correct for every width.
    let (gl_format, alignment) = match self.format {
      TextureFormat::Rgba8 => (glow::RGBA, 4),
      TextureFormat::R8 => (glow::RED, 1),
      TextureFormat::Rg8 => (glow::RG, 1),
    };
    unsafe {
      let prev = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
      gl.bind_texture(glow::TEXTURE_2D, Some(self.gl_texture));
      gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, alignment);
      gl.tex_sub_image_2d(
        glow::TEXTURE_2D,
        0,
        0,
        0,
        width,
        height,
        gl_format,
        glow::UNSIGNED_BYTE,
        glow::PixelUnpackData::Slice(Some(data)),
      );
      // Unpack alignment is context state shared with Impeller's own uploads
      // (glyph atlas), which assume the GL default of 4; restore it.
      if alignment != 4 {
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
      }
      if self.sampler.mipmap {
        gl.generate_mipmap(glow::TEXTURE_2D);
      }
      gl.bind_texture(glow::TEXTURE_2D, NonZeroU32::new(prev as u32).map(glow::NativeTexture));
      // No glFinish: the texture is sampled later on this same (single) GL
      // context, so program order already sequences the upload first.
    }
  }
}
