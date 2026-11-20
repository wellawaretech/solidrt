//! The GL half of a draw: the fill, rounded, shape and glyph programs (the
//! fill, the rounded and the shape program each with a gradient variant),
//! the mesh program (with a gradient and a textured variant), the
//! composite program of a layer's effects (a shader blend mode, a color
//! matrix, a blurred image), the blur program and the shadow program (a
//! mask-blurred rect or rounded rect, evaluated analytically), thirteen
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
//! (`walk::GradientInstance`), with the v of its ramp row, and the
//! fragment reads both interpolated (`vGrad`), no divide; the kind and
//! the tile are the batch's or the mesh draw's uniform (`uGradMeta`), and
//! only the glyph program, whose runs join whatever their gradient, reads
//! them per instance (`vGradMeta`). Its ramp is one row of the ramp atlas,
//! whose rows the walk assigns (`super::ramps`) and the draw uploads before
//! any batch samples them. A stencil clip (`walk::ClipDraw`) draws into
//! the stencil with the color writes off, a rect's quad through the fill
//! program and a rounded rect's or an oval's mesh through the solid mesh
//! program, and every later batch tests the stencil at its depth. A layer
//! (`walk::Layer`) draws its batches into a target of the layer pool
//! (`super::layers`) under a shifted viewport (`Frame`), and its composite,
//! a textured quad through the shape program, into the frame it opened on;
//! a shader-mode layer's composite goes through the composite program
//! instead, against a blitted copy of the destination under its rect, and
//! so does a layer with a color matrix or a blur, whose pixels first run
//! through the blur program's passes into the pool's single-sample storages
//! (`run_blur`). A backdrop layer starts from the target's pixels under its
//! rect, resolved into the scratch and drawn into its storage by the blur
//! program as a one-tap copy. Every other batch blends by its
//! fixed-function mode (`super::blend`), set where it changes. Raster thread only, with the context current.

use super::blend::{gl_factors, shader_mode, BLEND_SOURCE_OVER};
use super::blur::{blur_levels, gaussian_weights, halved, level_sigmas, DeviceBlur, BLUR_SIGMA_MIN, BLUR_TAPS_MAX};
use super::layers::{bucket, rescale_composite, LayerPool, LayerTarget};
use super::mesh::{gpu_slot, GpuMesh, GpuSlot, Mesh};
use super::ramps::{RampTable, RAMP_ROWS};
use super::record::DisplayList;
use super::schedule::{draws_ahead, schedule, Step};
use super::walk::MESH_VERTEX_FLOATS;
use super::walk::{self, Batch, BatchProgram, Instance, MeshDraw, MeshVertex, PaintDraw, Projective, Walked};
use super::walk::{ClipDraw, ClipShape};
use super::{DeletionQueue, Freed};
use crate::contract::{DrawReport, Target};
use crate::{BlendMode, ColorMatrix, CoveragePolicy, RAMP_WIDTH};
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
/// Whether a layer that does not read the pixels beneath it is drawn ahead
/// of the frame it composites into (`schedule`), so that frame's pass does
/// not break for it; false draws every layer inline, the walk's order. The
/// switch a driver fault would turn off, as a named entry of the register
/// (okf/backlog/gpu-driver-faults.md).
const DRAW_AHEAD: bool = true;

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
/// The texture unit `uBackdrop` samples: the destination copy a shader-mode
/// layer's composite blends with.
const BACKDROP_UNIT: u32 = 2;
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
// any map; the third is the ramp row's normalized v, computed here once per
// vertex (no divide per fragment), constant over the quad, so its
// interpolation is exact too. The fragment reads that one smooth vec3 where
// it once read the rows and the local point; the kind and the tile are the
// batch's uniform (GRADIENT_BATCH), except in the glyph program, which
// reads them per instance from the flat `vGradMeta` (the linker drops it
// from every other program). A solid instance carries zero rows: its
// vGrad.xy is zero and the whole is unread.
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
layout(location = 10) in vec4 aGradX;    // a gradient's x row and ramp row; a texel u row; a shadow's cutout
layout(location = 11) in vec4 aGradY;    // its y row and kind (0: none); a texel v row; the cutout's radii
uniform vec2 uSize;                      // the target in device pixels
uniform float uRampRows;                 // the ramp atlas's height in rows
out vec2 vUv;
out vec3 vGrad;                          // the gradient's parameter-space point, its ramp row's v
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
  // The ramp row's v at its texel centers, so the bilinear lookup reads
  // that row alone.
  vGrad = vec3(dot(aGradX.xyz, q), dot(aGradY.xyz, q), (aGradX.w + 0.5) / uRampRows);
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
// not read (vUv, vGradMeta). Two parts: ROUNDED_PLACE, the outputs and
// `placeRounded()`, which the shadow program's vertex stage calls too
// (SHADOW_VERTEX), and ROUNDED_VERTEX, the rounded program's `main`.
const ROUNDED_PLACE: &str = r"
layout(location = 6) in vec4 aRadii;     // corner radii in local pixels: tl, tr, br, bl
out vec2 vLocal;                         // position within the shape quad, local pixels from its origin
flat out vec4 vShape;                    // the shape quad's w, h; z, w unused
flat out vec4 vRadii;
void placeRounded() {
  vec2 p = place(quadCorner());
  vLocal = p - aTex.xy;
  vShape = vec4(aTex.zw, 0.0, 0.0);
  vRadii = aRadii;
}
";

const ROUNDED_VERTEX: &str = r"
void main() {
  placeRounded();
}
";

