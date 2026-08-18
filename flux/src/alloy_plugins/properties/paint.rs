use alloy::impellers::{BlendMode, DrawStyle, Matrix, Point, StrokeCap, StrokeJoin, TileMode};

use super::{decode_color, f32_of, str_of};
use crate::alloy_plugins::value::PropValue;
use alloy::rendertree::Damage;
use alloy::rendertree::{Gradient, GradientStop, GradientUnits, PaintState};

pub fn apply(paint: &mut PaintState, name: &str, value: &PropValue) -> Result<Option<Damage>, String> {
  Ok(Some(match name {
    // `color` carries either a solid (a packed-u32 number) or a gradient created
    // by createLinearGradient/createRadialGradient (a branded object).
    "color" => match value {
      PropValue::Map(_) => paint.set_gradient(decode_gradient(value)?),
      _ => paint.set_color(decode_color(value)?),
    },
    "strokeWidth" => paint.set_stroke_width(f32_of(value, "strokeWidth")?),
    "strokeMiter" => paint.set_stroke_miter(f32_of(value, "strokeMiter")?),
    "drawStyle" => paint.set_draw_style(match str_of(value, "drawStyle")? {
      "fill" => DrawStyle::Fill,
      "stroke" => DrawStyle::Stroke,
      "stroke-and-fill" => DrawStyle::StrokeAndFill,
      v => return Err(format!("Unknown drawStyle \"{v}\"; expected fill, stroke or stroke-and-fill")),
    }),
    "strokeCap" => paint.set_stroke_cap(match str_of(value, "strokeCap")? {
      "butt" => StrokeCap::Butt,
      "round" => StrokeCap::Round,
      "square" => StrokeCap::Square,
      v => return Err(format!("Unknown strokeCap \"{v}\"; expected butt, round or square")),
    }),
    "strokeJoin" => paint.set_stroke_join(match str_of(value, "strokeJoin")? {
      "miter" => StrokeJoin::Miter,
      "round" => StrokeJoin::Round,
      "bevel" => StrokeJoin::Bevel,
      v => return Err(format!("Unknown strokeJoin \"{v}\"; expected miter, round or bevel")),
    }),
    "blendMode" => paint.set_blend_mode(decode_blend_mode(str_of(value, "blendMode")?)?),
    _ => return Ok(None),
  }))
}

// The branded gradient object, decoded by key (stop colors are packed u32):
//   linear: { __gradient: "linear", x0, y0, x1, y1, stops: [{offset, color}, ...] }
//   radial: { __gradient: "radial", cx, cy, r, circle, stops: [...] }
// The core factories emit box-relative gradients (coordinates 0..1 of the
// element box). parseSvg draws add `units: "absolute"` (document-space
// coordinates) plus optional `spread` and `transform` (SVG matrix sextet).
fn decode_gradient(value: &PropValue) -> Result<Gradient, String> {
  let absolute = value.get("units").and_then(PropValue::as_str) == Some("absolute");
  match value.get("__gradient").and_then(PropValue::as_str) {
    Some("linear") => {
      let start = Point::new(field_f32(value, "x0")?, field_f32(value, "y0")?);
      let end = Point::new(field_f32(value, "x1")?, field_f32(value, "y1")?);
      if absolute {
        Ok(Gradient::Linear {
          start,
          end,
          stops: decode_stops(value)?,
          tile: decode_spread(value)?,
          transform: decode_transform(value)?,
          units: GradientUnits::Absolute,
        })
      } else {
        Ok(Gradient::linear_box(start, end, decode_stops(value)?))
      }
    }
    Some("radial") => {
      let center = Point::new(field_f32(value, "cx")?, field_f32(value, "cy")?);
      let radius = field_f32(value, "r")?;
      if absolute {
        Ok(Gradient::Radial {
          center,
          radius,
          stops: decode_stops(value)?,
          tile: decode_spread(value)?,
          transform: decode_transform(value)?,
          units: GradientUnits::Absolute,
          circle: false,
        })
      } else {
        let circle = value.get("circle").and_then(PropValue::as_bool).unwrap_or(false);
        Ok(Gradient::radial_box(center, radius, circle, decode_stops(value)?))
      }
    }
    Some(other) => Err(format!("Unknown gradient kind \"{other}\"; expected linear or radial")),
    None => Err("Color object is not a gradient (missing __gradient); create one with createLinearGradient/createRadialGradient".to_string()),
  }
}

