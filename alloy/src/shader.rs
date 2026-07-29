use glow::HasContext;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;

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

/// A shader uniform value as supplied from the app: a scalar or a flat
/// component array. The shader's own declaration decides how components are
/// dispatched (vec2/vec3/vec4/mat4, float or int scalar) - the value only
/// carries numbers, matched against the reflected uniform type at render.
#[derive(Clone, Debug, PartialEq)]
pub enum ParamValue {
  Scalar(f32),
  Array(Vec<f32>),
}

impl ParamValue {
  /// The value's components as one flat slice (a scalar is one component).
  pub fn components(&self) -> &[f32] {
    match self {
      ParamValue::Scalar(v) => std::slice::from_ref(v),
      ParamValue::Array(a) => a.as_slice(),
    }
  }
}

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

/// Blending applied to a pipeline's mesh draw. Absent (the default) the draw
/// overwrites: overlapping geometry resolves by depth or draw order, never by
/// accumulation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlendMode {
  /// glBlendFunc(ONE, ONE): every fragment adds onto what is already in the
  /// target. Order-independent, so geometry needs no sorting - the additive
  /// half of translucency (point splats, glow passes).
  Add,
}

pub fn parse_blend(s: &str) -> Result<Option<BlendMode>, String> {
  Ok(match s {
    "none" => None,
    "add" => Some(BlendMode::Add),
    _ => return Err(format!("unsupported blend mode '{s}' (expected none|add)")),
  })
}

/// The string form `parse_blend` accepts, for reporting back out.
pub fn blend_name(b: Option<BlendMode>) -> &'static str {
  match b {
    None => "none",
    Some(BlendMode::Add) => "add",
  }
}

/// How a pipeline's vertices assemble into primitives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Topology {
  Points,
  Lines,
  LineStrip,
  Triangles,
  TriangleStrip,
}

impl Topology {
  pub fn parse(s: &str) -> Result<Self, String> {
    Ok(match s {
      "points" => Topology::Points,
      "lines" => Topology::Lines,
      "line-strip" => Topology::LineStrip,
      "triangles" => Topology::Triangles,
      "triangle-strip" => Topology::TriangleStrip,
      _ => return Err(format!("unsupported topology '{s}'")),
    })
  }

  /// The string form `parse` accepts, for reporting back out.
  pub fn name(self) -> &'static str {
    match self {
      Topology::Points => "points",
      Topology::Lines => "lines",
      Topology::LineStrip => "line-strip",
      Topology::Triangles => "triangles",
      Topology::TriangleStrip => "triangle-strip",
    }
  }

  fn gl(self) -> u32 {
    match self {
      Topology::Points => glow::POINTS,
      Topology::Lines => glow::LINES,
      Topology::LineStrip => glow::LINE_STRIP,
      Topology::Triangles => glow::TRIANGLES,
      Topology::TriangleStrip => glow::TRIANGLE_STRIP,
    }
  }
}

/// Depth state for a pipeline's draws. Present means every target gets a
/// private depth buffer and the draw tests against it; `write` is whether the
/// draw also writes it (the clear always does). "Test without write" is the
/// blended-pass half an app opts into explicitly; "write without test" does
/// not exist, which is exactly why this is an Option of a struct and not two
/// booleans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DepthState {
  pub write: bool,
}

/// The draw-state half of a render pipeline: everything about HOW a program
/// draws (vertex layout, primitive assembly, blending, depth), as opposed to
/// where it draws (the target's size, buffer, and clear are per-target).
/// Vocabulary is typed here - callers parse strings at their own boundary
/// (`AttrFormat::parse`, `Topology::parse`, `parse_blend`), so an invalid
/// word fails at the call site, not on the raster thread.
#[derive(Clone, Debug)]
pub struct PipelineDesc {
  /// One interleaved float vertex, in buffer order: (attribute name, format).
  /// Empty for attributeless rendering driven by gl_VertexID.
  pub attributes: Vec<(String, AttrFormat)>,
  pub topology: Topology,
  /// None = overwrite (the default); see `BlendMode`.
  pub blend: Option<BlendMode>,
  pub depth: Option<DepthState>,
}

impl Default for PipelineDesc {
  fn default() -> Self {
    PipelineDesc { attributes: Vec::new(), topology: Topology::Triangles, blend: None, depth: None }
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
fn prev_sampler(name: i32) -> Option<glow::NativeSampler> {
  NonZeroU32::new(name as u32).map(glow::NativeSampler)
}

/// A resolved sampler input for a pass: uniform name, source GL texture, and
/// the sampler object carrying the source's declared filter/wrap (None for
/// internal textures - window layers, the MSAA resolve - which keep their
/// texture-object state).
pub type PassInput = (String, glow::Texture, Option<glow::Sampler>);

/// A stage of the programmable pipeline, for the raw compile path.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShaderStage {
  Vertex,
  Fragment,
}

impl ShaderStage {
  pub fn parse(s: &str) -> Result<Self, String> {
    Ok(match s {
      "vertex" => ShaderStage::Vertex,
      "fragment" => ShaderStage::Fragment,
      _ => return Err(format!("unsupported shader stage '{s}' (expected vertex|fragment)")),
    })
  }

