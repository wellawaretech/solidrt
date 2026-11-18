//! The pure half of a draw: a list's ops walked into the instances the
//! shape program draws, with the save stack, the transforms and the clip
//! applied and the draws outside the clip rejected. No GL here, so the walk
//! is unit-tested without a context (`tests/anvil.rs`).

use super::super::{ISize, Matrix, Point, Rect, Size, StrokeCap, TextureSampling};
use super::ops::{Op, Premultiplied};
use super::record::DisplayList;
use super::Texture;
use euclid::default::{Box2D, Transform2D};
use euclid::Angle;

/// One quad for the shape program, as the instance buffer carries it. All
/// f32 and `repr(C)`, so a slice of instances is uploaded as its bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Instance {
  /// The quad in local space: x, y, width, height.
  pub rect: [f32; 4],
  /// The 2x3 affine, local to device: m11, m12, m21, m22 (euclid's
  /// row-vector convention: device = local * m).
  pub linear: [f32; 4],
  /// m31, m32 (the 2D map's translation), the shape kind, the stroke width
  /// (0 = fill).
  pub offset_kind_stroke: [f32; 4],
  /// Premultiplied color.
  pub color: [f32; 4],
  /// The texture rect in normalized texels: u0, v0, u1, v1 (texture kind),
  /// or the line's cap and length in x and y (line kind). Unused, zero, for
  /// rects and ovals.
  pub tex: [f32; 4],
  /// The clip in device pixels: x0, y0, x1, y1. Tested per fragment.
  pub clip: [f32; 4],
}

/// The floats of one instance: six vec4 attributes.
pub(crate) const INSTANCE_FLOATS: usize = 24;

// The upload sends a slice of instances as its bytes: no padding, every
// field an f32.
const _: () = assert!(std::mem::size_of::<Instance>() == INSTANCE_FLOATS * std::mem::size_of::<f32>());

/// The shape kinds the fragment stage switches on. The `const int KIND_*`
/// declarations in exec.rs's FRAGMENT_SRC must match them.
pub(crate) const KIND_RECT: f32 = 0.0;
pub(crate) const KIND_OVAL: f32 = 1.0;
pub(crate) const KIND_LINE: f32 = 2.0;
pub(crate) const KIND_TEXTURE: f32 = 3.0;

/// The line caps, as the instance carries them in `tex.x` (line kind). The
/// `const int CAP_*` declarations in exec.rs's FRAGMENT_SRC must match them.
pub(crate) const CAP_BUTT: f32 = 0.0;
pub(crate) const CAP_SQUARE: f32 = 1.0;
pub(crate) const CAP_ROUND: f32 = 2.0;

/// A run of consecutive instances one draw issues: the shape instances
/// before any texture, or everything that shares one texture and sampling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Batch {
  pub start: usize,
  pub count: usize,
  /// The texture bound for the run, with its sampling; None for a run of
  /// shapes only.
  pub texture: Option<(glow::NativeTexture, TextureSampling)>,
}

/// What a walk produces: the instances in painter's order and the batches
/// over them, back to back and covering every instance.
#[derive(Default)]
pub(crate) struct Walked {
  pub instances: Vec<Instance>,
  pub batches: Vec<Batch>,
}

impl Walked {
  // Append `instance` to the current batch when it can share its draw,
  // else open a new batch. Shapes share any batch (the shader branches on
  // the kind; a bound sampler they never read costs nothing), so only a
  // texture change or a sampling change ends a run.
  fn push(&mut self, instance: Instance, texture: Option<(glow::NativeTexture, TextureSampling)>) {
    let start = self.instances.len();
    match self.batches.last_mut() {
      None => self.batches.push(Batch { start, count: 1, texture }),
      Some(last) if texture.is_none() || last.texture == texture => last.count += 1,
      // The shapes before it do not care which sampler is bound.
      Some(last) if last.texture.is_none() => {
        last.texture = texture;
        last.count += 1;
      }
      Some(_) => self.batches.push(Batch { start, count: 1, texture }),
    }
    self.instances.push(instance);
  }
}

/// The transform and the clip in effect, as `Save` pushes them.
#[derive(Clone, Copy)]
struct State {
  matrix: Matrix,
  /// Device pixels. Empty (max below min) once two clips are disjoint,
  /// which rejects every draw until the enclosing restore.
  clip: Box2D<f32>,
}

