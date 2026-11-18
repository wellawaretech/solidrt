//! The pure half of a draw: a list's ops walked into the instances the
//! shape program draws, with the save stack, the transforms and the clip
//! applied and the draws outside the clip rejected. No GL here, so the walk
//! is unit-tested without a context (`tests/anvil.rs`).

use super::super::{ISize, Matrix, Rect};
use super::ops::{Op, Premultiplied};
use super::record::DisplayList;
use euclid::default::Box2D;

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
  /// The texture rect in normalized texels: u0, v0, u1, v1 (texture kind).
  pub tex: [f32; 4],
  /// The clip in device pixels: x0, y0, x1, y1. Tested per fragment.
  pub clip: [f32; 4],
}

/// The floats of one instance: six vec4 attributes.
pub(crate) const INSTANCE_FLOATS: usize = 24;

// The upload sends a slice of instances as its bytes: no padding, every
// field an f32.
const _: () = assert!(std::mem::size_of::<Instance>() == INSTANCE_FLOATS * std::mem::size_of::<f32>());

/// The shape kinds the fragment stage switches on.
pub(crate) const KIND_RECT: f32 = 0.0;
#[allow(dead_code)] // emitted from step 4
pub(crate) const KIND_OVAL: f32 = 1.0;
#[allow(dead_code)] // emitted from step 4
pub(crate) const KIND_LINE: f32 = 2.0;
#[allow(dead_code)] // emitted from step 3
pub(crate) const KIND_TEXTURE: f32 = 3.0;

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
/// Step 2 emits rect fills only; the other kinds are skipped with a warning.
pub(crate) fn walk(list: &DisplayList, size: ISize) -> Vec<Instance> {
  // The target rect, not an infinite one: every instance's clip stays
  // finite (a highp float holds 1e7 but not INFINITY reliably), the
  // rejection culls offscreen content, and the rendertree's "infinite"
  // overflow extents (composite.rs, CLIP_INF) intersect down to the target.
  let mut state = State { matrix: Matrix::identity(), clip: Box2D::from_size(size.to_f32()) };
  let mut stack = Vec::new();
  let mut out = Vec::new();
  walk_ops(list.executable(), &mut state, &mut stack, &mut out);
  out
}

// One list's ops. A restore pops only what this list saved (a restore with
// nothing saved does nothing, as `oplog::walk` treats it), so a nested
// list with an extra restore cannot pop its parent's save.
fn walk_ops(ops: &[Op], state: &mut State, stack: &mut Vec<State>, out: &mut Vec<Instance>) {
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
      Op::DrawRect { rect, color, stroke: None } => push_rect(state, rect, *color, out),
      Op::DrawRect { stroke: Some(_), .. } => unsupported!("a stroked rect", "the stroke is not drawn"),
      Op::DrawOval { .. } => unsupported!("draw_oval", "the oval is not drawn"),
      Op::DrawLine { .. } => unsupported!("draw_line", "the line is not drawn"),
      Op::DrawTexture { .. } => unsupported!("draw_texture_rect", "the texture is not drawn"),
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

// A rect fill under the current state, unless the clip rejects it or it is
// fully transparent.
fn push_rect(state: &State, rect: &Rect, color: Premultiplied, out: &mut Vec<Instance>) {
  if !state.matrix.is_2d() {
    unsupported!("a perspective transform", "the draw uses the affine part");
  }
  let m2 = state.matrix.to_2d();
  let bounds = m2.outer_transformed_rect(rect).to_box2d();
  if color[3] == 0.0 || state.clip.is_empty() || !bounds.intersects(&state.clip) {
    return;
  }
  let clip = state.clip;
  out.push(Instance {
    rect: [rect.origin.x, rect.origin.y, rect.size.width, rect.size.height],
    linear: [m2.m11, m2.m12, m2.m21, m2.m22],
    offset_kind_stroke: [m2.m31, m2.m32, KIND_RECT, 0.0],
    color,
    tex: [0.0; 4],
    clip: [clip.min.x, clip.min.y, clip.max.x, clip.max.y],
  });
}
