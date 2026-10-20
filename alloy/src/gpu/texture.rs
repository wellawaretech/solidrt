use crate::renderer::Texture;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// How a texture is sampled, everywhere it is sampled: filter, wrap mode and
/// whether a mip chain exists, declared at creation as a property of the
/// texture id. One
/// state for both consumers - shader passes (applied via a bound GL sampler
/// object) and `<texture>` display (the filter maps to the renderer's per-draw
/// sampling). Never stored as GL texture-object state: the renderer configures
/// sampling by mutating the parameters of whatever texture it draws, so
/// object state on a displayed texture does not survive a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SamplerState {
  pub filter: SamplerFilter,
  pub wrap: SamplerWrap,
  /// The id keeps a mip chain: regenerated after every upload (data
  /// textures) or render (targets), and shader passes minify through it
  /// (trilinear for `Linear`, NEAREST_MIPMAP_LINEAR for `Nearest`). The
  /// `<texture>` display draw samples level 0 only (renderer per-draw
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
  /// The first mip level the id samples (Unity's mipmap limit, a texture
  /// quality knob): 0 is the full-size level, 1 skips it and samples from
  /// the half-size level down, a quarter of the texel bandwidth and
  /// memory traffic for every tap. Texture-object state
  /// (`TEXTURE_BASE_LEVEL`), so it is applied to the object rather than
  /// through a sampler object; only meaningful with a chain (`mipmap`),
  /// and below the chain's level count - both validated UI-side. A render
  /// target's output is its level 0 and refuses a base level.
  pub base_level: u8,
}

/// The anisotropy level range: 1 is off, 16 is the most any GL exposes
/// (`MAX_TEXTURE_MAX_ANISOTROPY_EXT` never exceeds it in practice).
pub const MIN_ANISOTROPY: u8 = 1;
pub const MAX_ANISOTROPY: u8 = 16;
/// The distinct levels a SamplerState may carry (1, 2, 4, 8, 16): the
/// sampler cache enumerates one object per level.
pub const ANISOTROPY_LEVELS: usize = 5;

/// One rect of an upload into an existing texture (`update_texture_rects`):
/// where it lands in level 0, in texels, and exactly its `width` x `height`
/// pixels at the texture's format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureRect {
  pub x: u32,
  pub y: u32,
  pub width: u32,
  pub height: u32,
  pub pixels: Vec<u8>,
}

impl SamplerState {
  /// The fixed sampling of a depth texture id: NEAREST (the only complete
  /// filter without a comparison mode), clamped, no chain. Overrides on a
  /// binding may still ask for linear; they get an incomplete sample, so
  /// consumers filter in the shader (PCF).
  pub const DEPTH: SamplerState = SamplerState {
    filter: SamplerFilter::Nearest,
    wrap: SamplerWrap::Clamp,
    mipmap: false,
    anisotropy: MIN_ANISOTROPY,
    base_level: 0,
  };

  /// The cache slot of an anisotropy level: log2 of the (power-of-two) level.
  pub(crate) fn anisotropy_slot(anisotropy: u8) -> usize {
    anisotropy.max(MIN_ANISOTROPY).trailing_zeros() as usize
  }
}