// The shadow program's vertex stage, after SOURCE_HEADER, VERTEX_COMMON and
// ROUNDED_PLACE: the rounded program's, the shape measured from the
// shape's own rect in `aTex` (walk.rs `push_shadow`: the quad lies the
// blur's reach outside it), the sigma from the stroke slot, and the cutout
// from the gradient slots (walk.rs `Instance::grad_x`, `grad_y`), which a
// shadow never uses for a gradient. `place` still writes `vGrad`; the
// shadow fragment never declares it, so the linker drops it.
const SHADOW_VERTEX: &str = r"
flat out float vSigma;                   // the blur's sigma, local pixels
flat out vec4 vCutout;                   // the cutout: x, y, w, h from the shape's origin; w or h 0: none
flat out vec4 vCutoutRadii;              // its corner radii, local pixels: tl, tr, br, bl
void main() {
  placeRounded();
  vSigma = aOffset.w;
  vCutout = aGradX;
  vCutoutRadii = aGradY;
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
uniform vec2 uFrag;       // the frame's window-to-device offset (exec.rs Frame)
// The fragment's device position, y down. At a pixel center it equals the
// device position the vertex stage once interpolated, exactly: gl_FragCoord
// is that center. It is right under perspective too, where a linearly
// interpolated device position would not be (GLSL ES 3.00 has no
// noperspective qualifier). uFrag is (0, H) on the target and (ox, oy + Hl)
// in a layer (exec.rs `Frame` derives it).
vec2 device() { return vec2(gl_FragCoord.x + uFrag.x, uFrag.y - gl_FragCoord.y); }
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
// GradientInstance) at the corner, interpolated, and the v of the
// gradient's ramp row, constant over the quad or the mesh. The kind and
// the tile are not here: the batch programs read them from a uniform
// (GRADIENT_BATCH), the glyph program from its own per-instance input
// (GLYPH_FRAGMENT).
const GRADIENT_INPUTS: &str = r"
in vec3 vGrad;            // the parameter-space point, the ramp row's v
";

// A gradient's evaluation, after GRADIENT_INPUTS: the color at this
// fragment, from the ramp atlas's row at the v the vertex stage computed
// (VERTEX_COMMON, MESH_VERTEX), for the gradient `kind` tiled by `tile`.
// The kinds are GradientKind's discriminants and the tile modes walk.rs
// TILE_*: they must match. No integer division or modulo: the kind
// and the tile arrive apart.
const GRADIENT_COMMON: &str = r"
uniform sampler2D uRamp;
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
  vec4 g = texture(uRamp, vec2(clamp(t, 0.0, 1.0), vGrad.z));
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
uniform vec2 uFrag;
vec2 device() { return vec2(gl_FragCoord.x + uFrag.x, uFrag.y - gl_FragCoord.y); }
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

// An analytic shadow (walk.rs `push_shadow`): the coverage of a rect or
// rounded rect blurred by a gaussian of sigma `vSigma`, evaluated at the
// fragment's local point, after Evan Wallace's "Fast Rounded Rectangle
// Shadows" (2016), generalized to per-corner radii. The blur of the box's
// indicator is a 2D integral of the gaussian over the box. Along x a row of
// the box is an interval, so its integral is a closed form in erf
// (`shadowRow`); along y it is integrated numerically, SHADOW_SAMPLES
// midpoint samples over the rows within +-SHADOW_REACH sigma of the
// fragment (`shadowMask`). In local coordinates centred on the shape, y down.
//
// The row's mask at x: with g the gaussian and Phi(t) = 0.5 + 0.5 * erf(t /
// sqrt 2) its standard CDF, the row [left, right] blurred along x is the
// integral from left to right of g(t - x) dt = Phi((right - x) / sigma) -
// Phi((left - x) / sigma), so erf's argument is (edge - x) / (sigma *
// sqrt 2), (edge - x) * SQRT_HALF / sigma. Wallace's `roundedBoxShadowX`
// writes erf((x + (-c, c)) * sqrt(0.5) / sigma) for his symmetric row (left
// = -c, right = c), which is the same difference, erf being odd; his y loop
// passes `point.y - y` as the row, as `shadowMask` does.
//
// Each side of a row is cut in where the row lies in a corner's reach:
// the top corner of radius r covers the rows from the top edge to its arc's
// centre, r below it, the bottom one likewise from the bottom edge, and the
// clamped radii (record.rs `clamp_radii`: a side's two radii sum to at most
// its length) keep the two reaches apart, so a row is in at most one per
// side. That picks the corner by the row's place in the box, exact for any
// clamped radii, where picking it by the row's half (Wallace's one radius
// needs neither) would break at the middle row when a radius exceeds half
// its side. With equal radii it is Wallace's row.
//
// The quad's edges lie SHADOW_REACH sigma out, where the mask is under
// half a percent of a channel step: the clip box is a factor, the rounded
// clip a product (the clip cuts the blurred shadow), no discard.
//
// A cutout (walk.rs `cutout_shadow`: the casting box the shadow is not
// drawn under) multiplies the mask by its outside coverage, the rounded-box
// distance in local space through `coverage()`: the one-pixel analytic ramp
// the rounded program's edges get, in device pixels under any scale. Its
// derivatives, and `roundedClip()`'s, are the only ones the program takes;
// the cutout is flat, so every fragment of a quad takes the same branch.
//
// After SOURCE_HEADER and the common fragment parts.
pub(crate) const SHADOW_FRAGMENT: &str = r"
in vec2 vLocal;
flat in vec4 vShape;      // the shape's w, h; z, w unused
flat in vec4 vRadii;      // corner radii, local pixels: tl, tr, br, bl
flat in float vSigma;     // the blur's sigma, local pixels
flat in vec4 vCutout;     // the cutout from the shape's origin: x, y, w, h; w or h 0: none
flat in vec4 vCutoutRadii; // its corner radii, local pixels: tl, tr, br, bl
// How many sigmas the quad reaches past the shape, and the y integral's
// half range (walk.rs SHADOW_REACH_SIGMAS must match).
const float SHADOW_REACH = 3.0;
// The y integral's midpoint samples, Wallace's count: the mask errs by up
// to about 0.01 on rounded corners and 0.03 to 0.04 on a near-circular one
// (8 samples halve that). The one knob of the shadow's accuracy.
const int SHADOW_SAMPLES = 4;
const float SQRT_HALF = 0.70710678118;
const float INV_SQRT_TWO_PI = 0.39894228040;
float gaussian(float x, float sigma) {
  return exp(-(x * x) / (2.0 * sigma * sigma)) * INV_SQRT_TWO_PI / sigma;
}
// erf per component: Abramowitz and Stegun 7.1.27, the approximation
// Wallace uses (error under 5e-4).
vec2 erf2(vec2 x) {
  vec2 s = sign(x), a = abs(x);
  x = 1.0 + (0.278393 + (0.230389 + 0.078108 * (a * a)) * a) * a;
  x *= x;
  return s - s / (x * x);
}
// How far one side of the row at height y (centred) of a box of half
// height hy is cut in by the side's corners, the top one of radius `top`
// (its arc's centre at y = top - hy) and the bottom one of radius `bottom`
// (at y = hy - bottom): r - sqrt(r^2 - d^2) for the corner whose reach the
// row lies in, d its distance past the arc's centre; 0 between them.
float cornerInset(float y, float top, float bottom, float hy) {
  float dt = max(top - hy - y, 0.0);
  float db = max(y - hy + bottom, 0.0);
  float it = top - sqrt(max(top * top - dt * dt, 0.0));
  float ib = bottom - sqrt(max(bottom * bottom - db * db, 0.0));
  return it + ib;
}
// The row at height y (centred) of the box of half size hs with the radii
// r (tl, tr, br, bl), blurred along x and read at x: the gaussian's integral
// from the row's left edge to its right edge.
float shadowRow(float x, float y, float sigma, vec4 r, vec2 hs) {
  float left = -hs.x + cornerInset(y, r.x, r.w, hs.y);
  float right = hs.x - cornerInset(y, r.y, r.z, hs.y);
  vec2 integral = 0.5 + 0.5 * erf2((vec2(left, right) - x) * (SQRT_HALF / sigma));
  return integral.y - integral.x;
}
// The blurred mask at p (centred): the rows' masks weighted by the gaussian
// of their distance y from p, the row at height p.y - y. The box's rows lie
// at y in [p.y - hs.y, p.y + hs.y], the weight within +-SHADOW_REACH sigma:
// the samples cover the overlap.
float shadowMask(vec2 p, vec2 hs, vec4 r, float sigma) {
  float low = p.y - hs.y;
  float high = p.y + hs.y;
  float start = clamp(-SHADOW_REACH * sigma, low, high);
  float end = clamp(SHADOW_REACH * sigma, low, high);
  float stride = (end - start) / float(SHADOW_SAMPLES);
  float y = start + stride * 0.5;
  float value = 0.0;
  for (int i = 0; i < SHADOW_SAMPLES; i++) {
    value += shadowRow(p.x, p.y - y, sigma, r, hs) * gaussian(y, sigma) * stride;
    y += stride;
  }
  return value;
}
void main() {
  vec2 dp = device();
  float inside = float(all(greaterThanEqual(dp, vClip.xy)) && all(lessThan(dp, vClip.zw)));
  vec2 hs = vShape.xy * 0.5;
  // The cutout first: where it covers the fragment whole, the mask is 0
  // and the y integral is skipped (most of a card's shadow quad lies under
  // the card). Its coverage takes derivatives, so it is read before the
  // branch, under the flat cutout test every fragment of a quad shares.
  float outside = 1.0;
  if (vCutout.z > 0.0 && vCutout.w > 0.0) {
    vec2 cutHalf = vCutout.zw * 0.5;
    outside = 1.0 - coverage(roundedBox(vLocal - (vCutout.xy + cutHalf), cutHalf, vCutoutRadii));
  }
  float mask = 0.0;
  if (outside > 0.0) {
    mask = clamp(shadowMask(vLocal - hs, hs, vRadii, vSigma), 0.0, 1.0) * outside;
  }
  fragColor = vColor * (mask * inside * roundedClip());
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
// ramp row's v as the third component (constant over the mesh), and the
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
uniform float uRampRows;              // the ramp atlas's height in rows
out vec3 vGrad;                       // the gradient's parameter-space point, its ramp row's v
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
  // The ramp row's v at its texel centers, as VERTEX_COMMON's `place`.
  vGrad = vec3(dot(uGradX, q), dot(uGradY, q), (uGradMeta.x + 0.5) / uRampRows);
#elif defined(TEXTURED)
  vTexel = vec2(dot(uTexelX, q), dot(uTexelY, q));
#endif
}
";

// A path mesh's fragment stage: the fill program's, a triangle's edges are
// MSAA's and its coverage is 1 inside. The color and the clips are the
// draw's uniforms (walk.rs MeshDraw); the clip box and the rounded clip a
// factor, and a fragment whose factor is zero discards after every
// derivative and sample, so a path under a blend mode other than
// source-over writes nothing outside its clips (under Clear, Source and the
// other modes `blend::keeps_destination` refuses, a zero fragment changes
// the pixel); the rounded clip's all-zero radii return before any derivative.
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
uniform vec2 uFrag;       // the frame's window-to-device offset (exec.rs Frame)
vec2 device() { return vec2(gl_FragCoord.x + uFrag.x, uFrag.y - gl_FragCoord.y); }
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
  float f = inside * roundedClip();
  if (f <= 0.0) discard;
  fragColor = color * f;
}
";

