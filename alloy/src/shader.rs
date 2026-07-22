use glow::HasContext;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::num::NonZeroU32;

// Attributeless fullscreen triangle. GLES 3.0 exposes gl_VertexID, so the three
// covering vertices are computed in the shader with no vertex buffer. vUV is the
// 0..1 screen coordinate with origin at the displayed top-left: a fragment at
// the bottom of the framebuffer (clip y = -1) lands in texture memory row 0,
// which Impeller samples as the top, so vUV = p needs no extra flip.
const VERTEX_SRC: &str = r"#version 300 es
out vec2 vUV;
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  vUV = p;
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
";

// Injected ahead of a user fragment body that omits its own #version line, so
// the body can reference vUV/fragColor/iResolution/iTime directly. A source that
// provides its own #version is treated as complete and gets no preamble.
const FRAGMENT_PREAMBLE: &str = r"#version 300 es
precision highp float;
in vec2 vUV;
out vec4 fragColor;
uniform vec2 iResolution;
uniform float iTime;
";

// Pipeline preambles. A pipeline's varyings are the user's own (the vertex
// shader declares `out`s matching the fragment's `in`s), so the pipeline
// fragment preamble declares no vUV. Attributes (`in` at vertex stage) are also
// the user's own; their locations are resolved by name against the declared
// attribute list. As above, a source with its own #version gets no preamble.
const PIPELINE_VERTEX_PREAMBLE: &str = r"#version 300 es
precision highp float;
uniform vec2 iResolution;
uniform float iTime;
";

const PIPELINE_FRAGMENT_PREAMBLE: &str = r"#version 300 es
precision highp float;
out vec4 fragColor;
uniform vec2 iResolution;
uniform float iTime;
";

/// A float vertex attribute's shape within the interleaved vertex buffer.
#[derive(Clone, Copy, Debug)]
pub enum AttrFormat {
  F32,
  Vec2,
  Vec3,
  Vec4,
}

impl AttrFormat {
  pub fn parse(s: &str) -> Result<Self, String> {
    Ok(match s {
      "f32" => AttrFormat::F32,
      "vec2" => AttrFormat::Vec2,
      "vec3" => AttrFormat::Vec3,
      "vec4" => AttrFormat::Vec4,
      _ => return Err(format!("unsupported attribute format '{s}' (expected f32|vec2|vec3|vec4)")),
    })
  }

  fn components(self) -> i32 {
    match self {
      AttrFormat::F32 => 1,
      AttrFormat::Vec2 => 2,
      AttrFormat::Vec3 => 3,
      AttrFormat::Vec4 => 4,
    }
  }

  /// The string form `parse` accepts, for reporting the layout back out.
  pub fn name(self) -> &'static str {
    match self {
      AttrFormat::F32 => "f32",
      AttrFormat::Vec2 => "vec2",
      AttrFormat::Vec3 => "vec3",
      AttrFormat::Vec4 => "vec4",
    }
  }
}

pub fn parse_topology(s: &str) -> Result<u32, String> {
  Ok(match s {
    "points" => glow::POINTS,
    "lines" => glow::LINES,
    "line-strip" => glow::LINE_STRIP,
    "triangles" => glow::TRIANGLES,
    "triangle-strip" => glow::TRIANGLE_STRIP,
    _ => return Err(format!("unsupported topology '{s}'")),
  })
}

/// The string form `parse_topology` accepts, for reporting back out.
pub fn topology_name(t: u32) -> &'static str {
  match t {
    glow::POINTS => "points",
    glow::LINES => "lines",
    glow::LINE_STRIP => "line-strip",
    glow::TRIANGLES => "triangles",
    glow::TRIANGLE_STRIP => "triangle-strip",
    _ => "unknown",
  }
}

/// Byte stride of one interleaved vertex for the given attribute list.
pub fn vertex_stride(attributes: &[(String, AttrFormat)]) -> i32 {
  attributes.iter().map(|(_, f)| f.components() * 4).sum()
}