impl Default for SamplerState {
  fn default() -> Self {
    SamplerState {
      filter: SamplerFilter::default(),
      wrap: SamplerWrap::default(),
      mipmap: false,
      anisotropy: MIN_ANISOTROPY,
      base_level: 0,
    }
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
/// upload-and-sample only, nearest/texelFetch only (see `filterable`), never
/// readable back or copied (float is not color-renderable in core GLES 3.0).
/// Rgba16f is the HDR image format (environment maps, panoramas): the same
/// float payload at the boundary, stored as half float, and filterable like
/// a byte format because GLES 3.0 lists RGBA16F as texture-filterable.
/// The integer formats (`is_integer`) are shader data fetched through a
/// `usampler*` as unsigned integers, nothing converted: Rgba32ui is four u32
/// per texel (packed records a shader unpacks with `uintBitsToFloat`,
/// `unpackHalf2x16` and `unpackUnorm4x8` - a splat cloud's records, any
/// bit-exact table; a Uint32Array payload), and R8ui/Rg8ui/Rgba8ui are one,
/// two or four bytes per texel read as plain integers (indices into a
/// palette or codebook, labels, small ids; a Uint8Array payload, the byte
/// formats' row alignment). All are upload-and-sample only and
/// nearest/texelFetch only like the 32-bit floats (integer textures never
/// filter).
/// Rgba8Srgb is an rgba8 whose stored bytes are sRGB-encoded: sampling
/// decodes them to linear light in hardware, before filtering, so the
/// filter and the mip chain are right - the color-map format of
/// linear-space lighting. Draw targets (2D and cube) take Rgba8, Rgba8Srgb
/// or, where half float is color-renderable, Rgba16f
/// (`GpuLimits::check_render_format`); a non-Rgba8 draw target is
/// sampler-only like the uploads of those formats. Shader and pipeline
/// textures are always RGBA8.
///
/// The block-compressed formats (`is_compressed`) store 4x4 texel blocks of
/// 16 bytes, 1 byte per texel, decoded by the sampler: Etc2Rgba8 (and its
/// sRGB twin) is GLES 3.0 core, so every device takes it, though a desktop
/// driver without the hardware format expands it to RGBA8 on upload;
/// Bc7Rgba8 (and its sRGB twin) needs `GpuLimits::bc7_textures`
/// (EXT_texture_compression_bptc: ANGLE over D3D11 and Metal, Mesa
/// desktop), the format every desktop decodes in hardware. The payload is
/// raw blocks, the FULL mip chain level-major when the sampling declares
/// one (`chain_byte_len`): glGenerateMipmap cannot run on compressed
/// storage, so the chain comes from the file. Create-once (no upload or
/// resize, like a cube map), sample-only like the float formats, filterable
/// like the byte formats. The one shipping-portable raw form is ETC2; BC7 is
/// a load-time target picked from the limits (the transcode path in forge).
/// Grammar: an optional codec prefix, the base layout, a qualifier suffix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum TextureFormat {
  #[default]
  Rgba8,
  R8,
  /// One float per pixel, sampled as `(v, 0, 0, 1)` (read `.r`).
  R32f,
  /// Four floats per pixel - a vec4 (or mat4 column) per texel.
  Rgba32f,
  /// Four half floats per pixel: the HDR image format. Filterable (see
  /// `filterable`); a generated mip chain needs half float to be
  /// color-renderable (`GpuLimits::half_float_renderable`), which
  /// glGenerateMipmap requires of every format.
  Rgba16f,
  /// Four u32 per pixel - a uvec4 per texel through a `usampler2D`: the
  /// bit-exact data-texture format (packed records a shader unpacks).
  /// Nearest-only and sample-only like the 32-bit floats.
  Rgba32ui,
  /// One byte per pixel read as an integer - a uint through a `usampler2D`
  /// (`.r`): an index or label per texel. Integer rules as Rgba32ui.
  R8ui,
  /// Two bytes per pixel read as integers - a uvec2 (`.rg`).
  Rg8ui,
  /// Four bytes per pixel read as integers - a uvec4.
  Rgba8ui,
  /// Rgba8 stored sRGB-encoded (SRGB8_ALPHA8): sampling decodes RGB to
  /// linear light in hardware, alpha stays linear. Upload-and-sample only
  /// like the float formats: the readback path samples (decodes) instead of
  /// returning the stored bytes, so there is no readback or copy.
  Rgba8Srgb,
  /// ETC2/EAC RGBA8 blocks (COMPRESSED_RGBA8_ETC2_EAC), GLES 3.0 core.
  Etc2Rgba8,
  /// ETC2/EAC blocks stored sRGB-encoded (COMPRESSED_SRGB8_ALPHA8_ETC2_EAC),
  /// GLES 3.0 core; decoded to linear light on sample like Rgba8Srgb.
  Etc2Rgba8Srgb,
  /// BC7 blocks (COMPRESSED_RGBA_BPTC_UNORM), behind
  /// `GpuLimits::bc7_textures`.
  Bc7Rgba8,
  /// BC7 blocks stored sRGB-encoded (COMPRESSED_SRGB_ALPHA_BPTC_UNORM),
  /// behind `GpuLimits::bc7_textures`.
  Bc7Rgba8Srgb,
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
      Some("rgba16f") => Ok(TextureFormat::Rgba16f),
      Some("rgba32ui") => Ok(TextureFormat::Rgba32ui),
      Some("r8ui") => Ok(TextureFormat::R8ui),
      Some("rg8ui") => Ok(TextureFormat::Rg8ui),
      Some("rgba8ui") => Ok(TextureFormat::Rgba8ui),
      Some("rgba8-srgb") => Ok(TextureFormat::Rgba8Srgb),
      Some("etc2-rgba8") => Ok(TextureFormat::Etc2Rgba8),
      Some("etc2-rgba8-srgb") => Ok(TextureFormat::Etc2Rgba8Srgb),
      Some("bc7-rgba8") => Ok(TextureFormat::Bc7Rgba8),
      Some("bc7-rgba8-srgb") => Ok(TextureFormat::Bc7Rgba8Srgb),
      Some(other) => Err(format!(
        "unknown format '{other}' (expected \"rgba8\", \"rgba8-srgb\", \"r8\", \"r32f\", \"rgba32f\", \"rgba16f\", \"rgba32ui\", \"r8ui\", \"rg8ui\", \"rgba8ui\", \"etc2-rgba8\", \"etc2-rgba8-srgb\", \"bc7-rgba8\" or \"bc7-rgba8-srgb\")"
      )),
    }
  }

  /// The byte length of one level at this format. The sizing seam every
  /// validation site goes through: per-pixel for the plain formats, per
  /// 4x4 block (a partial edge block counts whole) for the compressed ones.
  pub fn byte_len(self, width: u32, height: u32) -> usize {
    if self.is_compressed() {
      let blocks_x = (width as usize).div_ceil(BLOCK_EDGE);
      let blocks_y = (height as usize).div_ceil(BLOCK_EDGE);
      return blocks_x * blocks_y * BLOCK_BYTES;
    }
    let per_pixel = match self {
      TextureFormat::Rgba8 => 4,
      TextureFormat::R8 => 1,
      TextureFormat::Rg8 => 2,
      TextureFormat::Depth24 => 4,
      TextureFormat::R32f => 4,
      TextureFormat::Rgba32f => 16,
      TextureFormat::Rgba16f => 8,
      TextureFormat::Rgba32ui => 16,
      TextureFormat::R8ui => 1,
      TextureFormat::Rg8ui => 2,
      TextureFormat::Rgba8ui => 4,
      TextureFormat::Rgba8Srgb => 4,
      TextureFormat::Etc2Rgba8
      | TextureFormat::Etc2Rgba8Srgb
      | TextureFormat::Bc7Rgba8
      | TextureFormat::Bc7Rgba8Srgb => {
        unreachable!("compressed formats size by block")
      }
    };
    (width as usize) * (height as usize) * per_pixel
  }

  /// The byte length of the full mip chain of a `width` x `height` level 0
  /// at this format, level-major down to 1x1 (`mip_levels` of the longer
  /// edge, each level `byte_len` of its `mip_size` edges): what a
  /// compressed create with `mipmap` takes, since its chain is uploaded,
  /// never generated.
  pub fn chain_byte_len(self, width: u32, height: u32) -> usize {
    (0..mip_levels(width.max(height))).map(|level| self.byte_len(mip_size(width, level), mip_size(height, level))).sum()
  }

  /// Whether the format stores 4x4 blocks the sampler decodes (see the type
  /// docs): sized by `byte_len` per block, uploaded with
  /// glCompressedTexImage2D level by level, create-once.
  pub fn is_compressed(self) -> bool {
    matches!(
      self,
      TextureFormat::Etc2Rgba8 | TextureFormat::Etc2Rgba8Srgb | TextureFormat::Bc7Rgba8 | TextureFormat::Bc7Rgba8Srgb
    )
  }

  /// Whether the format is fetched through an integer sampler (`usampler*`)
  /// as unsigned integers: the 32-bit and the 8-bit integer formats alike.
  /// The sampler rule and the filtering rule both key on this.
  pub fn is_integer(self) -> bool {
    matches!(self, TextureFormat::Rgba32ui | TextureFormat::R8ui | TextureFormat::Rg8ui | TextureFormat::Rgba8ui)
  }

  /// Whether the payload is unsigned 32-bit integers (a Uint32Array in JS),
  /// stored as-is. The 8-bit integer formats take bytes like Rgba8.
  pub fn is_uint32(self) -> bool {
    self == TextureFormat::Rgba32ui
  }

  /// Whether the payload is floats: one f32 per component at the boundary
  /// (a Float32Array in JS), stored as f32 (R32f, Rgba32f) or packed to f16
  /// (Rgba16f, see `f16_bytes`).
  pub fn is_float(self) -> bool {
    matches!(self, TextureFormat::R32f | TextureFormat::Rgba32f | TextureFormat::Rgba16f)
  }

  /// Whether linear filtering (and with it a mip chain and anisotropy)
  /// applies. The 32-bit float formats are nearest-only: linear float
  /// filtering needs OES_texture_float_linear and RGBA32F is never
  /// filterable in core, so nearest/texelFetch is their portable contract;
  /// an integer format never filters at all. RGBA16F is texture-filterable
  /// in core GLES 3.0 like every byte format; depth samples nearest without
  /// a comparison mode.
  pub fn filterable(self) -> bool {
    !self.is_integer() && !matches!(self, TextureFormat::R32f | TextureFormat::Rgba32f | TextureFormat::Depth24)
  }

  /// Whether the format is sample-only, with no readback, copy or display
  /// path: the 32-bit floats are not color-renderable in core GLES 3.0, a
  /// half-float readback would quantize, an integer texture is shader
  /// data with no image meaning, and an sRGB texture's readback would
  /// sample (decode) instead of returning the stored bytes, and compressed
  /// storage is not color-renderable at all. A draw target of such a format
  /// renders (through a pass) but is read the same way: through a pass into
  /// an Rgba8 target.
  pub fn sample_only(self) -> bool {
    self.is_float() || self.is_integer() || self == TextureFormat::Rgba8Srgb || self.is_compressed()
  }

  /// Pack an f32 payload (its native-endian bytes, as a Float32Array views
  /// them) to the f16 bytes an Rgba16f upload stores, native endian as
  /// HALF_FLOAT unpacks them. The boundary converts, so alloy sees every
  /// payload at its `byte_len`.
  pub fn f16_bytes(f32_bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(f32_bytes.len() / 2);
    for c in f32_bytes.chunks_exact(4) {
      let v = f32::from_ne_bytes([c[0], c[1], c[2], c[3]]);
      out.extend_from_slice(&half::f16::from_f32(v).to_ne_bytes());
    }
    out
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
      TextureFormat::Rgba16f => "rgba16f",
      TextureFormat::Rgba32ui => "rgba32ui",
      TextureFormat::R8ui => "r8ui",
      TextureFormat::Rg8ui => "rg8ui",
      TextureFormat::Rgba8ui => "rgba8ui",
      TextureFormat::Rgba8Srgb => "rgba8-srgb",
      TextureFormat::Etc2Rgba8 => "etc2-rgba8",
      TextureFormat::Etc2Rgba8Srgb => "etc2-rgba8-srgb",
      TextureFormat::Bc7Rgba8 => "bc7-rgba8",
      TextureFormat::Bc7Rgba8Srgb => "bc7-rgba8-srgb",
    }
  }
}

