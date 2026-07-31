//! Device limits: the hard per-driver ceilings (texture size, sampler units,
//! vertex attributes). Queried from GL once at raster-thread startup, served
//! to the UI thread over a blocking RPC and cached there, so every create and
//! bind can be checked at the call site with the limit named in the error -
//! instead of the raw GL failure ("framebuffer incomplete 0x8cd6", a silently
//! garbage draw) surfacing later on the raster thread.

use glow::HasContext;

/// The device ceilings alloy validates against. Plain Copy data, so one value
/// crosses the raster channel and lands in the UI-side cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuLimits {
  /// Largest width/height of any texture or render target, in pixels: the
  /// smaller of GL_MAX_TEXTURE_SIZE and GL_MAX_RENDERBUFFER_SIZE, folded into
  /// one number because any pipeline target may attach a depth renderbuffer
  /// and one app-facing ceiling is what a size check can name.
  pub max_texture_size: u32,
  /// Sampler inputs one pass may bind (GL_MAX_TEXTURE_IMAGE_UNITS): the
  /// fragment-stage unit count `run_pass` assigns by enumeration index.
  pub max_texture_units: u32,
  /// Vertex attributes one pipeline may declare (GL_MAX_VERTEX_ATTRIBS).
  pub max_vertex_attribs: u32,
}

impl GpuLimits {
  /// The GLES 3.0 guaranteed minimums: the fallback when the raster thread is
  /// gone (engine shutdown) and nothing can be queried, and the clamp floor
  /// for a driver reporting nonsense.
  pub const FLOOR: GpuLimits = GpuLimits { max_texture_size: 2048, max_texture_units: 16, max_vertex_attribs: 16 };

  /// Query the ceilings from the live context (raster thread, the one place
  /// GL exists). Values are clamped up to the ES 3.0 floors: a context this
  /// engine could create guarantees them, and a garbage glGet must not turn
  /// every create into an error.
  pub fn query(gl: &glow::Context) -> Self {
    let floor = GpuLimits::FLOOR;
    unsafe {
      let tex = gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE);
      let rb = gl.get_parameter_i32(glow::MAX_RENDERBUFFER_SIZE);
      let units = gl.get_parameter_i32(glow::MAX_TEXTURE_IMAGE_UNITS);
      let attribs = gl.get_parameter_i32(glow::MAX_VERTEX_ATTRIBS);
      GpuLimits {
        max_texture_size: tex.min(rb).max(floor.max_texture_size as i32) as u32,
        max_texture_units: units.max(floor.max_texture_units as i32) as u32,
        max_vertex_attribs: attribs.max(floor.max_vertex_attribs as i32) as u32,
      }
    }
  }

  /// Check a texture or target size against the device ceiling. Runs UI-side
  /// at the call-site boundary, like the validators in `vocab`.
  pub fn check_texture_size(&self, width: u32, height: u32) -> Result<(), String> {
    let max = self.max_texture_size;
    if width > max || height > max {
      return Err(format!("{width}x{height} exceeds this device's max texture size ({max})"));
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
