//! Engine-free compressed-texture codec: KTX2 files holding Basis Universal
//! texel data (ETC1S or UASTC), the one device-independent form an app
//! ships, and the block formats a GPU samples. `transcode` turns a KTX2 into
//! the format a device takes (ETC2 everywhere, BC7 on desktops) at load;
//! `encode` makes the KTX2 from RGBA8 pixels at bake time. Pure CPU, no GPU
//! or scripting types; `flux:image` is the marshalling layer over this.
//!
//! The codec is Binomial's Basis Universal, vendored and reached through its
//! C API (see `ffi`). Its output is the same bytes wherever it runs, which
//! is what lets one baked file serve every platform.
//!
//! Alpha: `encode` takes the pixels as the GPU will sample them, which in
//! this runtime means premultiplied (the pixel contract) - blocks cannot be
//! premultiplied after the fact, and mips only filter correctly on
//! premultiplied color. `transcode` hands a file's texels through as they
//! are, so a file from elsewhere that stores straight alpha samples as
//! straight alpha.

mod ffi;

use std::sync::Once;

/// The first twelve bytes of every KTX2 file.
const KTX2_MAGIC: [u8; 12] = [0xAB, b'K', b'T', b'X', b' ', b'2', b'0', 0xBB, 0x0D, 0x0A, 0x1A, 0x0A];

/// The block geometry of both compressed targets: a 4x4 texel block in 16
/// bytes (ETC2/EAC RGBA8 and BC7 alike).
const BLOCK_EDGE: u32 = 4;
const BLOCK_BYTES: usize = 16;
/// Bytes per texel of the uncompressed target.
const RGBA8_BYTES: usize = 4;

/// Whether `bytes` starts as a KTX2 file: the sniff a loader uses to tell a
/// compressed image from a PNG or JPEG.
pub fn is_ktx2(bytes: &[u8]) -> bool {
  bytes.starts_with(&KTX2_MAGIC)
}

/// What a KTX2 is transcoded to. The two block formats are 1 byte per texel
/// and decoded by the sampler; `Rgba8` is the uncompressed form, for
/// inspection and comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
  Etc2Rgba8,
  Bc7Rgba8,
  Rgba8,
}

impl Target {
  /// Parse the app-facing name (the texture format vocabulary, without the
  /// sRGB qualifier: that comes from the file).
  pub fn parse(name: &str) -> Result<Self, String> {
    match name {
      "etc2-rgba8" => Ok(Target::Etc2Rgba8),
      "bc7-rgba8" => Ok(Target::Bc7Rgba8),
      "rgba8" => Ok(Target::Rgba8),
      other => Err(format!("unknown target '{other}' (expected \"etc2-rgba8\", \"bc7-rgba8\" or \"rgba8\")")),
    }
  }

  /// The texture format name of a transcode to this target: the target,
  /// with the sRGB qualifier when the file's texels are sRGB-encoded.
  pub fn format_name(self, srgb: bool) -> &'static str {
    match (self, srgb) {
      (Target::Etc2Rgba8, false) => "etc2-rgba8",
      (Target::Etc2Rgba8, true) => "etc2-rgba8-srgb",
      (Target::Bc7Rgba8, false) => "bc7-rgba8",
      (Target::Bc7Rgba8, true) => "bc7-rgba8-srgb",
      (Target::Rgba8, false) => "rgba8",
      (Target::Rgba8, true) => "rgba8-srgb",
    }
  }

  fn transcoder_format(self) -> u32 {
    match self {
      Target::Etc2Rgba8 => ffi::TF_ETC2_RGBA,
      Target::Bc7Rgba8 => ffi::TF_BC7_RGBA,
      Target::Rgba8 => ffi::TF_RGBA32,
    }
  }

  /// Bytes of one `width` x `height` level, and the unit count the
  /// transcoder wants for it (blocks for a block format, pixels for rgba8).
  fn level_size(self, width: u32, height: u32) -> (usize, u32) {
    match self {
      Target::Rgba8 => {
        let pixels = width * height;
        (pixels as usize * RGBA8_BYTES, pixels)
      }
      Target::Etc2Rgba8 | Target::Bc7Rgba8 => {
        let blocks = width.div_ceil(BLOCK_EDGE) * height.div_ceil(BLOCK_EDGE);
        (blocks as usize * BLOCK_BYTES, blocks)
      }
    }
  }
}

