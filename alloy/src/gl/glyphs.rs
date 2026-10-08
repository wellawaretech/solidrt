//! The glyph pass: a text layer's quads drawn from the text atlas into an
//! adopted texture (okf/plans/text-own-rasterizer.md, stage 2). One
//! attributeless instanced-by-arithmetic draw per group of quads: the
//! vertex stage derives the quad and its corner from `gl_VertexID` and
//! fetches the quad's rects and color from a float texture the pass
//! uploads per group, so no vertex buffer or VAO is involved and the whole
//! pass runs through `run_pass` with its exhaustive save/restore. The
//! fragment stage resolves the run's color (the quad's own, or a gradient
//! sampled from a ramp texture at the parameter the layer pixel maps to),
//! samples the mask's coverage (the atlas's red channel: the text atlas is
//! R8, and an rgba8 mask atlas holds premultiplied white, the same value
//! in red), applies the coverage-to-color policy, and
//! writes premultiplied color blended source-over, so kerned pairs and
//! diacritics that overlap composite right. Single-sample, GLES 3.0.

use super::pass::{run_pass, PassDraw, PassInput};
use super::program::ShaderProgram;
use super::storage::create_layer_target;
use super::{prev_framebuffer, prev_texture};
use crate::gpu::{CoveragePolicy, GlyphGroup, GlyphQuad, ParamValue, RAMP_WIDTH};
use glow::HasContext;
use impellers::{Context as ImpellerContext, Texture};
use std::num::NonZeroU32;
use std::rc::Rc;

/// Texels per row of the quad texture: three per quad (its two rects and
/// its color), so a thousand-glyph layer is three rows. Within the 2048
/// every GLES 3.0 device allows.
const QUADS_TEXTURE_WIDTH: u32 = 1024;
const TEXELS_PER_QUAD: u32 = 3;
/// Two triangles a quad.
const VERTICES_PER_QUAD: i32 = 6;
/// Bytes per rgba32f texel.
const BYTES_PER_FLOAT_TEXEL: usize = 16;
/// Bytes per rgba8 ramp texel.
const BYTES_PER_RAMP_TEXEL: usize = 4;

// The quads texture is read with texelFetch at integer coordinates, so its
// sampler must be highp: at the default lowp the coordinates would not
// survive past a few hundred texels.
const VERTEX_SRC: &str = r"
uniform highp sampler2D uQuads;
uniform vec2 uAtlasSize;
uniform int uQuadsWidth;
out vec2 vUv;
out vec2 vPos;
out vec4 vColor;
vec4 part(int quad, int index) {
  int i = quad * 3 + index;
  return texelFetch(uQuads, ivec2(i % uQuadsWidth, i / uQuadsWidth), 0);
}
void main() {
  int quad = gl_VertexID / 6;
  int corner = gl_VertexID - quad * 6;
  // Two triangles: (0,0) (1,0) (0,1) and (0,1) (1,0) (1,1).
  vec2 c = vec2(
    (corner == 1 || corner == 4 || corner == 5) ? 1.0 : 0.0,
    (corner == 2 || corner == 3 || corner == 5) ? 1.0 : 0.0);
  vec4 dst = part(quad, 0);
  vec4 src = part(quad, 1);
  vColor = part(quad, 2);
  vec2 p = dst.xy + c * dst.zw;
  // Layer pixels, y down from the top row, to clip space: a fragment at
  // clip y = -1 lands in texture row 0, which the compositor samples as
  // the top, so no flip.
  gl_Position = vec4(p / iResolution * 2.0 - 1.0, 0.0, 1.0);
  vPos = p;
  vUv = (src.xy + c * src.zw) / uAtlasSize;
}
";