// The shader blend modes, after SOURCE_HEADER: one function per mode over
// premultiplied `src` and `dst` as the W3C Compositing and Blending Level 1
// spec defines them, the separable ones through B(cb, cs) on
// unpremultiplied color, the non-separable four through SetLum, SetSat and
// ClipColor with the spec's luminance weights, the result premultiplied:
// res.a = sa + da - sa * da, res.rgb = (1 - da) * src.rgb + (1 - sa) *
// dst.rgb + sa * da * B(dst.rgb / da, src.rgb / sa), the divisions guarded
// for zero alpha. A zero `src` returns `dst` exactly under every mode. The
// mode constants are blend.rs BLEND_*, BlendMode's discriminants: they must
// match.
pub(crate) const COMPOSITE_BLEND: &str = r"
uniform int uBlend;       // a BLEND_* below
const int BLEND_SOURCE_OVER = 3;
const int BLEND_OVERLAY = 15;
const int BLEND_DARKEN = 16;
const int BLEND_LIGHTEN = 17;
const int BLEND_COLOR_DODGE = 18;
const int BLEND_COLOR_BURN = 19;
const int BLEND_HARD_LIGHT = 20;
const int BLEND_SOFT_LIGHT = 21;
const int BLEND_DIFFERENCE = 22;
const int BLEND_EXCLUSION = 23;
const int BLEND_MULTIPLY = 24;
const int BLEND_HUE = 25;
const int BLEND_SATURATION = 26;
const int BLEND_COLOR = 27;
const int BLEND_LUMINOSITY = 28;
// The spec's luminance weights (Lum).
const vec3 LUM_WEIGHTS = vec3(0.3, 0.59, 0.11);
// The smallest alpha a premultiplied color is divided by, and the smallest
// denominator ClipColor divides by: below it the color is taken as zero.
const float BLEND_EPS = 1e-6;
// A premultiplied color's straight color, within 0..1 (unorm8 rounding may
// put a channel a hair above its alpha); zero for a transparent one.
vec3 unpremultiply(vec4 c) {
  return c.a > BLEND_EPS ? clamp(c.rgb / c.a, 0.0, 1.0) : vec3(0.0);
}
vec3 multiplyB(vec3 cb, vec3 cs) {
  return cb * cs;
}
vec3 screenB(vec3 cb, vec3 cs) {
  return cb + cs - cb * cs;
}
vec3 hardLightB(vec3 cb, vec3 cs) {
  vec3 low = multiplyB(cb, 2.0 * cs);
  vec3 high = screenB(cb, 2.0 * cs - 1.0);
  return mix(low, high, vec3(greaterThan(cs, vec3(0.5))));
}
float colorDodge1(float cb, float cs) {
  if (cb <= 0.0) return 0.0;
  if (cs >= 1.0) return 1.0;
  return min(1.0, cb / (1.0 - cs));
}
float colorBurn1(float cb, float cs) {
  if (cb >= 1.0) return 1.0;
  if (cs <= 0.0) return 0.0;
  return 1.0 - min(1.0, (1.0 - cb) / cs);
}
float softLight1(float cb, float cs) {
  if (cs <= 0.5) return cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb);
  float d = cb <= 0.25 ? ((16.0 * cb - 12.0) * cb + 4.0) * cb : sqrt(cb);
  return cb + (2.0 * cs - 1.0) * (d - cb);
}
vec3 colorDodgeB(vec3 cb, vec3 cs) {
  return vec3(colorDodge1(cb.r, cs.r), colorDodge1(cb.g, cs.g), colorDodge1(cb.b, cs.b));
}
vec3 colorBurnB(vec3 cb, vec3 cs) {
  return vec3(colorBurn1(cb.r, cs.r), colorBurn1(cb.g, cs.g), colorBurn1(cb.b, cs.b));
}
vec3 softLightB(vec3 cb, vec3 cs) {
  return vec3(softLight1(cb.r, cs.r), softLight1(cb.g, cs.g), softLight1(cb.b, cs.b));
}
float lum(vec3 c) {
  return dot(c, LUM_WEIGHTS);
}
vec3 clipColor(vec3 c) {
  float l = lum(c);
  float n = min(min(c.r, c.g), c.b);
  float x = max(max(c.r, c.g), c.b);
  if (n < 0.0) c = l + (c - l) * l / max(l - n, BLEND_EPS);
  if (x > 1.0) c = l + (c - l) * (1.0 - l) / max(x - l, BLEND_EPS);
  return c;
}
vec3 setLum(vec3 c, float l) {
  return clipColor(c + (l - lum(c)));
}
float sat(vec3 c) {
  return max(max(c.r, c.g), c.b) - min(min(c.r, c.g), c.b);
}
// The spec's SetSat: the largest channel to s, the smallest to 0, the
// middle one scaled between; all zero for a gray.
vec3 setSat(vec3 c, float s) {
  float n = min(min(c.r, c.g), c.b);
  float x = max(max(c.r, c.g), c.b);
  return x > n ? (c - n) * s / (x - n) : vec3(0.0);
}
// B(cb, cs) of the mode `uBlend`, on straight color.
vec3 blendColor(vec3 cb, vec3 cs) {
  if (uBlend == BLEND_OVERLAY) return hardLightB(cs, cb);
  if (uBlend == BLEND_DARKEN) return min(cb, cs);
  if (uBlend == BLEND_LIGHTEN) return max(cb, cs);
  if (uBlend == BLEND_COLOR_DODGE) return colorDodgeB(cb, cs);
  if (uBlend == BLEND_COLOR_BURN) return colorBurnB(cb, cs);
  if (uBlend == BLEND_HARD_LIGHT) return hardLightB(cb, cs);
  if (uBlend == BLEND_SOFT_LIGHT) return softLightB(cb, cs);
  if (uBlend == BLEND_DIFFERENCE) return abs(cb - cs);
  if (uBlend == BLEND_EXCLUSION) return cb + cs - 2.0 * cb * cs;
  if (uBlend == BLEND_MULTIPLY) return multiplyB(cb, cs);
  if (uBlend == BLEND_HUE) return setLum(setSat(cs, sat(cb)), lum(cb));
  if (uBlend == BLEND_SATURATION) return setLum(setSat(cb, sat(cs)), lum(cb));
  if (uBlend == BLEND_COLOR) return setLum(cs, lum(cb));
  if (uBlend == BLEND_LUMINOSITY) return setLum(cb, lum(cs));
  // Never reached: the executor sets one of the above. The normal blend.
  return cs;
}
// The premultiplied result of `src` blended onto `dst` by `uBlend`.
vec4 blend(vec4 src, vec4 dst) {
  float sa = src.a;
  float da = dst.a;
  vec3 b = blendColor(unpremultiply(dst), unpremultiply(src));
  return vec4((1.0 - da) * src.rgb + (1.0 - sa) * dst.rgb + sa * da * b, sa + da - sa * da);
}
";

// The composite of a layer with an effect the shape program does not draw:
// a shader blend mode (walk.rs `Layer::blend`), a color matrix, a blurred
// image. The layer's texel, read within `uExtent` (its content's texels,
// half a texel in, so a bilinear read of a downsampled blur never reaches
// the storage past the content), through the color matrix when it has one
// (straight alpha: unpremultiplied, `uMatrix * c + uOffset`, clamped,
// premultiplied again), then faded by the alpha (`vColor`). Under
// `BLEND_SOURCE_OVER` the result is written times the clip coverage, with
// the fixed-function blend on (source-over, or a draw's own fixed-function
// mode), and the backdrop is never read. Under a shader mode it is drawn
// with blending off: the destination's color under it from the resolve
// scratch, and the mode's result written, mixed back to the destination by
// the clip box and the rounded clip of the state the layer opened in (the
// stencil test still gates it). Outside the op's coverage the layer is
// transparent and `blend` returns the destination exactly, so the quad
// changes no pixel the op did not touch. `vUv` (SHAPE_VERTEX's, from the
// composite's texel rows) addresses the layer; the backdrop is read at the
// fragment's own window position: the composite draws in the parent frame,
// and the executor resolved the region into the scratch at the parent's
// window coordinates (`resolve_region`). Its vertex stage is SHAPE_VERTEX;
// the linker drops what this stage does not read.
//
// A known approximation of the fixed-function modes, not of this program:
// a batch's analytic coverage (an oval's rim, a rounded edge, a stroke's
// inner edge) is multiplied into the source, not applied as lerp(dst,
// result, coverage) as Skia does, so rim pixels differ from Skia's under
// the modes `blend::keeps_destination` refuses: Clear, Source, SourceIn,
// DestinationIn, SourceOut, DestinationATop and Modulate. Coverage times
// the source equals a coverage lerp of the result exactly whenever the
// result is affine in the premultiplied source with a destination factor of
// 1 at a zero source, which holds for the other eight fixed-function modes
// and for every shader mode (the W3C composite is affine in the
// premultiplied source, and B depends on Cs alone). The quads' own edges
// are MSAA's and exact.
//
// After SOURCE_HEADER, the common fragment parts and COMPOSITE_BLEND.
const COMPOSITE_FRAGMENT: &str = r"
uniform sampler2D uTexture;   // the layer, premultiplied
uniform sampler2D uBackdrop;  // the destination under the layer's rect, premultiplied
uniform bool uHasMatrix;      // whether the color matrix applies
uniform mat4 uMatrix;         // the 4x4 block of the 4x5 color matrix
uniform vec4 uOffset;         // its fifth column
uniform vec4 uExtent;         // the content's texels, normalized, half a texel in
void main() {
  vec2 d = device();
  float inside = float(all(greaterThanEqual(d, vClip.xy)) && all(lessThan(d, vClip.zw)));
  float f = inside * roundedClip();
  vec4 texel = texture(uTexture, clamp(vUv, uExtent.xy, uExtent.zw));
  if (uHasMatrix) {
    vec4 c = clamp(uMatrix * vec4(unpremultiply(texel), texel.a) + uOffset, 0.0, 1.0);
    texel = vec4(c.rgb * c.a, c.a);
  }
  vec4 src = texel * vColor;
  if (uBlend == BLEND_SOURCE_OVER) {
    fragColor = src * f;
    return;
  }
  vec4 dst = texelFetch(uBackdrop, ivec2(gl_FragCoord.xy), 0);
  fragColor = mix(dst, blend(src, dst), f);
}
";

// The blur program's vertex stage, after SOURCE_HEADER: no attributes, the
// quad over `uRect` (the destination's pixels, window coordinates, y up)
// in a storage of `uAlloc` pixels, drawn with the viewport over the whole
// storage, and `vUv` the source's normalized texels from `uSourceRect` at
// its corners: a translation for a same-size pass, a halving for a
// downsample. Every rect keeps GL's orientation, so a layer's content
// stays in its storage's bottom-left corner the right way up through every
// pass.
const BLUR_VERTEX: &str = r"
uniform vec4 uRect;        // the destination quad: x0, y0, x1, y1, storage pixels
uniform vec2 uAlloc;       // the destination storage's size in pixels
uniform vec4 uSourceRect;  // the source texels at the quad's corners, normalized: u0, v0, u1, v1
out vec2 vUv;
void main() {
  int corner = gl_VertexID - (gl_VertexID / 6) * 6;
  // Two triangles: (0,0) (1,0) (0,1) and (0,1) (1,0) (1,1).
  vec2 c = vec2(
    (corner == 1 || corner == 4 || corner == 5) ? 1.0 : 0.0,
    (corner == 2 || corner == 3 || corner == 5) ? 1.0 : 0.0);
  gl_Position = vec4(mix(uRect.xy, uRect.zw, c) / uAlloc * 2.0 - 1.0, 0.0, 1.0);
  vUv = mix(uSourceRect.xy, uSourceRect.zw, c);
}
";