/// A transcoded texture: every level of the file, level-major (level 0
/// first) in one buffer, which is the payload a texture create takes.
#[derive(Debug)]
pub struct Transcoded {
  pub data: Vec<u8>,
  pub width: u32,
  pub height: u32,
  /// Levels in `data`: 1, or the full chain down to 1x1.
  pub levels: u32,
  /// The file's texels are sRGB-encoded.
  pub srgb: bool,
  pub target: Target,
}

impl Transcoded {
  /// The texture format name to create this with.
  pub fn format_name(&self) -> &'static str {
    self.target.format_name(self.srgb)
  }
}

/// The record a host hands to its texture create: `data` (the payload),
/// `width`, `height`, `format` (the texture format name, sRGB qualifier
/// included) and `mipmap` (the payload holds the full chain).
impl From<Transcoded> for crate::value::Value {
  fn from(t: Transcoded) -> Self {
    let format = t.format_name();
    crate::value::Value::Map(vec![
      ("data".to_string(), crate::value::Value::bytes(t.data)),
      ("width".to_string(), crate::value::Value::Int(t.width as i64)),
      ("height".to_string(), crate::value::Value::Int(t.height as i64)),
      ("format".to_string(), crate::value::Value::String(format.to_string())),
      ("mipmap".to_string(), crate::value::Value::Bool(t.levels > 1)),
    ])
  }
}

/// The level count of a full mip chain of a `width` x `height` level 0.
fn full_chain(width: u32, height: u32) -> u32 {
  u32::BITS - width.max(height).max(1).leading_zeros()
}

/// The edge of level `level` of a chain from `size` (GL's rule).
fn level_edge(size: u32, level: u32) -> u32 {
  (size >> level).max(1)
}

/// An open KTX2 in the transcoder, closed on drop.
struct Ktx2(u64);

impl Drop for Ktx2 {
  fn drop(&mut self) {
    // SAFETY: the handle came from bt_ktx2_open and is closed exactly once.
    unsafe { ffi::bt_ktx2_close(self.0) };
  }
}

fn init_transcoder() {
  static INIT: Once = Once::new();
  // SAFETY: builds the transcoder's global tables; Once makes it single.
  INIT.call_once(|| unsafe { ffi::bt_init() });
}

