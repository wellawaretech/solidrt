use crate::impellers::{
  BlendMode, Color, ColorSource, DrawStyle, Matrix, Paint, Point, Rect, StrokeCap, StrokeJoin, TileMode,
};
use crate::rendertree::Damage;

#[derive(Clone, Debug)]
pub struct GradientStop {
  pub offset: f32,
  pub color: Color,
}

// Whether a gradient's coordinates are already in the drawing space (SVG resolves
// everything to absolute coordinates) or are box-relative 0..1 fractions resolved
// against the painted element's bounds at paint time (the factory API).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradientUnits {
  Absolute,
  BoundingBox,
}

// A gradient color source described as plain data. The native ColorSource is
// built on demand in `to_paint*` (like Paint itself), so PaintState stays cheap
// to clone and compare. For Absolute units, `transform` maps the parametric
// coordinates into the drawing space; for BoundingBox units it is unused (the
// box transform is derived from the element bounds at paint time).
#[derive(Clone, Debug)]
pub enum Gradient {
  Linear {
    start: Point,
    end: Point,
    stops: Vec<GradientStop>,
    tile: TileMode,
    transform: Matrix,
    units: GradientUnits,
  },
  Radial {
    center: Point,
    radius: f32,
    stops: Vec<GradientStop>,
    tile: TileMode,
    transform: Matrix,
    units: GradientUnits,
    // For BoundingBox radials: true keeps a true circle (radius a fraction of the
    // shorter side); false lets the non-uniform box transform stretch it into an
    // ellipse. Ignored for Absolute units.
    circle: bool,
  },
}

impl Gradient {
  // Box-relative linear gradient (factory API): endpoints in 0..1 of the element
  // box, clamped at the ends like CSS.
  pub fn linear_box(start: Point, end: Point, stops: Vec<GradientStop>) -> Self {
    Gradient::Linear {
      start,
      end,
      stops,
      tile: TileMode::Clamp,
      transform: Matrix::identity(),
      units: GradientUnits::BoundingBox,
    }
  }

  // Box-relative radial gradient (factory API): center in 0..1, radius in 0..1.
  pub fn radial_box(center: Point, radius: f32, circle: bool, stops: Vec<GradientStop>) -> Self {
    Gradient::Radial {
      center,
      radius,
      stops,
      tile: TileMode::Clamp,
      transform: Matrix::identity(),
      units: GradientUnits::BoundingBox,
      circle,
    }
  }

  // Builds the native color source, resolving box-relative coordinates against
  // `bounds` (x, y, w, h) when given. Returns None for a box-relative gradient
  // with no bounds, so the caller falls back to the solid color.
  fn to_color_source(&self, bounds: Option<(f32, f32, f32, f32)>) -> Option<ColorSource> {
    match self {
      Gradient::Linear { start, end, stops, tile, transform, units } => {
        let (colors, offsets) = split_stops(stops);
        let xform = match units {
          GradientUnits::Absolute => *transform,
          GradientUnits::BoundingBox => box_matrix(bounds?),
        };
        Some(ColorSource::new_linear_gradient(*start, *end, &colors, &offsets, *tile, Some(&xform)))
      }
      Gradient::Radial { center, radius, stops, tile, transform, units, circle } => {
        let (colors, offsets) = split_stops(stops);
        match units {
          GradientUnits::Absolute => {
            Some(ColorSource::new_radial_gradient(*center, *radius, &colors, &offsets, *tile, Some(transform)))
          }
          GradientUnits::BoundingBox if *circle => {
            // True circle: resolve center and radius to pixels; no stretch.
            let (x, y, w, h) = bounds?;
            let c = Point::new(x + center.x * w, y + center.y * h);
            let r = radius * w.min(h);
            Some(ColorSource::new_radial_gradient(c, r, &colors, &offsets, *tile, None))
          }
          GradientUnits::BoundingBox => {
            // Ellipse: keep 0..1 coords and let the box transform stretch the circle.
            let xform = box_matrix(bounds?);
            Some(ColorSource::new_radial_gradient(*center, *radius, &colors, &offsets, *tile, Some(&xform)))
          }
        }
      }
    }
  }
}

