// mod audio;
// mod oval;
// mod path;
mod rectangle;
mod span;
mod text;
// pub mod texture;
mod view;
mod window;

// pub use audio::AudioNode;
// pub use oval::OvalNode;
// pub use path::PathNode;
pub use rectangle::Rectangle;
pub use span::Span;
pub use text::Text;
pub use view::View;
pub use window::Window;

use alloy::impellers::{BlendMode, Color, DrawStyle, Paint, StrokeCap, StrokeJoin};

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
      color: Color::new_srgba(1.0, 0.0, 0.0, 1.0),
      draw_style: DrawStyle::Fill,
      blend_mode: BlendMode::SourceOver,
      stroke_width: 0.0,
      stroke_cap: StrokeCap::Butt,
      stroke_join: StrokeJoin::Miter,
      stroke_miter: 4.0,
    }
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
}
