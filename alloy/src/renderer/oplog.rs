//! The log of what a display list recorded, for alloy's own tests: the ops
//! in order with their arguments, a paint as the fields set on it, a path
//! as its segments, a nested list as one op that can be opened. Plain data
//! over the vocabulary, kept by the renderer's recording types beside their
//! own recording. It exists in test builds only (`cfg(test)`): a shipped
//! build has no log, no field and no branch for it.
//!
//! `placed` applies the saves, restores and transforms, so a test asks for
//! a box in device pixels and not for raw ops.

use super::vocab::{
  BlendMode, ClipOperation, Color, DrawStyle, FillType, Matrix, Point, Rect, RoundingRadii, StrokeCap, StrokeJoin,
  TextureSampling,
};
use std::sync::Arc;

/// The miter limit a paint starts at, the contract's default for `Paint`.
const DEFAULT_STROKE_MITER: f32 = 4.0;
/// How far apart the two axis scales of a matrix may be and still count as
/// one scale: float noise from composed transforms, not a real non-uniform
/// scale.
const UNIFORM_SCALE_EPSILON: f32 = 1e-5;

/// The fields set on a paint. A field never set reads as the contract's
/// default; a source or a filter reads as whether one was set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintLog {
  pub color: Color,
  pub draw_style: DrawStyle,
  pub blend_mode: BlendMode,
  pub stroke_width: f32,
  pub stroke_cap: StrokeCap,
  pub stroke_join: StrokeJoin,
  pub stroke_miter: f32,
  /// `set_color_source` was called.
  pub color_source: bool,
  pub color_filter: bool,
  pub image_filter: bool,
  pub mask_filter: bool,
}

impl Default for PaintLog {
  /// The default the contract states for `Paint`: a solid opaque black
  /// fill, source-over, width 1, butt caps, miter joins at limit 4, no
  /// source and no filters.
  fn default() -> Self {
    PaintLog {
      color: Color::new_srgba(0.0, 0.0, 0.0, 1.0),
      draw_style: DrawStyle::Fill,
      blend_mode: BlendMode::SourceOver,
      stroke_width: super::contract::DEFAULT_STROKE_WIDTH,
      stroke_cap: StrokeCap::Butt,
      stroke_join: StrokeJoin::Miter,
      stroke_miter: DEFAULT_STROKE_MITER,
      color_source: false,
      color_filter: false,
      image_filter: false,
      mask_filter: false,
    }
  }
}

/// One segment of a path, as the pen walk drew it.
#[derive(Debug, PartialEq)]
pub enum PathSegment {
  MoveTo(Point),
  LineTo(Point),
  QuadraticTo { ctrl: Point, to: Point },
  CubicTo { ctrl1: Point, ctrl2: Point, to: Point },
  Close,
}

/// A finished path: its segments in order and the fill type it was built
/// with.
#[derive(Debug, PartialEq)]
pub struct PathLog {
  pub segments: Vec<PathSegment>,
  pub fill: FillType,
}

/// One recording op of the contract's `DisplayListBuilder`, with its
/// arguments.
#[derive(Debug, PartialEq)]
pub enum Op {
  Save,
  SaveLayer {
    bounds: Rect,
    paint: Option<PaintLog>,
  },
  SaveBackdropLayer {
    bounds: Rect,
    paint: Option<PaintLog>,
    /// Whether a filter was passed.
    filter: bool,
  },
  Restore,
  Translate {
    x: f32,
    y: f32,
  },
  Scale {
    x: f32,
    y: f32,
  },
  Transform(Matrix),
  ClipRect {
    rect: Rect,
    op: ClipOperation,
  },
  ClipOval {
    bounds: Rect,
    op: ClipOperation,
  },
  ClipRoundedRect {
    rect: Rect,
    radii: RoundingRadii,
    op: ClipOperation,
  },
  DrawLine {
    from: Point,
    to: Point,
    paint: PaintLog,
  },
  DrawRect {
    rect: Rect,
    paint: PaintLog,
  },
  DrawOval {
    bounds: Rect,
    paint: PaintLog,
  },
  DrawRoundedRect {
    rect: Rect,
    radii: RoundingRadii,
    paint: PaintLog,
  },
  DrawPath {
    path: Arc<PathLog>,
    paint: PaintLog,
  },
  /// No texture handle: the log stays plain data.
  DrawTextureRect {
    src: Rect,
    dst: Rect,
    sampling: TextureSampling,
    paint: Option<PaintLog>,
  },
  /// A nested list, opened by reading `ops`.
  DrawDisplayList {
    ops: Arc<[Op]>,
    opacity: f32,
  },
}

impl Op {
  /// The paint of a draw or a layer; None for the rest, and for a layer or
  /// a texture draw recorded without one.
  pub fn paint(&self) -> Option<&PaintLog> {
    match self {
      Op::SaveLayer { paint, .. } | Op::SaveBackdropLayer { paint, .. } | Op::DrawTextureRect { paint, .. } => {
        paint.as_ref()
      }
      Op::DrawLine { paint, .. }
      | Op::DrawRect { paint, .. }
      | Op::DrawOval { paint, .. }
      | Op::DrawRoundedRect { paint, .. }
      | Op::DrawPath { paint, .. } => Some(paint),
      Op::Save
      | Op::Restore
      | Op::Translate { .. }
      | Op::Scale { .. }
      | Op::Transform(_)
      | Op::ClipRect { .. }
      | Op::ClipOval { .. }
      | Op::ClipRoundedRect { .. }
      | Op::DrawDisplayList { .. } => None,
    }
  }
}

