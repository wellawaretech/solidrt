use alloy::impellers::{BlendMode, DrawStyle, Point, StrokeCap, StrokeJoin};

use super::{decode_color, f32_of, str_of};
use crate::plugins::gui::value::PropValue;
use alloy::rendertree::{Gradient, GradientStop, PaintState};

pub fn apply(paint: &mut PaintState, name: &str, value: &PropValue) -> Option<bool> {
  Some(match name {
    // `color` carries either a solid (a packed-u32 number) or a gradient created
    // by createLinearGradient/createRadialGradient (a branded object).
    "color" => match value {
      PropValue::Map(_) => paint.set_gradient(decode_gradient(value)),
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

// The branded gradient object produced by the core factories, decoded by key
// (coordinates are 0..1 of the element box; stop colors are packed u32):
//   linear: { __gradient: "linear", x0, y0, x1, y1, stops: [{offset, color}, ...] }
//   radial: { __gradient: "radial", cx, cy, r, circle, stops: [...] }
fn decode_gradient(value: &PropValue) -> Gradient {
  match value.get("__gradient").and_then(PropValue::as_str) {
    Some("linear") => {
      let start = Point::new(field_f32(value, "x0"), field_f32(value, "y0"));
      let end = Point::new(field_f32(value, "x1"), field_f32(value, "y1"));
      Gradient::linear_box(start, end, decode_stops(value))
    }
    Some("radial") => {
      let center = Point::new(field_f32(value, "cx"), field_f32(value, "cy"));
      let radius = field_f32(value, "r");
      let circle = value.get("circle").and_then(PropValue::as_bool).unwrap_or(false);
      Gradient::radial_box(center, radius, circle, decode_stops(value))
    }
    other => panic!("unknown gradient kind {other:?}"),
  }
}

fn decode_stops(gradient: &PropValue) -> Vec<GradientStop> {
  let stops = gradient.get("stops").and_then(PropValue::as_list).expect("gradient must have a stops list");
  stops
    .iter()
    .map(|stop| GradientStop {
      offset: field_f32(stop, "offset"),
      color: decode_color(stop.get("color").expect("gradient stop must have a color")),
    })
    .collect()
}

fn field_f32(map: &PropValue, key: &str) -> f32 {
  f32_of(map.get(key).unwrap_or_else(|| panic!("gradient missing '{key}'")), key)
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
