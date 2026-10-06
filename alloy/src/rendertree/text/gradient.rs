// A run's gradient paint resolved for the glyph pass: the layer's pixels
// mapped to the gradient's parameter, and its stops sampled into a ramp
// (see gpu::GlyphGradient). What Impeller's color source did per paragraph
// draw, the pass does per fragment (gl/glyphs.rs), so gradient text draws
// as a gradient and not in a solid stand-in color. Coordinates: a box
// gradient resolves against the text's own box (its wrap width by its
// lines' height, at the text origin); an absolute one's coordinates are
// text-local pixels from that origin, its transform mapping gradient space
// into them as it does for every other element.
use crate::gpu::{GlyphGradient, GradientKind, GradientTile, RAMP_WIDTH};
use crate::impellers::{Matrix, Point, Size, TileMode};
use crate::rendertree::{Gradient, GradientStop, GradientUnits};

// A 2D affine map as two rows over (x, y, 1).
type Affine = [[f32; 3]; 2];

const IDENTITY: Affine = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];

// `a` after `b`.
fn compose(a: Affine, b: Affine) -> Affine {
  let row = |r: [f32; 3]| {
    [r[0] * b[0][0] + r[1] * b[1][0], r[0] * b[0][1] + r[1] * b[1][1], r[0] * b[0][2] + r[1] * b[1][2] + r[2]]
  };
  [row(a[0]), row(a[1])]
}

fn translate(x: f32, y: f32) -> Affine {
  [[1.0, 0.0, x], [0.0, 1.0, y]]
}

fn scale_by(s: f32) -> Affine {
  [[s, 0.0, 0.0], [0.0, s, 0.0]]
}

// The map from the unit square onto a box of `size` at the origin.
fn box_map(size: Size) -> Affine {
  [[size.width, 0.0, 0.0], [0.0, size.height, 0.0]]
}

// The inverse of an affine map; None when it collapses a dimension.
fn invert(a: Affine) -> Option<Affine> {
  let det = a[0][0] * a[1][1] - a[0][1] * a[1][0];
  if det == 0.0 || !det.is_finite() {
    return None;
  }
  let (ia, ib, ic, id) = (a[1][1] / det, -a[0][1] / det, -a[1][0] / det, a[0][0] / det);
  let tx = -(ia * a[0][2] + ib * a[1][2]);
  let ty = -(ic * a[0][2] + id * a[1][2]);
  Some([[ia, ib, tx], [ic, id, ty]])
}

// The 2D part of a paint transform as an affine map (euclid's row-vector
// convention: x' = x * m11 + y * m21 + m41).
fn affine_of(m: &Matrix) -> Affine {
  [[m.m11, m.m21, m.m41], [m.m12, m.m22, m.m42]]
}

fn tile_of(tile: TileMode) -> GradientTile {
  match tile {
    TileMode::Clamp => GradientTile::Clamp,
    TileMode::Repeat => GradientTile::Repeat,
    TileMode::Mirror => GradientTile::Mirror,
    TileMode::Decal => GradientTile::Decal,
  }
}

// The stops sampled across the ramp: the first color before the first
// stop, the last after the last, straight-alpha sRGB in between (what the
// color source interpolated). None without stops.
fn ramp(stops: &[GradientStop]) -> Option<Vec<[u8; 4]>> {
  let first = stops.first()?;
  let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
  let texel = |r: f32, g: f32, b: f32, a: f32| [to_u8(r), to_u8(g), to_u8(b), to_u8(a)];
  let mut out = Vec::with_capacity(RAMP_WIDTH);
  for i in 0..RAMP_WIDTH {
    let t = i as f32 / (RAMP_WIDTH - 1) as f32;
    let color = match stops.iter().position(|s| s.offset > t) {
      Some(0) => first.color,
      None => stops[stops.len() - 1].color,
      Some(j) => {
        let (a, b) = (&stops[j - 1], &stops[j]);
        let span = b.offset - a.offset;
        let f = if span > 0.0 { ((t - a.offset) / span).clamp(0.0, 1.0) } else { 1.0 };
        let mix = |x: f32, y: f32| x + (y - x) * f;
        crate::impellers::Color::new_srgba(
          mix(a.color.red, b.color.red),
          mix(a.color.green, b.color.green),
          mix(a.color.blue, b.color.blue),
          mix(a.color.alpha, b.color.alpha),
        )
      }
    };
    out.push(texel(color.red, color.green, color.blue, color.alpha));
  }
  Some(out)
}

/// `gradient` as the glyph pass evaluates it over a layer whose pixel
/// (0, 0) is `box_origin` in text-local pixels, `scale` layer pixels per
/// logical pixel, the text's own box being `content` at the text origin.
/// None when the gradient cannot be evaluated (no stops, a degenerate
/// geometry), in which case the run draws in its solid color.
pub(crate) fn layer_gradient(
  gradient: &Gradient,
  content: Size,
  box_origin: Point,
  scale: f32,
) -> Option<GlyphGradient> {
  // Layer pixels to text-local pixels.
  let to_local = [[1.0 / scale, 0.0, box_origin.x], [0.0, 1.0 / scale, box_origin.y]];
  match gradient {
    Gradient::Linear { start, end, stops, tile, transform, units } => {
      let to_space = match units {
        GradientUnits::Absolute => invert(affine_of(transform))?,
        GradientUnits::BoundingBox => invert(box_map(content))?,
      };
      let d = (end.x - start.x, end.y - start.y);
      let len2 = d.0 * d.0 + d.1 * d.1;
      if len2 <= 0.0 || !len2.is_finite() {
        return None;
      }
      // t = ((q - start) . d) / |d|^2 over the gradient-space point q.
      let projection = [[d.0 / len2, d.1 / len2, -(start.x * d.0 + start.y * d.1) / len2], [0.0; 3]];
      let rows = compose(projection, compose(to_space, to_local));
      Some(GlyphGradient {
        kind: GradientKind::Linear,
        x_row: rows[0],
        y_row: [0.0; 3],
        tile: tile_of(*tile),
        ramp: ramp(stops)?,
      })
    }
    Gradient::Radial { center, radius, stops, tile, transform, units, circle } => {
      if *radius <= 0.0 || !radius.is_finite() {
        return None;
      }
      let (to_space, center, radius) = match units {
        GradientUnits::Absolute => (invert(affine_of(transform))?, *center, *radius),
        // A true circle: center and radius in pixels, no stretch.
        GradientUnits::BoundingBox if *circle => (
          IDENTITY,
          Point::new(center.x * content.width, center.y * content.height),
          radius * content.width.min(content.height),
        ),
        // An ellipse: the box map stretches the unit circle.
        GradientUnits::BoundingBox => (invert(box_map(content))?, *center, *radius),
      };
      let rows = compose(scale_by(1.0 / radius), compose(translate(-center.x, -center.y), compose(to_space, to_local)));
      Some(GlyphGradient {
        kind: GradientKind::Radial,
        x_row: rows[0],
        y_row: rows[1],
        tile: tile_of(*tile),
        ramp: ramp(stops)?,
      })
    }
  }
}