  pub fn name(self) -> &'static str {
    match self {
      ShaderStage::Vertex => "vertex",
      ShaderStage::Fragment => "fragment",
    }
  }

  fn gl_kind(self) -> u32 {
    match self {
      ShaderStage::Vertex => glow::VERTEX_SHADER,
      ShaderStage::Fragment => glow::FRAGMENT_SHADER,
    }
  }
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

/// Compile a single stage: the raw primitive under `from_stages`. By default
/// nothing is injected - the source is complete GLSL ES, declaring its own
/// `#version`, precision, varyings and uniforms (a missing `#version` line is
/// the most common mistake, so the error hints at it). With `header` the
/// stage-appropriate standard header is prepended - explicitly, on request:
/// `#version 300 es`, highp float precision, the `iResolution`/`iTime`
/// uniforms, and for a fragment stage `out vec4 fragColor`. A source
/// carrying its own `#version` must not also ask for the header.
pub fn compile_stage(gl: &glow::Context, stage: ShaderStage, src: &str, header: bool) -> Result<glow::Shader, String> {
  let full;
  let src = if header {
    let preamble = match stage {
      ShaderStage::Vertex => PIPELINE_VERTEX_PREAMBLE,
      ShaderStage::Fragment => PIPELINE_FRAGMENT_PREAMBLE,
    };
    full = format!("{preamble}{src}");
    &full
  } else {
    src
  };
  compile_shader(gl, stage.gl_kind(), src).map_err(|e| {
    if header || src.trim_start().starts_with("#version") {
      format!("{} {e}", stage.name())
    } else {
      format!(
        "{} {e} (raw sources are complete GLSL ES: start with `#version 300 es`, or pass header: true)",
        stage.name()
      )
    }
  })
}

/// Delete a compiled stage. Safe right after linking: a linked program keeps
/// its own compiled copies, per GL semantics.
pub fn delete_stage(gl: &glow::Context, shader: glow::Shader) {
  unsafe { gl.delete_shader(shader) };
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

/// A compiled and linked GL program with its reflected uniform locations: the
/// compile half of what used to be fused into ShaderTexture. One program can
/// back many targets (the ordinary GL shape); targets hold it by Rc, so it
/// stays alive - and its GL name stays valid - until the last user is gone,
/// whichever destruction order the app picks. Owned by the raster thread:
/// registered programs live in its registry, a fused createShader's program is
/// held only by its target.
pub struct ShaderProgram {
  program: glow::Program,
  /// Active uniform name -> (location, GL type), reflected once at link time.
  /// JS params are matched to locations by name and dispatched by the
  /// reflected type; iResolution is filled from the target size at render.
  uniforms: HashMap<String, (glow::UniformLocation, u32)>,
  /// Vertex+fragment pipeline (own vertex stage over a buffer) vs fullscreen
  /// fragment pass. Decides which target shape the program can back.
  pipeline: bool,
}

impl ShaderProgram {
  /// Compile a fullscreen fragment pass: the fixed covering-triangle vertex
  /// stage plus `fragment_src` (preamble-injected unless it brings its own
  /// #version).
  pub fn new_fragment(gl: &glow::Context, fragment_src: &str) -> Result<Self, String> {
    let fragment_full = with_preamble(fragment_src, FRAGMENT_PREAMBLE);
    let program = link_program(gl, VERTEX_SRC, &fragment_full)?;
    let uniforms = reflect_uniforms(gl, program);
    Ok(ShaderProgram { program, uniforms, pipeline: false })
  }

  /// Compile a vertex+fragment pipeline program. Attribute locations are
  /// resolved by name later, when a target's VAO is built against a concrete
  /// buffer layout.
  pub fn new_pipeline(gl: &glow::Context, vertex_src: &str, fragment_src: &str) -> Result<Self, String> {
    let vertex_full = with_preamble(vertex_src, PIPELINE_VERTEX_PREAMBLE);
    let fragment_full = with_preamble(fragment_src, PIPELINE_FRAGMENT_PREAMBLE);
    let program = link_program(gl, &vertex_full, &fragment_full)?;
    let uniforms = reflect_uniforms(gl, program);
    Ok(ShaderProgram { program, uniforms, pipeline: true })
  }

  /// Link two already-compiled stages into a program: the raw path, no
  /// preamble anywhere. The stages stay owned by the caller (attach, link,
  /// detach), so they can back further links or be deleted independently. A
  /// raw-linked program carries its own vertex stage and is therefore
  /// pipeline-kind: targets built over it use the mesh draw path.
  pub fn from_stages(gl: &glow::Context, vertex: glow::Shader, fragment: glow::Shader) -> Result<Self, String> {
    unsafe {
      let program = gl.create_program().map_err(|e| format!("glCreateProgram failed: {e}"))?;
      gl.attach_shader(program, vertex);
      gl.attach_shader(program, fragment);
      gl.link_program(program);
      gl.detach_shader(program, vertex);
      gl.detach_shader(program, fragment);
      if !gl.get_program_link_status(program) {
        let log = gl.get_program_info_log(program);
        gl.delete_program(program);
        return Err(format!("program link failed: {log}"));
      }
      let uniforms = reflect_uniforms(gl, program);
      Ok(ShaderProgram { program, uniforms, pipeline: true })
    }
  }

  pub fn is_pipeline(&self) -> bool {
    self.pipeline
  }

  pub(crate) fn delete(self, gl: &glow::Context) {
    unsafe { gl.delete_program(self.program) };
  }
}

/// Drop a use of a shared program, deleting the GL program when this was the
/// last one. The raster thread is the only place program Rcs live, so
/// try_unwrap succeeding is exactly "no registry entry and no target still
/// holds it".
pub fn release_program(gl: &glow::Context, program: Rc<ShaderProgram>) {
  if let Ok(program) = Rc::try_unwrap(program) {
    program.delete(gl);
  }
}

/// A pipeline-kind program paired with the draw state its targets share: the
/// pipeline state object of every modern GPU API, minus anything per-target
/// (a target brings its own size, vertex buffer, clear color, and FBO). One
/// pipeline can back many targets; targets hold it by Rc, like programs, so
/// either destruction order is safe.
pub struct RenderPipeline {
  program: Rc<ShaderProgram>,
  /// Registry id of the shared program this pipeline was created from; None
  /// for the fused create path, whose program is anonymous and dies with the
  /// pipeline.
  program_id: Option<u64>,
  desc: PipelineDesc,
}

impl RenderPipeline {
  /// Pair a linked program with draw state. Fragment programs never get here
  /// through the public surface (a fullscreen fragment pass has no draw
  /// state); the check is the raster-side backstop. On error the program Rc
  /// is handed back so the caller decides its fate.
  pub fn new(
    program: Rc<ShaderProgram>,
    program_id: Option<u64>,
    desc: PipelineDesc,
  ) -> Result<Self, (Rc<ShaderProgram>, String)> {
    if !program.is_pipeline() {
      return Err((program, "program is a fragment shader, not a pipeline".to_string()));
    }
    Ok(RenderPipeline { program, program_id, desc })
  }

  pub fn program_id(&self) -> Option<u64> {
    self.program_id
  }

  pub fn desc(&self) -> &PipelineDesc {
    &self.desc
  }
}

/// Drop a use of a shared render pipeline, releasing its program use when
/// this was the last one (which in turn deletes the GL program once nothing
/// else holds it). The pipeline Rcs all live on the raster thread, like
/// program Rcs.
pub fn release_pipeline(gl: &glow::Context, pipeline: Rc<RenderPipeline>) {
  if let Ok(pipeline) = Rc::try_unwrap(pipeline) {
    release_program(gl, pipeline.program);
  }
}

/// The per-target mesh half of a pipeline target: the pipeline it draws with
/// (which owns the draw state), plus everything bound to THIS target - the
/// VAO built against its concrete vertex buffer, that buffer's registry id,
/// the draw count, the private depth storage, and the clear color.
struct MeshState {
  pipeline: Rc<RenderPipeline>,
  /// Registry id of the shared pipeline this target was created from; None
  /// for the fused create path, whose pipeline is anonymous and dies with the
  /// target.
  pipeline_id: Option<u64>,
  vao: glow::VertexArray,
  /// Registry id of the interleaved vertex buffer (Context resolves writes to
  /// re-renders through this). 0 when the pipeline is attributeless.
  buffer_id: u64,
  draw_count: Cell<i32>,
  /// Present when the pipeline carries depth state; the renderbuffer stays
  /// private to the FBO (never adopted into Impeller).
  depth: Option<glow::Renderbuffer>,
  clear_color: [f32; 4],
}

/// An FBO-backed RGBA8 target texture rendered by a ShaderProgram: either a
/// fullscreen fragment pass (`mesh: None`) or a vertex+fragment pipeline
/// drawing an interleaved vertex buffer (`mesh: Some`). The target's GL name
/// is also adopted into Impeller (and held in the TextureRegistry); this
/// struct keeps the program/FBO so the same texture can be re-rendered with
/// new params. Like GpuTexture it never deletes the target name: Impeller
/// owns it once adopted, and deleting here would double-free.
pub struct ShaderTexture {
  /// The program rendering this target: a mesh target's own clone of its
  /// pipeline's program Rc (so render and reflection never branch on kind),
  /// a fragment target's whole identity.
  program: Rc<ShaderProgram>,
  fbo: glow::Framebuffer,
  target: glow::Texture,
  width: u32,
  height: u32,
  /// sampler2D uniform name -> source texture id. Resolved to a live GL texture
  /// at each render by the owner (which holds the texture registry), so an input
  /// whose contents or registry entry changed is picked up automatically.
  sampler_bindings: Vec<(String, u64)>,
  mesh: Option<MeshState>,
  /// The target's current uniform values, folded in by `merge_params` (its
  /// only writer) and read by every render. Held here so a re-render the app
  /// did not ask for directly - a vertex-buffer write, a draw-count change, a
  /// sampled input that changed - needs no params from the caller.
  last_params: RefCell<Vec<(String, ParamValue)>>,
  /// Declared sampling for this target's output (how OTHER passes and the
  /// display draw sample it; the target's own inputs carry their own states).
  /// Survives resize; set via `with_sampler` after construction.
  sampler: crate::texture::SamplerState,
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

/// Create the retained window-layer target: an exactly window-sized RGBA8
/// texture + FBO the frame resolves into while a window shader is active.
/// Exactly window-sized on purpose - the shader samples it with 0..1
/// coordinates, so aligned padding would leak into the sampling contract.
/// Completeness-checked here (unlike shader targets, nothing later would
/// catch it); restores the FBO binding it touches. The new layer starts
/// opaque black (the pass's clear color): a history layer (`uPrevious`) is
/// sampled one frame before anything resolves into it, and undefined storage
/// must not reach a program.
pub fn create_layer_target(
  gl: &glow::Context,
  width: u32,
  height: u32,
) -> Result<(glow::Texture, glow::Framebuffer), String> {
  unsafe {
    let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
    let result = create_target(gl, width, height).and_then(|(target, fbo)| {
      let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
      if status != glow::FRAMEBUFFER_COMPLETE {
        gl.delete_framebuffer(fbo);
        gl.delete_texture(target);
        return Err(format!("window layer framebuffer incomplete: {status:#x}"));
      }
      // The FBO is still bound from create_target. Scissor, color mask, and
      // clear color are Impeller-cached state on this shared context: force
      // a full clear and put all three back.
      let scissor = gl.is_enabled(glow::SCISSOR_TEST);
      let mut prev_mask = [0i32; 4];
      gl.get_parameter_i32_slice(glow::COLOR_WRITEMASK, &mut prev_mask);
      let mut prev_clear = [0f32; 4];
      gl.get_parameter_f32_slice(glow::COLOR_CLEAR_VALUE, &mut prev_clear);
      gl.disable(glow::SCISSOR_TEST);
      gl.color_mask(true, true, true, true);
      gl.clear_color(0.0, 0.0, 0.0, 1.0);
      gl.clear(glow::COLOR_BUFFER_BIT);
      gl.clear_color(prev_clear[0], prev_clear[1], prev_clear[2], prev_clear[3]);
      gl.color_mask(prev_mask[0] != 0, prev_mask[1] != 0, prev_mask[2] != 0, prev_mask[3] != 0);
      if scissor {
        gl.enable(glow::SCISSOR_TEST);
      }
      Ok((target, fbo))
    });
    gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
    result
  }
}

/// Reflect active uniforms so JS params can be matched to locations by name
/// and dispatched by the declared GL type.
fn reflect_uniforms(gl: &glow::Context, program: glow::Program) -> HashMap<String, (glow::UniformLocation, u32)> {
  let mut uniforms = HashMap::new();
  unsafe {
    let count = gl.get_active_uniforms(program);
    for i in 0..count {
      if let Some(u) = gl.get_active_uniform(program, i) {
        if let Some(loc) = gl.get_uniform_location(program, &u.name) {
          uniforms.insert(u.name, (loc, u.utype));
        }
      }
    }
  }
  uniforms
}

/// Set one uniform from a param value, dispatching on the reflected GL type.
/// A component count that does not match the declaration is skipped with a
/// warning (renders run on the raster thread after fire-and-forget commands,
/// so there is no JS call site left to throw at), as is a type outside the
/// supported set - notably samplers, which are bound via `textures`, not
/// params.
fn apply_uniform(gl: &glow::Context, name: &str, loc: &glow::UniformLocation, utype: u32, value: &ParamValue) {
  let c = value.components();
  unsafe {
    match (utype, c.len()) {
      (glow::FLOAT, 1) => gl.uniform_1_f32(Some(loc), c[0]),
      (glow::INT | glow::BOOL, 1) => gl.uniform_1_i32(Some(loc), c[0] as i32),
      (glow::FLOAT_VEC2, 2) => gl.uniform_2_f32(Some(loc), c[0], c[1]),
      (glow::FLOAT_VEC3, 3) => gl.uniform_3_f32(Some(loc), c[0], c[1], c[2]),
      (glow::FLOAT_VEC4, 4) => gl.uniform_4_f32(Some(loc), c[0], c[1], c[2], c[3]),
      (glow::FLOAT_MAT4, 16) => gl.uniform_matrix_4_f32_slice(Some(loc), false, c),
      _ => log::warn!("[shader] param '{name}' has {} component(s), which does not fit uniform type {utype:#x}; skipped", c.len()),
    }
  }
}

impl ShaderTexture {
  pub fn new(
    gl: &glow::Context,
    width: u32,
    height: u32,
    fragment_src: &str,
    sampler_bindings: Vec<(String, u64)>,
  ) -> Result<Self, String> {
    let program = Rc::new(ShaderProgram::new_fragment(gl, fragment_src)?);
    Self::from_fragment_program(gl, program, width, height, sampler_bindings).map_err(|(program, e)| {
      release_program(gl, program);
      e
    })
  }

  /// A fullscreen fragment target over an already-compiled program. On error
  /// the program Rc is handed back so the caller decides its fate.
  pub fn from_fragment_program(
    gl: &glow::Context,
    program: Rc<ShaderProgram>,
    width: u32,
    height: u32,
    sampler_bindings: Vec<(String, u64)>,
  ) -> Result<Self, (Rc<ShaderProgram>, String)> {
    if program.is_pipeline() {
      return Err((program, "program is a pipeline; the target needs a render pipeline".to_string()));
    }
    unsafe {
      let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
      let (target, fbo) = match create_target(gl, width, height) {
        Ok(pair) => pair,
        Err(e) => return Err((program, e)),
      };
      let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
      gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));

      if status != glow::FRAMEBUFFER_COMPLETE {
        gl.delete_framebuffer(fbo);
        gl.delete_texture(target);
        return Err((program, format!("shader framebuffer incomplete: {status:#x}")));
      }

      Ok(ShaderTexture {
        program,
        fbo,
        target,
        width,
        height,
        sampler_bindings,
        mesh: None,
        last_params: RefCell::new(Vec::new()),
        sampler: crate::texture::SamplerState::default(),
      })
    }
  }

  /// The fused create path: compile a vertex+fragment pair, wrap it in an
  /// anonymous pipeline, and build a target over it in one step.
  #[allow(clippy::too_many_arguments)]
  pub fn new_pipeline(
    gl: &glow::Context,
    width: u32,
    height: u32,
    vertex_src: &str,
    fragment_src: &str,
    sampler_bindings: Vec<(String, u64)>,
    desc: PipelineDesc,
    vbo: Option<glow::Buffer>,
    buffer_id: u64,
    draw_count: i32,
    clear_color: [f32; 4],
  ) -> Result<Self, String> {
    let program = Rc::new(ShaderProgram::new_pipeline(gl, vertex_src, fragment_src)?);
    let pipeline = match RenderPipeline::new(program, None, desc) {
      Ok(p) => Rc::new(p),
      Err((program, e)) => {
        release_program(gl, program);
        return Err(e);
      }
    };
    Self::from_pipeline(gl, pipeline, None, width, height, sampler_bindings, vbo, buffer_id, draw_count, clear_color)
      .map_err(|(pipeline, e)| {
        release_pipeline(gl, pipeline);
        e
      })
  }

  /// A target over a render pipeline: the pipeline's vertex layout is bound
  /// to this target's concrete buffer in a fresh VAO (attribute locations are
  /// looked up by name, so an attribute the shader does not use is skipped -
  /// its bytes still occupy the stride), and depth state gets a private
  /// renderbuffer. On error the pipeline Rc is handed back so the caller
  /// decides its fate (a fused create releases it, a shared pipeline stays
  /// registered).
  #[allow(clippy::too_many_arguments)]
  pub fn from_pipeline(
    gl: &glow::Context,
    pipeline: Rc<RenderPipeline>,
    pipeline_id: Option<u64>,
    width: u32,
    height: u32,
    sampler_bindings: Vec<(String, u64)>,
    vbo: Option<glow::Buffer>,
    buffer_id: u64,
    draw_count: i32,
    clear_color: [f32; 4],
  ) -> Result<Self, (Rc<RenderPipeline>, String)> {
    if !pipeline.desc.attributes.is_empty() && vbo.is_none() {
      return Err((pipeline, "pipeline declares attributes but no vertex buffer".to_string()));
    }
    let program = pipeline.program.clone();
    let attributes = &pipeline.desc.attributes;
    let depth = pipeline.desc.depth.is_some();

    unsafe {
      let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
      let prev_rb = gl.get_parameter_i32(glow::RENDERBUFFER_BINDING);
      let prev_vao = gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING);
      let prev_ab = gl.get_parameter_i32(glow::ARRAY_BUFFER_BINDING);

      // Cleanup helper for every early exit below; the pipeline itself
      // travels back in the Err.
      let fail = |gl: &glow::Context,
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
      };

      let (target, fbo) = match create_target(gl, width, height) {
        Ok(pair) => pair,
        Err(e) => {
          gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
          return Err((pipeline, e));
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
            fail(gl, Some(target), Some(fbo), None, None);
            return Err((pipeline, format!("glGenRenderbuffers failed: {e}")));
          }
        }
      } else {
        None
      };

