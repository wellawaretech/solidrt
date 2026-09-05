//! The GL half of the texture story: the process-lifetime sampler-object
//! cache and the GL texture objects behind uploaded ids. The vocabulary these
//! implement (SamplerState, TextureFormat) and the UI-side registry live in
//! `gpu::texture`; everything here takes the live context and runs on the
//! raster thread.

use glow::HasContext;
use impellers::ISize;
use std::num::NonZeroU32;

use crate::gpu::texture::{
  check_cube_faces, mip_size, SamplerFilter, SamplerState, SamplerWrap, TextureFormat, TextureShape,
  ANISOTROPY_LEVELS, CUBE_FACES, MIN_ANISOTROPY,
};

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
  /// The one comparison sampler (see `compare()`), outside the indexed
  /// state combinations: comparison is picked by the program's declared
  /// sampler type, never by SamplerState.
  compare: glow::Sampler,
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
    // The comparison sampler: LINEAR so the hardware compares the four
    // neighbours and bilinearly weights the RESULTS (2x2 PCF - the step a
    // shader-side loop cannot take, which blends depth values), LEQUAL to
    // match the `ref <= depth` convention every shadow lookup here uses.
    let compare = unsafe {
      let sampler = gl.create_sampler().expect("glGenSamplers failed");
      gl.sampler_parameter_i32(sampler, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
      gl.sampler_parameter_i32(sampler, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
      gl.sampler_parameter_i32(sampler, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
      gl.sampler_parameter_i32(sampler, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
      gl.sampler_parameter_i32(sampler, glow::TEXTURE_COMPARE_MODE, glow::COMPARE_REF_TO_TEXTURE as i32);
      gl.sampler_parameter_i32(sampler, glow::TEXTURE_COMPARE_FUNC, glow::LEQUAL as i32);
      sampler
    };
    SamplerCache { samplers: samplers.map(|s| s.expect("all sampler states populated")), compare }
  }

  pub fn get(&self, state: SamplerState) -> glow::Sampler {
    self.samplers[Self::index(state)]
  }

  /// The sampler for a `sampler2DShadow` binding: `texture(map, vec3(uv,
  /// ref))` returns the LEQUAL compare of `ref` against the depth texture,
  /// LINEAR-weighted over the 2x2 footprint. Only meaningful on a
  /// depth-format texture (the resolver enforces that).
  pub fn compare(&self) -> glow::Sampler {
    self.compare
  }

  pub(crate) fn index(state: SamplerState) -> usize {
    let base = (state.filter as usize) * 4 + (state.wrap as usize) * 2 + (state.mipmap as usize);
    base * ANISOTROPY_LEVELS + SamplerState::anisotropy_slot(state.anisotropy)
  }
}

/// A GL texture that is adopted into Impeller right after creation. Impeller
/// takes ownership of the GL name and deletes it when its Texture drops, so
/// GpuTexture deliberately does NOT delete the name (no Drop impl) - doing so
/// would double-free the name and corrupt whatever live texture reuses it.
/// The one exception is a cube map (`shape == Cube`): Impeller adopts 2D
/// names only, so a cube name stays ours and the raster thread deletes it
/// on destroy. Raster-thread-only: creation and uploads are GL work.
pub struct GpuTexture {
  pub gl_texture: glow::Texture,
  pub width: u32,
  pub height: u32,
  /// 2D or cube map; picks the bind target of every use of the name.
  pub shape: TextureShape,
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

/// The GL storage triple (internal format, pixel layout, component type) of
/// a format, for the allocating `glTexImage2D` calls.
fn gl_storage(format: TextureFormat) -> (u32, u32, u32) {
  match format {
    TextureFormat::Rgba8 => (glow::RGBA8, glow::RGBA, glow::UNSIGNED_BYTE),
    TextureFormat::R8 => (glow::R8, glow::RED, glow::UNSIGNED_BYTE),
    TextureFormat::Rg8 => (glow::RG8, glow::RG, glow::UNSIGNED_BYTE),
    TextureFormat::Depth24 => (glow::DEPTH_COMPONENT24, glow::DEPTH_COMPONENT, glow::UNSIGNED_INT),
    TextureFormat::R32f => (glow::R32F, glow::RED, glow::FLOAT),
    TextureFormat::Rgba32f => (glow::RGBA32F, glow::RGBA, glow::FLOAT),
    TextureFormat::Rgba16f => (glow::RGBA16F, glow::RGBA, glow::HALF_FLOAT),
    TextureFormat::Rgba8Srgb => (glow::SRGB8_ALPHA8, glow::RGBA, glow::UNSIGNED_BYTE),
  }
}

/// The completeness-fallback filter of a texture object (see `new`): depth
/// and 32-bit float textures are only complete at NEAREST.
fn fallback_filter(format: TextureFormat) -> u32 {
  if !format.filterable() {
    glow::NEAREST
  } else {
    glow::LINEAR
  }
}

impl GpuTexture {
  pub fn new(gl: &glow::Context, size: ISize, sampler: SamplerState, format: TextureFormat) -> Self {
    let (width, height) = (size.width as u32, size.height as u32);
    let (internal, layout, ty) = gl_storage(format);
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
      // rewrites them on every draw of the texture). Depth and 32-bit float
      // textures are only complete at NEAREST (float linear needs an
      // extension).
      let filter = fallback_filter(format);
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter as i32);
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter as i32);
      gl.bind_texture(glow::TEXTURE_2D, NonZeroU32::new(prev as u32).map(glow::NativeTexture));
      GpuTexture { gl_texture, width, height, shape: TextureShape::D2, sampler, format, label: None }
    }
  }

  /// A cube map from six `size` x `size` faces in GL order (+X, -X, +Y, -Y,
  /// +Z, -Z), each `format.byte_len(size, size)` bytes, or from an explicit
  /// mip chain (the full chain level-major; see `check_cube_faces` - checked
  /// UI-side, backstopped here), allocated and uploaded in one go - a cube
  /// map is create-once. The mip chain, when the sampling declares one, is
  /// generated from the six faces here, or uploaded level by level when
  /// given explicitly (no generation, so no color-renderable format
  /// needed). Wrap modes do not apply: GLES 3.0 filters across cube faces
  /// seamlessly. Restores the cube map binding it touches.
  pub fn new_cube(
    gl: &glow::Context,
    size: u32,
    faces: &[Vec<u8>],
    sampler: SamplerState,
    format: TextureFormat,
  ) -> Result<Self, String> {
    let levels = check_cube_faces(size, faces, format)?;
    let (internal, layout, ty) = gl_storage(format);
    let (_, _, alignment) = upload_layout(format).ok_or_else(|| format!("{} is not an upload format", format.name()))?;
    unsafe {
      let prev = gl.get_parameter_i32(glow::TEXTURE_BINDING_CUBE_MAP);
      let gl_texture = gl.create_texture().map_err(|e| format!("glGenTextures failed: {e}"))?;
      gl.bind_texture(glow::TEXTURE_CUBE_MAP, Some(gl_texture));
      gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, alignment);
      for (i, face) in faces.iter().enumerate() {
        let level = (i / CUBE_FACES) as u32;
        let edge = mip_size(size, level) as i32;
        gl.tex_image_2d(
          glow::TEXTURE_CUBE_MAP_POSITIVE_X + (i % CUBE_FACES) as u32,
          level as i32,
          internal as i32,
          edge,
          edge,
          0,
          layout,
          ty,
          glow::PixelUnpackData::Slice(Some(face)),
        );
      }
      if alignment != 4 {
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
      }
      let filter = fallback_filter(format);
      gl.tex_parameter_i32(glow::TEXTURE_CUBE_MAP, glow::TEXTURE_MIN_FILTER, filter as i32);
      gl.tex_parameter_i32(glow::TEXTURE_CUBE_MAP, glow::TEXTURE_MAG_FILTER, filter as i32);
      if sampler.mipmap && levels == 1 {
        gl.generate_mipmap(glow::TEXTURE_CUBE_MAP);
      }
      gl.bind_texture(glow::TEXTURE_CUBE_MAP, NonZeroU32::new(prev as u32).map(glow::NativeTexture));
      Ok(GpuTexture { gl_texture, width: size, height: size, shape: TextureShape::Cube, sampler, format, label: None })
    }
  }

  pub fn upload(&self, gl: &glow::Context, data: &[u8], size: ISize) {
    if self.shape == TextureShape::Cube {
      // Gated UI-side (a cube map is create-once); backstop.
      log::warn!("[alloy] upload into a cube map ignored: cube maps are create-once");
      return;
    }
    let (width, height) = (size.width as i32, size.height as i32);
    // RGBA8 rows are width*4, always a multiple of 4, so the default unpack
    // alignment holds. R8 rows are width*1 and must unpack at alignment 1 or
    // any width not divisible by 4 reads rows off by their padding - the
    // whole reason the format exists is to avoid that per-frame repacking.
    // RG8 rows are width*2; alignment 1 is correct for every width. Float
    // rows are multiples of 4 bytes at any width, so the default holds.
    let Some((gl_format, ty, alignment)) = upload_layout(self.format) else {
      // Gated UI-side (a depth id is not an upload texture); backstop.
      log::warn!("[alloy] upload into a depth texture ignored: depth is render-written");
      return;
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
        ty,
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

/// The unpack layout (pixel format, component type, row alignment) of an
/// upload format; None for the render-written depth format.
fn upload_layout(format: TextureFormat) -> Option<(u32, u32, i32)> {
  match format {
    TextureFormat::Rgba8 => Some((glow::RGBA, glow::UNSIGNED_BYTE, 4)),
    TextureFormat::R8 => Some((glow::RED, glow::UNSIGNED_BYTE, 1)),
    TextureFormat::Rg8 => Some((glow::RG, glow::UNSIGNED_BYTE, 1)),
    TextureFormat::R32f => Some((glow::RED, glow::FLOAT, 4)),
    TextureFormat::Rgba32f => Some((glow::RGBA, glow::FLOAT, 4)),
    TextureFormat::Rgba16f => Some((glow::RGBA, glow::HALF_FLOAT, 4)),
    TextureFormat::Rgba8Srgb => Some((glow::RGBA, glow::UNSIGNED_BYTE, 4)),
    TextureFormat::Depth24 => None,
  }
}