fn prev_texture(name: i32) -> Option<glow::NativeTexture> {
  NonZeroU32::new(name as u32).map(glow::NativeTexture)
}
fn prev_framebuffer(name: i32) -> Option<glow::NativeFramebuffer> {
  NonZeroU32::new(name as u32).map(glow::NativeFramebuffer)
}
fn prev_program(name: i32) -> Option<glow::NativeProgram> {
  NonZeroU32::new(name as u32).map(glow::NativeProgram)
}
fn prev_vertex_array(name: i32) -> Option<glow::NativeVertexArray> {
  NonZeroU32::new(name as u32).map(glow::NativeVertexArray)
}
fn prev_buffer(name: i32) -> Option<glow::NativeBuffer> {
  NonZeroU32::new(name as u32).map(glow::NativeBuffer)
}

fn compile_shader(gl: &glow::Context, kind: u32, src: &str) -> Result<glow::Shader, String> {
  unsafe {
    let shader = gl.create_shader(kind).map_err(|e| format!("glCreateShader failed: {e}"))?;
    gl.shader_source(shader, src);
    gl.compile_shader(shader);
    if !gl.get_shader_compile_status(shader) {
      let log = gl.get_shader_info_log(shader);
      gl.delete_shader(shader);
      return Err(format!("shader compile failed: {log}"));
    }
    Ok(shader)
  }
}

fn with_preamble(src: &str, preamble: &str) -> String {
  if src.trim_start().starts_with("#version") {
    src.to_string()
  } else {
    format!("{preamble}{src}")
  }
}

fn link_program(gl: &glow::Context, vertex_full: &str, fragment_full: &str) -> Result<glow::Program, String> {
  unsafe {
    let vs = compile_shader(gl, glow::VERTEX_SHADER, vertex_full).map_err(|e| format!("vertex {e}"))?;
    let fs = match compile_shader(gl, glow::FRAGMENT_SHADER, fragment_full) {
      Ok(fs) => fs,
      Err(e) => {
        gl.delete_shader(vs);
        return Err(format!("fragment {e}"));
      }
    };
    let program = gl.create_program().map_err(|e| format!("glCreateProgram failed: {e}"))?;
    gl.attach_shader(program, vs);
    gl.attach_shader(program, fs);
    gl.link_program(program);
    // Once linked the program holds its own compiled copies; the shaders can go.
    gl.delete_shader(vs);
    gl.delete_shader(fs);
    if !gl.get_program_link_status(program) {
      let log = gl.get_program_info_log(program);
      gl.delete_program(program);
      return Err(format!("program link failed: {log}"));
    }
    Ok(program)
  }
}

/// A vertex buffer usable as a pipeline's interleaved attribute source. Owned
/// by the Context's buffer registry; pipelines reference it by registry id so a
/// write re-renders every pipeline drawing from it.
pub struct GpuBuffer {
  pub vbo: glow::Buffer,
  pub size: usize,
}

impl GpuBuffer {
  pub fn new(gl: &glow::Context, data: &[u8]) -> Result<Self, String> {
    unsafe {
      let prev = gl.get_parameter_i32(glow::ARRAY_BUFFER_BINDING);
      let vbo = gl.create_buffer().map_err(|e| format!("glGenBuffers failed: {e}"))?;
      gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
      // DYNAMIC_DRAW: buffers back per-frame geometry (e.g. sprite quads) as
      // often as static meshes, and the hint costs static users nothing.
      gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, data, glow::DYNAMIC_DRAW);
      gl.bind_buffer(glow::ARRAY_BUFFER, prev_buffer(prev));
      Ok(GpuBuffer { vbo, size: data.len() })
    }
  }

  pub fn write(&self, gl: &glow::Context, data: &[u8], byte_offset: usize) -> Result<(), String> {
    let end = byte_offset.checked_add(data.len()).ok_or_else(|| "offset overflow".to_string())?;
    if end > self.size {
      return Err(format!("write of {} bytes at offset {byte_offset} exceeds buffer size {}", data.len(), self.size));
    }
    unsafe {
      let prev = gl.get_parameter_i32(glow::ARRAY_BUFFER_BINDING);
      gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
      gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, byte_offset as i32, data);
      gl.bind_buffer(glow::ARRAY_BUFFER, prev_buffer(prev));
    }
    Ok(())
  }

  /// Read back part of the buffer via glMapBufferRange (ES 3.0's only buffer
  /// readback path; glGetBufferSubData does not exist there). On-demand and
  /// rare (a dev-server query), so the map stall is acceptable.
  pub fn read(&self, gl: &glow::Context, byte_offset: usize, len: usize) -> Result<Vec<u8>, String> {
    let end = byte_offset.checked_add(len).ok_or_else(|| "offset overflow".to_string())?;
    if end > self.size {
      return Err(format!("read of {len} bytes at offset {byte_offset} exceeds buffer size {}", self.size));
    }
    if len == 0 {
      return Ok(Vec::new());
    }
    unsafe {
      let prev = gl.get_parameter_i32(glow::ARRAY_BUFFER_BINDING);
      gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
      let ptr = gl.map_buffer_range(glow::ARRAY_BUFFER, byte_offset as i32, len as i32, glow::MAP_READ_BIT);
      let result = if ptr.is_null() {
        Err("glMapBufferRange failed".to_string())
      } else {
        let data = std::slice::from_raw_parts(ptr, len).to_vec();
        gl.unmap_buffer(glow::ARRAY_BUFFER);
        Ok(data)
      };
      gl.bind_buffer(glow::ARRAY_BUFFER, prev_buffer(prev));
      result
    }
  }

  pub fn destroy(self, gl: &glow::Context) {
    unsafe { gl.delete_buffer(self.vbo) };
  }
}