// Maps the unit square (0..1) onto the box (x, y, w, h).
fn box_matrix((x, y, w, h): (f32, f32, f32, f32)) -> Matrix {
  Matrix::new_2d(w, 0.0, 0.0, h, x, y)
}

fn split_stops(stops: &[GradientStop]) -> (Vec<Color>, Vec<f32>) {
  (stops.iter().map(|s| s.color).collect(), stops.iter().map(|s| s.offset).collect())
}

// Average of the stop colors, used as the solid fallback for hit-testing and for
// when a box-relative gradient is painted without bounds.
fn average_stops(stops: &[GradientStop]) -> Color {
  if stops.is_empty() {
    return Color::new_srgba(0.5, 0.5, 0.5, 1.0);
  }
  let (mut r, mut g, mut b, mut a) = (0.0, 0.0, 0.0, 0.0);
  for s in stops {
    r += s.color.red;
    g += s.color.green;
    b += s.color.blue;
    a += s.color.alpha;
  }
  let n = stops.len() as f32;
  Color::new_srgba(r / n, g / n, b / n, a / n)
}

#[derive(Clone, Debug)]
pub struct PaintState {
  pub color: Color,
  // When set, painted as a gradient color source; `color` remains the solid
  // fallback (hit-testing, and any region the source does not cover).
  pub gradient: Option<Gradient>,
  pub draw_style: DrawStyle,
  pub blend_mode: BlendMode,
  pub stroke_width: f32,
  pub stroke_cap: StrokeCap,
  pub stroke_join: StrokeJoin,
  pub stroke_miter: f32,
}

impl Default for PaintState {
  fn default() -> Self {
    Self {
      color: Color::new_srgba(0.5, 0.5, 0.5, 1.0),
      gradient: None,
      draw_style: DrawStyle::Fill,
      blend_mode: BlendMode::SourceOver,
      stroke_width: 0.0,
      stroke_cap: StrokeCap::Butt,
      stroke_join: StrokeJoin::Miter,
      stroke_miter: 4.0,
    }
  }
}

// Manual impl because impellers::Color has no PartialEq. Used as part of the
// shaped-paragraph cache key in text.rs.
impl PartialEq for PaintState {
  fn eq(&self, other: &Self) -> bool {
    color_eq(self.color, other.color)
      && gradient_eq(&self.gradient, &other.gradient)
      && self.draw_style == other.draw_style
      && self.blend_mode == other.blend_mode
      && self.stroke_width == other.stroke_width
      && self.stroke_cap == other.stroke_cap
      && self.stroke_join == other.stroke_join
      && self.stroke_miter == other.stroke_miter
  }
}

fn color_eq(a: Color, b: Color) -> bool {
  a.red == b.red && a.green == b.green && a.blue == b.blue && a.alpha == b.alpha && a.color_space == b.color_space
}

fn stops_eq(a: &[GradientStop], b: &[GradientStop]) -> bool {
  a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.offset == y.offset && color_eq(x.color, y.color))
}

fn gradient_eq(a: &Option<Gradient>, b: &Option<Gradient>) -> bool {
  match (a, b) {
    (None, None) => true,
    (
      Some(Gradient::Linear { start: s1, end: e1, stops: st1, tile: t1, transform: m1, units: u1 }),
      Some(Gradient::Linear { start: s2, end: e2, stops: st2, tile: t2, transform: m2, units: u2 }),
    ) => s1 == s2 && e1 == e2 && t1 == t2 && m1 == m2 && u1 == u2 && stops_eq(st1, st2),
    (
      Some(Gradient::Radial { center: c1, radius: r1, stops: st1, tile: t1, transform: m1, units: u1, circle: ci1 }),
      Some(Gradient::Radial { center: c2, radius: r2, stops: st2, tile: t2, transform: m2, units: u2, circle: ci2 }),
    ) => c1 == c2 && r1 == r2 && t1 == t2 && m1 == m2 && u1 == u2 && ci1 == ci2 && stops_eq(st1, st2),
    _ => false,
  }
}

