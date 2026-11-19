//! The GL half of a draw: the fill, shape and glyph programs (the fill and
//! the shape program each with a gradient variant), the paint shaders'
//! programs linked behind anvil's vertex stage, the instance buffer under
//! the VAO the executor owns, and the draw that uploads a list's instances
//! (`super::walk`) and issues them, a batch through the program its kind
//! needs. A gradient evaluates per fragment in device space, from
//! `vDevice` through the rows the walk composed (`walk::GradientDraw`),
//! with one ramp texture rewritten per gradient batch. Raster thread only,
//! with the context current.

use super::record::DisplayList;
use super::walk::{self, Batch, BatchProgram, GradientDraw, Instance, PaintDraw, Walked};
use super::Freed;
use crate::contract::{DrawReport, Target};
use crate::{CoveragePolicy, RAMP_WIDTH};
use crate::{TextureSampling, TextureWrap, TileMode, UniformInfo, UniformKind};
use euclid::default::{Box2D, Point2D};
use glow::HasContext;
use std::collections::HashMap;
use std::os::raw::c_void;

/// Two triangles a quad, their corners derived from `gl_VertexID`.
const VERTICES_PER_QUAD: i32 = 6;
/// Floats per instance attribute: each is a vec4.
const FLOATS_PER_ATTRIBUTE: i32 = 4;

// The attribute locations. They must match the `layout(location = N)`
// literals in VERTEX_COMMON and SHAPE_VERTEX.
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
/// The texture unit `uRamp` samples: a gradient draw's ramp.
const RAMP_UNIT: u32 = 1;
/// Bytes per rgba8 ramp texel.
const BYTES_PER_RAMP_TEXEL: usize = 4;
/// The texture units a paint shader's inputs may take, from unit 0 on: GLES
/// 3.0's guaranteed minimum of fragment texture units. An input past it is
/// skipped.
const MAX_PAINT_INPUTS: u32 = 16;

/// `uGradient` for a glyph run without a gradient; a gradient's is its
/// `GradientKind` discriminant. GRADIENT_COMMON's `const int NO_GRADIENT`
/// must match it.
const NO_GRADIENT: i32 = 0;
// `uTile` per tile mode. The `const int TILE_*` declarations in
// GRADIENT_COMMON must match them.
const TILE_CLAMP: i32 = 0;
const TILE_REPEAT: i32 = 1;
const TILE_MIRROR: i32 = 2;
const TILE_DECAL: i32 = 3;

// The first lines of every shader source here.
pub(crate) const SOURCE_HEADER: &str = "#version 300 es\nprecision highp float;\n";

// Right after SOURCE_HEADER in a fragment source, it selects the gradient
// variant of the fill and the shape program: the same source, its color
// from `gradientAt` (GRADIENT_COMMON).
const GRADIENT_DEFINE: &str = "#define GRADIENT\n";

// A target's logical (0, 0) is its top: it maps to clip (-1, +1) for every
// target, window and texture alike. Window lists are drawn unflipped and
// offscreen lists arrive pre-flipped (raster::offscreen::flip_for_fbo),
// which is what put texel row 0 at the top of a sampled image under
// Impeller; gl/program.rs describes the same row 0 is top convention for
// alloy's passes, and the plan (okf/plans/own-renderer.md) pins it.
// Texture sampling reads vUv with v = 0 at texel row 0, no flip.
//
// The vertex stage the programs share: the instance attributes they all
// read, the outputs their fragment stages read, and the quad's placement.
// A program reads only the attributes it declares, so the shape program
// adds `aRadii` and all draw from the one VAO; the linker drops the outputs
// a fragment stage never declares. The layout(location = N) literals must
// match the ATTR_* constants.
const VERTEX_COMMON: &str = r"
layout(location = 0) in vec4 aRect;      // x, y, w, h in local space
layout(location = 1) in vec4 aLinear;    // m11, m12, m21, m22
layout(location = 2) in vec4 aOffset;    // m31, m32 (the translation), kind, stroke width
layout(location = 3) in vec4 aColor;     // premultiplied; straight alpha for a glyph
layout(location = 4) in vec4 aTex;       // u0, v0, u1, v1 for a textured instance; cap, length for a line
layout(location = 5) in vec4 aClip;      // x0, y0, x1, y1 in device pixels
layout(location = 7) in vec4 aRClip;     // the rounded clip in device pixels: x0, y0, x1, y1
layout(location = 8) in vec4 aRClipRadii; // its corner radii in device pixels; all zero: none
uniform vec2 uSize;                      // the target in device pixels
out vec2 vDevice;                        // device pixels, y down
out vec2 vUv;
flat out vec4 vColor;
flat out vec4 vClip;
flat out vec4 vRClip;
flat out vec4 vRClipRadii;
// The vertex's corner of the quad, 0 or 1 per axis.
vec2 quadCorner() {
  // One quad per instance, so this is gl_VertexID; the modulo keeps the
  // shader right if a later step draws several quads per instance.
  int corner = gl_VertexID - (gl_VertexID / 6) * 6;
  // Two triangles: (0,0) (1,0) (0,1) and (0,1) (1,0) (1,1).
  return vec2(
    (corner == 1 || corner == 4 || corner == 5) ? 1.0 : 0.0,
    (corner == 2 || corner == 3 || corner == 5) ? 1.0 : 0.0);
}
// Place the vertex at the quad's corner c: its position, and the outputs
// both fragment stages read. Returns the point in the op's space.
vec2 place(vec2 c) {
  vec2 p = aRect.xy + c * aRect.zw;
  // euclid's row-vector convention: device = p * M, M = [m11 m12; m21 m22] + t.
  vec2 device = vec2(
    p.x * aLinear.x + p.y * aLinear.z + aOffset.x,
    p.x * aLinear.y + p.y * aLinear.w + aOffset.y);
  // Logical (0, 0) is the top of the target: clip y = +1.
  gl_Position = vec4(device.x / uSize.x * 2.0 - 1.0, 1.0 - device.y / uSize.y * 2.0, 0.0, 1.0);
  vDevice = device;
  vUv = aTex.xy + c * (aTex.zw - aTex.xy);
  vColor = aColor;
  vClip = aClip;
  vRClip = aRClip;
  vRClipRadii = aRClipRadii;
  return p;
}
";

