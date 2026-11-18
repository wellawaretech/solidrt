//! The GL half of a draw: the shape program, the instance buffer under the
//! VAO the executor owns, and the draw that uploads a list's instances
//! (`super::walk`) and issues them. Raster thread only, with the context
//! current.

use super::super::contract::Target;
use super::record::DisplayList;
use super::walk::{self, Instance};
use glow::HasContext;
use std::os::raw::c_void;

/// Two triangles a quad, their corners derived from `gl_VertexID`.
const VERTICES_PER_QUAD: i32 = 6;
/// Floats per instance attribute: each is a vec4.
const FLOATS_PER_ATTRIBUTE: i32 = 4;

// The attribute locations. They must match the `layout(location = N)`
// literals in VERTEX_SRC.
const ATTR_RECT: u32 = 0;
const ATTR_LINEAR: u32 = 1;
const ATTR_OFFSET: u32 = 2;
const ATTR_COLOR: u32 = 3;
const ATTR_TEX: u32 = 4;
const ATTR_CLIP: u32 = 5;

// A target's logical (0, 0) is its top: it maps to clip (-1, +1) for every
// target, window and texture alike. Window lists are drawn unflipped and
// offscreen lists arrive pre-flipped (raster::offscreen::flip_for_fbo),
// which is what put texel row 0 at the top of a sampled image under
// Impeller; gl/glyphs.rs and gl/program.rs describe the same row 0 is top
// convention for the passes, and the plan (okf/plans/own-renderer.md)
// pins it. Texture sampling (step 3) reads vUv with v = 0 at texel row 0,
// no flip.
//
// The layout(location = N) literals must match the ATTR_* constants.
const VERTEX_SRC: &str = r"#version 300 es
precision highp float;
layout(location = 0) in vec4 aRect;      // x, y, w, h in local space
layout(location = 1) in vec4 aLinear;    // m11, m12, m21, m22
layout(location = 2) in vec4 aOffset;    // m31, m32 (the translation), kind, stroke width
layout(location = 3) in vec4 aColor;     // premultiplied
layout(location = 4) in vec4 aTex;       // u0, v0, u1, v1
layout(location = 5) in vec4 aClip;      // x0, y0, x1, y1 in device pixels
uniform vec2 uSize;                      // the target in device pixels
out vec2 vLocal;                         // position within the shape, local pixels from its origin
out vec2 vDevice;                        // device pixels, y down
flat out vec4 vColor;
flat out vec4 vClip;
flat out vec4 vShape;                    // w, h, kind, stroke width
out vec2 vUv;
void main() {
  // One quad per instance, so this is gl_VertexID; the modulo keeps the
  // shader right if a later step draws several quads per instance.
  int corner = gl_VertexID - (gl_VertexID / 6) * 6;
  // Two triangles: (0,0) (1,0) (0,1) and (0,1) (1,0) (1,1).
  vec2 c = vec2(
    (corner == 1 || corner == 4 || corner == 5) ? 1.0 : 0.0,
    (corner == 2 || corner == 3 || corner == 5) ? 1.0 : 0.0);
  vec2 local = c * aRect.zw;
  vec2 p = aRect.xy + local;
  // euclid's row-vector convention: device = p * M, M = [m11 m12; m21 m22] + t.
  vec2 device = vec2(
    p.x * aLinear.x + p.y * aLinear.z + aOffset.x,
    p.x * aLinear.y + p.y * aLinear.w + aOffset.y);
  // Logical (0, 0) is the top of the target: clip y = +1.
  gl_Position = vec4(device.x / uSize.x * 2.0 - 1.0, 1.0 - device.y / uSize.y * 2.0, 0.0, 1.0);
  vLocal = local;
  vDevice = device;
  vColor = aColor;
  vClip = aClip;
  vShape = vec4(aRect.zw, aOffset.zw);
  vUv = aTex.xy + c * (aTex.zw - aTex.xy);
}
";