impl PaintState {
  // For paints with no box-relative gradient (solids and SVG's absolute
  // gradients). A box-relative gradient reaching here has no bounds to resolve
  // against, so it is skipped and the solid fallback color shows.
  pub fn to_paint(&self) -> Paint {
    self.build_paint(None)
  }

  // For elements that fill a known box; resolves a box-relative gradient against
  // `bounds` in the element's own paint space.
  pub fn to_paint_in(&self, bounds: &Rect) -> Paint {
    self.build_paint(Some((bounds.origin.x, bounds.origin.y, bounds.size.width, bounds.size.height)))
  }

  // Half the stroke width for the stroked draw styles, 0 for a plain fill.
  // Rect and oval inset their geometry by this so a stroke paints inside its
  // bounds (CSS border semantics) instead of straddling them; see
  // `Rectangle::build`. Clamped to half the shorter side so a stroke wider
  // than the shape collapses onto the shape's center rather than inverting it.
  pub fn stroke_inset(&self, w: f32, h: f32) -> f32 {
    match self.draw_style {
      DrawStyle::Fill => 0.0,
      DrawStyle::Stroke | DrawStyle::StrokeAndFill => {
        let limit = (w / 2.0).min(h / 2.0).max(0.0);
        (self.stroke_width / 2.0).clamp(0.0, limit)
      }
    }
  }

  fn build_paint(&self, bounds: Option<(f32, f32, f32, f32)>) -> Paint {
    let mut paint = Paint::default();
    paint.set_color(self.color);
    if let Some(gradient) = &self.gradient {
      if let Some(source) = gradient.to_color_source(bounds) {
        paint.set_color_source(&source);
      }
    }
    paint.set_draw_style(self.draw_style);
    paint.set_blend_mode(self.blend_mode);
    paint.set_stroke_width(self.stroke_width);
    paint.set_stroke_cap(self.stroke_cap);
    paint.set_stroke_join(self.stroke_join);
    paint.set_stroke_miter(self.stroke_miter);
    paint
  }

  // Paint never affects layout, so all setters report false. Values arrive
  // already decoded (color unpacked, enums resolved) from the binding layer.

  // A solid color clears any gradient (the two are mutually exclusive fills).
  pub fn set_color(&mut self, color: Color) -> Damage {
    self.color = color;
    self.gradient = None;
    Damage::Paint
  }

  // Sets a gradient fill and derives a solid fallback from its stops (used for
  // hit-testing and when painted without resolvable bounds).
  pub fn set_gradient(&mut self, gradient: Gradient) -> Damage {
    self.color = match &gradient {
      Gradient::Linear { stops, .. } | Gradient::Radial { stops, .. } => average_stops(stops),
    };
    self.gradient = Some(gradient);
    Damage::Paint
  }
  pub fn set_draw_style(&mut self, v: DrawStyle) -> Damage {
    self.draw_style = v;
    Damage::Paint
  }
  pub fn set_blend_mode(&mut self, v: BlendMode) -> Damage {
    self.blend_mode = v;
    Damage::Paint
  }
  pub fn set_stroke_width(&mut self, v: f32) -> Damage {
    self.stroke_width = v;
    Damage::Paint
  }
  pub fn set_stroke_cap(&mut self, v: StrokeCap) -> Damage {
    self.stroke_cap = v;
    Damage::Paint
  }
  pub fn set_stroke_join(&mut self, v: StrokeJoin) -> Damage {
    self.stroke_join = v;
    Damage::Paint
  }
  pub fn set_stroke_miter(&mut self, v: f32) -> Damage {
    self.stroke_miter = v;
    Damage::Paint
  }
}