// SVG spread vocabulary; absent means pad (clamp), like the factories.
fn decode_spread(gradient: &PropValue) -> Result<TileMode, String> {
  match gradient.get("spread").and_then(PropValue::as_str) {
    None | Some("pad") => Ok(TileMode::Clamp),
    Some("reflect") => Ok(TileMode::Mirror),
    Some("repeat") => Ok(TileMode::Repeat),
    Some(v) => Err(format!("Unknown gradient spread \"{v}\"; expected pad, reflect or repeat")),
  }
}

// An SVG matrix(a b c d e f) sextet; absent means identity. new_2d takes the
// same column order: (a,b) (c,d) (e,f).
fn decode_transform(gradient: &PropValue) -> Result<Matrix, String> {
  let Some(t) = gradient.get("transform") else { return Ok(Matrix::identity()) };
  let list = t.as_list().ok_or_else(|| "Gradient transform must be a list".to_string())?;
  if list.len() != 6 {
    return Err(format!("Gradient transform must have 6 entries, got {}", list.len()));
  }
  let n = |i: usize| f32_of(&list[i], "gradient transform");
  Ok(Matrix::new_2d(n(0)?, n(1)?, n(2)?, n(3)?, n(4)?, n(5)?))
}

fn decode_stops(gradient: &PropValue) -> Result<Vec<GradientStop>, String> {
  let stops =
    gradient.get("stops").and_then(PropValue::as_list).ok_or_else(|| "Gradient must have a stops list".to_string())?;
  stops
    .iter()
    .map(|stop| {
      Ok(GradientStop {
        offset: field_f32(stop, "offset")?,
        color: decode_color(stop.get("color").ok_or_else(|| "Gradient stop must have a color".to_string())?)?,
      })
    })
    .collect()
}

fn field_f32(map: &PropValue, key: &str) -> Result<f32, String> {
  f32_of(map.get(key).ok_or_else(|| format!("Gradient missing '{key}'"))?, key)
}

fn decode_blend_mode(s: &str) -> Result<BlendMode, String> {
  Ok(match s {
    "clear" => BlendMode::Clear,
    "source" => BlendMode::Source,
    "destination" => BlendMode::Destination,
    "source-over" => BlendMode::SourceOver,
    "destination-over" => BlendMode::DestinationOver,
    "source-in" => BlendMode::SourceIn,
    "destination-in" => BlendMode::DestinationIn,
    "source-out" => BlendMode::SourceOut,
    "destination-out" => BlendMode::DestinationOut,
    "source-atop" => BlendMode::SourceATop,
    "destination-atop" => BlendMode::DestinationATop,
    "xor" => BlendMode::Xor,
    "plus" => BlendMode::Plus,
    "modulate" => BlendMode::Modulate,
    "screen" => BlendMode::Screen,
    "overlay" => BlendMode::Overlay,
    "darken" => BlendMode::Darken,
    "lighten" => BlendMode::Lighten,
    "color-dodge" => BlendMode::ColorDodge,
    "color-burn" => BlendMode::ColorBurn,
    "hard-light" => BlendMode::HardLight,
    "soft-light" => BlendMode::SoftLight,
    "difference" => BlendMode::Difference,
    "exclusion" => BlendMode::Exclusion,
    "multiply" => BlendMode::Multiply,
    "hue" => BlendMode::Hue,
    "saturation" => BlendMode::Saturation,
    "color" => BlendMode::Color,
    "luminosity" => BlendMode::Luminosity,
    v => return Err(format!("Unknown blendMode \"{v}\"")),
  })
}