// Step 2 draws rect fills; steps 3 and 4 add the other kinds here. The
// unused varyings are kept so they add only fragment code.
const FRAGMENT_SRC: &str = r"#version 300 es
precision highp float;
in vec2 vLocal;
in vec2 vDevice;
flat in vec4 vColor;
flat in vec4 vClip;
flat in vec4 vShape;
in vec2 vUv;
out vec4 fragColor;
void main() {
  // The clip, per fragment, so a clip change never ends a batch. Under
  // MSAA the test runs per fragment, not per sample: a clip edge is hard
  // where a shape edge is antialiased. Acceptable for stage 1.
  if (any(lessThan(vDevice, vClip.xy)) || any(greaterThanEqual(vDevice, vClip.zw))) discard;
  fragColor = vColor;
}
";

/// The GL half: the shape program, the instance buffer and the draw. Lives
/// as long as the renderer, which the raster thread drops before it releases
/// the context (raster/recover.rs), so the GL names here die with it and
/// Drop deletes nothing.
pub(super) struct Executor {
  gl: glow::Context,
  program: glow::Program,
  /// `uSize`, the target in device pixels.
  size_uniform: glow::UniformLocation,
  vao: glow::VertexArray,
  instances: glow::Buffer,
}

impl Executor {
  /// Over the context current on the calling thread; compiles the program
  /// now (every frame draws shapes, so the first frame must not pay it).
  ///
  /// # Safety
  /// As `contract::Renderer::new`: the context stays current on this
  /// thread for the executor's life, and `proc_address` resolves its entry
  /// points.
  pub(super) unsafe fn new(proc_address: &mut dyn FnMut(&str) -> *mut c_void) -> Result<Self, String> {
    let gl = glow::Context::from_loader_function(|name| proc_address(name) as *const c_void);
    let program = link_program(&gl)?;
    let size_uniform =
      gl.get_uniform_location(program, "uSize").ok_or_else(|| "shape program has no uSize uniform".to_string())?;
    let vao = gl.create_vertex_array().map_err(|e| format!("shape vertex array: {e}"))?;
    let instances = gl.create_buffer().map_err(|e| format!("shape instance buffer: {e}"))?;
    gl.bind_vertex_array(Some(vao));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(instances));
    // The pointers are captured in the VAO, so a draw only rebinds it.
    let stride = std::mem::size_of::<Instance>() as i32;
    let attributes = [
      (ATTR_RECT, std::mem::offset_of!(Instance, rect)),
      (ATTR_LINEAR, std::mem::offset_of!(Instance, linear)),
      (ATTR_OFFSET, std::mem::offset_of!(Instance, offset_kind_stroke)),
      (ATTR_COLOR, std::mem::offset_of!(Instance, color)),
      (ATTR_TEX, std::mem::offset_of!(Instance, tex)),
      (ATTR_CLIP, std::mem::offset_of!(Instance, clip)),
    ];
    for (location, offset) in attributes {
      gl.enable_vertex_attrib_array(location);
      gl.vertex_attrib_pointer_f32(location, FLOATS_PER_ATTRIBUTE, glow::FLOAT, false, stride, offset as i32);
      // Every attribute is per instance: the corners come from gl_VertexID.
      gl.vertex_attrib_divisor(location, 1);
    }
    gl.bind_vertex_array(None);
    gl.bind_buffer(glow::ARRAY_BUFFER, None);
    Ok(Executor { gl, program, size_uniform, vao, instances })
  }

  /// Draw `list` into `target`, cleared first. Sets every piece of state
  /// it uses on each draw and caches none across draws: the other passes
  /// on this context save and restore around their own work, and the
  /// contract lets a draw leave state changed.
  pub(super) fn draw(&mut self, target: Target, list: &DisplayList) -> Result<(), String> {
    let instances = walk::walk(list, target.size);
    let (width, height) = (target.size.width as i32, target.size.height as i32);
    let gl = &self.gl;
    unsafe {
      gl.bind_framebuffer(glow::FRAMEBUFFER, target.framebuffer);
      gl.viewport(0, 0, width, height);
      gl.disable(glow::SCISSOR_TEST);
      gl.color_mask(true, true, true, true);
      gl.clear_color(0.0, 0.0, 0.0, 0.0);
      // Clearing the depth and stencil of a target that has none is a
      // no-op; the rig's depth-stencil attachment stays for stage 5.
      gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT | glow::STENCIL_BUFFER_BIT);
      // Premultiplied source-over in sRGB, no linear step.
      gl.enable(glow::BLEND);
      gl.blend_equation(glow::FUNC_ADD);
      gl.blend_func_separate(glow::ONE, glow::ONE_MINUS_SRC_ALPHA, glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
      gl.disable(glow::DEPTH_TEST);
      gl.disable(glow::STENCIL_TEST);
      gl.disable(glow::CULL_FACE);
      gl.disable(glow::SAMPLE_ALPHA_TO_COVERAGE);
      // Either would silently kill every draw while the clear still lands.
      gl.disable(glow::RASTERIZER_DISCARD);
      gl.disable(glow::SAMPLE_COVERAGE);
      if instances.is_empty() {
        return Ok(());
      }
      gl.use_program(Some(self.program));
      gl.uniform_2_f32(Some(&self.size_uniform), width as f32, height as f32);
      gl.bind_vertex_array(Some(self.vao));
      gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.instances));
      // Sound: Instance is repr(C) with every field an f32, so it has no
      // padding (walk.rs asserts its size) and any byte view of it is
      // initialized; the slice borrows `instances` for the upload only.
      let bytes =
        std::slice::from_raw_parts(instances.as_ptr() as *const u8, instances.len() * std::mem::size_of::<Instance>());
      gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);
      // One draw for the whole list: everything shares the program and no
      // texture in this step. Step 3 splits the run at texture changes;
      // stage 2 batches further.
      gl.draw_arrays_instanced(glow::TRIANGLES, 0, VERTICES_PER_QUAD, instances.len() as i32);
      gl.bind_vertex_array(None);
      gl.bind_buffer(glow::ARRAY_BUFFER, None);
      gl.use_program(None);
    }
    // The framebuffer stays bound as the target: the callers in
    // gl/draw.rs restore their own.
    Ok(())
  }
}