/// Everything that makes a shader target a vertex+fragment pipeline instead of
/// a fullscreen fragment pass: the VAO describing the interleaved vertex
/// layout, the source buffer's registry id, and the draw state.
struct MeshState {
  vao: glow::VertexArray,
  /// Registry id of the interleaved vertex buffer (Context resolves writes to
  /// re-renders through this). 0 when the pipeline is attributeless.
  buffer_id: u64,
  /// The declared interleaved layout, kept for resource introspection (the VAO
  /// holds the live GL form).
  attributes: Vec<(String, AttrFormat)>,
  topology: u32,
  draw_count: Cell<i32>,
  /// Present when the pipeline was created with depth testing; the
  /// renderbuffer stays private to the FBO (never adopted into Impeller).
  depth: Option<glow::Renderbuffer>,
  clear_color: [f32; 4],
}

/// A compiled shader program with its own FBO-backed RGBA8 target texture,
/// either a fullscreen fragment pass (`mesh: None`) or a vertex+fragment
/// pipeline drawing an interleaved vertex buffer (`mesh: Some`). The target's
/// GL name is also adopted into Impeller (and held in the TextureRegistry);
/// this struct keeps the program/FBO so the same texture can be re-rendered
/// with new params. Like GpuTexture it never deletes the target name: Impeller
/// owns it once adopted, and deleting here would double-free.
pub struct ShaderTexture {
  program: glow::Program,
  fbo: glow::Framebuffer,
  target: glow::Texture,
  width: u32,
  height: u32,
  /// Active uniform name -> location, reflected once at link time. JS params are
  /// matched to locations by name; iResolution is filled from the target size.
  uniforms: HashMap<String, glow::UniformLocation>,
  /// sampler2D uniform name -> source texture id. Resolved to a live GL texture
  /// at each render by the owner (which holds the texture registry), so an input
  /// whose contents or registry entry changed is picked up automatically.
  sampler_bindings: Vec<(String, u64)>,
  mesh: Option<MeshState>,
  /// Params applied on the most recent render, kept so a vertex-buffer write or
  /// draw-count change can re-render without the caller re-supplying them.
  last_params: RefCell<Vec<(String, f32)>>,
}

/// Target texture + FBO shared by both shader kinds. Returns (target, fbo)
/// with the FBO left bound so the caller can attach more (depth) before the
/// completeness check; the caller restores the previous FBO binding.
fn create_target(gl: &glow::Context, width: u32, height: u32) -> Result<(glow::Texture, glow::Framebuffer), String> {
  unsafe {
    let prev_tex = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);

    let target = gl.create_texture().map_err(|e| format!("glGenTextures failed: {e}"))?;
    gl.bind_texture(glow::TEXTURE_2D, Some(target));
    gl.tex_image_2d(
      glow::TEXTURE_2D,
      0,
      glow::RGBA8 as i32,
      width as i32,
      height as i32,
      0,
      glow::RGBA,
      glow::UNSIGNED_BYTE,
      glow::PixelUnpackData::Slice(None),
    );
    // No mips exist: the default MIN_FILTER references mipmaps, which would
    // make the texture sampling-incomplete (reads as black) when Impeller
    // samples it.
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
    gl.bind_texture(glow::TEXTURE_2D, prev_texture(prev_tex));

    let fbo = match gl.create_framebuffer() {
      Ok(fbo) => fbo,
      Err(e) => {
        gl.delete_texture(target);
        return Err(format!("glGenFramebuffers failed: {e}"));
      }
    };
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
    gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(target), 0);
    Ok((target, fbo))
  }
}

