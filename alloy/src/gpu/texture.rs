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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
  /// Anisotropic filtering level (Three's `texture.anisotropy`): the most
  /// taps a shader sample may take along the axis a surface is foreshortened
  /// on, so a ground plane seen at a grazing angle stays sharp instead of
  /// smearing into the mip level its long axis picks. Always a power of two
  /// in `MIN_ANISOTROPY..=MAX_ANISOTROPY`, 1 = off (the default). Id state
  /// like the mip flag (not overridable per binding), only meaningful with a
  /// chain to minify through, ignored by the `<texture>` display draw. The
  /// device clamp lives where the sampler objects are built: without
  /// `GL_EXT_texture_filter_anisotropic` every level samples as 1.
  pub anisotropy: u8,
}

/// The anisotropy level range: 1 is off, 16 is the most any GL exposes
/// (`MAX_TEXTURE_MAX_ANISOTROPY_EXT` never exceeds it in practice).
pub const MIN_ANISOTROPY: u8 = 1;
pub const MAX_ANISOTROPY: u8 = 16;
/// The distinct levels a SamplerState may carry (1, 2, 4, 8, 16): the
/// sampler cache enumerates one object per level.
pub const ANISOTROPY_LEVELS: usize = 5;

impl SamplerState {
  /// The fixed sampling of a depth texture id: NEAREST (the only complete
  /// filter without a comparison mode), clamped, no chain. Overrides on a
  /// binding may still ask for linear; they get an incomplete sample, so
  /// consumers filter in the shader (PCF).
  pub const DEPTH: SamplerState =
    SamplerState { filter: SamplerFilter::Nearest, wrap: SamplerWrap::Clamp, mipmap: false, anisotropy: MIN_ANISOTROPY };

  /// The cache slot of an anisotropy level: log2 of the (power-of-two) level.
  fn anisotropy_slot(anisotropy: u8) -> usize {
    anisotropy.max(MIN_ANISOTROPY).trailing_zeros() as usize
  }
}

impl Default for SamplerState {
  fn default() -> Self {
    SamplerState { filter: SamplerFilter::default(), wrap: SamplerWrap::default(), mipmap: false, anisotropy: MIN_ANISOTROPY }
  }
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
  /// A draw target's depth texture (`DepthStorage::Texture`), registered
  /// under its own id: 24-bit window depth in 0..1, sampled as `(d, 0, 0,
  /// 1)` (read `.r`). Render-written only - never uploadable (`parse`
  /// refuses it), never read back or copied - and sampling-complete only at
  /// NEAREST without a comparison mode, which its registry entry declares.
  Depth24,
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
      TextureFormat::Depth24 => 4,
    }
  }

  /// The app-facing name, for the resource inventory and error messages.
  pub fn name(self) -> &'static str {
    match self {
      TextureFormat::Rgba8 => "rgba8",
      TextureFormat::R8 => "r8",
      TextureFormat::Rg8 => "rg8",
      TextureFormat::Depth24 => "depth24",
    }
  }
}

/// The app-facing sampling options as written at a create call, before
/// validation: every field optional, absent = the default. One struct
/// rather than positional arguments so a new sampling axis is one field
/// here and one read in the plugin, not a signature change at every
/// caller.
#[derive(Clone, Copy, Debug, Default)]
pub struct SamplerOptions<'a> {
  pub filter: Option<&'a str>,
  pub wrap: Option<&'a str>,
  pub mipmap: Option<bool>,
  pub anisotropy: Option<f64>,
}

/// A per-binding deviation from a texture's declared sampling: the filter
/// and/or wrap one pass samples it with (a nearest atlas blurred linearly by
/// a blur pass, a clamped target tiled by one consumer). Never the mip
/// flag: the chain either exists on the id or it does not. Empty means "the
/// texture's own state", the default for every binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct SamplerOverride {
  pub filter: Option<SamplerFilter>,
  pub wrap: Option<SamplerWrap>,
}

impl SamplerFilter {
  pub fn name(self) -> &'static str {
    match self {
      SamplerFilter::Linear => "linear",
      SamplerFilter::Nearest => "nearest",
    }
  }
}

impl SamplerWrap {
  pub fn name(self) -> &'static str {
    match self {
      SamplerWrap::Clamp => "clamp",
      SamplerWrap::Repeat => "repeat",
    }
  }
}

impl SamplerOverride {
  /// Parse the app-facing override strings, same vocabulary as the
  /// creation-time options.
  pub fn parse(filter: Option<&str>, wrap: Option<&str>) -> Result<Self, String> {
    let state = SamplerState::parse(&SamplerOptions { filter, wrap, ..SamplerOptions::default() })?;
    Ok(SamplerOverride { filter: filter.map(|_| state.filter), wrap: wrap.map(|_| state.wrap) })
  }

  pub fn is_empty(&self) -> bool {
    self.filter.is_none() && self.wrap.is_none()
  }
}

impl SamplerState {
  /// The state a binding samples with: the texture's own, with the
  /// override's fields replacing it where set. The mip flag is untouched.
  pub fn overridden(self, o: &SamplerOverride) -> SamplerState {
    SamplerState {
      filter: o.filter.unwrap_or(self.filter),
      wrap: o.wrap.unwrap_or(self.wrap),
      mipmap: self.mipmap,
      anisotropy: self.anisotropy,
    }
  }

