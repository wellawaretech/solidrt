use alloy::impellers::{BlendMode, Color, DrawStyle, Paint, StrokeCap, StrokeJoin};
use rquickjs::Value;

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

  pub fn set_property(&mut self, property: &str, value: Value<'_>) -> Option<bool> {
    match property {
      "color" => {
        let rgba = value.get::<f64>().expect("color must be a number") as u32;
        self.color = Color::new_srgba(
          ((rgba >> 24) & 0xFF) as f32 / 255.0,
          ((rgba >> 16) & 0xFF) as f32 / 255.0,
          ((rgba >> 8) & 0xFF) as f32 / 255.0,
          (rgba & 0xFF) as f32 / 255.0,
        );
        Some(false)
      }
      "strokeWidth" => {
        self.stroke_width = value.get::<f64>().expect("strokeWidth must be a number") as f32;
        Some(false)
      }
      "strokeMiter" => {
        self.stroke_miter = value.get::<f64>().expect("strokeMiter must be a number") as f32;
        Some(false)
      }
      "drawStyle" => {
        self.draw_style = match value.get::<String>().expect("drawStyle must be a string").as_str() {
          "fill" => DrawStyle::Fill,
          "stroke" => DrawStyle::Stroke,
          "strokeAndFill" => DrawStyle::StrokeAndFill,
          v => panic!("unknown drawStyle '{v}'"),
        };
        Some(false)
      }
      "strokeCap" => {
        self.stroke_cap = match value.get::<String>().expect("strokeCap must be a string").as_str() {
          "butt" => StrokeCap::Butt,
          "round" => StrokeCap::Round,
          "square" => StrokeCap::Square,
          v => panic!("unknown strokeCap '{v}'"),
        };
        Some(false)
      }
      "strokeJoin" => {
        self.stroke_join = match value.get::<String>().expect("strokeJoin must be a string").as_str() {
          "miter" => StrokeJoin::Miter,
          "round" => StrokeJoin::Round,
          "bevel" => StrokeJoin::Bevel,
          v => panic!("unknown strokeJoin '{v}'"),
        };
        Some(false)
      }
      "blendMode" => {
        self.blend_mode = match value.get::<String>().expect("blendMode must be a string").as_str() {
          "clear" => BlendMode::Clear,
          "source" => BlendMode::Source,
          "destination" => BlendMode::Destination,
          "sourceOver" => BlendMode::SourceOver,
          "destinationOver" => BlendMode::DestinationOver,
          "sourceIn" => BlendMode::SourceIn,
          "destinationIn" => BlendMode::DestinationIn,
          "sourceOut" => BlendMode::SourceOut,
          "destinationOut" => BlendMode::DestinationOut,
          "sourceATop" => BlendMode::SourceATop,
          "destinationATop" => BlendMode::DestinationATop,
          "xor" => BlendMode::Xor,
          "plus" => BlendMode::Plus,
          "modulate" => BlendMode::Modulate,
          "screen" => BlendMode::Screen,
          "overlay" => BlendMode::Overlay,
          "darken" => BlendMode::Darken,
          "lighten" => BlendMode::Lighten,
          "colorDodge" => BlendMode::ColorDodge,
          "colorBurn" => BlendMode::ColorBurn,
          "hardLight" => BlendMode::HardLight,
          "softLight" => BlendMode::SoftLight,
          "difference" => BlendMode::Difference,
          "exclusion" => BlendMode::Exclusion,
          "multiply" => BlendMode::Multiply,
          "hue" => BlendMode::Hue,
          "saturation" => BlendMode::Saturation,
          "color" => BlendMode::Color,
          "luminosity" => BlendMode::Luminosity,
          v => panic!("unknown blendMode '{v}'"),
        };
        Some(false)
      }
      _ => None,
    }
  }
}
