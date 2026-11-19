//! The gradient of any draw: a linear or radial gradient source resolved
//! to two affine rows over the op's space and a ramp of its stop colors,
//! with the small affine toolkit the resolution is written in. A shape's
//! paint resolves here at record time (`renderer::record`), a text run's in
//! alloy's text resolver; the walk puts the rows on a draw's instances over
//! the quad's local space, so every program evaluates a gradient at the
//! local point it shades (a path mesh's, over device pixels, through the
//! inverse of its map).

use crate::{Color, Point, TileMode};

/// Texels across a gradient ramp: the stop colors sampled at this many
/// positions along 0..1, the steps 8-bit output can show.
pub const RAMP_WIDTH: usize = 256;

/// The shape of a gradient; the discriminants are the kinds the programs
/// read from an instance's packed kind and tile (0 is no gradient).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientKind {
  /// The parameter is the mapped x.
  Linear = 1,
  /// The parameter is the mapped point's distance from the origin.
  Radial = 2,
}

/// A gradient source resolved to the space of the op that draws it: how a
/// point of that space maps to the gradient's parameter, and the colors
/// along it.
#[derive(Clone, Debug, PartialEq)]
pub struct RampGradient {
  pub kind: GradientKind,
  /// The op's space (x, y) to the gradient's parameter space as two affine
  /// rows over (x, y, 1): x' from `x_row`, y' from `y_row`. A linear
  /// gradient reads its parameter as x', a radial one as the length of
  /// (x', y').
  pub x_row: [f32; 3],
  pub y_row: [f32; 3],
  /// What the gradient does past its ends.
  pub tile: TileMode,
  /// The stop colors sampled along the parameter: `RAMP_WIDTH` texels of
  /// straight-alpha sRGB rgba8, the ramp texture the programs sample.
  pub ramp: Vec<[u8; 4]>,
}

/// A 2D affine map as two rows over (x, y, 1).
pub type Affine = [[f32; 3]; 2];

/// The affine toolkit the gradients are resolved in.
pub mod affine {
  use super::Affine;
  use crate::Matrix;
  use euclid::default::Transform2D;

  /// The map that moves nothing.
  pub const IDENTITY: Affine = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];

  /// `a` after `b`.
  pub fn compose(a: Affine, b: Affine) -> Affine {
    let row = |r: [f32; 3]| {
      [r[0] * b[0][0] + r[1] * b[1][0], r[0] * b[0][1] + r[1] * b[1][1], r[0] * b[0][2] + r[1] * b[1][2] + r[2]]
    };
    [row(a[0]), row(a[1])]
  }

  /// The translation by (x, y).
  pub fn translate(x: f32, y: f32) -> Affine {
    [[1.0, 0.0, x], [0.0, 1.0, y]]
  }

  /// The uniform scale by `s`.
  pub fn scale_by(s: f32) -> Affine {
    [[s, 0.0, 0.0], [0.0, s, 0.0]]
  }

  /// The inverse of an affine map; None when it collapses a dimension or
  /// its determinant is not finite.
  pub fn invert(a: Affine) -> Option<Affine> {
    let det = a[0][0] * a[1][1] - a[0][1] * a[1][0];
    if det == 0.0 || !det.is_finite() {
      return None;
    }
    let (ia, ib, ic, id) = (a[1][1] / det, -a[0][1] / det, -a[1][0] / det, a[0][0] / det);
    let tx = -(ia * a[0][2] + ib * a[1][2]);
    let ty = -(ic * a[0][2] + id * a[1][2]);
    Some([[ia, ib, tx], [ic, id, ty]])
  }

  /// The 2D part of a paint transform as an affine map (euclid's row-vector
  /// convention: x' = x * m11 + y * m21 + m41).
  pub fn of_matrix(m: &Matrix) -> Affine {
    [[m.m11, m.m21, m.m41], [m.m12, m.m22, m.m42]]
  }

  /// A 2D map as an affine map (euclid's row-vector convention:
  /// x' = x * m11 + y * m21 + m31).
  pub fn of_2d(m: &Transform2D<f32>) -> Affine {
    [[m.m11, m.m21, m.m31], [m.m12, m.m22, m.m32]]
  }
}

impl RampGradient {
  /// The stops sampled across the ramp, `colors[i]` at the offset
  /// `stops[i]` (as many stops as the shorter slice holds): the first color
  /// before the first stop, the last after the last, straight-alpha sRGB in
  /// between (what the color source interpolates). None without stops.
  pub fn ramp(colors: &[Color], stops: &[f32]) -> Option<Vec<[u8; 4]>> {
    let count = colors.len().min(stops.len());
    if count == 0 {
      return None;
    }
    let (colors, stops) = (&colors[..count], &stops[..count]);
    let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let texel = |c: Color| [to_u8(c.red), to_u8(c.green), to_u8(c.blue), to_u8(c.alpha)];
    let mut out = Vec::with_capacity(RAMP_WIDTH);
    for i in 0..RAMP_WIDTH {
      let t = i as f32 / (RAMP_WIDTH - 1) as f32;
      let color = match stops.iter().position(|s| *s > t) {
        Some(0) => colors[0],
        None => colors[count - 1],
        Some(j) => {
          let (a, b) = (colors[j - 1], colors[j]);
          let span = stops[j] - stops[j - 1];
          let f = if span > 0.0 { ((t - stops[j - 1]) / span).clamp(0.0, 1.0) } else { 1.0 };
          let mix = |x: f32, y: f32| x + (y - x) * f;
          Color::new_srgba(mix(a.red, b.red), mix(a.green, b.green), mix(a.blue, b.blue), mix(a.alpha, b.alpha))
        }
      };
      out.push(texel(color));
    }
    Some(out)
  }

  /// A linear gradient from `start` to `end` in the gradient's own space,
  /// `to_space` mapping the op's space into it: the parameter is
  /// t = ((q - start) . d) / |d|^2 over the gradient-space point q, with
  /// d = end - start. None for a zero-length or non-finite axis, or no
  /// stops.
  pub fn linear(
    start: Point,
    end: Point,
    colors: &[Color],
    stops: &[f32],
    tile: TileMode,
    to_space: Affine,
  ) -> Option<RampGradient> {
    let d = (end.x - start.x, end.y - start.y);
    let len2 = d.0 * d.0 + d.1 * d.1;
    if len2 <= 0.0 || !len2.is_finite() {
      return None;
    }
    let projection = [[d.0 / len2, d.1 / len2, -(start.x * d.0 + start.y * d.1) / len2], [0.0; 3]];
    let rows = affine::compose(projection, to_space);
    Some(RampGradient {
      kind: GradientKind::Linear,
      x_row: rows[0],
      y_row: [0.0; 3],
      tile,
      ramp: Self::ramp(colors, stops)?,
    })
  }

  /// A radial gradient around `center` with `radius` in the gradient's own
  /// space, `to_space` mapping the op's space into it: the parameter is
  /// the distance from the center over the radius. None for a non-positive
  /// or non-finite radius, or no stops.
  pub fn radial(
    center: Point,
    radius: f32,
    colors: &[Color],
    stops: &[f32],
    tile: TileMode,
    to_space: Affine,
  ) -> Option<RampGradient> {
    if radius <= 0.0 || !radius.is_finite() {
      return None;
    }
    let rows = affine::compose(
      affine::scale_by(1.0 / radius),
      affine::compose(affine::translate(-center.x, -center.y), to_space),
    );
    Some(RampGradient {
      kind: GradientKind::Radial,
      x_row: rows[0],
      y_row: rows[1],
      tile,
      ramp: Self::ramp(colors, stops)?,
    })
  }
}