/// The block geometry every compressed format here shares: ETC2/EAC RGBA8
/// and BC7 both pack a 4x4 texel block into 16 bytes.
const BLOCK_EDGE: usize = 4;
const BLOCK_BYTES: usize = 16;

/// The face count of a cube map, in GL (and app-facing) order: +X, -X, +Y,
/// -Y, +Z, -Z.
pub const CUBE_FACES: usize = 6;

/// The level count of a full mip chain from a `size` edge down to 1x1
/// (GL's rule: level k has edge `max(1, size >> k)`).
pub fn mip_levels(size: u32) -> u32 {
  u32::BITS - size.max(1).leading_zeros()
}

/// The edge of level `level` of a chain from a `size` edge.
pub fn mip_size(size: u32, level: u32) -> u32 {
  (size >> level).max(1)
}

/// Check a cube map's face list: the six base faces (+X, -X, +Y, -Y, +Z,
/// -Z), or an explicit mip chain - the FULL chain down to 1x1, level-major
/// (level 0's six faces, then level 1's, ...), which is what prefiltered
/// environment maps upload - each face `format.byte_len(edge, edge)`
/// bytes for its level's edge. Returns the level count carried: 1 for
/// the base faces (a chain, if sampled, is generated from them).
pub fn check_cube_faces(size: u32, faces: &[Vec<u8>], format: TextureFormat) -> Result<u32, String> {
  let chain = mip_levels(size);
  let levels = if faces.len() == CUBE_FACES {
    1
  } else if faces.len() == CUBE_FACES * chain as usize {
    chain
  } else {
    return Err(format!(
      "a cube map takes {CUBE_FACES} faces (+X, -X, +Y, -Y, +Z, -Z) or its full {chain}-level mip chain of {} faces, got {}",
      CUBE_FACES * chain as usize,
      faces.len()
    ));
  };
  for (i, face) in faces.iter().enumerate() {
    let level = (i / CUBE_FACES) as u32;
    let edge = mip_size(size, level);
    let expected = format.byte_len(edge, edge);
    if face.len() != expected {
      return Err(format!(
        "cube face {} of level {level} is {} bytes, expected {expected} ({edge}x{edge} {})",
        i % CUBE_FACES,
        face.len(),
        format.name()
      ));
    }
  }
  Ok(levels)
}