/// One clip, draw or layer op with the transform in effect where it was
/// recorded.
#[derive(Debug)]
pub struct Placed<'a> {
  pub op: &'a Op,
  /// Maps the op's coordinates to the root list's space.
  pub to_device: Matrix,
}

/// The clip, draw and layer ops of `ops` in order, each with the transform
/// in effect where it was recorded: the saves, restores and transforms
/// applied, a nested list yielded as itself and then walked under the
/// transform at the nest point. `to_device` maps an op's coordinates to the
/// root list's space, which is device pixels for a list recorded under the
/// display scale, the way `rendertree::composite::render` records one
/// (`builder.scale(scale, scale)` before `paint_phase`).
pub fn placed(ops: &[Op]) -> Vec<Placed<'_>> {
  let mut out = Vec::new();
  walk(ops, Matrix::identity(), &mut out);
  out
}

// One list under `base`, with a save stack of its own: what it does to the
// transform ends with it. A restore with nothing saved does nothing, as a
// renderer treats it. A transform composes onto the current one applied
// before it (vocab::Matrix, DisplayListBuilder::transform).
fn walk<'a>(ops: &'a [Op], base: Matrix, out: &mut Vec<Placed<'a>>) {
  let mut current = base;
  let mut saved = Vec::new();
  for op in ops {
    match op {
      Op::Save => saved.push(current),
      Op::SaveLayer { .. } | Op::SaveBackdropLayer { .. } => {
        saved.push(current);
        out.push(Placed { op, to_device: current });
      }
      Op::Restore => {
        if let Some(m) = saved.pop() {
          current = m;
        }
      }
      Op::Translate { x, y } => current = Matrix::translation(*x, *y, 0.0).then(&current),
      Op::Scale { x, y } => current = Matrix::scale(*x, *y, 1.0).then(&current),
      Op::Transform(m) => current = m.then(&current),
      Op::DrawDisplayList { ops: nested, .. } => {
        out.push(Placed { op, to_device: current });
        walk(nested, current, out);
      }
      Op::ClipRect { .. }
      | Op::ClipOval { .. }
      | Op::ClipRoundedRect { .. }
      | Op::DrawLine { .. }
      | Op::DrawRect { .. }
      | Op::DrawOval { .. }
      | Op::DrawRoundedRect { .. }
      | Op::DrawPath { .. }
      | Op::DrawTextureRect { .. } => out.push(Placed { op, to_device: current }),
    }
  }
}

// `m` as a 2D map with no rotation and no skew; None for any other matrix.
fn axis_aligned(m: &Matrix) -> Option<euclid::default::Transform2D<f32>> {
  (m.is_2d() && m.m12 == 0.0 && m.m21 == 0.0).then(|| m.to_2d())
}

impl Placed<'_> {
  /// The op's box in the root list's space: a clip's rect or bounds, a
  /// rect, oval or rounded rect draw's, a texture draw's `dst`, a layer's
  /// bounds, when `to_device` is a 2D map with no rotation or skew. None
  /// for a line, a path and a nested list, and under any other matrix.
  pub fn device_rect(&self) -> Option<Rect> {
    let rect = match self.op {
      Op::SaveLayer { bounds, .. }
      | Op::SaveBackdropLayer { bounds, .. }
      | Op::ClipOval { bounds, .. }
      | Op::DrawOval { bounds, .. } => bounds,
      Op::ClipRect { rect, .. }
      | Op::ClipRoundedRect { rect, .. }
      | Op::DrawRect { rect, .. }
      | Op::DrawRoundedRect { rect, .. } => rect,
      Op::DrawTextureRect { dst, .. } => dst,
      Op::Save
      | Op::Restore
      | Op::Translate { .. }
      | Op::Scale { .. }
      | Op::Transform(_)
      | Op::DrawLine { .. }
      | Op::DrawPath { .. }
      | Op::DrawDisplayList { .. } => return None,
    };
    axis_aligned(&self.to_device).map(|m| m.outer_transformed_rect(rect))
  }

  /// The width the op strokes at in the root list's space: its paint's
  /// `stroke_width` times the scale of `to_device`, when it is axis-aligned
  /// with one scale on both axes. Only for a draw that strokes: a line
  /// always, a rect, oval, rounded rect or path when its draw style is not
  /// `Fill`. None for a filled draw, a layer, a texture draw and the ops
  /// without a paint, and under any other matrix.
  pub fn device_stroke_width(&self) -> Option<f32> {
    let paint = match self.op {
      Op::DrawLine { paint, .. } => paint,
      Op::DrawRect { paint, .. }
      | Op::DrawOval { paint, .. }
      | Op::DrawRoundedRect { paint, .. }
      | Op::DrawPath { paint, .. } => Some(paint).filter(|p| p.draw_style != DrawStyle::Fill)?,
      Op::Save
      | Op::SaveLayer { .. }
      | Op::SaveBackdropLayer { .. }
      | Op::Restore
      | Op::Translate { .. }
      | Op::Scale { .. }
      | Op::Transform(_)
      | Op::ClipRect { .. }
      | Op::ClipOval { .. }
      | Op::ClipRoundedRect { .. }
      | Op::DrawTextureRect { .. }
      | Op::DrawDisplayList { .. } => return None,
    };
    let m = axis_aligned(&self.to_device)?;
    let (sx, sy) = (m.m11.abs(), m.m22.abs());
    ((sx - sy).abs() <= UNIFORM_SCALE_EPSILON).then_some(paint.stroke_width * sx)
  }
}
