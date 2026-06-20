use crate::impellers::{BlendMode, Color, ColorSource, DrawStyle, Matrix, Paint, Point, StrokeCap, StrokeJoin, TileMode};

#[derive(Clone, Debug)]
pub struct GradientStop {
  pub offset: f32,
  pub color: Color,
}

// A gradient color source described as plain data. The native ColorSource is
// built on demand in `to_paint` (like Paint itself), so PaintState stays cheap
// to clone and compare. `transform` maps the gradient's parametric coordinates
// into the same space the geometry is drawn in.
#[derive(Clone, Debug)]
pub enum Gradient {
  Linear { start: Point, end: Point, stops: Vec<GradientStop>, tile: TileMode, transform: Matrix },
  Radial { center: Point, radius: f32, stops: Vec<GradientStop>, tile: TileMode, transform: Matrix },
}

impl Gradient {
  fn to_color_source(&self) -> ColorSource {
    match self {
      Gradient::Linear { start, end, stops, tile, transform } => {
        let (colors, offsets) = split_stops(stops);
        ColorSource::new_linear_gradient(*start, *end, &colors, &offsets, *tile, Some(transform))
      }
      Gradient::Radial { center, radius, stops, tile, transform } => {
        let (colors, offsets) = split_stops(stops);
        ColorSource::new_radial_gradient(*center, *radius, &colors, &offsets, *tile, Some(transform))
      }
    }
  }
}

fn split_stops(stops: &[GradientStop]) -> (Vec<Color>, Vec<f32>) {
  (stops.iter().map(|s| s.color).collect(), stops.iter().map(|s| s.offset).collect())
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
      Some(Gradient::Linear { start: s1, end: e1, stops: st1, tile: t1, transform: m1 }),
      Some(Gradient::Linear { start: s2, end: e2, stops: st2, tile: t2, transform: m2 }),
    ) => s1 == s2 && e1 == e2 && t1 == t2 && m1 == m2 && stops_eq(st1, st2),
    (
      Some(Gradient::Radial { center: c1, radius: r1, stops: st1, tile: t1, transform: m1 }),
      Some(Gradient::Radial { center: c2, radius: r2, stops: st2, tile: t2, transform: m2 }),
    ) => c1 == c2 && r1 == r2 && t1 == t2 && m1 == m2 && stops_eq(st1, st2),
    _ => false,
  }
}

impl PaintState {
  pub fn to_paint(&self) -> Paint {
    let mut paint = Paint::default();
    paint.set_color(self.color);
    if let Some(gradient) = &self.gradient {
      paint.set_color_source(&gradient.to_color_source());
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
  pub fn set_color(&mut self, color: Color) -> bool {
    self.color = color;
    false
  }
  pub fn set_draw_style(&mut self, v: DrawStyle) -> bool {
    self.draw_style = v;
    false
  }
  pub fn set_blend_mode(&mut self, v: BlendMode) -> bool {
    self.blend_mode = v;
    false
  }
  pub fn set_stroke_width(&mut self, v: f32) -> bool {
    self.stroke_width = v;
    false
  }
  pub fn set_stroke_cap(&mut self, v: StrokeCap) -> bool {
    self.stroke_cap = v;
    false
  }
  pub fn set_stroke_join(&mut self, v: StrokeJoin) -> bool {
    self.stroke_join = v;
    false
  }
  pub fn set_stroke_miter(&mut self, v: f32) -> bool {
    self.stroke_miter = v;
    false
  }
}
