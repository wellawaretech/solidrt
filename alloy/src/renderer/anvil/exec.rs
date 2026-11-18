//! The GL half of a draw: the shape program, the instance buffer under the
//! VAO the executor owns, and the draw that uploads a list's instances
//! (`super::walk`) and issues them. Raster thread only, with the context
//! current.

use super::super::contract::{DrawReport, Target};
use super::super::{TextureSampling, TileMode};
use super::record::DisplayList;
use super::walk::{self, Instance};
use crate::gpu::{CoveragePolicy, GlyphGradient, RAMP_WIDTH};
use euclid::default::{Box2D, Point2D};
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
const ATTR_RADII: u32 = 6;
const ATTR_RCLIP: u32 = 7;
const ATTR_RCLIP_RADII: u32 = 8;

// The nine instanced attributes: location and byte offset in `Instance`.
// Enabled with divisor 1 once in `new`; re-pointed per batch in `draw`.
const ATTRIBUTES: [(u32, usize); 9] = [
  (ATTR_RECT, std::mem::offset_of!(Instance, rect)),
  (ATTR_LINEAR, std::mem::offset_of!(Instance, linear)),
  (ATTR_OFFSET, std::mem::offset_of!(Instance, offset_kind_stroke)),
  (ATTR_COLOR, std::mem::offset_of!(Instance, color)),
  (ATTR_TEX, std::mem::offset_of!(Instance, tex)),
  (ATTR_CLIP, std::mem::offset_of!(Instance, clip)),
  (ATTR_RADII, std::mem::offset_of!(Instance, radii)),
  (ATTR_RCLIP, std::mem::offset_of!(Instance, rclip)),
  (ATTR_RCLIP_RADII, std::mem::offset_of!(Instance, rclip_radii)),
];

/// The texture unit `uTexture` samples: a batch's texture, an image or a
/// glyph run's atlas.
const TEXTURE_UNIT: u32 = 0;
/// The texture unit `uRamp` samples: a glyph run's gradient ramp.
const RAMP_UNIT: u32 = 1;
/// Bytes per rgba8 ramp texel.
const BYTES_PER_RAMP_TEXEL: usize = 4;

/// `uGradient` for a glyph run without a gradient; a gradient's is its
/// `GradientKind` discriminant.
const NO_GRADIENT: i32 = 0;
// `uTile` per tile mode. The `const int TILE_*` declarations in
// FRAGMENT_SRC must match them.
const TILE_CLAMP: i32 = 0;
const TILE_REPEAT: i32 = 1;
const TILE_MIRROR: i32 = 2;
const TILE_DECAL: i32 = 3;

// A target's logical (0, 0) is its top: it maps to clip (-1, +1) for every
// target, window and texture alike. Window lists are drawn unflipped and
// offscreen lists arrive pre-flipped (raster::offscreen::flip_for_fbo),
// which is what put texel row 0 at the top of a sampled image under
// Impeller; gl/glyphs.rs and gl/program.rs describe the same row 0 is top
// convention for the passes, and the plan (okf/plans/own-renderer.md)
// pins it. Texture sampling reads vUv with v = 0 at texel row 0, no flip.
//
// The layout(location = N) literals must match the ATTR_* constants.
const VERTEX_SRC: &str = r"#version 300 es
precision highp float;
layout(location = 0) in vec4 aRect;      // x, y, w, h in local space
layout(location = 1) in vec4 aLinear;    // m11, m12, m21, m22
layout(location = 2) in vec4 aOffset;    // m31, m32 (the translation), kind, stroke width
layout(location = 3) in vec4 aColor;     // premultiplied; straight alpha for a glyph
layout(location = 4) in vec4 aTex;       // u0, v0, u1, v1 for a textured instance; cap, length for a line
layout(location = 5) in vec4 aClip;      // x0, y0, x1, y1 in device pixels
layout(location = 6) in vec4 aRadii;     // corner radii in local pixels: tl, tr, br, bl
layout(location = 7) in vec4 aRClip;     // the rounded clip in device pixels: x0, y0, x1, y1
layout(location = 8) in vec4 aRClipRadii; // its corner radii in device pixels; all zero: none
uniform vec2 uSize;                      // the target in device pixels
out vec2 vLocal;                         // position within the shape, local pixels from its origin
out vec2 vDevice;                        // device pixels, y down
out vec2 vPos;                           // the op's space, untransformed: what a glyph gradient maps
flat out vec4 vColor;
flat out vec4 vClip;
flat out vec4 vShape;                    // w, h, kind, stroke width
flat out vec4 vParams;                   // the line kind's cap and length
flat out vec4 vRadii;
flat out vec4 vRClip;
flat out vec4 vRClipRadii;
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
  vPos = p;
  vColor = aColor;
  vClip = aClip;
  vShape = vec4(aRect.zw, aOffset.zw);
  vUv = aTex.xy + c * (aTex.zw - aTex.xy);
  vParams = aTex;
  vRadii = aRadii;
  vRClip = aRClip;
  vRClipRadii = aRClipRadii;
}
";