/// The dimensionality of a texture id, declared at creation like `format`.
/// Everything but `D2` is sampling-only: the `<texture>` display draw,
/// `readTexture`, `copyTexture` and the id-stable resize are 2D-shaped and
/// reject the id at the call site, naming the shape and its sampler.
/// `Cube` is a cube map: six square faces behind one id, sampled with a
/// `samplerCube` by direction, create-once (no upload after creation;
/// render-to-face is the cube draw target). `D3` is a volume: `depth`
/// slices of `width` x `height`, sampled with a `sampler3D` by position
/// and filtered across all three axes (hardware trilinear), the storage of
/// a brick pool, a 3D LUT or a density field. `D2Array` is an array of
/// `depth` same-size layers behind one `sampler2DArray`, filtered within a
/// layer only and picked by an unnormalized layer index (terrain splat
/// sets, equal-size sprite sheets). Both volumes are mutable: the whole
/// volume or a box of it (`TextureBox`) re-uploads in place, and the mip
/// chain regenerates after either (a 3D chain halves depth too, an array
/// chain never halves layers). Render-to-slice, compressed volumes and
/// explicit chains are not shapes of the contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum TextureShape {
  #[default]
  D2,
  Cube,
  D3,
  D2Array,
}

impl TextureShape {
  /// The app-facing name, for the resource inventory and error messages.
  pub fn name(self) -> &'static str {
    match self {
      TextureShape::D2 => "2d",
      TextureShape::Cube => "cube",
      TextureShape::D3 => "3d",
      TextureShape::D2Array => "2d-array",
    }
  }

  /// The shape as an error message names it, with its article.
  pub fn describe(self) -> &'static str {
    match self {
      TextureShape::D2 => "a 2D texture",
      TextureShape::Cube => "a cube map",
      TextureShape::D3 => "a 3D texture",
      TextureShape::D2Array => "an array texture",
    }
  }

  /// The GLSL float sampler that binds this shape, for error messages.
  pub fn sampler_name(self) -> &'static str {
    match self {
      TextureShape::D2 => "sampler2D",
      TextureShape::Cube => "samplerCube",
      TextureShape::D3 => "sampler3D",
      TextureShape::D2Array => "sampler2DArray",
    }
  }

  /// The GLSL integer sampler that binds this shape (an rgba32ui id), for
  /// error messages; a cube map has no integer sampler in the contract.
  pub fn integer_sampler_name(self) -> &'static str {
    match self {
      TextureShape::D2 => "usampler2D",
      TextureShape::Cube => "usamplerCube",
      TextureShape::D3 => "usampler3D",
      TextureShape::D2Array => "usampler2DArray",
    }
  }

  /// The app-facing create that makes an id of this shape, for error
  /// messages that say what to bind instead.
  pub fn create_name(self) -> &'static str {
    match self {
      TextureShape::D2 => "createTexture",
      TextureShape::Cube => "createCubeTexture",
      TextureShape::D3 => "createTexture3D",
      TextureShape::D2Array => "createArrayTexture",
    }
  }

  /// Whether the shape is one of the two volumes (`D3`, `D2Array`): the
  /// shapes that allocate 3D storage, upload by box and carry a depth.
  pub fn is_volume(self) -> bool {
    matches!(self, TextureShape::D3 | TextureShape::D2Array)
  }

  /// The level count of a full mip chain over a `width` x `height` x
  /// `depth` level 0 of this shape: a 3D chain halves all three axes, so
  /// its depth counts; every other shape's chain is over the face.
  pub fn mip_levels(self, width: u32, height: u32, depth: u32) -> u32 {
    match self {
      TextureShape::D3 => mip_levels(width.max(height).max(depth)),
      _ => mip_levels(width.max(height)),
    }
  }

  /// The texel storage of an id of this shape at `format`, in bytes: every
  /// level of the chain when `mipmap`, all six faces of a cube map, every
  /// slice or layer of a volume (a 3D level's depth halves with its edges,
  /// an array level keeps every layer). What the inventory reports.
  pub fn storage_byte_len(self, format: TextureFormat, width: u32, height: u32, depth: u32, mipmap: bool) -> usize {
    let levels = if mipmap { self.mip_levels(width, height, depth) } else { 1 };
    (0..levels)
      .map(|level| {
        let level_depth = if self == TextureShape::D3 { mip_size(depth, level) } else { depth };
        format.byte_len(mip_size(width, level), mip_size(height, level)) * level_depth as usize
      })
      .sum::<usize>()
      * if self == TextureShape::Cube { CUBE_FACES } else { 1 }
  }
}