/// Transcode a KTX2 file (Basis Universal ETC1S or UASTC LDR, a plain 2D
/// texture with one level or its full mip chain) to `target`. Errs on
/// anything else: not a KTX2, another codec, a cube map, an array, a video,
/// a partial chain.
pub fn transcode(bytes: &[u8], target: Target) -> Result<Transcoded, String> {
  if !is_ktx2(bytes) {
    return Err("not a KTX2 file (the file does not start with the KTX2 identifier)".to_string());
  }
  let len = u32::try_from(bytes.len()).map_err(|_| "KTX2 file is larger than 4 GiB".to_string())?;
  init_transcoder();
  // SAFETY: `bytes` outlives `file` (the transcoder keeps a pointer into it
  // until close, and `file` drops before this function returns); every
  // call below passes the live handle; each output slice is sized to what
  // the transcoder is told it may write.
  unsafe {
    let handle = ffi::bt_ktx2_open(bytes.as_ptr() as u64, len);
    if handle == 0 {
      return Err("the KTX2 file does not parse (truncated or corrupt)".to_string());
    }
    let file = Ktx2(handle);
    if ffi::bt_ktx2_is_etc1s(file.0) == 0 && ffi::bt_ktx2_is_uastc_ldr_4x4(file.0) == 0 {
      return Err("the KTX2 file's codec is not supported (Basis Universal ETC1S and UASTC LDR 4x4 are)".to_string());
    }
    if ffi::bt_ktx2_get_faces(file.0) != 1 {
      return Err("the KTX2 file is a cube map; only 2D textures transcode".to_string());
    }
    if ffi::bt_ktx2_get_layers(file.0) != 0 || ffi::bt_ktx2_is_video(file.0) != 0 {
      return Err("the KTX2 file is a texture array or video; only 2D textures transcode".to_string());
    }
    let width = ffi::bt_ktx2_get_width(file.0);
    let height = ffi::bt_ktx2_get_height(file.0);
    let levels = ffi::bt_ktx2_get_levels(file.0);
    if width == 0 || height == 0 || levels == 0 {
      return Err("the KTX2 file declares an empty texture".to_string());
    }
    let chain = full_chain(width, height);
    if levels != 1 && levels != chain {
      return Err(format!(
        "the KTX2 file carries {levels} mip levels; a {width}x{height} texture takes 1 or its full chain of {chain}"
      ));
    }
    let srgb = ffi::bt_ktx2_is_srgb(file.0) != 0;
    if ffi::bt_ktx2_start_transcoding(file.0) == 0 {
      return Err("the KTX2 file's codebooks do not decode (corrupt file)".to_string());
    }
    let total: usize = (0..levels).map(|l| target.level_size(level_edge(width, l), level_edge(height, l)).0).sum();
    let mut data = vec![0u8; total];
    let mut offset = 0;
    for level in 0..levels {
      let (w, h) = (level_edge(width, level), level_edge(height, level));
      let (file_w, file_h) = (
        ffi::bt_ktx2_get_level_orig_width(file.0, level, 0, 0),
        ffi::bt_ktx2_get_level_orig_height(file.0, level, 0, 0),
      );
      if (file_w, file_h) != (w, h) {
        return Err(format!("the KTX2 file's level {level} is {file_w}x{file_h}, expected {w}x{h}"));
      }
      let (len, units) = target.level_size(w, h);
      let out = &mut data[offset..offset + len];
      let ok = ffi::bt_ktx2_transcode_image_level(
        file.0,
        level,
        0,
        0,
        out.as_mut_ptr() as u64,
        units,
        target.transcoder_format(),
        0,
        0,
        0,
        -1,
        -1,
        0,
      );
      if ok == 0 {
        return Err(format!("level {level} does not transcode to {}", target.format_name(srgb)));
      }
      offset += len;
    }
    Ok(Transcoded { data, width, height, levels, srgb, target })
  }
}

/// The Basis Universal codec a KTX2 is encoded with. Etc1s is the small
/// one (lossy at JPEG-class quality, around 1 bit per texel): color maps.
/// Uastc is the high quality one (8 bits per texel before the file's zstd
/// compression): normal and data maps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
  Etc1s,
  Uastc,
}

impl Codec {
  pub fn parse(name: &str) -> Result<Self, String> {
    match name {
      "etc1s" => Ok(Codec::Etc1s),
      "uastc" => Ok(Codec::Uastc),
      other => Err(format!("unknown codec '{other}' (expected \"etc1s\" or \"uastc\")")),
    }
  }
}

/// How the generated mip levels filter across the texture's edges: what the
/// texture will be sampled with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MipWrap {
  #[default]
  Clamp,
  Repeat,
}

impl MipWrap {
  pub fn parse(name: &str) -> Result<Self, String> {
    match name {
      "clamp" => Ok(MipWrap::Clamp),
      "repeat" => Ok(MipWrap::Repeat),
      other => Err(format!("unknown wrap '{other}' (expected \"clamp\" or \"repeat\")")),
    }
  }
}

/// The encoder's default quality on the 0..1 scale: the middle, where the
/// codecs trade size and fidelity evenly.
pub const DEFAULT_QUALITY: f32 = 0.5;

#[derive(Clone, Copy, Debug)]
pub struct EncodeOptions {
  pub codec: Codec,
  /// The pixels are sRGB-encoded color (a base color or emissive map): the
  /// file is flagged so, the error metric is perceptual and the mips
  /// filter in linear light.
  pub srgb: bool,
  /// Generate and store the full mip chain.
  pub mipmap: bool,
  pub wrap: MipWrap,
  /// 0..1, mapped onto the codec's own quality scale. For Uastc, 1 is the
  /// encoder's best blocks; below it upstream turns rate-distortion
  /// optimization on, stronger the lower the quality, which costs block
  /// error and makes the file compress.
  pub quality: f32,
}