// Every shape is evaluated here from the quad's local coordinates: the
// walk expanded a stroke's quad by half the width on every side, so the
// un-expanded shape's half extent is the quad's less half the stroke.
// `hs` is the quad's half size (`half` is reserved in GLSL ES 3.00).
//
// Anti-aliasing has two sources. The quad's own edges (a rect fill's, a
// rect stroke's outer edge, a line body's long edges and its butt and
// square ends) are MSAA's: the rasterizer covers them per sample. The
// edges the fragment stage decides (an oval's rim, a stroke band's inner
// edge, a round cap) get analytic coverage: under MSAA the fragment stage
// runs once per pixel and a discard keeps or drops all of its samples, so
// a discard alone would leave them aliased.
//
// A rounded rect's fill and stroke are both analytic: the rounded-box
// distance to the un-expanded edge, its inside for a fill and the band
// |d| <= s/2 around it for a stroke (the centered stroke Skia draws: outer
// radius r + s/2, inner r - s/2). Its quad is padded by ROUNDED_QUAD_PAD
// past the stroke's half width, so a straight run's analytic edge never
// lies on the quad's edge, where MSAA and the ramp would both attenuate
// it; the padding fragments are past the edge and discard. The pad is in
// local units, device pixels for a rendertree list; under a view
// transform that shrinks, a sliver of the double attenuation may return.
// The rounded clip is the same distance in device space, multiplied in.
// A textured instance (KIND_TEXTURED added to its kind) multiplies its
// coverage into the premultiplied texel.
//
// A glyph (KIND_GLYPH) has no analytic coverage: its quad is axis-aligned
// at whole pixels and the atlas's red channel carries the coverage. Its
// color is straight alpha, the quad's own or the gradient ramp's at the
// parameter its position in the op's space maps to, and the batch's
// coverage policy turns the coverage into color before the fragment
// premultiplies; the glyph pass's (gl/glyphs.rs) recipe, ported.
const FRAGMENT_SRC: &str = r"#version 300 es
precision highp float;
uniform sampler2D uTexture;
uniform sampler2D uRamp;
uniform int uMode;
uniform float uGamma;
uniform float uContrast;
uniform vec4 uGammaRatios;
uniform int uGradient;
uniform vec3 uGradientX;
uniform vec3 uGradientY;
uniform int uTile;
in vec2 vLocal;
in vec2 vDevice;
in vec2 vPos;
flat in vec4 vColor;
flat in vec4 vClip;
flat in vec4 vShape;      // quad w, quad h, kind (+ KIND_TEXTURED), stroke width (0 = fill)
flat in vec4 vParams;     // line kind: cap, length
flat in vec4 vRadii;      // corner radii, local pixels: tl, tr, br, bl
flat in vec4 vRClip;      // the rounded clip in device pixels: x0, y0, x1, y1
flat in vec4 vRClipRadii; // its corner radii in device pixels; all zero: none
in vec2 vUv;
out vec4 fragColor;
// The shape kinds, the textured flag and the line caps, as the instance
// carries them (walk.rs KIND_* and CAP_*): they must match.
const int KIND_RECT = 0;
const int KIND_OVAL = 1;
const int KIND_LINE = 2;
const int KIND_GLYPH = 3;
const int KIND_ROUNDED_RECT = 4;
const int KIND_TEXTURED = 8;
const int CAP_ROUND = 2;
// The coverage modes are CoverageMode's discriminants (0 is naive), the
// gradient kinds GradientKind's (exec.rs NO_GRADIENT for none), the tile
// modes exec.rs TILE_*: they must match.
const int MODE_LINEAR_LIGHT = 1;
const int MODE_POLARITY_REMAP = 2;
const int MODE_POLARITY_LINEAR = 3;
const int MODE_DIRECTWRITE = 4;
const int NO_GRADIENT = 0;
const int GRADIENT_LINEAR = 1;
const int TILE_REPEAT = 1;
const int TILE_MIRROR = 2;
const int TILE_DECAL = 3;
// Rec. 709 luminance weights; text whose color is above LIGHT_TEXT_LUMA is
// light text to the polarity-aware modes.
const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);
const float LIGHT_TEXT_LUMA = 0.5;
// DirectWrite's grayscale blend constants (Windows Terminal's
// dwrite_helpers.hlsl, MIT): the lightness weights its contrast fade reads,
// the fade FADE_END - FADE_SLOPE * lightness (full at 0.5, gone at 0.75),
// and the intensity weights its alpha correction reads.
const vec3 DWRITE_LIGHTNESS = vec3(0.30, 0.59, 0.11);
const float DWRITE_FADE_SLOPE = 4.0;
const float DWRITE_FADE_END = 3.0;
const vec3 DWRITE_INTENSITY = vec3(0.25, 0.5, 0.25);
// Local pixels a rounded rect's quad extends past its edge and its
// stroke's half width (walk.rs ROUNDED_QUAD_PAD must match).
const float ROUNDED_QUAD_PAD = 1.0;
// A radius a stroke wider than its oval would drive to zero or below;
// keeps the normalization finite.
const float OVAL_RADIUS_FLOOR = 1e-3;
// How close to the center the normalized point may be before the 0/0
// form of the oval distance is replaced by deep inside.
const float OVAL_CENTER_EPS = 1e-4;
// The smallest screen-space footprint a distance is divided by: a
// constant distance (deep inside) has a zero gradient and must read as
// covered.
const float FOOTPRINT_FLOOR = 1e-6;
// A distance that reads as fully inside whatever the footprint: used
// where the quad's own edges are the shape's edges and MSAA covers them,
// and negated, as far outside a rect stroke's hole. Large and finite,
// never -INFINITY.
const float INSIDE = -1e6;
// The screen-space footprint of d: how far d changes across one device
// pixel along the edge's normal, so a local-unit distance divided by it
// is in device pixels under any scale or skew. The gradient's length,
// not the L1 norm fwidth takes, which would widen a diagonal edge's
// ramp to about 1.4 pixels.
float footprint(float d) {
  return max(length(vec2(dFdx(d), dFdy(d))), FOOTPRINT_FLOOR);
}
// Coverage of a fragment at signed distance d (positive outside) from an
// edge the quad does not provide: the pixel-center distance in device
// pixels as a linear ramp one pixel wide, the box filter 4x MSAA
// approximates on the quad edges.
float coverage(float d) {
  return clamp(0.5 - d / footprint(d), 0.0, 1.0);
}
// The signed distance from p (measured from the box's center, y down) to a
// box of half size hs with the corner radii r (tl, tr, br, bl): the
// rounded-box distance with the radius of p's quadrant. Positive outside.
// On the axes the distance does not depend on the radius, so the
// per-quadrant choice is continuous where it matters.
float roundedBox(vec2 p, vec2 hs, vec4 r) {
  float radius = p.x < 0.0 ? (p.y < 0.0 ? r.x : r.w) : (p.y < 0.0 ? r.y : r.z);
  vec2 q = abs(p) - hs + radius;
  return min(max(q.x, q.y), 0.0) + length(max(q, vec2(0.0))) - radius;
}
// The rounded clip's coverage at this fragment, in device space: the same
// distance function over the fragment's device position. All-zero radii
// mean no rounded clip is in effect (the box test already clipped): 1.
float roundedClip() {
  if (!any(greaterThan(vRClipRadii, vec4(0.0)))) return 1.0;
  vec2 center = (vRClip.xy + vRClip.zw) * 0.5;
  vec2 half_size = (vRClip.zw - vRClip.xy) * 0.5;
  return coverage(roundedBox(vDevice - center, half_size, vRClipRadii));
}
// A glyph fragment: the run's straight-alpha color, the atlas's coverage
// under the policy, premultiplied, and the rounded clip multiplied in (as
// the clip fades a text layer's texels).
vec4 glyph() {
  vec4 color = vColor;
  if (uGradient != NO_GRADIENT) {
    vec3 p3 = vec3(vPos, 1.0);
    vec2 q = vec2(dot(uGradientX, p3), dot(uGradientY, p3));
    float t = uGradient == GRADIENT_LINEAR ? q.x : length(q);
    float inside = 1.0;
    if (uTile == TILE_REPEAT) {
      t = fract(t);
    } else if (uTile == TILE_MIRROR) {
      t = 1.0 - abs(1.0 - mod(t, 2.0));
    } else if (uTile == TILE_DECAL) {
      inside = (t < 0.0 || t > 1.0) ? 0.0 : 1.0;
    }
    vec4 g = texture(uRamp, vec2(clamp(t, 0.0, 1.0), 0.5));
    color = vec4(g.rgb, g.a * vColor.a * inside);
  }
  float c = texture(uTexture, vUv).r;
  bool light = dot(color.rgb, LUMA) > LIGHT_TEXT_LUMA;
  if (uMode == MODE_LINEAR_LIGHT) {
    c = light ? pow(c, 1.0 / uGamma) : 1.0 - pow(1.0 - c, 1.0 / uGamma);
  } else if (uMode == MODE_POLARITY_REMAP) {
    if (light) c = pow(1.0 - pow(1.0 - c, uGamma), 1.0 / uGamma);
  } else if (uMode == MODE_POLARITY_LINEAR) {
    if (light) c = pow(c, 1.0 / uGamma);
  } else if (uMode == MODE_DIRECTWRITE) {
    // The contrast boost fades with the text's lightness, the alpha
    // correction reads its intensity.
    float k = uContrast * clamp(dot(color.rgb, DWRITE_LIGHTNESS * -DWRITE_FADE_SLOPE) + DWRITE_FADE_END, 0.0, 1.0);
    float f = dot(color.rgb, DWRITE_INTENSITY);
    c = c * (k + 1.0) / (c * k + 1.0);
    c = c + c * (1.0 - c) * ((uGammaRatios.x * f + uGammaRatios.y) * c + (uGammaRatios.z * f + uGammaRatios.w));
  }
  float a = c * color.a * roundedClip();
  return vec4(color.rgb * a, a);
}
void main() {
  // The clip, per fragment, so a clip change never ends a batch. Under
  // MSAA the test runs per fragment, not per sample: a clip edge is hard
  // where a shape edge is antialiased. Acceptable for stage 1.
  if (any(lessThan(vDevice, vClip.xy)) || any(greaterThanEqual(vDevice, vClip.zw))) discard;
  // The kind arrives as a flat float; rounding reads it robustly. The
  // textured flag is added on top of the shape kind.
  int kind = int(vShape.z + 0.5);
  bool textured = kind >= KIND_TEXTURED;
  if (textured) kind -= KIND_TEXTURED;
  // A flat per-instance test like the shape branches below, so the
  // derivatives in the rounded clip stay defined.
  if (kind == KIND_GLYPH) {
    fragColor = glyph();
    return;
  }
  float s = vShape.w;
  vec2 hs = vShape.xy * 0.5;
  // From the quad's center.
  vec2 p = vLocal - hs;
  // The branches below test flat per-instance values, so every fragment
  // of one quad takes the same path and the derivatives after them are
  // defined.
  float d = INSIDE;
  // A rect stroke's hole, per axis: the distance past the inner rect's
  // edge, negative inside it. The default is far outside: no hole.
  vec2 hole = vec2(-INSIDE);
  if (kind == KIND_RECT) {
    // A fill is the quad. A stroke is the quad less the inner rect, with
    // sharp corners (a miter join); the inner edge is the analytic one.
    // A stroke wider than the box has no inner rect and fills it.
    vec2 inner = hs - s;
    if (s > 0.0 && all(greaterThan(inner, vec2(0.0)))) {
      hole = abs(p) - inner;
    }
  } else if (kind == KIND_ROUNDED_RECT) {
    // The quad is the shape's box grown by the stroke's half width and
    // the pad; the distance is to the un-grown box, so a stroke is the
    // band around it and a fill is its inside, both analytic.
    vec2 inner = hs - s * 0.5 - ROUNDED_QUAD_PAD;
    float dist = roundedBox(p, inner, vRadii);
    d = s > 0.0 ? abs(dist) - s * 0.5 : dist;
  } else if (kind == KIND_OVAL) {
    vec2 r = max(hs - s * 0.5, vec2(OVAL_RADIUS_FLOOR));
    vec2 q = p / r;
    float lq = length(q);
    // The signed distance to the ellipse, gradient-normalized: |q| - 1 over
    // |grad| = |q / r| / |q|. First-order exact for the offset curve, so a
    // thin stroke or a circle is exact and a thick stroke on an eccentric
    // oval is off by a few percent near the long axis. At the center both
    // vanish; the center is deep inside, at least the smaller radius in.
    float dist = lq < OVAL_CENTER_EPS ? -min(r.x, r.y) : (lq - 1.0) * lq / length(q / r);
    d = s > 0.0 ? abs(dist) - s * 0.5 : dist;
  } else if (kind == KIND_LINE) {
    // Local x along the segment from 0 to its length, y across it; the
    // quad extends by ext at both ends for square and round caps, and its
    // edges are the body's and the butt and square ends'. A round cap is
    // the segment's distance beyond the ends.
    float len = vParams.y;
    float ext = (vShape.x - len) * 0.5;
    vec2 l = vec2(vLocal.x - ext, p.y);
    float ax = max(max(-l.x, l.x - len), 0.0);
    if (int(vParams.x + 0.5) == CAP_ROUND && ax > 0.0) {
      d = length(vec2(ax, l.y)) - s * 0.5;
    }
  }
  // The hole is covered per axis, each through its own footprint, and
  // removed as the product: one distance through the corner's kink would
  // count both axes' derivatives at the inner corner and leave a pixel
  // there partly covered where the corner sits on the grid.
  vec2 inHole = clamp(0.5 - hole / vec2(footprint(hole.x), footprint(hole.y)), 0.0, 1.0);
  float c = coverage(d) * (1.0 - inHole.x * inHole.y);
  // The rounded clip, in device space: the same distance function over
  // the fragment's device position, composed as the minimum, not the
  // product. A shape whose edge lies on the clip's edge (a rounded card
  // under an overflow clip of the same radius, the common case) would
  // otherwise ramp twice and show a light rim; the minimum is what a
  // per-sample stencil clip gives there, and errs toward opaque by at
  // most one pixel where two independent edges cross.
  c = min(c, roundedClip());
  if (c <= 0.0) discard;
  // Premultiplied texels; vColor is then the op's alpha in every channel,
  // so the multiply fades the texel as a whole.
  vec4 color = textured ? texture(uTexture, vUv) * vColor : vColor;
  fragColor = color * c;
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
  /// The two ways a texture quad samples, as sampler objects, so a bound
  /// texture's own parameter state is never read or written: on this
  /// shared context only a bound sampler object is a reliable contract
  /// (raster/offscreen.rs says the same for the node shader passes).
  sampler_linear: glow::Sampler,
  sampler_nearest: glow::Sampler,
  /// The glyph runs' policy and gradient uniforms, set per glyph batch.
  glyph_uniforms: GlyphUniforms,
  /// The gradient ramp (`RAMP_WIDTH` x 1 rgba8, straight-alpha sRGB),
  /// rewritten per gradient batch and bound on `RAMP_UNIT` for every
  /// glyph batch; a solid run never samples it.
  ramp: glow::Texture,
}

