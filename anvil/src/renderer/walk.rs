//! The pure half of a draw: a list's ops walked into the instances the
//! programs draw (plain rect fills, shapes, glyphs, paint shaders) and the
//! path meshes' device-space vertices and indices, with the save stack,
//! the transforms and the clip applied, the draws outside the clip rejected
//! and a gradient's rows composed into device space. No GL here, so the
//! walk is unit-tested without a context (`tests/anvil.rs`,
//! `tests/gradient.rs`, `tests/mesh.rs`).

use super::mesh::{scale_bucket, MeshStyle};
use super::ops::{Op, Premultiplied, Radii, Source};
use super::path::Path;
use super::record::DisplayList;
use super::{PaintShader, ShaderInput, Texture};
use crate::{affine, Affine, CoveragePolicy, GlyphQuad, RampGradient};
use crate::{ISize, Matrix, Point, Rect, ShaderParam, Size, StrokeCap, TextureSampling};
use euclid::default::{Box2D, Point2D, Transform2D};
use euclid::Angle;
use std::f32::consts::SQRT_2;
use std::sync::Arc;

/// One quad for any program, as the instance buffer carries it. All
/// f32 and `repr(C)`, so a slice of instances is uploaded as its bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instance {
  /// The quad in local space: x, y, width, height.
  pub rect: [f32; 4],
  /// The 2x3 affine, local to device: m11, m12, m21, m22 (euclid's
  /// row-vector convention: device = local * m).
  pub linear: [f32; 4],
  /// m31, m32 (the 2D map's translation), the shape kind, the stroke width
  /// (0 = fill).
  pub offset_kind_stroke: [f32; 4],
  /// Premultiplied color. The glyph kind alone carries straight alpha: its
  /// coverage policy reads the color's luminance and alpha apart, and the
  /// fragment stage premultiplies after it.
  pub color: [f32; 4],
  /// The texture rect in normalized texels at the quad's corners: u0, v0,
  /// u1, v1 (a textured instance and a glyph), or the line's cap and length
  /// in x and y (line kind). Unused, zero, for untextured rects and ovals.
  pub tex: [f32; 4],
  /// The clip in device pixels: x0, y0, x1, y1. Tested per fragment.
  pub clip: [f32; 4],
  /// The shape's corner radii in local pixels, CSS order; zero for the
  /// kinds without corners.
  pub radii: [f32; 4],
  /// The rounded clip in effect, in device pixels: x0, y0, x1, y1. Read
  /// only when `rclip_radii` has a non-zero entry.
  pub rclip: [f32; 4],
  /// The rounded clip's corner radii in device pixels, CSS order; all zero
  /// when no rounded clip is in effect.
  pub rclip_radii: [f32; 4],
}

/// The floats of one instance: nine vec4 attributes (GLES 3.0 guarantees
/// sixteen).
pub const INSTANCE_FLOATS: usize = 36;

// The upload sends a slice of instances as its bytes: no padding, every
// field an f32.
const _: () = assert!(std::mem::size_of::<Instance>() == INSTANCE_FLOATS * std::mem::size_of::<f32>());

/// One vertex of a path mesh, as the mesh vertex buffer carries it: all
/// f32 and `repr(C)`, so a slice of them is uploaded as its bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshVertex {
  /// The position in device pixels.
  pub device: [f32; 2],
  /// Premultiplied color; for a mesh filled from a source, the paint's
  /// alpha in all four channels.
  pub color: [f32; 4],
  /// The clip in device pixels: x0, y0, x1, y1. Tested per fragment.
  pub clip: [f32; 4],
}

/// The floats of one mesh vertex: a vec2 and two vec4 attributes.
pub const MESH_VERTEX_FLOATS: usize = 10;

// The upload sends a slice of mesh vertices as its bytes: no padding,
// every field an f32.
const _: () = assert!(std::mem::size_of::<MeshVertex>() == MESH_VERTEX_FLOATS * std::mem::size_of::<f32>());

/// The shape kinds the shape program's fragment stage switches on. The
/// `const int KIND_*` declarations in exec.rs's SHAPE_FRAGMENT must match
/// them.
pub const KIND_RECT: f32 = 0.0;
pub const KIND_OVAL: f32 = 1.0;
pub const KIND_LINE: f32 = 2.0;
/// A glyph quad: the atlas's red channel is its coverage, its color
/// straight alpha, turned into color under the batch's coverage policy.
/// It samples without `KIND_TEXTURED`, which means a premultiplied texel.
/// It draws through the glyph program, which never reads the kind; the
/// shape program has no arm for it.
pub const KIND_GLYPH: f32 = 3.0;
pub const KIND_ROUNDED_RECT: f32 = 4.0;
/// A paint shader's quad. It draws through its shader's program, which
/// never reads the kind; the shape program has no arm for it.
pub const KIND_PAINT: f32 = 5.0;
/// Added to a shape kind when the instance samples its texture: coverage
/// times texel. A texture quad is `KIND_RECT + KIND_TEXTURED`.
pub const KIND_TEXTURED: f32 = 8.0;

/// Local pixels a rounded rect's quad extends past its edge (and past a
/// stroke's half width), so the quad's edge never coincides with the
/// analytic edge on a straight run, where MSAA and the ramp would both
/// attenuate. exec.rs's GLSL const of the same name must match.
pub const ROUNDED_QUAD_PAD: f32 = 1.0;