      let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
      gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
      if status != glow::FRAMEBUFFER_COMPLETE {
        fail(gl, Some(target), Some(fbo), depth_rb, None);
        return Err((pipeline, format!("pipeline framebuffer incomplete: {status:#x}")));
      }

      // Record the interleaved vertex layout in a VAO. The VAO captures the
      // buffer binding per attribute, so rendering only rebinds the VAO.
      let vao = match gl.create_vertex_array() {
        Ok(vao) => vao,
        Err(e) => {
          fail(gl, Some(target), Some(fbo), depth_rb, None);
          return Err((pipeline, format!("glGenVertexArrays failed: {e}")));
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
          if let Some(loc) = gl.get_attrib_location(program.program, name) {
            gl.enable_vertex_attrib_array(loc);
            gl.vertex_attrib_pointer_f32(loc, fmt.components(), glow::FLOAT, false, stride, offset);
          }
          offset += fmt.components() * 4;
        }
      }
      gl.bind_vertex_array(prev_vertex_array(prev_vao));
      gl.bind_buffer(glow::ARRAY_BUFFER, prev_buffer(prev_ab));

      Ok(ShaderTexture {
        program,
        fbo,
        target,
        width,
        height,
        sampler_bindings,
        mesh: Some(MeshState {
          pipeline,
          pipeline_id,
          vao,
          buffer_id,
          draw_count: Cell::new(draw_count),
          depth: depth_rb,
          clear_color,
        }),
        last_params: RefCell::new(Vec::new()),
        sampler: crate::texture::SamplerState::default(),
      })
    }
  }

  /// Set the declared sampling for this target's output (builder-style, right
  /// after construction).
  pub fn with_sampler(mut self, sampler: crate::texture::SamplerState) -> Self {
    self.sampler = sampler;
    self
  }

  pub fn sampler(&self) -> crate::texture::SamplerState {
    self.sampler
  }

  pub fn gl_texture(&self) -> glow::Texture {
    self.target
  }

  /// Registry id of the shared program behind this target's pipeline; None
  /// for fragment targets and for the fused create path, whose program is
  /// anonymous.
  pub fn program_id(&self) -> Option<u64> {
    self.mesh.as_ref().and_then(|m| m.pipeline.program_id)
  }

  /// Registry id of the shared pipeline this target was created from; None
  /// for fragment targets and the fused create path.
  pub fn pipeline_id(&self) -> Option<u64> {
    self.mesh.as_ref().and_then(|m| m.pipeline_id)
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

  /// The pipeline's topology as the string `Topology::parse` accepts; None on
  /// a fragment-only shader.
  pub fn topology_name(&self) -> Option<&'static str> {
    self.mesh.as_ref().map(|m| m.pipeline.desc.topology.name())
  }

  /// The declared interleaved attribute layout; empty for fragment-only
  /// shaders and attributeless pipelines.
  pub fn attributes(&self) -> &[(String, AttrFormat)] {
    self.mesh.as_ref().map(|m| m.pipeline.desc.attributes.as_slice()).unwrap_or(&[])
  }

  /// Whether the pipeline renders with a depth buffer attached.
  pub fn has_depth(&self) -> bool {
    self.mesh.as_ref().is_some_and(|m| m.depth.is_some())
  }

  /// Whether the pipeline's draw writes depth; None on a fragment-only shader.
  pub fn depth_write(&self) -> Option<bool> {
    self.mesh.as_ref().map(|m| m.pipeline.desc.depth.map_or(true, |d| d.write))
  }

  /// The pipeline's blend mode as the string `parse_blend` accepts; None on a
  /// fragment-only shader.
  pub fn blend_name(&self) -> Option<&'static str> {
    self.mesh.as_ref().map(|m| blend_name(m.pipeline.desc.blend))
  }

  /// Set the number of vertices the next render draws. Errors on a
  /// fragment-only shader (its fullscreen triangle is fixed).
  pub fn set_draw_count(&self, count: i32) -> Result<(), String> {
    let mesh = self.mesh.as_ref().ok_or_else(|| "not a pipeline texture".to_string())?;
    mesh.draw_count.set(count);
    Ok(())
  }

  /// A copy of the target's current uniform values, for resource
  /// introspection (`render` reads the record directly).
  pub fn last_params(&self) -> Vec<(String, ParamValue)> {
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

  /// Release GL resources owned by this target (FBO, and for pipelines the
  /// VAO and depth renderbuffer), and drop its uses of the pipeline and
  /// program - which delete the underlying GL program only when nothing else
  /// (a registry, another target) still holds them. The target texture is NOT
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
    }
    release_program(gl, self.program);
    if let Some(mesh) = self.mesh {
      release_pipeline(gl, mesh.pipeline);
    }
  }

  /// The sampler2D inputs this shader declared, as (uniform name, source texture
  /// id). The owner resolves each id to a live GL texture before rendering.
  pub fn sampler_bindings(&self) -> &[(String, u64)] {
    &self.sampler_bindings
  }

  /// Rebind sampler2D inputs by uniform name; bindings not named keep their
  /// current source, and a name without an existing binding is added (a
  /// declared sampler left unbound at creation). Every name is validated
  /// against the program's active uniforms before anything changes, so a
  /// failed call leaves all bindings intact. The caller re-renders afterwards.
  pub fn set_sampler_bindings(&mut self, updates: &[(String, u64)]) -> Result<(), String> {
    for (name, _) in updates {
      if !self.program.uniforms.contains_key(name) {
        return Err(format!("no active uniform named '{name}'"));
      }
    }
    for (name, src_id) in updates {
      match self.sampler_bindings.iter_mut().find(|(n, _)| n == name) {
        Some(binding) => binding.1 = *src_id,
        None => self.sampler_bindings.push((name.clone(), *src_id)),
      }
    }
    Ok(())
  }

  /// Fold a params update into the current record by name (new names
  /// append, existing names overwrite). Uniforms are program state in GL, so
  /// rendering once with the merged record is equivalent to rendering after
  /// each partial params list; the owner defers that render to its dirty
  /// flush.
  pub fn merge_params(&self, params: &[(String, ParamValue)]) {
    let mut last = self.last_params.borrow_mut();
    for (name, value) in params {
      match last.iter_mut().find(|(n, _)| n == name) {
        Some(entry) => entry.1 = value.clone(),
        None => last.push((name.clone(), value.clone())),
      }
    }
  }

  /// Render the shader into its target texture with its current params (see
  /// `merge_params`, the only writer) and the given resolved sampler inputs
  /// (uniform name -> source GL texture, in the order `sampler_bindings`
  /// declared them). See `run_pass` for the GL state contract;
  /// Context::submit's per-frame fence orders the work ahead of the render
  /// thread sampling the target from its shared GL context, so no glFinish is
  /// needed here.
  pub fn render(&self, gl: &glow::Context, textures: &[PassInput]) {
    let params = self.last_params.borrow();
    let draw = match &self.mesh {
      None => PassDraw::Fullscreen { vertex_count: 3, clear: None },
      Some(mesh) => PassDraw::Mesh(mesh),
    };
    run_pass(gl, &self.program, Some(self.fbo), self.width, self.height, &params, textures, draw);
  }
}

