//! The pure half of a draw: a list's ops walked into the instances the
//! three programs draw (plain rect fills, shapes, glyphs), with the save
//! stack, the transforms and the clip applied and the draws outside the
//! clip rejected. No GL here, so the walk is unit-tested without a context
//! (`tests/anvil.rs`).

use super::ops::{Image, Op, Premultiplied, Radii};
use super::record::DisplayList;
use super::Texture;
use crate::{CoveragePolicy, GlyphGradient, GlyphQuad};
use crate::{ISize, Matrix, Point, Rect, Size, StrokeCap, TextureSampling};
use euclid::default::{Box2D, Point2D, Transform2D};
use euclid::Angle;
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
}

/// A run of consecutive instances one draw issues, of one class: plain
/// rect fills (untextured, unstroked, under no rounded clip), shapes (the
/// shape instances before any texture, or every shape that shares one
/// texture and sampling, small plain fills among them) or glyphs (one
/// atlas, one coverage policy and at most one gradient). The classes draw
/// through different programs, so a run never mixes them, except that the
/// shape program also draws the plain fills a shape run takes.
#[derive(Clone, Debug, PartialEq)]
pub struct Batch {
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
  /// A gradient run's gradient; such a run holds that one op's quads.
  pub gradient: Option<Arc<GlyphGradient>>,
}

/// What a walk produces: the instances in painter's order and the batches
/// over them, back to back and covering every instance.
#[derive(Default)]
pub struct Walked {
  pub instances: Vec<Instance>,
  pub batches: Vec<Batch>,
  /// The union of the emitted instances' device clip boxes, rounded
  /// outward to whole pixels: no instance draws outside it, so the
  /// executor scissors its clear and its draws to it. None when nothing
  /// was emitted.
  pub scissor: Option<Box2D<i32>>,
  /// The last batch takes no further instance: a gradient run ended it.
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
  // glyph run ends a run.
  fn push(&mut self, instance: Instance, texture: Option<(glow::NativeTexture, TextureSampling)>, plain: bool) {
    let start = self.instances.len();
    let sealed = self.sealed;
    let last = self.batches.last_mut().filter(|_| !sealed);
    if plain {
      match last {
        Some(last) if last.program == BatchProgram::Fill => last.count += 1,
        Some(last) if last.program == BatchProgram::Shape && device_area(&instance) < FILL_FOLD_AREA => last.count += 1,
        _ => {
          self.open(Batch { start, count: 1, program: BatchProgram::Fill, texture: None, policy: None, gradient: None })
        }
      }
      self.instances.push(instance);
      return;
    }
    let shape = Batch { start, count: 1, program: BatchProgram::Shape, texture, policy: None, gradient: None };
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
    gradient: Option<&Arc<GlyphGradient>>,
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
    match self.batches.last_mut().filter(|last| !sealed && joins(last)) {
      Some(last) => last.count += count,
      None => self.open(Batch {
        start,
        count,
        program: BatchProgram::Glyph,
        texture,
        policy: Some(policy),
        gradient: gradient.cloned(),
      }),
    }
    self.sealed = gradient.is_some();
    self.instances.extend(instances);
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
#[derive(Clone, Copy)]
struct RoundedClip {
  rect: Box2D<f32>,
  radii: Radii,
}

/// The instances a list draws into a target of `size` device pixels, in
/// painter's order, with every op's state applied: the transform stack,
/// the device-space clip, and the rejection of draws that fall outside it.
/// Every stage 1 shape draws: rect and oval fills and strokes, lines with
/// their caps, and texture quads; stage 2 adds rounded rects, one rounded
/// clip in device space, shapes filled with an image, and glyph runs. A
/// stroke expands
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
  out.scissor = scissor(&out.instances);
  out
}

// The union of `instances`' device clips, rounded outward to whole pixels;
// None for no instances. Every clip is already intersected with the target
// and finite (see `walk`), so the union lies inside the target.
fn scissor(instances: &[Instance]) -> Option<Box2D<i32>> {
  let union =
    instances.iter().map(|i| Box2D::new(Point2D::new(i.clip[0], i.clip[1]), Point2D::new(i.clip[2], i.clip[3])));
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
      Op::DrawRect { rect, radii, color, stroke, image } => {
        let kind = if radii.iter().all(|r| *r == 0.0) { KIND_RECT } else { KIND_ROUNDED_RECT };
        push_shape(state, rect, kind, *radii, *color, *stroke, image.as_ref(), out)
      }
      Op::DrawOval { bounds, color, stroke, image } => {
        push_shape(state, bounds, KIND_OVAL, [0.0; 4], *color, *stroke, image.as_ref(), out)
      }
      Op::DrawLine { from, to, color, width, cap } => push_line(state, *from, *to, *color, *width, *cap, out),
      Op::DrawTexture { texture, src, dst, sampling, alpha } => {
        push_texture(state, texture, src, dst, *sampling, *alpha, out)
      }
      Op::DrawGlyphs { atlas, quads, bounds, policy, gradient } => {
        push_glyphs(state, atlas, quads, bounds, *policy, gradient.as_ref(), out)
      }
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
// analytic edge never lies on the quad's. With `image` the instance
// samples it: coverage times texel, faded by the color's alpha (the paint's
// RGB is not used for an image, as Impeller did not use it), the uv the
// quad's corners through the image's inverse map.
#[allow(clippy::too_many_arguments)]
fn push_shape(
  state: &State,
  rect: &Rect,
  kind: f32,
  radii: Radii,
  color: Premultiplied,
  stroke: Option<f32>,
  image: Option<&Image>,
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
  let (kind, color, tex, texture) = match image {
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
  out.push(shape, texture, plain);
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
// for butt caps, as Skia and Impeller draw it.
fn push_line(
  state: &State,
  from: Point,
  to: Point,
  color: Premultiplied,
  width: f32,
  cap: StrokeCap,
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
  let m = line_map.then(&state_map(state));
  let Some(clip) = placed_under(state, m, &local) else { return };
  let tex = [cap_value(cap), len, 0.0, 0.0];
  out.push(
    instance(state, &m, clip, Quad { rect: local, kind: KIND_LINE, stroke: width, color, tex, radii: [0.0; 4] }),
    None,
    false,
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
// transparent. Texel row 0 is the top of `src` (the glyph pass writes its
// layers that way, snapshots are pre-flipped for it), so v is not flipped:
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
  out.push(instance(state, &m2, clip, quad), Some((texture.name(), sampling)), false);
}

// One glyph run under the current state: rejected whole when its `bounds`
// fall outside the clip, else one `KIND_GLYPH` instance per quad the clip
// keeps and whose alpha is not zero. A quad's rect is its `dst`, its uv its
// `src` over the atlas's size (v grows downward with the texel rows, as for
// a texture quad), its color the quad's own, straight alpha.
fn push_glyphs(
  state: &State,
  atlas: &Texture,
  quads: &[GlyphQuad],
  bounds: &Rect,
  policy: CoveragePolicy,
  gradient: Option<&Arc<GlyphGradient>>,
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
  out.push_glyphs(instances, atlas.name(), policy, gradient);
}
