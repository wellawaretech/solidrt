//! Pass execution: one render into a target or the window - a fullscreen
//! fragment draw, or clear + an ordered list of mesh draws - with the
//! exhaustive GL save/restore that keeps Impeller (which shares this context
//! and caches state) working. Every change here needs that save/restore set
//! reviewed.

use glow::HasContext;

use super::program::ShaderProgram;
use super::vocab::{BlendMode, DrawRange, IndexFormat, ParamValue, PipelineDesc, UniformKind, UniformSlot};
use super::{prev_framebuffer, prev_program, prev_sampler, prev_texture, prev_vertex_array};

/// A resolved sampler input for a pass: uniform name, source GL texture, and
/// the sampler object carrying the source's declared filter/wrap (None for
/// internal textures - window layers, the MSAA resolve - which keep their
/// texture-object state).
pub type PassInput = (String, glow::Texture, Option<glow::Sampler>);

/// One resolved draw of a mesh pass: the entry's program and draw state plus
/// its resolved sampler inputs, ready for the raster thread to execute in
/// list order.
pub(super) struct ResolvedDraw<'a> {
  pub(super) program: &'a ShaderProgram,
  pub(super) desc: &'a PipelineDesc,
  pub(super) vao: glow::VertexArray,
  pub(super) range: DrawRange,
  /// Present = draw indexed (`range` counts indices): the index buffer is
  /// already captured in `vao`, so the format is all the draw call needs.
  pub(super) index: Option<IndexFormat>,
  pub(super) params: &'a [(String, ParamValue)],
  pub(super) inputs: Vec<PassInput>,
}

/// One group of a mesh pass: an ordered run of draws sharing a viewport, a
/// clear and the shared params. A pass has one group when a target renders
/// its own entries over its whole storage (`rect` None: the pass viewport,
/// no clear of its own - the pass-level clear covers it) and one more per
/// sub-target drawn into a rectangle of that storage (`rect` Some, in
/// viewport pixels - which for a target is image space, row 0 = top:
/// viewport and scissor are set to it, and the rectangle is wiped before
/// the group's draws - `clear` the color when given, `clear_depth` the
/// depth to the far plane). The wipe is a covering-triangle DRAW through
/// the pass's `tile_clear` program, never a scissored glClear: on a tiled
/// GPU a glClear mid-pass ends the pass and restarts it with a full
/// load/store of the whole surface, which cost more than the passes the
/// tiles were merged to save (measured on an Adreno 610). `iResolution`
/// is the group's size.
pub(super) struct DrawGroup<'a> {
  pub(super) rect: Option<(i32, i32, u32, u32)>,
  pub(super) clear: Option<[f32; 4]>,
  pub(super) clear_depth: bool,
  pub(super) shared: &'a [(String, ParamValue)],
  pub(super) draws: &'a [ResolvedDraw<'a>],
}

/// The tile-clear program's fragment stage (over the fullscreen vertex
/// stage `ShaderProgram::new_fragment` supplies): the clear color, and the
/// far plane into depth. Compiled once per raster thread by the owner.
pub const TILE_CLEAR_FRAGMENT: &str = "uniform vec4 uColor;\nvoid main() { fragColor = uColor; gl_FragDepth = 1.0; }";