// The shape program's vertex stage, after SOURCE_HEADER and VERTEX_COMMON.
const SHAPE_VERTEX: &str = r"
layout(location = 6) in vec4 aRadii;     // corner radii in local pixels: tl, tr, br, bl
out vec2 vLocal;                         // position within the shape, local pixels from its origin
flat out vec4 vShape;                    // w, h, kind, stroke width
flat out vec4 vParams;                   // the line kind's cap and length
flat out vec4 vRadii;
void main() {
  vec2 c = quadCorner();
  place(c);
  vLocal = c * aRect.zw;
  vShape = vec4(aRect.zw, aOffset.zw);
  vParams = aTex;
  vRadii = aRadii;
}
";

// The vertex stage of the fill programs and of the glyph program, after
// SOURCE_HEADER and VERTEX_COMMON: the quad placed, nothing more (a
// gradient reads `vDevice`).
const FILL_VERTEX: &str = r"
void main() {
  place(quadCorner());
}
";

// A paint shader's vertex stage, after SOURCE_HEADER and VERTEX_COMMON:
// `vUV` is the quad's corner, 0..1 over the draw's rect, top-left origin.
const PAINT_VERTEX: &str = r"
out vec2 vUV;
void main() { vec2 c = quadCorner(); place(c); vUV = c; }
";

// What a paint shader's source reads beside FRAGMENT_COMMON (`fragColor`
// among it): the corner varying and the quad's size in device pixels.
const PAINT_FRAGMENT_PREAMBLE: &str = r"
in vec2 vUV;
uniform vec2 iResolution;
";

// The name the app's `main` is renamed to, by macro, so the epilogue's
// `main` calls it. A name no app source would use; the call in
// PAINT_FRAGMENT_EPILOGUE must match it.
const PAINT_MAIN: &str = "solidrt_paint_main";

// After the app's source: its main, then the clip factor the glyph program
// applies, a multiply, never a discard. `fragColor` is premultiplied, so
// the one factor fades all four channels.
pub(crate) const PAINT_FRAGMENT_EPILOGUE: &str = r"
void main() {
  solidrt_paint_main();
  float inside = float(all(greaterThanEqual(vDevice, vClip.xy)) && all(lessThan(vDevice, vClip.zw)));
  fragColor *= inside * roundedClip();
}
";

// The fragment stage the shape and glyph programs (and the fill program's
// gradient variant) share: the inputs VERTEX_COMMON writes,
// the analytic coverage of a distance and the rounded clip over it. One
// text, so a glyph under a rounded clip fades exactly as a shape does.
const FRAGMENT_COMMON: &str = r"
in vec2 vDevice;
in vec2 vUv;
flat in vec4 vColor;
flat in vec4 vClip;
flat in vec4 vRClip;      // the rounded clip in device pixels: x0, y0, x1, y1
flat in vec4 vRClipRadii; // its corner radii in device pixels; all zero: none
out vec4 fragColor;
// The smallest screen-space footprint a distance is divided by: a
// constant distance (deep inside) has a zero gradient and must read as
// covered.
const float FOOTPRINT_FLOOR = 1e-6;
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
// The radii are flat, so every fragment of a quad takes the same path and
// the derivatives stay defined.
float roundedClip() {
  if (!any(greaterThan(vRClipRadii, vec4(0.0)))) return 1.0;
  vec2 center = (vRClip.xy + vRClip.zw) * 0.5;
  vec2 half_size = (vRClip.zw - vRClip.xy) * 0.5;
  return coverage(roundedBox(vDevice - center, half_size, vRClipRadii));
}
";