/// One box of an upload into an existing volume (`update_texture_box`):
/// where it lands in level 0, in texels (`z` is the slice of a 3D texture
/// or the layer of an array), and exactly its `width` x `height` x `depth`
/// texels at the texture's format, slice-major. WebGPU's `writeTexture`
/// origin and size; GL's `texSubImage3D` with offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureBox {
  pub x: u32,
  pub y: u32,
  pub z: u32,
  pub width: u32,
  pub height: u32,
  pub depth: u32,
  pub pixels: Vec<u8>,
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
  pub base_level: Option<f64>,
}

/// A change to a texture's declared sampling after creation
/// (`set_texture_sampling`): every field optional, absent = keep the
/// current value. The mip flag is not here: the chain is storage, fixed at
/// creation.
#[derive(Clone, Copy, Debug, Default)]
pub struct SamplerUpdate<'a> {
  pub filter: Option<&'a str>,
  pub wrap: Option<&'a str>,
  pub anisotropy: Option<f64>,
  pub base_level: Option<f64>,
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
      base_level: self.base_level,
    }
  }

  /// Parse the app-facing options (each optional, defaulting to
  /// linear/clamp/no mips/no anisotropy). Filter: "linear" | "nearest";
  /// wrap: "clamp" | "repeat"; mipmap: bool; anisotropy: a number >= 1,
  /// rounded down to a power of two and capped at MAX_ANISOTROPY (the
  /// engines' clamp-not-error semantics: a level is a wish, the hardware
  /// quantizes it anyway). Below 1 or non-finite is the one error.
  pub fn parse(opts: &SamplerOptions<'_>) -> Result<Self, String> {
    let SamplerOptions { filter, wrap, mipmap, anisotropy, base_level } = *opts;
    let filter = parse_filter(filter)?.unwrap_or(SamplerFilter::Linear);
    let wrap = parse_wrap(wrap)?.unwrap_or(SamplerWrap::Clamp);
    let anisotropy = parse_anisotropy(anisotropy)?.unwrap_or(MIN_ANISOTROPY);
    let mipmap = mipmap.unwrap_or(false);
    let base_level = parse_base_level(base_level, mipmap)?.unwrap_or(0);
    Ok(SamplerState { filter, wrap, mipmap, anisotropy, base_level })
  }

  /// Parse the app-facing options against the texture's declared format.
  /// Same vocabulary as `parse`; a non-filterable format (the 32-bit
  /// floats, the integer format) flips the filter default to nearest and
  /// refuses linear, mipmaps and anisotropy outright (nearest/texelFetch
  /// is their portable contract - see `TextureFormat::filterable`).
  pub fn parse_for(format: TextureFormat, opts: &SamplerOptions<'_>) -> Result<Self, String> {
    let mut state = Self::parse(opts)?;
    if !format.filterable() && opts.filter.is_none() {
      state.filter = SamplerFilter::Nearest;
    }
    state.check_format(format)?;
    Ok(state)
  }

  /// This state with `update`'s given fields replaced (`set_texture_sampling`):
  /// the same vocabulary and quantization as `parse`, the same format rule
  /// as `parse_for`, the mip flag kept (it is storage).
  pub fn updated(self, format: TextureFormat, update: &SamplerUpdate<'_>) -> Result<Self, String> {
    let SamplerUpdate { filter, wrap, anisotropy, base_level } = *update;
    let state = SamplerState {
      filter: parse_filter(filter)?.unwrap_or(self.filter),
      wrap: parse_wrap(wrap)?.unwrap_or(self.wrap),
      mipmap: self.mipmap,
      anisotropy: parse_anisotropy(anisotropy)?.unwrap_or(self.anisotropy),
      base_level: parse_base_level(base_level, self.mipmap)?.unwrap_or(self.base_level),
    };
    state.check_format(format)?;
    Ok(state)
  }

  /// The base level must name a level the chain has: `levels` is the id's
  /// level count (`mip_levels` of its edge, 1 without a chain).
  pub fn check_chain(&self, levels: u32) -> Result<(), String> {
    if u32::from(self.base_level) >= levels {
      return Err(format!(
        "baseLevel {} is past the mip chain ({levels} level{}, 0..{})",
        self.base_level,
        if levels == 1 { "" } else { "s" },
        levels - 1
      ));
    }
    Ok(())
  }

  /// A non-filterable format (the 32-bit floats, the integer format)
  /// refuses linear, mipmaps and anisotropy outright: nearest/texelFetch is
  /// their portable contract (see `TextureFormat::filterable`).
  fn check_format(&self, format: TextureFormat) -> Result<(), String> {
    if format.filterable() {
      return Ok(());
    }
    if self.filter == SamplerFilter::Linear {
      return Err(format!(
        "{} textures sample nearest-only (no linear filtering for 32-bit float or integer texels in core GLES 3.0); drop filter: \"linear\"",
        format.name()
      ));
    }
    if self.mipmap {
      return Err(format!("{} textures cannot carry a mip chain; drop mipmap: true", format.name()));
    }
    if self.anisotropy > MIN_ANISOTROPY {
      return Err(format!("{} textures sample nearest-only; drop anisotropy", format.name()));
    }
    Ok(())
  }
}

