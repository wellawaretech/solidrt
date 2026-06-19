use crate::impellers::{BlendMode, Color, DrawStyle, Paint, StrokeCap, StrokeJoin};

#[derive(Clone, Debug)]
pub struct PaintState {
  pub color: Color,
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
    let c = self.color;
    let o = other.color;
    c.red == o.red
      && c.green == o.green
      && c.blue == o.blue
      && c.alpha == o.alpha
      && c.color_space == o.color_space
      && self.draw_style == other.draw_style
      && self.blend_mode == other.blend_mode
      && self.stroke_width == other.stroke_width
      && self.stroke_cap == other.stroke_cap
      && self.stroke_join == other.stroke_join
      && self.stroke_miter == other.stroke_miter
  }
}

impl PaintState {
  pub fn to_paint(&self) -> Paint {
    let mut paint = Paint::default();
    paint.set_color(self.color);
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
