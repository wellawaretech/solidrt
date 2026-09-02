use impellers::Texture;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
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
  pub(crate) fn anisotropy_slot(anisotropy: u8) -> usize {
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
/// four). R32f/Rgba32f are the float data-texture formats (heights fetched in
/// a vertex stage, bone matrices at scale, lookup tables wider than 8 bits):
/// upload-and-sample only, nearest/texelFetch only (see `is_float`), never
/// readable back or copied (float is not color-renderable in core GLES 3.0).
/// Shader/pipeline targets are always RGBA8; this only applies to pixel
/// uploads.
///
/// Reserved future values of the same app-facing vocabulary, so they slot in
/// without an API rethink: "etc2-rgba8" (compressed uploads; changes
/// `byte_len` to block sizing and the upload verb to glCompressedTexImage2D)
/// and "rgba8-srgb" (linear-space rendering). Grammar: base layout plus a
/// qualifier suffix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum TextureFormat {
  #[default]
  Rgba8,
  R8,
  /// One float per pixel, sampled as `(v, 0, 0, 1)` (read `.r`).
  R32f,
  /// Four floats per pixel - a vec4 (or mat4 column) per texel.
  Rgba32f,
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
      Some("r32f") => Ok(TextureFormat::R32f),
      Some("rgba32f") => Ok(TextureFormat::Rgba32f),
      Some(other) => Err(format!("unknown format '{other}' (expected \"rgba8\", \"r8\", \"r32f\" or \"rgba32f\")")),
    }
  }

  /// The byte length of one frame at this format. The sizing seam every
  /// validation site goes through: today all formats are per-pixel, and a
  /// future block-compressed format changes only this function.
  pub fn byte_len(self, width: u32, height: u32) -> usize {
    let per_pixel = match self {
      TextureFormat::Rgba8 => 4,
      TextureFormat::R8 => 1,
      TextureFormat::Rg8 => 2,
      TextureFormat::Depth24 => 4,
      TextureFormat::R32f => 4,
      TextureFormat::Rgba32f => 16,
    };
    (width as usize) * (height as usize) * per_pixel
  }

  /// Whether this is a float data format: upload-and-sample only, and
  /// nearest-only sampling - linear float filtering needs
  /// OES_texture_float_linear (and RGBA32F is never filterable in core), so
  /// nearest/texelFetch is the portable contract.
  pub fn is_float(self) -> bool {
    matches!(self, TextureFormat::R32f | TextureFormat::Rgba32f)
  }

  /// The app-facing name, for the resource inventory and error messages.
  pub fn name(self) -> &'static str {
    match self {
      TextureFormat::Rgba8 => "rgba8",
      TextureFormat::R8 => "r8",
      TextureFormat::Rg8 => "rg8",
      TextureFormat::Depth24 => "depth24",
      TextureFormat::R32f => "r32f",
      TextureFormat::Rgba32f => "rgba32f",
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

  /// Parse the app-facing options against the texture's declared format.
  /// Same vocabulary as `parse`; a float format flips the filter default to
  /// nearest and refuses linear, mipmaps and anisotropy outright (nearest/
  /// texelFetch is the portable float contract - see
  /// `TextureFormat::is_float`).
  pub fn parse_for(format: TextureFormat, opts: &SamplerOptions<'_>) -> Result<Self, String> {
    let mut state = Self::parse(opts)?;
    if format.is_float() {
      if opts.filter.is_none() {
        state.filter = SamplerFilter::Nearest;
      }
      if state.filter == SamplerFilter::Linear {
        return Err(format!(
          "{} textures sample nearest-only (float linear filtering is not in core GLES 3.0); drop filter: \"linear\"",
          format.name()
        ));
      }
      if state.mipmap {
        return Err(format!("{} textures cannot carry a mip chain; drop mipmap: true", format.name()));
      }
      if state.anisotropy > MIN_ANISOTROPY {
        return Err(format!("{} textures sample nearest-only; drop anisotropy", format.name()));
      }
    }
    Ok(state)
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