/// What a `run_pass` invocation executes.
pub(super) enum PassDraw<'a> {
  /// Attributeless triangles (vertex fetch via gl_VertexID): the fragment
  /// targets, the window shader, node shader passes, the copy program.
  /// `clear` first when the geometry is not guaranteed to cover the target.
  /// `blend` composites the draw over the target's existing contents with
  /// premultiplied alpha (the overlay composite) instead of overwriting.
  Fullscreen {
    program: &'a ShaderProgram,
    params: &'a [(String, ParamValue)],
    textures: &'a [PassInput],
    vertex_count: i32,
    clear: Option<[f32; 4]>,
    blend: bool,
  },
  /// A mesh target's pass: the pass-level clear once (`clear` = the color,
  /// absent under loadOp "load" or on a partial render; `clear_depth` = the
  /// depth buffer too), then the groups in order, each group's draw entries
  /// in list order with their own program, uniforms, inputs, VAO, and
  /// pipeline blend/depth state. `depth` says the target owns depth storage
  /// (entries may test against it). Each group's `shared` carries the
  /// target-level params every entry of the group gets, applied before the
  /// entry's own so an entry naming the same uniform overrides the shared
  /// value. `tile_clear` is the program the groups' rectangle wipes draw
  /// with (see `DrawGroup`); None skips those wipes (the owner failed to
  /// compile it, already logged).
  Draws {
    clear: Option<[f32; 4]>,
    clear_depth: bool,
    depth: bool,
    groups: &'a [DrawGroup<'a>],
    tile_clear: Option<&'a ShaderProgram>,
  },
}

/// Set one uniform from a param value, dispatching on the reflected slot
/// (element kind + declared array size) through the `v` slice forms, which
/// serve a single value and a flat array with the same call. A component
/// count that does not match the declaration is skipped with a warning
/// (renders run on the raster thread after fire-and-forget commands, so
/// there is no JS call site left to throw at), as is a kind outside the
/// supported set - notably samplers, which are bound via `textures`, not
/// params.
fn apply_uniform(gl: &glow::Context, name: &str, loc: &glow::UniformLocation, slot: UniformSlot, value: &ParamValue) {
  let c = value.components();
  if slot.components() != Some(c.len()) {
    let declared = match slot.kind {
      UniformKind::Other(utype) => format!("type {utype:#x}"),
      _ => slot.glsl_name(),
    };
    log::warn!("[shader] param '{name}' has {} component(s), which does not fit uniform {declared}; skipped", c.len());
    return;
  }
  unsafe {
    match slot.kind {
      UniformKind::Float => gl.uniform_1_f32_slice(Some(loc), c),
      UniformKind::Int | UniformKind::Bool => {
        let ints: Vec<i32> = c.iter().map(|v| *v as i32).collect();
        gl.uniform_1_i32_slice(Some(loc), &ints);
      }
      UniformKind::Vec2 => gl.uniform_2_f32_slice(Some(loc), c),
      UniformKind::Vec3 => gl.uniform_3_f32_slice(Some(loc), c),
      UniformKind::Vec4 => gl.uniform_4_f32_slice(Some(loc), c),
      UniformKind::Mat4 => gl.uniform_matrix_4_f32_slice(Some(loc), false, c),
      // No component count, so the guard above already returned.
      UniformKind::Sampler2D | UniformKind::Inactive | UniformKind::Other(_) => {}
    }
  }
}

/// Apply params to the bound program by uniform name; a name the program
/// does not declare is skipped (how a shared param covers only the entries
/// whose program declares it).
fn apply_params(gl: &glow::Context, program: &ShaderProgram, params: &[(String, ParamValue)]) {
  for (name, value) in params {
    if let Some((loc, slot)) = program.uniform(name) {
      apply_uniform(gl, name, loc, *slot, value);
    }
  }
}

/// Bind `program` and fill `iResolution` plus the given params by uniform
/// name. The preambles declare iResolution as vec2; a raw source may declare
/// it vec3 (a common convention in ported shaders), which gets the size with
/// z = 1.
fn apply_program(
  gl: &glow::Context,
  program: &ShaderProgram,
  width: u32,
  height: u32,
  params: &[(String, ParamValue)],
) {
  unsafe {
    gl.use_program(Some(program.program));
    match program.uniform("iResolution") {
      Some((loc, UniformSlot { kind: UniformKind::Vec2, .. })) => {
        gl.uniform_2_f32(Some(loc), width as f32, height as f32)
      }
      Some((loc, UniformSlot { kind: UniformKind::Vec3, .. })) => {
        gl.uniform_3_f32(Some(loc), width as f32, height as f32, 1.0)
      }
      _ => {}
    }
  }
  apply_params(gl, program, params);
}