/// The uniforms a glyph batch sets: its coverage policy and its gradient.
struct GlyphUniforms {
  mode: glow::UniformLocation,
  gamma: glow::UniformLocation,
  contrast: glow::UniformLocation,
  gamma_ratios: glow::UniformLocation,
  gradient: glow::UniformLocation,
  gradient_x: glow::UniformLocation,
  gradient_y: glow::UniformLocation,
  tile: glow::UniformLocation,
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
    let size_uniform = uniform(&gl, program, "uSize")?;
    let texture_uniform = uniform(&gl, program, "uTexture")?;
    let ramp_uniform = uniform(&gl, program, "uRamp")?;
    let glyph_uniforms = GlyphUniforms {
      mode: uniform(&gl, program, "uMode")?,
      gamma: uniform(&gl, program, "uGamma")?,
      contrast: uniform(&gl, program, "uContrast")?,
      gamma_ratios: uniform(&gl, program, "uGammaRatios")?,
      gradient: uniform(&gl, program, "uGradient")?,
      gradient_x: uniform(&gl, program, "uGradientX")?,
      gradient_y: uniform(&gl, program, "uGradientY")?,
      tile: uniform(&gl, program, "uTile")?,
    };
    // A sampler uniform names a unit, not a texture: set once, the unit
    // never changes.
    gl.use_program(Some(program));
    gl.uniform_1_i32(Some(&texture_uniform), TEXTURE_UNIT as i32);
    gl.uniform_1_i32(Some(&ramp_uniform), RAMP_UNIT as i32);
    gl.use_program(None);
    let ramp = create_ramp(&gl)?;
    let sampler_linear = create_sampler(&gl, glow::LINEAR, "linear")?;
    let sampler_nearest = create_sampler(&gl, glow::NEAREST, "nearest")?;
    let vao = gl.create_vertex_array().map_err(|e| format!("shape vertex array: {e}"))?;
    let instances = gl.create_buffer().map_err(|e| format!("shape instance buffer: {e}"))?;
    gl.bind_vertex_array(Some(vao));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(instances));
    // The pointers are captured in the VAO, so a draw only rebinds it and
    // re-points them per batch.
    for (location, _) in ATTRIBUTES {
      gl.enable_vertex_attrib_array(location);
      // Every attribute is per instance: the corners come from gl_VertexID.
      gl.vertex_attrib_divisor(location, 1);
    }
    point_attributes(&gl, 0);
    gl.bind_vertex_array(None);
    gl.bind_buffer(glow::ARRAY_BUFFER, None);
    Ok(Executor { gl, program, size_uniform, vao, instances, sampler_linear, sampler_nearest, glyph_uniforms, ramp })
  }

  /// Delete `names`: the textures whose last handle dropped since the last
  /// draw (see `super::DeletionQueue`). On this thread, context current.
  pub(super) fn reclaim(&self, names: Vec<glow::NativeTexture>) {
    for name in names {
      unsafe { self.gl.delete_texture(name) }
    }
  }

  /// Draw `list` into `target`, cleared first, both inside the scissor the
  /// walk reports; the scissor test is off again on return. Sets every
  /// piece of state it uses on each draw and caches none across draws: the
  /// other passes on this context save and restore around their own work,
  /// and the contract lets a draw leave state changed. The texture units and the
  /// sampler bindings are set per batch and cleared after. Reports the
  /// instances written and the draw calls issued, zeros for an empty list.
  pub(super) fn draw(&mut self, target: Target, list: &DisplayList) -> Result<DrawReport, String> {
    let walked = walk::walk(list, target.size);
    let instances = &walked.instances;
    let (width, height) = (target.size.width as i32, target.size.height as i32);
    let gl = &self.gl;
    unsafe {
      gl.bind_framebuffer(glow::FRAMEBUFFER, target.framebuffer);
      gl.viewport(0, 0, width, height);
      // The scissor is the union of the instances' clips (walk.rs), so a
      // patch frame pays GPU, and on a tiler tile loads and stores, only
      // inside the patch. The clear is scissored too: every caller in
      // gl/draw.rs cleared the full target before the draw, so the pixels
      // outside are already clear. An empty list (no scissor) clears the
      // full target, as the contract says a draw does.
      let full = Box2D::new(Point2D::new(0, 0), Point2D::new(width, height));
      match walked.scissor.filter(|s| *s != full) {
        Some(s) => {
          gl.enable(glow::SCISSOR_TEST);
          // GL scissor is in window coordinates, y up from the bottom.
          gl.scissor(s.min.x, height - s.max.y, s.width(), s.height());
        }
        None => gl.disable(glow::SCISSOR_TEST),
      }
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
      // No instances means no scissor: the test is already off here.
      if instances.is_empty() {
        return Ok(DrawReport::default());
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
      // One draw per batch: everything shares the program, and a batch
      // ends where the texture or its sampling changes, or a glyph run's
      // policy or gradient (walk.rs); stage 2 batches further. GLES 3.0
      // has no base instance, so a batch is drawn by re-pointing the
      // attributes at its first instance.
      let glyphs = walked.batches.iter().any(|b| b.policy.is_some());
      if glyphs {
        gl.active_texture(glow::TEXTURE0 + RAMP_UNIT);
        gl.bind_texture(glow::TEXTURE_2D, Some(self.ramp));
        gl.bind_sampler(RAMP_UNIT, Some(self.sampler_linear));
      }
      gl.active_texture(glow::TEXTURE0 + TEXTURE_UNIT);
      for batch in &walked.batches {
        point_attributes(gl, batch.start);
        if let Some(policy) = batch.policy {
          self.glyph_state(policy, batch.gradient.as_deref());
        }
        match batch.texture {
          Some((name, sampling)) => {
            let sampler = match sampling {
              TextureSampling::Linear => self.sampler_linear,
              TextureSampling::Nearest => self.sampler_nearest,
            };
            gl.bind_texture(glow::TEXTURE_2D, Some(name));
            gl.bind_sampler(TEXTURE_UNIT, Some(sampler));
          }
          None => {
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.bind_sampler(TEXTURE_UNIT, None);
          }
        }
        gl.draw_arrays_instanced(glow::TRIANGLES, 0, VERTICES_PER_QUAD, batch.count as i32);
      }
      if glyphs {
        gl.active_texture(glow::TEXTURE0 + RAMP_UNIT);
        gl.bind_sampler(RAMP_UNIT, None);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.active_texture(glow::TEXTURE0 + TEXTURE_UNIT);
      }
      gl.bind_sampler(TEXTURE_UNIT, None);
      gl.bind_texture(glow::TEXTURE_2D, None);
      // Leave the VAO pointing at the first instance, as `new` set it.
      point_attributes(gl, 0);
      gl.bind_vertex_array(None);
      gl.bind_buffer(glow::ARRAY_BUFFER, None);
      gl.use_program(None);
      // The callers' resolve blit after the draw honours the scissor, and
      // they expect it off, as Impeller left it.
      gl.disable(glow::SCISSOR_TEST);
    }
    // The framebuffer stays bound as the target: the callers in
    // gl/draw.rs restore their own.
    Ok(super::report(&walked))
  }
}

