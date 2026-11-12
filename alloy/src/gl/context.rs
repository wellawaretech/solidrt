//! Context bootstrap on the raster thread: the interactive GlBinding over
//! ore's GL surface (ore configures GL and creates the context on the main
//! thread), and loading the glow context and the renderer.

use crate::backend::GlBinding;
use crate::gpu::GpuLimits;
use crate::renderer::prelude::*;
use crate::renderer::Renderer;
use glow::HasContext;

/// The interactive binding: ore's GL surface (see GlBinding).
pub(crate) struct WindowBinding(ore::gl_surface::GlSurface);

impl WindowBinding {
  pub(crate) fn new(surface: ore::gl_surface::GlSurface) -> Self {
    WindowBinding(surface)
  }
}

impl GlBinding for WindowBinding {
  /// Re-executes the bind even when the pair looks current already (see
  /// ore::gl_surface::GlSurface::make_current).
  fn bind(&self) -> bool {
    self.0.make_current()
  }

  /// Waits the macOS display link's tick first (inside the surface's swap).
  fn swap(&self) -> bool {
    self.0.swap()
  }

  fn set_swap_interval(&self) -> bool {
    self.0.set_swap_interval()
  }

  fn release(&self) -> bool {
    self.0.release()
  }

  fn proc_address(&self, name: &std::ffi::CStr) -> *const std::ffi::c_void {
    self.0.proc_address(name)
  }

  fn error(&self) -> String {
    self.0.error()
  }

  fn device_probe(&self) -> ore::gl_surface::DeviceProbe {
    self.0.device_probe()
  }
}

/// Load GLES bindings through the binding's proc-address. Must be called on
/// the raster thread with the GL context current; the returned context drives
/// all of alloy's GL work (texture allocation/upload, offscreen FBOs,
/// readback) and shares the underlying GL objects with the renderer, which
/// draws on the same context.
pub(crate) fn create_gl_context(binding: &dyn GlBinding) -> glow::Context {
  unsafe {
    glow::Context::from_loader_function(|name| {
      let cname = std::ffi::CString::new(name).expect("GL proc name contains null byte");
      binding.proc_address(&cname)
    })
  }
}

/// Create the renderer over the context current on the raster thread; it
/// lives as long as the context (see raster/recover.rs for the order on a
/// context loss).
pub(crate) fn create_renderer(binding: &dyn GlBinding) -> Renderer {
  unsafe {
    Renderer::new(&mut |name| {
      let cname = std::ffi::CString::new(name).expect("GL proc name contains null byte");
      binding.proc_address(&cname) as *mut _
    })
  }
  .expect("Failed to create the renderer")
}

/// Query the device ceilings from the live context (raster thread, the one
/// place GL exists). Values are clamped up to the ES 3.0 floors: a context
/// this engine could create guarantees them, and a garbage glGet must not
/// turn every create into an error.
pub(crate) fn query_limits(gl: &glow::Context) -> GpuLimits {
  let floor = GpuLimits::FLOOR;
  unsafe {
    let tex = gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE);
    let cube = gl.get_parameter_i32(glow::MAX_CUBE_MAP_TEXTURE_SIZE);
    let tex_3d = gl.get_parameter_i32(glow::MAX_3D_TEXTURE_SIZE);
    let layers = gl.get_parameter_i32(glow::MAX_ARRAY_TEXTURE_LAYERS);
    let rb = gl.get_parameter_i32(glow::MAX_RENDERBUFFER_SIZE);
    let units = gl.get_parameter_i32(glow::MAX_TEXTURE_IMAGE_UNITS);
    let attribs = gl.get_parameter_i32(glow::MAX_VERTEX_ATTRIBS);
    let vertex_vectors = gl.get_parameter_i32(glow::MAX_VERTEX_UNIFORM_VECTORS);
    // An extension, never core at any GL level, but present on practically
    // every ES 3.0 device and on ANGLE over D3D11/Metal; absence is a fact
    // to report, not an error. A desktop core profile may list only the
    // GL 4.6 spelling (ARB), same enums. The parameter is a float in the
    // spec.
    let ext = gl.supported_extensions();
    let anisotropy =
      if ext.contains("GL_EXT_texture_filter_anisotropic") || ext.contains("GL_ARB_texture_filter_anisotropic") {
        gl.get_parameter_f32(glow::MAX_TEXTURE_MAX_ANISOTROPY_EXT) as i32
      } else {
        1
      };
    // Half-float render targets are likewise an extension at every GLES
    // level (the float one implies the half one); a fact for the mip gate
    // on rgba16f, never an error.
    let half_float_renderable =
      ext.contains("GL_EXT_color_buffer_half_float") || ext.contains("GL_EXT_color_buffer_float");
    // BC7 uploads: the desktop block format, an extension at every GLES
    // level (ANGLE over D3D11 and Metal, Mesa) and absent on mobile GPUs;
    // a fact for the upload-format gate, never an error. ETC2 needs no
    // query: GLES 3.0 core.
    let bc7_textures =
      ext.contains("GL_EXT_texture_compression_bptc") || ext.contains("GL_ARB_texture_compression_bptc");
    GpuLimits {
      max_texture_size: tex.min(rb).max(floor.max_texture_size as i32) as u32,
      max_cube_map_size: cube.max(floor.max_cube_map_size as i32) as u32,
      max_3d_texture_size: tex_3d.max(floor.max_3d_texture_size as i32) as u32,
      max_array_texture_layers: layers.max(floor.max_array_texture_layers as i32) as u32,
      max_texture_units: units.max(floor.max_texture_units as i32) as u32,
      max_vertex_attribs: attribs.max(floor.max_vertex_attribs as i32) as u32,
      max_anisotropy: anisotropy.max(floor.max_anisotropy as i32) as u32,
      max_vertex_uniform_vectors: vertex_vectors.max(floor.max_vertex_uniform_vectors as i32) as u32,
      half_float_renderable,
      bc7_textures,
    }
  }
}