/// "linear" | "nearest"; None stays None (the caller's default).
fn parse_filter(filter: Option<&str>) -> Result<Option<SamplerFilter>, String> {
  match filter {
    None => Ok(None),
    Some("linear") => Ok(Some(SamplerFilter::Linear)),
    Some("nearest") => Ok(Some(SamplerFilter::Nearest)),
    Some(other) => Err(format!("unknown filter '{other}' (expected \"linear\" or \"nearest\")")),
  }
}

/// "clamp" | "repeat"; None stays None.
fn parse_wrap(wrap: Option<&str>) -> Result<Option<SamplerWrap>, String> {
  match wrap {
    None => Ok(None),
    Some("clamp") => Ok(Some(SamplerWrap::Clamp)),
    Some("repeat") => Ok(Some(SamplerWrap::Repeat)),
    Some(other) => Err(format!("unknown wrap '{other}' (expected \"clamp\" or \"repeat\")")),
  }
}

/// A number >= 1, rounded down to a power of two and capped at
/// MAX_ANISOTROPY (the engines' clamp-not-error semantics: a level is a
/// wish, the hardware quantizes it anyway). Below 1 or non-finite is the
/// one error; None stays None.
fn parse_anisotropy(anisotropy: Option<f64>) -> Result<Option<u8>, String> {
  match anisotropy {
    None => Ok(None),
    Some(a) if a.is_finite() && a >= f64::from(MIN_ANISOTROPY) => {
      let level = a.min(f64::from(MAX_ANISOTROPY)) as u8;
      Ok(Some(1 << (u8::BITS - 1 - level.leading_zeros())))
    }
    Some(a) => Err(format!("anisotropy {a} must be a number >= 1 (1 = off, up to 16)")),
  }
}

