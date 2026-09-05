//! Device limits: the hard per-driver ceilings (texture size, sampler units,
//! vertex attributes). Queried from GL once at raster-thread startup (see
//! `gl::query_limits`), served to the UI thread over a blocking RPC and
//! cached there, so every create and bind can be checked at the call site
//! with the limit named in the error - instead of the raw GL failure
//! ("framebuffer incomplete 0x8cd6", a silently garbage draw) surfacing
//! later on the raster thread.

use super::texture::TextureFormat;

/// The device ceilings alloy validates against. Plain Copy data, so one value
/// crosses the raster channel and lands in the UI-side cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuLimits {
  /// Largest width/height of any texture or render target, in pixels: the
  /// smaller of GL_MAX_TEXTURE_SIZE and GL_MAX_RENDERBUFFER_SIZE, folded into
  /// one number because any pipeline target may attach a depth renderbuffer
  /// and one app-facing ceiling is what a size check can name.
  pub max_texture_size: u32,
  /// Largest face edge of a cube map (GL_MAX_CUBE_MAP_TEXTURE_SIZE): its
  /// own ceiling in GL, often but not always the 2D one.
  pub max_cube_map_size: u32,
  /// Sampler inputs one pass may bind (GL_MAX_TEXTURE_IMAGE_UNITS): the
  /// fragment-stage unit count `run_pass` assigns by enumeration index.
  pub max_texture_units: u32,
  /// Vertex attributes one pipeline may declare (GL_MAX_VERTEX_ATTRIBS).
  pub max_vertex_attribs: u32,
  /// vec4 uniform slots a vertex stage may declare
  /// (GL_MAX_VERTEX_UNIFORM_VECTORS): a mat4 costs 4, a mat4[N] array 4N.
  /// What sizes a bone palette - a `uniform mat4 uBones[J]` under dynamic
  /// indexing keeps all J elements active, so the declaration itself must
  /// fit this budget or the program fails to link.
  pub max_vertex_uniform_vectors: u32,
  /// Highest anisotropic filtering level the device samples at
  /// (GL_MAX_TEXTURE_MAX_ANISOTROPY_EXT), 1 when
  /// `GL_EXT_texture_filter_anisotropic` is absent. A report, not a
  /// validation ceiling: a requested level above it is clamped silently at
  /// sampler creation, the way every engine treats the level.
  pub max_anisotropy: u32,
  /// Whether half float is color-renderable here
  /// (GL_EXT_color_buffer_half_float or GL_EXT_color_buffer_float): what
  /// glGenerateMipmap requires of a format, so it gates `mipmap: true` on
  /// an rgba16f texture. An extension at every GLES level, present on
  /// practically every device; where it is absent an HDR cube map samples
  /// its base level (or, later, ships its levels explicitly).
  pub half_float_renderable: bool,
}

impl GpuLimits {
  /// The GLES 3.0 guaranteed minimums: the fallback when the raster thread is
  /// gone (engine shutdown) and nothing can be queried, and the clamp floor
  /// for a driver reporting nonsense.
  pub const FLOOR: GpuLimits = GpuLimits {
    max_texture_size: 2048,
    max_cube_map_size: 2048,
    max_texture_units: 16,
    max_vertex_attribs: 16,
    max_anisotropy: 1,
    max_vertex_uniform_vectors: 256,
    half_float_renderable: false,
  };

  /// Check a texture or target size against the device ceiling (and against
  /// zero: a 0-sized attachment only surfaces later as an opaque framebuffer
  /// completeness failure). Runs UI-side at the call-site boundary, like the
  /// validators in `vocab`.
  pub fn check_texture_size(&self, width: u32, height: u32) -> Result<(), String> {
    if width == 0 || height == 0 {
      return Err(format!("{width}x{height}: width and height must be at least 1"));
    }
    let max = self.max_texture_size;
    if width > max || height > max {
      return Err(format!("{width}x{height} exceeds this device's max texture size ({max})"));
    }
    Ok(())
  }

  /// Check a texture's mip request against its format: the chain comes from
  /// glGenerateMipmap, which needs a color-renderable format, and half float
  /// is renderable only through an extension.
  pub fn check_mipmap(&self, format: TextureFormat, mipmap: bool) -> Result<(), String> {
    if mipmap && format == TextureFormat::Rgba16f && !self.half_float_renderable {
      return Err(
        "rgba16f cannot carry a generated mip chain on this device (half float is not color-renderable: no EXT_color_buffer_half_float); drop mipmap: true"
          .to_string(),
      );
    }
    Ok(())
  }

  /// Check a draw target's color format: the two 8-bit formats are
  /// color-renderable in core GLES 3.0, half float only through an
  /// extension (the HDR probe and bake format), and the 32-bit float and
  /// single-channel formats are upload-and-sample only here.
  pub fn check_render_format(&self, format: TextureFormat) -> Result<(), String> {
    match format {
      TextureFormat::Rgba8 | TextureFormat::Rgba8Srgb => Ok(()),
      TextureFormat::Rgba16f if self.half_float_renderable => Ok(()),
      TextureFormat::Rgba16f => Err(
        "rgba16f is not renderable on this device (half float is not color-renderable: no EXT_color_buffer_half_float); check limits.halfFloatRenderable and fall back to rgba8"
          .to_string(),
      ),
      other => Err(format!("draw target format must be rgba8, rgba8-srgb or rgba16f, got {}", other.name())),
    }
  }

  /// Check a cube map's face edge against the device ceiling.
  pub fn check_cube_map_size(&self, size: u32) -> Result<(), String> {
    let max = self.max_cube_map_size;
    if size > max {
      return Err(format!("face size {size} exceeds this device's max cube map size ({max})"));
    }
    Ok(())
  }

  /// Check a pass's sampler-input count against the device's texture units.
  pub fn check_texture_units(&self, count: usize) -> Result<(), String> {
    let max = self.max_texture_units;
    if count > max as usize {
      return Err(format!("{count} sampler inputs exceed this device's texture unit limit ({max} per pass)"));
    }
    Ok(())
  }

  /// Check a pipeline's declared attribute count against the device limit.
  pub fn check_vertex_attribs(&self, count: usize) -> Result<(), String> {
    let max = self.max_vertex_attribs;
    if count > max as usize {
      return Err(format!("{count} vertex attributes exceed this device's limit ({max})"));
    }
    Ok(())
  }
}
