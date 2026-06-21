use alloy::impellers::{BlendMode, DrawStyle, Point, StrokeCap, StrokeJoin};

use super::{decode_color, f32_of, str_of};
use crate::plugins::gui::value::PropValue;
use alloy::rendertree::{Gradient, GradientStop, PaintState};

pub fn apply(paint: &mut PaintState, name: &str, value: &PropValue) -> Option<bool> {
  Some(match name {
    // `color` carries either a solid (a packed-u32 number) or a gradient created
    // by createLinearGradient/createRadialGradient (encoded as a list).
    "color" => match value {
      PropValue::List(_) => paint.set_gradient(decode_gradient(value)),
      _ => paint.set_color(decode_color(value)),
    },
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

// Gradient encoding produced by the core factories (coordinates are 0..1 of the
// element box; colors are packed u32):
//   linear: ["linear", x0, y0, x1, y1, [off0, col0, off1, col1, ...]]
//   radial: ["radial", cx, cy, r, circleFlag, [off0, col0, ...]]
fn decode_gradient(value: &PropValue) -> Gradient {
  let list = value.as_list().expect("gradient must be a list");
  match list.first().and_then(PropValue::as_str) {
    Some("linear") => {
      let start = Point::new(f32_of(&list[1], "x0"), f32_of(&list[2], "y0"));
      let end = Point::new(f32_of(&list[3], "x1"), f32_of(&list[4], "y1"));
      Gradient::linear_box(start, end, decode_stops(&list[5]))
    }
    Some("radial") => {
      let center = Point::new(f32_of(&list[1], "cx"), f32_of(&list[2], "cy"));
      let radius = f32_of(&list[3], "r");
      let circle = f32_of(&list[4], "circle") != 0.0;
      Gradient::radial_box(center, radius, circle, decode_stops(&list[5]))
    }
    other => panic!("unknown gradient kind {other:?}"),
  }
}

fn decode_stops(value: &PropValue) -> Vec<GradientStop> {
  let flat = value.as_list().expect("gradient stops must be a list");
  flat
    .chunks(2)
    .map(|pair| GradientStop { offset: f32_of(&pair[0], "stop offset"), color: decode_color(&pair[1]) })
    .collect()
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