/// How far apart the two axis scales of a transform may be and still count
/// as one scale for a rounded clip's device radii: float noise from
/// composed transforms, not a real non-uniform scale (the value oplog.rs
/// uses for the same question about stroke widths).
const UNIFORM_SCALE_EPSILON: f32 = 1e-5;

/// The line caps, as the instance carries them in `tex.x` (line kind). The
/// `const int CAP_*` declarations in exec.rs's SHAPE_FRAGMENT must match them.
pub const CAP_BUTT: f32 = 0.0;
pub const CAP_SQUARE: f32 = 1.0;
pub const CAP_ROUND: f32 = 2.0;

/// The device area, in device pixels, under which a plain rect fill rides
/// along in an open shape batch instead of opening a fill batch of its own
/// (256 x 256): about where the fragment cost the fill program would save
/// on the TV's Mali meets the CPU cost of one more draw call. A tuning
/// value, to be re-read on the TV.
pub const FILL_FOLD_AREA: f32 = 65_536.0;

/// The program a batch draws through, by the class of its instances.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BatchProgram {
  /// Plain rect fills only (see `plain_fill`): the fill program, no
  /// texture.
  Fill,
  /// Every other shape and texture quad, and the small plain fills that
  /// ride along with them: the shape program.
  Shape,
  /// Glyph runs: the glyph program.
  Glyph,
  /// One paint shader draw: the program of the batch's `paint` shader.
  Paint,
  /// Path meshes: a range of `Walked::mesh_indices`, drawn through the
  /// mesh program (its gradient or textured variant by the batch's
  /// `gradient` and `texture`).
  Mesh,
}

/// What the executor needs for a paint batch beside its one instance: the
/// shader, its params and inputs, and `resolution`, the quad's size in
/// device pixels (`iResolution`). Equal when it holds the same handle and
/// the same shared slices, and the same resolution.
#[derive(Clone)]
pub struct PaintDraw {
  pub shader: PaintShader,
  pub params: Arc<[ShaderParam]>,
  pub inputs: Arc<[ShaderInput]>,
  pub resolution: [f32; 2],
}

impl PartialEq for PaintDraw {
  fn eq(&self, other: &PaintDraw) -> bool {
    self.shader == other.shader
      && Arc::ptr_eq(&self.params, &other.params)
      && Arc::ptr_eq(&self.inputs, &other.inputs)
      && self.resolution == other.resolution
  }
}

impl std::fmt::Debug for PaintDraw {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("PaintDraw")
      .field("shader", &self.shader)
      .field("params", &self.params)
      .field("inputs", &self.inputs.len())
      .field("resolution", &self.resolution)
      .finish()
  }
}

/// A gradient as one batch draws it: the gradient, shared with the op that
/// carries it, and `rows`, which map a device pixel to the gradient's
/// parameter space (the gradient's own rows composed with the inverse of
/// the instance's device map), so a program evaluates it from `vDevice`
/// alone. Equal when it holds the same gradient and the same rows.
#[derive(Clone)]
pub struct GradientDraw {
  pub gradient: Arc<RampGradient>,
  pub rows: Affine,
}

impl PartialEq for GradientDraw {
  fn eq(&self, other: &GradientDraw) -> bool {
    Arc::ptr_eq(&self.gradient, &other.gradient) && self.rows == other.rows
  }
}

impl std::fmt::Debug for GradientDraw {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("GradientDraw")
      .field("kind", &self.gradient.kind)
      .field("tile", &self.gradient.tile)
      .field("rows", &self.rows)
      .finish()
  }
}

/// A run of consecutive instances one draw issues, of one class: plain
/// rect fills (untextured, unstroked, under no rounded clip), shapes (the
/// shape instances before any texture, or every shape that shares one
/// texture and sampling, small plain fills among them) or glyphs (one
/// atlas, one coverage policy and at most one gradient). The classes draw
/// through different programs, so a run never mixes them, except that the
/// shape program also draws the plain fills a shape run takes. A paint
/// shader draw is a run of its own, one instance; so is a gradient shape,
/// a fill or a shape whose program is that class's gradient variant. A
/// mesh run is consecutive path meshes that share a texture and its map,
/// a rounded clip and no gradient; a gradient mesh is a run of its own.
#[derive(Clone, Debug, PartialEq)]
pub struct Batch {
  /// The run's first instance and its instance count for the quad
  /// programs; for `Mesh`, its first index into `Walked::mesh_indices` and
  /// its index count.
  pub start: usize,
  pub count: usize,
  /// The program the run draws through. `Glyph` exactly when `policy` is
  /// set; a `Fill` run has no texture.
  pub program: BatchProgram,
  /// The texture bound for the run, with its sampling; None for a run of
  /// fills, or of shapes that sample nothing.
  pub texture: Option<(glow::NativeTexture, TextureSampling)>,
  /// Set exactly when the run is glyphs (`program` is `Glyph`): the policy
  /// their coverage is turned into color with.
  pub policy: Option<CoveragePolicy>,
  /// A gradient draw's gradient, in device space: such a run holds that
  /// one op's instances (a glyph run's quads, or one shape), and draws
  /// through its program's gradient variant (a fill or a shape) or with
  /// the glyph program's gradient on.
  pub gradient: Option<GradientDraw>,
  /// Set exactly when the run is a paint shader draw (`program` is
  /// `Paint`); `texture` is then None, the inputs bind their own units.
  pub paint: Option<PaintDraw>,
  /// A mesh run's rounded clip, a uniform pair for the whole run. None
  /// for every quad run: a quad carries its rounded clip per instance.
  pub rounded: Option<RoundedClip>,
  /// A textured mesh run's map from a device pixel to the normalized texel
  /// (the image's own map composed with the inverse of the meshes' device
  /// map), so no vertex carries a uv. None for every other run.
  pub texel: Option<Affine>,
}