impl Executor {
  // A glyph batch's uniforms: `policy`, and `gradient` with its ramp
  // uploaded into the ramp texture (bound on RAMP_UNIT by `draw`), or no
  // gradient. The program is in use and TEXTURE_UNIT is the active unit,
  // as `draw` left them; the active unit is TEXTURE_UNIT again after.
  unsafe fn glyph_state(&self, policy: CoveragePolicy, gradient: Option<&GlyphGradient>) {
    let (gl, u) = (&self.gl, &self.glyph_uniforms);
    gl.uniform_1_i32(Some(&u.mode), policy.mode as i32);
    gl.uniform_1_f32(Some(&u.gamma), policy.gamma);
    gl.uniform_1_f32(Some(&u.contrast), policy.contrast);
    let [r0, r1, r2, r3] = policy.gamma_ratios();
    gl.uniform_4_f32(Some(&u.gamma_ratios), r0, r1, r2, r3);
    let Some(g) = gradient else {
      gl.uniform_1_i32(Some(&u.gradient), NO_GRADIENT);
      return;
    };
    let mut bytes = Vec::with_capacity(RAMP_WIDTH * BYTES_PER_RAMP_TEXEL);
    for texel in g.ramp.iter().take(RAMP_WIDTH) {
      bytes.extend_from_slice(texel);
    }
    bytes.resize(RAMP_WIDTH * BYTES_PER_RAMP_TEXEL, 0);
    gl.active_texture(glow::TEXTURE0 + RAMP_UNIT);
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
    gl.active_texture(glow::TEXTURE0 + TEXTURE_UNIT);
    gl.uniform_1_i32(Some(&u.gradient), g.kind as i32);
    let ([x0, x1, x2], [y0, y1, y2]) = (g.x_row, g.y_row);
    gl.uniform_3_f32(Some(&u.gradient_x), x0, x1, x2);
    gl.uniform_3_f32(Some(&u.gradient_y), y0, y1, y2);
    gl.uniform_1_i32(Some(&u.tile), tile_uniform(g.tile));
  }
}