// The shape program, compiled and linked, its shaders deleted after the
// link.
unsafe fn link_program(gl: &glow::Context) -> Result<glow::Program, String> {
  let vertex = compile_shader(gl, glow::VERTEX_SHADER, VERTEX_SRC, "vertex")?;
  let fragment = match compile_shader(gl, glow::FRAGMENT_SHADER, FRAGMENT_SRC, "fragment") {
    Ok(fragment) => fragment,
    Err(e) => {
      gl.delete_shader(vertex);
      return Err(e);
    }
  };
  let program = match gl.create_program() {
    Ok(program) => program,
    Err(e) => {
      gl.delete_shader(vertex);
      gl.delete_shader(fragment);
      return Err(format!("shape program: {e}"));
    }
  };
  gl.attach_shader(program, vertex);
  gl.attach_shader(program, fragment);
  gl.link_program(program);
  let linked = gl.get_program_link_status(program);
  gl.detach_shader(program, vertex);
  gl.detach_shader(program, fragment);
  gl.delete_shader(vertex);
  gl.delete_shader(fragment);
  if !linked {
    let log = gl.get_program_info_log(program);
    gl.delete_program(program);
    return Err(format!("shape program link failed: {log}"));
  }
  Ok(program)
}

unsafe fn compile_shader(gl: &glow::Context, kind: u32, source: &str, label: &str) -> Result<glow::Shader, String> {
  let shader = gl.create_shader(kind).map_err(|e| format!("shape {label} shader: {e}"))?;
  gl.shader_source(shader, source);
  gl.compile_shader(shader);
  if !gl.get_shader_compile_status(shader) {
    let log = gl.get_shader_info_log(shader);
    gl.delete_shader(shader);
    return Err(format!("shape {label} shader compile failed: {log}"));
  }
  Ok(shader)
}