/// The instances a list draws into a target of `size` device pixels, in
/// painter's order, with every op's state applied: the transform stack,
/// the device-space clip, and the rejection of draws that fall outside it.
/// Every stage 1 shape draws: rect and oval fills and strokes, lines with
/// their caps, and texture quads. A stroke expands its quad by half the
/// width on every side so it covers the band, which the fragment stage
/// keeps from the quad's local coordinates. There is no hairline rule: a
/// width of zero or below draws nothing, and a width under one device pixel
/// draws thin, covering only some of the MSAA samples it crosses.
pub(crate) fn walk(list: &DisplayList, size: ISize) -> Walked {
  // The target rect, not an infinite one: every instance's clip stays
  // finite (a highp float holds 1e7 but not INFINITY reliably), the
  // rejection culls offscreen content, and the rendertree's "infinite"
  // overflow extents (composite.rs, CLIP_INF) intersect down to the target.
  let mut state = State { matrix: Matrix::identity(), clip: Box2D::from_size(size.to_f32()) };
  let mut stack = Vec::new();
  let mut out = Walked::default();
  walk_ops(list.executable(), &mut state, &mut stack, &mut out);
  out
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
        if !state.matrix.is_2d() {
          unsupported!("a clip under a perspective transform", "the clip uses the affine part");
        }
        let m2 = state.matrix.to_2d();
        if m2.m12 != 0.0 || m2.m21 != 0.0 {
          unsupported!("a clip under a rotated transform", "the clip is the rotated rect's bounding box");
        }
        let device = m2.outer_transformed_rect(rect).to_box2d();
        state.clip = state.clip.intersection_unchecked(&device);
      }
      Op::DrawRect { rect, color, stroke } => push_shape(state, rect, *color, KIND_RECT, *stroke, out),
      Op::DrawOval { bounds, color, stroke } => push_shape(state, bounds, *color, KIND_OVAL, *stroke, out),
      Op::DrawLine { from, to, color, width, cap } => push_line(state, *from, *to, *color, *width, *cap, out),
      Op::DrawTexture { texture, src, dst, sampling, alpha } => {
        push_texture(state, texture, src, dst, *sampling, *alpha, out)
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

// A rect or an oval (`kind`) under the current state, filled or stroked
// `stroke` wide centered on its edge, unless the clip rejects it or it is
// fully transparent. A stroke's quad is the shape's box grown by half the
// width on every side, so it covers the outer half of the band.
fn push_shape(state: &State, rect: &Rect, color: Premultiplied, kind: f32, stroke: Option<f32>, out: &mut Walked) {
  let quad = match stroke {
    None => *rect,
    Some(s) => rect.inflate(s * 0.5, s * 0.5),
  };
  let Some((m2, clip)) = placed(state, &quad) else { return };
  if color[3] == 0.0 {
    return;
  }
  let instance = Instance {
    rect: [quad.origin.x, quad.origin.y, quad.size.width, quad.size.height],
    linear: [m2.m11, m2.m12, m2.m21, m2.m22],
    offset_kind_stroke: [m2.m31, m2.m32, kind, stroke.unwrap_or(0.0)],
    color,
    tex: [0.0; 4],
    clip,
  };
  out.push(instance, None);
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
  let instance = Instance {
    rect: [local.origin.x, local.origin.y, local.size.width, local.size.height],
    linear: [m.m11, m.m12, m.m21, m.m22],
    offset_kind_stroke: [m.m31, m.m32, KIND_LINE, width],
    color,
    tex: [cap_value(cap), len, 0.0, 0.0],
    clip,
  };
  out.push(instance, None);
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
  let instance = Instance {
    rect: [dst.origin.x, dst.origin.y, dst.size.width, dst.size.height],
    linear: [m2.m11, m2.m12, m2.m21, m2.m22],
    offset_kind_stroke: [m2.m31, m2.m32, KIND_TEXTURE, 0.0],
    // The texel is premultiplied, so scaling all four channels by the
    // alpha fades it as a whole (the fragment stage multiplies).
    color: [alpha; 4],
    tex: [src.min_x() / w, src.min_y() / h, src.max_x() / w, src.max_y() / h],
    clip,
  };
  out.push(instance, Some((texture.name(), sampling)));
}