// The fragment stage's `uTile` value for a tile mode.
fn tile_uniform(tile: TileMode) -> i32 {
  match tile {
    TileMode::Clamp => TILE_CLAMP,
    TileMode::Repeat => TILE_REPEAT,
    TileMode::Mirror => TILE_MIRROR,
    TileMode::Decal => TILE_DECAL,
  }
}

// The location of the shape program's uniform `name`; an error naming it
// when the program has none.
unsafe fn uniform(gl: &glow::Context, program: glow::Program, name: &str) -> Result<glow::UniformLocation, String> {
  gl.get_uniform_location(program, name).ok_or_else(|| format!("shape program has no {name} uniform"))
}

// The gradient ramp texture: `RAMP_WIDTH` x 1 rgba8, a blank row until a
// gradient batch writes it. Its own parameters are linear and clamped, but
// `draw` samples it through the linear sampler object, as every texture
// here is sampled.
unsafe fn create_ramp(gl: &glow::Context) -> Result<glow::Texture, String> {
  let ramp = gl.create_texture().map_err(|e| format!("shape ramp texture: {e}"))?;
  gl.bind_texture(glow::TEXTURE_2D, Some(ramp));
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
  gl.bind_texture(glow::TEXTURE_2D, None);
  Ok(ramp)
}

// Point the instanced attributes at instance `first` of the bound buffer.
unsafe fn point_attributes(gl: &glow::Context, first: usize) {
  let stride = std::mem::size_of::<Instance>();
  for (location, offset) in ATTRIBUTES {
    gl.vertex_attrib_pointer_f32(
      location,
      FLOATS_PER_ATTRIBUTE,
      glow::FLOAT,
      false,
      stride as i32,
      (first * stride + offset) as i32,
    );
  }
}

// A sampler object filtering with `filter` for both minification and
// magnification, clamped at the edges. No mip selection: a texture quad's
// Linear is bilinear at level 0, as Impeller's was.
unsafe fn create_sampler(gl: &glow::Context, filter: u32, label: &str) -> Result<glow::Sampler, String> {
  let sampler = gl.create_sampler().map_err(|e| format!("shape {label} sampler: {e}"))?;
  gl.sampler_parameter_i32(sampler, glow::TEXTURE_MIN_FILTER, filter as i32);
  gl.sampler_parameter_i32(sampler, glow::TEXTURE_MAG_FILTER, filter as i32);
  gl.sampler_parameter_i32(sampler, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
  gl.sampler_parameter_i32(sampler, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
  Ok(sampler)
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