// The policy's modes are `CoverageMode`'s discriminants, the gradient
// kinds `GradientKind`'s (0 for none) and the tile modes `GradientTile`'s
// (gpu/glyphs.rs). A gradient run's color comes from the ramp at the
// parameter its layer pixel maps to; the quad's alpha still applies.
const FRAGMENT_SRC: &str = r"
uniform sampler2D uAtlas;
uniform sampler2D uRamp;
uniform int uMode;
uniform float uGamma;
uniform float uContrast;
uniform vec4 uGammaRatios;
uniform int uGradient;
uniform vec3 uGradientX;
uniform vec3 uGradientY;
uniform int uTile;
in vec2 vUv;
in vec2 vPos;
in vec4 vColor;
float lum(vec3 c) { return dot(c, vec3(0.2126, 0.7152, 0.0722)); }
void main() {
  vec4 color = vColor;
  if (uGradient != 0) {
    vec3 p = vec3(vPos, 1.0);
    vec2 q = vec2(dot(uGradientX, p), dot(uGradientY, p));
    float t = uGradient == 1 ? q.x : length(q);
    float inside = 1.0;
    if (uTile == 1) {
      t = fract(t);
    } else if (uTile == 2) {
      t = 1.0 - abs(1.0 - mod(t, 2.0));
    } else if (uTile == 3) {
      inside = (t < 0.0 || t > 1.0) ? 0.0 : 1.0;
    }
    vec4 g = texture(uRamp, vec2(clamp(t, 0.0, 1.0), 0.5));
    color = vec4(g.rgb, g.a * vColor.a * inside);
  }
  float c = texture(uAtlas, vUv).r;
  bool light = lum(color.rgb) > 0.5;
  if (uMode == 1) {
    c = light ? pow(c, 1.0 / uGamma) : 1.0 - pow(1.0 - c, 1.0 / uGamma);
  } else if (uMode == 2) {
    if (light) c = pow(1.0 - pow(1.0 - c, uGamma), 1.0 / uGamma);
  } else if (uMode == 3) {
    if (light) c = pow(c, 1.0 / uGamma);
  } else if (uMode == 4) {
    // DirectWrite's grayscale blend (Windows Terminal's dwrite_helpers.hlsl,
    // MIT): the contrast boost fades with the text's lightness, the alpha
    // correction reads its intensity.
    float k = uContrast * clamp(dot(color.rgb, vec3(0.30, 0.59, 0.11) * -4.0) + 3.0, 0.0, 1.0);
    float f = dot(color.rgb, vec3(0.25, 0.5, 0.25));
    c = c * (k + 1.0) / (c * k + 1.0);
    c = c + c * (1.0 - c) * ((uGammaRatios.x * f + uGammaRatios.y) * c + (uGammaRatios.z * f + uGammaRatios.w));
  }
  float a = c * color.a;
  fragColor = vec4(color.rgb * a, a);
}
";

/// The pass's retained GL objects: the program, compiled when the raster
/// state is created (every screen has text, so its first frame must not
/// pay the compile), the quads texture, grown as layers need, and the
/// gradient ramp texture, rewritten per gradient group.
pub(crate) struct GlyphRig {
  program: Option<Rc<ShaderProgram>>,
  quads: Option<glow::Texture>,
  rows: u32,
  ramp: Option<glow::Texture>,
}

impl GlyphRig {
  /// Compile the pass's program now; a failure is logged and retried at
  /// the first layer, which then reports it.
  pub(crate) fn new(gl: &glow::Context) -> Self {
    let mut rig = Self { program: None, quads: None, rows: 0, ramp: None };
    if let Err(e) = rig.program(gl) {
      log::warn!("[alloy] {e}");
    }
    rig
  }

  fn program(&mut self, gl: &glow::Context) -> Result<Rc<ShaderProgram>, String> {
    if self.program.is_none() {
      let program = ShaderProgram::new_pipeline(gl, VERTEX_SRC, FRAGMENT_SRC)
        .map_err(|e| format!("glyph pass program: {e}"))?
        .with_label(Some("glyph-pass".to_string()));
      self.program = Some(Rc::new(program));
    }
    Ok(self.program.as_ref().expect("just ensured").clone())
  }

