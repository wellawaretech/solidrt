//! Stage compilation and program linking: the preamble injection contract,
//! `ShaderProgram` (a linked GL program with its reflected uniforms), and
//! `RenderPipeline` (a pipeline-kind program paired with shared draw state).

use glow::HasContext;
use std::collections::HashMap;
use std::rc::Rc;

use super::vocab::{PipelineDesc, ShaderStage};

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
// the body can reference vUV/fragColor/iResolution directly. A source that
// provides its own #version is treated as complete and gets no preamble.
// The preamble declares exactly what the runtime provides: anything the app
// drives (a time uniform, say) is the app's own declaration, like any other
// uniform, so forgetting to drive it is a compile error rather than a value
// silently stuck at 0.
const FRAGMENT_PREAMBLE: &str = r"#version 300 es
precision highp float;
in vec2 vUV;
out vec4 fragColor;
uniform vec2 iResolution;
";

// Pipeline preambles. A pipeline's varyings are the user's own (the vertex
// shader declares `out`s matching the fragment's `in`s), so the pipeline
// fragment preamble declares no vUV. Attributes (`in` at vertex stage) are also
// the user's own; their locations are resolved by name against the declared
// attribute list. As above, a source with its own #version gets no preamble.
const PIPELINE_VERTEX_PREAMBLE: &str = r"#version 300 es
precision highp float;
uniform vec2 iResolution;
";

const PIPELINE_FRAGMENT_PREAMBLE: &str = r"#version 300 es
precision highp float;
out vec4 fragColor;
uniform vec2 iResolution;
";

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
/// `#version 300 es`, highp float precision, the `iResolution` uniform, and
/// for a fragment stage `out vec4 fragColor`. A source carrying its own
/// `#version` must not also ask for the header.
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

/// A compiled and linked GL program with its reflected uniform locations: the
/// compile half of what used to be fused into ShaderTexture. One program can
/// back many targets (the ordinary GL shape); targets hold it by Rc, so it
/// stays alive - and its GL name stays valid - until the last user is gone,
/// whichever destruction order the app picks. Owned by the raster thread:
/// registered programs live in its registry, a fused createShaderTexture's
/// program is held only by its target.
pub struct ShaderProgram {
  pub(super) program: glow::Program,
  /// Active uniform name -> (location, GL type), reflected once at link time.
  /// JS params are matched to locations by name and dispatched by the
  /// reflected type; iResolution is filled from the target size at render.
  pub(super) uniforms: HashMap<String, (glow::UniformLocation, u32)>,
  /// Vertex+fragment pipeline (own vertex stage over a buffer) vs fullscreen
  /// fragment pass. Decides which target shape the program can back.
  pipeline: bool,
  /// Free-form debug name from the create (WebGPU's label), for the resource
  /// inventory. None on the fused paths, whose anonymous program is covered
  /// by its target's label.
  label: Option<String>,
}

impl ShaderProgram {
  /// Compile a fullscreen fragment pass: the fixed covering-triangle vertex
  /// stage plus `fragment_src` (preamble-injected unless it brings its own
  /// #version).
  pub fn new_fragment(gl: &glow::Context, fragment_src: &str) -> Result<Self, String> {
    let fragment_full = with_preamble(fragment_src, FRAGMENT_PREAMBLE);
    let program = link_program(gl, VERTEX_SRC, &fragment_full)?;
    let uniforms = reflect_uniforms(gl, program);
    Ok(ShaderProgram { program, uniforms, pipeline: false, label: None })
  }

  /// Compile a vertex+fragment pipeline program. Attribute locations are
  /// resolved by name later, when a target's VAO is built against a concrete
  /// buffer layout.
  pub fn new_pipeline(gl: &glow::Context, vertex_src: &str, fragment_src: &str) -> Result<Self, String> {
    let vertex_full = with_preamble(vertex_src, PIPELINE_VERTEX_PREAMBLE);
    let fragment_full = with_preamble(fragment_src, PIPELINE_FRAGMENT_PREAMBLE);
    let program = link_program(gl, &vertex_full, &fragment_full)?;
    let uniforms = reflect_uniforms(gl, program);
    Ok(ShaderProgram { program, uniforms, pipeline: true, label: None })
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
      Ok(ShaderProgram { program, uniforms, pipeline: true, label: None })
    }
  }

  /// Set the debug label (builder-style, right after construction).
  pub fn with_label(mut self, label: Option<String>) -> Self {
    self.label = label;
    self
  }

  pub fn label(&self) -> Option<&str> {
    self.label.as_deref()
  }

  pub fn is_pipeline(&self) -> bool {
    self.pipeline
  }

  /// The active uniforms as a plain-data table (name -> kind), for the
  /// UI-side mirror and call-site validation (see `vocab::UniformTable`).
  pub fn uniform_table(&self) -> super::vocab::UniformTable {
    self.uniforms.iter().map(|(name, (_, utype))| (name.clone(), super::vocab::UniformKind::from_gl(*utype))).collect()
  }

  pub(crate) fn delete(self, gl: &glow::Context) {
    unsafe { gl.delete_program(self.program) };
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
  pub(super) program: Rc<ShaderProgram>,
  /// Registry id of the shared program this pipeline was created from; None
  /// for the fused create path, whose program is anonymous and dies with the
  /// pipeline.
  pub(super) program_id: Option<u64>,
  pub(super) desc: PipelineDesc,
  /// Free-form debug name from the create (WebGPU's label), for the resource
  /// inventory. None on the fused path.
  label: Option<String>,
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
    Ok(RenderPipeline { program, program_id, desc, label: None })
  }

  /// Set the debug label (builder-style, right after construction).
  pub fn with_label(mut self, label: Option<String>) -> Self {
    self.label = label;
    self
  }

  pub fn label(&self) -> Option<&str> {
    self.label.as_deref()
  }

  pub fn program_id(&self) -> Option<u64> {
    self.program_id
  }

  pub fn desc(&self) -> &PipelineDesc {
    &self.desc
  }

  /// The shared program's active uniforms (see `ShaderProgram::uniform_table`).
  pub fn uniform_table(&self) -> super::vocab::UniformTable {
    self.program.uniform_table()
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