/// A non-negative integer naming the first level sampled, which needs a
/// chain to pick from: a level above 0 without `mipmap` is the error (the
/// chain's length is checked where the id's size is known, `check_chain`).
fn parse_base_level(base_level: Option<f64>, mipmap: bool) -> Result<Option<u8>, String> {
  match base_level {
    None => Ok(None),
    Some(l) if l.is_finite() && l >= 0.0 && l.fract() == 0.0 && l <= f64::from(u8::MAX) => {
      if l > 0.0 && !mipmap {
        return Err(format!("baseLevel {l} needs a mip chain to pick a level from; set mipmap: true"));
      }
      Ok(Some(l as u8))
    }
    Some(l) => Err(format!("baseLevel {l} must be a non-negative integer (0 = the full-size level)")),
  }
}

/// The UI thread's view of a registered texture: the adopted renderer handle
/// (all a display list needs) plus dimensions for layout measure and update
/// validation, and the sampler state (the paint walk picks the display
/// sampling from its filter). The GL name behind it lives in the raster
/// thread's map.
pub struct TextureEntry {
  /// The renderer's handle on the GL name: what the `<texture>` display
  /// draw and the readback consume. None for a cube map (the renderer
  /// imports 2D names only, and a cube map is sampling-only anyway, see
  /// `TextureShape`) and for a 2D draw target of a format other than rgba8
  /// (half float or sRGB storage the renderer would display raw): both are
  /// sampler-only, and those two consumers reject them by this field.
  pub handle: Option<Texture>,
  pub width: u32,
  pub height: u32,
  pub sampler: SamplerState,
  /// Pixel format of the id (rgba8 unless created otherwise); sizes update
  /// and resize validation. Display of an r8 texture shows the red channel
  /// only (the renderer samples it as `(v, 0, 0, 1)` like any shader would).
  pub format: TextureFormat,
  /// The shape (a cube map's face edge is `width` == `height`),
  /// creation-time state like `format`.
  pub shape: TextureShape,
  /// The slices of a 3D texture or the layers of an array; 1 for every
  /// other shape.
  pub depth: u32,
}