/// Bind each resolved sampler input to its own texture unit, bind the input's
/// sampler object on that unit (its declared filter/wrap; the bound sampler
/// overrides texture-object parameters, which Impeller rewrites at will on
/// textures it draws), and point the sampler uniform at the unit. The prior
/// texture and sampler binding of each unit is saved into `saved` the FIRST
/// time the pass touches it - draws reuse units from 0 up, so one save per
/// unit covers the whole pass - and the caller restores them all at the end,
/// so Impeller (which assumes its own units) is not left looking at our
/// textures or sampling through our state.
/// Unit-cap backstop: the UI side validates binding counts against the
/// mirrored device limits at the call site, so reaching the cap here means
/// the mirrors diverged. Past the limit the bind itself errors and the draw
/// samples garbage, so drop the input and say which.
fn bind_inputs(
  gl: &glow::Context,
  program: &ShaderProgram,
  inputs: &[PassInput],
  saved: &mut Vec<(u32, i32, i32)>,
  max_units: usize,
) {
  unsafe {
    for (unit, (name, tex, sampler)) in inputs.iter().enumerate() {
      if unit >= max_units {
        log::warn!(
          "[shader] sampler input '{name}' exceeds this device's texture unit limit ({max_units} per pass); skipped"
        );
        continue;
      }
      let Some((loc, _)) = program.uniform(name) else { continue };
      let unit = unit as u32;
      gl.active_texture(glow::TEXTURE0 + unit);
      if !saved.iter().any(|(u, _, _)| *u == unit) {
        saved.push((unit, gl.get_parameter_i32(glow::TEXTURE_BINDING_2D), gl.get_parameter_i32(glow::SAMPLER_BINDING)));
      }
      gl.bind_texture(glow::TEXTURE_2D, Some(*tex));
      gl.bind_sampler(unit, *sampler);
      gl.uniform_1_i32(Some(loc), unit as i32);
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
  let draw =
    PassDraw::Fullscreen { program, params, textures, vertex_count, clear: Some([0.0, 0.0, 0.0, 1.0]), blend: false };
  run_pass(gl, None, (0, 0), width, height, draw);
}

/// Composite `program`'s output over the default framebuffer's existing
/// contents at the viewport rectangle `origin`/`width` x `height` (GL
/// bottom-up coordinates): no clear, premultiplied-alpha blending - the
/// stats overlay draw over the finished frame. A rectangle partly outside
/// the window is clipped by the viewport transform.
pub fn composite_program_over_window(
  gl: &glow::Context,
  program: &ShaderProgram,
  origin: (i32, i32),
  width: u32,
  height: u32,
  textures: &[PassInput],
) {
  let draw = PassDraw::Fullscreen { program, params: &[], textures, vertex_count: 3, clear: None, blend: true };
  run_pass(gl, None, origin, width, height, draw);
}

/// Run one fullscreen draw of `program` into `fbo` (None = the default
/// framebuffer), no clear: the covering triangle writes every pixel. The
/// in-tile MSAA resolve consumes its resolved texture through this instead of
/// a blit (see `gl::draw::draw_and_resolve` for why a blit is not an option there);
/// node shader passes (shaded snapshot boundaries) run through it with their
/// declared params.
pub fn render_program_to_fbo(
  gl: &glow::Context,
  program: &ShaderProgram,
  fbo: Option<glow::Framebuffer>,
  width: u32,
  height: u32,
  params: &[(String, ParamValue)],
  textures: &[PassInput],
) {
  let draw = PassDraw::Fullscreen { program, params, textures, vertex_count: 3, clear: None, blend: false };
  run_pass(gl, fbo, (0, 0), width, height, draw);
}

/// Run one pass into `fbo` (None = the default framebuffer) at viewport
/// `origin` + `width` x `height` (origin is (0, 0) for every whole-target
/// pass; only the overlay composite positions a sub-rectangle): neutralize
/// every piece of fixed-function state that could clip or blend the output
/// away, execute the draw(s), and restore all of it. The save/restore set
/// must stay exhaustive: Impeller runs full render passes on this same
/// context (the process's only one) and both leaves state behind and caches
/// what it set (see the comments below).
pub(super) fn run_pass(
  gl: &glow::Context,
  fbo: Option<glow::Framebuffer>,
  origin: (i32, i32),
  width: u32,
  height: u32,
  draw: PassDraw,
) {
  unsafe {
    let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
    let prev_program_name = gl.get_parameter_i32(glow::CURRENT_PROGRAM);
    let prev_active = gl.get_parameter_i32(glow::ACTIVE_TEXTURE);
    let mut prev_vp = [0i32; 4];
    gl.get_parameter_i32_slice(glow::VIEWPORT, &mut prev_vp);
    let mut prev_scissor_box = [0i32; 4];
    gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut prev_scissor_box);
    let blend = gl.is_enabled(glow::BLEND);
    let depth = gl.is_enabled(glow::DEPTH_TEST);
    let scissor = gl.is_enabled(glow::SCISSOR_TEST);
    let cull = gl.is_enabled(glow::CULL_FACE);

    gl.bind_framebuffer(glow::FRAMEBUFFER, fbo);
    gl.viewport(origin.0, origin.1, width as i32, height as i32);

    // Fixed-function state that could clip or blend the output away is off
    // for both paths; the mesh draws opt depth testing and blending back in
    // per entry below. This set must be exhaustive: Impeller runs full render
    // passes on this same context (the process's only one) and may have left
    // any of the states below active - e.g. rasterizer discard or a zero
    // sample coverage silently kills every draw while clears still land.
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

    // Per-unit texture/sampler bindings saved on first touch, restored once
    // at the end (see bind_inputs).
    let max_units = gl.get_parameter_i32(glow::MAX_TEXTURE_IMAGE_UNITS).max(1) as usize;
    let mut saved_units: Vec<(u32, i32, i32)> = Vec::new();

    match draw {
      PassDraw::Fullscreen { program, params, textures, vertex_count, clear, blend: blended } => {
        apply_program(gl, program, width, height, params);
        bind_inputs(gl, program, textures, &mut saved_units, max_units);
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
        if blended {
          // Premultiplied-alpha composite over the target's existing
          // contents. The blend func is Impeller-cached state, same as in
          // the mesh arm: save and restore it; the enable toggle is already
          // restored by the outer save.
          let prev_blend_func = [
            gl.get_parameter_i32(glow::BLEND_SRC_RGB) as u32,
            gl.get_parameter_i32(glow::BLEND_DST_RGB) as u32,
            gl.get_parameter_i32(glow::BLEND_SRC_ALPHA) as u32,
            gl.get_parameter_i32(glow::BLEND_DST_ALPHA) as u32,
          ];
          gl.enable(glow::BLEND);
          gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
          gl.draw_arrays(glow::TRIANGLES, 0, vertex_count);
          gl.disable(glow::BLEND);
          gl.blend_func_separate(prev_blend_func[0], prev_blend_func[1], prev_blend_func[2], prev_blend_func[3]);
        } else {
          gl.draw_arrays(glow::TRIANGLES, 0, vertex_count);
        }
      }
      PassDraw::Draws { clear, clear_depth, depth: has_depth, groups, tile_clear } => {
        // Mesh pass: geometry does not cover the target, so clear first -
        // once, at the top; every entry then draws over the shared result.
        // Clear color, depth mask, depth func, clear-depth value, the blend
        // func, the scissor box and the cull face/winding are
        // Impeller-cached state: save all of them here and restore after
        // the list. With loadOp "load" (manual targets only) the color
        // buffer keeps its previous contents - the accumulation unlock -
        // while depth stays per-render scratch and clears with every full
        // render. A partial render (only some sub-targets redrawn) clears
        // nothing at the pass level: each group wipes its own rectangle
        // with a covering draw and the rest of the storage keeps its pixels.
        let prev_vao = gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING);
        let mut prev_clear = [0f32; 4];
        gl.get_parameter_f32_slice(glow::COLOR_CLEAR_VALUE, &mut prev_clear);
        let prev_depth_mask = gl.get_parameter_i32(glow::DEPTH_WRITEMASK) != 0;
        let prev_depth_func = gl.get_parameter_i32(glow::DEPTH_FUNC) as u32;
        let prev_clear_depth = gl.get_parameter_f32(glow::DEPTH_CLEAR_VALUE);
        let prev_blend_func = [
          gl.get_parameter_i32(glow::BLEND_SRC_RGB) as u32,
          gl.get_parameter_i32(glow::BLEND_DST_RGB) as u32,
          gl.get_parameter_i32(glow::BLEND_SRC_ALPHA) as u32,
          gl.get_parameter_i32(glow::BLEND_DST_ALPHA) as u32,
        ];
        let prev_cull_face = gl.get_parameter_i32(glow::CULL_FACE_MODE) as u32;
        let prev_front_face = gl.get_parameter_i32(glow::FRONT_FACE) as u32;

        // The clear always writes depth (glClear honors the write mask).
        // Impeller's clip-culling passes set their own depth-clear value
        // (0.0) on this context; clearing with that inverts the test and
        // silently discards every fragment. Always clear to the far plane.
        gl.depth_mask(true);
        gl.clear_depth_f32(1.0);
        let clear_bits = |color: Option<[f32; 4]>, depth: bool| -> u32 {
          let color_bit = match color {
            Some([r, g, b, a]) => {
              gl.clear_color(r, g, b, a);
              glow::COLOR_BUFFER_BIT
            }
            None => 0,
          };
          color_bit | if depth { glow::DEPTH_BUFFER_BIT } else { 0 }
        };
        let bits = clear_bits(clear, clear_depth && has_depth);
        if bits != 0 {
          gl.clear(bits);
        }

        for group in groups {
          // A sub-target's group: viewport and scissor to its rectangle (the
          // scissor is what confines the clear; the viewport transforms the
          // draws), then wipe the rectangle. The whole-target group runs at
          // the pass viewport with the scissor off.
          let (gw, gh) = match group.rect {
            Some((x, y, w, h)) => {
              gl.viewport(x, y, w as i32, h as i32);
              gl.scissor(x, y, w as i32, h as i32);
              gl.enable(glow::SCISSOR_TEST);
              // The rectangle wipe: a covering triangle writing the clear
              // color (masked off when only depth clears) and the far plane
              // (depth test ALWAYS so every fragment lands; skipped when
              // depth does not clear). A draw, not a glClear - see DrawGroup.
              let clear_depth = group.clear_depth && has_depth;
              if let (Some(program), true) = (tile_clear, group.clear.is_some() || clear_depth) {
                let color = group.clear.unwrap_or([0.0; 4]);
                let params = [("uColor".to_string(), ParamValue::Array(color.to_vec()))];
                apply_program(gl, program, w, h, &params);
                gl.color_mask(group.clear.is_some(), group.clear.is_some(), group.clear.is_some(), group.clear.is_some());
                if clear_depth {
                  gl.enable(glow::DEPTH_TEST);
                  gl.depth_func(glow::ALWAYS);
                  gl.depth_mask(true);
                } else {
                  gl.disable(glow::DEPTH_TEST);
                }
                gl.disable(glow::BLEND);
                gl.disable(glow::CULL_FACE);
                gl.bind_vertex_array(None);
                gl.draw_arrays(glow::TRIANGLES, 0, 3);
                gl.color_mask(true, true, true, true);
                gl.depth_func(glow::LESS);
              }
              (w, h)
            }
            None => {
              gl.viewport(origin.0, origin.1, width as i32, height as i32);
              gl.disable(glow::SCISSOR_TEST);
              (width, height)
            }
          };
          for d in group.draws {
            // Shared params first, the entry's own second: an entry naming the
            // same uniform overwrites the shared value (specific beats general).
            apply_program(gl, d.program, gw, gh, group.shared);
            apply_params(gl, d.program, d.params);
            bind_inputs(gl, d.program, &d.inputs, &mut saved_units, max_units);
            // Depth test and write per entry: the pipeline's declared depth
            // state, against the target-owned storage.
            match (has_depth, d.desc.depth) {
              (true, Some(state)) => {
                gl.enable(glow::DEPTH_TEST);
                gl.depth_func(glow::LESS);
                gl.depth_mask(state.write);
              }
              _ => gl.disable(glow::DEPTH_TEST),
            }
            match d.desc.blend {
              Some(BlendMode::Add) => {
                gl.enable(glow::BLEND);
                gl.blend_func(glow::ONE, glow::ONE);
              }
              Some(BlendMode::Multiply) => {
                gl.enable(glow::BLEND);
                gl.blend_func(glow::DST_COLOR, glow::ZERO);
              }
              Some(BlendMode::Alpha) => {
                gl.enable(glow::BLEND);
                gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
              }
              None => gl.disable(glow::BLEND),
            }
            // Face culling per entry: winding is pinned (Impeller may have
            // left either behind) to CW - which is counter-clockwise AS
            // DISPLAYED, because the displayed image is the y flip of GL
            // window space. That makes "front" mean what WebGPU's
            // framebuffer-space rule means: counter-clockwise on screen, so
            // standard meshes drawn with the usual y negation cull the
            // intuitive way.
            match d.desc.cull {
              Some(mode) => {
                gl.enable(glow::CULL_FACE);
                gl.cull_face(mode.gl());
                gl.front_face(glow::CW);
              }
              None => gl.disable(glow::CULL_FACE),
            }
            gl.bind_vertex_array(Some(d.vao));
            // instance_count 1 keeps the plain draw - bit-identical to the
            // non-instanced path (gl_InstanceID reads 0 either way); 0 draws
            // nothing. gl_VertexID includes first_vertex, as in WebGPU (on an
            // indexed draw it reads the index value). An indexed entry's
            // element buffer is VAO state, bound since build_vao; the byte
            // offset positions the range within it.
            match d.index {
              Some(fmt) => {
                let offset = d.range.first_vertex * fmt.size();
                if d.range.instance_count == 1 {
                  gl.draw_elements(d.desc.topology.gl(), d.range.vertex_count, fmt.gl(), offset);
                } else {
                  gl.draw_elements_instanced(
                    d.desc.topology.gl(),
                    d.range.vertex_count,
                    fmt.gl(),
                    offset,
                    d.range.instance_count,
                  );
                }
              }
              None => {
                if d.range.instance_count == 1 {
                  gl.draw_arrays(d.desc.topology.gl(), d.range.first_vertex, d.range.vertex_count);
                } else {
                  gl.draw_arrays_instanced(
                    d.desc.topology.gl(),
                    d.range.first_vertex,
                    d.range.vertex_count,
                    d.range.instance_count,
                  );
                }
              }
            }
          }
        }

        // Re-disable what the entries may have enabled (the outer restore
        // only re-enables) and put the Impeller-cached values back.
        gl.disable(glow::BLEND);
        gl.disable(glow::DEPTH_TEST);
        gl.disable(glow::CULL_FACE);
        gl.disable(glow::SCISSOR_TEST);
        gl.scissor(prev_scissor_box[0], prev_scissor_box[1], prev_scissor_box[2], prev_scissor_box[3]);
        gl.blend_func_separate(prev_blend_func[0], prev_blend_func[1], prev_blend_func[2], prev_blend_func[3]);
        gl.cull_face(prev_cull_face);
        gl.front_face(prev_front_face);
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
    for (unit, prev, prev_smp) in saved_units {
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
