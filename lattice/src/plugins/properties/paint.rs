use alloy::impellers::{BlendMode, Color, DrawStyle, StrokeCap, StrokeJoin};

use super::{f32_of, str_of};
use crate::plugins::value::PropValue;
use alloy::rendertree::PaintState;

pub fn apply(paint: &mut PaintState, name: &str, value: &PropValue) -> Option<bool> {
  Some(match name {
    "color" => paint.set_color(decode_color(value)),
    "strokeWidth" => paint.set_stroke_width(f32_of(value, "strokeWidth")),
    "strokeMiter" => paint.set_stroke_miter(f32_of(value, "strokeMiter")),
    "drawStyle" => paint.set_draw_style(match str_of(value, "drawStyle") {
      "fill" => DrawStyle::Fill,
      "stroke" => DrawStyle::Stroke,
      "strokeAndFill" => DrawStyle::StrokeAndFill,
      v => panic!("unknown drawStyle '{v}'"),
    }),
    "strokeCap" => paint.set_stroke_cap(match str_of(value, "strokeCap") {
      "butt" => StrokeCap::Butt,
      "round" => StrokeCap::Round,
      "square" => StrokeCap::Square,
      v => panic!("unknown strokeCap '{v}'"),
    }),
    "strokeJoin" => paint.set_stroke_join(match str_of(value, "strokeJoin") {
      "miter" => StrokeJoin::Miter,
      "round" => StrokeJoin::Round,
      "bevel" => StrokeJoin::Bevel,
      v => panic!("unknown strokeJoin '{v}'"),
    }),
    "blendMode" => paint.set_blend_mode(decode_blend_mode(str_of(value, "blendMode"))),
    _ => return None,
  })
}

// JSX sends colors as a packed 0xRRGGBBAA u32 (parsed from a CSS string in JS).
fn decode_color(value: &PropValue) -> Color {
  let rgba = value.as_f64().expect("color must be a number") as u32;
  Color::new_srgba(
    ((rgba >> 24) & 0xFF) as f32 / 255.0,
    ((rgba >> 16) & 0xFF) as f32 / 255.0,
    ((rgba >> 8) & 0xFF) as f32 / 255.0,
    (rgba & 0xFF) as f32 / 255.0,
  )
}

fn decode_blend_mode(s: &str) -> BlendMode {
  match s {
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
  }
}