// The blur program's fragment stage, after SOURCE_HEADER and the
// `BLUR_TAPS_MAX` const (`blur_taps_define`, from blur.rs's): the sum of
// `uTaps` taps of `uSource` centered on `vUv`, `uStep` apart (one source
// texel along the pass's axis, normalized), weighted by `uWeights` (the
// normalized gaussian, blur.rs `gaussian_weights`), each tap's position
// clamped to `uExtent`, the content's texels half a texel in. For a `Clamp`
// blur the content's edge continues; a `Decal` one's content was inflated
// by the reach and is transparent past its drawing, so the clamp changes
// nothing inside the reach: one code path. One tap of weight 1 and no step
// copies, or under linear filtering halves a 2x2 block (a downsample).
const BLUR_FRAGMENT: &str = r"
uniform sampler2D uSource;            // premultiplied
uniform vec2 uStep;                   // one tap's step, normalized source texels
uniform vec4 uExtent;                 // the content's texels, normalized, half a texel in
uniform int uTaps;                    // the kernel's width, odd, at most BLUR_TAPS_MAX
uniform float uWeights[BLUR_TAPS_MAX];
in vec2 vUv;
out vec4 fragColor;
void main() {
  float radius = float(uTaps - 1) * 0.5;
  vec4 sum = vec4(0.0);
  for (int i = 0; i < BLUR_TAPS_MAX; i++) {
    if (i >= uTaps) break;
    vec2 at = clamp(vUv + (float(i) - radius) * uStep, uExtent.xy, uExtent.zw);
    sum += uWeights[i] * texture(uSource, at);
  }
  fragColor = sum;
}
";

// The GLSL const the blur fragment sizes its weights by, from blur.rs's.
fn blur_taps_define() -> String {
  format!("const int BLUR_TAPS_MAX = {BLUR_TAPS_MAX};\n")
}

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
/// at link: `uSize`, `uFrag`, `iResolution` when the source reads it, and every
/// active uniform by name (`uniforms`, for params and inputs; `infos`, the
/// same as the contract's `PaintShader::uniforms` returns them).
pub(super) struct PaintProgram {
  pub(super) program: glow::Program,
  size: glow::UniformLocation,
  frag: glow::UniformLocation,
  resolution: Option<glow::UniformLocation>,
  uniforms: UniformTable,
  pub(super) infos: Vec<UniformInfo>,
}

impl PaintProgram {
  /// `program` with no uniforms and a placeholder `uSize` and `uFrag`, for
  /// a test handle no GL ever draws.
  #[cfg(any(test, feature = "oplog"))]
  pub(super) fn without_gl(program: glow::Program) -> PaintProgram {
    PaintProgram {
      program,
      size: glow::NativeUniformLocation(0),
      frag: glow::NativeUniformLocation(0),
      resolution: None,
      uniforms: HashMap::new(),
      infos: Vec::new(),
    }
  }
}

/// The GL half: the thirteen programs, the two VAOs and the instance buffer, and
/// the draw. Lives
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
  /// The composite of a layer with a shader blend mode, a color matrix or a
  /// blur (COMPOSITE_FRAGMENT), and its uniforms.
  composite: Program,
  composite_uniforms: CompositeUniforms,
  /// The blur passes, the downsamples and the backdrop fill
  /// (BLUR_FRAGMENT).
  blur: BlurProgram,
  /// Every shadow batch: analytic shadows (SHADOW_FRAGMENT).
  shadow: Program,
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
  /// The layer targets, kept across draws (`super::layers`).
  layers: LayerPool,
  /// The resolve scratch a shader-mode composite reads the destination
  /// from, kept across draws.
  scratch: ResolveScratch,
}

/// Where a draw's batches currently render: the target, or a layer's
/// storage, with the layer's device origin (ox, oy) and size (Wl, Hl); the
/// target is the frame at the origin of the target's own size. Every frame
/// draws with the target's clip-space mapping (`uSize` is the target's, so
/// no vertex stage changes): device (x, y) is clip (2x / W - 1, 1 - 2y / H),
/// W and H the target's. The viewport (-ox, Hl + oy - H, W, H) maps that
/// clip point to window (x - ox, Hl + oy - y), so the layer's device rect
/// lands at window [0, Wl] x [0, Hl], the storage's bottom-left corner,
/// device y = oy at its top row; negative viewport origins and a viewport
/// past the framebuffer are legal GL, and what falls outside the
/// framebuffer is discarded. A fragment at window (fx, fy) is then device
/// (fx + ox, oy + Hl - fy), which `device()` reads through `uFrag = (ox, oy
/// + Hl)`: (0, H) on the target.
#[derive(Clone, Copy)]
pub(crate) struct Frame {
  pub(crate) fbo: Option<glow::Framebuffer>,
  pub(crate) origin: (i32, i32),
  pub(crate) size: (i32, i32),
  /// The target itself, which draws inside the walk's scissor; a layer
  /// draws unscissored.
  pub(crate) root: bool,
}

impl Frame {
  /// `uFrag` in this frame.
  fn frag(&self) -> [f32; 2] {
    [self.origin.0 as f32, (self.origin.1 + self.size.1) as f32]
  }
}

/// Clear the bound frame for a pass that starts: its color to transparent
/// (unless `color` is false, a backdrop layer filled already), its depth,
/// and its stencil to 0 with every bit writable, the color writes on. The
/// stencil clips count up from 0; a list that ended with stencil clips in
/// effect left them in the target's stencil, and this clear of the next
/// draw takes them. Clearing the depth and stencil of a target that has
/// none is a no-op; every target alloy draws into has DEPTH24_STENCIL8
/// (gl/rig.rs, the window's stencil size 8), which the stencil clips need.
/// A root frame's clear honours its scissor.
unsafe fn clear_frame(gl: &glow::Context, color: bool) {
  gl.color_mask(true, true, true, true);
  gl.clear_color(0.0, 0.0, 0.0, 0.0);
  gl.stencil_mask(STENCIL_ALL_BITS);
  gl.clear_stencil(0);
  let color = if color { glow::COLOR_BUFFER_BIT } else { 0 };
  gl.clear(color | glow::DEPTH_BUFFER_BIT | glow::STENCIL_BUFFER_BIT);
}

/// The read rect of a shader-mode layer's destination copy: the layer's
/// device `rect` in `parent`'s GL window coordinates, (x, y, width, height),
/// y up from the bottom. The parent draws device (x, y) at window (x - ox,
/// Hp + oy - y) (`Frame`, its origin (ox, oy) and height Hp), so the rect's
/// bottom edge, device y = rect.max.y, is window row Hp - (rect.max.y - oy).
pub(crate) fn blit_rect(parent: &Frame, rect: Box2D<i32>) -> (i32, i32, i32, i32) {
  let (w, h) = (rect.width(), rect.height());
  let x = rect.min.x - parent.origin.0;
  let y = parent.size.1 - (rect.min.y - parent.origin.1) - h;
  (x, y, w, h)
}

/// The resolve scratch: one single-sample RGBA8 texture with its own
/// framebuffer, at least the target's size (bucketed, `bucket`), grown when
/// the target grows, kept across draws. A region of any frame resolves into
/// it at the frame's own window coordinates (`resolve_region`), which lie
/// inside the target's size for the root and for a layer frame alike. Its
/// GL names die with the executor's context, as the programs do.
#[derive(Default)]
struct ResolveScratch {
  current: Option<Scratch>,
}

/// The scratch's names and size.
#[derive(Clone, Copy)]
struct Scratch {
  fbo: glow::Framebuffer,
  color: glow::Texture,
  size: (i32, i32),
}

impl ResolveScratch {
  // The scratch covering `target`, made or grown now; None, warned once,
  // when GL cannot make it. Leaves the framebuffer binding dirty and
  // nothing bound on TEXTURE_2D of the active unit.
  unsafe fn ensure(&mut self, gl: &glow::Context, target: (i32, i32)) -> Option<Scratch> {
    if let Some(s) = self.current.filter(|s| s.size.0 >= target.0 && s.size.1 >= target.1) {
      return Some(s);
    }
    let size = match self.current.take() {
      Some(old) => {
        gl.delete_framebuffer(old.fbo);
        gl.delete_texture(old.color);
        bucket((old.size.0.max(target.0), old.size.1.max(target.1)))
      }
      None => bucket(target),
    };
    let made = create_scratch(gl, size);
    if made.is_none() {
      warn_once!(
        "[anvil] Renderer: the resolve scratch could not be created; shader blend modes do not draw and backdrop layers start empty"
      );
    }
    self.current = made;
    made
  }
}

// A scratch of `size`: the RGBA8 texture (nearest, clamped) attached to a
// new framebuffer; None when GL makes no name or the framebuffer is
// incomplete.
unsafe fn create_scratch(gl: &glow::Context, size: (i32, i32)) -> Option<Scratch> {
  let color = gl.create_texture().ok()?;
  let Ok(fbo) = gl.create_framebuffer() else {
    gl.delete_texture(color);
    return None;
  };
  gl.bind_texture(glow::TEXTURE_2D, Some(color));
  gl.tex_image_2d(
    glow::TEXTURE_2D,
    0,
    glow::RGBA8 as i32,
    size.0,
    size.1,
    0,
    glow::RGBA,
    glow::UNSIGNED_BYTE,
    glow::PixelUnpackData::Slice(None),
  );
  gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
  gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
  gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
  gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
  gl.bind_texture(glow::TEXTURE_2D, None);
  gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
  gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(color), 0);
  if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
    gl.delete_framebuffer(fbo);
    gl.delete_texture(color);
    return None;
  }
  Some(Scratch { fbo, color, size })
}