/// The encoder's parameter block, deleted on drop.
struct CompParams(u64);

impl Drop for CompParams {
  fn drop(&mut self) {
    // SAFETY: the handle came from bu_new_comp_params, deleted exactly once.
    unsafe { ffi::bu_delete_comp_params(self.0) };
  }
}

fn init_encoder() {
  static INIT: Once = Once::new();
  // SAFETY: builds the encoder's global tables; Once makes it single.
  INIT.call_once(|| unsafe { ffi::bu_init() });
}

/// Encode tightly-packed RGBA8 pixels (premultiplied, see the module docs)
/// into a KTX2 file. Runs the encoder's own thread pool over the machine's
/// cores and takes seconds for a large image: bake-time work.
pub fn encode(pixels: &[u8], width: u32, height: u32, options: &EncodeOptions) -> Result<Vec<u8>, String> {
  if width == 0 || height == 0 {
    return Err(format!("{width}x{height}: width and height must be at least 1"));
  }
  let expected = width as usize * height as usize * RGBA8_BYTES;
  if pixels.len() != expected {
    return Err(format!("expected {expected} bytes ({width}x{height} rgba8), got {}", pixels.len()));
  }
  if !(0.0..=1.0).contains(&options.quality) {
    return Err(format!("quality {} out of range 0..1", options.quality));
  }
  let pitch = u32::try_from(width as usize * RGBA8_BYTES).map_err(|_| "image is too wide".to_string())?;
  let span = (ffi::QUALITY_MAX - ffi::QUALITY_MIN) as f32;
  let quality = ffi::QUALITY_MIN + (options.quality * span).round() as std::ffi::c_int;
  let format = match options.codec {
    Codec::Etc1s => ffi::BTF_ETC1S,
    Codec::Uastc => ffi::BTF_UASTC_LDR_4X4,
  };
  let mut flags = ffi::COMP_FLAGS_KTX2_OUTPUT | ffi::COMP_FLAGS_THREADED;
  if options.srgb {
    flags |= ffi::COMP_FLAGS_SRGB;
  }
  // UASTC levels are zstd-compressed in the file: lossless, so there is no
  // option for it. ETC1S has its own entropy coding and takes none.
  if options.codec == Codec::Uastc {
    flags |= ffi::COMP_FLAGS_KTX2_UASTC_ZSTD;
  }
  if options.mipmap {
    flags |= match options.wrap {
      MipWrap::Clamp => ffi::COMP_FLAGS_GEN_MIPS_CLAMP,
      MipWrap::Repeat => ffi::COMP_FLAGS_GEN_MIPS_WRAP,
    };
  }
  init_encoder();
  // SAFETY: `pixels` holds `height` rows of `pitch` bytes (checked above)
  // and is copied by set_image before the call returns; the output pointer
  // is read for the size the encoder reports, while `params` is alive.
  unsafe {
    let params = CompParams(ffi::bu_new_comp_params());
    if params.0 == 0 {
      return Err("the encoder could not allocate its parameters".to_string());
    }
    if ffi::bu_comp_params_set_image_rgba32(params.0, 0, pixels.as_ptr() as u64, width, height, pitch) == 0 {
      return Err("the encoder refused the image".to_string());
    }
    if ffi::bu_compress_texture(params.0, format, quality, ffi::EFFORT_DEFAULT, flags, ffi::LOW_LEVEL_QUALITY_UNUSED) == 0 {
      return Err(format!("the encoder failed on the {width}x{height} image"));
    }
    let size = ffi::bu_comp_params_get_comp_data_size(params.0) as usize;
    let data = ffi::bu_comp_params_get_comp_data_ofs(params.0) as *const u8;
    if size == 0 || data.is_null() {
      return Err("the encoder produced no output".to_string());
    }
    Ok(std::slice::from_raw_parts(data, size).to_vec())
  }
}