/// What a walk produces: the instances and the path meshes in painter's
/// order and the batches over them, back to back and covering every
/// instance and every mesh index.
#[derive(Default)]
pub struct Walked {
  pub instances: Vec<Instance>,
  /// The path meshes' vertices in device space, and their triangles'
  /// indices into them, mesh after mesh.
  pub mesh_vertices: Vec<MeshVertex>,
  pub mesh_indices: Vec<u32>,
  pub batches: Vec<Batch>,
  /// The path ops drawn: one mesh each.
  pub meshes: u32,
  /// The meshes tessellated by this walk: the path caches' misses.
  pub tessellated: u32,
  /// The union of the emitted instances' and meshes' device clip boxes,
  /// rounded outward to whole pixels: nothing draws outside it, so the
  /// executor scissors its clear and its draws to it. None when nothing
  /// was emitted.
  pub scissor: Option<Box2D<i32>>,
  /// The device clip box of every mesh drawn, so `scissor` takes the
  /// meshes' clips without walking their vertices.
  mesh_clips: Vec<[f32; 4]>,
  /// The last batch takes no further instance: a gradient draw or a paint
  /// shader draw ended it.
  sealed: bool,
}

impl Walked {
  // Append the shape `instance` to the current batch when it can share its
  // draw, else open a new batch. A plain fill (`plain`, see `plain_fill`)
  // joins an open fill batch, or an open shape batch while its device area
  // is below FILL_FOLD_AREA (the shape program draws a rect too, and one
  // draw call fewer where rects and other shapes interleave); otherwise it
  // opens a fill batch. Any other shape shares only an open shape batch
  // (the shader branches on the kind; a bound sampler it never reads costs
  // nothing), so a texture change, a sampling change, a fill run or a
  // glyph run ends a run. A gradient instance (`gradient` Some) always
  // opens a batch of its own and seals it: a fill batch when it is plain
  // (the fill program's gradient variant), else a shape batch; no fold
  // rule applies, and no instance after it joins.
  fn push(
    &mut self,
    instance: Instance,
    texture: Option<(glow::NativeTexture, TextureSampling)>,
    plain: bool,
    gradient: Option<GradientDraw>,
  ) {
    let start = self.instances.len();
    if let Some(gradient) = gradient {
      let program = if plain { BatchProgram::Fill } else { BatchProgram::Shape };
      self.open(Batch {
        start,
        count: 1,
        program,
        texture,
        policy: None,
        gradient: Some(gradient),
        paint: None,
        rounded: None,
        texel: None,
      });
      self.sealed = true;
      self.instances.push(instance);
      return;
    }
    let sealed = self.sealed;
    let last = self.batches.last_mut().filter(|_| !sealed);
    if plain {
      match last {
        Some(last) if last.program == BatchProgram::Fill => last.count += 1,
        Some(last) if last.program == BatchProgram::Shape && device_area(&instance) < FILL_FOLD_AREA => last.count += 1,
        _ => self.open(Batch {
          start,
          count: 1,
          program: BatchProgram::Fill,
          texture: None,
          policy: None,
          gradient: None,
          paint: None,
          rounded: None,
          texel: None,
        }),
      }
      self.instances.push(instance);
      return;
    }
    let shape = Batch {
      start,
      count: 1,
      program: BatchProgram::Shape,
      texture,
      policy: None,
      gradient: None,
      paint: None,
      rounded: None,
      texel: None,
    };
    match last.filter(|last| last.program == BatchProgram::Shape) {
      None => self.open(shape),
      Some(last) if texture.is_none() || last.texture == texture => last.count += 1,
      // The shapes before it do not care which sampler is bound.
      Some(last) if last.texture.is_none() => {
        last.texture = texture;
        last.count += 1;
      }
      Some(_) => self.open(shape),
    }
    self.instances.push(instance);
  }

  // Append one glyph op's surviving `instances` over the atlas `atlas`
  // (sampled linearly) under `policy`. A solid run joins the current batch
  // when that batch is a solid glyph run over the same atlas under the
  // same policy; a gradient run is its own batch, sealed after it, so the
  // next instance opens a new one.
  fn push_glyphs(
    &mut self,
    instances: Vec<Instance>,
    atlas: glow::NativeTexture,
    policy: CoveragePolicy,
    gradient: Option<GradientDraw>,
  ) {
    if instances.is_empty() {
      return;
    }
    let start = self.instances.len();
    let count = instances.len();
    let texture = Some((atlas, TextureSampling::Linear));
    let joins = |last: &Batch| {
      last.program == BatchProgram::Glyph
        && gradient.is_none()
        && last.gradient.is_none()
        && last.texture == texture
        && last.policy == Some(policy)
    };
    let sealed = self.sealed;
    let graded = gradient.is_some();
    match self.batches.last_mut().filter(|last| !sealed && joins(last)) {
      Some(last) => last.count += count,
      None => self.open(Batch {
        start,
        count,
        program: BatchProgram::Glyph,
        texture,
        policy: Some(policy),
        gradient,
        paint: None,
        rounded: None,
        texel: None,
      }),
    }
    self.sealed = graded;
    self.instances.extend(instances);
  }