// A gradient's evaluation, after FRAGMENT_COMMON in the glyph program and
// the gradient variants: the batch's ramp and rows (walk.rs GradientDraw,
// set by `gradient_state`) and the color at a device pixel. The gradient
// kinds are GradientKind's discriminants (exec.rs NO_GRADIENT for none),
// the tile modes exec.rs TILE_*: they must match.
const GRADIENT_COMMON: &str = r"
uniform sampler2D uRamp;
uniform int uGradient;   // GradientKind's discriminant; NO_GRADIENT for a solid glyph run
uniform vec3 uGradientX; // device pixel (x, y, 1) to the parameter space's x'
uniform vec3 uGradientY; // and y'
uniform int uTile;
const int NO_GRADIENT = 0;
const int GRADIENT_LINEAR = 1;
const int TILE_REPEAT = 1;
const int TILE_MIRROR = 2;
const int TILE_DECAL = 3;
// The gradient's straight-alpha color at the device pixel `device`: the
// rows map it to the parameter space, the tile mode folds the parameter,
// the decal mode's outside is alpha 0.
vec4 gradientAt(vec2 device) {
  vec3 p3 = vec3(device, 1.0);
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
  return vec4(g.rgb, g.a * inside);
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
// coverage into the premultiplied texel. Glyphs never reach this program
// (GLYPH_FRAGMENT draws them). The gradient variant (GRADIENT defined)
// multiplies its coverage into the gradient's color at the fragment's
// device pixel, premultiplied; it draws no textured instance.
//
// After SOURCE_HEADER and FRAGMENT_COMMON; the gradient variant after
// SOURCE_HEADER, GRADIENT_DEFINE, FRAGMENT_COMMON and GRADIENT_COMMON.
const SHAPE_FRAGMENT: &str = r"
uniform sampler2D uTexture;
in vec2 vLocal;
flat in vec4 vShape;      // quad w, quad h, kind (+ KIND_TEXTURED), stroke width (0 = fill)
flat in vec4 vParams;     // line kind: cap, length
flat in vec4 vRadii;      // corner radii, local pixels: tl, tr, br, bl
// The shape kinds, the textured flag and the line caps, as the instance
// carries them (walk.rs KIND_* and CAP_*): they must match.
const int KIND_RECT = 0;
const int KIND_OVAL = 1;
const int KIND_LINE = 2;
const int KIND_ROUNDED_RECT = 4;
const int KIND_TEXTURED = 8;
const int CAP_ROUND = 2;
// Local pixels a rounded rect's quad extends past its edge and its
// stroke's half width (walk.rs ROUNDED_QUAD_PAD must match).
const float ROUNDED_QUAD_PAD = 1.0;
// A radius a stroke wider than its oval would drive to zero or below;
// keeps the normalization finite.
const float OVAL_RADIUS_FLOOR = 1e-3;
// How close to the center the normalized point may be before the 0/0
// form of the oval distance is replaced by deep inside.
const float OVAL_CENTER_EPS = 1e-4;
// A distance that reads as fully inside whatever the footprint: used
// where the quad's own edges are the shape's edges and MSAA covers them,
// and negated, as far outside a rect stroke's hole. Large and finite,
// never -INFINITY.
const float INSIDE = -1e6;
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
#ifdef GRADIENT
  // The ramp's straight-alpha color, premultiplied; vColor is the op's
  // alpha in every channel, so the multiply fades it as a whole.
  vec4 g = gradientAt(vDevice);
  vec4 color = vec4(g.rgb * g.a, g.a) * vColor;
#else
  // Premultiplied texels; vColor is then the op's alpha in every channel,
  // so the multiply fades the texel as a whole.
  vec4 color = textured ? texture(uTexture, vUv) * vColor : vColor;
#endif
  fragColor = color * c;
}
";

// A plain rect fill (walk.rs `plain_fill`: a rect, untextured, unstroked,
// under no rounded clip) has no edge the fragment stage decides: the quad's
// edges are MSAA's and its coverage is 1 inside. A program of its own,
// because on a Midgard GPU (the Mali-T860) the shape program costs every
// fragment its nine flat and four smooth varyings, its kind branches, its
// discard and the derivatives in `footprint`, and a full-window background
// pays all of that for none of it. It clips by multiplying, never discards,
// and reads no derivative. The gradient variant (GRADIENT defined) takes
// its color from the gradient at the fragment's device pixel,
// premultiplied and faded by vColor, the op's alpha in all four channels;
// FRAGMENT_COMMON then declares the inputs, and its derivative functions
// are never called.
//
// After SOURCE_HEADER only; the gradient variant after SOURCE_HEADER,
// GRADIENT_DEFINE, FRAGMENT_COMMON and GRADIENT_COMMON.
const FILL_FRAGMENT: &str = r"
#ifndef GRADIENT
in vec2 vDevice;
flat in vec4 vColor;
flat in vec4 vClip;
out vec4 fragColor;
#endif
void main() {
  // The clip box as a factor, not a discard, as in GLYPH_FRAGMENT: a
  // fragment outside writes zero, which premultiplied source-over leaves
  // the target unchanged by.
  float inside = float(all(greaterThanEqual(vDevice, vClip.xy)) && all(lessThan(vDevice, vClip.zw)));
#ifdef GRADIENT
  vec4 g = gradientAt(vDevice);
  fragColor = vec4(g.rgb * g.a, g.a) * vColor * inside;
#else
  fragColor = vColor * inside;
#endif
}
";