// The destination under the device `rect`, resolved from `parent`'s
// framebuffer into `scratch` with identical bounds, `rect` in the parent's
// window coordinates (`blit_rect`), COLOR, NEAREST: identical bounds are
// what GLES 3.0 requires of a blit that reads a multisampled framebuffer,
// and an EXT_multisampled_render_to_texture parent resolves on the read
// too. The blit honours the scissor, set only on the root (`scissored`),
// so it is off for the blit. The parent is bound again after, its scissor
// as it was.
unsafe fn resolve_region(gl: &glow::Context, parent: &Frame, rect: Box2D<i32>, scratch: &Scratch, scissored: bool) {
  let (x, y, w, h) = blit_rect(parent, rect);
  let scissored = scissored && parent.root;
  if scissored {
    gl.disable(glow::SCISSOR_TEST);
  }
  gl.bind_framebuffer(glow::READ_FRAMEBUFFER, parent.fbo);
  gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(scratch.fbo));
  gl.blit_framebuffer(x, y, x + w, y + h, x, y, x + w, y + h, glow::COLOR_BUFFER_BIT, glow::NEAREST);
  gl.bind_framebuffer(glow::FRAMEBUFFER, parent.fbo);
  if scissored {
    gl.enable(glow::SCISSOR_TEST);
  }
}

/// The backdrop fill's rects, x0, y0, x1, y1 in window pixels, y up: the
/// source, the layer's device `rect` in the resolve scratch, where
/// `resolve_region` put it at `parent`'s window coordinates (`blit_rect`),
/// and the destination, the layer storage's bottom-left corner of the
/// rect's size (`Frame`). Both frames turn device y down into window y up
/// alike, so the copy is a translation.
pub(crate) fn fill_rects(parent: &Frame, rect: Box2D<i32>) -> ([i32; 4], [i32; 4]) {
  let (x, y, w, h) = blit_rect(parent, rect);
  ([x, y, x + w, y + h], [0, 0, w, h])
}

/// A `Clamp` blur's content rect `clamp` (device pixels, within the layer's
/// device `rect`) in the layer storage's window pixels, x0, y0, x1, y1, y
/// up: the layer frame draws device (x, y) at window (x - ox, Hl + oy - y)
/// (`Frame`, its origin (ox, oy) and height Hl).
pub(crate) fn clamp_rect(rect: Box2D<i32>, clamp: Box2D<i32>) -> [i32; 4] {
  let (ox, oy, h) = (rect.min.x, rect.min.y, rect.height());
  [clamp.min.x - ox, h + oy - clamp.max.y, clamp.max.x - ox, h + oy - clamp.min.y]
}

/// The texels of `rect` (x0, y0, x1, y1) in a storage of `size` pixels,
/// normalized and inset half a texel: the positions a bilinear read keeps
/// inside the rect.
fn texel_extent(rect: [i32; 4], size: (i32, i32)) -> [f32; 4] {
  let (w, h) = (size.0 as f32, size.1 as f32);
  let [x0, y0, x1, y1] = rect.map(|v| v as f32);
  [(x0 + 0.5) / w, (y0 + 0.5) / h, (x1 - 0.5) / w, (y1 - 0.5) / h]
}

/// `m`, a 4x5 row-major matrix over (R, G, B, A, 1) in straight alpha, as
/// the composite program takes it: `uMatrix`, its 4x4 block in the
/// column-major order glow's `uniform_matrix_4_f32_slice` takes
/// untransposed (column j holds input channel j's coefficients of the four
/// outputs, so GLSL's `uMatrix * c` is output i = row i of the block dot c),
/// and `uOffset`, its fifth column.
pub(crate) fn matrix_uniforms(m: &ColorMatrix) -> ([f32; 16], [f32; 4]) {
  let m = &m.m;
  // Five entries a row: four channel coefficients and the offset.
  let row = |i: usize| &m[i * 5..i * 5 + 5];
  let mut columns = [0.0; 16];
  for j in 0..4 {
    for i in 0..4 {
      columns[j * 4 + i] = row(i)[j];
    }
  }
  (columns, [row(0)[4], row(1)[4], row(2)[4], row(3)[4]])
}

// Draw one blur `pass` (`BlurPass`) through `program`, its source sampled on
// TEXTURE_UNIT by the nearest sampler of `samplers` (nearest, linear) when
// the target rect is the source rect's size (a copy, a gaussian pass: every
// tap at a texel center) and by the linear one for a halving (the 2x2
// average): the destination bound with the viewport
// over its whole storage, blending, the stencil test and the scissor off.
// Blending is on again after; the framebuffer, the viewport and the
// program are left for the caller's next frame (`set_frame`), the source
// bound on TEXTURE_UNIT until the caller's next bind.
unsafe fn blur_pass(
  gl: &glow::Context,
  program: &BlurProgram,
  samplers: (glow::Sampler, glow::Sampler),
  pass: &BlurPass,
) {
  gl.bind_framebuffer(glow::FRAMEBUFFER, Some(pass.target));
  gl.viewport(0, 0, pass.target_size.0, pass.target_size.1);
  gl.disable(glow::BLEND);
  gl.disable(glow::STENCIL_TEST);
  gl.disable(glow::SCISSOR_TEST);
  gl.use_program(Some(program.program));
  let [x0, y0, x1, y1] = pass.target_rect.map(|v| v as f32);
  gl.uniform_4_f32(Some(&program.rect), x0, y0, x1, y1);
  gl.uniform_2_f32(Some(&program.alloc), pass.target_size.0 as f32, pass.target_size.1 as f32);
  let (sw, sh) = (pass.source_size.0 as f32, pass.source_size.1 as f32);
  let [u0, v0, u1, v1] = pass.source_rect.map(|v| v as f32);
  gl.uniform_4_f32(Some(&program.source_rect), u0 / sw, v0 / sh, u1 / sw, v1 / sh);
  let [e0, e1, e2, e3] = texel_extent(pass.clamp.unwrap_or(pass.source_rect), pass.source_size);
  gl.uniform_4_f32(Some(&program.extent), e0, e1, e2, e3);
  gl.uniform_2_f32(Some(&program.step), pass.direction[0] / sw, pass.direction[1] / sh);
  gl.uniform_1_i32(Some(&program.taps), pass.weights.len() as i32);
  gl.uniform_1_f32_slice(Some(&program.weights), pass.weights);
  let size = |r: [i32; 4]| (r[2] - r[0], r[3] - r[1]);
  let (nearest, linear) = samplers;
  let sampler = if size(pass.target_rect) == size(pass.source_rect) { nearest } else { linear };
  gl.bind_texture(glow::TEXTURE_2D, Some(pass.source));
  gl.bind_sampler(TEXTURE_UNIT, Some(sampler));
  gl.draw_arrays(glow::TRIANGLES, 0, VERTICES_PER_QUAD);
  gl.enable(glow::BLEND);
}

// Blur the layer `layer`, its `content` pixels complete in its texture,
// by `blur` (blur.rs, in the order `blur_passes` counts): halve the content
// `blur_levels` times by a one-tap linear pass each, then a horizontal and
// a vertical gaussian pass at the halved sigmas (`level_sigmas`), each where
// its sigma reaches BLUR_SIGMA_MIN. The first pass clamps its taps to
// `clamp` when given (a `Clamp` blur's content rect in the layer's storage,
// `clamp_rect`), so the content's edge color fills the reach's margin; the
// later ones read that result over the whole content. Every pass draws into a single-sample
// storage of the pool and releases its source as soon as it has drawn, the
// layer's own storage first. The last storage and its content size; None,
// every storage released, when GL could make none for a pass (the pool
// warned).
unsafe fn run_blur(
  gl: &glow::Context,
  pool: &mut LayerPool,
  program: &BlurProgram,
  samplers: (glow::Sampler, glow::Sampler),
  layer: (LayerTarget, (i32, i32)),
  mut clamp: Option<[i32; 4]>,
  blur: DeviceBlur,
) -> Option<(LayerTarget, (i32, i32))> {
  let levels = blur_levels(blur.sigma_x, blur.sigma_y);
  let (sigma_x, sigma_y) = level_sigmas(blur.sigma_x, blur.sigma_y, levels);
  let mut current = layer;
  // One pass from `current` into a new storage of `size`.
  let mut step = |current: (LayerTarget, (i32, i32)), size: (i32, i32), direction: [f32; 2], weights: &[f32]| {
    let (source, from) = current;
    let Some(next) = pool.acquire(gl, bucket(size), 1) else {
      pool.release(&source);
      return None;
    };
    let pass = BlurPass {
      source: source.color,
      source_size: source.size,
      source_rect: [0, 0, from.0, from.1],
      target: next.fbo,
      target_size: next.size,
      target_rect: [0, 0, size.0, size.1],
      clamp: clamp.take(),
      direction,
      weights,
    };
    blur_pass(gl, program, samplers, &pass);
    pool.release(&source);
    Some((next, size))
  };
  for _ in 0..levels {
    let size = halved(current.1, 1);
    current = step(current, size, [0.0, 0.0], &[1.0])?;
  }
  for (sigma, direction) in [(sigma_x, [1.0, 0.0]), (sigma_y, [0.0, 1.0])] {
    if sigma >= BLUR_SIGMA_MIN {
      let size = current.1;
      current = step(current, size, direction, &gaussian_weights(sigma))?;
    }
  }
  Some(current)
}

/// Blend by `mode`, a fixed-function one (`super::blend::gl_factors`),
/// unless `current` already is it; `current` is then `mode`.
unsafe fn set_blend(gl: &glow::Context, current: &mut BlendMode, mode: BlendMode) {
  if *current == mode {
    return;
  }
  let (src, dst) = gl_factors(mode).expect("a batch blends by a fixed-function mode (walk.rs Batch)");
  gl.blend_func(src, dst);
  *current = mode;
}