  // Append a paint shader's one `instance` as a batch of its own, sealed
  // after it: the program is the shader's, so nothing before or after it
  // shares its draw.
  fn push_paint(&mut self, instance: Instance, paint: PaintDraw) {
    let start = self.instances.len();
    self.open(Batch {
      start,
      count: 1,
      program: BatchProgram::Paint,
      texture: None,
      policy: None,
      gradient: None,
      paint: Some(paint),
      rounded: None,
      texel: None,
    });
    self.sealed = true;
    self.instances.push(instance);
  }

  // Add a mesh op's `count` indices, appended at `start` of
  // `mesh_indices`, to the current batch when that batch is an open mesh
  // run with the same `texture`, `rounded` clip and `texel` map and
  // neither carries a gradient, else open a mesh batch. A gradient mesh is
  // its own batch, sealed after it, as a gradient quad is. A mesh run's
  // indices are contiguous: every index after its start is its own.
  fn push_mesh(
    &mut self,
    start: usize,
    count: usize,
    texture: Option<(glow::NativeTexture, TextureSampling)>,
    rounded: Option<RoundedClip>,
    texel: Option<Affine>,
    gradient: Option<GradientDraw>,
  ) {
    let joins = |last: &Batch| {
      last.program == BatchProgram::Mesh
        && gradient.is_none()
        && last.gradient.is_none()
        && last.texture == texture
        && last.rounded == rounded
        && last.texel == texel
    };
    let sealed = self.sealed;
    let graded = gradient.is_some();
    match self.batches.last_mut().filter(|last| !sealed && joins(last)) {
      Some(last) => last.count += count,
      None => self.open(Batch {
        start,
        count,
        program: BatchProgram::Mesh,
        texture,
        policy: None,
        gradient,
        paint: None,
        rounded,
        texel,
      }),
    }
    self.sealed = graded;
  }

  // Open `batch` as the current one, which takes instances again.
  fn open(&mut self, batch: Batch) {
    self.batches.push(batch);
    self.sealed = false;
  }
}

/// The transform and the clips in effect, as `Save` pushes them.
#[derive(Clone, Copy)]
struct State {
  matrix: Matrix,
  /// Device pixels. Empty (max below min) once two clips are disjoint,
  /// which rejects every draw until the enclosing restore. A rounded clip
  /// narrows it to its rect too, so rejection and the box test hold.
  clip: Box2D<f32>,
  /// The one rounded clip in effect, if any; a further one only narrows
  /// `clip`.
  rounded: Option<RoundedClip>,
}

/// An axis-aligned rounded clip, in device pixels: its rect and its corner
/// radii in CSS order, at least one of them non-zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoundedClip {
  pub rect: Box2D<f32>,
  pub radii: Radii,
}

/// The instances a list draws into a target of `size` device pixels, in
/// painter's order, with every op's state applied: the transform stack,
/// the device-space clip, and the rejection of draws that fall outside it.
/// Every stage 1 shape draws: rect and oval fills and strokes, lines with
/// their caps, and texture quads; stage 2 adds rounded rects, one rounded
/// clip in device space, shapes filled with an image, and glyph runs;
/// stage 4 adds gradients and paths, a path as its mesh's device-space
/// triangles (`push_path`). A stroke expands
/// its quad by half the
/// width on every side so it covers the band, which the fragment stage
/// keeps from the quad's local coordinates. There is no hairline rule: a
/// width of zero or below draws nothing, and a width under one device pixel
/// draws thin, covering only some of the MSAA samples it crosses.
pub fn walk(list: &DisplayList, size: ISize) -> Walked {
  // The target rect, not an infinite one: every instance's clip stays
  // finite (a highp float holds 1e7 but not INFINITY reliably), the
  // rejection culls offscreen content, and the rendertree's "infinite"
  // overflow extents (composite.rs, CLIP_INF) intersect down to the target.
  let mut state = State { matrix: Matrix::identity(), clip: Box2D::from_size(size.to_f32()), rounded: None };
  let mut stack = Vec::new();
  let mut out = Walked::default();
  walk_ops(list.executable(), &mut state, &mut stack, &mut out);
  out.scissor = scissor(&out.instances, &out.mesh_clips);
  out
}

// The union of `instances`' and the meshes' device clips, rounded outward
// to whole pixels; None for neither. Every clip is already intersected with
// the target and finite (see `walk`), so the union lies inside the target.
fn scissor(instances: &[Instance], mesh_clips: &[[f32; 4]]) -> Option<Box2D<i32>> {
  let clips = instances.iter().map(|i| &i.clip).chain(mesh_clips);
  let union = clips.map(|c| Box2D::new(Point2D::new(c[0], c[1]), Point2D::new(c[2], c[3])));
  union.reduce(|a, b| a.union(&b)).map(|b| b.round_out().to_i32())
}

