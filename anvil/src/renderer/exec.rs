//! The GL half of a draw: the fill, rounded, shape and glyph programs (the
//! fill, the rounded and the shape program each with a gradient variant)
//! and the mesh program (with a gradient and a textured variant), ten
//! programs in all; the
//! paint shaders' programs linked behind anvil's vertex stage; two VAOs the
//! executor owns, one over the instance buffer and one the path meshes'
//! own vertex and index buffers are bound into per draw; and the draw that
//! uploads a list's instances (`super::walk`), uploads a path mesh at its
//! first draw and keeps it on the GPU after (`mesh::GpuMesh`), and issues
//! them, a batch through the program its kind needs. A quad is placed in
//! homogeneous coordinates under its instance's projective row, a mesh
//! under its map as a uniform, so every smooth varying interpolates
//! perspective-correct. A gradient's parameter is affine in the op's
//! space, so the vertex stage evaluates it at the corner, through the rows
//! on the quad's instance or the rows of the mesh draw
//! (`walk::GradientInstance`), and the fragment reads it interpolated with
//! its ramp row (`vGrad`); the kind and the tile are the batch's or the mesh
//! draw's uniform (`uGradMeta`), and only the glyph program, whose runs
//! join whatever their gradient, reads them per instance (`vGradMeta`). Its
//! ramp is one row of the ramp atlas, whose rows the walk assigns
//! (`super::ramps`) and the draw uploads before any batch samples them. A
//! stencil clip (`walk::ClipDraw`) draws into the stencil with the color
//! writes off, a rect's quad through the fill program
//! and a rounded rect's or an oval's mesh through the solid mesh program,
//! and every later batch tests the stencil at its depth. Raster thread
//! only, with the context current.

use super::mesh::{gpu_slot, GpuMesh, GpuSlot, Mesh};
use super::ramps::{RampTable, RAMP_ROWS};
use super::record::DisplayList;
use super::walk::MESH_VERTEX_FLOATS;
use super::walk::{self, Batch, BatchProgram, Instance, MeshDraw, MeshVertex, PaintDraw, Projective, Walked};
use super::walk::{ClipDraw, ClipShape};
use super::{DeletionQueue, Freed};
use crate::contract::{DrawReport, Target};
use crate::{CoveragePolicy, RAMP_WIDTH};
use crate::{TextureSampling, TextureWrap, UniformInfo, UniformKind};
use euclid::default::{Box2D, Point2D};
use glow::HasContext;
use std::collections::HashMap;
use std::os::raw::c_void;
use std::sync::Arc;

/// Two triangles a quad, their corners derived from `gl_VertexID`.
const VERTICES_PER_QUAD: i32 = 6;
/// Floats per instance attribute: each is a vec4.
const FLOATS_PER_ATTRIBUTE: i32 = 4;
/// Every bit of the 8-bit stencil (DEPTH24_STENCIL8): the write mask and
/// the test mask of every stencil draw and test.
const STENCIL_ALL_BITS: u32 = 0xFF;

// The attribute locations. They must match the `layout(location = N)`
// literals in VERTEX_COMMON, SHAPE_VERTEX and ROUNDED_VERTEX.
const ATTR_RECT: u32 = 0;
const ATTR_LINEAR: u32 = 1;
const ATTR_OFFSET: u32 = 2;
const ATTR_COLOR: u32 = 3;
const ATTR_TEX: u32 = 4;
const ATTR_CLIP: u32 = 5;
const ATTR_RADII: u32 = 6;
const ATTR_RCLIP: u32 = 7;
const ATTR_RCLIP_RADII: u32 = 8;
const ATTR_PERSP: u32 = 9;
const ATTR_GRAD_X: u32 = 10;
const ATTR_GRAD_Y: u32 = 11;

// The twelve instanced attributes: location and byte offset in `Instance`.
// Enabled with divisor 1 once in `new`; re-pointed per batch in `draw`.
const ATTRIBUTES: [(u32, usize); 12] = [
  (ATTR_RECT, std::mem::offset_of!(Instance, rect)),
  (ATTR_LINEAR, std::mem::offset_of!(Instance, linear)),
  (ATTR_OFFSET, std::mem::offset_of!(Instance, offset_kind_stroke)),
  (ATTR_COLOR, std::mem::offset_of!(Instance, color)),
  (ATTR_TEX, std::mem::offset_of!(Instance, tex)),
  (ATTR_CLIP, std::mem::offset_of!(Instance, clip)),
  (ATTR_RADII, std::mem::offset_of!(Instance, radii)),
  (ATTR_RCLIP, std::mem::offset_of!(Instance, rclip)),
  (ATTR_RCLIP_RADII, std::mem::offset_of!(Instance, rclip_radii)),
  (ATTR_PERSP, std::mem::offset_of!(Instance, persp)),
  (ATTR_GRAD_X, std::mem::offset_of!(Instance, grad_x)),
  (ATTR_GRAD_Y, std::mem::offset_of!(Instance, grad_y)),
];

// The mesh vertex attribute, the path-space position: per vertex (no
// divisor), enabled once in `new` and pointed at a mesh's own vertex buffer
// per draw. It must match the `layout(location = N)` literal in
// MESH_VERTEX.
const ATTR_MESH_POS: u32 = 0;

/// The texture unit `uTexture` samples: a batch's texture, an image or a
/// glyph run's atlas.
const TEXTURE_UNIT: u32 = 0;
/// The texture unit `uRamp` samples: the ramp atlas.
const RAMP_UNIT: u32 = 1;
/// Bytes per rgba8 ramp texel.
const BYTES_PER_RAMP_TEXEL: usize = 4;
/// The texture units a paint shader's inputs may take, from unit 0 on: GLES
/// 3.0's guaranteed minimum of fragment texture units. An input past it is
/// skipped.
const MAX_PAINT_INPUTS: u32 = 16;

// The first lines of every shader source here.
pub(crate) const SOURCE_HEADER: &str = "#version 300 es\nprecision highp float;\n";

// Right after SOURCE_HEADER in a fragment source, it selects the gradient
// variant of the fill, the rounded and the shape program: the same source, its color
// from `gradientAt` (GRADIENT_COMMON).
const GRADIENT_DEFINE: &str = "#define GRADIENT\n";