/// A linked program, its `uSize` (the target in device pixels), its
/// `uFrag` (the frame's window-to-device offset, `Frame`), on the
/// programs that sample a gradient its `uRampRows` (read by the vertex
/// stage, which computes the ramp row's v), and on the programs
/// that link GRADIENT_BATCH (the fill, rounded, shape and mesh gradient
/// variants) its `uGradMeta`, the batch's gradient kind and tile.
struct Program {
  program: glow::Program,
  size: glow::UniformLocation,
  frag: glow::UniformLocation,
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

/// The uniforms a layer's composite sets on the composite program: the
/// blend mode, the color matrix (`uHasMatrix`, `uMatrix`, `uOffset`) and
/// the content's extent.
struct CompositeUniforms {
  blend: glow::UniformLocation,
  has_matrix: glow::UniformLocation,
  matrix: glow::UniformLocation,
  offset: glow::UniformLocation,
  extent: glow::UniformLocation,
}

/// The blur program (BLUR_VERTEX, BLUR_FRAGMENT) and the uniforms a pass
/// sets. It reads no `uSize` or `uFrag`: a pass places its quad in its
/// destination storage's own pixels.
struct BlurProgram {
  program: glow::Program,
  rect: glow::UniformLocation,
  alloc: glow::UniformLocation,
  source_rect: glow::UniformLocation,
  step: glow::UniformLocation,
  extent: glow::UniformLocation,
  taps: glow::UniformLocation,
  weights: glow::UniformLocation,
}

impl BlurProgram {
  // Compiled, linked, with its uniforms; `uSource` names TEXTURE_UNIT.
  unsafe fn link(gl: &glow::Context) -> Result<BlurProgram, String> {
    let label = "blur";
    let fragment = [SOURCE_HEADER, &blur_taps_define(), BLUR_FRAGMENT].concat();
    let p = link_program(gl, label, &[SOURCE_HEADER, BLUR_VERTEX].concat(), &fragment)?;
    let source = uniform(gl, p, label, "uSource")?;
    gl.use_program(Some(p));
    gl.uniform_1_i32(Some(&source), TEXTURE_UNIT as i32);
    Ok(BlurProgram {
      program: p,
      rect: uniform(gl, p, label, "uRect")?,
      alloc: uniform(gl, p, label, "uAlloc")?,
      source_rect: uniform(gl, p, label, "uSourceRect")?,
      step: uniform(gl, p, label, "uStep")?,
      extent: uniform(gl, p, label, "uExtent")?,
      taps: uniform(gl, p, label, "uTaps")?,
      weights: uniform(gl, p, label, "uWeights")?,
    })
  }
}

/// One draw of the blur program: `weights` taps (blur.rs
/// `gaussian_weights`; one of weight 1 for a copy or a downsample) of the
/// texture `source`, a storage of `source_size` pixels, over its pixels
/// `source_rect`, along `direction` (in source texels; zero for one tap),
/// into the pixels `target_rect` of the framebuffer `target`, a storage of
/// `target_size` pixels, every tap clamped to `clamp` (a `Clamp` blur's
/// content rect, `Layer::clamp`, on its first pass) or else to
/// `source_rect`. Rects are x0, y0, x1, y1 in window pixels, y up.
struct BlurPass<'a> {
  source: glow::Texture,
  source_size: (i32, i32),
  source_rect: [i32; 4],
  target: glow::Framebuffer,
  target_size: (i32, i32),
  target_rect: [i32; 4],
  clamp: Option<[i32; 4]>,
  direction: [f32; 2],
  weights: &'a [f32],
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
  /// Over the context current on the calling thread; compiles the thirteen
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
      &[SOURCE_HEADER, VERTEX_COMMON, ROUNDED_PLACE, ROUNDED_VERTEX],
      &[SOURCE_HEADER, FRAGMENT_INPUTS, FRAGMENT_MATH, FRAGMENT_ROUNDED_CLIP, ROUNDED_FRAGMENT],
    )?;
    let rounded_gradient = Program::link(
      &gl,
      "rounded gradient",
      &[SOURCE_HEADER, VERTEX_COMMON, ROUNDED_PLACE, ROUNDED_VERTEX],
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
    let composite = Program::link(
      &gl,
      "composite",
      &[SOURCE_HEADER, VERTEX_COMMON, SHAPE_VERTEX],
      &[SOURCE_HEADER, FRAGMENT_INPUTS, FRAGMENT_MATH, FRAGMENT_ROUNDED_CLIP, COMPOSITE_BLEND, COMPOSITE_FRAGMENT],
    )?;
    let (cp, label) = (composite.program, "composite");
    let composite_uniforms = CompositeUniforms {
      blend: uniform(&gl, cp, label, "uBlend")?,
      has_matrix: uniform(&gl, cp, label, "uHasMatrix")?,
      matrix: uniform(&gl, cp, label, "uMatrix")?,
      offset: uniform(&gl, cp, label, "uOffset")?,
      extent: uniform(&gl, cp, label, "uExtent")?,
    };
    let blur = BlurProgram::link(&gl)?;
    let shadow = Program::link(
      &gl,
      "shadow",
      &[SOURCE_HEADER, VERTEX_COMMON, ROUNDED_PLACE, SHADOW_VERTEX],
      &[SOURCE_HEADER, FRAGMENT_INPUTS, FRAGMENT_MATH, FRAGMENT_ROUNDED_CLIP, SHADOW_FRAGMENT],
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
    // The composite program samples the layer on the one unit and the
    // destination copy on its own.
    for (program, label) in [
      (shape.program, "shape"),
      (glyph.program, "glyph"),
      (mesh_textured.program.program, "mesh textured"),
      (composite.program, "composite"),
    ] {
      let texture_uniform = uniform(&gl, program, label, "uTexture")?;
      gl.use_program(Some(program));
      gl.uniform_1_i32(Some(&texture_uniform), TEXTURE_UNIT as i32);
    }
    let backdrop_uniform = uniform(&gl, composite.program, "composite", "uBackdrop")?;
    gl.use_program(Some(composite.program));
    gl.uniform_1_i32(Some(&backdrop_uniform), BACKDROP_UNIT as i32);
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
    let layers = LayerPool::new(&gl, proc_address);
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
      composite,
      composite_uniforms,
      blur,
      shadow,
      vao,
      instances,
      mesh_vao,
      samplers,
      glyph_uniforms,
      ramp,
      ramps: RampTable::new(),
      layers,
      scratch: ResolveScratch::default(),
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
      let (size, frag) = match (uniform(gl, program, "paint", "uSize"), uniform(gl, program, "paint", "uFrag")) {
        (Ok(size), Ok(frag)) => (size, frag),
        (Err(e), _) | (_, Err(e)) => {
          gl.delete_program(program);
          return Err(e);
        }
      };
      // Optional: the compiler drops it when the source never reads it.
      let resolution = gl.get_uniform_location(program, "iResolution");
      let (uniforms, infos) = reflect_uniforms(gl, program);
      Ok(PaintProgram { program, size, frag, resolution, uniforms, infos })
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
  /// the path meshes drawn and tessellated, the stencil clip draws, the
  /// layers and their blur passes (`super::report`), zeros for an empty list; no draw call and no
  /// clip draw for a list that emits only clips (every draw culled), whose
  /// batches are never issued. A layer's content draws into a target of the
  /// layer pool (`super::layers`) between its two marker batches, its
  /// composite into the frame it opened on (a blurred layer's after its
  /// passes, `run_blur`); the target is bound again on return.
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
    unsafe { self.layers.begin(&self.gl) };
    let instances = &walked.instances;
    let (width, height) = (target.size.width as i32, target.size.height as i32);
    // The scissor is the union of the instances' clips (walk.rs), so a
    // patch frame pays GPU, and on a tiler tile loads and stores, only
    // inside the patch. The clear is scissored too: every caller in
    // gl/draw.rs cleared the full target before the draw, so the pixels
    // outside are already clear. An empty list (no scissor) clears the
    // full target, as the contract says a draw does.
    let full = Box2D::new(Point2D::new(0, 0), Point2D::new(width, height));
    let scissor = walked.scissor.filter(|s| *s != full);
    let root = Frame { fbo: target.framebuffer, origin: (0, 0), size: (width, height), root: true };
    let gl = &self.gl;
    unsafe {
      // The target bound, its viewport, every program's `uFrag` and the
      // scissor: bound, not cleared, until the layers drawn ahead are drawn
      // (`Step::Target`), so its pass starts once; bound now for the sample
      // count a layer's storage takes.
      self.set_frame(&root, (width, height), scissor);
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
      // leaves its clip batches undrawn. A drawn layer always has content
      // among the instances or the mesh draws, except a backdrop layer,
      // which draws with none.
      let drawn = |l: &walk::Layer| !l.folded && !l.rect.is_empty();
      if instances.is_empty() && walked.mesh_draws.is_empty() && !walked.layers.iter().any(drawn) {
        clear_frame(gl, true);
        gl.use_program(None);
        return Ok(DrawReport { draw_calls: 0, clips: 0, ..super::report(&walked) });
      }
      for program in self.programs() {
        gl.use_program(Some(program.program));
        gl.uniform_2_f32(Some(&program.size), width as f32, height as f32);
        if let Some(rows) = &program.ramp_rows {
          gl.uniform_1_f32(Some(rows), RAMP_ROWS as f32);
        }
      }
      // The shape program is in use, as the loop left it. None after a
      // paint batch: its program is the shader's, so the next batch binds.
      let mut in_use = Some(self.shape.program);
      // The fixed-function blend mode set, source-over as set above: a quad
      // or a mesh batch sets its own where it changes; the composites, the
      // paint and glyph batches draw source-over (a clip batch writes no
      // color). Source-over again on return.
      let mut blending = BlendMode::SourceOver;
      gl.bind_vertex_array(Some(self.vao));
      gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.instances));
      // The stencil clips' quads ride in the one instance buffer after the
      // walk's instances, and the drawn layers' composites after them:
      // `clip_slots` holds each clip draw's quad index, None for a mesh, and
      // `layer_slots` each layer's composite index, None for a folded or
      // empty one. A composite's texel rows are rescaled here to the storage
      // it samples, a bucket's (`super::layers`): the layer's own, or for a
      // blurred layer the last pass's, at the content halved by the blur's
      // levels (`run_blur`).
      let mut tail = Vec::new();
      let clip_slots: Vec<Option<usize>> = walked
        .clip_draws
        .iter()
        .map(|clip| {
          let (_, Cover::Quad(quad)) = clip_test(clip) else { return None };
          tail.push(*quad);
          Some(instances.len() + tail.len() - 1)
        })
        .collect();
      let layer_slots: Vec<Option<usize>> = walked
        .layers
        .iter()
        .map(|layer| {
          if layer.folded || layer.rect.is_empty() {
            return None;
          }
          let content = (layer.rect.width(), layer.rect.height());
          let shown = layer.blur.map_or(content, |b| halved(content, blur_levels(b.sigma_x, b.sigma_y)));
          tail.push(rescale_composite(&layer.composite, shown, bucket(shown)));
          Some(instances.len() + tail.len() - 1)
        })
        .collect();
      if tail.is_empty() {
        if !instances.is_empty() {
          gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, instance_bytes(instances), glow::STREAM_DRAW);
        }
      } else {
        let stride = std::mem::size_of::<Instance>();
        let total = (instances.len() + tail.len()) * stride;
        gl.buffer_data_size(glow::ARRAY_BUFFER, total as i32, glow::STREAM_DRAW);
        if !instances.is_empty() {
          gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, 0, instance_bytes(instances));
        }
        gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, (instances.len() * stride) as i32, instance_bytes(&tail));
      }
      // The target's sample count, which a layer's storage takes: queried
      // with the target bound, once, and only when a layer draws.
      let samples = if layer_slots.iter().any(Option::is_some) { gl.get_parameter_i32(glow::SAMPLES) } else { 0 };
      // The frames drawn into, the target at the bottom, and the layers
      // open, innermost last: each one's storage (a skipped layer opens
      // none).
      let mut frames = vec![root];
      let mut open: Vec<LayerTarget> = Vec::new();
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
      let batches = &walked.batches;
      // The steps in their order (`schedule`): the layers drawn ahead, each
      // into its storage, then the target bound and cleared, then its pass.
      let steps = schedule(batches, &walked.layers, DRAW_AHEAD);
      // Each closed layer's image until its composite: the storage it
      // samples (the layer's own, or its blur's last pass's) and the
      // content's size in it. Indexed by layer.
      let mut images: Vec<Option<(LayerTarget, (i32, i32))>> = vec![None; walked.layers.len()];
      // The layer whose storage could not be taken (the pool warned): its
      // steps up to its close are skipped, and the images of the layers
      // drawn ahead inside it are released unused.
      let mut skipping: Option<usize> = None;
      // Whether a layer was drawn ahead, so the target's pass starts after a
      // flush (see `Step::Target`).
      let mut drew_ahead = false;
      // Whether the bound framebuffer is not the frame on top of `frames`:
      // a close (its finish and blur passes) and a destination copy's
      // scratch leave it dirty, and a composite binds its frame again.
      let mut dirty = false;
      for &step in &steps {
        if let Some(skipped) = skipping {
          match step {
            Step::Close(end) if batches[end].start == skipped => skipping = None,
            Step::Composite(end) => {
              if let Some((image, _)) = images[batches[end].start].take() {
                self.layers.release(&image);
              }
            }
            _ => {}
          }
          continue;
        }
        let index = match step {
          Step::Batch(index) => index,
          Step::Target => {
            // A driver workaround, not a GL rule: on the Adreno 610 a layer
            // sampled with its passes still queued composited black, and a
            // frame that left the multisampled window's framebuffer for a
            // layer and came back presented a stale buffer two frames in
            // three. The layers drawn ahead are flushed once, before the
            // window's pass starts; an inline layer flushes at its close.
            if drew_ahead {
              gl.flush();
            }
            in_use = Some(self.set_frame(&root, (width, height), scissor));
            clear_frame(gl, true);
            dirty = false;
            continue;
          }
          Step::Open(begin) => {
            let layer = &walked.layers[batches[begin].start];
            let content = (layer.rect.width(), layer.rect.height());
            // GL could make no storage for it (the pool warned): skip to its
            // close.
            let Some(storage) = self.layers.acquire(gl, bucket(content), samples) else {
              skipping = Some(batches[begin].start);
              continue;
            };
            open.push(storage);
            debug_assert_eq!(storage.size, bucket(content));
            let parent = *frames.last().expect("the target's frame is at the bottom of the stack");
            let frame = Frame {
              fbo: Some(storage.fbo),
              origin: (layer.rect.min.x, layer.rect.min.y),
              size: content,
              root: false,
            };
            frames.push(frame);
            // A backdrop layer starts from the parent's pixels under its rect:
            // resolved into the scratch at the parent's window coordinates,
            // then drawn into the storage's bottom-left corner by the blur
            // program as a one-tap copy (a blit cannot write a multisampled
            // storage under GLES 3.0). Without a scratch (warned once) it
            // starts transparent. It is never drawn ahead (`schedule`), so
            // its parent is the frame drawing now.
            let mut filled = false;
            if layer.backdrop {
              if mesh_bound {
                gl.bind_vertex_array(Some(self.vao));
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.instances));
                mesh_bound = false;
              }
              let samplers = self.pass_samplers();
              if let Some(scratch) = self.scratch.ensure(gl, (width, height)) {
                resolve_region(gl, &parent, layer.rect, &scratch, scissor.is_some());
                let (source_rect, target_rect) = fill_rects(&parent, layer.rect);
                let pass = BlurPass {
                  source: scratch.color,
                  source_size: scratch.size,
                  source_rect,
                  target: storage.fbo,
                  target_size: storage.size,
                  target_rect,
                  clamp: None,
                  direction: [0.0, 0.0],
                  weights: &[1.0],
                };
                blur_pass(gl, &self.blur, samplers, &pass);
                gl.bind_sampler(TEXTURE_UNIT, None);
                gl.bind_texture(glow::TEXTURE_2D, None);
                filled = true;
              }
            }
            // Unscissored (`set_frame`); the layer's stencil clips count up
            // from 0 again. A filled backdrop keeps its color.
            in_use = Some(self.set_frame(&frame, (width, height), scissor));
            clear_frame(gl, !filled);
            dirty = false;
            continue;
          }
          Step::Close(end) => {
            let storage = open.pop().expect("a layer's close follows its open");
            let layer = &walked.layers[batches[end].start];
            let content = (layer.rect.width(), layer.rect.height());
            frames.pop();
            self.layers.finish(gl, &storage, content);
            let ahead = DRAW_AHEAD && draws_ahead(layer);
            if ahead {
              drew_ahead = true;
            } else {
              // The Adreno workaround of `Step::Target`, for a layer whose
              // parent's pass resumes right after it.
              gl.flush();
            }
            // The blur passes draw from the instance VAO.
            if mesh_bound {
              gl.bind_vertex_array(Some(self.vao));
              gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.instances));
              mesh_bound = false;
            }
            // The image the composite samples: the layer's storage, or the
            // last blur pass's (every other storage released already). None
            // when GL could make no storage for a pass (the pool warned): the
            // layer does not draw.
            images[batches[end].start] = match layer.blur {
              Some(blur) => {
                let samplers = self.pass_samplers();
                let clamp = layer.clamp.map(|c| clamp_rect(layer.rect, c));
                run_blur(gl, &mut self.layers, &self.blur, samplers, (storage, content), clamp, blur)
              }
              None => Some((storage, content)),
            };
            dirty = true;
            continue;
          }
          Step::Composite(end) => {
            let batch = &batches[end];
            let layer = &walked.layers[batch.start];
            let image = images[batch.start].take();
            // The composite draws from the instance VAO.
            if mesh_bound {
              gl.bind_vertex_array(Some(self.vao));
              gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.instances));
              mesh_bound = false;
            }
            let linear = self.sampler(TextureSampling::Linear, TextureWrap::Clamp);
            let nearest = self.sampler(TextureSampling::Nearest, TextureWrap::Clamp);
            let mode = shader_mode(layer.blend);
            // A shader mode's destination: the resolve scratch, made or grown
            // before the frame is bound again (it leaves the framebuffer
            // binding dirty).
            let backdrop =
              if mode.is_some() && image.is_some() { self.scratch.ensure(gl, (width, height)) } else { None };
            dirty |= mode.is_some();
            let parent = *frames.last().expect("the target's frame is at the bottom of the stack");
            if dirty {
              in_use = Some(self.set_frame(&parent, (width, height), scissor));
              dirty = false;
            }
            let Some((image, shown)) = image else { continue };
            // GL could make no scratch (warned once): the layer does not draw.
            if mode.is_some() && backdrop.is_none() {
              self.layers.release(&image);
              continue;
            }
            // The composite's rows were rescaled to this size before the upload.
            debug_assert_eq!(image.size, bucket(shown));
            if let Some(scratch) = &backdrop {
              resolve_region(gl, &parent, layer.rect, scratch, scissor.is_some());
            }
            if batch.stencil == 0 {
              gl.disable(glow::STENCIL_TEST);
            } else {
              gl.enable(glow::STENCIL_TEST);
              gl.stencil_func(glow::EQUAL, batch.stencil as i32, STENCIL_ALL_BITS);
              gl.stencil_op(glow::KEEP, glow::KEEP, glow::KEEP);
            }
            point_attributes(gl, layer_slots[batch.start].expect("a drawn layer has its composite's slot"));
            // A shader mode draws with blending off; every other composite
            // blends by the layer's fixed-function mode (source-over but for
            // the implicit layer around a draw with a color matrix).
            set_blend(gl, &mut blending, if mode.is_some() { BlendMode::SourceOver } else { layer.blend });
            // The texel rows sample the storage 1:1, where nearest is exact,
            // unless the blur halved the content: then bilinear, the upsample.
            let downsampled = layer.blur.is_some_and(|b| blur_levels(b.sigma_x, b.sigma_y) > 0);
            gl.bind_texture(glow::TEXTURE_2D, Some(image.color));
            gl.bind_sampler(TEXTURE_UNIT, Some(if downsampled { linear } else { nearest }));
            let composite = mode.is_some() || layer.matrix.is_some() || layer.blur.is_some();
            if composite {
              let u = &self.composite_uniforms;
              gl.use_program(Some(self.composite.program));
              in_use = Some(self.composite.program);
              gl.uniform_1_i32(Some(&u.blend), mode.unwrap_or(BLEND_SOURCE_OVER));
              match &layer.matrix {
                Some(m) => {
                  let (columns, offset) = matrix_uniforms(m);
                  gl.uniform_1_i32(Some(&u.has_matrix), 1);
                  gl.uniform_matrix_4_f32_slice(Some(&u.matrix), false, &columns);
                  gl.uniform_4_f32_slice(Some(&u.offset), &offset);
                }
                None => gl.uniform_1_i32(Some(&u.has_matrix), 0),
              }
              let [x0, y0, x1, y1] = texel_extent([0, 0, shown.0, shown.1], image.size);
              gl.uniform_4_f32(Some(&u.extent), x0, y0, x1, y1);
            } else if in_use != Some(self.shape.program) {
              // A plain composite is a textured quad of the shape program,
              // whose `uFrag` the frame's `set_frame` set.
              gl.use_program(Some(self.shape.program));
              in_use = Some(self.shape.program);
            }
            match (composite, &backdrop) {
              (true, Some(scratch)) => {
                gl.active_texture(glow::TEXTURE0 + BACKDROP_UNIT);
                gl.bind_texture(glow::TEXTURE_2D, Some(scratch.color));
                // texelFetch ignores the filter; bound for a defined state.
                gl.bind_sampler(BACKDROP_UNIT, Some(nearest));
                // The fragment writes the mode's result: no fixed-function
                // blend on top of it.
                gl.disable(glow::BLEND);
                gl.draw_arrays_instanced(glow::TRIANGLES, 0, VERTICES_PER_QUAD, 1);
                gl.enable(glow::BLEND);
                gl.bind_sampler(BACKDROP_UNIT, None);
                gl.bind_texture(glow::TEXTURE_2D, None);
                gl.active_texture(glow::TEXTURE0 + TEXTURE_UNIT);
              }
              // The composite program blending by the layer's mode, or the
              // shape program.
              _ => gl.draw_arrays_instanced(glow::TRIANGLES, 0, VERTICES_PER_QUAD, 1),
            }
            // Unbound again: the next layer that takes the storage draws into
            // it.
            gl.bind_sampler(TEXTURE_UNIT, None);
            gl.bind_texture(glow::TEXTURE_2D, None);
            self.layers.release(&image);
            continue;
          }
        };
        let batch = &batches[index];
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
        set_blend(gl, &mut blending, batch.blend);
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
          let frame = frames.last().expect("the target's frame is at the bottom of the stack");
          self.draw_paint(paint, [width as f32, height as f32], frame.frag(), batch.count);
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
          (BatchProgram::Shadow, _) => &self.shadow,
          (BatchProgram::Paint, _) => unreachable!("a paint batch carries its draw (walk.rs Batch)"),
          (BatchProgram::Mesh, _) => unreachable!("a mesh batch draws above"),
          (BatchProgram::Clip, _) => unreachable!("a clip batch draws above"),
          (BatchProgram::LayerBegin | BatchProgram::LayerEnd, _) => unreachable!("a layer marker draws above"),
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
      debug_assert!(frames.len() == 1 && open.is_empty(), "every layer closed: the target is bound again");
      debug_assert!(images.iter().all(Option::is_none), "every closed layer composited or released");
      set_blend(gl, &mut blending, BlendMode::SourceOver);
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

  // Render into `frame` from here on: its framebuffer bound, the viewport
  // that puts its device rect at its storage's bottom-left (`Frame`), every
  // program's `uFrag`, and the scissor: the walk's `scissor` on the target
  // (None: off), off in a layer. `target` is the target's size. Returns the
  // program it leaves in use, the shape program.
  unsafe fn set_frame(&self, frame: &Frame, target: (i32, i32), scissor: Option<Box2D<i32>>) -> glow::Program {
    let gl = &self.gl;
    let (w, h) = target;
    let (ox, oy) = frame.origin;
    gl.bind_framebuffer(glow::FRAMEBUFFER, frame.fbo);
    gl.viewport(-ox, frame.size.1 + oy - h, w, h);
    let [fx, fy] = frame.frag();
    for program in self.programs() {
      gl.use_program(Some(program.program));
      gl.uniform_2_f32(Some(&program.frag), fx, fy);
    }
    match scissor.filter(|_| frame.root) {
      Some(s) => {
        gl.enable(glow::SCISSOR_TEST);
        // GL scissor is in window coordinates, y up from the bottom.
        gl.scissor(s.min.x, h - s.max.y, s.width(), s.height());
      }
      None => gl.disable(glow::SCISSOR_TEST),
    }
    self.shape.program
  }

  // The twelve programs that read `uFrag` (the blur program places its quad
  // from its own uniforms), the shape program last, so a loop over them
  // leaves it in use.
  fn programs(&self) -> [&Program; 12] {
    [
      &self.composite,
      &self.shadow,
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
    ]
  }
}