// One list's ops. A restore pops only what this list saved (a restore with
// nothing saved does nothing, as `oplog::walk` treats it), so a nested
// list with an extra restore cannot pop its parent's save.
fn walk_ops(ops: &[Op], state: &mut State, stack: &mut Vec<State>, out: &mut Walked) {
  let base = stack.len();
  for op in ops {
    match op {
      Op::Save => stack.push(*state),
      Op::Restore => {
        if stack.len() > base {
          if let Some(saved) = stack.pop() {
            *state = saved;
          }
        }
      }
      Op::Translate { x, y } => state.matrix = Matrix::translation(*x, *y, 0.0).then(&state.matrix),
      Op::Scale { x, y } => state.matrix = Matrix::scale(*x, *y, 1.0).then(&state.matrix),
      Op::Transform(m) => state.matrix = m.then(&state.matrix),
      Op::ClipRect(rect) => {
        let m2 = clip_map(state);
        if m2.m12 != 0.0 || m2.m21 != 0.0 {
          unsupported!("a clip under a rotated transform", "the clip is the rotated rect's bounding box");
        }
        let device = m2.outer_transformed_rect(rect).to_box2d();
        state.clip = state.clip.intersection_unchecked(&device);
      }
      Op::ClipRoundedRect { rect, radii } => clip_rounded(state, rect, radii),
      Op::DrawRect { rect, radii, color, stroke, source } => {
        let kind = if radii.iter().all(|r| *r == 0.0) { KIND_RECT } else { KIND_ROUNDED_RECT };
        push_shape(state, rect, kind, *radii, *color, *stroke, source.as_ref(), out)
      }
      Op::DrawOval { bounds, color, stroke, source } => {
        push_shape(state, bounds, KIND_OVAL, [0.0; 4], *color, *stroke, source.as_ref(), out)
      }
      Op::DrawLine { from, to, color, width, cap, gradient } => {
        push_line(state, *from, *to, *color, *width, *cap, gradient.as_ref(), out)
      }
      Op::DrawTexture { texture, src, dst, sampling, alpha } => {
        push_texture(state, texture, src, dst, *sampling, *alpha, out)
      }
      Op::DrawGlyphs { atlas, quads, bounds, policy, gradient } => {
        push_glyphs(state, atlas, quads, bounds, *policy, gradient.as_ref(), out)
      }
      Op::DrawShader { rect, shader, params, inputs } => push_paint(state, rect, shader, params, inputs, out),
      Op::DrawPath { path, style, color, source } => push_path(state, path, *style, *color, source.as_ref(), out),
      Op::DrawList { list, opacity } => {
        if *opacity < 1.0 {
          unsupported!("draw_display_list below opacity 1", "the nested list draws at full opacity");
        }
        // Whatever the nested list leaves on the stack or in the state
        // ends with it.
        let depth = stack.len();
        let saved = *state;
        walk_ops(list.executable(), state, stack, out);
        stack.truncate(depth);
        *state = saved;
      }
    }
  }
}

// The current state's transform as the 2D map a clip uses. One call site
// for both clip ops, so the perspective warning is one line.
fn clip_map(state: &State) -> Transform2D<f32> {
  if !state.matrix.is_2d() {
    unsupported!("a clip under a perspective transform", "the clip uses the affine part");
  }
  state.matrix.to_2d()
}

// An intersect clip to `rect` with the corner `radii` (local, CSS order,
// clamped). The device box of the rect narrows the clip box in every case;
// the rounded corners ride on the instances as the one rounded clip, in
// device space, when the transform keeps the rect axis-aligned and no
// rounded clip is in effect yet. Otherwise the clip degrades to its box,
// warned once per cause.
fn clip_rounded(state: &mut State, rect: &Rect, radii: &Radii) {
  let m2 = clip_map(state);
  let device = m2.outer_transformed_rect(rect).to_box2d();
  state.clip = state.clip.intersection_unchecked(&device);
  if radii.iter().all(|r| *r == 0.0) {
    // A plain rect: the box did it.
    return;
  }
  if m2.m12 != 0.0 || m2.m21 != 0.0 {
    unsupported!("a rounded clip under a rotated transform", "the clip is the rotated rect's bounding box");
    return;
  }
  if state.rounded.is_some() {
    unsupported!("nested rounded clips", "the inner one clips as a plain rect");
    return;
  }
  let (sx, sy) = (m2.m11.abs(), m2.m22.abs());
  if (sx - sy).abs() > UNIFORM_SCALE_EPSILON {
    unsupported!("a rounded clip under a non-uniform scale", "the corners use the smaller scale");
  }
  let scale = sx.min(sy);
  let [tl, tr, br, bl] = radii.map(|r| r * scale);
  // A mirrored axis swaps the corners: local top-left lands on device
  // top-right under a negative x scale, on bottom-left under a negative y.
  let [tl, tr, br, bl] = if m2.m11 < 0.0 { [tr, tl, bl, br] } else { [tl, tr, br, bl] };
  let device_radii = if m2.m22 < 0.0 { [bl, br, tr, tl] } else { [tl, tr, br, bl] };
  if device_radii.iter().any(|r| *r > 0.0) {
    state.rounded = Some(RoundedClip { rect: device, radii: device_radii });
  }
}

// The current state's transform as the 2D map a draw uses.
fn state_map(state: &State) -> Transform2D<f32> {
  if !state.matrix.is_2d() {
    unsupported!("a perspective transform", "the draw uses the affine part");
  }
  state.matrix.to_2d()
}

// `gradient` as a draw whose op space reaches device pixels through `m`:
// its rows composed with the inverse of `m`. None when `m` is singular,
// and the draw then takes its solid color.
fn gradient_draw(gradient: &Arc<RampGradient>, m: &Transform2D<f32>) -> Option<GradientDraw> {
  let from_device = affine::invert(affine::of_2d(m))?;
  let rows = affine::compose([gradient.x_row, gradient.y_row], from_device);
  Some(GradientDraw { gradient: gradient.clone(), rows })
}