// Right after SOURCE_HEADER in the mesh program's fragment source, it
// selects the textured variant: the color from `uTexture` at the texel the
// draw's path-to-texel rows give.
const TEXTURED_DEFINE: &str = "#define TEXTURED\n";

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
//
// The quad is placed in homogeneous coordinates, its corner's (x', y', w')
// under the instance's map with no divide: the rasterizer divides, and
// every smooth varying (vUv, vGrad, the shape program's vLocal) then
// interpolates perspective-correct. A 2D map's w' is 1, which is the
// placement of before.
//
// A gradient's parameter is affine in the quad's local space: the corner
// evaluates its rows into `vGrad`'s first two components, and the
// interpolation of an affine function is exact, perspective-correct under
// any map; the third is the ramp row, constant over the quad, so its
// interpolation is exact too. The fragment reads that one smooth vec3 where
// it once read the rows and the local point; the kind and the tile are the
// batch's uniform (GRADIENT_BATCH), except in the glyph program, which
// reads them per instance from the flat `vGradMeta` (the linker drops it
// from every other program). A solid instance carries zero rows: its vGrad
// is zero and unread.
const VERTEX_COMMON: &str = r"
layout(location = 0) in vec4 aRect;      // x, y, w, h in local space
layout(location = 1) in vec4 aLinear;    // m11, m12, m21, m22
layout(location = 2) in vec4 aOffset;    // m41, m42 (the translation), kind, stroke width
layout(location = 3) in vec4 aColor;     // premultiplied; straight alpha for a glyph
layout(location = 4) in vec4 aTex;       // u0, v0, u1, v1 for a glyph; cap, length for a line; a rounded fill's shape quad
layout(location = 5) in vec4 aClip;      // x0, y0, x1, y1 in device pixels
layout(location = 7) in vec4 aRClip;     // the rounded clip in device pixels: x0, y0, x1, y1
layout(location = 8) in vec4 aRClipRadii; // its corner radii in device pixels; all zero: none
layout(location = 9) in vec4 aPersp;     // the projective row: w' = x * p1 + y * p2 + p3; the tile
layout(location = 10) in vec4 aGradX;    // a gradient's x row and ramp row; a texel u row
layout(location = 11) in vec4 aGradY;    // its y row and kind (0: none); a texel v row
uniform vec2 uSize;                      // the target in device pixels
out vec2 vUv;
out vec3 vGrad;                          // the gradient's parameter-space point, its ramp row
flat out vec4 vColor;
flat out vec4 vClip;
flat out vec4 vRClip;
flat out vec4 vRClipRadii;
flat out vec2 vGradMeta;                 // the kind, the tile: the glyph program's alone
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
  // euclid's row-vector convention: (x', y', w') = (p, 1) * M.
  vec3 h = vec3(
    p.x * aLinear.x + p.y * aLinear.z + aOffset.x,
    p.x * aLinear.y + p.y * aLinear.w + aOffset.y,
    p.x * aPersp.x + p.y * aPersp.y + aPersp.z);
  // Logical (0, 0) is the top of the target: clip y = +1. Device (x, y)
  // to clip space, both sides times w'.
  gl_Position = vec4(h.x / uSize.x * 2.0 - h.z, h.z - h.y / uSize.y * 2.0, 0.0, h.z);
  vUv = aTex.xy + c * (aTex.zw - aTex.xy);
  vec3 q = vec3(p, 1.0);
  vGrad = vec3(dot(aGradX.xyz, q), dot(aGradY.xyz, q), aGradX.w);
  vColor = aColor;
  vClip = aClip;
  vRClip = aRClip;
  vRClipRadii = aRClipRadii;
  vGradMeta = vec2(aGradY.w, aPersp.w);
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
// The textured flag, as the instance adds it to the kind (walk.rs
// KIND_TEXTURED): it must match.
const float KIND_TEXTURED = 8.0;
void main() {
  vec2 c = quadCorner();
  place(c);
  // A textured instance's uv is its texel rows evaluated at the corner: an
  // affine function of the local point, so its interpolation over the quad
  // is exact under any image map, a rotated one among them. The rows sit
  // in the gradient slots with kind 0, so `place` has evaluated them into
  // `vGrad.xy` already: a textured instance's vGrad is its texel, unread as
  // a gradient (a textured instance never draws through a gradient variant).
  if (aOffset.z >= KIND_TEXTURED) {
    vUv = vGrad.xy;
  }
  vLocal = c * aRect.zw;
  vShape = vec4(aRect.zw, aOffset.zw);
  vParams = aTex;
  vRadii = aRadii;
}
";

// The rounded program's vertex stage, after SOURCE_HEADER and VERTEX_COMMON:
// the quad placed, and the shape measured from the whole shape quad the
// instance carries in `aTex` (walk.rs `Instance::tex`: x, y, w, h, local
// space), not from the quad it draws. A piece of a split rounded fill
// (walk.rs `split_rounded`) draws only its own rect, so its vLocal is the
// local point relative to the shape quad's origin and vShape the shape
// quad's size: the fragment's rounded-box distance is the same function
// over the same points for a piece and for a whole quad alike. It writes
// no vParams and no textured vUv; the linker drops what the fragment does
// not read (vUv, vGradMeta).
const ROUNDED_VERTEX: &str = r"
layout(location = 6) in vec4 aRadii;     // corner radii in local pixels: tl, tr, br, bl
out vec2 vLocal;                         // position within the shape quad, local pixels from its origin
flat out vec4 vShape;                    // the shape quad's w, h; z, w unused
flat out vec4 vRadii;
void main() {
  vec2 p = place(quadCorner());
  vLocal = p - aTex.xy;
  vShape = vec4(aTex.zw, 0.0, 0.0);
  vRadii = aRadii;
}
";

// The vertex stage of the fill programs and of the glyph program, after
// SOURCE_HEADER and VERTEX_COMMON: the quad placed, nothing more (a
// gradient reads `vGrad`, and a glyph's `vGradMeta`, which `place` writes).
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

// What a paint shader's source reads beside the common fragment parts (`fragColor`
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
  vec2 d = device();
  float inside = float(all(greaterThanEqual(d, vClip.xy)) && all(lessThan(d, vClip.zw)));
  fragColor *= inside * roundedClip();
}
";

// The fragment stage the shape and glyph programs (and the fill program's
// gradient variant) share, "the common fragment parts" below, three
// sources concatenated in this order: FRAGMENT_INPUTS, the inputs
// VERTEX_COMMON writes and the fragment's device position; FRAGMENT_MATH,
// the analytic coverage of a distance and the rounded-box distance, pure
// functions the mesh program uses on its own inputs; and FRAGMENT_ROUNDED_CLIP, the rounded clip over the
// inputs. One text, so a glyph under a rounded clip fades exactly as a
// shape does. The parts split at line ends: concatenated they are the one
// text they were.
const FRAGMENT_INPUTS: &str = r"
uniform vec2 uSize;       // the target in device pixels, the vertex stage's
// The fragment's device position, y down. At a pixel center it equals the
// device position the vertex stage once interpolated, exactly: gl_FragCoord
// is that center. It is right under perspective too, where a linearly
// interpolated device position would not be (GLSL ES 3.00 has no
// noperspective qualifier).
vec2 device() { return vec2(gl_FragCoord.x, uSize.y - gl_FragCoord.y); }
in vec2 vUv;
flat in vec4 vColor;
flat in vec4 vClip;
flat in vec4 vRClip;      // the rounded clip in device pixels: x0, y0, x1, y1
flat in vec4 vRClipRadii; // its corner radii in device pixels; all zero: none
out vec4 fragColor;";

const FRAGMENT_MATH: &str = r"
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
}";

const FRAGMENT_ROUNDED_CLIP: &str = r"
// The rounded clip's coverage at this fragment, in device space: the same
// distance function over the fragment's device position. All-zero radii
// mean no rounded clip is in effect (the box test already clipped): 1.
// The radii are flat, so every fragment of a quad takes the same path and
// the derivatives stay defined.
float roundedClip() {
  if (!any(greaterThan(vRClipRadii, vec4(0.0)))) return 1.0;
  vec2 center = (vRClip.xy + vRClip.zw) * 0.5;
  vec2 half_size = (vRClip.zw - vRClip.xy) * 0.5;
  return coverage(roundedBox(device() - center, half_size, vRClipRadii));
}
";

// The gradient input of every gradient program, after the common fragment
// parts in the glyph program and the quad gradient variants (after
// FRAGMENT_MATH in the mesh program's), before GRADIENT_COMMON: the
// parameter-space point the vertex stage evaluated from the rows (walk.rs
// GradientInstance) at the corner, interpolated, and the gradient's ramp
// row, constant over the quad or the mesh. The kind and the tile are not
// here: the batch programs read them from a uniform (GRADIENT_BATCH), the
// glyph program from its own per-instance input (GLYPH_FRAGMENT).
const GRADIENT_INPUTS: &str = r"
in vec3 vGrad;            // the parameter-space point, the ramp row
";

// A gradient's evaluation, after GRADIENT_INPUTS: the color at this
// fragment, from the ramp atlas's row, for the gradient `kind` tiled by
// `tile`. The kinds are GradientKind's discriminants and the tile modes
// walk.rs TILE_*: they must match. No integer division or modulo: the kind
// and the tile arrive apart.
const GRADIENT_COMMON: &str = r"
uniform sampler2D uRamp;
uniform float uRampRows;  // the ramp atlas's height in rows
const int GRADIENT_LINEAR = 1;
const int TILE_REPEAT = 1;
const int TILE_MIRROR = 2;
const int TILE_DECAL = 3;
// The gradient's straight-alpha color at this fragment: the tile mode
// folds the parameter, the decal mode's outside is alpha 0.
vec4 gradientAt(int kind, int tile) {
  float t = kind == GRADIENT_LINEAR ? vGrad.x : length(vGrad.xy);
  float inside = 1.0;
  if (tile == TILE_REPEAT) {
    t = fract(t);
  } else if (tile == TILE_MIRROR) {
    t = 1.0 - abs(1.0 - mod(t, 2.0));
  } else if (tile == TILE_DECAL) {
    inside = (t < 0.0 || t > 1.0) ? 0.0 : 1.0;
  }
  // The row's texel centers, so the bilinear lookup reads that row alone.
  vec4 g = texture(uRamp, vec2(clamp(t, 0.0, 1.0), (vGrad.z + 0.5) / uRampRows));
  return vec4(g.rgb, g.a * inside);
}
";