  // Upload `quads` into the quads texture, three texels each, rows top to
  // bottom; the texture is reallocated when it needs more rows.
  fn upload(&mut self, gl: &glow::Context, quads: &[GlyphQuad]) -> Result<glow::Texture, String> {
    let texels = quads.len() as u32 * TEXELS_PER_QUAD;
    let rows = texels.div_ceil(QUADS_TEXTURE_WIDTH).max(1);
    let mut bytes = Vec::with_capacity((QUADS_TEXTURE_WIDTH * rows) as usize * BYTES_PER_FLOAT_TEXEL);
    for quad in quads {
      for value in quad.dst.iter().chain(&quad.src).chain(&quad.color) {
        bytes.extend_from_slice(&value.to_ne_bytes());
      }
    }
    bytes.resize((QUADS_TEXTURE_WIDTH * rows) as usize * BYTES_PER_FLOAT_TEXEL, 0);
    unsafe {
      let prev = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
      let texture = match self.quads {
        Some(texture) => texture,
        None => {
          let texture = gl.create_texture().map_err(|e| format!("glGenTextures failed: {e}"))?;
          gl.bind_texture(glow::TEXTURE_2D, Some(texture));
          // A float texture is not filterable on GLES 3.0 without an
          // extension; texelFetch never filters, and NEAREST keeps the
          // texture complete.
          gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
          gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
          gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
          gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
          self.quads = Some(texture);
          texture
        }
      };
      gl.bind_texture(glow::TEXTURE_2D, Some(texture));
      if rows > self.rows {
        gl.tex_image_2d(
          glow::TEXTURE_2D,
          0,
          glow::RGBA32F as i32,
          QUADS_TEXTURE_WIDTH as i32,
          rows as i32,
          0,
          glow::RGBA,
          glow::FLOAT,
          glow::PixelUnpackData::Slice(Some(&bytes)),
        );
        self.rows = rows;
      } else {
        gl.tex_sub_image_2d(
          glow::TEXTURE_2D,
          0,
          0,
          0,
          QUADS_TEXTURE_WIDTH as i32,
          rows as i32,
          glow::RGBA,
          glow::FLOAT,
          glow::PixelUnpackData::Slice(Some(&bytes)),
        );
      }
      gl.bind_texture(glow::TEXTURE_2D, prev_texture(prev));
      Ok(texture)
    }
  }

  // The ramp texture (RAMP_WIDTH by 1, rgba8, sampled linearly and
  // clamped), created on first use and rewritten with `colors` when a
  // group has a gradient; a solid group binds it untouched and never
  // samples it.
  fn ramp(&mut self, gl: &glow::Context, colors: Option<&[[u8; 4]]>) -> Result<glow::Texture, String> {
    unsafe {
      let prev = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
      let texture = match self.ramp {
        Some(texture) => texture,
        None => {
          let texture = gl.create_texture().map_err(|e| format!("glGenTextures failed: {e}"))?;
          gl.bind_texture(glow::TEXTURE_2D, Some(texture));
          gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
          gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
          gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
          gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
          let blank = vec![0u8; RAMP_WIDTH * BYTES_PER_RAMP_TEXEL];
          gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            RAMP_WIDTH as i32,
            1,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(Some(&blank)),
          );
          self.ramp = Some(texture);
          texture
        }
      };
      if let Some(colors) = colors {
        let mut bytes = Vec::with_capacity(RAMP_WIDTH * BYTES_PER_RAMP_TEXEL);
        for texel in colors.iter().take(RAMP_WIDTH) {
          bytes.extend_from_slice(texel);
        }
        bytes.resize(RAMP_WIDTH * BYTES_PER_RAMP_TEXEL, 0);
        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        gl.tex_sub_image_2d(
          glow::TEXTURE_2D,
          0,
          0,
          0,
          RAMP_WIDTH as i32,
          1,
          glow::RGBA,
          glow::UNSIGNED_BYTE,
          glow::PixelUnpackData::Slice(Some(&bytes)),
        );
      }
      gl.bind_texture(glow::TEXTURE_2D, prev_texture(prev));
      Ok(texture)
    }
  }
}

