//! Pass execution: one draw of a program into a target or the window, with
//! the exhaustive GL save/restore that keeps Impeller (which shares this
//! context and caches state) working. Every change here needs that
//! save/restore set reviewed.

use glow::HasContext;

use super::program::ShaderProgram;
use super::target::MeshState;
use super::vocab::{BlendMode, ParamValue};
use super::{prev_framebuffer, prev_program, prev_sampler, prev_texture, prev_vertex_array};

/// A resolved sampler input for a pass: uniform name, source GL texture, and
/// the sampler object carrying the source's declared filter/wrap (None for
/// internal textures - window layers, the MSAA resolve - which keep their
/// texture-object state).
pub type PassInput = (String, glow::Texture, Option<glow::Sampler>);

/// What a `run_pass` draw executes once the program and uniforms are bound.
pub(super) enum PassDraw<'a> {
  /// Attributeless triangles (vertex fetch via gl_VertexID). `clear` first
  /// when the geometry is not guaranteed to cover the target.
  Fullscreen { vertex_count: i32, clear: Option<[f32; 4]> },
  /// A pipeline target's VAO-backed mesh draw, with its clear and depth state.
  Mesh(&'a MeshState),
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
#[allow(clippy::too_many_arguments)]
pub(super) fn run_pass(
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