// The gradient of a batch program, after GRADIENT_COMMON: the fill, rounded
// and shape gradient variants and the mesh gradient variant, whose batch or
// draw holds one kind and one tile mode (walk.rs GradientClass), set as the
// uniform `uGradMeta`, so the compiler treats the branches on them as
// uniform. The mesh program's vertex stage reads the same uniform for its
// ramp row. Not in the glyph program, whose runs join whatever their
// gradient: it reads the kind and the tile per instance.
const GRADIENT_BATCH: &str = r"
uniform vec4 uGradMeta;   // 0, the kind, the tile, 0 (a mesh draw: its ramp row first)
// The batch's gradient at this fragment. The kind and the tile arrive as
// floats; rounding reads them robustly.
vec4 gradientBatch() {
  return gradientAt(int(uGradMeta.y + 0.5), int(uGradMeta.z + 0.5));
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
// multiplies its coverage into the gradient's color at the fragment
// (`vGrad`), premultiplied, its kind and tile the batch's uniform; it draws
// no textured instance. It reads nine varyings: vLocal and vGrad smooth,
// vColor, vClip, vRClip, vRClipRadii, vShape, vParams and vRadii flat
// (eleven before stage 5 step 2c: vLocal and vOp smooth, the rows vGradX
// and vGradY flat in place of vGrad's ramp row, and the kind and the tile
// per instance; ten in step 2c, the flat vGradMeta beside vGrad).
//
// After SOURCE_HEADER and the common fragment parts; the gradient variant after
// SOURCE_HEADER, GRADIENT_DEFINE, the common fragment parts, GRADIENT_INPUTS,
// GRADIENT_COMMON and GRADIENT_BATCH.
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
  // The clip box, per fragment, so a clip change never ends a batch, as a
  // factor on the color and never a discard before a derivative: a discard
  // ends its lane of the 2x2 derivative quad, so where a clip edge lies at
  // an odd device coordinate the fragments on the edge row inside the box
  // lose their neighbour, `footprint()` falls to its floor and the
  // coverage of a rounded corner on that row collapses to a step (seen as
  // a hard corner on a rounded rect under a same-box rounded clip, stage 5
  // step 2b). Under MSAA the test is per fragment, not per sample: a clip
  // edge is hard where a shape edge is antialiased.
  vec2 dp = device();
  float inside = float(all(greaterThanEqual(dp, vClip.xy)) && all(lessThan(dp, vClip.zw)));
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
  c = min(c, roundedClip()) * inside;
  // After every derivative: a fragment the box or the coverage rejects
  // drops here, which keeps the MSAA resolve of the quad's own edges.
  if (c <= 0.0) discard;
#ifdef GRADIENT
  // The ramp's straight-alpha color, premultiplied; vColor is the op's
  // alpha in every channel, so the multiply fades it as a whole.
  vec4 g = gradientBatch();
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
// its color from the gradient at the fragment (`vGrad`), premultiplied
// and faded by vColor, the op's alpha in all four channels, its kind and
// tile the batch's uniform; it reads three varyings, vColor, vClip and
// vGrad, as stage 4's did (five before stage 5 step 2c: vColor, vClip, vOp
// and the rows vGradX and vGradY; four in step 2c, the flat vGradMeta
// beside vGrad); the common fragment parts then declare the inputs, and
// their derivative functions are never called. The solid one declares its own few inputs
// and the same `device()` (FRAGMENT_INPUTS says why it is right).
//
// After SOURCE_HEADER only; the gradient variant after SOURCE_HEADER,
// GRADIENT_DEFINE, the common fragment parts, GRADIENT_INPUTS,
// GRADIENT_COMMON and GRADIENT_BATCH.
const FILL_FRAGMENT: &str = r"
#ifndef GRADIENT
uniform vec2 uSize;
vec2 device() { return vec2(gl_FragCoord.x, uSize.y - gl_FragCoord.y); }
flat in vec4 vColor;
flat in vec4 vClip;
out vec4 fragColor;
#endif
void main() {
  // The clip box as a factor, not a discard, as in GLYPH_FRAGMENT: a
  // fragment outside writes zero, which premultiplied source-over leaves
  // the target unchanged by.
  vec2 d = device();
  float inside = float(all(greaterThanEqual(d, vClip.xy)) && all(lessThan(d, vClip.zw)));
#ifdef GRADIENT
  vec4 g = gradientBatch();
  fragColor = vec4(g.rgb * g.a, g.a) * vColor * inside;
#else
  fragColor = vColor * inside;
#endif
}
";

// A rounded fill (walk.rs `rounded_fill`: an untextured, unstroked rounded
// rect, or a plain rect under a rounded clip) has the rounded-box distance
// and nothing else to decide. A program of its own, because rounded rects
// are the most common UI shape after plain rects (every card, panel and
// button) and on the Mali the shape program's per-fragment price, its kind
// branches, its texture sample and its discard, made rounded panels cost
// several times what plain ones did (okf/plans/rounded-rect-fill-program.md).
// It keeps the derivatives (`coverage()` reads them through `footprint()`)
// and drops the branches on the kind, the texture and the discard: the
// clip box is a factor, the rounded clip is composed by the minimum as the
// shape program composes it, and a fragment outside writes zero, which
// premultiplied source-over leaves the target unchanged by. The shape's
// coverage is exactly the shape program's fill arm for a rounded rect (its
// quad padded by ROUNDED_QUAD_PAD, stroke 0); a plain rect has no radii and
// coverage 1, its edges MSAA's. The radii are flat, so every fragment of a
// quad takes the same branch and the derivatives stay defined, as in
// `roundedClip()`. Its vertex stage is ROUNDED_VERTEX, which measures the
// shape from the whole shape quad, so a large rounded fill draws here only
// as its rim (the corners and a thin band along each straight edge) and
// its plain pieces, whose coverage is exactly 1, go through the fill
// program (walk.rs `split_rounded`); every rim fragment computes the same
// distance from the same shape as the unsplit quad's would. The gradient
// variant (GRADIENT defined) takes its color from the gradient at the
// fragment (`vGrad`), premultiplied and faded by vColor, its kind and tile
// the batch's uniform, as the shape program's does. It reads eight
// varyings: vLocal and vGrad smooth, vColor, vClip, vRClip, vRClipRadii,
// vShape and vRadii flat (the solid one seven, no vGrad); vShape's z and w
// are unused (ten before stage 5 step 2c: vLocal and vOp smooth, the rows
// vGradX and vGradY flat in place of vGrad's ramp row, and the kind and the
// tile per instance; nine in step 2c, the flat vGradMeta beside vGrad).
//
// After SOURCE_HEADER and the common fragment parts; the gradient variant
// after SOURCE_HEADER, GRADIENT_DEFINE, the common fragment parts,
// GRADIENT_INPUTS, GRADIENT_COMMON and GRADIENT_BATCH.
const ROUNDED_FRAGMENT: &str = r"
in vec2 vLocal;
flat in vec4 vShape;      // the shape quad's w, h; z, w unused
flat in vec4 vRadii;      // corner radii, local pixels: tl, tr, br, bl; all zero: a plain rect
// Local pixels a rounded rect's quad extends past its edge (walk.rs
// ROUNDED_QUAD_PAD must match).
const float ROUNDED_QUAD_PAD = 1.0;
void main() {
  vec2 dp = device();
  float inside = float(all(greaterThanEqual(dp, vClip.xy)) && all(lessThan(dp, vClip.zw)));
  float c = 1.0;
  if (any(greaterThan(vRadii, vec4(0.0)))) {
    vec2 hs = vShape.xy * 0.5;
    c = coverage(roundedBox(vLocal - hs, hs - ROUNDED_QUAD_PAD, vRadii));
  }
  c = min(c, roundedClip());
#ifdef GRADIENT
  vec4 g = gradientBatch();
  vec4 color = vec4(g.rgb * g.a, g.a) * vColor;
#else
  vec4 color = vColor;
#endif
  fragColor = color * c * inside;
}
";

// A glyph has no analytic coverage: its quad is axis-aligned at whole
// pixels and the atlas's red channel carries the coverage. Its color is
// straight alpha, the quad's own or, when its instance carries a gradient
// (a non-zero kind in `vGradMeta`), the gradient's at the fragment
// (`gradientAt` with the instance's kind and tile: a glyph run joins
// whatever its gradient, so they are per instance here, never a batch
// uniform), and the batch's coverage policy turns the coverage into
// color before the fragment premultiplies, as `draw_glyphs` asks
// (contract.rs) and `CoverageMode` defines each mode (glyphs.rs). A
// program of its own, so a glyph fragment carries none of the shape
// program's varyings or kind branches: it clips by multiplying, never
// discards, and reads the derivatives only under a rounded clip. It reads
// seven varyings: vUv and vGrad smooth, vColor, vClip, vRClip, vRClipRadii
// and vGradMeta flat (eight before stage 5 step 2c: vUv and vOp smooth,
// the rows vGradX and vGradY flat in place of vGrad's ramp row and
// vGradMeta).
//
// After SOURCE_HEADER, the common fragment parts, GRADIENT_INPUTS and
// GRADIENT_COMMON (not GRADIENT_BATCH: no batch uniform here).
const GLYPH_FRAGMENT: &str = r"
flat in vec2 vGradMeta;     // the instance's gradient kind (0: none), its tile
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
  // The kind and the tile arrive as flat floats; rounding reads them
  // robustly.
  int kind = int(vGradMeta.x + 0.5);
  if (kind != 0) {
    vec4 g = gradientAt(kind, int(vGradMeta.y + 0.5));
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
  vec2 d = device();
  float inside = float(all(greaterThanEqual(d, vClip.xy)) && all(lessThan(d, vClip.zw)));
  fragColor = glyph(inside * roundedClip());
}
";

// The mesh program's vertex stage, after SOURCE_HEADER (and
// GRADIENT_DEFINE or TEXTURED_DEFINE for its variants): a path mesh's
// vertex is in the path's space (the mesh stays on the GPU across frames),
// placed under the draw's map in homogeneous coordinates as VERTEX_COMMON
// places a quad, so every smooth varying interpolates perspective-correct
// and the GPU clips a vertex behind the eye. `uMap` is the map's 3x3 in
// euclid's row-vector convention, (x', y', w') = (x, y, 1) * M, so its
// column j is the coefficients of output j (`map_columns`). As for a quad,
// the gradient variant evaluates the draw's gradient rows over the path's
// space (walk.rs MeshDraw `gradient`) at the vertex into `vGrad`, with its
// ramp row as the third component (constant over the mesh), and the
// textured variant its texel rows into `vTexel`: both affine in the path's
// space, so their interpolation is exact. The fragment reads the kind and
// the tile from the same `uGradMeta` (GRADIENT_BATCH). The
// layout(location = N) literal must match ATTR_MESH_POS.
const MESH_VERTEX: &str = r"
layout(location = 0) in vec2 aPos;    // the path's space
uniform mat3 uMap;                    // the path's space to device pixels, homogeneous
uniform vec2 uSize;                   // the target in device pixels
#ifdef GRADIENT
uniform vec3 uGradX;                  // the path's (x, y, 1) to the parameter's x
uniform vec3 uGradY;                  // and y
uniform vec4 uGradMeta;               // the ramp row, the kind, the tile, 0
out vec3 vGrad;                       // the gradient's parameter-space point, its ramp row
#elif defined(TEXTURED)
uniform vec3 uTexelX;                 // the path's (x, y, 1) to the normalized texel's u
uniform vec3 uTexelY;                 // and v
out vec2 vTexel;                      // the normalized texel
#endif
void main() {
  vec3 q = vec3(aPos, 1.0);
  vec3 h = q * uMap;
  // Logical (0, 0) is the top of the target: clip y = +1. Device (x, y)
  // to clip space, both sides times w'.
  gl_Position = vec4(h.x / uSize.x * 2.0 - h.z, h.z - h.y / uSize.y * 2.0, 0.0, h.z);
#ifdef GRADIENT
  vGrad = vec3(dot(uGradX, q), dot(uGradY, q), uGradMeta.x);
#elif defined(TEXTURED)
  vTexel = vec2(dot(uTexelX, q), dot(uTexelY, q));
#endif
}
";

// A path mesh's fragment stage: the fill program's, a triangle's edges are
// MSAA's and its coverage is 1 inside. The color and the clips are the
// draw's uniforms (walk.rs MeshDraw); the clip box as a factor, never a
// discard; the rounded clip's all-zero radii return before any derivative.
// The gradient variant (GRADIENT defined) takes the color from the gradient
// at the parameter-space point MESH_VERTEX interpolates (`vGrad`), the
// textured variant (TEXTURED defined) from the draw's texture at the
// texel it interpolates (`vTexel`), so no inverse and no divide; both
// premultiplied and faded by uColor, the op's alpha in all four channels.
// The device position is `device()`, as in FRAGMENT_INPUTS. The solid
// variant reads no varying, the textured one vTexel (smooth), the gradient
// one vGrad (smooth), its kind and tile the draw's uniform; before stage 5
// step 2c both read vPath (smooth) and evaluated their rows, uniforms then,
// per fragment, and in step 2c the gradient one read a flat vGradMeta too.
//
// After SOURCE_HEADER and FRAGMENT_MATH; the gradient variant after
// SOURCE_HEADER, GRADIENT_DEFINE, FRAGMENT_MATH, GRADIENT_INPUTS,
// GRADIENT_COMMON and GRADIENT_BATCH; the textured variant after SOURCE_HEADER,
// TEXTURED_DEFINE and FRAGMENT_MATH.
const MESH_FRAGMENT: &str = r"
uniform vec2 uSize;       // the target in device pixels, the vertex stage's
vec2 device() { return vec2(gl_FragCoord.x, uSize.y - gl_FragCoord.y); }
uniform vec4 uColor;      // premultiplied
uniform vec4 uClip;       // x0, y0, x1, y1 in device pixels
uniform vec4 uRClip;      // the draw's rounded clip in device pixels: x0, y0, x1, y1
uniform vec4 uRClipRadii; // its corner radii in device pixels; all zero: none
out vec4 fragColor;
#if defined(TEXTURED) && !defined(GRADIENT)
uniform sampler2D uTexture;
in vec2 vTexel;           // the normalized texel
#endif
// The rounded clip's coverage at this fragment, as the quads' roundedClip()
// computes it, over the draw's uniforms. The radii are uniform, so every
// fragment takes the same path and the derivatives stay defined.
float roundedClip() {
  if (!any(greaterThan(uRClipRadii, vec4(0.0)))) return 1.0;
  vec2 center = (uRClip.xy + uRClip.zw) * 0.5;
  vec2 half_size = (uRClip.zw - uRClip.xy) * 0.5;
  return coverage(roundedBox(device() - center, half_size, uRClipRadii));
}
void main() {
  vec2 d = device();
  float inside = float(all(greaterThanEqual(d, uClip.xy)) && all(lessThan(d, uClip.zw)));
#ifdef GRADIENT
  vec4 g = gradientBatch();
  vec4 color = vec4(g.rgb * g.a, g.a) * uColor;
#elif defined(TEXTURED)
  vec4 color = texture(uTexture, vTexel) * uColor;
#else
  vec4 color = uColor;
#endif
  fragColor = color * inside * roundedClip();
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
  Ok(
    [
      SOURCE_HEADER,
      FRAGMENT_INPUTS,
      FRAGMENT_MATH,
      FRAGMENT_ROUNDED_CLIP,
      PAINT_FRAGMENT_PREAMBLE,
      &renamed,
      PAINT_FRAGMENT_EPILOGUE,
    ]
    .concat(),
  )
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

/// The GL half: the ten programs, the two VAOs and the instance buffer, and the
/// draw. Lives
/// as long as the renderer, which the raster thread drops before it releases
/// the context (raster/recover.rs), so the GL names here die with it and
/// Drop deletes nothing.
pub(super) struct Executor {
  gl: glow::Context,
  /// Every fill batch without a gradient: plain rect fills.
  fill: Program,
  /// A fill batch with a gradient: plain rect fills of one gradient kind
  /// and tile.
  fill_gradient: Program,
  /// Every rounded batch without a gradient: rounded rect fills and plain
  /// rect fills under a rounded clip.
  rounded: Program,
  /// A rounded batch with a gradient, of one kind and tile.
  rounded_gradient: Program,
  /// Every shape batch without a gradient: shapes, images, textures, and
  /// the small plain fills among them.
  shape: Program,
  /// A shape batch with a gradient: shapes of one gradient kind and tile.
  shape_gradient: Program,
  /// Every glyph batch (a batch with a coverage policy).
  glyph: Program,
  /// A path mesh draw without a gradient or a texture: a solid path.
  mesh: MeshProgram,
  /// A path mesh draw with a gradient.
  mesh_gradient: MeshProgram,
  /// A path mesh draw with a texture: a path filled with an image.
  mesh_textured: MeshProgram,
  /// The instance VAO: the instanced quad attributes over `instances`.
  vao: glow::VertexArray,
  instances: glow::Buffer,
  /// The mesh VAO: the per-vertex position attribute, pointed per draw at
  /// the mesh's own vertex buffer, and the mesh's index buffer bound as
  /// its element buffer (the VAO's state) per draw.
  mesh_vao: glow::VertexArray,
  /// The sampler objects per (sampling, wrap), created on first use, so a
  /// bound texture's own parameter state is never read or written: on this
  /// shared context only a bound sampler object is a reliable contract
  /// (raster/offscreen.rs says the same for the node shader passes). The
  /// texture batches and the ramp sample clamped; a paint shader's input
  /// takes its own wrap.
  samplers: HashMap<(TextureSampling, TextureWrap), glow::Sampler>,
  /// The glyph runs' policy uniforms, set per glyph batch.
  glyph_uniforms: GlyphUniforms,
  /// The ramp atlas (`RAMP_WIDTH` x `RAMP_ROWS` rgba8, straight-alpha
  /// sRGB, a gradient's ramp per row), its rows uploaded as `ramps`
  /// assigns them, before the batches of the draw that assigned them, and
  /// bound on `RAMP_UNIT` for every draw with a glyph or a gradient batch;
  /// a solid batch never samples it.
  ramp: glow::Texture,
  /// Which gradient holds which row of `ramp`, kept across draws.
  ramps: RampTable,
}

/// A linked program, its `uSize` (the target in device pixels), on the
/// programs that sample a gradient its `uRampRows`, and on the programs
/// that link GRADIENT_BATCH (the fill, rounded, shape and mesh gradient
/// variants) its `uGradMeta`, the batch's gradient kind and tile.
struct Program {
  program: glow::Program,
  size: glow::UniformLocation,
  ramp_rows: Option<glow::UniformLocation>,
  grad_meta: Option<glow::UniformLocation>,
}

/// A mesh program and the uniforms a mesh draw sets on it.
struct MeshProgram {
  program: Program,
  uniforms: MeshUniforms,
}

/// The per-draw uniforms of a mesh program: the map, the color, the clip
/// box and the rounded clip pair, the path-to-texel rows on the textured
/// variant and the gradient on the gradient variant.
struct MeshUniforms {
  map: glow::UniformLocation,
  color: glow::UniformLocation,
  clip: glow::UniformLocation,
  rclip: glow::UniformLocation,
  rclip_radii: glow::UniformLocation,
  /// `uTexelX` and `uTexelY`; None on the variants that sample nothing.
  texel: Option<RowUniforms>,
  /// `uGradX` and `uGradY`, and `uGradMeta` (the ramp row, the kind, the
  /// tile); None on the variants without a gradient.
  gradient: Option<(RowUniforms, glow::UniformLocation)>,
}

/// Two rows over the path's space, as a mesh draw sets them
/// (MESH_VERTEX).
struct RowUniforms {
  x: glow::UniformLocation,
  y: glow::UniformLocation,
}

impl RowUniforms {
  // The uniforms `{prefix}X` and `{prefix}Y` of `program` (`label` names
  // it).
  unsafe fn of(gl: &glow::Context, p: glow::Program, label: &str, prefix: &str) -> Result<RowUniforms, String> {
    Ok(RowUniforms {
      x: uniform(gl, p, label, &format!("{prefix}X"))?,
      y: uniform(gl, p, label, &format!("{prefix}Y"))?,
    })
  }
}

/// What a mesh program variant reads beside the solid one's uniforms.
#[derive(Clone, Copy, PartialEq)]
enum MeshVariant {
  Solid,
  Gradient,
  Textured,
}

impl MeshVariant {
  // The define that selects the variant in MESH_VERTEX and MESH_FRAGMENT,
  // right after SOURCE_HEADER; none for the solid one.
  fn define(self) -> &'static str {
    match self {
      MeshVariant::Solid => "",
      MeshVariant::Gradient => GRADIENT_DEFINE,
      MeshVariant::Textured => TEXTURED_DEFINE,
    }
  }
}

impl MeshProgram {
  // The mesh program `label` from the fragment parts `fragment` (the
  // vertex stage is MESH_VERTEX under the variant's define), linked, with
  // the uniforms its `variant` reads.
  unsafe fn link(
    gl: &glow::Context,
    label: &str,
    fragment: &[&str],
    variant: MeshVariant,
  ) -> Result<MeshProgram, String> {
    let program = Program::link(gl, label, &[SOURCE_HEADER, variant.define(), MESH_VERTEX], fragment)?;
    let p = program.program;
    let texel = if variant == MeshVariant::Textured { Some(RowUniforms::of(gl, p, label, "uTexel")?) } else { None };
    let gradient = if variant == MeshVariant::Gradient {
      Some((RowUniforms::of(gl, p, label, "uGrad")?, uniform(gl, p, label, "uGradMeta")?))
    } else {
      None
    };
    let uniforms = MeshUniforms {
      map: uniform(gl, p, label, "uMap")?,
      color: uniform(gl, p, label, "uColor")?,
      clip: uniform(gl, p, label, "uClip")?,
      rclip: uniform(gl, p, label, "uRClip")?,
      rclip_radii: uniform(gl, p, label, "uRClipRadii")?,
      texel,
      gradient,
    };
    Ok(MeshProgram { program, uniforms })
  }
}

/// The uniforms a glyph batch sets on the glyph program for its coverage
/// policy.
struct GlyphUniforms {
  mode: glow::UniformLocation,
  gamma: glow::UniformLocation,
  contrast: glow::UniformLocation,
  gamma_ratios: glow::UniformLocation,
}

impl Executor {
  /// Over the context current on the calling thread; compiles the ten
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
      &[
        SOURCE_HEADER,
        GRADIENT_DEFINE,
        FRAGMENT_INPUTS,
        FRAGMENT_MATH,
        FRAGMENT_ROUNDED_CLIP,
        GRADIENT_INPUTS,
        GRADIENT_COMMON,
        GRADIENT_BATCH,
        FILL_FRAGMENT,
      ],
    )?;
    let rounded = Program::link(
      &gl,
      "rounded",
      &[SOURCE_HEADER, VERTEX_COMMON, ROUNDED_VERTEX],
      &[SOURCE_HEADER, FRAGMENT_INPUTS, FRAGMENT_MATH, FRAGMENT_ROUNDED_CLIP, ROUNDED_FRAGMENT],
    )?;
    let rounded_gradient = Program::link(
      &gl,
      "rounded gradient",
      &[SOURCE_HEADER, VERTEX_COMMON, ROUNDED_VERTEX],
      &[
        SOURCE_HEADER,
        GRADIENT_DEFINE,
        FRAGMENT_INPUTS,
        FRAGMENT_MATH,
        FRAGMENT_ROUNDED_CLIP,
        GRADIENT_INPUTS,
        GRADIENT_COMMON,
        GRADIENT_BATCH,
        ROUNDED_FRAGMENT,
      ],
    )?;
    let shape = Program::link(
      &gl,
      "shape",
      &[SOURCE_HEADER, VERTEX_COMMON, SHAPE_VERTEX],
      &[SOURCE_HEADER, FRAGMENT_INPUTS, FRAGMENT_MATH, FRAGMENT_ROUNDED_CLIP, SHAPE_FRAGMENT],
    )?;
    let shape_gradient = Program::link(
      &gl,
      "shape gradient",
      &[SOURCE_HEADER, VERTEX_COMMON, SHAPE_VERTEX],
      &[
        SOURCE_HEADER,
        GRADIENT_DEFINE,
        FRAGMENT_INPUTS,
        FRAGMENT_MATH,
        FRAGMENT_ROUNDED_CLIP,
        GRADIENT_INPUTS,
        GRADIENT_COMMON,
        GRADIENT_BATCH,
        SHAPE_FRAGMENT,
      ],
    )?;
    let glyph = Program::link(
      &gl,
      "glyph",
      &[SOURCE_HEADER, VERTEX_COMMON, FILL_VERTEX],
      &[
        SOURCE_HEADER,
        FRAGMENT_INPUTS,
        FRAGMENT_MATH,
        FRAGMENT_ROUNDED_CLIP,
        GRADIENT_INPUTS,
        GRADIENT_COMMON,
        GLYPH_FRAGMENT,
      ],
    )?;
    let mesh = MeshProgram::link(&gl, "mesh", &[SOURCE_HEADER, FRAGMENT_MATH, MESH_FRAGMENT], MeshVariant::Solid)?;
    let mesh_gradient = MeshProgram::link(
      &gl,
      "mesh gradient",
      &[SOURCE_HEADER, GRADIENT_DEFINE, FRAGMENT_MATH, GRADIENT_INPUTS, GRADIENT_COMMON, GRADIENT_BATCH, MESH_FRAGMENT],
      MeshVariant::Gradient,
    )?;
    let mesh_textured = MeshProgram::link(
      &gl,
      "mesh textured",
      &[SOURCE_HEADER, TEXTURED_DEFINE, FRAGMENT_MATH, MESH_FRAGMENT],
      MeshVariant::Textured,
    )?;
    let (gp, label) = (glyph.program, "glyph");
    let glyph_uniforms = GlyphUniforms {
      mode: uniform(&gl, gp, label, "uMode")?,
      gamma: uniform(&gl, gp, label, "uGamma")?,
      contrast: uniform(&gl, gp, label, "uContrast")?,
      gamma_ratios: uniform(&gl, gp, label, "uGammaRatios")?,
    };
    // A sampler uniform names a unit, not a texture: set once, the unit
    // never changes. The shape and glyph programs sample `uTexture` (an
    // image, a texture, the atlas) on the one unit; the gradient programs
    // and the glyph program sample the ramp on its own. The fill and the
    // rounded program sample nothing, and the shape program's gradient variant no
    // `uTexture` (it draws no image, so the compiler drops the sampler);
    // of the mesh programs only the textured variant samples `uTexture`.
    for (program, label) in
      [(shape.program, "shape"), (glyph.program, "glyph"), (mesh_textured.program.program, "mesh textured")]
    {
      let texture_uniform = uniform(&gl, program, label, "uTexture")?;
      gl.use_program(Some(program));
      gl.uniform_1_i32(Some(&texture_uniform), TEXTURE_UNIT as i32);
    }
    for (program, label) in [
      (fill_gradient.program, "fill gradient"),
      (rounded_gradient.program, "rounded gradient"),
      (shape_gradient.program, "shape gradient"),
      (glyph.program, "glyph"),
      (mesh_gradient.program.program, "mesh gradient"),
    ] {
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
    let mesh_vao = gl.create_vertex_array().map_err(|e| format!("mesh vertex array: {e}"))?;
    gl.bind_vertex_array(Some(mesh_vao));
    // Per vertex; a mesh draw points it at the mesh's own buffer.
    gl.enable_vertex_attrib_array(ATTR_MESH_POS);
    gl.bind_vertex_array(None);
    let samplers = HashMap::new();
    Ok(Executor {
      gl,
      fill,
      fill_gradient,
      rounded,
      rounded_gradient,
      shape,
      shape_gradient,
      glyph,
      mesh,
      mesh_gradient,
      mesh_textured,
      vao,
      instances,
      mesh_vao,
      samplers,
      glyph_uniforms,
      ramp,
      ramps: RampTable::new(),
    })
  }

  /// Delete `names`: the textures, paint shader programs and path mesh
  /// buffers whose last handle dropped since the last draw (see
  /// `super::DeletionQueue`). On this thread, context current.
  pub(super) fn reclaim(&self, names: Vec<Freed>) {
    for name in names {
      match name {
        Freed::Texture(texture) => unsafe { self.gl.delete_texture(texture) },
        Freed::Program(program) => unsafe { self.gl.delete_program(program) },
        Freed::Buffers(vertices, indices) => unsafe {
          self.gl.delete_buffer(vertices);
          self.gl.delete_buffer(indices);
        },
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
  /// sampler bindings are set per batch and cleared after. A path mesh
  /// uploads at its first draw, its buffers' names to go back to `queue`,
  /// the drawing renderer's. The stencil test is off on return, its write
  /// mask every bit. Reports the instances written, the draw calls issued,
  /// the path meshes drawn and tessellated and the stencil clip draws
  /// (`super::report`), zeros for an empty list; no draw call and no clip
  /// draw for a list that emits only clips (every draw culled), whose
  /// batches are never issued.
  pub(super) fn draw(
    &mut self,
    target: Target,
    list: &DisplayList,
    queue: &Arc<DeletionQueue>,
  ) -> Result<DrawReport, String> {
    self.ramps.begin_frame();
    let walked = walk::walk(list, target.size, &mut self.ramps);
    unsafe { self.create_samplers(&walked) }?;
    // The rows the walk assigned, before any batch samples them. A row a
    // gradient keeps across draws is not uploaded again; a row is
    // re-assigned only when a draw holds more distinct gradients than the
    // atlas has free rows, and its upload may then wait on the draw before,
    // which sampled it.
    unsafe { self.upload_ramps() };
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
      // The stencil clear honours the write mask, and the clips count up
      // from 0. A list that ends with stencil clips in effect leaves them in
      // the stencil; this clear of the next draw takes them.
      gl.stencil_mask(STENCIL_ALL_BITS);
      gl.clear_stencil(0);
      // Clearing the depth and stencil of a target that has none is a
      // no-op; every target alloy draws into has DEPTH24_STENCIL8 (gl/rig.rs,
      // the window's stencil size 8), which the stencil clips need.
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
      // Nothing emitted means no scissor: the test is already off here. The
      // report still counts a mesh tessellated and then rejected, but no
      // draw call and no clip draw: a list whose every draw was culled
      // leaves its clip batches undrawn.
      if instances.is_empty() && walked.mesh_draws.is_empty() {
        return Ok(DrawReport { draw_calls: 0, clips: 0, ..super::report(&walked) });
      }
      let programs = [
        &self.glyph,
        &self.fill,
        &self.fill_gradient,
        &self.rounded,
        &self.rounded_gradient,
        &self.shape_gradient,
        &self.mesh.program,
        &self.mesh_gradient.program,
        &self.mesh_textured.program,
        &self.shape,
      ];
      for program in programs {
        gl.use_program(Some(program.program));
        gl.uniform_2_f32(Some(&program.size), width as f32, height as f32);
        if let Some(rows) = &program.ramp_rows {
          gl.uniform_1_f32(Some(rows), RAMP_ROWS as f32);
        }
      }
      // The shape program is in use, as the loop left it. None after a
      // paint batch: its program is the shader's, so the next batch binds.
      let mut in_use = Some(self.shape.program);
      gl.bind_vertex_array(Some(self.vao));
      gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.instances));
      // The stencil clips' quads ride in the one instance buffer after the
      // walk's instances: `clip_slots` holds each clip draw's quad index,
      // None for a mesh.
      let mut clip_quads = Vec::new();
      let clip_slots: Vec<Option<usize>> = walked
        .clip_draws
        .iter()
        .map(|clip| {
          let (_, Cover::Quad(quad)) = clip_test(clip) else { return None };
          clip_quads.push(*quad);
          Some(instances.len() + clip_quads.len() - 1)
        })
        .collect();
      if clip_quads.is_empty() {
        if !instances.is_empty() {
          gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, instance_bytes(instances), glow::STREAM_DRAW);
        }
      } else {
        let stride = std::mem::size_of::<Instance>();
        let total = (instances.len() + clip_quads.len()) * stride;
        gl.buffer_data_size(glow::ARRAY_BUFFER, total as i32, glow::STREAM_DRAW);
        if !instances.is_empty() {
          gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, 0, instance_bytes(instances));
        }
        gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, (instances.len() * stride) as i32, instance_bytes(&clip_quads));
      }
      // Whether the mesh VAO is bound; the instance VAO is, with its
      // buffer on ARRAY_BUFFER, when it is not.
      let mut mesh_bound = false;
      // One draw per batch, in painter's order: a batch ends where the
      // texture or its sampling changes, where fills, shapes, text and path
      // meshes meet, at a glyph run's policy, at every path mesh, or where
      // gradient and solid quads or gradients of another kind or tile meet
      // (walk.rs). A batch draws through its
      // program, the variant its gradient or texture asks for, switched
      // only where the program changes, from the VAO of its kind: a mesh
      // batch is one path's mesh, drawn as elements from its own buffers;
      // a quad batch is instanced, and since GLES 3.0 has no base instance
      // it is drawn by re-pointing the attributes at its first instance.
      let ramped = walked.batches.iter().any(needs_ramp);
      if ramped {
        self.bind_ramp();
      }
      gl.active_texture(glow::TEXTURE0 + TEXTURE_UNIT);
      // A stencil clip batch writes the stencil only, through its own test
      // and op; every other batch tests at its depth (off at 0) and keeps
      // the stencil.
      for batch in &walked.batches {
        if batch.program == BatchProgram::Clip {
          let ((func, reference, op), cover) = clip_test(&walked.clip_draws[batch.start]);
          gl.enable(glow::STENCIL_TEST);
          gl.stencil_func(func, reference as i32, STENCIL_ALL_BITS);
          gl.stencil_op(glow::KEEP, glow::KEEP, op);
          gl.color_mask(false, false, false, false);
          match cover {
            Cover::Mesh(draw) => {
              if !mesh_bound {
                gl.bind_vertex_array(Some(self.mesh_vao));
                mesh_bound = true;
              }
              self.draw_mesh(draw, queue, &mut in_use);
            }
            Cover::Quad(_) => {
              if mesh_bound {
                gl.bind_vertex_array(Some(self.vao));
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.instances));
                mesh_bound = false;
              }
              let slot = clip_slots[batch.start].expect("a clip quad has its slot in the instance buffer");
              point_attributes(gl, slot);
              if in_use != Some(self.fill.program) {
                gl.use_program(Some(self.fill.program));
                in_use = Some(self.fill.program);
              }
              gl.draw_arrays_instanced(glow::TRIANGLES, 0, VERTICES_PER_QUAD, 1);
            }
          }
          gl.color_mask(true, true, true, true);
          continue;
        }
        if batch.stencil == 0 {
          gl.disable(glow::STENCIL_TEST);
        } else {
          gl.enable(glow::STENCIL_TEST);
          gl.stencil_func(glow::EQUAL, batch.stencil as i32, STENCIL_ALL_BITS);
          gl.stencil_op(glow::KEEP, glow::KEEP, glow::KEEP);
        }
        if batch.program == BatchProgram::Mesh {
          if !mesh_bound {
            gl.bind_vertex_array(Some(self.mesh_vao));
            mesh_bound = true;
          }
          self.draw_mesh(&walked.mesh_draws[batch.start], queue, &mut in_use);
          continue;
        }
        if mesh_bound {
          // The pointers read the buffer bound when they are set.
          gl.bind_vertex_array(Some(self.vao));
          gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.instances));
          mesh_bound = false;
        }
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
          (BatchProgram::Rounded, false) => &self.rounded,
          (BatchProgram::Rounded, true) => &self.rounded_gradient,
          (BatchProgram::Shape, false) => &self.shape,
          (BatchProgram::Shape, true) => &self.shape_gradient,
          (BatchProgram::Glyph, _) => &self.glyph,
          (BatchProgram::Paint, _) => unreachable!("a paint batch carries its draw (walk.rs Batch)"),
          (BatchProgram::Mesh, _) => unreachable!("a mesh batch draws above"),
          (BatchProgram::Clip, _) => unreachable!("a clip batch draws above"),
        };
        if in_use != Some(program.program) {
          gl.use_program(Some(program.program));
          in_use = Some(program.program);
        }
        if batch.program == BatchProgram::Glyph {
          let policy = batch.policy.expect("a glyph batch carries its coverage policy (walk.rs Batch)");
          self.glyph_state(policy);
        }
        // A gradient batch's one kind and tile (GRADIENT_BATCH).
        if let (Some(class), Some(meta)) = (batch.gradient, &program.grad_meta) {
          gl.uniform_4_f32(Some(meta), 0.0, class.kind, class.tile, 0.0);
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
      // The stencil test off and every bit writable, as the clear found it.
      gl.disable(glow::STENCIL_TEST);
      if mesh_bound {
        gl.bind_vertex_array(Some(self.vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.instances));
      }
      // Leave the instance VAO pointing at the first instance, as `new` set
      // it. The mesh VAO keeps the last mesh's buffers bound until the next
      // mesh draw rebinds it; a buffer deleted meanwhile lives on in it
      // until then, which is the GL's rule, not a leak.
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
  // executor does not have yet: (sampling, clamp) for a texture batch (a
  // textured mesh batch among them) and for the ramp a glyph or gradient
  // batch samples, each input's own for a paint batch.
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

  // Upload the ramp atlas rows `ramps` assigned since the last upload, one
  // row each, from the gradient each now holds; TEXTURE_UNIT is the
  // active unit again after, with nothing bound on RAMP_UNIT.
  unsafe fn upload_ramps(&mut self) {
    let dirty = self.ramps.take_dirty();
    if dirty.is_empty() {
      return;
    }
    let gl = &self.gl;
    gl.active_texture(glow::TEXTURE0 + RAMP_UNIT);
    gl.bind_texture(glow::TEXTURE_2D, Some(self.ramp));
    let mut bytes = Vec::with_capacity(RAMP_WIDTH * BYTES_PER_RAMP_TEXEL);
    for row in dirty {
      bytes.clear();
      for texel in self.ramps.ramp(row).iter().take(RAMP_WIDTH) {
        bytes.extend_from_slice(texel);
      }
      bytes.resize(RAMP_WIDTH * BYTES_PER_RAMP_TEXEL, 0);
      gl.tex_sub_image_2d(
        glow::TEXTURE_2D,
        0,
        0,
        row as i32,
        RAMP_WIDTH as i32,
        1,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelUnpackData::Slice(Some(&bytes)),
      );
    }
    gl.bind_texture(glow::TEXTURE_2D, None);
    gl.active_texture(glow::TEXTURE0 + TEXTURE_UNIT);
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

  // One path mesh draw: its program (the gradient variant for a gradient,
  // the textured one for a texture, else the solid one) in use unless
  // `in_use` already is, the mesh's own buffers (uploaded now at its first
  // draw, `mesh_buffers`) bound into the mesh VAO, its map, color and clips
  // (zero radii: no rounded clip), its gradient's or its texture's rows,
  // and one element draw over all its indices. The mesh VAO is bound and
  // TEXTURE_UNIT is the active unit, as `draw` left them; the active unit
  // is TEXTURE_UNIT again after. A mesh whose buffers cannot be created
  // does not draw, warned once.
  unsafe fn draw_mesh(&self, draw: &MeshDraw, queue: &Arc<DeletionQueue>, in_use: &mut Option<glow::Program>) {
    let gl = &self.gl;
    let Some((vertices, indices)) = self.mesh_buffers(&draw.mesh, queue) else {
      warn_once!("[anvil] Renderer: a path mesh's GPU buffers could not be created; the path does not draw");
      return;
    };
    // The pointer reads the buffer bound when it is set; the element
    // binding is the bound VAO's.
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertices));
    let stride = std::mem::size_of::<MeshVertex>() as i32;
    gl.vertex_attrib_pointer_f32(ATTR_MESH_POS, MESH_VERTEX_FLOATS as i32, glow::FLOAT, false, stride, 0);
    gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(indices));
    let mesh = match (&draw.gradient, draw.texture) {
      (Some(_), _) => &self.mesh_gradient,
      (None, Some(_)) => &self.mesh_textured,
      (None, None) => &self.mesh,
    };
    if *in_use != Some(mesh.program.program) {
      gl.use_program(Some(mesh.program.program));
      *in_use = Some(mesh.program.program);
    }
    let u = &mesh.uniforms;
    gl.uniform_matrix_3_f32_slice(Some(&u.map), false, &map_columns(&draw.map));
    let [r, g, b, a] = draw.color;
    gl.uniform_4_f32(Some(&u.color), r, g, b, a);
    let [x0, y0, x1, y1] = draw.clip;
    gl.uniform_4_f32(Some(&u.clip), x0, y0, x1, y1);
    let (rclip, radii) = match draw.rounded {
      Some(r) => ([r.rect.min.x, r.rect.min.y, r.rect.max.x, r.rect.max.y], r.radii),
      None => ([0.0; 4], [0.0; 4]),
    };
    gl.uniform_4_f32(Some(&u.rclip), rclip[0], rclip[1], rclip[2], rclip[3]);
    gl.uniform_4_f32(Some(&u.rclip_radii), radii[0], radii[1], radii[2], radii[3]);
    if let (Some(gradient), Some((rows, meta))) = (&draw.gradient, &u.gradient) {
      let [[x0, x1, x2], [y0, y1, y2]] = gradient.rows;
      gl.uniform_3_f32(Some(&rows.x), x0, x1, x2);
      gl.uniform_3_f32(Some(&rows.y), y0, y1, y2);
      gl.uniform_4_f32(Some(meta), gradient.row as f32, gradient.kind(), gradient.tile(), 0.0);
    }
    match (draw.texture, &u.texel) {
      (Some((name, sampling)), Some(rows)) if draw.gradient.is_none() => {
        let sampler = self.sampler(sampling, TextureWrap::Clamp);
        gl.bind_texture(glow::TEXTURE_2D, Some(name));
        gl.bind_sampler(TEXTURE_UNIT, Some(sampler));
        let [[x0, x1, x2], [y0, y1, y2]] =
          draw.texel.expect("a textured mesh draw carries its texel map (walk.rs MeshDraw)");
        gl.uniform_3_f32(Some(&rows.x), x0, x1, x2);
        gl.uniform_3_f32(Some(&rows.y), y0, y1, y2);
      }
      _ => {
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.bind_sampler(TEXTURE_UNIT, None);
      }
    }
    gl.draw_elements(glow::TRIANGLES, draw.mesh.indices.len() as i32, glow::UNSIGNED_INT, 0);
  }

  // The vertex and index buffers of `mesh` on this executor's context: the
  // ones its slot holds when the slot is this renderer's (`queue` is the
  // drawing renderer's), else new ones, uploaded once (STATIC_DRAW) and
  // stored in the slot with `queue`, which the mesh's drop sends their
  // names back to. A slot of another renderer's queue is one that died with
  // its context (raster/recover.rs makes a new renderer): its names go onto
  // that dead renderer's queue, which nobody drains, which is right, since
  // those names died with that context (the renderer module doc). The
  // mesh VAO is bound: an upload leaves the new buffers bound on
  // ARRAY_BUFFER and as its element buffer. None when GL creates no buffer.
  unsafe fn mesh_buffers(&self, mesh: &Mesh, queue: &Arc<DeletionQueue>) -> Option<(glow::Buffer, glow::Buffer)> {
    let gl = &self.gl;
    // A poisoned lock only means a panic elsewhere while it was held; the
    // slot is still whole, so the draw goes on.
    let mut slot = mesh.gpu.lock().unwrap_or_else(|e| e.into_inner());
    match gpu_slot(slot.as_ref().map(|gpu| &gpu.queue), queue) {
      GpuSlot::Keep => return slot.as_ref().map(|gpu| (gpu.vertices, gpu.indices)),
      GpuSlot::Replace => {
        if let Some(old) = slot.take() {
          old.queue.push(Freed::Buffers(old.vertices, old.indices));
        }
      }
      GpuSlot::Upload => {}
    }
    let vertices = gl.create_buffer().ok()?;
    let indices = match gl.create_buffer() {
      Ok(indices) => indices,
      Err(_) => {
        gl.delete_buffer(vertices);
        return None;
      }
    };
    // Sound: a vertex is two f32s and an index a u32, neither with padding
    // (walk.rs asserts the vertex's size), so any byte view is initialized;
    // the slices borrow the mesh for the upload only.
    let bytes =
      std::slice::from_raw_parts(mesh.vertices.as_ptr() as *const u8, std::mem::size_of_val(&mesh.vertices[..]));
    gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertices));
    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STATIC_DRAW);
    let bytes =
      std::slice::from_raw_parts(mesh.indices.as_ptr() as *const u8, std::mem::size_of_val(&mesh.indices[..]));
    gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(indices));
    gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, bytes, glow::STATIC_DRAW);
    *slot = Some(GpuMesh { vertices, indices, queue: queue.clone() });
    Some((vertices, indices))
  }

  // A glyph batch's uniforms: `policy`. The glyph program is in use, as
  // `draw` left it; each instance carries its own gradient.
  unsafe fn glyph_state(&self, policy: CoveragePolicy) {
    let (gl, u) = (&self.gl, &self.glyph_uniforms);
    gl.uniform_1_i32(Some(&u.mode), policy.mode as i32);
    gl.uniform_1_f32(Some(&u.gamma), policy.gamma);
    gl.uniform_1_f32(Some(&u.contrast), policy.contrast);
    let [r0, r1, r2, r3] = policy.gamma_ratios();
    gl.uniform_4_f32(Some(&u.gamma_ratios), r0, r1, r2, r3);
  }
}