/// Reflect active uniforms so JS params can be matched to locations by name.
fn reflect_uniforms(gl: &glow::Context, program: glow::Program) -> HashMap<String, glow::UniformLocation> {
  let mut uniforms = HashMap::new();
  unsafe {
    let count = gl.get_active_uniforms(program);
    for i in 0..count {
      if let Some(u) = gl.get_active_uniform(program, i) {
        if let Some(loc) = gl.get_uniform_location(program, &u.name) {
          uniforms.insert(u.name, loc);
        }
      }
    }
  }
  uniforms
}

impl ShaderTexture {
  pub fn new(
    gl: &glow::Context,
    width: u32,
    height: u32,
    fragment_src: &str,
    sampler_bindings: Vec<(String, u64)>,
  ) -> Result<Self, String> {
    let fragment_full = with_preamble(fragment_src, FRAGMENT_PREAMBLE);
    let program = link_program(gl, VERTEX_SRC, &fragment_full)?;

    unsafe {
      let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
      let (target, fbo) = match create_target(gl, width, height) {
        Ok(pair) => pair,
        Err(e) => {
          gl.delete_program(program);
          return Err(e);
        }
      };
      let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
      gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));

      if status != glow::FRAMEBUFFER_COMPLETE {
        gl.delete_framebuffer(fbo);
        gl.delete_texture(target);
        gl.delete_program(program);
        return Err(format!("shader framebuffer incomplete: {status:#x}"));
      }