impl TextureEntry {
  /// An ordinary 2D entry over its renderer handle.
  pub fn d2(handle: Texture, width: u32, height: u32, sampler: SamplerState, format: TextureFormat) -> Self {
    TextureEntry { handle: Some(handle), width, height, sampler, format, shape: TextureShape::D2, depth: 1 }
  }

  /// A 2D entry with no renderer handle: a draw target of a format other
  /// than rgba8, sampler-only.
  pub fn d2_sampler_only(width: u32, height: u32, sampler: SamplerState, format: TextureFormat) -> Self {
    TextureEntry { handle: None, width, height, sampler, format, shape: TextureShape::D2, depth: 1 }
  }

  /// A cube map entry: `size` x `size` faces, no renderer handle.
  pub fn cube(size: u32, sampler: SamplerState, format: TextureFormat) -> Self {
    TextureEntry { handle: None, width: size, height: size, sampler, format, shape: TextureShape::Cube, depth: 1 }
  }

  /// A volume entry (`TextureShape::D3` or `D2Array`): `depth` slices or
  /// layers of `width` x `height`, no renderer handle.
  pub fn volume(
    shape: TextureShape,
    width: u32,
    height: u32,
    depth: u32,
    sampler: SamplerState,
    format: TextureFormat,
  ) -> Self {
    debug_assert!(shape.is_volume(), "a volume entry takes a volume shape");
    TextureEntry { handle: None, width, height, sampler, format, shape, depth }
  }

  pub fn width(&self) -> u32 {
    self.width
  }
  pub fn height(&self) -> u32 {
    self.height
  }

  /// The byte length of one whole upload at the id's format: a frame of a
  /// 2D texture, the full volume of a 3D or array one (`depth` slices).
  pub fn byte_len(&self) -> usize {
    self.format.byte_len(self.width, self.height) * self.depth as usize
  }
  pub fn sampler(&self) -> SamplerState {
    self.sampler
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

  /// Give `id`'s entry a new renderer adoption, every other fact unchanged:
  /// the context-loss restore (okf/done/gpu-context-loss.md), where the
  /// raster thread rebuilt the id's storage on a fresh context and the
  /// handle the entry held belongs to the dead one. A fresh entry (the
  /// `Rc` holders of the old one read it per build), and a generation bump
  /// so a retained display list over the old handles is rebuilt. An id the
  /// registry no longer knows is skipped.
  pub fn replace_handle(&self, id: u64, handle: Texture) {
    let mut entries = self.entries.borrow_mut();
    let Some(old) = entries.get(&id) else { return };
    let entry = TextureEntry {
      handle: Some(handle),
      width: old.width,
      height: old.height,
      sampler: old.sampler,
      format: old.format,
      shape: old.shape,
      depth: old.depth,
    };
    entries.insert(id, Rc::new(entry));
    self.generation.set(self.generation.get().wrapping_add(1));
  }

  /// Give `id`'s entry a new declared sampling, every other fact unchanged
  /// (`set_texture_sampling`): a fresh entry like `replace_handle`, and
  /// the generation bump, since a retained display list drew the id with
  /// its old filter. An id the registry no longer knows is skipped.
  pub fn replace_sampler(&self, id: u64, sampler: SamplerState) {
    let mut entries = self.entries.borrow_mut();
    let Some(old) = entries.get(&id) else { return };
    let entry = TextureEntry {
      handle: old.handle.clone(),
      width: old.width,
      height: old.height,
      sampler,
      format: old.format,
      shape: old.shape,
      depth: old.depth,
    };
    entries.insert(id, Rc::new(entry));
    self.generation.set(self.generation.get().wrapping_add(1));
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