// The 2D map and the device clip of a draw covering `rect` under the
// current state; None when the clip rejects it.
fn placed(state: &State, rect: &Rect) -> Option<(Transform2D<f32>, [f32; 4])> {
  let m2 = state_map(state);
  placed_under(state, m2, rect).map(|clip| (m2, clip))
}

// The device clip of a draw covering `rect` under the 2D map `m2` (the
// state's, or a line's own composed onto it); None when the clip rejects it.
fn placed_under(state: &State, m2: Transform2D<f32>, rect: &Rect) -> Option<[f32; 4]> {
  let bounds = m2.outer_transformed_rect(rect).to_box2d();
  if state.clip.is_empty() || !bounds.intersects(&state.clip) {
    return None;
  }
  let clip = state.clip;
  Some([clip.min.x, clip.min.y, clip.max.x, clip.max.y])
}

/// What one draw puts on its instance beside the state's map and clips.
struct Quad {
  /// The quad in local space.
  rect: Rect,
  /// A `KIND_*`, with `KIND_TEXTURED` added when it samples.
  kind: f32,
  /// The stroke width; 0 fills.
  stroke: f32,
  color: Premultiplied,
  tex: [f32; 4],
  /// The shape's corner radii; zero for the kinds without corners.
  radii: Radii,
}

// The one place an instance's fields are set: `quad` under the 2D map `m`
// with the device clip `clip` (from `placed`), and the state's rounded
// clip, zeros when none is in effect.
fn instance(state: &State, m: &Transform2D<f32>, clip: [f32; 4], quad: Quad) -> Instance {
  let (rclip, rclip_radii) = match state.rounded {
    Some(r) => ([r.rect.min.x, r.rect.min.y, r.rect.max.x, r.rect.max.y], r.radii),
    None => ([0.0; 4], [0.0; 4]),
  };
  let rect = quad.rect;
  Instance {
    rect: [rect.origin.x, rect.origin.y, rect.size.width, rect.size.height],
    linear: [m.m11, m.m12, m.m21, m.m22],
    offset_kind_stroke: [m.m31, m.m32, quad.kind, quad.stroke],
    color: quad.color,
    tex: quad.tex,
    clip,
    radii: quad.radii,
    rclip,
    rclip_radii,
  }
}

// A rect, a rounded rect or an oval (`kind`) under the current state,
// filled or stroked `stroke` wide centered on its edge, unless the clip
// rejects it or it is fully transparent. A stroke's quad is the shape's box
// grown by half the width on every side, so it covers the outer half of
// the band; a rounded rect's grows by `ROUNDED_QUAD_PAD` more, so its
// analytic edge never lies on the quad's. With an image `source` the
// instance samples it: coverage times texel, faded by the color's alpha
// (the paint's RGB is not used for an image, as Impeller did not use it),
// the uv the quad's corners through the image's inverse map. With a
// gradient `source` it keeps its kind and takes the gradient's color,
// faded the same way, in a batch of its own (`Walked::push`); under a
// singular map it draws its solid color.
#[allow(clippy::too_many_arguments)]
fn push_shape(
  state: &State,
  rect: &Rect,
  kind: f32,
  radii: Radii,
  color: Premultiplied,
  stroke: Option<f32>,
  source: Option<&Source>,
  out: &mut Walked,
) {
  let s = stroke.unwrap_or(0.0);
  let pad = if kind == KIND_ROUNDED_RECT { ROUNDED_QUAD_PAD } else { 0.0 };
  let grow = s * 0.5 + pad;
  let quad = rect.inflate(grow, grow);
  let Some((m2, clip)) = placed(state, &quad) else { return };
  if color[3] == 0.0 {
    return;
  }
  let gradient = match source {
    Some(Source::Gradient(g)) => gradient_draw(g, &m2),
    _ => None,
  };
  let image = match source {
    Some(Source::Image(image)) => Some(image),
    _ => None,
  };
  let (kind, color, tex, texture) = match image {
    // The ramp sample is premultiplied in the program: the alpha in all
    // four channels fades it, as for an image.
    None if gradient.is_some() => (kind, [color[3]; 4], [0.0; 4], None),
    None => (kind, color, [0.0; 4], None),
    Some(image) => {
      // A texture's size is never zero (see `push_texture`).
      let size = image.texture.size();
      let (w, h) = (size.width as f32, size.height as f32);
      let t0 = image.to_texel.transform_point(quad.min());
      let t1 = image.to_texel.transform_point(quad.max());
      // The texel is premultiplied: the alpha in all four channels fades it.
      let faded = [color[3]; 4];
      (
        kind + KIND_TEXTURED,
        faded,
        [t0.x / w, t0.y / h, t1.x / w, t1.y / h],
        Some((image.texture.name(), image.sampling)),
      )
    }
  };
  let shape = instance(state, &m2, clip, Quad { rect: quad, kind, stroke: s, color, tex, radii });
  let plain = plain_fill(&shape);
  out.push(shape, texture, plain, gradient);
}

// Whether `instance` is a plain rect fill, which the fill program draws:
// a rect, not textured, not stroked, under no rounded clip. Its coverage
// is 1 inside the quad, whose edges are MSAA's.
fn plain_fill(instance: &Instance) -> bool {
  let [_, _, kind, stroke] = instance.offset_kind_stroke;
  kind == KIND_RECT && stroke == 0.0 && instance.rclip_radii == [0.0; 4]
}