impl Executor {
  // Create the sampler objects the batches of `walked` bind and this
  // executor does not have yet: (sampling, clamp) for a texture batch (a
  // textured mesh batch among them) and for the ramp a glyph or gradient
  // batch samples, each input's own for a paint batch, and (nearest, clamp)
  // and (linear, clamp) for a layer's composite, blur passes and backdrop
  // fill.
  unsafe fn create_samplers(&mut self, walked: &Walked) -> Result<(), String> {
    for batch in &walked.batches {
      let ramp = needs_ramp(batch).then_some((TextureSampling::Linear, TextureWrap::Clamp));
      let texture = batch.texture.map(|(_, sampling)| (sampling, TextureWrap::Clamp));
      let inputs = batch.paint.iter().flat_map(|p| p.inputs.iter()).map(|i| (i.sampling, i.wrap));
      // A composite samples nearest, or linear after a downsampling blur; a
      // blur pass and the backdrop fill nearest, a halving linear.
      let layer = matches!(batch.program, BatchProgram::LayerBegin | BatchProgram::LayerEnd);
      let composite = layer
        .then_some([(TextureSampling::Nearest, TextureWrap::Clamp), (TextureSampling::Linear, TextureWrap::Clamp)])
        .into_iter()
        .flatten();
      for key in ramp.into_iter().chain(texture).chain(inputs).chain(composite) {
        if !self.samplers.contains_key(&key) {
          let sampler = create_sampler(&self.gl, key.0, key.1)?;
          self.samplers.insert(key, sampler);
        }
      }
    }
    Ok(())
  }

