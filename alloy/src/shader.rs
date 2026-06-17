use glow::HasContext;
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

fn prev_texture(name: i32) -> Option<glow::NativeTexture> {
  NonZeroU32::new(name as u32).map(glow::NativeTexture)
}
fn prev_framebuffer(name: i32) -> Option<glow::NativeFramebuffer> {
  NonZeroU32::new(name as u32).map(glow::NativeFramebuffer)
}
fn prev_program(name: i32) -> Option<glow::NativeProgram> {
  NonZeroU32::new(name as u32).map(glow::NativeProgram)
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

fn link_program(gl: &glow::Context, fragment_src: &str) -> Result<glow::Program, String> {
  let fragment_full = if fragment_src.trim_start().starts_with("#version") {
    fragment_src.to_string()
  } else {
    format!("{FRAGMENT_PREAMBLE}{fragment_src}")
  };
  unsafe {
    let vs = compile_shader(gl, glow::VERTEX_SHADER, VERTEX_SRC)?;
    let fs = match compile_shader(gl, glow::FRAGMENT_SHADER, &fragment_full) {
      Ok(fs) => fs,
      Err(e) => {
        gl.delete_shader(vs);
        return Err(e);
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

/// A compiled fragment-shader program with its own FBO-backed RGBA8 target
/// texture. The target's GL name is also adopted into Impeller (and held in the
/// TextureRegistry); this struct keeps the program/FBO so the same texture can
/// be re-rendered with new params. Like GpuTexture it never deletes the target
/// name: Impeller owns it once adopted, and deleting here would double-free.
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
}

impl ShaderTexture {
  pub fn new(
    gl: &glow::Context,
    width: u32,
    height: u32,
    fragment_src: &str,
    sampler_bindings: Vec<(String, u64)>,
  ) -> Result<Self, String> {
    let program = link_program(gl, fragment_src)?;

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
      // No mips exist: the default MIN_FILTER references mipmaps, which would
      // make the texture sampling-incomplete (reads as black) when Impeller
      // samples it.
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
      gl.bind_texture(glow::TEXTURE_2D, prev_texture(prev_tex));

      let fbo = gl.create_framebuffer().map_err(|e| format!("glGenFramebuffers failed: {e}"))?;
      gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
      gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(target), 0);
      let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
      gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));

      if status != glow::FRAMEBUFFER_COMPLETE {
        gl.delete_framebuffer(fbo);
        gl.delete_texture(target);
        gl.delete_program(program);
        return Err(format!("shader framebuffer incomplete: {status:#x}"));
      }

      // Reflect active uniforms so JS params can be matched to locations by name.
      let mut uniforms = HashMap::new();
      let count = gl.get_active_uniforms(program);
      for i in 0..count {
        if let Some(u) = gl.get_active_uniform(program, i) {
          if let Some(loc) = gl.get_uniform_location(program, &u.name) {
            uniforms.insert(u.name, loc);
          }
        }
      }

      Ok(ShaderTexture { program, fbo, target, width, height, uniforms, sampler_bindings })
    }
  }

  pub fn gl_texture(&self) -> glow::Texture {
    self.target
  }

  /// Release GL resources owned by this shader (program and FBO). The target
  /// texture is NOT deleted here: Impeller owns it via the adopted Texture handle
  /// in the TextureRegistry, and that handle is responsible for deletion.
  pub fn destroy(self, gl: &glow::Context) {
    unsafe {
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
  /// enables it touches so Impeller's cached state stays valid, then glFinish so
  /// the texture is complete before the render thread samples it from a separate
  /// shared GL context.
  pub fn render(&self, gl: &glow::Context, params: &[(String, f32)], textures: &[(String, glow::Texture)]) {
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

      // The fragment writes opaque coverage over the whole target; turn off the
      // fixed-function state that could clip or blend it away.
      gl.disable(glow::BLEND);
      gl.disable(glow::DEPTH_TEST);
      gl.disable(glow::SCISSOR_TEST);
      gl.disable(glow::CULL_FACE);
      gl.draw_arrays(glow::TRIANGLES, 0, 3);

      // Restore prior GL state for Impeller.
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

      // glFlush only submits; the render thread's shared context sees defined
      // contents only after the producing context's writes actually finish.
      gl.finish();
    }
  }
}