// The device pixels `instance`'s quad covers, unclipped: its local area
// times the absolute determinant of its 2D map.
fn device_area(instance: &Instance) -> f32 {
  let [m11, m12, m21, m22] = instance.linear;
  (m11 * m22 - m12 * m21).abs() * instance.rect[2] * instance.rect[3]
}

// A segment stroked `width` wide under the current state, unless it has no
// width, the clip rejects it or it is fully transparent. The quad lives in
// the segment's own frame: x from `from` along the segment, y across it,
// extended by half the width at both ends for square and round caps. A
// zero-length segment is a width-square dot for those caps and nothing
// for butt caps, as Skia and Impeller draw it. A `gradient` is in the op's
// space, not the segment's frame, so its rows compose with the inverse of
// the state's map; the line then takes the gradient's color in a batch of
// its own, its solid color under a singular map.
#[allow(clippy::too_many_arguments)]
fn push_line(
  state: &State,
  from: Point,
  to: Point,
  color: Premultiplied,
  width: f32,
  cap: StrokeCap,
  gradient: Option<&Arc<RampGradient>>,
  out: &mut Walked,
) {
  if width <= 0.0 || color[3] == 0.0 {
    return;
  }
  let (dx, dy) = (to.x - from.x, to.y - from.y);
  let len = (dx * dx + dy * dy).sqrt();
  let ext = match cap {
    StrokeCap::Butt => 0.0,
    StrokeCap::Square | StrokeCap::Round => width * 0.5,
  };
  let local = Rect::new(Point::new(-ext, -width * 0.5), Size::new(len + 2.0 * ext, width));
  // Row vectors: local (len, 0) rotates onto (dx, dy), then moves to `from`,
  // then takes the state's map.
  let line_map = Transform2D::rotation(Angle::radians(dy.atan2(dx))).then(&Transform2D::translation(from.x, from.y));
  let state_m = state_map(state);
  let m = line_map.then(&state_m);
  let Some(clip) = placed_under(state, m, &local) else { return };
  let tex = [cap_value(cap), len, 0.0, 0.0];
  let gradient = gradient.and_then(|g| gradient_draw(g, &state_m));
  // The ramp sample is premultiplied in the program: the alpha fades it.
  let color = if gradient.is_some() { [color[3]; 4] } else { color };
  out.push(
    instance(state, &m, clip, Quad { rect: local, kind: KIND_LINE, stroke: width, color, tex, radii: [0.0; 4] }),
    None,
    false,
    gradient,
  );
}

// The cap as the instance carries it.
fn cap_value(cap: StrokeCap) -> f32 {
  match cap {
    StrokeCap::Butt => CAP_BUTT,
    StrokeCap::Square => CAP_SQUARE,
    StrokeCap::Round => CAP_ROUND,
  }
}

// The texel rect `src` of `texture` mapped onto `dst` under the current
// state, faded by `alpha`, unless the clip rejects it or it is fully
// transparent. Texel row 0 is the top of `src` (offscreen lists are
// pre-flipped for it, a text layer's among them), so v is not flipped:
// the top edge of `dst` samples `src.min_y()`.
fn push_texture(
  state: &State,
  texture: &Texture,
  src: &Rect,
  dst: &Rect,
  sampling: TextureSampling,
  alpha: f32,
  out: &mut Walked,
) {
  let Some((m2, clip)) = placed(state, dst) else { return };
  if alpha <= 0.0 {
    return;
  }
  // A texture's size is never zero: the registry refuses one, and a layer
  // is at least one texel.
  let size = texture.size();
  let (w, h) = (size.width as f32, size.height as f32);
  let quad = Quad {
    rect: *dst,
    // A rect fill's coverage is 1 inside the quad: the texel as it is.
    kind: KIND_RECT + KIND_TEXTURED,
    stroke: 0.0,
    // The texel is premultiplied, so scaling all four channels by the
    // alpha fades it as a whole (the fragment stage multiplies).
    color: [alpha; 4],
    tex: [src.min_x() / w, src.min_y() / h, src.max_x() / w, src.max_y() / h],
    radii: [0.0; 4],
  };
  out.push(instance(state, &m2, clip, quad), Some((texture.name(), sampling)), false, None);
}

// One glyph run under the current state: rejected whole when its `bounds`
// fall outside the clip, else one `KIND_GLYPH` instance per quad the clip
// keeps and whose alpha is not zero. A quad's rect is its `dst`, its uv its
// `src` over the atlas's size (v grows downward with the texel rows, as for
// a texture quad), its color the quad's own, straight alpha. A
// `gradient`'s rows compose once per run with the inverse of the state's
// map; under a singular map the run draws its solid colors.
fn push_glyphs(
  state: &State,
  atlas: &Texture,
  quads: &[GlyphQuad],
  bounds: &Rect,
  policy: CoveragePolicy,
  gradient: Option<&Arc<RampGradient>>,
  out: &mut Walked,
) {
  if quads.is_empty() || placed(state, bounds).is_none() {
    return;
  }
  // An atlas's size is never zero, as for any texture (see `push_texture`).
  let size = atlas.size();
  let (w, h) = (size.width as f32, size.height as f32);
  let mut instances = Vec::with_capacity(quads.len());
  for q in quads {
    if q.color[3] == 0.0 {
      continue;
    }
    let dst = Rect::new(Point::new(q.dst[0], q.dst[1]), Size::new(q.dst[2], q.dst[3]));
    let Some((m2, clip)) = placed(state, &dst) else { continue };
    let [sx, sy, sw, sh] = q.src;
    let quad = Quad {
      rect: dst,
      kind: KIND_GLYPH,
      stroke: 0.0,
      color: q.color,
      tex: [sx / w, sy / h, (sx + sw) / w, (sy + sh) / h],
      radii: [0.0; 4],
    };
    instances.push(instance(state, &m2, clip, quad));
  }
  let gradient = gradient.and_then(|g| gradient_draw(g, &state_map(state)));
  out.push_glyphs(instances, atlas.name(), policy, gradient);
}