// A glyph has no analytic coverage: its quad is axis-aligned at whole
// pixels and the atlas's red channel carries the coverage. Its color is
// straight alpha, the quad's own or the gradient's at the fragment's
// device pixel (`gradientAt`, through the rows the walk composed into
// device space), and the batch's coverage policy
// turns the coverage into color before the fragment premultiplies, as
// `draw_glyphs` asks (contract.rs) and `CoverageMode` defines each mode
// (glyphs.rs). A program of its own, so a
// glyph fragment carries none of the shape program's varyings or kind
// branches: it clips by multiplying, never discards, and reads the
// derivatives only under a rounded clip.
//
// After SOURCE_HEADER, FRAGMENT_COMMON and GRADIENT_COMMON.
const GLYPH_FRAGMENT: &str = r"
uniform sampler2D uTexture; // the atlas
uniform int uMode;
uniform float uGamma;
uniform float uContrast;
uniform vec4 uGammaRatios;
// The coverage modes are CoverageMode's discriminants (0 is naive): they
// must match.
const int MODE_LINEAR_LIGHT = 1;
const int MODE_POLARITY_REMAP = 2;
const int MODE_POLARITY_LINEAR = 3;
const int MODE_DIRECTWRITE = 4;
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
// A glyph fragment: the run's straight-alpha color, the atlas's coverage
// under the policy, premultiplied, and `clip` (the clips' factor)
// multiplied in, as the clip fades a text layer's texels.
vec4 glyph(float clip) {
  vec4 color = vColor;
  if (uGradient != NO_GRADIENT) {
    vec4 g = gradientAt(vDevice);
    color = vec4(g.rgb, g.a * vColor.a);
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
  float a = c * color.a * clip;
  return vec4(color.rgb * a, a);
}
void main() {
  // The clip box as a factor, not a discard: a discard anywhere in a
  // program costs every fragment its early kill. A fragment outside
  // writes zero, which source-over leaves the target unchanged by.
  float inside = float(all(greaterThanEqual(vDevice, vClip.xy)) && all(lessThan(vDevice, vClip.zw)));
  fragColor = glyph(inside * roundedClip());
}
";

/// The fragment source a paint shader compiles from the app's `src`:
/// anvil's header, the common fragment inputs and the paint preamble, the
/// source with its `main` renamed by macro under `#line 1` (so a compile
/// error's line numbers are the app's), and the epilogue that calls it and
/// applies the clips. A source with a `#version` line is refused: a paint
/// shader is fragment-only GLSL ES, and anvil supplies the header.
pub(crate) fn paint_fragment_source(src: &str) -> Result<String, String> {
  // A `#version` directive, not the word in a comment: the same test alloy's
  // preamble injection makes, so a shader-texture source is accepted here
  // exactly when it is accepted there.
  if src.lines().any(|line| line.trim_start().starts_with("#version")) {
    return Err(
      "A paint shader source is fragment-only GLSL ES without a #version line: anvil supplies the header".to_string(),
    );
  }
  let renamed = format!("#define main {PAINT_MAIN}\n#line 1\n{src}\n#undef main\n");
  Ok([SOURCE_HEADER, FRAGMENT_COMMON, PAINT_FRAGMENT_PREAMBLE, &renamed, PAINT_FRAGMENT_EPILOGUE].concat())
}

/// A paint shader's active uniforms by bare name: location, kind and
/// array size.
type UniformTable = HashMap<String, (glow::UniformLocation, UniformKind, usize)>;

/// A linked paint shader program and what a draw sets on it, as reflected
/// at link: `uSize`, `iResolution` when the source reads it, and every
/// active uniform by name (`uniforms`, for params and inputs; `infos`, the
/// same as the contract's `PaintShader::uniforms` returns them).
pub(super) struct PaintProgram {
  pub(super) program: glow::Program,
  size: glow::UniformLocation,
  resolution: Option<glow::UniformLocation>,
  uniforms: UniformTable,
  pub(super) infos: Vec<UniformInfo>,
}

impl PaintProgram {
  /// `program` with no uniforms and a placeholder `uSize`, for a test
  /// handle no GL ever draws.
  #[cfg(any(test, feature = "oplog"))]
  pub(super) fn without_gl(program: glow::Program) -> PaintProgram {
    PaintProgram {
      program,
      size: glow::NativeUniformLocation(0),
      resolution: None,
      uniforms: HashMap::new(),
      infos: Vec::new(),
    }
  }
}

/// The GL half: the five programs, the instance buffer and the draw. Lives
/// as long as the renderer, which the raster thread drops before it releases
/// the context (raster/recover.rs), so the GL names here die with it and
/// Drop deletes nothing.
pub(super) struct Executor {
  gl: glow::Context,
  /// Every fill batch without a gradient: plain rect fills.
  fill: Program,
  /// A fill batch with a gradient: one plain rect fill.
  fill_gradient: Program,
  /// Every shape batch without a gradient: shapes, images, textures, and
  /// the small plain fills among them.
  shape: Program,
  /// A shape batch with a gradient: one shape.
  shape_gradient: Program,
  /// Every glyph batch (a batch with a coverage policy).
  glyph: Program,
  vao: glow::VertexArray,
  instances: glow::Buffer,
  /// The sampler objects per (sampling, wrap), created on first use, so a
  /// bound texture's own parameter state is never read or written: on this
  /// shared context only a bound sampler object is a reliable contract
  /// (raster/offscreen.rs says the same for the node shader passes). The
  /// texture batches and the ramp sample clamped; a paint shader's input
  /// takes its own wrap.
  samplers: HashMap<(TextureSampling, TextureWrap), glow::Sampler>,
  /// The glyph runs' policy uniforms, set per glyph batch.
  glyph_uniforms: GlyphUniforms,
  /// The gradient uniforms of the three programs that draw a gradient,
  /// set per gradient batch (and `uGradient` per solid glyph batch).
  fill_gradient_uniforms: GradientUniforms,
  shape_gradient_uniforms: GradientUniforms,
  glyph_gradient_uniforms: GradientUniforms,
  /// The gradient ramp (`RAMP_WIDTH` x 1 rgba8, straight-alpha sRGB),
  /// rewritten per gradient batch and bound on `RAMP_UNIT` for every draw
  /// with a glyph or a gradient batch; a solid batch never samples it.
  ramp: glow::Texture,
}

/// A linked program and its `uSize`, the target in device pixels.
struct Program {
  program: glow::Program,
  size: glow::UniformLocation,
}

/// The uniforms a glyph batch sets on the glyph program for its coverage
/// policy.
struct GlyphUniforms {
  mode: glow::UniformLocation,
  gamma: glow::UniformLocation,
  contrast: glow::UniformLocation,
  gamma_ratios: glow::UniformLocation,
}

/// The uniforms a gradient batch sets on its program (GRADIENT_COMMON):
/// the kind (or NO_GRADIENT), the rows and the tile mode.
struct GradientUniforms {
  gradient: glow::UniformLocation,
  gradient_x: glow::UniformLocation,
  gradient_y: glow::UniformLocation,
  tile: glow::UniformLocation,
}

impl GradientUniforms {
  // The gradient uniforms of `program` (`label` names it).
  unsafe fn of(gl: &glow::Context, program: &Program, label: &str) -> Result<GradientUniforms, String> {
    let p = program.program;
    Ok(GradientUniforms {
      gradient: uniform(gl, p, label, "uGradient")?,
      gradient_x: uniform(gl, p, label, "uGradientX")?,
      gradient_y: uniform(gl, p, label, "uGradientY")?,
      tile: uniform(gl, p, label, "uTile")?,
    })
  }
}

impl Executor {
  /// Over the context current on the calling thread; compiles the five
  /// programs now (every frame draws shapes and most draw text, so the
  /// first frame must not pay them).
  ///
  /// # Safety
  /// As `contract::Renderer::new`: the context stays current on this
  /// thread for the executor's life, and `proc_address` resolves its entry
  /// points.
  pub(super) unsafe fn new(proc_address: &mut dyn FnMut(&str) -> *mut c_void) -> Result<Self, String> {
    let gl = glow::Context::from_loader_function(|name| proc_address(name) as *const c_void);
    let fill =
      Program::link(&gl, "fill", &[SOURCE_HEADER, VERTEX_COMMON, FILL_VERTEX], &[SOURCE_HEADER, FILL_FRAGMENT])?;
    let fill_gradient = Program::link(
      &gl,
      "fill gradient",
      &[SOURCE_HEADER, VERTEX_COMMON, FILL_VERTEX],
      &[SOURCE_HEADER, GRADIENT_DEFINE, FRAGMENT_COMMON, GRADIENT_COMMON, FILL_FRAGMENT],
    )?;
    let shape = Program::link(
      &gl,
      "shape",
      &[SOURCE_HEADER, VERTEX_COMMON, SHAPE_VERTEX],
      &[SOURCE_HEADER, FRAGMENT_COMMON, SHAPE_FRAGMENT],
    )?;
    let shape_gradient = Program::link(
      &gl,
      "shape gradient",
      &[SOURCE_HEADER, VERTEX_COMMON, SHAPE_VERTEX],
      &[SOURCE_HEADER, GRADIENT_DEFINE, FRAGMENT_COMMON, GRADIENT_COMMON, SHAPE_FRAGMENT],
    )?;
    let glyph = Program::link(
      &gl,
      "glyph",
      &[SOURCE_HEADER, VERTEX_COMMON, FILL_VERTEX],
      &[SOURCE_HEADER, FRAGMENT_COMMON, GRADIENT_COMMON, GLYPH_FRAGMENT],
    )?;
    let (gp, label) = (glyph.program, "glyph");
    let glyph_uniforms = GlyphUniforms {
      mode: uniform(&gl, gp, label, "uMode")?,
      gamma: uniform(&gl, gp, label, "uGamma")?,
      contrast: uniform(&gl, gp, label, "uContrast")?,
      gamma_ratios: uniform(&gl, gp, label, "uGammaRatios")?,
    };
    let fill_gradient_uniforms = GradientUniforms::of(&gl, &fill_gradient, "fill gradient")?;
    let shape_gradient_uniforms = GradientUniforms::of(&gl, &shape_gradient, "shape gradient")?;
    let glyph_gradient_uniforms = GradientUniforms::of(&gl, &glyph, "glyph")?;
    // A sampler uniform names a unit, not a texture: set once, the unit
    // never changes. The shape and glyph programs sample `uTexture` (an
    // image, a texture, the atlas) on the one unit; the gradient programs
    // and the glyph program sample the ramp on its own. The fill program
    // samples nothing, and the shape program's gradient variant no
    // `uTexture` (it draws no image, so the compiler drops the sampler).
    for (program, label) in [(shape.program, "shape"), (glyph.program, "glyph")] {
      let texture_uniform = uniform(&gl, program, label, "uTexture")?;
      gl.use_program(Some(program));
      gl.uniform_1_i32(Some(&texture_uniform), TEXTURE_UNIT as i32);
    }
    for (program, label) in
      [(fill_gradient.program, "fill gradient"), (shape_gradient.program, "shape gradient"), (glyph.program, "glyph")]
    {
      let ramp_uniform = uniform(&gl, program, label, "uRamp")?;
      gl.use_program(Some(program));
      gl.uniform_1_i32(Some(&ramp_uniform), RAMP_UNIT as i32);
    }
    gl.use_program(None);
    let ramp = create_ramp(&gl)?;
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
    let samplers = HashMap::new();
    Ok(Executor {
      gl,
      fill,
      fill_gradient,
      shape,
      shape_gradient,
      glyph,
      vao,
      instances,
      samplers,
      glyph_uniforms,
      fill_gradient_uniforms,
      shape_gradient_uniforms,
      glyph_gradient_uniforms,
      ramp,
    })
  }

  /// Delete `names`: the textures and paint shader programs whose last
  /// handle dropped since the last draw (see `super::DeletionQueue`). On
  /// this thread, context current.
  pub(super) fn reclaim(&self, names: Vec<Freed>) {
    for name in names {
      match name {
        Freed::Texture(texture) => unsafe { self.gl.delete_texture(texture) },
        Freed::Program(program) => unsafe { self.gl.delete_program(program) },
      }
    }
  }

  /// The paint shader program of the app's fragment source `fragment_src`
  /// (`paint_fragment_source`) behind anvil's vertex stage, compiled,
  /// linked and reflected. The error names the stage that failed, with the
  /// GL log.
  pub(super) fn link_paint(&self, fragment_src: &str) -> Result<PaintProgram, String> {
    let fragment = paint_fragment_source(fragment_src)?;
    let vertex = [SOURCE_HEADER, VERTEX_COMMON, PAINT_VERTEX].concat();
    let gl = &self.gl;
    unsafe {
      // A source without a `main` fails at the epilogue's call to the
      // renamed one: the GL log then names anvil's hidden symbol, which no
      // app can act on, so say what the source is missing instead.
      let program = link_program(gl, "paint", &vertex, &fragment).map_err(|e| {
        if e.contains(PAINT_MAIN) {
          format!("paint fragment shader: the source must define void main() ({e})")
        } else {
          e
        }
      })?;
      let size = match uniform(gl, program, "paint", "uSize") {
        Ok(size) => size,
        Err(e) => {
          gl.delete_program(program);
          return Err(e);
        }
      };
      // Optional: the compiler drops it when the source never reads it.
      let resolution = gl.get_uniform_location(program, "iResolution");
      let (uniforms, infos) = reflect_uniforms(gl, program);
      Ok(PaintProgram { program, size, resolution, uniforms, infos })
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
    unsafe { self.create_samplers(&walked) }?;
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
      for program in [&self.glyph, &self.fill, &self.fill_gradient, &self.shape_gradient, &self.shape] {
        gl.use_program(Some(program.program));
        gl.uniform_2_f32(Some(&program.size), width as f32, height as f32);
      }
      // The shape program is in use, as the loop left it. None after a
      // paint batch: its program is the shader's, so the next batch binds.
      let mut in_use = Some(self.shape.program);
      gl.bind_vertex_array(Some(self.vao));
      gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.instances));
      // Sound: Instance is repr(C) with every field an f32, so it has no
      // padding (walk.rs asserts its size) and any byte view of it is
      // initialized; the slice borrows `instances` for the upload only.
      let bytes =
        std::slice::from_raw_parts(instances.as_ptr() as *const u8, instances.len() * std::mem::size_of::<Instance>());
      gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);
      // One draw per batch: a batch ends where the texture or its sampling
      // changes, where fills, shapes and text meet, at a glyph run's
      // policy, or around a gradient draw (walk.rs). A batch draws through
      // its program, the gradient variant when it carries a gradient,
      // switched only where the program changes. GLES 3.0 has no base
      // instance, so a batch is drawn by re-pointing the attributes at its
      // first instance.
      let ramped = walked.batches.iter().any(needs_ramp);
      if ramped {
        self.bind_ramp();
      }
      gl.active_texture(glow::TEXTURE0 + TEXTURE_UNIT);
      for batch in &walked.batches {
        point_attributes(gl, batch.start);
        if let Some(paint) = &batch.paint {
          self.draw_paint(paint, [width as f32, height as f32], batch.count);
          // The paint inputs may have taken the ramp's unit.
          if ramped {
            self.bind_ramp();
          }
          in_use = None;
          continue;
        }
        let program = match (batch.program, batch.gradient.is_some()) {
          (BatchProgram::Fill, false) => &self.fill,
          (BatchProgram::Fill, true) => &self.fill_gradient,
          (BatchProgram::Shape, false) => &self.shape,
          (BatchProgram::Shape, true) => &self.shape_gradient,
          (BatchProgram::Glyph, _) => &self.glyph,
          (BatchProgram::Paint, _) => unreachable!("a paint batch carries its draw (walk.rs Batch)"),
        };
        if in_use != Some(program.program) {
          gl.use_program(Some(program.program));
          in_use = Some(program.program);
        }
        match (batch.program, &batch.gradient) {
          (BatchProgram::Glyph, gradient) => {
            let policy = batch.policy.expect("a glyph batch carries its coverage policy (walk.rs Batch)");
            self.glyph_state(policy, gradient.as_ref());
          }
          (BatchProgram::Fill, Some(gradient)) => self.gradient_state(&self.fill_gradient_uniforms, gradient),
          (BatchProgram::Shape, Some(gradient)) => self.gradient_state(&self.shape_gradient_uniforms, gradient),
          _ => {}
        }
        match batch.texture {
          Some((name, sampling)) => {
            let sampler = self.sampler(sampling, TextureWrap::Clamp);
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
      if ramped {
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
  // Create the sampler objects the batches of `walked` bind and this
  // executor does not have yet: (sampling, clamp) for a texture batch and
  // for the ramp a glyph or gradient batch samples, each input's own for a
  // paint batch.
  unsafe fn create_samplers(&mut self, walked: &Walked) -> Result<(), String> {
    for batch in &walked.batches {
      let ramp = needs_ramp(batch).then_some((TextureSampling::Linear, TextureWrap::Clamp));
      let texture = batch.texture.map(|(_, sampling)| (sampling, TextureWrap::Clamp));
      let inputs = batch.paint.iter().flat_map(|p| p.inputs.iter()).map(|i| (i.sampling, i.wrap));
      for key in ramp.into_iter().chain(texture).chain(inputs) {
        if !self.samplers.contains_key(&key) {
          let sampler = create_sampler(&self.gl, key.0, key.1)?;
          self.samplers.insert(key, sampler);
        }
      }
    }
    Ok(())
  }

  // The sampler object for (`sampling`, `wrap`), which `create_samplers`
  // made before the draw.
  fn sampler(&self, sampling: TextureSampling, wrap: TextureWrap) -> glow::Sampler {
    *self.samplers.get(&(sampling, wrap)).expect("create_samplers made every sampler a draw binds")
  }

  // Bind the gradient ramp on RAMP_UNIT through the linear clamped
  // sampler, then make TEXTURE_UNIT the active unit, as `draw` expects.
  unsafe fn bind_ramp(&self) {
    let gl = &self.gl;
    gl.active_texture(glow::TEXTURE0 + RAMP_UNIT);
    gl.bind_texture(glow::TEXTURE_2D, Some(self.ramp));
    gl.bind_sampler(RAMP_UNIT, Some(self.sampler(TextureSampling::Linear, TextureWrap::Clamp)));
    gl.active_texture(glow::TEXTURE0 + TEXTURE_UNIT);
  }

  // A paint batch of `count` instances into a target of `size` device
  // pixels: the shader's program in use, its params set by its uniform
  // table, `uSize` and `iResolution` set after them (so a param of either
  // name cannot displace them), its inputs bound on units 0.. in order, one
  // draw, and the units it used unbound again. TEXTURE_UNIT is the active
  // unit before and after, with nothing bound on it.
  unsafe fn draw_paint(&self, paint: &PaintDraw, size: [f32; 2], count: usize) {
    let gl = &self.gl;
    let shader = paint.shader.linked();
    gl.use_program(Some(shader.program));
    for param in paint.params.iter() {
      // A name the program has not got is skipped: alloy validated it.
      let Some((location, kind, elements)) = shader.uniforms.get(&param.name) else { continue };
      apply_param(gl, &param.name, location, *kind, *elements, &param.value);
    }
    gl.uniform_2_f32(Some(&shader.size), size[0], size[1]);
    if let Some(resolution) = &shader.resolution {
      gl.uniform_2_f32(Some(resolution), paint.resolution[0], paint.resolution[1]);
    }
    let mut units = 0;
    for input in paint.inputs.iter() {
      let Some((location, kind, _)) = shader.uniforms.get(&input.name) else { continue };
      if *kind != UniformKind::Sampler2D {
        log::debug!("[anvil] Paint shader: input '{}' names a uniform that is not a sampler2D; skipped", input.name);
        continue;
      }
      if units == MAX_PAINT_INPUTS {
        log::debug!(
          "[anvil] Paint shader: input '{}' is past the {MAX_PAINT_INPUTS} texture units; skipped",
          input.name
        );
        continue;
      }
      gl.active_texture(glow::TEXTURE0 + units);
      gl.bind_texture(glow::TEXTURE_2D, Some(input.texture.name()));
      gl.bind_sampler(units, Some(self.sampler(input.sampling, input.wrap)));
      gl.uniform_1_i32(Some(location), units as i32);
      units += 1;
    }
    gl.draw_arrays_instanced(glow::TRIANGLES, 0, VERTICES_PER_QUAD, count as i32);
    for unit in 0..units {
      gl.active_texture(glow::TEXTURE0 + unit);
      gl.bind_sampler(unit, None);
      gl.bind_texture(glow::TEXTURE_2D, None);
    }
    gl.active_texture(glow::TEXTURE0 + TEXTURE_UNIT);
    gl.bind_sampler(TEXTURE_UNIT, None);
    gl.bind_texture(glow::TEXTURE_2D, None);
  }

  // A glyph batch's uniforms: `policy`, and `gradient` (`gradient_state`)
  // or no gradient. The glyph program is in use and TEXTURE_UNIT is the
  // active unit, as `draw` left them; the active unit is TEXTURE_UNIT again
  // after.
  unsafe fn glyph_state(&self, policy: CoveragePolicy, gradient: Option<&GradientDraw>) {
    let (gl, u) = (&self.gl, &self.glyph_uniforms);
    gl.uniform_1_i32(Some(&u.mode), policy.mode as i32);
    gl.uniform_1_f32(Some(&u.gamma), policy.gamma);
    gl.uniform_1_f32(Some(&u.contrast), policy.contrast);
    let [r0, r1, r2, r3] = policy.gamma_ratios();
    gl.uniform_4_f32(Some(&u.gamma_ratios), r0, r1, r2, r3);
    match gradient {
      Some(draw) => self.gradient_state(&self.glyph_gradient_uniforms, draw),
      None => gl.uniform_1_i32(Some(&self.glyph_gradient_uniforms.gradient), NO_GRADIENT),
    }
  }

  // A gradient batch's uniforms `u` on its program: the ramp uploaded into
  // the ramp texture (bound on RAMP_UNIT by `draw`), the kind, the rows
  // from device pixels and the tile mode. The batch's program is in use
  // and TEXTURE_UNIT is the active unit, as `draw` left them; the active
  // unit is TEXTURE_UNIT again after.
  unsafe fn gradient_state(&self, u: &GradientUniforms, draw: &GradientDraw) {
    let (gl, g) = (&self.gl, &*draw.gradient);
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
    let [[x0, x1, x2], [y0, y1, y2]] = draw.rows;
    gl.uniform_3_f32(Some(&u.gradient_x), x0, x1, x2);
    gl.uniform_3_f32(Some(&u.gradient_y), y0, y1, y2);
    gl.uniform_1_i32(Some(&u.tile), tile_uniform(g.tile));
  }
}

// Whether `batch` samples the gradient ramp: a glyph batch (the glyph
// program declares it, gradient or not) or any batch with a gradient.
fn needs_ramp(batch: &Batch) -> bool {
  batch.program == BatchProgram::Glyph || batch.gradient.is_some()
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

// The location of the uniform `name` in `program` (`label` names it); an
// error naming both when the program has none.
unsafe fn uniform(
  gl: &glow::Context,
  program: glow::Program,
  label: &str,
  name: &str,
) -> Result<glow::UniformLocation, String> {
  gl.get_uniform_location(program, name).ok_or_else(|| format!("{label} program has no {name} uniform"))
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

// A sampler object filtering by `sampling` for both minification and
// magnification, wrapping by `wrap` on both axes. No mip selection: a
// texture quad's Linear is bilinear at level 0, as Impeller's was.
unsafe fn create_sampler(
  gl: &glow::Context,
  sampling: TextureSampling,
  wrap: TextureWrap,
) -> Result<glow::Sampler, String> {
  let filter = match sampling {
    TextureSampling::Linear => glow::LINEAR,
    TextureSampling::Nearest => glow::NEAREST,
  };
  let wrap_mode = match wrap {
    TextureWrap::Clamp => glow::CLAMP_TO_EDGE,
    TextureWrap::Repeat => glow::REPEAT,
  };
  let sampler = gl.create_sampler().map_err(|e| format!("{sampling:?} {wrap:?} sampler: {e}"))?;
  gl.sampler_parameter_i32(sampler, glow::TEXTURE_MIN_FILTER, filter as i32);
  gl.sampler_parameter_i32(sampler, glow::TEXTURE_MAG_FILTER, filter as i32);
  gl.sampler_parameter_i32(sampler, glow::TEXTURE_WRAP_S, wrap_mode as i32);
  gl.sampler_parameter_i32(sampler, glow::TEXTURE_WRAP_T, wrap_mode as i32);
  Ok(sampler)
}

// Every active uniform of `program`: by bare name (an array's `[0]`
// stripped) to its location, kind and array size, and as the reflected
// list. A uniform GL reports but cannot locate is left out of both.
unsafe fn reflect_uniforms(gl: &glow::Context, program: glow::Program) -> (UniformTable, Vec<UniformInfo>) {
  let mut uniforms = HashMap::new();
  let mut infos = Vec::new();
  for index in 0..gl.get_active_uniforms(program) {
    let Some(active) = gl.get_active_uniform(program, index) else { continue };
    let Some(location) = gl.get_uniform_location(program, &active.name) else { continue };
    let name = active.name.strip_suffix("[0]").unwrap_or(&active.name).to_string();
    let kind = UniformKind::from_gl(active.utype);
    let count = active.size.max(1) as usize;
    infos.push(UniformInfo { name: name.clone(), kind, count });
    uniforms.insert(name, (location, kind, count));
  }
  (uniforms, infos)
}

// Set the param `name`'s `value` on the uniform at `location`, of `kind`
// and `count` elements, in the program in use: the float kinds through
// their slice setters, int and bool as each float cast to i32. A value
// whose length is not the kind's components times the count, and a kind
// no param sets, is skipped (alloy validated the params; this is the
// raster side's guard).
unsafe fn apply_param(
  gl: &glow::Context,
  name: &str,
  location: &glow::UniformLocation,
  kind: UniformKind,
  count: usize,
  value: &[f32],
) {
  let wanted = kind.components().map(|c| c * count);
  if wanted != Some(value.len()) {
    log::debug!(
      "[anvil] Paint shader: param '{name}' has {} component(s), uniform {kind:?} x {count} takes {wanted:?}; skipped",
      value.len()
    );
    return;
  }
  let location = Some(location);
  match kind {
    UniformKind::Float => gl.uniform_1_f32_slice(location, value),
    UniformKind::Int | UniformKind::Bool => {
      let ints: Vec<i32> = value.iter().map(|v| *v as i32).collect();
      gl.uniform_1_i32_slice(location, &ints);
    }
    UniformKind::Vec2 => gl.uniform_2_f32_slice(location, value),
    UniformKind::Vec3 => gl.uniform_3_f32_slice(location, value),
    UniformKind::Vec4 => gl.uniform_4_f32_slice(location, value),
    UniformKind::Mat4 => gl.uniform_matrix_4_f32_slice(location, false, value),
    // No component count, so the guard above already returned.
    UniformKind::Sampler2D | UniformKind::Other(_) => {}
  }
}

impl Program {
  // The program `label` from the concatenated `vertex` and `fragment`
  // source parts, linked, with its `uSize`.
  unsafe fn link(gl: &glow::Context, label: &str, vertex: &[&str], fragment: &[&str]) -> Result<Program, String> {
    let program = link_program(gl, label, &vertex.concat(), &fragment.concat())?;
    let size = uniform(gl, program, label, "uSize")?;
    Ok(Program { program, size })
  }
}

// The program `label`, compiled and linked, its shaders deleted after the
// link.
unsafe fn link_program(
  gl: &glow::Context,
  label: &str,
  vertex_src: &str,
  fragment_src: &str,
) -> Result<glow::Program, String> {
  let vertex = compile_shader(gl, glow::VERTEX_SHADER, vertex_src, label, "vertex")?;
  let fragment = match compile_shader(gl, glow::FRAGMENT_SHADER, fragment_src, label, "fragment") {
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
      return Err(format!("{label} program: {e}"));
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
    return Err(format!("{label} program link failed: {log}"));
  }
  Ok(program)
}

unsafe fn compile_shader(
  gl: &glow::Context,
  kind: u32,
  source: &str,
  program: &str,
  label: &str,
) -> Result<glow::Shader, String> {
  let shader = gl.create_shader(kind).map_err(|e| format!("{program} {label} shader: {e}"))?;
  gl.shader_source(shader, source);
  gl.compile_shader(shader);
  if !gl.get_shader_compile_status(shader) {
    let log = gl.get_shader_info_log(shader);
    gl.delete_shader(shader);
    return Err(format!("{program} {label} shader compile failed: {log}"));
  }
  Ok(shader)
}