/// What a stencil clip draw covers: its quad, uploaded after the walk's
/// instances, or its mesh.
enum Cover<'a> {
  Quad(&'a Instance),
  Mesh(&'a MeshDraw),
}

// A stencil clip draw's test and op, (func, reference, the op on pass), and
// what it covers. A raise passes where the stencil equals its reference
// and increments it, a lower decrements it there; a restore passes where
// its depth is below the stencil (GL's LESS compares the reference to the
// stored value) and replaces the stencil with it.
fn clip_test(clip: &ClipDraw) -> ((u32, u8, u32), Cover<'_>) {
  fn cover(shape: &ClipShape) -> Cover<'_> {
    match shape {
      ClipShape::Quad(quad) => Cover::Quad(quad),
      ClipShape::Mesh(draw) => Cover::Mesh(draw),
    }
  }
  match clip {
    ClipDraw::Raise { reference, shape } => ((glow::EQUAL, *reference, glow::INCR), cover(shape)),
    ClipDraw::Lower { reference, shape } => ((glow::EQUAL, *reference, glow::DECR), cover(shape)),
    ClipDraw::Restore { to, area } => ((glow::LESS, *to, glow::REPLACE), Cover::Quad(area)),
  }
}

// `instances` as the bytes the instance buffer takes.
fn instance_bytes(instances: &[Instance]) -> &[u8] {
  // Sound: Instance is repr(C) with every field an f32, so it has no
  // padding (walk.rs asserts its size) and any byte view of it is
  // initialized; the slice borrows `instances`.
  unsafe { std::slice::from_raw_parts(instances.as_ptr() as *const u8, std::mem::size_of_val(instances)) }
}