      let uniforms = reflect_uniforms(gl, program);
      Ok(ShaderTexture {
        program,
        fbo,
        target,
        width,
        height,
        uniforms,
        sampler_bindings,
        mesh: None,
        last_params: RefCell::new(Vec::new()),
      })
    }
  }

  /// A vertex+fragment pipeline rendering into its own target texture.
  /// `attributes` describes one interleaved float vertex in `vbo` (resolved by
  /// the owner from `buffer_id`); locations are looked up by name, so an
  /// attribute the shader does not use is skipped (its bytes still occupy the
  /// stride). Pass an empty attribute list (and `buffer_id` 0) for
  /// attributeless rendering driven by gl_VertexID.
  #[allow(clippy::too_many_arguments)]
  pub fn new_pipeline(
    gl: &glow::Context,
    width: u32,
    height: u32,
    vertex_src: &str,
    fragment_src: &str,
    sampler_bindings: Vec<(String, u64)>,
    attributes: &[(String, AttrFormat)],
    vbo: Option<glow::Buffer>,
    buffer_id: u64,
    topology: u32,
    draw_count: i32,
    depth: bool,
    clear_color: [f32; 4],
  ) -> Result<Self, String> {
    if !attributes.is_empty() && vbo.is_none() {
      return Err("pipeline declares attributes but no vertex buffer".to_string());
    }
    let vertex_full = with_preamble(vertex_src, PIPELINE_VERTEX_PREAMBLE);
    let fragment_full = with_preamble(fragment_src, PIPELINE_FRAGMENT_PREAMBLE);
    let program = link_program(gl, &vertex_full, &fragment_full)?;

    unsafe {
      let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
      let prev_rb = gl.get_parameter_i32(glow::RENDERBUFFER_BINDING);
      let prev_vao = gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING);
      let prev_ab = gl.get_parameter_i32(glow::ARRAY_BUFFER_BINDING);

      // Cleanup helper for every early exit below.
      let fail = |gl: &glow::Context,
                  msg: String,
                  target: Option<glow::Texture>,
                  fbo: Option<glow::Framebuffer>,
                  rb: Option<glow::Renderbuffer>,
                  vao: Option<glow::VertexArray>| {
        if let Some(v) = vao {
          gl.delete_vertex_array(v);
        }
        if let Some(r) = rb {
          gl.delete_renderbuffer(r);
        }
        if let Some(f) = fbo {
          gl.delete_framebuffer(f);
        }
        if let Some(t) = target {
          gl.delete_texture(t);
        }
        gl.delete_program(program);
        Err(msg)
      };

      let (target, fbo) = match create_target(gl, width, height) {
        Ok(pair) => pair,
        Err(e) => {
          gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
          return fail(gl, e, None, None, None, None);
        }
      };

      // FBO is still bound; attach a private depth renderbuffer when asked.
      let depth_rb = if depth {
        match gl.create_renderbuffer() {
          Ok(rb) => {
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
            gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, width as i32, height as i32);
            gl.bind_renderbuffer(glow::RENDERBUFFER, NonZeroU32::new(prev_rb as u32).map(glow::NativeRenderbuffer));
            gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(rb));
            Some(rb)
          }
          Err(e) => {
            gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
            return fail(gl, format!("glGenRenderbuffers failed: {e}"), Some(target), Some(fbo), None, None);
          }
        }
      } else {
        None
      };

      let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
      gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
      if status != glow::FRAMEBUFFER_COMPLETE {
        return fail(
          gl,
          format!("pipeline framebuffer incomplete: {status:#x}"),
          Some(target),
          Some(fbo),
          depth_rb,
          None,
        );
      }

      // Record the interleaved vertex layout in a VAO. The VAO captures the
      // buffer binding per attribute, so rendering only rebinds the VAO.
      let vao = match gl.create_vertex_array() {
        Ok(vao) => vao,
        Err(e) => {
          return fail(gl, format!("glGenVertexArrays failed: {e}"), Some(target), Some(fbo), depth_rb, None);
        }
      };
      gl.bind_vertex_array(Some(vao));
      if let Some(vbo) = vbo {
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        let stride = vertex_stride(attributes);
        let mut offset = 0i32;
        for (name, fmt) in attributes {
          // None means the shader does not (actively) use the attribute; that
          // is fine, the bytes are simply skipped over via the stride.
          if let Some(loc) = gl.get_attrib_location(program, name) {
            gl.enable_vertex_attrib_array(loc);
            gl.vertex_attrib_pointer_f32(loc, fmt.components(), glow::FLOAT, false, stride, offset);
          }
          offset += fmt.components() * 4;
        }
      }
      gl.bind_vertex_array(prev_vertex_array(prev_vao));
      gl.bind_buffer(glow::ARRAY_BUFFER, prev_buffer(prev_ab));

      let uniforms = reflect_uniforms(gl, program);
      Ok(ShaderTexture {
        program,
        fbo,
        target,
        width,
        height,
        uniforms,
        sampler_bindings,
        mesh: Some(MeshState {
          vao,
          buffer_id,
          attributes: attributes.to_vec(),
          topology,
          draw_count: Cell::new(draw_count),
          depth: depth_rb,
          clear_color,
        }),
        last_params: RefCell::new(Vec::new()),
      })
    }
  }

  pub fn gl_texture(&self) -> glow::Texture {
    self.target
  }

  /// Registry id of the vertex buffer this pipeline draws from, if any.
  pub fn buffer_id(&self) -> Option<u64> {
    self.mesh.as_ref().map(|m| m.buffer_id).filter(|id| *id != 0)
  }

  /// Whether this is a vertex+fragment pipeline (vs a fullscreen fragment pass).
  pub fn is_pipeline(&self) -> bool {
    self.mesh.is_some()
  }

  /// The number of vertices the next render draws; None on a fragment-only
  /// shader.
  pub fn draw_count(&self) -> Option<i32> {
    self.mesh.as_ref().map(|m| m.draw_count.get())
  }

  /// The pipeline's topology as the string `parse_topology` accepts; None on a
  /// fragment-only shader.
  pub fn topology_name(&self) -> Option<&'static str> {
    self.mesh.as_ref().map(|m| topology_name(m.topology))
  }

  /// The declared interleaved attribute layout; empty for fragment-only
  /// shaders and attributeless pipelines.
  pub fn attributes(&self) -> &[(String, AttrFormat)] {
    self.mesh.as_ref().map(|m| m.attributes.as_slice()).unwrap_or(&[])
  }

  /// Whether the pipeline renders with a depth buffer attached.
  pub fn has_depth(&self) -> bool {
    self.mesh.as_ref().is_some_and(|m| m.depth.is_some())
  }

  /// Set the number of vertices the next render draws. Errors on a
  /// fragment-only shader (its fullscreen triangle is fixed).
  pub fn set_draw_count(&self, count: i32) -> Result<(), String> {
    let mesh = self.mesh.as_ref().ok_or_else(|| "not a pipeline texture".to_string())?;
    mesh.draw_count.set(count);
    Ok(())
  }

  /// The params applied on the most recent render, for re-renders triggered by
  /// vertex-buffer writes or draw-count changes.
  pub fn last_params(&self) -> Vec<(String, f32)> {
    self.last_params.borrow().clone()
  }

  /// Recreate the render target at a new size, keeping the compiled program,
  /// FBO, sampler bindings, and draw state; the caller re-renders afterwards.
  /// The old target texture is NOT deleted here: Impeller owns its GL name via
  /// the adopted Texture handle (see register_shader_target), which dies with
  /// the UI side's last reference once the registry entry is replaced. On
  /// error the old target is left attached and the shader stays usable at its
  /// previous size.
  pub fn resize(&mut self, gl: &glow::Context, width: u32, height: u32) -> Result<(), String> {
    unsafe {
      let prev_tex = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
      let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);

      let target = gl.create_texture().map_err(|e| format!("glGenTextures failed: {e}"))?;
      gl.bind_texture(glow::TEXTURE_2D, Some(target));
      gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RGBA8 as i32,
        width as i32,
        height as i32,
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelUnpackData::Slice(None),
      );
      // Same sampling state as create_target: no mips exist, so the default
      // MIN_FILTER would make the texture sampling-incomplete (reads black).
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
      gl.bind_texture(glow::TEXTURE_2D, prev_texture(prev_tex));

      gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
      gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(target), 0);

      // A pipeline's private depth buffer must match the color target's size
      // or the FBO goes incomplete.
      let depth_rb = self.mesh.as_ref().and_then(|m| m.depth);
      if let Some(rb) = depth_rb {
        let prev_rb = gl.get_parameter_i32(glow::RENDERBUFFER_BINDING);
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
        gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, width as i32, height as i32);
        gl.bind_renderbuffer(glow::RENDERBUFFER, NonZeroU32::new(prev_rb as u32).map(glow::NativeRenderbuffer));
      }

      let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
      if status != glow::FRAMEBUFFER_COMPLETE {
        // Roll back to the old target (and depth storage) so the shader keeps
        // rendering at its previous size.
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(self.target), 0);
        if let Some(rb) = depth_rb {
          let prev_rb = gl.get_parameter_i32(glow::RENDERBUFFER_BINDING);
          gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rb));
          gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, self.width as i32, self.height as i32);
          gl.bind_renderbuffer(glow::RENDERBUFFER, NonZeroU32::new(prev_rb as u32).map(glow::NativeRenderbuffer));
        }
        gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
        gl.delete_texture(target);
        return Err(format!("shader framebuffer incomplete after resize: {status:#x}"));
      }
      gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));

      self.target = target;
      self.width = width;
      self.height = height;
      Ok(())
    }
  }

  /// Release GL resources owned by this shader (program, FBO, and for
  /// pipelines the VAO and depth renderbuffer). The target texture is NOT
  /// deleted here: Impeller owns it via the adopted Texture handle in the
  /// TextureRegistry, and that handle is responsible for deletion. The vertex
  /// buffer is owned by the buffer registry, not deleted here either.
  pub fn destroy(self, gl: &glow::Context) {
    unsafe {
      if let Some(mesh) = &self.mesh {
        gl.delete_vertex_array(mesh.vao);
        if let Some(rb) = mesh.depth {
          gl.delete_renderbuffer(rb);
        }
      }
      gl.delete_framebuffer(self.fbo);
      gl.delete_program(self.program);
    }
  }

  /// The sampler2D inputs this shader declared, as (uniform name, source texture
  /// id). The owner resolves each id to a live GL texture before rendering.
  pub fn sampler_bindings(&self) -> &[(String, u64)] {
    &self.sampler_bindings
  }

  /// Render the shader into its target texture with the given float params and
  /// resolved sampler inputs (uniform name -> source GL texture, in the order
  /// `sampler_bindings` declared them). Saves and restores the GL bindings and
  /// enables it touches so Impeller's cached state stays valid; Context::submit's
  /// per-frame fence orders the work ahead of the render thread sampling the
  /// target from its shared GL context, so no glFinish is needed here.
  pub fn render(&self, gl: &glow::Context, params: &[(String, f32)], textures: &[(String, glow::Texture)]) {
    // Recorded for both kinds: pipelines need it for buffer-write re-renders,
    // and resource introspection reports it as the last-applied uniforms.
    // Re-renders triggered with an empty list (a sampled texture's contents
    // changed before any params update) keep the previous record: uniforms are
    // program state in GL, so the old values still apply.
    if !params.is_empty() {
      *self.last_params.borrow_mut() = params.to_vec();
    }
    unsafe {
      let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
      let prev_program_name = gl.get_parameter_i32(glow::CURRENT_PROGRAM);
      let prev_active = gl.get_parameter_i32(glow::ACTIVE_TEXTURE);
      let mut prev_vp = [0i32; 4];
      gl.get_parameter_i32_slice(glow::VIEWPORT, &mut prev_vp);
      let blend = gl.is_enabled(glow::BLEND);
      let depth = gl.is_enabled(glow::DEPTH_TEST);
      let scissor = gl.is_enabled(glow::SCISSOR_TEST);
      let cull = gl.is_enabled(glow::CULL_FACE);

      gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
      gl.viewport(0, 0, self.width as i32, self.height as i32);
      gl.use_program(Some(self.program));

      if let Some(loc) = self.uniforms.get("iResolution") {
        gl.uniform_2_f32(Some(loc), self.width as f32, self.height as f32);
      }
      for (name, value) in params {
        if let Some(loc) = self.uniforms.get(name) {
          gl.uniform_1_f32(Some(loc), *value);
        }
      }

      // Bind each resolved sampler input to its own texture unit and point the
      // sampler uniform at that unit. Save the prior binding per unit so Impeller
      // (which assumes its own units) is not left looking at our textures.
      let mut prev_unit_bindings: Vec<(u32, i32)> = Vec::new();
      for (unit, (name, tex)) in textures.iter().enumerate() {
        let Some(loc) = self.uniforms.get(name) else { continue };
        let unit = unit as u32;
        gl.active_texture(glow::TEXTURE0 + unit);
        prev_unit_bindings.push((unit, gl.get_parameter_i32(glow::TEXTURE_BINDING_2D)));
        gl.bind_texture(glow::TEXTURE_2D, Some(*tex));
        gl.uniform_1_i32(Some(loc), unit as i32);
      }

      // Fixed-function state that could clip or blend the output away is off
      // for both paths; the mesh path opts depth testing back in below. This
      // set must be exhaustive: after a node capture, Impeller has run a full
      // render pass on this same UI-thread context (its usual painting happens
      // on the render thread's separate context) and may have left any of the
      // states below active - e.g. rasterizer discard or a zero sample
      // coverage silently kills every draw while clears still land.
      gl.disable(glow::BLEND);
      gl.disable(glow::DEPTH_TEST);
      gl.disable(glow::SCISSOR_TEST);
      gl.disable(glow::CULL_FACE);
      let stencil = gl.is_enabled(glow::STENCIL_TEST);
      let discard = gl.is_enabled(glow::RASTERIZER_DISCARD);
      let alpha_to_coverage = gl.is_enabled(glow::SAMPLE_ALPHA_TO_COVERAGE);
      let sample_coverage = gl.is_enabled(glow::SAMPLE_COVERAGE);
      let polygon_offset = gl.is_enabled(glow::POLYGON_OFFSET_FILL);
      let mut prev_color_mask = [0i32; 4];
      gl.get_parameter_i32_slice(glow::COLOR_WRITEMASK, &mut prev_color_mask);
      let mut prev_depth_range = [0f32; 2];
      gl.get_parameter_f32_slice(glow::DEPTH_RANGE, &mut prev_depth_range);
      gl.disable(glow::STENCIL_TEST);
      gl.disable(glow::RASTERIZER_DISCARD);
      gl.disable(glow::SAMPLE_ALPHA_TO_COVERAGE);
      gl.disable(glow::SAMPLE_COVERAGE);
      gl.disable(glow::POLYGON_OFFSET_FILL);
      gl.color_mask(true, true, true, true);
      gl.depth_range_f32(0.0, 1.0);

      match &self.mesh {
        None => {
          // Fullscreen fragment pass: the triangle writes opaque coverage over
          // the whole target, so nothing needs clearing.
          gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
        Some(mesh) => {
          // Mesh pass: geometry does not cover the target, so clear first, and
          // depth-test the draw when a depth buffer is attached. Clear color,
          // depth mask, and depth func are Impeller-cached state too: save and
          // restore them around the pass.
          let prev_vao = gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING);
          let mut prev_clear = [0f32; 4];
          gl.get_parameter_f32_slice(glow::COLOR_CLEAR_VALUE, &mut prev_clear);
          let prev_depth_mask = gl.get_parameter_i32(glow::DEPTH_WRITEMASK) != 0;
          let prev_depth_func = gl.get_parameter_i32(glow::DEPTH_FUNC) as u32;
          let prev_clear_depth = gl.get_parameter_f32(glow::DEPTH_CLEAR_VALUE);

          let [r, g, b, a] = mesh.clear_color;
          gl.clear_color(r, g, b, a);
          if mesh.depth.is_some() {
            gl.enable(glow::DEPTH_TEST);
            gl.depth_mask(true);
            gl.depth_func(glow::LESS);
            // Impeller's clip-culling passes set their own depth-clear value
            // (0.0) on this context; clearing with that inverts the test and
            // silently discards every fragment. Always clear to the far plane.
            gl.clear_depth_f32(1.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
          } else {
            gl.clear(glow::COLOR_BUFFER_BIT);
          }
          gl.bind_vertex_array(Some(mesh.vao));
          gl.draw_arrays(mesh.topology, 0, mesh.draw_count.get());

          gl.bind_vertex_array(prev_vertex_array(prev_vao));
          gl.clear_color(prev_clear[0], prev_clear[1], prev_clear[2], prev_clear[3]);
          gl.depth_mask(prev_depth_mask);
          gl.depth_func(prev_depth_func);
          gl.clear_depth_f32(prev_clear_depth);
        }
      }

      // Restore prior GL state for Impeller.
      if stencil {
        gl.enable(glow::STENCIL_TEST);
      }
      if discard {
        gl.enable(glow::RASTERIZER_DISCARD);
      }
      if alpha_to_coverage {
        gl.enable(glow::SAMPLE_ALPHA_TO_COVERAGE);
      }
      if sample_coverage {
        gl.enable(glow::SAMPLE_COVERAGE);
      }
      if polygon_offset {
        gl.enable(glow::POLYGON_OFFSET_FILL);
      }
      gl.color_mask(prev_color_mask[0] != 0, prev_color_mask[1] != 0, prev_color_mask[2] != 0, prev_color_mask[3] != 0);
      gl.depth_range_f32(prev_depth_range[0], prev_depth_range[1]);
      for (unit, prev) in prev_unit_bindings {
        gl.active_texture(glow::TEXTURE0 + unit);
        gl.bind_texture(glow::TEXTURE_2D, prev_texture(prev));
      }
      gl.active_texture(prev_active as u32);
      gl.use_program(prev_program(prev_program_name));
      gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
      gl.viewport(prev_vp[0], prev_vp[1], prev_vp[2], prev_vp[3]);
      if blend {
        gl.enable(glow::BLEND);
      }
      if depth {
        gl.enable(glow::DEPTH_TEST);
      }
      if scissor {
        gl.enable(glow::SCISSOR_TEST);
      }
      if cull {
        gl.enable(glow::CULL_FACE);
      }

      // A latched error here means some part of the pass silently no-opped
      // (e.g. a bad enum on this driver); surface it instead of drawing black.
      let err = gl.get_error();
      if err != glow::NO_ERROR {
        log::warn!("[shader] GL error {err:#x} after shader target render");
      }
    }
  }
}