/// Draw `groups` from the atlas (`atlas` its GL name and sampler,
/// `atlas_size` its texels) into a texture of `width` x `height` under
/// `policy`: a new adopted texture, or `into` an existing one from an
/// earlier call at exactly that size, cleared first either way. The
/// calling thread must have the GL context current.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_glyphs(
  gl: &glow::Context,
  impeller_ctx: &mut ImpellerContext,
  rig: &mut GlyphRig,
  groups: &[GlyphGroup],
  atlas: glow::Texture,
  atlas_sampler: Option<glow::Sampler>,
  atlas_size: (u32, u32),
  width: u32,
  height: u32,
  policy: CoveragePolicy,
  into: Option<&Texture>,
) -> Result<Texture, String> {
  let program = rig.program(gl)?;
  // The target: `into`'s storage behind a framebuffer of our own, or a new
  // layer texture.
  let (fbo, created) = match into {
    Some(texture) => {
      let name = texture.get_opengl_handle();
      let tex =
        glow::NativeTexture(NonZeroU32::new(name as u32).ok_or_else(|| "texture has no GL handle".to_string())?);
      unsafe {
        let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);
        let fbo = gl.create_framebuffer().map_err(|e| format!("glGenFramebuffers failed: {e}"))?;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
        let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
        gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
        if status != glow::FRAMEBUFFER_COMPLETE {
          gl.delete_framebuffer(fbo);
          return Err(format!("text layer framebuffer incomplete: {status:#x}"));
        }
        (fbo, None)
      }
    }
    None => {
      let (tex, fbo) = create_layer_target(gl, width, height, [0.0; 4])?;
      (fbo, Some(tex))
    }
  };
  let drawn = draw_groups(gl, rig, &program, groups, atlas, atlas_sampler, atlas_size, width, height, policy, fbo);
  unsafe { gl.delete_framebuffer(fbo) };
  match (drawn, created) {
    (Err(e), Some(tex)) => {
      unsafe { gl.delete_texture(tex) };
      Err(e)
    }
    (Err(e), None) => Err(e),
    (Ok(()), None) => Ok(into.expect("the retained texture").clone()),
    (Ok(()), Some(tex)) => match unsafe { impeller_ctx.adopt_opengl_texture(width, height, 1, tex.0.get() as u64) } {
      Some(adopted) => Ok(adopted),
      None => {
        unsafe { gl.delete_texture(tex) };
        Err("failed to adopt the text layer texture".to_string())
      }
    },
  }
}

// Every group as one draw into `fbo`, the target cleared transparent by
// the first draw (or once, when no group has quads, so a re-render into
// retained storage leaves no stale pixels).
#[allow(clippy::too_many_arguments)]
fn draw_groups(
  gl: &glow::Context,
  rig: &mut GlyphRig,
  program: &ShaderProgram,
  groups: &[GlyphGroup],
  atlas: glow::Texture,
  atlas_sampler: Option<glow::Sampler>,
  atlas_size: (u32, u32),
  width: u32,
  height: u32,
  policy: CoveragePolicy,
  fbo: glow::Framebuffer,
) -> Result<(), String> {
  let mut cleared = false;
  for group in groups.iter().filter(|g| !g.quads.is_empty()) {
    let quads_texture = rig.upload(gl, &group.quads)?;
    let ramp = rig.ramp(gl, group.gradient.as_ref().map(|g| g.ramp.as_slice()))?;
    let (kind, tile, x_row, y_row) = match &group.gradient {
      Some(g) => (g.kind as i32, g.tile as i32, g.x_row, g.y_row),
      None => (0, 0, [0.0; 3], [0.0; 3]),
    };
    let params = [
      ("uMode".to_string(), ParamValue::Scalar(policy.mode as i32 as f32)),
      ("uGamma".to_string(), ParamValue::Scalar(policy.gamma)),
      ("uContrast".to_string(), ParamValue::Scalar(policy.contrast)),
      ("uGammaRatios".to_string(), ParamValue::Array(policy.gamma_ratios().to_vec())),
      ("uAtlasSize".to_string(), ParamValue::Array(vec![atlas_size.0 as f32, atlas_size.1 as f32])),
      ("uQuadsWidth".to_string(), ParamValue::Scalar(QUADS_TEXTURE_WIDTH as f32)),
      ("uGradient".to_string(), ParamValue::Scalar(kind as f32)),
      ("uGradientX".to_string(), ParamValue::Array(x_row.to_vec())),
      ("uGradientY".to_string(), ParamValue::Array(y_row.to_vec())),
      ("uTile".to_string(), ParamValue::Scalar(tile as f32)),
    ];
    let textures = [
      PassInput::d2("uAtlas", atlas, atlas_sampler),
      PassInput::d2("uQuads", quads_texture, None),
      PassInput::d2("uRamp", ramp, None),
    ];
    let draw = PassDraw::Fullscreen {
      program,
      params: &params,
      textures: &textures,
      vertex_count: group.quads.len() as i32 * VERTICES_PER_QUAD,
      clear: (!cleared).then_some([0.0; 4]),
      blend: true,
    };
    run_pass(gl, Some(fbo), (0, 0), width, height, None, draw);
    cleared = true;
  }
  if !cleared {
    let draw =
      PassDraw::Fullscreen { program, params: &[], textures: &[], vertex_count: 0, clear: Some([0.0; 4]), blend: true };
    run_pass(gl, Some(fbo), (0, 0), width, height, None, draw);
  }
  Ok(())
}