// A paint shader's quad over `rect` under the current state, unless the
// clip rejects it: one `KIND_PAINT` instance whose `tex` is the unit square
// (the program's `vUV` is the quad's corner, not read from it), in a batch
// of its own that carries the shader, its params and inputs, and the quad's
// device size: the device lengths of the rect's x and y edges under the map.
fn push_paint(
  state: &State,
  rect: &Rect,
  shader: &PaintShader,
  params: &Arc<[ShaderParam]>,
  inputs: &Arc<[ShaderInput]>,
  out: &mut Walked,
) {
  let Some((m2, clip)) = placed(state, rect) else { return };
  let resolution = [rect.size.width * m2.m11.hypot(m2.m12), rect.size.height * m2.m21.hypot(m2.m22)];
  let quad = Quad {
    rect: *rect,
    kind: KIND_PAINT,
    stroke: 0.0,
    // Unread: the program writes its own color.
    color: [1.0; 4],
    tex: [0.0, 0.0, 1.0, 1.0],
    radii: [0.0; 4],
  };
  let paint = PaintDraw { shader: shader.clone(), params: params.clone(), inputs: inputs.clone(), resolution };
  out.push_paint(instance(state, &m2, clip, quad), paint);
}

// A path's mesh under `style` and the current state, unless the map
// collapses it, the clip rejects it or it is fully transparent. Rejected
// twice: before the mesh exists by the path's control-point box outset by
// the stroke's reach (half the width times the larger of the miter limit
// and sqrt 2: a miter's tip, or a square cap's corner), after by the
// mesh's own box. The mesh comes from the path's cache at the map's scale
// bucket (mesh.rs), tessellated on a miss; its vertices are mapped into
// device space here and appended with the state's clip box and the color,
// its indices rebased onto them. An image `source` binds its texture and
// the batch's device-to-texel map, a gradient `source` its device-space
// rows; either fades by the color's alpha (in all four channels) and draws
// the solid color under a singular map.
fn push_path(
  state: &State,
  path: &Path,
  style: MeshStyle,
  color: Premultiplied,
  source: Option<&Source>,
  out: &mut Walked,
) {
  let m2 = state_map(state);
  let Some(bucket) = scale_bucket(&m2) else { return };
  let Some(bounds) = path.bounds() else { return };
  let reach = match style {
    MeshStyle::Fill => 0.0,
    MeshStyle::Stroke { width, miter, .. } => width * 0.5 * miter.max(SQRT_2),
  };
  if placed_under(state, m2, &bounds.inflate(reach, reach).to_rect()).is_none() {
    return;
  }
  if color[3] == 0.0 {
    return;
  }
  let (mesh, missed) = path.mesh(style, bucket);
  out.tessellated += missed as u32;
  let Some(mesh) = mesh else { return };
  let Some(clip) = placed_under(state, m2, &mesh.bounds.to_rect()) else { return };
  let mut texture = None;
  let mut texel = None;
  let mut gradient = None;
  let color = match source {
    Some(Source::Image(image)) => match image_texel(image, &m2) {
      Some(map) => {
        texture = Some((image.texture.name(), image.sampling));
        texel = Some(map);
        // The texel is premultiplied: the alpha in all four channels fades it.
        [color[3]; 4]
      }
      None => color,
    },
    Some(Source::Gradient(g)) => match gradient_draw(g, &m2) {
      Some(draw) => {
        gradient = Some(draw);
        // The ramp sample is premultiplied in the program: the alpha fades it.
        [color[3]; 4]
      }
      None => color,
    },
    None => color,
  };
  let base = out.mesh_vertices.len() as u32;
  out.mesh_vertices.extend(mesh.vertices.iter().map(|v| {
    let p = m2.transform_point(Point2D::new(v[0], v[1]));
    MeshVertex { device: [p.x, p.y], color, clip }
  }));
  let start = out.mesh_indices.len();
  out.mesh_indices.extend(mesh.indices.iter().map(|i| i + base));
  out.push_mesh(start, mesh.indices.len(), texture, state.rounded, texel, gradient);
  out.meshes += 1;
  out.mesh_clips.push(clip);
}

// The map from a device pixel to `image`'s normalized texel for a draw
// whose op space reaches device pixels through `m`: device to the op's
// space (the inverse of `m`), then to texels (the image's own map), then
// over the texture's size. None when `m` is singular.
fn image_texel(image: &super::ops::Image, m: &Transform2D<f32>) -> Option<Affine> {
  let from_device = affine::invert(affine::of_2d(m))?;
  // A texture's size is never zero (see `push_texture`).
  let size = image.texture.size();
  let normalize = [[1.0 / size.width as f32, 0.0, 0.0], [0.0, 1.0 / size.height as f32, 0.0]];
  Some(affine::compose(normalize, affine::compose(affine::of_2d(&image.to_texel), from_device)))
}