  // The (nearest, linear) clamped samplers a blur pass picks from
  // (`blur_pass`), which `create_samplers` made for every layer.
  fn pass_samplers(&self) -> (glow::Sampler, glow::Sampler) {
    let nearest = self.sampler(TextureSampling::Nearest, TextureWrap::Clamp);
    (nearest, self.sampler(TextureSampling::Linear, TextureWrap::Clamp))
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
  // pixels, in the frame whose `uFrag` is `frag`: the shader's program in
  // use, its params set by its uniform table, `uSize`, `uFrag` and
  // `iResolution` set after them (so a param of any of those names cannot
  // displace them), its inputs bound on units 0.. in order, one
  // draw, and the units it used unbound again. TEXTURE_UNIT is the active
  // unit before and after, with nothing bound on it.
  unsafe fn draw_paint(&self, paint: &PaintDraw, size: [f32; 2], frag: [f32; 2], count: usize) {
    let gl = &self.gl;
    let shader = paint.shader.linked();
    gl.use_program(Some(shader.program));
    for param in paint.params.iter() {
      // A name the program has not got is skipped: alloy validated it.
      let Some((location, kind, elements)) = shader.uniforms.get(&param.name) else { continue };
      apply_param(gl, &param.name, location, *kind, *elements, &param.value);
    }
    gl.uniform_2_f32(Some(&shader.size), size[0], size[1]);
    gl.uniform_2_f32(Some(&shader.frag), frag[0], frag[1]);
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
  // source parts, linked, with its `uSize` and `uFrag`, when it samples a
  // gradient its `uRampRows`, and when it reads a batch's gradient its
  // `uGradMeta`.
  unsafe fn link(gl: &glow::Context, label: &str, vertex: &[&str], fragment: &[&str]) -> Result<Program, String> {
    let program = link_program(gl, label, &vertex.concat(), &fragment.concat())?;
    let size = uniform(gl, program, label, "uSize")?;
    let frag = uniform(gl, program, label, "uFrag")?;
    let ramp_rows = gl.get_uniform_location(program, "uRampRows");
    let grad_meta = gl.get_uniform_location(program, "uGradMeta");
    Ok(Program { program, size, frag, ramp_rows, grad_meta })
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