/// What a `run_pass` draw executes once the program and uniforms are bound.
enum PassDraw<'a> {
  /// Attributeless triangles (vertex fetch via gl_VertexID). `clear` first
  /// when the geometry is not guaranteed to cover the target.
  Fullscreen { vertex_count: i32, clear: Option<[f32; 4]> },
  /// A pipeline target's VAO-backed mesh draw, with its clear and depth state.
  Mesh(&'a MeshState),
}

/// Draw `program` once into the default framebuffer (FBO 0) at the window's
/// pixel size: the window shader pass. Attributeless, `vertex_count` vertices
/// as triangles (3 = the covering triangle). FBO 0 is cleared to opaque black
/// first, so a program whose geometry does not cover the window still
/// presents a defined frame. `textures` carries the resolved sampler inputs
/// (the layer as `uSource` first); uniforms fill by name as in every pass.
pub fn render_program_to_window(
  gl: &glow::Context,
  program: &ShaderProgram,
  width: u32,
  height: u32,
  params: &[(String, ParamValue)],
  textures: &[PassInput],
  vertex_count: i32,
) {
  let draw = PassDraw::Fullscreen { vertex_count, clear: Some([0.0, 0.0, 0.0, 1.0]) };
  run_pass(gl, program, None, width, height, params, textures, draw);
}