// Whether `batch` samples the gradient ramp: a glyph batch (the glyph
// program declares it, gradient or not), a gradient quad batch or a
// gradient mesh batch (both carry their gradient class).
fn needs_ramp(batch: &Batch) -> bool {
  batch.program == BatchProgram::Glyph || batch.gradient.is_some()
}

// `p` as `uMap` takes it: the 3x3 M of euclid's row-vector convention,
// (x', y', w') = (x, y, 1) * M, whose rows are (m11, m12, m14), (m21, m22,
// m24) and (m41, m42, m44), in the column-major order glow's
// `uniform_matrix_3_f32_slice` takes untransposed: column j first holds the
// coefficients of output j, so GLSL's `v * M` is (dot(v, col 0), dot(v,
// col 1), dot(v, col 2)) = (x', y', w').
fn map_columns(p: &Projective) -> [f32; 9] {
  let [m11, m12, m21, m22] = p.linear;
  let [m41, m42] = p.offset;
  let [m14, m24, m44] = p.persp;
  [m11, m21, m41, m12, m22, m42, m14, m24, m44]
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

// The ramp atlas: `RAMP_WIDTH` x `RAMP_ROWS` rgba8, blank rows until a draw
// uploads a gradient's into one. Its own parameters are linear and clamped, but
// `draw` samples it through the linear sampler object, as every texture
// here is sampled.
unsafe fn create_ramp(gl: &glow::Context) -> Result<glow::Texture, String> {
  let ramp = gl.create_texture().map_err(|e| format!("shape ramp texture: {e}"))?;
  gl.bind_texture(glow::TEXTURE_2D, Some(ramp));
  gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
  gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
  gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
  gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
  let blank = vec![0u8; RAMP_WIDTH * RAMP_ROWS as usize * BYTES_PER_RAMP_TEXEL];
  gl.tex_image_2d(
    glow::TEXTURE_2D,
    0,
    glow::RGBA8 as i32,
    RAMP_WIDTH as i32,
    RAMP_ROWS as i32,
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
  // source parts, linked, with its `uSize`, when it samples a gradient its
  // `uRampRows`, and when it reads a batch's gradient its `uGradMeta`.
  unsafe fn link(gl: &glow::Context, label: &str, vertex: &[&str], fragment: &[&str]) -> Result<Program, String> {
    let program = link_program(gl, label, &vertex.concat(), &fragment.concat())?;
    let size = uniform(gl, program, label, "uSize")?;
    let ramp_rows = gl.get_uniform_location(program, "uRampRows");
    let grad_meta = gl.get_uniform_location(program, "uGradMeta");
    Ok(Program { program, size, ramp_rows, grad_meta })
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