  /// Parse the app-facing options (each optional, defaulting to
  /// linear/clamp/no mips/no anisotropy). Filter: "linear" | "nearest";
  /// wrap: "clamp" | "repeat"; mipmap: bool; anisotropy: a number >= 1,
  /// rounded down to a power of two and capped at MAX_ANISOTROPY (the
  /// engines' clamp-not-error semantics: a level is a wish, the hardware
  /// quantizes it anyway). Below 1 or non-finite is the one error.
  pub fn parse(opts: &SamplerOptions<'_>) -> Result<Self, String> {
    let SamplerOptions { filter, wrap, mipmap, anisotropy } = *opts;
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
    let anisotropy = match anisotropy {
      None => MIN_ANISOTROPY,
      Some(a) if a.is_finite() && a >= f64::from(MIN_ANISOTROPY) => {
        // Round down to the power of two at or below, capped at the ceiling.
        let level = a.min(f64::from(MAX_ANISOTROPY)) as u8;
        1 << (u8::BITS - 1 - level.leading_zeros())
      }
      Some(a) => return Err(format!("anisotropy {a} must be a number >= 1 (1 = off, up to 16)")),
    };
    Ok(SamplerState { filter, wrap, mipmap: mipmap.unwrap_or(false), anisotropy })
  }
}

/// The GL sampler objects covering every SamplerState combination (filter x
/// wrap x mipmap x anisotropy level), created once on the raster thread and
/// never freed (process-lifetime). Alloy's own passes bind one of these
/// alongside each sampled texture unit: a bound sampler object overrides
/// texture-object parameters, so per-texture sampling state holds
/// regardless of what Impeller writes into the texture objects it draws,
/// and nothing alloy sets leaks into Impeller's own draws (the pass unbinds
/// on exit). The anisotropy levels are clamped to the device maximum here,
/// the one place GL exists: a state keeps the app's requested level, the
/// object behind it samples at what the device can do (1 without
/// `GL_EXT_texture_filter_anisotropic`).
pub struct SamplerCache {
  samplers: [glow::Sampler; SamplerCache::COUNT],
}

impl SamplerCache {
  pub(crate) const COUNT: usize = 2 * 2 * 2 * ANISOTROPY_LEVELS;

  pub fn new(gl: &glow::Context, max_anisotropy: u32) -> Self {
    let mut samplers = [None; Self::COUNT];
    for filter in [SamplerFilter::Linear, SamplerFilter::Nearest] {
      for wrap in [SamplerWrap::Clamp, SamplerWrap::Repeat] {
        for mipmap in [false, true] {
          for slot in 0..ANISOTROPY_LEVELS {
            let anisotropy = MIN_ANISOTROPY << slot;
            let state = SamplerState { filter, wrap, mipmap, anisotropy };
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
            let device_level = u32::from(anisotropy).min(max_anisotropy);
            unsafe {
              let sampler = gl.create_sampler().expect("glGenSamplers failed");
              gl.sampler_parameter_i32(sampler, glow::TEXTURE_MIN_FILTER, min as i32);
              gl.sampler_parameter_i32(sampler, glow::TEXTURE_MAG_FILTER, mag as i32);
              gl.sampler_parameter_i32(sampler, glow::TEXTURE_WRAP_S, wrap_st as i32);
              gl.sampler_parameter_i32(sampler, glow::TEXTURE_WRAP_T, wrap_st as i32);
              // The parameter is only legal with the extension; a device
              // without it reports a maximum of 1 and the write is skipped.
              if device_level > 1 {
                gl.sampler_parameter_f32(sampler, glow::TEXTURE_MAX_ANISOTROPY_EXT, device_level as f32);
              }
              samplers[Self::index(state)] = Some(sampler);
            }
          }
        }
      }
    }
    SamplerCache { samplers: samplers.map(|s| s.expect("all sampler states populated")) }
  }

  pub fn get(&self, state: SamplerState) -> glow::Sampler {
    self.samplers[Self::index(state)]
  }

  pub(crate) fn index(state: SamplerState) -> usize {
    let base = (state.filter as usize) * 4 + (state.wrap as usize) * 2 + (state.mipmap as usize);
    base * ANISOTROPY_LEVELS + SamplerState::anisotropy_slot(state.anisotropy)
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
    let (internal, layout, ty) = match format {
      TextureFormat::Rgba8 => (glow::RGBA8, glow::RGBA, glow::UNSIGNED_BYTE),
      TextureFormat::R8 => (glow::R8, glow::RED, glow::UNSIGNED_BYTE),
      TextureFormat::Rg8 => (glow::RG8, glow::RG, glow::UNSIGNED_BYTE),
      TextureFormat::Depth24 => (glow::DEPTH_COMPONENT24, glow::DEPTH_COMPONENT, glow::UNSIGNED_INT),
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
        layout,
        ty,
        glow::PixelUnpackData::Slice(None),
      );
      // No mips exist: the default MIN_FILTER references mipmaps, which would
      // make the texture sampling-incomplete (reads as black) when Impeller
      // samples it. Completeness fallback only - the declared SamplerState is
      // applied via sampler objects in alloy's passes and via per-draw
      // sampling in Impeller, never through these parameters (Impeller
      // rewrites them on every draw of the texture). A depth texture is only
      // complete at NEAREST.
      let filter = if format == TextureFormat::Depth24 { glow::NEAREST } else { glow::LINEAR };
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter as i32);
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter as i32);
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
      TextureFormat::Depth24 => {
        // Gated UI-side (a depth id is not an upload texture); backstop.
        log::warn!("[alloy] upload into a depth texture ignored: depth is render-written");
        return;
      }
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