/// Run one fullscreen draw of `program` into `fbo` (None = the default
/// framebuffer), no clear: the covering triangle writes every pixel. The
/// in-tile MSAA resolve consumes its resolved texture through this instead of
/// a blit (see `gl::draw_and_resolve` for why a blit is not an option there).
pub fn render_program_to_fbo(
  gl: &glow::Context,
  program: &ShaderProgram,
  fbo: Option<glow::Framebuffer>,
  width: u32,
  height: u32,
  textures: &[PassInput],
) {
  let draw = PassDraw::Fullscreen { vertex_count: 3, clear: None };
  run_pass(gl, program, fbo, width, height, &[], textures, draw);
}

/// Run one draw of `program` into `fbo` (None = the default framebuffer) at
/// viewport `width` x `height`: bind, fill `iResolution` and the float params
/// by uniform name, bind the resolved sampler inputs, neutralize every piece
/// of fixed-function state that could clip or blend the output away, draw,
/// and restore all of it. The save/restore set must stay exhaustive: Impeller
/// runs full render passes on this same context (the process's only one) and
/// both leaves state behind and caches what it set (see the comments below).
fn run_pass(
  gl: &glow::Context,
  program: &ShaderProgram,
  fbo: Option<glow::Framebuffer>,
  width: u32,
  height: u32,
  params: &[(String, ParamValue)],
  textures: &[PassInput],
  draw: PassDraw,
) {
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

    gl.bind_framebuffer(glow::FRAMEBUFFER, fbo);
    gl.viewport(0, 0, width as i32, height as i32);
    gl.use_program(Some(program.program));

    // The preambles declare iResolution as vec2; a raw source may declare it
    // vec3 (the Shadertoy contract), which gets the size with z = 1.
    match program.uniforms.get("iResolution") {
      Some((loc, glow::FLOAT_VEC2)) => gl.uniform_2_f32(Some(loc), width as f32, height as f32),
      Some((loc, glow::FLOAT_VEC3)) => gl.uniform_3_f32(Some(loc), width as f32, height as f32, 1.0),
      _ => {}
    }
    for (name, value) in params {
      if let Some((loc, utype)) = program.uniforms.get(name) {
        apply_uniform(gl, name, loc, *utype, value);
      }
    }

    // Bind each resolved sampler input to its own texture unit, bind the
    // input's sampler object on that unit (its declared filter/wrap; the
    // bound sampler overrides texture-object parameters, which Impeller
    // rewrites at will on textures it draws), and point the sampler uniform
    // at the unit. Save the prior texture and sampler binding per unit so
    // Impeller (which assumes its own units) is not left looking at our
    // textures or sampling through our state.
    let mut prev_unit_bindings: Vec<(u32, i32, i32)> = Vec::new();
    for (unit, (name, tex, sampler)) in textures.iter().enumerate() {
      let Some((loc, _)) = program.uniforms.get(name) else { continue };
      let unit = unit as u32;
      gl.active_texture(glow::TEXTURE0 + unit);
      prev_unit_bindings.push((
        unit,
        gl.get_parameter_i32(glow::TEXTURE_BINDING_2D),
        gl.get_parameter_i32(glow::SAMPLER_BINDING),
      ));
      gl.bind_texture(glow::TEXTURE_2D, Some(*tex));
      gl.bind_sampler(unit, *sampler);
      gl.uniform_1_i32(Some(loc), unit as i32);
    }

    // Fixed-function state that could clip or blend the output away is off
    // for both paths; the mesh path opts depth testing back in below. This
    // set must be exhaustive: Impeller runs full render passes on this same
    // context (the process's only one) and may have left any of the
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

    match draw {
      PassDraw::Fullscreen { vertex_count, clear } => {
        // The covering triangle writes opaque coverage over the whole target,
        // so a clear only happens when the caller asked for one (geometry not
        // guaranteed to cover). Clear color is Impeller-cached state: save
        // and restore it around the clear.
        if let Some([r, g, b, a]) = clear {
          let mut prev_clear = [0f32; 4];
          gl.get_parameter_f32_slice(glow::COLOR_CLEAR_VALUE, &mut prev_clear);
          gl.clear_color(r, g, b, a);
          gl.clear(glow::COLOR_BUFFER_BIT);
          gl.clear_color(prev_clear[0], prev_clear[1], prev_clear[2], prev_clear[3]);
        }
        gl.draw_arrays(glow::TRIANGLES, 0, vertex_count);
      }
      PassDraw::Mesh(mesh) => {
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

        let desc = &mesh.pipeline.desc;
        let [r, g, b, a] = mesh.clear_color;
        gl.clear_color(r, g, b, a);
        if let Some(depth) = desc.depth {
          gl.enable(glow::DEPTH_TEST);
          // The clear always writes depth (glClear honors the write mask);
          // the draw's mask is the pipeline's depthWrite option.
          gl.depth_mask(true);
          gl.depth_func(glow::LESS);
          // Impeller's clip-culling passes set their own depth-clear value
          // (0.0) on this context; clearing with that inverts the test and
          // silently discards every fragment. Always clear to the far plane.
          gl.clear_depth_f32(1.0);
          gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
          gl.depth_mask(depth.write);
        } else {
          gl.clear(glow::COLOR_BUFFER_BIT);
        }
        // Blend func is Impeller-cached state like the rest: save and restore
        // around the draw, and re-disable BLEND after it (the outer restore
        // only re-enables). Off the blended path nothing is touched.
        let prev_blend_func = desc.blend.map(|_| {
          [
            gl.get_parameter_i32(glow::BLEND_SRC_RGB) as u32,
            gl.get_parameter_i32(glow::BLEND_DST_RGB) as u32,
            gl.get_parameter_i32(glow::BLEND_SRC_ALPHA) as u32,
            gl.get_parameter_i32(glow::BLEND_DST_ALPHA) as u32,
          ]
        });
        match desc.blend {
          Some(BlendMode::Add) => {
            gl.enable(glow::BLEND);
            gl.blend_func(glow::ONE, glow::ONE);
          }
          None => {}
        }
        gl.bind_vertex_array(Some(mesh.vao));
        gl.draw_arrays(desc.topology.gl(), 0, mesh.draw_count.get());

        if let Some([src_rgb, dst_rgb, src_alpha, dst_alpha]) = prev_blend_func {
          gl.disable(glow::BLEND);
          gl.blend_func_separate(src_rgb, dst_rgb, src_alpha, dst_alpha);
        }
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
    for (unit, prev, prev_smp) in prev_unit_bindings {
      gl.active_texture(glow::TEXTURE0 + unit);
      gl.bind_texture(glow::TEXTURE_2D, prev_texture(prev));
      gl.bind_sampler(unit, prev_sampler(prev_smp));
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
      log::warn!("[shader] GL error {err:#x} after shader pass");
    }
  }
}
