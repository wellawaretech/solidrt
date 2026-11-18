//! What a list executes: anvil's own ops, resolved at record time (colors
//! premultiplied, a paint reduced to what stage 1 draws), shared behind an
//! Arc so a retained list is recorded once and replayed by reference.

use super::super::{Color, Matrix, Point, Rect, StrokeCap, TextureSampling};
use super::record::DisplayList;
use super::Texture;

/// A premultiplied sRGB color: (r*a, g*a, b*a, a). Blending is
/// ONE, ONE_MINUS_SRC_ALPHA in sRGB with no linear step, the convention
/// the plan pins (what Impeller did on an RGBA8 target).
pub(super) type Premultiplied = [f32; 4];

/// `c` with its color channels scaled by its alpha, the form every op
/// carries and the blend expects.
pub(super) fn premultiply(c: Color) -> Premultiplied {
  [c.red * c.alpha, c.green * c.alpha, c.blue * c.alpha, c.alpha]
}

/// One executable op of a list, in recording order.
pub(super) enum Op {
  Save,
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
  /// Intersect only: the difference op is stage 5 and is dropped at
  /// record time with a warning.
  ClipRect(Rect),
  /// `stroke` None fills; Some(width) strokes centered on the edge, with
  /// miter joins (sharp corners).
  DrawRect {
    rect: Rect,
    color: Premultiplied,
    stroke: Option<f32>,
  },
  /// `stroke` None fills; Some(width) strokes centered on the ellipse.
  DrawOval {
    bounds: Rect,
    color: Premultiplied,
    stroke: Option<f32>,
  },
  /// A segment stroked `width` wide, centered on it, ended by `cap` at
  /// both ends. A width of zero or below draws nothing (the walk skips it).
  DrawLine {
    from: Point,
    to: Point,
    color: Premultiplied,
    width: f32,
    cap: StrokeCap,
  },
  /// `alpha` is the paint's alpha (1 without a paint): a texture's opacity
  /// is per op, not a group.
  DrawTexture {
    texture: Texture,
    src: Rect,
    dst: Rect,
    sampling: TextureSampling,
    alpha: f32,
  },
  /// The nested list by reference, faded as a group by `opacity`.
  DrawList {
    list: DisplayList,
    opacity: f32,
  },
}
